use std::collections::VecDeque;

use crate::bits::{bits_to_bytes, bytes_to_bits, spread_bits};
use crate::codec::{EncodeConfig, Frame};
use crate::error::{Error, Result};
use crate::grid::accumulate_i16;
use crate::{Grid, Prng, qr};

/// Shared decode output for the signed codec.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedDecodeResult {
    pub qr: Grid<u8>,
    pub message: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// Batch signed codec encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedEncoder {
    config: EncodeConfig,
    frame_shape: (usize, usize),
    signal_strength: i16,
}

impl SignedEncoder {
    pub fn new(
        n_frames: usize,
        frame_shape: (usize, usize),
        seed: impl Into<String>,
        signal_strength: i16,
    ) -> Result<Self> {
        if n_frames < 4 {
            return Err(Error::Codec(
                "signed encoding requires at least 4 frames".into(),
            ));
        }
        if signal_strength < 1 {
            return Err(Error::Codec(
                "signed encoding requires signal_strength >= 1".into(),
            ));
        }
        if frame_shape.0 == 0 || frame_shape.1 == 0 {
            return Err(Error::Codec(
                "signed encoding requires non-empty frames".into(),
            ));
        }

        Ok(Self {
            config: EncodeConfig::new(n_frames, seed),
            frame_shape,
            signal_strength,
        })
    }

    pub fn encode_message(&self, qr_seed: &str, payload: &[u8]) -> Result<Vec<Grid<i8>>> {
        let qr_grid = qr::encode::encode(qr_seed)?;
        self.encode_qr(&qr_grid, qr_seed, payload)
    }

    pub fn encode_qr(
        &self,
        qr_grid: &Grid<u8>,
        qr_seed: &str,
        payload: &[u8],
    ) -> Result<Vec<Grid<i8>>> {
        let qr_in_frame = embed_qr_in_frame(qr_grid, self.frame_shape)?;
        let target_signs = qr_signs_in_frame(&qr_in_frame);
        let payload_target = payload_target_map(self.frame_shape, payload);
        let base_magnitude = normalize_magnitude(self.signal_strength, self.config.n_frames);
        let payload_delta = if payload.is_empty() { 0 } else { 2 };
        let mut frames =
            vec![Grid::<i8>::new(self.frame_shape.0, self.frame_shape.1); self.config.n_frames];

        for row in 0..self.frame_shape.1 {
            for col in 0..self.frame_shape.0 {
                let target_sign = target_signs[(row, col)] as i16;
                let mut desired_magnitude = base_magnitude;
                if payload_delta != 0 {
                    if payload_target[(row, col)] > 0 {
                        desired_magnitude += payload_delta;
                    } else {
                        desired_magnitude -= payload_delta;
                    }
                }
                desired_magnitude = normalize_magnitude(desired_magnitude, self.config.n_frames);
                let desired_sum = target_sign * desired_magnitude;
                assign_signed_samples(
                    &mut frames,
                    row,
                    col,
                    desired_sum,
                    qr_seed,
                    (row * self.frame_shape.0 + col) as u64,
                );
            }
        }
        Ok(frames)
    }
}

/// Batch signed codec decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedDecoder {
    expected_payload_len: usize,
    signal_strength: i16,
}

impl SignedDecoder {
    pub fn new(expected_payload_len: usize, signal_strength: i16) -> Self {
        Self {
            expected_payload_len,
            signal_strength,
        }
    }

    pub fn decode_qr(frames: &[Grid<i8>]) -> Result<Grid<u8>> {
        let accumulated = accumulate_i16(frames);
        extract_qr(&accumulated).ok_or_else(|| {
            Error::Codec("could not extract a valid QR crop from signed frame".into())
        })
    }

    pub fn decode_payload(
        &self,
        accumulated: &Grid<i16>,
        qr_seed: &str,
        n_frames: usize,
        payload_length: usize,
    ) -> Result<Vec<u8>> {
        if payload_length == 0 {
            return Ok(Vec::new());
        }

        let expected = expected_accumulation(
            (accumulated.width(), accumulated.height()),
            qr_seed,
            n_frames,
            self.signal_strength,
        )?;
        let qr_grid = qr::encode::encode(qr_seed)?;
        let qr_in_frame = embed_qr_in_frame(&qr_grid, (accumulated.width(), accumulated.height()))?;
        let signs = qr_signs_in_frame(&qr_in_frame);
        let mapping = spread_bits(accumulated.len(), payload_length * 8);
        let mut votes = vec![Vec::new(); payload_length * 8];

        for (flat_idx, ((&actual, &expected_val), &sign)) in accumulated
            .data()
            .iter()
            .zip(expected.data().iter())
            .zip(signs.data().iter())
            .enumerate()
        {
            let residual = (actual - expected_val) * sign as i16;
            let bit_idx = flat_idx % (payload_length * 8);
            debug_assert!(mapping[bit_idx].contains(&flat_idx));
            votes[bit_idx].push(if residual > 0 { 1 } else { 0 });
        }

        let bits: Vec<u8> = votes
            .into_iter()
            .map(|samples| {
                let ones = samples.iter().copied().sum::<u8>() as usize;
                let zeros = samples.len().saturating_sub(ones);
                u8::from(ones > zeros)
            })
            .collect();

        Ok(bits_to_bytes(&bits)
            .into_iter()
            .take(payload_length)
            .collect())
    }

