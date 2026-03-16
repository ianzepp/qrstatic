//! GF(256) finite field arithmetic for Reed-Solomon error correction.
//!
//! Uses the QR code standard polynomial: x^8 + x^4 + x^3 + x^2 + 1 (0x11d).

const POLY: u32 = 0x11d;

/// Precomputed log and exp tables for GF(256).
pub struct Gf256Tables {
    pub exp: [u8; 512],
    pub log: [u8; 256],
}

impl Default for Gf256Tables {
    fn default() -> Self {
        Self::new()
    }
}

impl Gf256Tables {
    /// Build the log/exp lookup tables.
    pub const fn new() -> Self {
        let mut exp = [0u8; 512];
        let mut log = [0u8; 256];
        let mut val: u32 = 1;
        let mut i = 0;
        while i < 255 {
            exp[i] = val as u8;
            exp[i + 255] = val as u8;
            log[val as usize] = i as u8;
            val <<= 1;
            if val >= 256 {
                val ^= POLY;
            }
            i += 1;
        }
        exp[510] = exp[0];
        exp[511] = exp[1];
        Self { exp, log }
    }
}

/// Global lookup tables, computed at compile time.
pub static TABLES: Gf256Tables = Gf256Tables::new();

/// Multiply two elements in GF(256). Returns 0 if either input is 0.
pub fn mul(a: u8, b: u8) -> u8 {
    if a == 0 || b == 0 {
        return 0;
    }
    let log_a = TABLES.log[a as usize] as usize;
    let log_b = TABLES.log[b as usize] as usize;
    TABLES.exp[log_a + log_b]
}

/// Divide a by b in GF(256). Panics if b is 0.
pub fn div(a: u8, b: u8) -> u8 {
    assert_ne!(b, 0, "division by zero in GF(256)");
    if a == 0 {
        return 0;
    }
    let log_a = TABLES.log[a as usize] as usize;
    let log_b = TABLES.log[b as usize] as usize;
    TABLES.exp[log_a + 255 - log_b]
}

/// Raise a to the power n in GF(256).
pub fn pow(a: u8, n: u32) -> u8 {
    if a == 0 {
        return 0;
    }
    let log_a = TABLES.log[a as usize] as usize;
    let e = (log_a * n as usize) % 255;
    TABLES.exp[e]
}

/// Add/subtract in GF(256) — both are XOR.
pub fn add(a: u8, b: u8) -> u8 {
    a ^ b
}

/// Compute the generator polynomial for `n` error correction codewords.
///
/// The generator polynomial is: (x - α^0)(x - α^1)...(x - α^(n-1))
/// returned as coefficients [coeff_0, coeff_1, ..., coeff_n] where
/// coeff_n is the leading coefficient (always 1).
pub fn generator_poly(n: usize) -> Vec<u8> {
    let mut gp = vec![0u8; n + 1];
    gp[0] = 1;
    let mut len = 1;

    for i in 0..n {
        let alpha_i = TABLES.exp[i];
        let new_len = len + 1;
        let mut next = vec![0u8; n + 1];
        for j in 0..len {
            next[j + 1] ^= gp[j]; // x * gp[j]
            next[j] ^= mul(gp[j], alpha_i); // α^i * gp[j]
        }
        gp = next;
        len = new_len;
    }

    gp[..n + 1].to_vec()
}

/// Evaluate a polynomial at x in GF(256).
/// Coefficients are [coeff_0, coeff_1, ...] (constant term first).
pub fn poly_eval(poly: &[u8], x: u8) -> u8 {
    if poly.is_empty() {
        return 0;
    }
    // Horner's method, but with reversed coefficient order
    let mut result = 0u8;
    for &coeff in poly.iter().rev() {
        result = add(mul(result, x), coeff);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mul_identity() {
        for i in 0..=255u16 {
            assert_eq!(mul(i as u8, 1), i as u8);
            assert_eq!(mul(1, i as u8), i as u8);
        }
    }

    #[test]
    fn mul_zero() {
        for i in 0..=255u16 {
            assert_eq!(mul(i as u8, 0), 0);
            assert_eq!(mul(0, i as u8), 0);
        }
    }

    #[test]
    fn mul_commutative() {
        for a in 1..=20u8 {
            for b in 1..=20u8 {
                assert_eq!(mul(a, b), mul(b, a));
            }
        }
    }

    #[test]
    fn mul_div_roundtrip() {
        for a in 1..=255u16 {
            for b in 1..=10u16 {
                let product = mul(a as u8, b as u8);
                assert_eq!(div(product, b as u8), a as u8);
            }
        }
    }

    #[test]
    fn pow_basic() {
        let a = 2u8;
        assert_eq!(pow(a, 0), 1);
        assert_eq!(pow(a, 1), a);
        assert_eq!(pow(a, 2), mul(a, a));
        assert_eq!(pow(a, 3), mul(mul(a, a), a));
    }

    #[test]
    fn add_is_xor() {
        assert_eq!(add(0xFF, 0xFF), 0);
        assert_eq!(add(0xAA, 0x55), 0xFF);
        assert_eq!(add(0, 42), 42);
    }

    #[test]
    fn exp_log_consistency() {
        for i in 1..=255u16 {
            let log_val = TABLES.log[i as usize];
            assert_eq!(TABLES.exp[log_val as usize], i as u8);
        }
    }

    #[test]
    fn generator_poly_length() {
        let gp = generator_poly(10);
        assert_eq!(gp.len(), 11);
    }

    #[test]
    fn generator_poly_roots() {
        // The generator polynomial should evaluate to 0 at each α^i for i in 0..n
        let n = 10;
        let gp = generator_poly(n);
        for i in 0..n {
            let root = TABLES.exp[i];
            assert_eq!(poly_eval(&gp, root), 0, "generator not zero at α^{i}");
        }
    }

    #[test]
    fn poly_eval_constant() {
        assert_eq!(poly_eval(&[42], 0), 42);
        assert_eq!(poly_eval(&[42], 1), 42);
        assert_eq!(poly_eval(&[42], 255), 42);
    }

    #[test]
    fn poly_eval_linear() {
        // f(x) = 3 + 5x → f(0) = 3, f(1) = 3^5 = 6
        let poly = vec![3, 5];
        assert_eq!(poly_eval(&poly, 0), 3);
        assert_eq!(poly_eval(&poly, 1), add(3, 5));
    }
}
