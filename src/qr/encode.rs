//! QR code encoder: byte mode, EC level H, versions 1-6.
//!
//! Produces a `Grid<u8>` where 1=black, 0=white.

use crate::error::{Error, Result};
use crate::grid::Grid;

use super::format;
use super::mask;
use super::reed_solomon;

/// QR code capacity information for EC level H.
#[derive(Debug, Clone, Copy)]
pub struct VersionInfo {
    pub version: u8,
    pub size: usize,
    pub ec_per_block: usize,
    pub blocks_g1: usize,
    pub data_per_block_g1: usize,
    pub blocks_g2: usize,
    pub data_per_block_g2: usize,
}

impl VersionInfo {
    pub const fn total_data_codewords(&self) -> usize {
        self.blocks_g1 * self.data_per_block_g1 + self.blocks_g2 * self.data_per_block_g2
    }

    pub const fn total_blocks(&self) -> usize {
        self.blocks_g1 + self.blocks_g2
    }
}

/// QR versions 1-6, EC level H (ISO 18004 Table 9).
const VERSIONS: [VersionInfo; 6] = [
    VersionInfo {
        version: 1,
        size: 21,
        ec_per_block: 17,
        blocks_g1: 1,
        data_per_block_g1: 9,
        blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionInfo {
        version: 2,
        size: 25,
        ec_per_block: 28,
        blocks_g1: 1,
        data_per_block_g1: 16,
        blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionInfo {
        version: 3,
        size: 29,
        ec_per_block: 22,
        blocks_g1: 2,
        data_per_block_g1: 13,
        blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionInfo {
        version: 4,
        size: 33,
        ec_per_block: 16,
        blocks_g1: 4,
        data_per_block_g1: 9,
        blocks_g2: 0,
        data_per_block_g2: 0,
    },
    VersionInfo {
        version: 5,
        size: 37,
        ec_per_block: 22,
        blocks_g1: 2,
        data_per_block_g1: 11,
        blocks_g2: 2,
        data_per_block_g2: 12,
    },
    VersionInfo {
        version: 6,
        size: 41,
        ec_per_block: 28,
        blocks_g1: 4,
        data_per_block_g1: 15,
        blocks_g2: 0,
        data_per_block_g2: 0,
    },
];

/// Alignment pattern center positions per version.
const ALIGNMENT_POSITIONS: [&[usize]; 6] = [
    &[],      // v1: no alignment
    &[6, 18], // v2
    &[6, 22], // v3
    &[6, 26], // v4
    &[6, 30], // v5
    &[6, 34], // v6
];

/// Byte mode uses an 8-bit character count indicator for QR versions 1-9.
pub const BYTE_MODE_COUNT_BITS: usize = 8;

fn select_version(data_len: usize) -> Result<&'static VersionInfo> {
    for v in &VERSIONS {
        let capacity = v.total_data_codewords();
        let header_bits = 4 + BYTE_MODE_COUNT_BITS; // mode indicator (4) + character count
        let data_bits = data_len * 8;
        let total_bits = header_bits + data_bits;
        let total_bytes = total_bits.div_ceil(8);
        if total_bytes <= capacity {
            return Ok(v);
        }
    }
    Err(Error::DataTooLarge {
        len: data_len,
        max: VERSIONS[5].total_data_codewords() - 2,
    })
}

fn encode_data_bits(data: &[u8], capacity: usize) -> Vec<u8> {
    let mut bits: Vec<bool> = Vec::new();

    // Mode indicator: 0100 (byte mode)
    bits.extend_from_slice(&[false, true, false, false]);

    // Character count: byte mode uses 8 bits for all supported versions (1-6).
    let count = data.len() as u8;
    for i in (0..BYTE_MODE_COUNT_BITS).rev() {
        bits.push((count >> i) & 1 == 1);
    }

    // Data
    for &byte in data {
        for i in (0..8).rev() {
            bits.push((byte >> i) & 1 == 1);
        }
    }

    // Terminator
    let remaining = capacity * 8 - bits.len();
    let terminator_len = remaining.min(4);
    bits.extend(std::iter::repeat_n(false, terminator_len));

    // Pad to byte boundary
    while !bits.len().is_multiple_of(8) {
        bits.push(false);
    }

    // Convert to bytes
    let mut bytes: Vec<u8> = Vec::with_capacity(capacity);
    for chunk in bits.chunks(8) {
        let mut byte = 0u8;
        for (i, &bit) in chunk.iter().enumerate() {
            if bit {
                byte |= 1 << (7 - i);
            }
        }
        bytes.push(byte);
    }

    // Pad with alternating 0xEC, 0x11
    let pad_bytes = [0xEC, 0x11];
    let mut pad_idx = 0;
    while bytes.len() < capacity {
        bytes.push(pad_bytes[pad_idx % 2]);
        pad_idx += 1;
    }

    bytes
}

fn interleave(version: &VersionInfo, data_bytes: &[u8]) -> Vec<u8> {
    let total_blocks = version.total_blocks();
    let mut blocks_data: Vec<Vec<u8>> = Vec::with_capacity(total_blocks);
    let mut offset = 0;

    for _ in 0..version.blocks_g1 {
        blocks_data.push(data_bytes[offset..offset + version.data_per_block_g1].to_vec());
        offset += version.data_per_block_g1;
    }
    for _ in 0..version.blocks_g2 {
        blocks_data.push(data_bytes[offset..offset + version.data_per_block_g2].to_vec());
        offset += version.data_per_block_g2;
    }

    let mut blocks_ec: Vec<Vec<u8>> = Vec::with_capacity(total_blocks);
    for block in &blocks_data {
        blocks_ec.push(reed_solomon::encode(block, version.ec_per_block));
    }

    let max_data_per_block = version.data_per_block_g1.max(version.data_per_block_g2);
    let mut interleaved = Vec::new();

    for i in 0..max_data_per_block {
        for block in &blocks_data {
            if i < block.len() {
                interleaved.push(block[i]);
            }
        }
    }

    for i in 0..version.ec_per_block {
        for ec_block in &blocks_ec {
            if i < ec_block.len() {
                interleaved.push(ec_block[i]);
            }
        }
    }

    interleaved
}

/// Place function patterns for a given version number.
/// Public so the decoder can reconstruct the function pattern map.
pub fn place_function_patterns_for_version(version_num: u8) -> (Grid<u8>, Grid<bool>) {
    let version = &VERSIONS[version_num as usize - 1];
    place_function_patterns(version)
}

fn place_function_patterns(version: &VersionInfo) -> (Grid<u8>, Grid<bool>) {
    let size = version.size;
    let mut grid = Grid::<u8>::new(size, size);
    let mut is_fn = Grid::<bool>::new(size, size);

    place_finder(&mut grid, &mut is_fn, 0, 0);
    place_finder(&mut grid, &mut is_fn, 0, size - 7);
    place_finder(&mut grid, &mut is_fn, size - 7, 0);
    place_separator(&mut grid, &mut is_fn, size);
    place_timing(&mut grid, &mut is_fn, size);

    // Dark module
    grid[(4 * version.version as usize + 9, 8)] = 1;
    is_fn[(4 * version.version as usize + 9, 8)] = true;

    // Alignment patterns (version 2+)
    let positions = ALIGNMENT_POSITIONS[version.version as usize - 1];
    if positions.len() >= 2 {
        for &r in positions {
            for &c in positions {
                if is_fn.get(r, c).copied().unwrap_or(false) {
                    continue;
                }
                place_alignment(&mut grid, &mut is_fn, r, c);
            }
        }
    }

    // Reserve format info positions
    let (copy1, copy2) = format::format_info_positions(size);
    for &(r, c) in &copy1 {
        is_fn[(r, c)] = true;
    }
    for &(r, c) in &copy2 {
        is_fn[(r, c)] = true;
    }

    (grid, is_fn)
}

fn place_finder(grid: &mut Grid<u8>, is_fn: &mut Grid<bool>, row: usize, col: usize) {
    for r in 0..7 {
        for c in 0..7 {
            let is_border = r == 0 || r == 6 || c == 0 || c == 6;
            let is_inner = (2..=4).contains(&r) && (2..=4).contains(&c);
            grid[(row + r, col + c)] = u8::from(is_border || is_inner);
            is_fn[(row + r, col + c)] = true;
        }
    }
}

fn place_separator(grid: &mut Grid<u8>, is_fn: &mut Grid<bool>, size: usize) {
    for i in 0..8 {
        if i < size {
            grid[(7, i)] = 0;
            is_fn[(7, i)] = true;
            grid[(i, 7)] = 0;
            is_fn[(i, 7)] = true;
        }
    }
    for i in 0..8 {
        if size >= 8 {
            grid[(7, size - 8 + i)] = 0;
            is_fn[(7, size - 8 + i)] = true;
            grid[(i, size - 8)] = 0;
            is_fn[(i, size - 8)] = true;
        }
    }
    for i in 0..8 {
        if size >= 8 {
            grid[(size - 8, i)] = 0;
            is_fn[(size - 8, i)] = true;
            grid[(size - 8 + i, 7)] = 0;
            is_fn[(size - 8 + i, 7)] = true;
        }
    }
}

fn place_timing(grid: &mut Grid<u8>, is_fn: &mut Grid<bool>, size: usize) {
    for i in 8..size - 8 {
        let val = if i % 2 == 0 { 1 } else { 0 };
        if !is_fn[(6, i)] {
            grid[(6, i)] = val;
            is_fn[(6, i)] = true;
        }
        if !is_fn[(i, 6)] {
            grid[(i, 6)] = val;
            is_fn[(i, 6)] = true;
        }
    }
}

fn place_alignment(grid: &mut Grid<u8>, is_fn: &mut Grid<bool>, center_r: usize, center_c: usize) {
    for dr in 0..5 {
        for dc in 0..5 {
            let r = center_r - 2 + dr;
            let c = center_c - 2 + dc;
            let is_border = dr == 0 || dr == 4 || dc == 0 || dc == 4;
            let is_center = dr == 2 && dc == 2;
            grid[(r, c)] = u8::from(is_border || is_center);
            is_fn[(r, c)] = true;
        }
    }
}

fn place_data(grid: &mut Grid<u8>, is_fn: &Grid<bool>, data: &[u8]) {
    let size = grid.width();
    let mut bit_idx = 0;
    let total_bits = data.len() * 8;

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
                if c < size && !is_fn[(row, c)] && bit_idx < total_bits {
                    let byte_idx = bit_idx / 8;
                    let bit_pos = 7 - (bit_idx % 8);
                    grid[(row, c)] = (data[byte_idx] >> bit_pos) & 1;
                    bit_idx += 1;
                }
            }
        }

        col -= 2;
    }
}

