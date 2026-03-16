use std::collections::VecDeque;

use crate::bits::{bits_to_bytes, bytes_to_bits};
use crate::codec::common::{
    embed_qr_in_frame, extract_qr_from_sign_grid, qr_signs_in_frame, validate_matching_frames,
};
use crate::codec::EncodeConfig;
use crate::error::{Error, Result};
use crate::grid::accumulate_i16;
use crate::{Grid, Prng, qr};

/// Default accumulation window used by the reference binary stream codec.
pub const DEFAULT_BINARY_WINDOW: usize = 60;

/// Shared decode output for the binary static codec.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryDecodeResult {
    pub qr: Grid<u8>,
    pub message: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// Batch binary static encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryEncoder {
    config: EncodeConfig,
    frame_shape: (usize, usize),
    base_bias: f32,
    payload_bias_delta: f32,
}

impl BinaryEncoder {
    pub fn new(
        n_frames: usize,
        frame_shape: (usize, usize),
        seed: impl Into<String>,
        base_bias: f32,
        payload_bias_delta: f32,
    ) -> Result<Self> {
        validate_binary_params(n_frames, frame_shape, base_bias, payload_bias_delta)?;
        Ok(Self {
            config: EncodeConfig::new(n_frames, seed),
            frame_shape,
            base_bias,
            payload_bias_delta,
        })
    }

    pub fn with_default_window(
        frame_shape: (usize, usize),
        seed: impl Into<String>,
        base_bias: f32,
        payload_bias_delta: f32,
    ) -> Result<Self> {
        Self::new(
            DEFAULT_BINARY_WINDOW,
            frame_shape,
            seed,
            base_bias,
            payload_bias_delta,
        )
    }

    pub fn config(&self) -> &EncodeConfig {
        &self.config
    }

    pub fn encode_message(&self, qr_key: &str, payload: &[u8]) -> Result<Vec<Grid<i8>>> {
        let qr_grid = qr::encode::encode(qr_key)?;
        self.encode_qr(&qr_grid, payload)
    }

    pub fn encode_qr(&self, qr_grid: &Grid<u8>, payload: &[u8]) -> Result<Vec<Grid<i8>>> {
        let qr_in_frame = embed_qr_in_frame(qr_grid, self.frame_shape)?;
        let bias_map = build_bias_map(
            &qr_in_frame,
            payload,
            self.base_bias,
            self.payload_bias_delta,
        );

        let mut frames = Vec::with_capacity(self.config.n_frames);
        for frame_index in 0..self.config.n_frames {
            let frame_seed = frame_seed(&self.config.seed, frame_index as u64);
            frames.push(sample_binary_frame(self.frame_shape, &bias_map, &frame_seed));
        }

        Ok(frames)
    }
}

/// Batch binary static decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryDecoder {
    expected_payload_len: usize,
    base_bias: f32,
}

impl BinaryDecoder {
    pub fn new(expected_payload_len: usize, base_bias: f32) -> Result<Self> {
        validate_bias(base_bias)?;
        Ok(Self {
            expected_payload_len,
            base_bias,
        })
    }

    pub fn decode_qr(frames: &[Grid<i8>]) -> Result<Grid<u8>> {
        let accumulated = accumulate_binary_checked(frames)?;
        extract_qr(&accumulated).ok_or_else(|| {
            Error::Codec("could not extract a valid QR crop from binary frame".into())
        })
    }

    pub fn decode_payload(
        &self,
        accumulated: &Grid<i16>,
        qr_key: &str,
        n_frames: usize,
        payload_length: usize,
    ) -> Result<Vec<u8>> {
        if payload_length == 0 {
            return Ok(Vec::new());
        }

        let qr_grid = qr::encode::encode(qr_key)?;
        let qr_in_frame = embed_qr_in_frame(&qr_grid, (accumulated.width(), accumulated.height()))?;
        let expected_signs = qr_signs_in_frame(&qr_in_frame);
        let expected_magnitude = n_frames as f32 * (2.0 * self.base_bias - 1.0);
        let n_bits = payload_length * 8;
        let mut votes = vec![Vec::new(); n_bits];

        for (flat_idx, (&value, &sign)) in accumulated
            .data()
            .iter()
            .zip(expected_signs.data().iter())
            .enumerate()
        {
            let aligned = value as f32 * sign as f32;
            votes[flat_idx % n_bits].push(u8::from(aligned > expected_magnitude));
        }

        let bits: Vec<u8> = votes
            .into_iter()
            .map(|samples| {
                let ones = samples.iter().copied().sum::<u8>() as usize;
                u8::from(ones > samples.len() / 2)
            })
            .collect();

        Ok(bits_to_bytes(&bits)
            .into_iter()
            .take(payload_length)
            .collect())
    }

