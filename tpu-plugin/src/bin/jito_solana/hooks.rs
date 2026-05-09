use {
    agave_tpu_plugin::{
        AccountFilter, BatchCommitPolicy, BundleAccountLockView, ReadLockView, TipContext,
        TipProcessor, TipProcessorError, WriteLockView, YieldControl,
    },
    solana_pubkey::Pubkey,
    std::{
        collections::HashSet,
        sync::{Arc, Mutex, RwLock},
        sync::atomic::{AtomicBool, Ordering},
    },
};

// The 8 mainnet Jito tip accounts (https://jito.network/docs/tip-accounts)
pub const TIP_ACCOUNTS: [&str; 8] = [
    "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
    "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
    "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
    "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
    "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
    "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
    "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
    "4xDsmeTWPNjgSVSS1VTfzFq3iHZhp77ffPkAmkZkdu71",
];

pub fn tip_account_pubkeys() -> impl Iterator<Item = Pubkey> {
    TIP_ACCOUNTS.iter().map(|s| s.parse().unwrap())
}

// --- AccountFilter ---

pub struct BundleFilter(HashSet<Pubkey>);

impl BundleFilter {
    pub fn jito_mainnet() -> Self {
        Self(tip_account_pubkeys().collect())
    }
}

impl AccountFilter for BundleFilter {
    #[inline(always)]
    fn is_blocked(&self, pubkey: &Pubkey) -> bool { self.0.contains(pubkey) }
}

// --- WriteLockView / ReadLockView ---

pub struct BundleLocks(Arc<RwLock<HashSet<Pubkey>>>);

impl BundleLocks {
    pub fn new() -> Self {
        Self(Arc::new(RwLock::new(HashSet::new())))
    }

    // BundleStage write path — called per-bundle during execution, not simulated here.
    #[allow(dead_code)]
    pub fn lock(&self, pubkey: Pubkey) { self.0.write().unwrap().insert(pubkey); }

    #[allow(dead_code)]
    pub fn unlock(&self, pubkey: &Pubkey) { self.0.write().unwrap().remove(pubkey); }
}

impl WriteLockView for BundleLocks {
    #[inline(always)]
    fn is_write_locked(&self, pubkey: &Pubkey) -> bool { self.0.read().unwrap().contains(pubkey) }
}

impl ReadLockView for BundleLocks {
    #[inline(always)]
    fn is_read_locked(&self, pubkey: &Pubkey) -> bool { self.0.read().unwrap().contains(pubkey) }
}

impl BundleAccountLockView for BundleLocks {}

// --- YieldControl ---

pub struct BundleYield(Arc<AtomicBool>);

impl BundleYield {
    pub fn new(flag: Arc<AtomicBool>) -> Self { Self(flag) }
}

impl YieldControl for BundleYield {
    #[inline(always)]
    fn should_yield(&self) -> bool { self.0.load(Ordering::Acquire) }
}

// --- TipProcessor ---

pub struct TipManager {
    #[allow(dead_code)]
    tip_accounts: HashSet<Pubkey>,
    // Multiple banking workers call process() per slot; only the first wins.
    initialized_slots: Arc<Mutex<HashSet<u64>>>,
}

impl TipManager {
    pub fn new(tip_accounts: impl IntoIterator<Item = Pubkey>) -> Self {
        Self {
            tip_accounts: tip_accounts.into_iter().collect(),
            initialized_slots: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl TipProcessor for TipManager {
    fn process(&self, ctx: &TipContext<'_>) -> Result<(), TipProcessorError> {
        if !self.initialized_slots.lock().unwrap().insert(ctx.slot) {
            return Err(TipProcessorError::AlreadyInitialized(ctx.slot));
        }
        Ok(())
    }
}

// --- BatchCommitPolicy ---

pub struct BundleBatchPolicy;

impl BatchCommitPolicy for BundleBatchPolicy {
    #[inline(always)]
    fn revert_batch_on_error(&self) -> bool { true }
    #[inline(always)]
    fn partition_into_entries(&self) -> bool { true }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tip_accounts_are_blocked() {
        let filter = BundleFilter::jito_mainnet();
        for s in TIP_ACCOUNTS {
            assert!(filter.is_blocked(&s.parse().unwrap()));
        }
    }

    #[test]
    fn non_tip_account_passes() {
        assert!(!BundleFilter::jito_mainnet().is_blocked(&Pubkey::default()));
    }

    #[test]
    fn lock_round_trip() {
        let locks = BundleLocks::new();
        let account = Pubkey::new_unique();
        locks.lock(account);
        assert!(locks.is_write_locked(&account));
        locks.unlock(&account);
        assert!(!locks.is_write_locked(&account));
    }

    #[test]
    fn yield_flag_starts_false() {
        let flag = Arc::new(AtomicBool::new(false));
        let ctrl = BundleYield::new(Arc::clone(&flag));
        assert!(!ctrl.should_yield());
        flag.store(true, Ordering::Release);
        assert!(ctrl.should_yield());
    }

    #[test]
    fn bundle_commit_semantics() {
        assert!(BundleBatchPolicy.revert_batch_on_error());
        assert!(BundleBatchPolicy.partition_into_entries());
    }
}
