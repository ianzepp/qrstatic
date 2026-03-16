//! Reed-Solomon encoder and decoder for QR code error correction.
//!
//! Supports encoding (generating EC codewords) and decoding (correcting errors).
//! EC level H can correct up to ~30% of codewords.

use super::gf256;

/// Compute Reed-Solomon error correction codewords.
///
/// Given `data` bytes and the number of EC codewords to generate,
/// returns just the EC codewords (not the data).
///
/// Uses systematic encoding: the data bytes are treated as the high-degree
/// coefficients of a polynomial, and we compute the remainder when dividing
/// by the generator polynomial. The data bytes are NOT modified.
pub fn encode(data: &[u8], n_ec: usize) -> Vec<u8> {
    let gp = gf256::generator_poly(n_ec);

    // We work in "high-degree-first" order for the division.
    // gp is in constant-term-first order [c0, c1, ..., cn], so
    // gp[n_ec] is the leading coefficient (always 1).
    // We reverse it to get high-degree-first for the long division.
    let gp_rev: Vec<u8> = gp.iter().rev().copied().collect();

    // msg holds the dividend: data shifted up by n_ec positions (high-degree-first).
    // Positions 0..data.len() are data, positions data.len()..data.len()+n_ec are remainder.
    let mut msg = vec![0u8; data.len() + n_ec];
    msg[..data.len()].copy_from_slice(data);

    // Polynomial long division (high-degree-first order).
    // gp_rev[0] == 1 (monic), so the leading coeff of the divisor is 1.
    for i in 0..data.len() {
        let coeff = msg[i];
        if coeff != 0 {
            // Subtract (XOR) coeff * generator from msg starting at position i.
            // Skip j=0 because gp_rev[0]=1 and we want to preserve data[i].
            for j in 1..gp_rev.len() {
                msg[i + j] ^= gf256::mul(gp_rev[j], coeff);
            }
        }
    }

    // The remainder is in msg[data.len()..].
    msg[data.len()..].to_vec()
}

/// Compute syndromes for received codeword. All-zero syndromes means no errors.
///
/// The received word is stored as [data..., ec...] in high-degree-first order,
/// but poly_eval expects constant-term-first order, so we reverse.
fn syndromes(received: &[u8], n_ec: usize) -> Vec<u8> {
    let rev: Vec<u8> = received.iter().rev().copied().collect();
    (0..n_ec)
        .map(|i| gf256::poly_eval(&rev, gf256::TABLES.exp[i]))
        .collect()
}

/// Decode Reed-Solomon encoded data, correcting errors if possible.
///
/// `received` contains data bytes followed by EC codewords.
/// `n_ec` is the number of EC codewords.
/// Returns the corrected data bytes (without EC), or None if uncorrectable.
pub fn decode(received: &[u8], n_ec: usize) -> Option<Vec<u8>> {
    let n = received.len();
    let n_data = n - n_ec;

    let synd = syndromes(received, n_ec);

    if synd.iter().all(|&s| s == 0) {
        return Some(received[..n_data].to_vec());
    }

    let sigma = berlekamp_massey(&synd, n_ec);

    let error_positions = chien_search(&sigma, n);

    if error_positions.len() != sigma.len() - 1 {
        return None;
    }

    let error_values = forney(&synd, &sigma, &error_positions, n);

    let mut corrected = received.to_vec();
    for (&pos, &val) in error_positions.iter().zip(error_values.iter()) {
        corrected[pos] ^= val;
    }

    let check = syndromes(&corrected, n_ec);
    if check.iter().all(|&s| s == 0) {
        Some(corrected[..n_data].to_vec())
    } else {
        None
    }
}

/// Berlekamp-Massey algorithm for finding the error locator polynomial.
#[allow(clippy::needless_range_loop)]
fn berlekamp_massey(synd: &[u8], n_ec: usize) -> Vec<u8> {
    let mut c = vec![0u8; n_ec + 1]; // current error locator
    let mut b = vec![0u8; n_ec + 1]; // previous error locator
    c[0] = 1;
    b[0] = 1;
    let mut l = 0usize;
    let mut m: usize = 1;
    let mut delta_b: u8 = 1;

    for n in 0..n_ec {
        // Compute discrepancy
        let mut delta = synd[n];
        for j in 1..=l {
            delta ^= gf256::mul(c[j], synd[n - j]);
        }

        if delta == 0 {
            m += 1;
        } else if 2 * l <= n {
            let t = c.clone();
            let factor = gf256::div(delta, delta_b);
            for j in m..=n_ec {
                let bj = j - m;
                if bj < b.len() {
                    c[j] ^= gf256::mul(factor, b[bj]);
                }
            }
            l = n + 1 - l;
            b = t;
            delta_b = delta;
            m = 1;
        } else {
            let factor = gf256::div(delta, delta_b);
            for j in m..=n_ec {
                let bj = j - m;
                if bj < b.len() {
                    c[j] ^= gf256::mul(factor, b[bj]);
                }
            }
            m += 1;
        }
    }

    c[..l + 1].to_vec()
}