    pub fn decode_message(&self, frames: &[Grid<i8>]) -> Result<SignedDecodeResult> {
        let accumulated = accumulate_i16(frames);
        let sign_grid = accumulated.map(|&value| u8::from(value <= 0));
        let Some(qr_grid) = extract_qr(&accumulated) else {
            return Ok(SignedDecodeResult {
                qr: sign_grid,
                message: None,
                payload: None,
            });
        };
        let message = qr::decode::decode(&qr_grid).ok();
        let payload = match &message {
            Some(seed) => Some(self.decode_payload(
                &accumulated,
                seed,
                frames.len(),
                self.expected_payload_len,
            )?),
            None => None,
        };

        Ok(SignedDecodeResult {
            qr: qr_grid,
            message,
            payload,
        })
    }
}

/// Continuous signed stream encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedStreamEncoder {
    frame_shape: (usize, usize),
    n_frames: usize,
    signal_strength: i16,
    queue: VecDeque<(String, Vec<u8>)>,
    active_frames: Vec<Grid<i8>>,
    frame_index: usize,
    idle_cycle: u64,
}

impl SignedStreamEncoder {
    pub fn new(n_frames: usize, frame_shape: (usize, usize), signal_strength: i16) -> Result<Self> {
        SignedEncoder::new(n_frames, frame_shape, "signed-stream", signal_strength)?;
        Ok(Self {
            frame_shape,
            n_frames,
            signal_strength,
            queue: VecDeque::new(),
            active_frames: Vec::new(),
            frame_index: 0,
            idle_cycle: 0,
        })
    }

    pub fn queue_message(&mut self, qr_seed: impl Into<String>, payload: impl Into<Vec<u8>>) {
        self.queue.push_back((qr_seed.into(), payload.into()));
    }

    pub fn next_frame(&mut self) -> Result<Grid<i8>> {
        if self.frame_index >= self.active_frames.len() {
            self.start_next_cycle()?;
        }

        let frame = self.active_frames[self.frame_index].clone();
        self.frame_index += 1;
        Ok(frame)
    }

    fn start_next_cycle(&mut self) -> Result<()> {
        self.frame_index = 0;
        self.active_frames = if let Some((seed, payload)) = self.queue.pop_front() {
            SignedEncoder::new(
                self.n_frames,
                self.frame_shape,
                "signed-stream",
                self.signal_strength,
            )?
            .encode_message(&seed, &payload)?
        } else {
            let base = format!("signed-idle:{}", self.idle_cycle);
            (0..self.n_frames)
                .map(|idx| signed_noise_frame(self.frame_shape, &base, idx as u64))
                .collect()
        };
        self.idle_cycle += 1;
        Ok(())
    }
}

/// Continuous signed stream decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct SignedStreamDecoder {
    n_frames: usize,
    decoder: SignedDecoder,
    buffer: Vec<Grid<i8>>,
}

impl SignedStreamDecoder {
    pub fn new(
        n_frames: usize,
        expected_payload_length: usize,
        signal_strength: i16,
    ) -> Result<Self> {
        if n_frames < 4 {
            return Err(Error::Codec(
                "signed decoding requires at least 4 frames".into(),
            ));
        }
        Ok(Self {
            n_frames,
            decoder: SignedDecoder::new(expected_payload_length, signal_strength),
            buffer: Vec::with_capacity(n_frames),
        })
    }

    pub fn push_frame(&mut self, frame: Grid<i8>) -> Result<Option<SignedDecodeResult>> {
        self.buffer.push(frame);
        if self.buffer.len() < self.n_frames {
            return Ok(None);
        }

        let result = self.decoder.decode_message(&self.buffer)?;
        self.buffer.clear();
        Ok(Some(result))
    }
}