    pub fn decode_message(&self, frames: &[Grid<i8>]) -> Result<BinaryDecodeResult> {
        let accumulated = accumulate_binary_checked(frames)?;
        let sign_grid = accumulated.map(|&value| u8::from(value < 0));
        let Some(qr_grid) = extract_qr(&accumulated) else {
            return Ok(BinaryDecodeResult {
                qr: sign_grid,
                message: None,
                payload: None,
            });
        };

        let message = qr::decode::decode(&qr_grid).ok();
        let payload = match &message {
            Some(qr_key) => Some(self.decode_payload(
                &accumulated,
                qr_key,
                frames.len(),
                self.expected_payload_len,
            )?),
            None => None,
        };

        Ok(BinaryDecodeResult {
            qr: qr_grid,
            message,
            payload,
        })
    }
}

/// Continuous binary static stream encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryStreamEncoder {
    encoder: BinaryEncoder,
    queue: VecDeque<(String, Vec<u8>)>,
    active_frames: Vec<Grid<i8>>,
    frame_index: usize,
    idle_cycle: u64,
}

impl BinaryStreamEncoder {
    pub fn new(
        n_frames: usize,
        frame_shape: (usize, usize),
        seed: impl Into<String>,
        base_bias: f32,
        payload_bias_delta: f32,
    ) -> Result<Self> {
        Ok(Self {
            encoder: BinaryEncoder::new(
                n_frames,
                frame_shape,
                seed,
                base_bias,
                payload_bias_delta,
            )?,
            queue: VecDeque::new(),
            active_frames: Vec::new(),
            frame_index: 0,
            idle_cycle: 0,
        })
    }

    pub fn with_default_window(
        frame_shape: (usize, usize),
        seed: impl Into<String>,
        base_bias: f32,
        payload_bias_delta: f32,
    ) -> Result<Self> {
        Self::new(
            DEFAULT_BINARY_WINDOW,
            frame_shape,
            seed,
            base_bias,
            payload_bias_delta,
        )
    }

    pub fn queue_message(&mut self, qr_key: impl Into<String>, payload: impl Into<Vec<u8>>) {
        self.queue.push_back((qr_key.into(), payload.into()));
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
        self.active_frames = if let Some((qr_key, payload)) = self.queue.pop_front() {
            self.encoder.encode_message(&qr_key, &payload)?
        } else {
            self.random_noise_cycle()
        };
        self.idle_cycle += 1;
        Ok(())
    }

    fn random_noise_cycle(&self) -> Vec<Grid<i8>> {
        let n_frames = self.encoder.config().n_frames;
        (0..n_frames)
            .map(|offset| {
                random_binary_noise(
                    self.encoder.frame_shape,
                    &frame_seed(
                        self.encoder.config().seed.as_str(),
                        self.idle_cycle * n_frames as u64 + offset as u64,
                    ),
                )
            })
            .collect()
    }
}

/// Continuous binary static stream decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryStreamDecoder {
    n_frames: usize,
    decoder: BinaryDecoder,
    buffer: Vec<Grid<i8>>,
}

impl BinaryStreamDecoder {
    pub fn new(n_frames: usize, expected_payload_len: usize, base_bias: f32) -> Result<Self> {
        if n_frames == 0 {
            return Err(Error::Codec(
                "binary static decoding requires at least 1 frame".into(),
            ));
        }
        Ok(Self {
            n_frames,
            decoder: BinaryDecoder::new(expected_payload_len, base_bias)?,
            buffer: Vec::with_capacity(n_frames),
        })
    }

    pub fn with_default_window(expected_payload_len: usize, base_bias: f32) -> Result<Self> {
        Self::new(DEFAULT_BINARY_WINDOW, expected_payload_len, base_bias)
    }

    pub fn push_frame(&mut self, frame: Grid<i8>) -> Result<Option<BinaryDecodeResult>> {
        self.buffer.push(frame);
        if self.buffer.len() < self.n_frames {
            return Ok(None);
        }

        let result = self.decoder.decode_message(&self.buffer)?;
        self.buffer.clear();
        Ok(Some(result))
    }
}

