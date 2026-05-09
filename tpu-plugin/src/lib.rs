//! Extension traits for plugging custom stages into the Agave TPU pipeline.
//!
//! # Extension points
//!
//! - [`TpuPlugin`] — the bundle a fork installs at startup: extra pipeline stages plus
//!   [`BankingHooks`]. Vanilla Agave uses [`TpuPlugin::default`], which is zero-cost.
//!
//! - [`BankingHooks`] — behavioral overrides injected into the banking stage: yield
//!   control, account filtering, lock visibility, tip processing, and batch commit policy.
//!
//! All hook traits have null implementations ([`NoFilter`], [`NoYield`], [`NoLocks`],
//! [`NoTip`], [`StandardCommit`]) that compile away on the vanilla path via monomorphisation.

mod defaults;
mod plugin;
mod traits;

pub use defaults::{NoFilter, NoLocks, NoTip, NoYield, SetAccountFilter, StandardCommit};
pub use plugin::{BankingHooks, TpuHandles, TpuPlugin};
pub use traits::{
    AccountFilter, BatchCommitPolicy, BundleAccountLockView, LifecycleStage, ReadLockView,
    TipContext, TipProcessor, TipProcessorError, TpuStage, WriteLockView, YieldControl,
};
