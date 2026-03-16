use crate::error::{Error, Result};
use crate::{Grid, qr};

pub(crate) fn embed_qr_in_frame(
    qr_grid: &Grid<u8>,
    frame_shape: (usize, usize),
) -> Result<Grid<u8>> {
    if qr_grid.width() > frame_shape.0 || qr_grid.height() > frame_shape.1 {
        return Err(Error::Codec(format!(
            "frame shape {:?} is smaller than QR size {}x{}",
            frame_shape,
            qr_grid.width(),
            qr_grid.height()
        )));
    }

    let mut frame = Grid::filled(frame_shape.0, frame_shape.1, 0u8);
    let row_offset = (frame_shape.1 - qr_grid.height()) / 2;
    let col_offset = (frame_shape.0 - qr_grid.width()) / 2;

    for row in 0..qr_grid.height() {
        for col in 0..qr_grid.width() {
            frame[(row + row_offset, col + col_offset)] = qr_grid[(row, col)];
        }
    }

    Ok(frame)
}

pub(crate) fn centered_qr_crop(grid: &Grid<u8>, size: usize) -> Result<Grid<u8>> {
    if size > grid.width() || size > grid.height() {
        return Err(Error::Codec(format!(
            "cannot crop {}x{} QR from {}x{} grid",
            size,
            size,
            grid.width(),
            grid.height()
        )));
    }

    let row_offset = (grid.height() - size) / 2;
    let col_offset = (grid.width() - size) / 2;
    let mut data = Vec::with_capacity(size * size);
    for row in 0..size {
        for col in 0..size {
            data.push(grid[(row + row_offset, col + col_offset)]);
        }
    }
    Ok(Grid::from_vec(data, size, size))
}

pub(crate) fn extract_qr_from_sign_grid(sign_grid: &Grid<u8>) -> Option<Grid<u8>> {
    for size in [21usize, 25, 29, 33, 37, 41] {
        if size > sign_grid.width() || size > sign_grid.height() {
            continue;
        }
        let candidate = centered_qr_crop(sign_grid, size).ok()?;
        if qr::decode::decode(&candidate).is_ok() {
            return Some(candidate);
        }
    }
    None
}

pub(crate) fn qr_signs_in_frame(qr_in_frame: &Grid<u8>) -> Grid<i8> {
    qr_in_frame.map(|&module| if module == 0 { 1i8 } else { -1i8 })
}

pub(crate) fn validate_matching_frames<T>(
    frames: &[Grid<T>],
    empty_message: &'static str,
) -> Result<()> {
    let Some(first) = frames.first() else {
        return Err(Error::Codec(empty_message.into()));
    };

    for frame in frames.iter().skip(1) {
        if frame.width() != first.width() || frame.height() != first.height() {
            return Err(Error::GridMismatch {
                expected: first.len(),
                actual: frame.len(),
            });
        }
    }

    Ok(())
}