fn signed_noise_frame(frame_shape: (usize, usize), seed: &str, index: u64) -> Grid<i8> {
    let mut rng = Prng::from_key(seed, index);
    let data = (0..(frame_shape.0 * frame_shape.1))
        .map(|_| if rng.next_bool(0.5) { 1 } else { -1 })
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn expected_accumulation(
    frame_shape: (usize, usize),
    seed: &str,
    n_frames: usize,
    signal_strength: i16,
) -> Result<Grid<i16>> {
    let qr_grid = qr::encode::encode(seed)?;
    let qr_in_frame = embed_qr_in_frame(&qr_grid, frame_shape)?;
    let signs = qr_signs_in_frame(&qr_in_frame);
    let base_magnitude = normalize_magnitude(signal_strength, n_frames);
    Ok(signs.map(|&sign| sign as i16 * base_magnitude))
}

fn payload_target_map(frame_shape: (usize, usize), payload: &[u8]) -> Grid<i8> {
    let n_cells = frame_shape.0 * frame_shape.1;
    if payload.is_empty() {
        return Grid::filled(frame_shape.0, frame_shape.1, 0);
    }

    let bits = bytes_to_bits(payload);
    let data = (0..n_cells)
        .map(|idx| if bits[idx % bits.len()] == 1 { 1 } else { -1 })
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn qr_signs_in_frame(qr_in_frame: &Grid<u8>) -> Grid<i8> {
    qr_in_frame.map(|&module| if module == 0 { 1i8 } else { -1i8 })
}

fn normalize_magnitude(magnitude: i16, n_frames: usize) -> i16 {
    let max = n_frames as i16;
    let min = if n_frames.is_multiple_of(2) { 2 } else { 1 };
    let parity = (n_frames as i16) & 1;
    let mut value = magnitude.clamp(min, max);
    if (value & 1) != parity {
        if value < max {
            value += 1;
        } else {
            value -= 1;
        }
    }
    value.max(min)
}

fn assign_signed_samples(
    frames: &mut [Grid<i8>],
    row: usize,
    col: usize,
    desired_sum: i16,
    seed: &str,
    cell_index: u64,
) {
    let n_frames = frames.len() as i16;
    let n_positive = ((n_frames + desired_sum) / 2) as usize;
    let mut order: Vec<usize> = (0..frames.len()).collect();
    let mut rng = Prng::from_key(seed, cell_index);
    for i in (1..order.len()).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        order.swap(i, j);
    }

    for (rank, frame_idx) in order.into_iter().enumerate() {
        frames[frame_idx][(row, col)] = if rank < n_positive { 1 } else { -1 };
    }
}

fn embed_qr_in_frame(qr_grid: &Grid<u8>, frame_shape: (usize, usize)) -> Result<Grid<u8>> {
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

fn centered_qr_crop(grid: &Grid<u8>, size: usize) -> Result<Grid<u8>> {
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

fn extract_qr(accumulated: &Grid<i16>) -> Option<Grid<u8>> {
    let sign_grid = accumulated.map(|&value| u8::from(value <= 0));
    for size in [21usize, 25, 29, 33, 37, 41] {
        if size > sign_grid.width() || size > sign_grid.height() {
            continue;
        }
        let candidate = centered_qr_crop(&sign_grid, size).ok()?;
        if qr::decode::decode(&candidate).is_ok() {
            return Some(candidate);
        }
    }
    None
}

impl From<Grid<i8>> for Frame {
    fn from(value: Grid<i8>) -> Self {
        Frame::Signed(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_frame_count_is_enforced() {
        assert!(SignedEncoder::new(3, (41, 41), "seed", 3).is_err());
        assert!(SignedStreamDecoder::new(3, 4, 3).is_err());
    }

    #[test]
    fn expected_accumulation_is_deterministic() {
        let a = expected_accumulation((41, 41), "deterministic", 6, 3).unwrap();
        let b = expected_accumulation((41, 41), "deterministic", 6, 3).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn signed_frames_are_binary_pm_one() {
        let encoder = SignedEncoder::new(4, (21, 21), "seed", 3).unwrap();
        let frames = encoder.encode_message("hi", b"x").unwrap();
        for frame in frames {
            assert!(frame.data().iter().all(|&v| v == -1 || v == 1));
        }
    }

    #[test]
    fn embed_qr_is_centered() {
        let qr_grid = qr::encode::encode("A").unwrap();
        let embedded = embed_qr_in_frame(&qr_grid, (25, 25)).unwrap();
        let cropped = centered_qr_crop(&embedded, qr_grid.width()).unwrap();
        assert_eq!(cropped, qr_grid);
    }
}
