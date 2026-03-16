use std::collections::VecDeque;

use crate::bits::{bits_to_bytes, bytes_to_bits};
use crate::codec::common::{
    embed_qr_in_frame, extract_qr_from_sign_grid, qr_signs_in_frame, validate_matching_frames,
};
use crate::error::{Error, Result};
use crate::grid::accumulate_f32;
use crate::{Grid, Prng, qr};

/// Shared decode output for the analog codec.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalogDecodeResult {
    pub qr: Grid<u8>,
    pub message: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// Batch analog codec encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalogEncoder {
    n_frames: usize,
    frame_shape: (usize, usize),
    noise_amplitude: f32,
    signal_strength: f32,
    payload_delta: f32,
}

impl AnalogEncoder {
    pub fn new(
        n_frames: usize,
        frame_shape: (usize, usize),
        noise_amplitude: f32,
        signal_strength: f32,
        payload_delta: f32,
    ) -> Result<Self> {
        validate_analog_params(
            n_frames,
            frame_shape,
            noise_amplitude,
            signal_strength,
            payload_delta,
        )?;
        Ok(Self {
            n_frames,
            frame_shape,
            noise_amplitude,
            signal_strength,
            payload_delta,
        })
    }

    pub fn encode_message(&self, qr_key: &str, payload: &[u8]) -> Result<Vec<Grid<f32>>> {
        let qr_grid = qr::encode::encode(qr_key)?;
        self.encode_qr(&qr_grid, qr_key, payload)
    }

    pub fn encode_qr(
        &self,
        qr_grid: &Grid<u8>,
        qr_key: &str,
        payload: &[u8],
    ) -> Result<Vec<Grid<f32>>> {
        let qr_in_frame = embed_qr_in_frame(qr_grid, self.frame_shape)?;
        let target_signs = qr_signs_in_frame(&qr_in_frame);
        let magnitude_bias = payload_bias_map(self.frame_shape, payload, self.payload_delta);
        let mut frames = Vec::with_capacity(self.n_frames);

        for frame_index in 0..self.n_frames {
            let mut rng = Prng::from_key(qr_key, frame_index as u64);
            let data = target_signs
                .data()
                .iter()
                .zip(magnitude_bias.data().iter())
                .map(|(&sign, &bias)| {
                    let noise = rng.next_range(-self.noise_amplitude, self.noise_amplitude);
                    sign as f32 * (self.signal_strength + bias) + noise
                })
                .collect();
            frames.push(Grid::from_vec(data, self.frame_shape.0, self.frame_shape.1));
        }

        Ok(frames)
    }
}

/// Batch analog codec decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalogDecoder {
    expected_payload_len: usize,
    noise_amplitude: f32,
    signal_strength: f32,
}

impl AnalogDecoder {
    pub fn new(
        expected_payload_len: usize,
        noise_amplitude: f32,
        signal_strength: f32,
    ) -> Result<Self> {
        validate_decode_params(noise_amplitude, signal_strength)?;
        Ok(Self {
            expected_payload_len,
            noise_amplitude,
            signal_strength,
        })
    }

    pub fn decode_qr(frames: &[Grid<f32>]) -> Result<Grid<u8>> {
        let accumulated = accumulate_analog_checked(frames)?;
        extract_qr(&accumulated).ok_or_else(|| {
            Error::Codec("could not extract a valid QR crop from analog frame".into())
        })
    }

