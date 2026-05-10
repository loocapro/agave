mod hooks;
mod stages;

use {
    agave_tpu_plugin::{BankingHooks, TipProcessor, TpuPlugin},
    hooks::{
        BlockEngineConfig, BundleBatchPolicy, BundleFilter, BundleLocks,
        BundleYield, TipManager, tip_account_pubkeys,
    },
    stages::{BlockEngineStage, BundleSigverifyStage, BundleStage},
    std::sync::{Arc, atomic::AtomicBool, mpsc},
};

fn main() {
    let exit        = Arc::new(AtomicBool::new(false));
    let yield_flag  = Arc::new(AtomicBool::new(false));
    let locks       = Arc::new(BundleLocks::new());
    let tip_manager = Arc::new(TipManager::new(tip_account_pubkeys()));

    let block_engine_config = BlockEngineConfig {
        block_engine_url: "https://mainnet.block-engine.jito.wtf".to_string(),
        trust_packets: false,
    };

    // Bundle pipeline: block engine → sigverify → bundle stage.
    // Bounded channels mirror real jito-solana for backpressure.
    let (unverified_tx, unverified_rx) = mpsc::sync_channel(1_024);
    let (verified_tx,   verified_rx)   = mpsc::sync_channel(1_024);

    let block_engine = BlockEngineStage::spawn(block_engine_config, unverified_tx, Arc::clone(&exit));
    let sigverify    = BundleSigverifyStage::spawn(unverified_rx, verified_tx, Arc::clone(&exit));
    let bundle_stage = BundleStage::spawn(
        verified_rx,
        Arc::clone(&locks),
        Arc::clone(&tip_manager),
        Arc::clone(&exit),
    );

    // F = BundleFilter resolves here; BankingStage workers receive Arc<BundleFilter> — no vtable.
    let plugin = TpuPlugin::<BundleFilter>::new(
        vec![Box::new(block_engine), Box::new(sigverify), Box::new(bundle_stage)],
        BankingHooks::new(
            Arc::new(BundleYield::new(yield_flag)),
            Arc::new(BundleFilter::jito_mainnet()),
            Arc::clone(&locks) as Arc<_>,
            Arc::clone(&tip_manager) as Arc<dyn TipProcessor>,
            Arc::new(BundleBatchPolicy),
        ),
    );

    // Real jito-solana wiring (Validator not instantiated here — needs genesis, bank, etc.):
    //
    //   Vanilla Agave:
    //     Validator::new_with_exit::<NoFilter>(config, ..., Arc::new(NoFilter))
    //
    //   jito-solana:
    //     Validator::new_with_exit::<BundleFilter>(config, ..., plugin)
    //
    // F resolves at the binary boundary; BankingStage<BundleFilter> never touches a vtable
    // on the per-packet filter path.

    for stage in plugin.stages.into_iter().rev() {
        stage.join().expect("stage thread panicked");
    }
}