/// Chien search: find the roots of the error locator polynomial.
///
/// The error locator polynomial σ(x) has roots at X_k^{-1} = α^{-j_k}
/// where j_k is the polynomial degree of the k-th error.
/// Array position is n-1-j_k (since byte array is high-degree-first).
fn chien_search(sigma: &[u8], n: usize) -> Vec<usize> {
    let mut positions = Vec::new();
    for j in 0..n {
        // Test x = α^{-j} = α^{255-j} (since α^255 = 1)
        let x = gf256::TABLES.exp[(255 - (j % 255)) % 255];
        if gf256::poly_eval(sigma, x) == 0 {
            // Error at polynomial degree j → array position n-1-j
            positions.push(n - 1 - j);
        }
    }
    positions
}

/// Forney's algorithm: compute error magnitudes.
fn forney(synd: &[u8], sigma: &[u8], positions: &[usize], n: usize) -> Vec<u8> {
    let n_errors = positions.len();
    let n_ec = synd.len();

    // Error evaluator polynomial: Ω(x) = S(x) * σ(x) mod x^n_ec
    // where S(x) = S_0 + S_1*x + S_2*x^2 + ...
    let mut omega = vec![0u8; n_ec];
    for i in 0..n_ec {
        for j in 0..sigma.len() {
            if i >= j {
                omega[i] ^= gf256::mul(synd[i - j], sigma[j]);
            }
        }
    }

    // Formal derivative of sigma: σ'(x) = σ_1 + 2*σ_2*x + 3*σ_3*x^2 + ...
    // In GF(2^m), even coefficients vanish: σ'(x) = σ_1 + σ_3*x^2 + σ_5*x^4 + ...
    let mut sigma_deriv = vec![0u8; sigma.len()];
    for i in (1..sigma.len()).step_by(2) {
        sigma_deriv[i - 1] = sigma[i];
    }

    let mut values = Vec::with_capacity(n_errors);
    for &pos in positions {
        // Array position pos → polynomial degree j = n-1-pos
        // Error locator X_k = α^j
        // Forney: e_k = X_k * Ω(X_k^{-1}) / σ'(X_k^{-1})
        // (negation is identity in GF(2^m))
        let j = n - 1 - pos;
        let x_k = gf256::TABLES.exp[j % 255];
        let x_k_inv = gf256::TABLES.exp[(255 - (j % 255)) % 255];

        let omega_val = gf256::poly_eval(&omega, x_k_inv);
        let sigma_deriv_val = gf256::poly_eval(&sigma_deriv, x_k_inv);

        if sigma_deriv_val == 0 {
            values.push(0);
        } else {
            let val = gf256::mul(x_k, gf256::div(omega_val, sigma_deriv_val));
            values.push(val);
        }
    }

    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_produces_correct_length() {
        let data = b"Hello";
        let ec = encode(data, 10);
        assert_eq!(ec.len(), 10);
    }

    #[test]
    fn encode_deterministic() {
        let data = b"test data";
        let ec1 = encode(data, 8);
        let ec2 = encode(data, 8);
        assert_eq!(ec1, ec2);
    }

    #[test]
    fn decode_no_errors() {
        let data = b"Hello, World!";
        let ec = encode(data, 10);
        let mut received = data.to_vec();
        received.extend_from_slice(&ec);
        let decoded = decode(&received, 10).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_single_error() {
        let data = b"Test message";
        let n_ec = 10;
        let ec = encode(data, n_ec);
        let mut received = data.to_vec();
        received.extend_from_slice(&ec);
        received[3] ^= 0x55;
        let decoded = decode(&received, n_ec).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_multiple_errors() {
        let data = b"Error correction test";
        let n_ec = 20;
        let ec = encode(data, n_ec);
        let mut received = data.to_vec();
        received.extend_from_slice(&ec);
        received[0] ^= 0xAA;
        received[5] ^= 0x55;
        received[10] ^= 0xFF;
        let decoded = decode(&received, n_ec).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_max_correctable() {
        let data = b"Max errors test!!";
        let n_ec = 10;
        let ec = encode(data, n_ec);
        let mut received = data.to_vec();
        received.extend_from_slice(&ec);
        received[0] ^= 0x01;
        received[4] ^= 0x02;
        received[8] ^= 0x04;
        received[12] ^= 0x08;
        received[16] ^= 0x10;
        let decoded = decode(&received, n_ec).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_too_many_errors_returns_none() {
        let data = b"Too many errors";
        let n_ec = 4;
        let ec = encode(data, n_ec);
        let mut received = data.to_vec();
        received.extend_from_slice(&ec);
        for byte in received.iter_mut().take(5) {
            *byte ^= 0xFF;
        }
        assert!(decode(&received, n_ec).is_none());
    }

    #[test]
    fn syndromes_zero_for_valid() {
        let data = b"Valid";
        let ec = encode(data, 8);
        let mut received = data.to_vec();
        received.extend_from_slice(&ec);
        let synd = syndromes(&received, 8);
        assert!(synd.iter().all(|&s| s == 0));
    }

    #[test]
    fn syndromes_nonzero_for_errors() {
        let data = b"Errors";
        let ec = encode(data, 8);
        let mut received = data.to_vec();
        received.extend_from_slice(&ec);
        received[0] ^= 1;
        let synd = syndromes(&received, 8);
        assert!(synd.iter().any(|&s| s != 0));
    }
}