fn place_format_info(grid: &mut Grid<u8>, version: &VersionInfo, mask_pattern: u8) {
    let format_bits = format::encode_format_info(mask_pattern);
    let (copy1, copy2) = format::format_info_positions(version.size);

    for (i, &(r, c)) in copy1.iter().enumerate() {
        grid[(r, c)] = ((format_bits >> i) & 1) as u8;
    }
    for (i, &(r, c)) in copy2.iter().enumerate() {
        grid[(r, c)] = ((format_bits >> i) & 1) as u8;
    }
}

/// Encode a string into a QR code grid.
///
/// Returns a `Grid<u8>` where 1=black module, 0=white module.
/// Uses byte mode encoding, EC level H, auto-selects smallest version (1-6).
pub fn encode(data: &str) -> Result<Grid<u8>> {
    let data_bytes = data.as_bytes();
    let version = select_version(data_bytes.len())?;

    let encoded = encode_data_bits(data_bytes, version.total_data_codewords());
    let interleaved = interleave(version, &encoded);
    let (mut grid, is_fn) = place_function_patterns(version);

    place_data(&mut grid, &is_fn, &interleaved);

    let best_mask = mask::best_mask(&grid, &is_fn);
    mask::apply_mask(&mut grid, &is_fn, best_mask);
    place_format_info(&mut grid, version, best_mask);

    Ok(grid)
}

