use {solana_pubkey::Pubkey, std::thread, thiserror::Error};

/// Abort and join are separate phases to allow concurrent draining.
/// `abort()` must be idempotent and non-panicking.
pub trait LifecycleStage: Send + 'static {
    fn abort(&self);
    fn join(self: Box<Self>) -> thread::Result<()>;
}

/// Stages are shut down in reverse push order — push order is the public
/// contract; bundle stages must drain before banking threads exit.
pub trait TpuStage: LifecycleStage {}

/// Checked by Consumer before each batch. Must be non-blocking and lock-free.
pub trait YieldControl: Send + Sync + 'static {
    fn should_yield(&self) -> bool;
}

/// Per-pubkey filter called at receive-and-buffer.
pub trait AccountFilter: Send + Sync + 'static {
    fn is_blocked(&self, pubkey: &Pubkey) -> bool;
    /// Returns false when this filter has no entries — allows O(1) short-circuit
    /// at the receive-and-buffer call site instead of iterating all account keys.
    fn is_active(&self) -> bool { true }
}

/// Advisory write-lock view. TOCTOU is expected; ~50 µs staleness is
/// acceptable. Split from [`ReadLockView`] (ISP): Consumer only needs
/// write-lock info.
pub trait WriteLockView: Send + Sync + 'static {
    fn is_write_locked(&self, pubkey: &Pubkey) -> bool;
}

pub trait ReadLockView: Send + Sync + 'static {
    fn is_read_locked(&self, pubkey: &Pubkey) -> bool;
}

pub trait BundleAccountLockView: WriteLockView + ReadLockView {}

/// Called once per banking worker at slot start. Must be idempotent —
/// `AlreadyInitialized` is non-fatal.
pub trait TipProcessor: Send + Sync + 'static {
    fn process(&self, ctx: &TipContext<'_>) -> Result<(), TipProcessorError>;
}

pub trait BatchCommitPolicy: Send + Sync + 'static {
    fn revert_batch_on_error(&self) -> bool;
    /// When true, Consumer partitions the bundle into conflict-free PoH entries
    /// before recording. Jito bundles require this because they can contain
    /// write-conflicting transactions that would otherwise violate PoH invariants.
    fn partition_into_entries(&self) -> bool;
}

/// Narrow context passed to [`TipProcessor`] at slot start.
/// Fork-specific state (block engine connection, tip PDA derivation logic)
/// lives inside the implementor.
#[non_exhaustive]
pub struct TipContext<'a> {
    pub slot: u64,
    pub epoch: u64,
    pub validator_fee_payer: &'a Pubkey,
    /// The current block builder's identity; used to derive tip distribution PDAs.
    pub block_builder_pubkey: Pubkey,
    pub block_builder_commission_bps: u16,
}

impl<'a> TipContext<'a> {
    pub fn new(
        slot: u64,
        epoch: u64,
        validator_fee_payer: &'a Pubkey,
        block_builder_pubkey: Pubkey,
        block_builder_commission_bps: u16,
    ) -> Self {
        Self { slot, epoch, validator_fee_payer, block_builder_pubkey, block_builder_commission_bps }
    }
}

#[derive(Debug, Error)]
pub enum TipProcessorError {
    /// Another banking worker already initialized this slot.
    #[error("tip PDAs already initialized for slot {0}")]
    AlreadyInitialized(u64),
    #[error("tip PDA initialization failed for slot {slot}: {reason}")]
    InitializationFailed { slot: u64, reason: String },
}
