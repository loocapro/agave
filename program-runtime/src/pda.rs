//! `Pubkey::create_program_address` with a per-thread memo of the on-curve check.
//!
//! The Ed25519 point decompression costs about 2 µs and is ~9% of mainnet SVM execution
//! time, because programs re-derive the same PDAs in every transaction and
//! `try_find_program_address` retries the same failing bumps. The predicate is a pure
//! function of the 32 candidate bytes, so a direct-mapped memo with full-key comparison
//! returns identical results.

use {
    solana_pubkey::{MAX_SEED_LEN, MAX_SEEDS, Pubkey, PubkeyError, bytes_are_curve_point},
    solana_sha256_hasher::Hasher,
    std::cell::RefCell,
};

/// Same marker the SDK appends before hashing (`solana_address::PDA_MARKER`).
const PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

/// 4096 × 33 bytes ≈ 135 KiB per thread, inside L2. Mainnet hit rate ≈ 68%.
const CURVE_MEMO_ENTRIES: usize = 4096;

#[derive(Clone, Copy)]
struct Entry {
    candidate: [u8; 32],
    on_curve: bool,
}

struct CurvePointMemo(Box<[Option<Entry>]>);

impl CurvePointMemo {
    fn new() -> Self {
        Self(vec![None; CURVE_MEMO_ENTRIES].into_boxed_slice())
    }

    fn is_curve_point(&mut self, candidate: &[u8; 32]) -> bool {
        let slot = Self::slot(candidate);
        match self.0[slot] {
            Some(entry) if entry.candidate == *candidate => entry.on_curve,
            _ => {
                let on_curve = bytes_are_curve_point(candidate);
                self.0[slot] = Some(Entry {
                    candidate: *candidate,
                    on_curve,
                });
                on_curve
            }
        }
    }

    /// Candidates are SHA-256 outputs, so any 8 bytes index uniformly.
    fn slot(candidate: &[u8; 32]) -> usize {
        u64::from_le_bytes(candidate[8..16].try_into().unwrap()) as usize % CURVE_MEMO_ENTRIES
    }
}

thread_local! {
    static CURVE_POINT_MEMO: RefCell<CurvePointMemo> = RefCell::new(CurvePointMemo::new());
}

/// `solana_pubkey::bytes_are_curve_point` with a per-thread memo.
pub fn bytes_are_curve_point_memoized(bytes: &[u8; 32]) -> bool {
    CURVE_POINT_MEMO.with(|memo| memo.borrow_mut().is_curve_point(bytes))
}

/// `Pubkey::create_program_address` with the on-curve check memoized. Same validation,
/// hash and errors.
pub fn create_program_address(seeds: &[&[u8]], program_id: &Pubkey) -> Result<Pubkey, PubkeyError> {
    if seeds.len() > MAX_SEEDS || seeds.iter().any(|seed| seed.len() > MAX_SEED_LEN) {
        return Err(PubkeyError::MaxSeedLengthExceeded);
    }
    let mut hasher = Hasher::default();
    hasher.hashv(seeds);
    hasher.hashv(&[program_id.as_ref(), PDA_MARKER]);
    let hash = hasher.result().to_bytes();
    if bytes_are_curve_point_memoized(&hash) {
        return Err(PubkeyError::InvalidSeeds);
    }
    Ok(Pubkey::from(hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_matches_sdk(seeds: &[&[u8]], program_id: &Pubkey) {
        assert_eq!(
            create_program_address(seeds, program_id),
            Pubkey::create_program_address(seeds, program_id),
            "{seeds:?}"
        );
    }

    #[test]
    fn matches_sdk_on_all_bumps_and_memo_thrash() {
        let program_id = Pubkey::new_unique();
        for i in 0..512u32 {
            for bump in 0..=u8::MAX {
                assert_matches_sdk(&[&i.to_le_bytes(), b"seed", &[bump]], &program_id);
            }
        }
        for _round in 0..2 {
            for i in 0..(CURVE_MEMO_ENTRIES as u32 * 3) {
                assert_matches_sdk(&[&i.to_be_bytes()], &program_id);
            }
        }
    }

    #[test]
    fn memo_matches_predicate_before_and_after_insertion() {
        for _ in 0..256 {
            let bytes = solana_pubkey::new_rand().to_bytes();
            let expected = bytes_are_curve_point(&bytes);
            assert_eq!(bytes_are_curve_point_memoized(&bytes), expected);
            assert_eq!(bytes_are_curve_point_memoized(&bytes), expected);
        }
    }

    #[test]
    fn validation_errors_match_sdk() {
        let program_id = Pubkey::new_unique();
        let long = [0u8; MAX_SEED_LEN + 1];
        let many = [&[1u8][..]; MAX_SEEDS + 1];
        assert_matches_sdk(&[&long], &program_id);
        assert_matches_sdk(&many, &program_id);
        assert_matches_sdk(&many[..MAX_SEEDS], &program_id);
    }
}