/// Get the version info for a grid of the given size.
pub fn version_for_size(size: usize) -> Option<&'static VersionInfo> {
    VERSIONS.iter().find(|v| v.size == size)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn select_version_small() {
        let v = select_version(1).unwrap();
        assert_eq!(v.version, 1);
    }

    #[test]
    fn select_version_boundaries_match_qr_h_capacity() {
        assert_eq!(select_version(7).unwrap().version, 1);
        assert_eq!(select_version(8).unwrap().version, 2);
        assert_eq!(select_version(14).unwrap().version, 2);
        assert_eq!(select_version(15).unwrap().version, 3);
        assert_eq!(select_version(24).unwrap().version, 3);
        assert_eq!(select_version(25).unwrap().version, 4);
        assert_eq!(select_version(34).unwrap().version, 4);
        assert_eq!(select_version(35).unwrap().version, 5);
        assert_eq!(select_version(44).unwrap().version, 5);
        assert_eq!(select_version(45).unwrap().version, 6);
        assert_eq!(select_version(58).unwrap().version, 6);
    }

    #[test]
    fn select_version_too_large() {
        assert!(select_version(59).is_err());
    }

    #[test]
    fn encode_data_bits_fills_capacity() {
        let bytes = encode_data_bits(b"Hi", 9);
        assert_eq!(bytes.len(), 9);
    }

    #[test]
    fn encode_data_bits_starts_with_mode() {
        let bytes = encode_data_bits(b"A", 9);
        assert_eq!(bytes[0], 0x40);
        assert_eq!(bytes[1], 0x14);
    }

    #[test]
    fn encode_produces_correct_size() {
        let grid = encode("A").unwrap();
        assert_eq!(grid.width(), 21);
        assert_eq!(grid.height(), 21);
    }

    #[test]
    fn encode_produces_binary_grid() {
        let grid = encode("Test").unwrap();
        assert!(grid.data().iter().all(|&v| v == 0 || v == 1));
    }

    #[test]
    fn encode_deterministic() {
        let a = encode("hello").unwrap();
        let b = encode("hello").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn encode_different_data_differs() {
        let a = encode("aaa").unwrap();
        let b = encode("bbb").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn finder_patterns_present() {
        let grid = encode("X").unwrap();
        let size = grid.width();
        assert_eq!(grid[(0, 0)], 1);
        assert_eq!(grid[(0, 6)], 1);
        assert_eq!(grid[(6, 0)], 1);
        assert_eq!(grid[(6, 6)], 1);
        assert_eq!(grid[(0, size - 1)], 1);
        assert_eq!(grid[(0, size - 7)], 1);
        assert_eq!(grid[(size - 1, 0)], 1);
        assert_eq!(grid[(size - 7, 0)], 1);
    }

    #[test]
    fn version_2_has_alignment() {
        let grid = encode("ABCDEFGHIJKLMN").unwrap();
        assert_eq!(grid.width(), 25);
    }

    #[test]
    fn version_6_max_payload_roundtrip_size() {
        let data = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz012345";
        assert_eq!(data.len(), 58);
        let grid = encode(data).unwrap();
        assert_eq!(grid.width(), 41);
    }

    #[test]
    fn interleave_single_block() {
        let version = &VERSIONS[0];
        let data = encode_data_bits(b"Hi", version.total_data_codewords());
        let interleaved = interleave(version, &data);
        assert_eq!(
            interleaved.len(),
            version.total_data_codewords() + version.ec_per_block
        );
    }
}
