use std::ops::{Index, IndexMut};

/// A 2D grid stored as a flat `Vec<T>` in row-major order.
///
/// This is the primary data structure for QR modules, noise frames,
/// and accumulated signal buffers. No external dependencies.
#[derive(Debug, Clone, PartialEq)]
pub struct Grid<T> {
    data: Vec<T>,
    width: usize,
    height: usize,
}

impl<T: Clone + Default> Grid<T> {
    /// Create a grid filled with `T::default()`.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            data: vec![T::default(); width * height],
            width,
            height,
        }
    }

    /// Create a square grid filled with `T::default()`.
    pub fn square(size: usize) -> Self {
        Self::new(size, size)
    }
}

impl<T: Clone> Grid<T> {
    /// Create a grid filled with the given value.
    pub fn filled(width: usize, height: usize, value: T) -> Self {
        Self {
            data: vec![value; width * height],
            width,
            height,
        }
    }

    /// Create a grid from a flat vec. Panics if `data.len() != width * height`.
    pub fn from_vec(data: Vec<T>, width: usize, height: usize) -> Self {
        assert_eq!(
            data.len(),
            width * height,
            "data length {} does not match {}x{}",
            data.len(),
            width,
            height
        );
        Self {
            data,
            width,
            height,
        }
    }

    /// Create a grid from row-major nested slices.
    pub fn from_rows(rows: &[&[T]]) -> Self {
        let height = rows.len();
        assert!(height > 0, "must have at least one row");
        let width = rows[0].len();
        assert!(
            rows.iter().all(|r| r.len() == width),
            "all rows must have equal length"
        );
        let data: Vec<T> = rows.iter().flat_map(|r| r.iter().cloned()).collect();
        Self {
            data,
            width,
            height,
        }
    }
}

