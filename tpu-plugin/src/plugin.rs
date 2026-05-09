use {
    crate::{
        AccountFilter, BatchCommitPolicy, NoFilter, NoLocks, NoTip, NoYield, StandardCommit,
        TipProcessor, TpuStage, WriteLockView, YieldControl,
    },
    std::sync::Arc,
};

/// Stages are aborted in reverse push order at shutdown — push order is the
/// public contract.
#[non_exhaustive]
pub struct TpuPlugin<F = NoFilter>
where
    F: AccountFilter,
{
    pub stages: Vec<Box<dyn TpuStage>>,
    pub banking: BankingHooks<F>,
}

impl<F: AccountFilter> TpuPlugin<F> {
    pub fn new(stages: Vec<Box<dyn TpuStage>>, banking: BankingHooks<F>) -> Self {
        Self { stages, banking }
    }
}

impl Default for TpuPlugin<NoFilter> {
    fn default() -> Self {
        Self::new(Vec::new(), BankingHooks::default())
    }
}

/// Banking hook points wired into Consumer and the scheduler receive paths.
/// Generic over `F` so `TpuPlugin<NoFilter>` monomorphises to zero-cost
/// filter checks on the vanilla path.
#[non_exhaustive]
pub struct BankingHooks<F = NoFilter>
where
    F: AccountFilter,
{
    pub yield_control: Arc<dyn YieldControl>,
    pub account_filter: Arc<F>,
    pub account_lock_view: Arc<dyn WriteLockView>,
    pub tip_processor: Arc<dyn TipProcessor>,
    pub batch_commit: Arc<dyn BatchCommitPolicy>,
}

impl<F: AccountFilter> BankingHooks<F> {
    pub fn new(
        yield_control: Arc<dyn YieldControl>,
        account_filter: Arc<F>,
        account_lock_view: Arc<dyn WriteLockView>,
        tip_processor: Arc<dyn TipProcessor>,
        batch_commit: Arc<dyn BatchCommitPolicy>,
    ) -> Self {
        Self { yield_control, account_filter, account_lock_view, tip_processor, batch_commit }
    }
}

impl Default for BankingHooks<NoFilter> {
    fn default() -> Self {
        Self::new(
            Arc::new(NoYield),
            Arc::new(NoFilter),
            Arc::new(NoLocks),
            Arc::new(NoTip),
            Arc::new(StandardCommit),
        )
    }
}

/// Cloneable bag of hook handles passed between composition layers.
/// Lives here so downstream extension crates never import each other.
#[derive(Clone)]
#[non_exhaustive]
pub struct TpuHandles {
    pub yield_control: Arc<dyn YieldControl>,
    pub account_filter: Arc<dyn AccountFilter>,
    pub account_lock_view: Arc<dyn WriteLockView>,
    pub tip_processor: Arc<dyn TipProcessor>,
    pub batch_commit: Arc<dyn BatchCommitPolicy>,
}

impl TpuHandles {
    pub fn new(
        yield_control: Arc<dyn YieldControl>,
        account_filter: Arc<dyn AccountFilter>,
        account_lock_view: Arc<dyn WriteLockView>,
        tip_processor: Arc<dyn TipProcessor>,
        batch_commit: Arc<dyn BatchCommitPolicy>,
    ) -> Self {
        Self { yield_control, account_filter, account_lock_view, tip_processor, batch_commit }
    }
}
