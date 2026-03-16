//! QR code mask patterns and penalty evaluation.
//!
//! The QR spec defines 8 mask patterns. After data placement, each pattern
//! is applied and scored; the lowest-penalty pattern is chosen.

use crate::grid::Grid;

/// Returns true if the module at (row, col) should be flipped for the given mask pattern.
pub fn mask_bit(pattern: u8, row: usize, col: usize) -> bool {
    match pattern {
        0 => (row + col).is_multiple_of(2),
        1 => row.is_multiple_of(2),
        2 => col.is_multiple_of(3),
        3 => (row + col).is_multiple_of(3),
        4 => (row / 2 + col / 3).is_multiple_of(2),
        5 => (row * col) % 2 + (row * col) % 3 == 0,
        6 => ((row * col) % 2 + (row * col) % 3).is_multiple_of(2),
        7 => ((row + col) % 2 + (row * col) % 3).is_multiple_of(2),
        _ => panic!("invalid mask pattern {pattern}"),
    }
}

/// Apply a mask pattern to a grid, flipping data modules only.
/// `is_function` indicates which modules are function patterns (not masked).
pub fn apply_mask(data: &mut Grid<u8>, is_function: &Grid<bool>, pattern: u8) {
    let w = data.width();
    let h = data.height();
    for row in 0..h {
        for col in 0..w {
            if !is_function[(row, col)] && mask_bit(pattern, row, col) {
                data[(row, col)] ^= 1;
            }
        }
    }
}

/// Evaluate the total penalty score for a QR grid.
/// Lower is better. Sum of all 4 penalty rules.
pub fn evaluate_penalty(grid: &Grid<u8>) -> u32 {
    penalty_rule_1(grid) + penalty_rule_2(grid) + penalty_rule_3(grid) + penalty_rule_4(grid)
}

/// Rule 1: Runs of same-colored modules in rows and columns.
/// Penalty = (run_length - 2) for each run of 5+ same-colored modules.
fn penalty_rule_1(grid: &Grid<u8>) -> u32 {
    let size = grid.width();
    let mut penalty = 0u32;

    // Rows
    for row in 0..size {
        let mut run = 1u32;
        for col in 1..size {
            if grid[(row, col)] == grid[(row, col - 1)] {
                run += 1;
            } else {
                if run >= 5 {
                    penalty += run - 2;
                }
                run = 1;
            }
        }
        if run >= 5 {
            penalty += run - 2;
        }
    }

    // Columns
    for col in 0..size {
        let mut run = 1u32;
        for row in 1..size {
            if grid[(row, col)] == grid[(row - 1, col)] {
                run += 1;
            } else {
                if run >= 5 {
                    penalty += run - 2;
                }
                run = 1;
            }
        }
        if run >= 5 {
            penalty += run - 2;
        }
    }

    penalty
}

/// Rule 2: 2×2 blocks of same-colored modules.
/// Penalty = 3 for each 2×2 block.
fn penalty_rule_2(grid: &Grid<u8>) -> u32 {
    let size = grid.width();
    let mut penalty = 0u32;
    for row in 0..size - 1 {
        for col in 0..size - 1 {
            let val = grid[(row, col)];
            if val == grid[(row, col + 1)]
                && val == grid[(row + 1, col)]
                && val == grid[(row + 1, col + 1)]
            {
                penalty += 3;
            }
        }
    }
    penalty
}

/// Rule 3: Finder-like patterns (1011101 preceded/followed by 4 white modules).
/// Penalty = 40 for each occurrence.
fn penalty_rule_3(grid: &Grid<u8>) -> u32 {
    let size = grid.width();
    let mut penalty = 0u32;
    let pattern_a: [u8; 7] = [1, 0, 1, 1, 1, 0, 1];

    for row in 0..size {
        for col in 0..=size.saturating_sub(11) {
            // Check horizontal: 0000 + pattern or pattern + 0000
            let matches_pattern = (0..7).all(|i| grid[(row, col + i)] == pattern_a[i]);
            if col + 11 <= size && matches_pattern {
                let four_white_after = (7..11).all(|i| grid[(row, col + i)] == 0);
                if four_white_after {
                    penalty += 40;
                }
            }
            if col >= 4 && matches_pattern {
                let four_white_before = (1..=4).all(|i| grid[(row, col - i)] == 0);
                if four_white_before {
                    penalty += 40;
                }
            }
        }
    }

    for col in 0..size {
        for row in 0..=size.saturating_sub(11) {
            let matches_pattern = (0..7).all(|i| grid[(row + i, col)] == pattern_a[i]);
            if row + 11 <= size && matches_pattern {
                let four_white_after = (7..11).all(|i| grid[(row + i, col)] == 0);
                if four_white_after {
                    penalty += 40;
                }
            }
            if row >= 4 && matches_pattern {
                let four_white_before = (1..=4).all(|i| grid[(row - i, col)] == 0);
                if four_white_before {
                    penalty += 40;
                }
            }
        }
    }

    penalty
}

