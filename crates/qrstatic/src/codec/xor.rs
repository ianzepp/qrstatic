use std::collections::VecDeque;

use crate::codec::{DecodeResult, EncodeConfig, Frame};
use crate::error::{Error, Result};
use crate::{Grid, Prng, qr};

/// Batch XOR codec encoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XorEncoder {
    config: EncodeConfig,
}

impl XorEncoder {
    pub fn new(n_frames: usize, seed: impl Into<String>) -> Result<Self> {
        if n_frames < 2 {
            return Err(Error::Codec(
                "XOR encoding requires at least 2 frames".into(),
            ));
        }

        Ok(Self {
            config: EncodeConfig::new(n_frames, seed),
        })
    }

    pub fn config(&self) -> &EncodeConfig {
        &self.config
    }

    pub fn encode_message(&self, data: &str) -> Result<Vec<Grid<u8>>> {
        let qr = qr::encode::encode(data)?;
        self.encode_qr(&qr)
    }

    pub fn encode_qr(&self, qr: &Grid<u8>) -> Result<Vec<Grid<u8>>> {
        let shape = (qr.width(), qr.height());
        let n_frames = self.config.n_frames;

        let mut frames = Vec::with_capacity(n_frames);
        let mut accumulated = Grid::new(shape.0, shape.1);

        for frame_index in 0..(n_frames - 1) {
            let frame = random_binary_frame(shape, &self.config.seed, frame_index as u64);
            xor_assign(&mut accumulated, &frame)?;
            frames.push(frame);
        }

        let final_frame = qr.zip_with(&accumulated, |a, b| a ^ b);
        frames.push(final_frame);

        Ok(frames)
    }
}

/// Batch XOR codec decoder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XorDecoder;

impl XorDecoder {
    pub fn decode_qr(frames: &[Grid<u8>]) -> Result<Grid<u8>> {
        let Some(first) = frames.first() else {
            return Err(Error::Codec("cannot decode zero XOR frames".into()));
        };

        let mut accumulated = Grid::new(first.width(), first.height());
        for frame in frames {
            xor_assign(&mut accumulated, frame)?;
        }

        Ok(accumulated)
    }

    pub fn decode_message(frames: &[Grid<u8>]) -> Result<DecodeResult> {
        let qr_grid = Self::decode_qr(frames)?;
        let message = qr::decode::decode(&qr_grid).ok();
        Ok(DecodeResult {
            qr: qr_grid,
            message,
        })
    }
}

/// Continuous XOR stream encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct XorStreamEncoder {
    encoder: XorEncoder,
    queue: VecDeque<String>,
    active_frames: Vec<Grid<u8>>,
    frame_index: usize,
    cycle_index: u64,
}

impl XorStreamEncoder {
    pub fn new(n_frames: usize, seed: impl Into<String>) -> Result<Self> {
        Ok(Self {
            encoder: XorEncoder::new(n_frames, seed)?,
            queue: VecDeque::new(),
            active_frames: Vec::new(),
            frame_index: 0,
            cycle_index: 0,
        })
    }

    pub fn queue_message(&mut self, data: impl Into<String>) {
        self.queue.push_back(data.into());
    }

    pub fn next_frame(&mut self) -> Result<Grid<u8>> {
        if self.frame_index >= self.active_frames.len() {
            self.start_next_cycle()?;
        }

        let frame = self.active_frames[self.frame_index].clone();
        self.frame_index += 1;
        Ok(frame)
    }

    fn start_next_cycle(&mut self) -> Result<()> {
        self.frame_index = 0;
        self.active_frames = if let Some(message) = self.queue.pop_front() {
            self.encoder.encode_message(&message)?
        } else {
            self.random_noise_cycle()
        };
        self.cycle_index += 1;
        Ok(())
    }

    fn random_noise_cycle(&self) -> Vec<Grid<u8>> {
        let size = self.encoder.config().n_frames;
        (0..size)
            .map(|offset| {
                random_binary_frame(
                    self.active_shape().unwrap_or((21, 21)),
                    self.encoder.config().seed.as_str(),
                    self.cycle_index * size as u64 + offset as u64,
                )
            })
            .collect()
    }

    fn active_shape(&self) -> Option<(usize, usize)> {
        self.active_frames
            .first()
            .map(|grid| (grid.width(), grid.height()))
            .or_else(|| {
                self.queue
                    .front()
                    .and_then(|message| qr::encode::encode(message).ok())
                    .map(|qr| (qr.width(), qr.height()))
            })
    }
}

/// Continuous XOR stream decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct XorStreamDecoder {
    n_frames: usize,
    buffer: Vec<Grid<u8>>,
    accumulated: Option<Grid<u8>>,
}

impl XorStreamDecoder {
    pub fn new(n_frames: usize) -> Result<Self> {
        if n_frames < 2 {
            return Err(Error::Codec(
                "XOR decoding requires at least 2 frames".into(),
            ));
        }

        Ok(Self {
            n_frames,
            buffer: Vec::with_capacity(n_frames),
            accumulated: None,
        })
    }

    pub fn push_frame(&mut self, frame: Grid<u8>) -> Result<Option<DecodeResult>> {
        if let Some(accumulated) = &mut self.accumulated {
            xor_assign(accumulated, &frame)?;
        } else {
            self.accumulated = Some(frame.clone());
        }

        self.buffer.push(frame);

        if self.buffer.len() < self.n_frames {
            return Ok(None);
        }

        let qr = self
            .accumulated
            .take()
            .ok_or_else(|| Error::Codec("missing XOR accumulator state".into()))?;
        self.buffer.clear();

        let message = qr::decode::decode(&qr).ok();
        Ok(Some(DecodeResult { qr, message }))
    }
}

fn random_binary_frame(shape: (usize, usize), seed: &str, index: u64) -> Grid<u8> {
    let mut rng = Prng::from_key(seed, index);
    let mut data = Vec::with_capacity(shape.0 * shape.1);
    for _ in 0..(shape.0 * shape.1) {
        data.push(u8::from(rng.next_bool(0.5)));
    }
    Grid::from_vec(data, shape.0, shape.1)
}

fn xor_assign(target: &mut Grid<u8>, frame: &Grid<u8>) -> Result<()> {
    if target.width() != frame.width() || target.height() != frame.height() {
        return Err(Error::GridMismatch {
            expected: target.len(),
            actual: frame.len(),
        });
    }

    for (lhs, rhs) in target.data_mut().iter_mut().zip(frame.data().iter()) {
        *lhs ^= *rhs;
    }

    Ok(())
}

impl From<Grid<u8>> for Frame {
    fn from(value: Grid<u8>) -> Self {
        Frame::Binary(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mismatched_dimensions_fail() {
        let mut decoder = XorStreamDecoder::new(2).unwrap();
        let a = Grid::new(21, 21);
        let b = Grid::new(25, 25);
        assert!(decoder.push_frame(a).unwrap().is_none());
        assert!(matches!(
            decoder.push_frame(b),
            Err(Error::GridMismatch { .. })
        ));
    }

    #[test]
    fn xor_requires_at_least_two_frames() {
        assert!(XorEncoder::new(1, "seed").is_err());
        assert!(XorStreamDecoder::new(1).is_err());
    }

    #[test]
    fn grid_converts_to_binary_frame() {
        let grid = Grid::filled(2, 2, 1u8);
        assert_eq!(Frame::from(grid.clone()), Frame::Binary(grid));
    }
}
