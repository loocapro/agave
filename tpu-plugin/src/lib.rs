//! Extension traits for plugging custom stages into the Agave TPU pipeline.
//! Null implementations ([`NoFilter`], [`NoYield`], etc.) compile away on the vanilla path.

mod defaults;
mod plugin;
mod traits;

pub use defaults::{NoFilter, NoLocks, NoTip, NoYield, SetAccountFilter, StandardCommit};
pub use plugin::{BankingHooks, TpuHandles, TpuPlugin};
pub use traits::{
    AccountFilter, BatchCommitPolicy, BundleAccountLockView, LifecycleStage, ReadLockView,
    TipContext, TipProcessor, TipProcessorError, TpuStage, WriteLockView, YieldControl,
};
