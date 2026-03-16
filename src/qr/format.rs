//! QR code format information encoding and decoding.
//!
//! Format info is a 15-bit BCH(15,5) code encoding:
//! - 2 bits: error correction level
//! - 3 bits: mask pattern
//!
//! It is placed in two copies around the finder patterns.

/// Error correction level values used in format info.
const EC_BITS_H: u8 = 0b10; // Level H (we only use H)

/// Mask for XORing format info (to avoid all-zero patterns).
const FORMAT_MASK: u16 = 0x5412;

/// Generator polynomial for BCH(15,5): x^10 + x^8 + x^5 + x^4 + x^2 + x + 1.
const BCH_GEN: u16 = 0x537;

/// Encode format information for a given mask pattern at EC level H.
/// Returns a 15-bit value ready to be placed in the QR code.
pub fn encode_format_info(mask_pattern: u8) -> u16 {
    assert!(mask_pattern < 8, "mask pattern must be 0-7");

    let data = ((EC_BITS_H as u16) << 3) | (mask_pattern as u16);
    let mut remainder = data << 10;

    // BCH division
    for i in (0..5).rev() {
        if remainder & (1 << (i + 10)) != 0 {
            remainder ^= BCH_GEN << i;
        }
    }

    let format_info = (data << 10) | remainder;
    format_info ^ FORMAT_MASK
}

/// Decode format information from a 15-bit value.
/// Returns (ec_level_bits, mask_pattern), or None if uncorrectable.
///
/// Since we only use EC level H, callers can verify ec_level == 0b10.
pub fn decode_format_info(raw: u16) -> Option<(u8, u8)> {
    let unmasked = raw ^ FORMAT_MASK;

    // Try all 32 possible valid format info values and find closest (by Hamming distance)
    let mut best_dist = u32::MAX;
    let mut best_data = 0u8;

    for data in 0..32u8 {
        let mut encoded = (data as u16) << 10;
        let mut rem = encoded;
        for i in (0..5).rev() {
            if rem & (1 << (i + 10)) != 0 {
                rem ^= BCH_GEN << i;
            }
        }
        encoded |= rem;

        let dist = (encoded ^ unmasked).count_ones();
        if dist < best_dist {
            best_dist = dist;
            best_data = data;
        }
    }

    // BCH(15,5) can correct up to 3 errors
    if best_dist > 3 {
        return None;
    }

    let ec_level = (best_data >> 3) & 0x03;
    let mask_pattern = best_data & 0x07;
    Some((ec_level, mask_pattern))
}

/// Positions where format info bits are placed in the QR code.
/// Returns two arrays of (row, col) for the two copies.
///
/// Copy 1: around top-left finder pattern
/// Copy 2: split between bottom-left and top-right finder patterns
#[allow(clippy::type_complexity)]
pub fn format_info_positions(size: usize) -> ([(usize, usize); 15], [(usize, usize); 15]) {
    let mut copy1 = [(0usize, 0usize); 15];
    let mut copy2 = [(0usize, 0usize); 15];

    // Copy 1: along row 8 (left side) and column 8 (top side)
    // Bits 0-7: column 8, rows 0-5, 7, 8 (skipping row 6 = timing)
    let rows1 = [0, 1, 2, 3, 4, 5, 7, 8];
    for (i, &r) in rows1.iter().enumerate() {
        copy1[i] = (r, 8);
    }
    // Bits 8-14: row 8, columns 7, 5, 4, 3, 2, 1, 0 (skipping col 6 = timing)
    let cols1 = [7, 5, 4, 3, 2, 1, 0];
    for (i, &c) in cols1.iter().enumerate() {
        copy1[8 + i] = (8, c);
    }

    // Copy 2: along bottom-left (column 8) and top-right (row 8)
    // Bits 0-6: column 8, rows (size-1) down to (size-7)
    for (i, item) in copy2.iter_mut().enumerate().take(7) {
        *item = (size - 1 - i, 8);
    }
    // Bits 7-14: row 8, columns (size-8) to (size-1)
    for i in 0..8 {
        copy2[7 + i] = (8, size - 8 + i);
    }

    (copy1, copy2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode_roundtrip() {
        for mask in 0..8u8 {
            let encoded = encode_format_info(mask);
            let (ec, decoded_mask) = decode_format_info(encoded).unwrap();
            assert_eq!(ec, EC_BITS_H, "wrong EC level for mask {mask}");
            assert_eq!(decoded_mask, mask, "wrong mask pattern");
        }
    }

    #[test]
    fn decode_with_errors() {
        for mask in 0..8u8 {
            let encoded = encode_format_info(mask);

            // Flip 1 bit
            let corrupted = encoded ^ 0x0001;
            let (_, decoded_mask) = decode_format_info(corrupted).unwrap();
            assert_eq!(decoded_mask, mask);

            // Flip 2 bits
            let corrupted = encoded ^ 0x0003;
            let (_, decoded_mask) = decode_format_info(corrupted).unwrap();
            assert_eq!(decoded_mask, mask);

            // Flip 3 bits
            let corrupted = encoded ^ 0x0007;
            let (_, decoded_mask) = decode_format_info(corrupted).unwrap();
            assert_eq!(decoded_mask, mask);
        }
    }

    #[test]
    fn format_info_positions_valid() {
        let (copy1, copy2) = format_info_positions(21); // Version 1
        // All positions should be within bounds
        for &(r, c) in &copy1 {
            assert!(r < 21 && c < 21, "copy1 out of bounds: ({r}, {c})");
        }
        for &(r, c) in &copy2 {
            assert!(r < 21 && c < 21, "copy2 out of bounds: ({r}, {c})");
        }
    }

    #[test]
    fn format_info_no_duplicate_positions() {
        let (copy1, _copy2) = format_info_positions(21);
        // Within each copy, all positions should be unique
        let mut seen = std::collections::HashSet::new();
        for &pos in &copy1 {
            assert!(seen.insert(pos), "duplicate position in copy1: {pos:?}");
        }
    }

    #[test]
    #[should_panic(expected = "mask pattern must be 0-7")]
    fn encode_invalid_mask_panics() {
        encode_format_info(8);
    }
}