impl<T> Grid<T> {
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn height(&self) -> usize {
        self.height
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Flat data in row-major order.
    pub fn data(&self) -> &[T] {
        &self.data
    }

    /// Mutable flat data in row-major order.
    pub fn data_mut(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Consume the grid and return the underlying vec.
    pub fn into_vec(self) -> Vec<T> {
        self.data
    }

    /// Get a reference to the element at (row, col). Returns `None` if out of bounds.
    pub fn get(&self, row: usize, col: usize) -> Option<&T> {
        if row < self.height && col < self.width {
            Some(&self.data[row * self.width + col])
        } else {
            None
        }
    }

    /// Get a mutable reference to the element at (row, col). Returns `None` if out of bounds.
    pub fn get_mut(&mut self, row: usize, col: usize) -> Option<&mut T> {
        if row < self.height && col < self.width {
            Some(&mut self.data[row * self.width + col])
        } else {
            None
        }
    }

    /// Apply a function to every element, producing a new grid.
    pub fn map<U>(&self, f: impl Fn(&T) -> U) -> Grid<U> {
        Grid {
            data: self.data.iter().map(f).collect(),
            width: self.width,
            height: self.height,
        }
    }

    /// Combine two grids element-wise. Panics if dimensions differ.
    pub fn zip_with<U, V>(&self, other: &Grid<U>, f: impl Fn(&T, &U) -> V) -> Grid<V> {
        assert_eq!(self.width, other.width, "width mismatch in zip_with");
        assert_eq!(self.height, other.height, "height mismatch in zip_with");
        Grid {
            data: self
                .data
                .iter()
                .zip(other.data.iter())
                .map(|(a, b)| f(a, b))
                .collect(),
            width: self.width,
            height: self.height,
        }
    }

    /// Iterate over all elements with their (row, col) coordinates.
    pub fn iter_coords(&self) -> impl Iterator<Item = (usize, usize, &T)> {
        self.data
            .iter()
            .enumerate()
            .map(move |(i, v)| (i / self.width, i % self.width, v))
    }

    /// Get a row as a slice.
    pub fn row(&self, r: usize) -> &[T] {
        let start = r * self.width;
        &self.data[start..start + self.width]
    }
}

impl<T> Index<(usize, usize)> for Grid<T> {
    type Output = T;

    fn index(&self, (row, col): (usize, usize)) -> &T {
        &self.data[row * self.width + col]
    }
}

impl<T> IndexMut<(usize, usize)> for Grid<T> {
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut T {
        &mut self.data[row * self.width + col]
    }
}

/// Accumulate a sequence of grids by summing element-wise into a target type.
///
/// This is the core operation for all analog/signed/binary codecs:
/// sum many noise frames to reveal the hidden signal.
pub fn accumulate_i16(frames: &[Grid<i8>]) -> Grid<i16> {
    assert!(!frames.is_empty(), "cannot accumulate zero frames");
    let width = frames[0].width();
    let height = frames[0].height();
    let mut acc = Grid::<i16>::new(width, height);
    for frame in frames {
        assert_eq!(frame.width(), width, "frame width mismatch");
        assert_eq!(frame.height(), height, "frame height mismatch");
        for (a, b) in acc.data_mut().iter_mut().zip(frame.data().iter()) {
            *a += *b as i16;
        }
    }
    acc
}

/// Accumulate float32 grids by element-wise summation.
pub fn accumulate_f32(frames: &[Grid<f32>]) -> Grid<f32> {
    assert!(!frames.is_empty(), "cannot accumulate zero frames");
    let width = frames[0].width();
    let height = frames[0].height();
    let mut acc = Grid::<f32>::new(width, height);
    for frame in frames {
        assert_eq!(frame.width(), width, "frame width mismatch");
        assert_eq!(frame.height(), height, "frame height mismatch");
        for (a, b) in acc.data_mut().iter_mut().zip(frame.data().iter()) {
            *a += *b;
        }
    }
    acc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grid_is_zeroed() {
        let g: Grid<u8> = Grid::new(3, 2);
        assert_eq!(g.width(), 3);
        assert_eq!(g.height(), 2);
        assert_eq!(g.len(), 6);
        assert!(g.data().iter().all(|&v| v == 0));
    }

    #[test]
    fn square_grid() {
        let g: Grid<i32> = Grid::square(4);
        assert_eq!(g.width(), 4);
        assert_eq!(g.height(), 4);
        assert_eq!(g.len(), 16);
    }

    #[test]
    fn filled_grid() {
        let g = Grid::filled(2, 3, 42u8);
        assert!(g.data().iter().all(|&v| v == 42));
    }

    #[test]
    fn from_vec_and_indexing() {
        let g = Grid::from_vec(vec![1, 2, 3, 4, 5, 6], 3, 2);
        assert_eq!(g[(0, 0)], 1);
        assert_eq!(g[(0, 2)], 3);
        assert_eq!(g[(1, 0)], 4);
        assert_eq!(g[(1, 2)], 6);
    }

    #[test]
    fn from_rows() {
        let g = Grid::from_rows(&[&[1, 2, 3], &[4, 5, 6]]);
        assert_eq!(g[(0, 0)], 1);
        assert_eq!(g[(1, 2)], 6);
        assert_eq!(g.width(), 3);
        assert_eq!(g.height(), 2);
    }

    #[test]
    #[should_panic(expected = "data length")]
    fn from_vec_wrong_size_panics() {
        let _ = Grid::from_vec(vec![1, 2, 3], 2, 2);
    }

    #[test]
    fn get_in_bounds() {
        let g = Grid::from_vec(vec![10, 20, 30, 40], 2, 2);
        assert_eq!(g.get(0, 1), Some(&20));
        assert_eq!(g.get(1, 0), Some(&30));
    }

    #[test]
    fn get_out_of_bounds() {
        let g: Grid<u8> = Grid::new(2, 2);
        assert_eq!(g.get(2, 0), None);
        assert_eq!(g.get(0, 2), None);
    }

    #[test]
    fn get_mut_works() {
        let mut g = Grid::from_vec(vec![1, 2, 3, 4], 2, 2);
        *g.get_mut(0, 1).unwrap() = 99;
        assert_eq!(g[(0, 1)], 99);
    }

    #[test]
    fn index_mut_works() {
        let mut g = Grid::from_vec(vec![1, 2, 3, 4], 2, 2);
        g[(1, 1)] = 77;
        assert_eq!(g[(1, 1)], 77);
    }

    #[test]
    fn map_doubles() {
        let g = Grid::from_vec(vec![1, 2, 3, 4], 2, 2);
        let doubled = g.map(|v| v * 2);
        assert_eq!(doubled.data(), &[2, 4, 6, 8]);
    }

    #[test]
    fn zip_with_add() {
        let a = Grid::from_vec(vec![1, 2, 3, 4], 2, 2);
        let b = Grid::from_vec(vec![10, 20, 30, 40], 2, 2);
        let sum = a.zip_with(&b, |x, y| x + y);
        assert_eq!(sum.data(), &[11, 22, 33, 44]);
    }

    #[test]
    #[should_panic(expected = "width mismatch")]
    fn zip_with_dimension_mismatch_panics() {
        let a: Grid<u8> = Grid::new(2, 2);
        let b: Grid<u8> = Grid::new(3, 2);
        let _ = a.zip_with(&b, |x, y| x + y);
    }

    #[test]
    fn iter_coords() {
        let g = Grid::from_vec(vec![10, 20, 30, 40, 50, 60], 3, 2);
        let coords: Vec<_> = g.iter_coords().collect();
        assert_eq!(coords[0], (0, 0, &10));
        assert_eq!(coords[2], (0, 2, &30));
        assert_eq!(coords[3], (1, 0, &40));
        assert_eq!(coords[5], (1, 2, &60));
    }

    #[test]
    fn row_slice() {
        let g = Grid::from_vec(vec![1, 2, 3, 4, 5, 6], 3, 2);
        assert_eq!(g.row(0), &[1, 2, 3]);
        assert_eq!(g.row(1), &[4, 5, 6]);
    }

    #[test]
    fn one_by_one_grid() {
        let g = Grid::from_vec(vec![42], 1, 1);
        assert_eq!(g[(0, 0)], 42);
        assert_eq!(g.width(), 1);
        assert_eq!(g.height(), 1);
    }

    #[test]
    fn accumulate_i16_basic() {
        let f1 = Grid::from_vec(vec![1i8, -1, 1, -1], 2, 2);
        let f2 = Grid::from_vec(vec![1i8, -1, -1, 1], 2, 2);
        let f3 = Grid::from_vec(vec![1i8, -1, 1, -1], 2, 2);
        let acc = accumulate_i16(&[f1, f2, f3]);
        assert_eq!(acc.data(), &[3i16, -3, 1, -1]);
    }

    #[test]
    fn accumulate_f32_basic() {
        let f1 = Grid::from_vec(vec![0.5f32, -0.5], 2, 1);
        let f2 = Grid::from_vec(vec![0.5f32, -0.5], 2, 1);
        let acc = accumulate_f32(&[f1, f2]);
        assert!((acc[(0, 0)] - 1.0).abs() < 1e-6);
        assert!((acc[(0, 1)] + 1.0).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "cannot accumulate zero frames")]
    fn accumulate_empty_panics() {
        let _: Grid<i16> = accumulate_i16(&[]);
    }

    #[test]
    fn into_vec_consumes() {
        let g = Grid::from_vec(vec![1, 2, 3, 4], 2, 2);
        let v = g.into_vec();
        assert_eq!(v, vec![1, 2, 3, 4]);
    }
}
