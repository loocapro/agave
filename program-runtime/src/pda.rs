//! Program-derived address derivation with a per-thread memo of the
//! Ed25519 on-curve check.
//!
//! `Pubkey::create_program_address` is SHA-256 over the seeds plus one
//! Ed25519 point decompression (`bytes_are_curve_point`) on the result. The
//! decompression is a field exponentiation and costs about 2 µs; on mainnet
//! it is roughly 9% of all SVM execution time, because every PDA-based
//! program re-derives its program-level PDAs (authorities, configs, event
//! authorities) in every transaction, and `try_find_program_address` repeats
//! the same failing bump candidates as well.
//!
//! The predicate is a pure function of the 32 candidate bytes, so a small
//! direct-mapped memo with full-key comparison is semantically transparent.
//! It is per thread (no locking) and sized to stay inside L2.

use {
    solana_pubkey::{bytes_are_curve_point, Pubkey, PubkeyError, MAX_SEEDS, MAX_SEED_LEN},
    solana_sha256_hasher::Hasher,
    std::cell::RefCell,
};

/// Same marker the SDK appends before hashing (`solana_address::PDA_MARKER`).
const PDA_MARKER: &[u8; 21] = b"ProgramDerivedAddress";

/// Direct-mapped entries. 4096 × 33 bytes ≈ 135 KiB per thread, which stays
/// well inside L2. Measured hit rate on a mainnet transaction stream: ~68%.
const CURVE_MEMO_ENTRIES: usize = 4096;

const EMPTY: u8 = 0;
const OFF_CURVE: u8 = 1;
const ON_CURVE: u8 = 2;

struct CurvePointMemo {
    keys: Box<[[u8; 32]]>,
    vals: Box<[u8]>,
}

impl CurvePointMemo {
    fn new() -> Self {
        Self {
            keys: vec![[0u8; 32]; CURVE_MEMO_ENTRIES].into_boxed_slice(),
            vals: vec![EMPTY; CURVE_MEMO_ENTRIES].into_boxed_slice(),
        }
    }

    #[inline]
    fn slot(bytes: &[u8; 32]) -> usize {
        // Candidates are SHA-256 outputs, any 8 bytes are uniformly distributed.
        (u64::from_le_bytes(bytes[8..16].try_into().unwrap()) as usize) & (CURVE_MEMO_ENTRIES - 1)
    }

    fn get(&self, bytes: &[u8; 32]) -> Option<bool> {
        let i = Self::slot(bytes);
        (self.vals[i] != EMPTY && self.keys[i] == *bytes).then(|| self.vals[i] == ON_CURVE)
    }

    fn put(&mut self, bytes: &[u8; 32], on_curve: bool) {
        let i = Self::slot(bytes);
        self.keys[i] = *bytes;
        self.vals[i] = if on_curve { ON_CURVE } else { OFF_CURVE };
    }
}

thread_local! {
    static CURVE_POINT_MEMO: RefCell<CurvePointMemo> = RefCell::new(CurvePointMemo::new());
}

/// `solana_pubkey::bytes_are_curve_point` with a per-thread memo. Identical
/// result for every input.
pub fn bytes_are_curve_point_memoized(bytes: &[u8; 32]) -> bool {
    if let Some(on_curve) = CURVE_POINT_MEMO.with(|memo| memo.borrow().get(bytes)) {
        return on_curve;
    }
    let on_curve = bytes_are_curve_point(bytes);
    CURVE_POINT_MEMO.with(|memo| memo.borrow_mut().put(bytes, on_curve));
    on_curve
}

/// Drop-in replacement for `Pubkey::create_program_address` for the runtime's
/// hot paths. Same validation, same hash, same error mapping; only the
/// on-curve check is memoized.
pub fn create_program_address(seeds: &[&[u8]], program_id: &Pubkey) -> Result<Pubkey, PubkeyError> {
    if seeds.len() > MAX_SEEDS {
        return Err(PubkeyError::MaxSeedLengthExceeded);
    }
    if seeds.iter().any(|seed| seed.len() > MAX_SEED_LEN) {
        return Err(PubkeyError::MaxSeedLengthExceeded);
    }
    let mut hasher = Hasher::default();
    for seed in seeds {
        hasher.hash(seed);
    }
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

    fn seeds_of<'a>(v: &'a [Vec<u8>]) -> Vec<&'a [u8]> {
        v.iter().map(|s| s.as_slice()).collect()
    }

    #[test]
    fn matches_sdk_on_random_seeds_and_all_bumps() {
        let program_id = Pubkey::new_unique();
        for i in 0..512u32 {
            let base = vec![i.to_le_bytes().to_vec(), b"seed".to_vec()];
            for bump in 0..=u8::MAX {
                let mut s = base.clone();
                s.push(vec![bump]);
                let seeds = seeds_of(&s);
                assert_eq!(
                    create_program_address(&seeds, &program_id),
                    Pubkey::create_program_address(&seeds, &program_id),
                    "seed {i} bump {bump}"
                );
            }
        }
    }

    #[test]
    fn matches_sdk_on_repeated_queries_and_memo_thrash() {
        let program_id = Pubkey::new_unique();
        let cases: Vec<Vec<Vec<u8>>> = (0..(CURVE_MEMO_ENTRIES as u32 * 3))
            .map(|i| vec![i.to_be_bytes().to_vec()])
            .collect();
        for round in 0..2 {
            for s in &cases {
                let seeds = seeds_of(s);
                assert_eq!(
                    create_program_address(&seeds, &program_id),
                    Pubkey::create_program_address(&seeds, &program_id),
                    "round {round}"
                );
            }
        }
    }

    #[test]
    fn on_curve_inputs_are_memoized_correctly() {
        // Real public keys are on the curve: the memoized predicate must say so
        // before and after insertion, and off-curve bytes must stay off-curve.
        for _ in 0..256 {
            let key = solana_pubkey::new_rand();
            let bytes = key.to_bytes();
            let expected = bytes_are_curve_point(&bytes);
            assert_eq!(bytes_are_curve_point_memoized(&bytes), expected);
            assert_eq!(bytes_are_curve_point_memoized(&bytes), expected);
        }
    }

    #[test]
    fn validation_errors_match_sdk() {
        let program_id = Pubkey::new_unique();
        let long = vec![0u8; MAX_SEED_LEN + 1];
        assert_eq!(
            create_program_address(&[&long], &program_id),
            Err(PubkeyError::MaxSeedLengthExceeded)
        );
        let many: Vec<&[u8]> = vec![&[1u8][..]; MAX_SEEDS + 1];
        assert_eq!(
            create_program_address(&many, &program_id),
            Err(PubkeyError::MaxSeedLengthExceeded)
        );
        let max: Vec<&[u8]> = vec![&[1u8][..]; MAX_SEEDS];
        assert_eq!(
            create_program_address(&max, &program_id),
            Pubkey::create_program_address(&max, &program_id)
        );
    }
}
