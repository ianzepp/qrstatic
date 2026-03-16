//! QR code decoder for programmatically generated grids.
//!
//! This decoder is simplified: it only handles grids we produced ourselves.
//! No image processing, no perspective correction, no finder pattern detection.
//! The input is a `Grid<u8>` with known dimensions.

use crate::error::{Error, Result};
use crate::grid::Grid;

use super::encode;
use super::format;
use super::mask;
use super::reed_solomon;

/// Decode a QR code from a grid of modules (0=white, 1=black).
///
/// The grid must be a valid QR code size (21, 25, 29, 33, 37, or 41).
/// Handles up to 30% module errors via EC level H Reed-Solomon correction.
pub fn decode(grid: &Grid<u8>) -> Result<String> {
    let size = grid.width();
    if grid.height() != size {
        return Err(Error::QrDecode("grid must be square".into()));
    }

    let version = encode::version_for_size(size)
        .ok_or_else(|| Error::QrDecode(format!("unsupported grid size {size}")))?;

    // Step 1: Read format info to determine mask pattern
    let mask_pattern = read_format_info(grid, size)?;

    // Step 2: Build function pattern map
    let (_, is_fn) = super::encode::place_function_patterns_for_version(version.version);

    // Step 3: Read data bits (reverse the zigzag placement)
    let raw_bits = read_data_bits(grid, &is_fn, size);

    // Step 4: Unmask
    let unmasked_bits = unmask_data_bits(&raw_bits, &is_fn, size, mask_pattern);

    // Step 5: Reassemble into codewords
    let codewords = bits_to_codewords(&unmasked_bits);

    // Step 6: De-interleave
    let (data_blocks, ec_blocks) = deinterleave(version, &codewords);

    // Step 7: RS decode each block
    let mut decoded_data = Vec::new();
    for (data, ec) in data_blocks.iter().zip(ec_blocks.iter()) {
        let mut block = data.clone();
        block.extend_from_slice(ec);
        match reed_solomon::decode(&block, ec.len()) {
            Some(corrected) => decoded_data.push(corrected),
            None => return Err(Error::QrDecode("Reed-Solomon correction failed".into())),
        }
    }

    // Step 8: Un-interleave data blocks back to original order
    // (For single-group versions, blocks are already in order)
    let all_data: Vec<u8> = decoded_data.into_iter().flatten().collect();

    // Step 9: Parse byte-mode data
    parse_byte_mode(&all_data, version.total_data_codewords())
}

/// Read format info from the grid (try both copies).
fn read_format_info(grid: &Grid<u8>, size: usize) -> Result<u8> {
    let (copy1_pos, copy2_pos) = format::format_info_positions(size);

    // Read copy 1
    let mut raw1 = 0u16;
    for (i, &(r, c)) in copy1_pos.iter().enumerate() {
        if grid[(r, c)] == 1 {
            raw1 |= 1 << i;
        }
    }

    // Read copy 2
    let mut raw2 = 0u16;
    for (i, &(r, c)) in copy2_pos.iter().enumerate() {
        if grid[(r, c)] == 1 {
            raw2 |= 1 << i;
        }
    }

    // Try copy 1 first
    if let Some((_, mask)) = format::decode_format_info(raw1) {
        return Ok(mask);
    }
    // Try copy 2
    if let Some((_, mask)) = format::decode_format_info(raw2) {
        return Ok(mask);
    }

    Err(Error::QrDecode("could not decode format info".into()))
}

/// Read raw data bits from the grid in zigzag order.
/// Returns (row, col, bit_value) for each data module.
fn read_data_bits(grid: &Grid<u8>, is_fn: &Grid<bool>, size: usize) -> Vec<(usize, usize, u8)> {
    let mut bits = Vec::new();
    let mut col = size as i32 - 1;

    while col >= 0 {
        if col == 6 {
            col -= 1;
            continue;
        }

        let going_up = ((size as i32 - 1 - col) / 2) % 2 == 0;

        for step in 0..size {
            let row = if going_up { size - 1 - step } else { step };

            for &dc in &[0i32, -1i32] {
                let c = (col + dc) as usize;
                if c < size && !is_fn[(row, c)] {
                    bits.push((row, c, grid[(row, c)]));
                }
            }
        }

        col -= 2;
    }

    bits
}

/// Remove mask from data bits.
fn unmask_data_bits(
    bits: &[(usize, usize, u8)],
    _is_fn: &Grid<bool>,
    _size: usize,
    mask_pattern: u8,
) -> Vec<u8> {
    bits.iter()
        .map(|&(row, col, val)| {
            if mask::mask_bit(mask_pattern, row, col) {
                val ^ 1
            } else {
                val
            }
        })
        .collect()
}

/// Convert a bit stream to codeword bytes.
fn bits_to_codewords(bits: &[u8]) -> Vec<u8> {
    bits.chunks(8)
        .filter(|chunk| chunk.len() == 8)
        .map(|chunk| {
            let mut byte = 0u8;
            for (i, &bit) in chunk.iter().enumerate() {
                byte |= (bit & 1) << (7 - i);
            }
            byte
        })
        .collect()
}