/// Rule 4: Proportion of dark modules.
/// Penalty based on how far the dark percentage is from 50%.
fn penalty_rule_4(grid: &Grid<u8>) -> u32 {
    let total = grid.len() as u32;
    let dark: u32 = grid.data().iter().map(|&v| v as u32).sum();
    let percent = (dark * 100) / total;
    let prev5 = (percent / 5) * 5;
    let next5 = prev5 + 5;
    let a = if prev5 > 50 {
        (prev5 - 50) / 5
    } else {
        (50 - prev5) / 5
    };
    let b = if next5 > 50 {
        (next5 - 50) / 5
    } else {
        (50 - next5) / 5
    };
    a.min(b) * 10
}

/// Select the best mask pattern (lowest penalty) for the given grid and function pattern.
/// Returns the mask pattern index (0-7).
pub fn best_mask(grid: &Grid<u8>, is_function: &Grid<bool>) -> u8 {
    let mut best_pattern = 0u8;
    let mut best_penalty = u32::MAX;

    for pattern in 0..8u8 {
        let mut masked = grid.clone();
        apply_mask(&mut masked, is_function, pattern);
        let penalty = evaluate_penalty(&masked);
        if penalty < best_penalty {
            best_penalty = penalty;
            best_pattern = pattern;
        }
    }

    best_pattern
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_patterns_produce_different_results() {
        let results: Vec<bool> = (0..8).map(|p| mask_bit(p, 3, 5)).collect();
        // Not all patterns should give the same result for the same coordinate
        assert!(results.iter().any(|&r| r) && results.iter().any(|&r| !r));
    }

    #[test]
    fn mask_pattern_0_checkerboard() {
        assert!(mask_bit(0, 0, 0)); // (0+0)%2 == 0
        assert!(!mask_bit(0, 0, 1)); // (0+1)%2 == 1
        assert!(!mask_bit(0, 1, 0)); // (1+0)%2 == 1
        assert!(mask_bit(0, 1, 1)); // (1+1)%2 == 0
    }

    #[test]
    fn mask_pattern_1_horizontal_stripes() {
        assert!(mask_bit(1, 0, 0));
        assert!(mask_bit(1, 0, 5));
        assert!(!mask_bit(1, 1, 0));
        assert!(!mask_bit(1, 1, 5));
        assert!(mask_bit(1, 2, 0));
    }

    #[test]
    fn apply_mask_respects_function_pattern() {
        let mut data = Grid::filled(3, 3, 0u8);
        let mut is_fn = Grid::filled(3, 3, false);
        is_fn[(0, 0)] = true; // Mark as function pattern
        data[(0, 0)] = 1;
        data[(0, 1)] = 1;

        apply_mask(&mut data, &is_fn, 0);

        // (0,0) is function pattern — should NOT be flipped
        assert_eq!(data[(0, 0)], 1);
        // (0,1) is not function — mask_bit(0, 0, 1) = false, so no flip
        // (1,1) is not function — mask_bit(0, 1, 1) = true, so flip 0→1
        assert_eq!(data[(1, 1)], 1);
    }

    #[test]
    fn penalty_rule_2_detects_blocks() {
        let mut grid = Grid::filled(4, 4, 0u8);
        // All white → 9 overlapping 2×2 blocks
        let p = penalty_rule_2(&grid);
        assert!(p > 0);

        // Checkerboard has no 2×2 blocks
        for row in 0..4 {
            for col in 0..4 {
                grid[(row, col)] = ((row + col) % 2) as u8;
            }
        }
        assert_eq!(penalty_rule_2(&grid), 0);
    }

    #[test]
    fn penalty_rule_4_balanced() {
        // 50% dark → minimum penalty
        let mut grid = Grid::filled(10, 10, 0u8);
        for i in 0..50 {
            grid.data_mut()[i] = 1;
        }
        let p = penalty_rule_4(&grid);
        assert_eq!(p, 0);
    }

    #[test]
    fn best_mask_returns_valid_pattern() {
        let grid = Grid::filled(21, 21, 0u8);
        let is_fn = Grid::filled(21, 21, false);
        let pattern = best_mask(&grid, &is_fn);
        assert!(pattern < 8);
    }
}
