use crate::sha256::sha256;

/// Xoshiro256** pseudorandom number generator.
///
/// Fast, deterministic, non-cryptographic PRNG used for reproducible
/// noise frame generation. Seeded via SHA-256 of a key string.
#[derive(Debug, Clone)]
pub struct Prng {
    s: [u64; 4],
}

impl Prng {
    /// Create a PRNG from a 32-byte seed (e.g. SHA-256 output).
    pub fn from_seed(seed: [u8; 32]) -> Self {
        let s = [
            u64::from_le_bytes(seed[0..8].try_into().unwrap()),
            u64::from_le_bytes(seed[8..16].try_into().unwrap()),
            u64::from_le_bytes(seed[16..24].try_into().unwrap()),
            u64::from_le_bytes(seed[24..32].try_into().unwrap()),
        ];
        Self { s }
    }

    /// Create a PRNG by hashing `"key:index"` with SHA-256.
    ///
    /// This is the standard pattern for per-frame deterministic RNG:
    /// both encoder and decoder can reconstruct the same sequence.
    pub fn from_key(key: &str, index: u64) -> Self {
        let input = format!("{key}:{index}");
        Self::from_seed(sha256(input.as_bytes()))
    }

    /// Create a PRNG by hashing an arbitrary string with SHA-256.
    pub fn from_str_seed(s: &str) -> Self {
        Self::from_seed(sha256(s.as_bytes()))
    }

    /// Next raw 64-bit value (xoshiro256** algorithm).
    pub fn next_u64(&mut self) -> u64 {
        let result = (self.s[1].wrapping_mul(5)).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;

        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];

        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);

        result
    }

    /// Next float in [0, 1).
    pub fn next_f32(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Next float in [0, 1) as f64 (higher precision).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Returns `true` with probability `p` (0.0 to 1.0).
    pub fn next_bool(&mut self, p: f32) -> bool {
        self.next_f32() < p
    }

    /// Next float uniformly distributed in [lo, hi).
    pub fn next_range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_f32() * (hi - lo)
    }

    /// Fill a slice with random bytes.
    pub fn fill_bytes(&mut self, buf: &mut [u8]) {
        let mut i = 0;
        while i < buf.len() {
            let val = self.next_u64();
            let bytes = val.to_le_bytes();
            let remaining = buf.len() - i;
            let to_copy = remaining.min(8);
            buf[i..i + to_copy].copy_from_slice(&bytes[..to_copy]);
            i += to_copy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn determinism_from_seed() {
        let seed = sha256(b"test-seed");
        let mut a = Prng::from_seed(seed);
        let mut b = Prng::from_seed(seed);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn determinism_from_key() {
        let mut a = Prng::from_key("my-key", 42);
        let mut b = Prng::from_key("my-key", 42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_keys_differ() {
        let mut a = Prng::from_key("key-a", 0);
        let mut b = Prng::from_key("key-b", 0);
        // Extremely unlikely to collide
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn different_indices_differ() {
        let mut a = Prng::from_key("same-key", 0);
        let mut b = Prng::from_key("same-key", 1);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn next_f32_in_range() {
        let mut rng = Prng::from_str_seed("range-test");
        for _ in 0..10_000 {
            let v = rng.next_f32();
            assert!((0.0..1.0).contains(&v), "f32 out of range: {v}");
        }
    }

    #[test]
    fn next_f64_in_range() {
        let mut rng = Prng::from_str_seed("range-test-f64");
        for _ in 0..10_000 {
            let v = rng.next_f64();
            assert!((0.0..1.0).contains(&v), "f64 out of range: {v}");
        }
    }

    #[test]
    fn next_bool_distribution() {
        let mut rng = Prng::from_str_seed("bool-test");
        let n = 10_000;
        let trues = (0..n).filter(|_| rng.next_bool(0.5)).count();
        // Should be roughly 50% ± 5%
        assert!(
            (4000..=6000).contains(&trues),
            "unexpected bool distribution: {trues}/{n}"
        );
    }

    #[test]
    fn next_bool_biased() {
        let mut rng = Prng::from_str_seed("bias-test");
        let n = 10_000;
        let trues = (0..n).filter(|_| rng.next_bool(0.8)).count();
        // Should be roughly 80% ± 5%
        assert!(
            (7000..=9000).contains(&trues),
            "unexpected biased distribution: {trues}/{n}"
        );
    }

    #[test]
    fn next_range_bounds() {
        let mut rng = Prng::from_str_seed("range-bounds");
        for _ in 0..10_000 {
            let v = rng.next_range(-1.0, 1.0);
            assert!((-1.0..1.0).contains(&v), "range value out of bounds: {v}");
        }
    }

    #[test]
    fn fill_bytes_deterministic() {
        let mut a = Prng::from_str_seed("fill-test");
        let mut b = Prng::from_str_seed("fill-test");
        let mut buf_a = [0u8; 37]; // Intentionally not aligned to 8
        let mut buf_b = [0u8; 37];
        a.fill_bytes(&mut buf_a);
        b.fill_bytes(&mut buf_b);
        assert_eq!(buf_a, buf_b);
    }

    #[test]
    fn fill_bytes_not_all_zero() {
        let mut rng = Prng::from_str_seed("nonzero-test");
        let mut buf = [0u8; 32];
        rng.fill_bytes(&mut buf);
        assert!(buf.iter().any(|&b| b != 0));
    }
}
