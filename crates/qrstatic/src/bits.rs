//! Bit packing, unpacking, and majority voting utilities.
//!
//! Every payload-carrying codec uses the same pattern:
//! 1. Unpack payload bytes to individual bits
//! 2. Spread bits across grid cells with repetition
//! 3. On decode: majority vote across all repetitions of each bit
//! 4. Pack voted bits back to bytes

/// Unpack bytes into a vector of individual bits (MSB first within each byte).
pub fn bytes_to_bits(data: &[u8]) -> Vec<u8> {
    let mut bits = Vec::with_capacity(data.len() * 8);
    for &byte in data {
        for shift in (0..8).rev() {
            bits.push((byte >> shift) & 1);
        }
    }
    bits
}

/// Pack individual bits (MSB first) back into bytes.
/// Truncates any trailing bits that don't fill a complete byte.
pub fn bits_to_bytes(bits: &[u8]) -> Vec<u8> {
    let n_bytes = bits.len() / 8;
    let mut bytes = Vec::with_capacity(n_bytes);
    for chunk in bits.chunks_exact(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            byte |= (bit & 1) << (7 - i);
        }
        bytes.push(byte);
    }
    bytes
}

/// Given a list of float samples for each bit position, vote on whether
/// each bit is 0 or 1 based on the sign/magnitude of the accumulated signal.
///
/// Each entry in `votes` contains all the repeated samples for one bit.
/// A positive sum → bit 1, negative sum → bit 0.
pub fn majority_vote_f32(votes: &[Vec<f32>]) -> Vec<u8> {
    votes
        .iter()
        .map(|samples| {
            let sum: f32 = samples.iter().sum();
            if sum >= 0.0 { 1 } else { 0 }
        })
        .collect()
}

/// Same as `majority_vote_f32` but for integer samples.
pub fn majority_vote_i16(votes: &[Vec<i16>]) -> Vec<u8> {
    votes
        .iter()
        .map(|samples| {
            let sum: i32 = samples.iter().map(|&v| v as i32).sum();
            if sum >= 0 { 1 } else { 0 }
        })
        .collect()
}

/// Map each cell in a grid to a bit index based on `cell_index % n_bits`.
///
/// Returns a vector of `n_bits` entries, each containing the grid cell indices
/// assigned to that bit. This is how payload bits are spread across many cells
/// for redundancy via majority voting.
pub fn spread_bits(n_cells: usize, n_bits: usize) -> Vec<Vec<usize>> {
    let mut mapping = vec![Vec::new(); n_bits];
    for cell in 0..n_cells {
        mapping[cell % n_bits].push(cell);
    }
    mapping
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_to_bits_single() {
        assert_eq!(bytes_to_bits(&[0b10110001]), vec![1, 0, 1, 1, 0, 0, 0, 1]);
    }

    #[test]
    fn bytes_to_bits_multiple() {
        let bits = bytes_to_bits(&[0xFF, 0x00]);
        assert_eq!(bits, vec![1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn bytes_to_bits_empty() {
        assert!(bytes_to_bits(&[]).is_empty());
    }

    #[test]
    fn bits_to_bytes_roundtrip() {
        let original = vec![0xDE, 0xAD, 0xBE, 0xEF];
        let bits = bytes_to_bits(&original);
        let recovered = bits_to_bytes(&bits);
        assert_eq!(recovered, original);
    }

    #[test]
    fn bits_to_bytes_truncates_partial() {
        // 10 bits → only first 8 become a byte
        let bits = vec![1, 0, 1, 0, 1, 0, 1, 0, 1, 1];
        let bytes = bits_to_bytes(&bits);
        assert_eq!(bytes, vec![0b10101010]);
    }

    #[test]
    fn majority_vote_f32_clean_signal() {
        let votes = vec![
            vec![1.0, 1.0, 1.0],    // clearly bit 1
            vec![-1.0, -1.0, -1.0], // clearly bit 0
            vec![0.5, 0.3, 0.1],    // positive → bit 1
        ];
        assert_eq!(majority_vote_f32(&votes), vec![1, 0, 1]);
    }

    #[test]
    fn majority_vote_f32_noisy_signal() {
        let votes = vec![
            vec![1.0, -0.5, 1.0, 0.8],   // sum = 1.3 → bit 1
            vec![-1.0, 0.5, -1.0, -0.8], // sum = -1.3 → bit 0
        ];
        assert_eq!(majority_vote_f32(&votes), vec![1, 0]);
    }

    #[test]
    fn majority_vote_i16_basic() {
        let votes = vec![
            vec![5i16, 3, -1],  // sum = 7 → bit 1
            vec![-5i16, -3, 1], // sum = -7 → bit 0
        ];
        assert_eq!(majority_vote_i16(&votes), vec![1, 0]);
    }

    #[test]
    fn majority_vote_zero_is_one() {
        // Tie-breaking: zero → bit 1 (positive side)
        let votes = vec![vec![1.0f32, -1.0]];
        assert_eq!(majority_vote_f32(&votes), vec![1]);
    }

    #[test]
    fn spread_bits_even() {
        let mapping = spread_bits(8, 4);
        assert_eq!(mapping[0], vec![0, 4]);
        assert_eq!(mapping[1], vec![1, 5]);
        assert_eq!(mapping[2], vec![2, 6]);
        assert_eq!(mapping[3], vec![3, 7]);
    }

    #[test]
    fn spread_bits_uneven() {
        let mapping = spread_bits(7, 3);
        assert_eq!(mapping[0], vec![0, 3, 6]);
        assert_eq!(mapping[1], vec![1, 4]);
        assert_eq!(mapping[2], vec![2, 5]);
    }

    #[test]
    fn spread_bits_more_bits_than_cells() {
        let mapping = spread_bits(3, 5);
        assert_eq!(mapping[0], vec![0]);
        assert_eq!(mapping[1], vec![1]);
        assert_eq!(mapping[2], vec![2]);
        assert!(mapping[3].is_empty());
        assert!(mapping[4].is_empty());
    }

    #[test]
    fn full_payload_roundtrip() {
        let payload = b"Hello!";
        let bits = bytes_to_bits(payload);
        let recovered = bits_to_bytes(&bits);
        assert_eq!(recovered, payload);
    }
}