/// De-interleave codewords into data and EC blocks.
fn deinterleave(
    version: &super::encode::VersionInfo,
    codewords: &[u8],
) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
    let total_blocks = version.total_blocks();
    let mut data_blocks: Vec<Vec<u8>> = vec![Vec::new(); total_blocks];
    let mut ec_blocks: Vec<Vec<u8>> = vec![Vec::new(); total_blocks];

    let max_data = version.data_per_block_g1.max(version.data_per_block_g2);

    // De-interleave data codewords
    let mut idx = 0;
    for i in 0..max_data {
        for (block_idx, block) in data_blocks.iter_mut().enumerate().take(total_blocks) {
            let block_size = if block_idx < version.blocks_g1 {
                version.data_per_block_g1
            } else {
                version.data_per_block_g2
            };
            if i < block_size && idx < codewords.len() {
                block.push(codewords[idx]);
                idx += 1;
            }
        }
    }

    // De-interleave EC codewords
    for _ in 0..version.ec_per_block {
        for ec_block in ec_blocks.iter_mut().take(total_blocks) {
            if idx < codewords.len() {
                ec_block.push(codewords[idx]);
                idx += 1;
            }
        }
    }

    (data_blocks, ec_blocks)
}

/// Parse byte-mode encoded data.
fn parse_byte_mode(data: &[u8], capacity: usize) -> Result<String> {
    if data.is_empty() {
        return Err(Error::QrDecode("empty data".into()));
    }

    // Read mode indicator from first nibble
    let mode = (data[0] >> 4) & 0x0F;
    if mode != 0x04 {
        return Err(Error::QrDecode(format!("unsupported mode {mode:#x}")));
    }

    // Character count bit width depends on version (capacity)
    let count_bits = encode::count_bits_for_capacity(capacity);
    let header_bits = 4 + count_bits;

    // Read character count
    let count = if count_bits == 4 {
        (data[0] & 0x0F) as usize
    } else {
        // 8-bit count: bits 4..12
        (((data[0] & 0x0F) as usize) << 4) | ((data[1] >> 4) as usize)
    };

    // Read data bytes
    let mut result = Vec::with_capacity(count);
    for i in 0..count {
        let bit_offset = header_bits + i * 8;
        let byte_idx = bit_offset / 8;
        let bit_pos = bit_offset % 8;

        if byte_idx >= data.len() {
            break;
        }

        let byte = if bit_pos == 0 {
            data[byte_idx]
        } else {
            let hi = data[byte_idx] << bit_pos;
            let lo = if byte_idx + 1 < data.len() {
                data[byte_idx + 1] >> (8 - bit_pos)
            } else {
                0
            };
            hi | lo
        };
        result.push(byte);
    }

    String::from_utf8(result).map_err(|e| Error::QrDecode(format!("invalid UTF-8: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_single_char() {
        let grid = encode::encode("A").unwrap();
        let decoded = decode(&grid).unwrap();
        assert_eq!(decoded, "A");
    }

    #[test]
    fn roundtrip_short_string() {
        let grid = encode::encode("Hello").unwrap();
        let decoded = decode(&grid).unwrap();
        assert_eq!(decoded, "Hello");
    }

    #[test]
    fn roundtrip_longer_string() {
        let grid = encode::encode("Hello, World!").unwrap();
        let decoded = decode(&grid).unwrap();
        assert_eq!(decoded, "Hello, World!");
    }

    #[test]
    fn roundtrip_version_2() {
        let data = "ABCDEFGHIJKLMNO"; // Needs version 2
        let grid = encode::encode(data).unwrap();
        assert_eq!(grid.width(), 25);
        let decoded = decode(&grid).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_url() {
        let data = "https://example.com";
        let grid = encode::encode(data).unwrap();
        let decoded = decode(&grid).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn roundtrip_special_chars() {
        let data = "Test!@#$%";
        let grid = encode::encode(data).unwrap();
        let decoded = decode(&grid).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_with_noise() {
        let data = "hello";
        let grid = encode::encode(data).unwrap();

        // Flip ~5% of data modules (EC-H can handle up to ~30%)
        let mut noisy = grid.clone();
        let size = noisy.width();
        let mut flip_count = 0;
        let target_flips = (size * size) / 20; // 5%

        // Only flip non-finder-pattern modules
        for row in 9..size - 9 {
            for col in 9..size - 9 {
                if flip_count >= target_flips {
                    break;
                }
                noisy[(row, col)] ^= 1;
                flip_count += 1;
            }
        }

        let decoded = decode(&noisy).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn decode_wrong_size_fails() {
        let grid = Grid::<u8>::new(20, 20); // Invalid QR size
        assert!(decode(&grid).is_err());
    }

    #[test]
    fn decode_non_square_fails() {
        let grid = Grid::<u8>::new(21, 25);
        assert!(decode(&grid).is_err());
    }

    #[test]
    fn bits_to_codewords_basic() {
        let bits = vec![1, 0, 1, 0, 0, 0, 1, 1, 1, 1, 0, 0, 0, 0, 1, 1];
        let codewords = bits_to_codewords(&bits);
        assert_eq!(codewords, vec![0b10100011, 0b11000011]);
    }
}
