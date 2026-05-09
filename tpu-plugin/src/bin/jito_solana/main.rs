mod hooks;
mod stages;

use {
    agave_tpu_plugin::{BankingHooks, TpuPlugin},
    hooks::{
        BundleBatchPolicy, BundleFilter, BundleLocks, BundleYield, TipManager,
        tip_account_pubkeys,
    },
    stages::{BlockEngineStage, BundleSigverifyStage, BundleStage},
    std::sync::{Arc, atomic::AtomicBool, mpsc},
};

fn main() {
    let yield_flag = Arc::new(AtomicBool::new(false));
    let locks = Arc::new(BundleLocks::new());

    let (unverified_tx, unverified_rx) = mpsc::channel();
    let (verified_tx, verified_rx) = mpsc::channel();
    let block_engine = BlockEngineStage::spawn(unverified_tx);
    let sigverify = BundleSigverifyStage::spawn(unverified_rx, verified_tx);
    let bundle_stage = BundleStage::spawn(verified_rx, Arc::clone(&locks));

    let plugin = TpuPlugin::new(
        vec![
            Box::new(block_engine),
            Box::new(sigverify),
            Box::new(bundle_stage),
        ],
        BankingHooks::new(
            Arc::new(BundleYield::new(yield_flag)),
            Arc::new(BundleFilter::jito_mainnet()),
            Arc::clone(&locks) as Arc<_>,
            Arc::new(TipManager::new(tip_account_pubkeys())),
            Arc::new(BundleBatchPolicy),
        ),
    );

    for stage in plugin.stages.into_iter().rev() {
        stage.join().expect("stage thread panicked");
    }
}