    pub fn decode_payload(
        &self,
        accumulated: &Grid<f32>,
        qr_key: &str,
        n_frames: usize,
        payload_length: usize,
    ) -> Result<Vec<u8>> {
        if payload_length == 0 {
            return Ok(Vec::new());
        }

        let expected_noise = expected_noise_sum(
            (accumulated.width(), accumulated.height()),
            qr_key,
            n_frames,
            self.noise_amplitude,
        );
        let cleaned = accumulated.zip_with(&expected_noise, |actual, expected| actual - expected);
        let n_bits = payload_length * 8;
        let mut votes = vec![Vec::new(); n_bits];

        let threshold = n_frames as f32 * self.signal_strength;
        for (flat_idx, &value) in cleaned.data().iter().enumerate() {
            votes[flat_idx % n_bits].push(u8::from(value.abs() > threshold));
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

    pub fn decode_message(&self, frames: &[Grid<f32>]) -> Result<AnalogDecodeResult> {
        let accumulated = accumulate_analog_checked(frames)?;
        let sign_grid = accumulated.map(|&value| u8::from(value <= 0.0));
        let Some(qr_grid) = extract_qr(&accumulated) else {
            return Ok(AnalogDecodeResult {
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

        Ok(AnalogDecodeResult {
            qr: qr_grid,
            message,
            payload,
        })
    }
}

/// Continuous analog stream encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalogStreamEncoder {
    encoder: AnalogEncoder,
    queue: VecDeque<(String, Vec<u8>)>,
    active_frames: Vec<Grid<f32>>,
    frame_index: usize,
    idle_cycle: u64,
}

impl AnalogStreamEncoder {
    pub fn new(
        n_frames: usize,
        frame_shape: (usize, usize),
        noise_amplitude: f32,
        signal_strength: f32,
        payload_delta: f32,
    ) -> Result<Self> {
        Ok(Self {
            encoder: AnalogEncoder::new(
                n_frames,
                frame_shape,
                noise_amplitude,
                signal_strength,
                payload_delta,
            )?,
            queue: VecDeque::new(),
            active_frames: Vec::new(),
            frame_index: 0,
            idle_cycle: 0,
        })
    }

    pub fn queue_message(&mut self, qr_key: impl Into<String>, payload: impl Into<Vec<u8>>) {
        self.queue.push_back((qr_key.into(), payload.into()));
    }

    pub fn next_frame(&mut self) -> Result<Grid<f32>> {
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

    fn random_noise_cycle(&self) -> Vec<Grid<f32>> {
        (0..self.encoder.n_frames)
            .map(|offset| {
                random_noise_frame(
                    self.encoder.frame_shape,
                    &format!("analog-idle:{}:{offset}", self.idle_cycle),
                    self.encoder.noise_amplitude,
                )
            })
            .collect()
    }
}

/// Continuous analog stream decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct AnalogStreamDecoder {
    n_frames: usize,
    decoder: AnalogDecoder,
    buffer: Vec<Grid<f32>>,
}

impl AnalogStreamDecoder {
    pub fn new(
        n_frames: usize,
        expected_payload_len: usize,
        noise_amplitude: f32,
        signal_strength: f32,
    ) -> Result<Self> {
        if n_frames < 2 {
            return Err(Error::Codec(
                "analog decoding requires at least 2 frames".into(),
            ));
        }
        Ok(Self {
            n_frames,
            decoder: AnalogDecoder::new(expected_payload_len, noise_amplitude, signal_strength)?,
            buffer: Vec::with_capacity(n_frames),
        })
    }

    pub fn push_frame(&mut self, frame: Grid<f32>) -> Result<Option<AnalogDecodeResult>> {
        self.buffer.push(frame);
        if self.buffer.len() < self.n_frames {
            return Ok(None);
        }

        let result = self.decoder.decode_message(&self.buffer)?;
        self.buffer.clear();
        Ok(Some(result))
    }
}

fn validate_analog_params(
    n_frames: usize,
    frame_shape: (usize, usize),
    noise_amplitude: f32,
    signal_strength: f32,
    payload_delta: f32,
) -> Result<()> {
    if n_frames < 2 {
        return Err(Error::Codec(
            "analog encoding requires at least 2 frames".into(),
        ));
    }
    if frame_shape.0 == 0 || frame_shape.1 == 0 {
        return Err(Error::Codec(
            "analog encoding requires non-empty frames".into(),
        ));
    }
    validate_decode_params(noise_amplitude, signal_strength)?;
    if payload_delta <= 0.0 {
        return Err(Error::Codec(
            "analog encoding requires payload_delta > 0".into(),
        ));
    }
    Ok(())
}

fn validate_decode_params(noise_amplitude: f32, signal_strength: f32) -> Result<()> {
    if noise_amplitude <= 0.0 {
        return Err(Error::Codec(
            "analog noise_amplitude must be > 0".into(),
        ));
    }
    if signal_strength <= noise_amplitude {
        return Err(Error::Codec(
            "analog signal_strength must be greater than noise_amplitude".into(),
        ));
    }
    Ok(())
}

fn accumulate_analog_checked(frames: &[Grid<f32>]) -> Result<Grid<f32>> {
    validate_matching_frames(frames, "cannot decode zero analog frames")?;
    Ok(accumulate_f32(frames))
}

fn random_noise_frame(frame_shape: (usize, usize), seed: &str, noise_amplitude: f32) -> Grid<f32> {
    let mut rng = Prng::from_str_seed(seed);
    let data = (0..(frame_shape.0 * frame_shape.1))
        .map(|_| rng.next_range(-noise_amplitude, noise_amplitude))
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn expected_noise_sum(
    frame_shape: (usize, usize),
    qr_key: &str,
    n_frames: usize,
    noise_amplitude: f32,
) -> Grid<f32> {
    let mut sum = Grid::new(frame_shape.0, frame_shape.1);
    for frame_index in 0..n_frames {
        let mut rng = Prng::from_key(qr_key, frame_index as u64);
        for value in sum.data_mut().iter_mut() {
            *value += rng.next_range(-noise_amplitude, noise_amplitude);
        }
    }
    sum
}

fn payload_bias_map(frame_shape: (usize, usize), payload: &[u8], payload_delta: f32) -> Grid<f32> {
    let n_cells = frame_shape.0 * frame_shape.1;
    if payload.is_empty() {
        return Grid::filled(frame_shape.0, frame_shape.1, 0.0);
    }

    let bits = bytes_to_bits(payload);
    let data = (0..n_cells)
        .map(|idx| if bits[idx % bits.len()] == 1 { payload_delta } else { -payload_delta })
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn extract_qr(accumulated: &Grid<f32>) -> Option<Grid<u8>> {
    let sign_grid = accumulated.map(|&value| u8::from(value <= 0.0));
    extract_qr_from_sign_grid(&sign_grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_rejects_invalid_params() {
        assert!(AnalogEncoder::new(1, (41, 41), 0.3, 5.0, 0.5).is_err());
        assert!(AnalogEncoder::new(8, (0, 41), 0.3, 5.0, 0.5).is_err());
        assert!(AnalogEncoder::new(8, (41, 41), 0.0, 5.0, 0.5).is_err());
        assert!(AnalogEncoder::new(8, (41, 41), 0.3, 0.3, 0.5).is_err());
    }

    #[test]
    fn decode_qr_rejects_empty_input() {
        assert!(AnalogDecoder::decode_qr(&[]).is_err());
    }

    #[test]
    fn expected_noise_sum_is_deterministic() {
        let a = expected_noise_sum((41, 41), "analog-key", 8, 0.3);
        let b = expected_noise_sum((41, 41), "analog-key", 8, 0.3);
        assert_eq!(a, b);
    }
}