fn validate_binary_params(
    n_frames: usize,
    frame_shape: (usize, usize),
    base_bias: f32,
    payload_bias_delta: f32,
) -> Result<()> {
    if n_frames == 0 {
        return Err(Error::Codec(
            "binary static encoding requires at least 1 frame".into(),
        ));
    }
    if frame_shape.0 == 0 || frame_shape.1 == 0 {
        return Err(Error::Codec(
            "binary static encoding requires non-empty frames".into(),
        ));
    }
    validate_bias(base_bias)?;
    if !(0.0..=0.45).contains(&payload_bias_delta) {
        return Err(Error::Codec(
            "binary static payload_bias_delta must be in [0.0, 0.45]".into(),
        ));
    }
    Ok(())
}

fn validate_bias(base_bias: f32) -> Result<()> {
    if !(0.5..1.0).contains(&base_bias) {
        return Err(Error::Codec(
            "binary static base_bias must be in [0.5, 1.0)".into(),
        ));
    }
    Ok(())
}

fn accumulate_binary_checked(frames: &[Grid<i8>]) -> Result<Grid<i16>> {
    validate_matching_frames(frames, "cannot decode zero binary static frames")?;
    Ok(accumulate_i16(frames))
}

fn frame_seed(base_seed: &str, frame_index: u64) -> String {
    format!("{base_seed}:{frame_index}")
}

fn sample_binary_frame(
    frame_shape: (usize, usize),
    bias_map: &Grid<f32>,
    seed: &str,
) -> Grid<i8> {
    let mut rng = Prng::from_str_seed(seed);
    let data = bias_map
        .data()
        .iter()
        .map(|&bias| if rng.next_bool(bias) { 1 } else { -1 })
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn random_binary_noise(frame_shape: (usize, usize), seed: &str) -> Grid<i8> {
    let mut rng = Prng::from_str_seed(seed);
    let data = (0..(frame_shape.0 * frame_shape.1))
        .map(|_| if rng.next_bool(0.5) { 1 } else { -1 })
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn build_bias_map(
    qr_in_frame: &Grid<u8>,
    payload: &[u8],
    base_bias: f32,
    payload_bias_delta: f32,
) -> Grid<f32> {
    let payload_bits = bytes_to_bits(payload);
    let data = qr_in_frame
        .data()
        .iter()
        .enumerate()
        .map(|(idx, &module)| {
            let base = if module == 0 {
                base_bias
            } else {
                1.0 - base_bias
            };

            if payload_bits.is_empty() {
                return base;
            }

            let bit = payload_bits[idx % payload_bits.len()];
            modulate_bias(base, bit, payload_bias_delta)
        })
        .collect();
    Grid::from_vec(data, qr_in_frame.width(), qr_in_frame.height())
}

fn modulate_bias(base: f32, bit: u8, delta: f32) -> f32 {
    if bit == 1 {
        if base > 0.5 {
            (base + delta).min(0.95)
        } else {
            (base - delta).max(0.05)
        }
    } else if base > 0.5 {
        (base - delta).max(0.5)
    } else {
        (base + delta).min(0.5)
    }
}

fn extract_qr(accumulated: &Grid<i16>) -> Option<Grid<u8>> {
    let sign_grid = accumulated.map(|&value| u8::from(value < 0));
    extract_qr_from_sign_grid(&sign_grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_rejects_invalid_params() {
        assert!(BinaryEncoder::new(0, (41, 41), "seed", 0.8, 0.1).is_err());
        assert!(BinaryEncoder::new(8, (0, 41), "seed", 0.8, 0.1).is_err());
        assert!(BinaryEncoder::new(8, (41, 41), "seed", 0.5, 0.1).is_ok());
        assert!(BinaryEncoder::new(8, (41, 41), "seed", 1.0, 0.1).is_err());
        assert!(BinaryEncoder::new(8, (41, 41), "seed", 0.8, 0.5).is_err());
    }

    #[test]
    fn seed_affects_binary_frames() {
        let a = BinaryEncoder::new(4, (41, 41), "seed-a", 0.8, 0.1)
            .unwrap()
            .encode_message("binary-seed", b"")
            .unwrap();
        let b = BinaryEncoder::new(4, (41, 41), "seed-b", 0.8, 0.1)
            .unwrap()
            .encode_message("binary-seed", b"")
            .unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn decode_qr_rejects_empty_input() {
        assert!(BinaryDecoder::decode_qr(&[]).is_err());
    }
}
