//! A small deterministic pseudo-random number generator.
//!
//! Used to generate Zobrist keys at compile time (and later, to search for magic numbers).
//! It only needs to be fast and well mixed, not cryptographically secure.

/// SplitMix64, by Sebastiano Vigna (public domain).
pub(crate) struct SplitMix64(u64);

impl SplitMix64 {
    pub(crate) const fn new(seed: u64) -> SplitMix64 {
        SplitMix64(seed)
    }

    pub(crate) const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_reference_output() {
        // First outputs of the reference implementation seeded with 1234567.
        let mut rng = SplitMix64::new(1_234_567);
        assert_eq!(rng.next_u64(), 6_457_827_717_110_365_317);
        assert_eq!(rng.next_u64(), 3_203_168_211_198_807_973);
        assert_eq!(rng.next_u64(), 9_817_491_932_198_370_423);
    }
}
