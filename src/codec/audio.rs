use crate::codec::common::extract_qr_from_sign_grid;
use crate::error::{Error, Result};
use crate::{Grid, Prng, qr};

/// Default virtual audio frame size: 64x64 samples.
pub const DEFAULT_AUDIO_FRAME_SIZE: usize = 64 * 64;

/// Shared decode output for the audio codec.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioDecodeResult {
    pub qr: Grid<u8>,
    pub message: Option<String>,
    pub accumulated: Grid<f32>,
}

/// Shared configuration for the audio codec.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioConfig {
    pub n_frames: usize,
    pub frame_size: usize,
    pub flip_probability: f32,
    pub seed: String,
}

impl AudioConfig {
    pub fn new(
        n_frames: usize,
        frame_size: usize,
        flip_probability: f32,
        seed: impl Into<String>,
    ) -> Self {
        Self {
            n_frames,
            frame_size,
            flip_probability,
            seed: seed.into(),
        }
    }
}

/// Batch audio encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioEncoder {
    config: AudioConfig,
}

impl AudioEncoder {
    pub fn new(config: AudioConfig) -> Result<Self> {
        validate_audio_config(&config)?;
        Ok(Self { config })
    }

    pub fn encode_samples(&self, cover_samples: &[f32], qr_key: &str) -> Result<Vec<f32>> {
        let desired = desired_signs(qr_key, self.config.frame_size)?;
        let mut rng = Prng::from_str_seed(&self.config.seed);
        let mut encoded = cover_samples.to_vec();

        for (index, sample) in encoded.iter_mut().enumerate() {
            let desired_sign = desired[index % self.config.frame_size];
            let current_sign = sample.signum();
            if current_sign != 0.0
                && ((desired_sign > 0.0 && current_sign < 0.0)
                    || (desired_sign < 0.0 && current_sign > 0.0))
                && rng.next_bool(self.config.flip_probability)
            {
                *sample = -*sample;
            }
        }

        Ok(encoded)
    }
}

/// Batch audio decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioDecoder {
    config: AudioConfig,
}

impl AudioDecoder {
    pub fn new(config: AudioConfig) -> Result<Self> {
        validate_audio_config(&config)?;
        Ok(Self { config })
    }

    pub fn decode_samples(&self, samples: &[f32]) -> Result<AudioDecodeResult> {
        let samples_needed = self.config.n_frames * self.config.frame_size;
        if samples.len() < samples_needed {
            return Err(Error::Codec(format!(
                "need {samples_needed} audio samples, got {}",
                samples.len()
            )));
        }

        let dim = frame_dim(self.config.frame_size)?;
        let mut accumulated = vec![0.0f32; self.config.frame_size];
        for frame_idx in 0..self.config.n_frames {
            let start = frame_idx * self.config.frame_size;
            let end = start + self.config.frame_size;
            for (acc, sample) in accumulated.iter_mut().zip(samples[start..end].iter()) {
                *acc += *sample;
            }
        }

        let accumulated = Grid::from_vec(accumulated, dim, dim);
        let qr =
            extract_qr(&accumulated).unwrap_or_else(|| accumulated.map(|&v| u8::from(v < 0.0)));
        let message = qr::decode::decode(&qr).ok();

        Ok(AudioDecodeResult {
            qr,
            message,
            accumulated,
        })
    }
}

/// Streaming audio encoder.
#[derive(Debug, Clone)]
pub struct AudioStreamEncoder {
    config: AudioConfig,
    desired: Vec<f32>,
    rng: Prng,
    sample_index: usize,
}

impl AudioStreamEncoder {
    pub fn new(config: AudioConfig, qr_key: &str) -> Result<Self> {
        validate_audio_config(&config)?;
        Ok(Self {
            desired: desired_signs(qr_key, config.frame_size)?,
            rng: Prng::from_str_seed(&config.seed),
            config,
            sample_index: 0,
        })
    }

    pub fn encode_sample(&mut self, sample: f32) -> f32 {
        let desired_sign = self.desired[self.sample_index % self.config.frame_size];
        let current_sign = sample.signum();
        self.sample_index += 1;

        if current_sign != 0.0
            && ((desired_sign > 0.0 && current_sign < 0.0)
                || (desired_sign < 0.0 && current_sign > 0.0))
            && self.rng.next_bool(self.config.flip_probability)
        {
            -sample
        } else {
            sample
        }
    }

    pub fn encode_chunk(&mut self, samples: &[f32]) -> Vec<f32> {
        samples
            .iter()
            .map(|&sample| self.encode_sample(sample))
            .collect()
    }
}

/// Streaming audio decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioStreamDecoder {
    config: AudioConfig,
    accumulated: Vec<f32>,
    sample_count: usize,
    frame_count: usize,
}

impl AudioStreamDecoder {
    pub fn new(config: AudioConfig) -> Result<Self> {
        validate_audio_config(&config)?;
        Ok(Self {
            accumulated: vec![0.0; config.frame_size],
            config,
            sample_count: 0,
            frame_count: 0,
        })
    }

    pub fn push_sample(&mut self, sample: f32) -> Result<Option<AudioDecodeResult>> {
        let module_idx = self.sample_count % self.config.frame_size;
        self.accumulated[module_idx] += sample;
        self.sample_count += 1;

        if self.sample_count.is_multiple_of(self.config.frame_size) {
            self.frame_count += 1;
            if self.frame_count >= self.config.n_frames {
                let dim = frame_dim(self.config.frame_size)?;
                let accumulated = Grid::from_vec(std::mem::take(&mut self.accumulated), dim, dim);
                let qr = extract_qr(&accumulated)
                    .unwrap_or_else(|| accumulated.map(|&v| u8::from(v < 0.0)));
                let message = qr::decode::decode(&qr).ok();
                self.accumulated = vec![0.0; self.config.frame_size];
                self.sample_count = 0;
                self.frame_count = 0;
                return Ok(Some(AudioDecodeResult {
                    qr,
                    message,
                    accumulated,
                }));
            }
        }

        Ok(None)
    }

    pub fn push_chunk(&mut self, samples: &[f32]) -> Result<Vec<AudioDecodeResult>> {
        let mut results = Vec::new();
        for &sample in samples {
            if let Some(result) = self.push_sample(sample)? {
                results.push(result);
            }
        }
        Ok(results)
    }
}

fn validate_audio_config(config: &AudioConfig) -> Result<()> {
    if config.n_frames < 1 {
        return Err(Error::Codec("audio requires n_frames >= 1".into()));
    }
    frame_dim(config.frame_size)?;
    if !(0.0..=1.0).contains(&config.flip_probability) {
        return Err(Error::Codec(
            "audio flip_probability must be in [0.0, 1.0]".into(),
        ));
    }
    Ok(())
}

fn frame_dim(frame_size: usize) -> Result<usize> {
    let dim = (frame_size as f64).sqrt() as usize;
    if dim * dim != frame_size {
        return Err(Error::Codec(
            "audio frame_size must be a perfect square".into(),
        ));
    }
    Ok(dim)
}

fn desired_signs(qr_key: &str, frame_size: usize) -> Result<Vec<f32>> {
    let dim = frame_dim(frame_size)?;
    let qr_grid = qr::encode::encode(qr_key)?;
    let qr_frame = if qr_grid.width() == dim && qr_grid.height() == dim {
        qr_grid
    } else {
        embed_qr(dim, &qr_grid)?
    };
    Ok(qr_frame
        .data()
        .iter()
        .map(|&module| if module == 0 { 1.0 } else { -1.0 })
        .collect())
}

fn embed_qr(dim: usize, qr_grid: &Grid<u8>) -> Result<Grid<u8>> {
    if qr_grid.width() > dim || qr_grid.height() > dim {
        return Err(Error::Codec(format!(
            "audio frame dimension {dim} is smaller than QR size {}x{}",
            qr_grid.width(),
            qr_grid.height()
        )));
    }
    let mut frame = Grid::filled(dim, dim, 0u8);
    let row_offset = (dim - qr_grid.height()) / 2;
    let col_offset = (dim - qr_grid.width()) / 2;
    for row in 0..qr_grid.height() {
        for col in 0..qr_grid.width() {
            frame[(row + row_offset, col + col_offset)] = qr_grid[(row, col)];
        }
    }
    Ok(frame)
}

fn extract_qr(accumulated: &Grid<f32>) -> Option<Grid<u8>> {
    let sign_grid = accumulated.map(|&value| u8::from(value < 0.0));
    extract_qr_from_sign_grid(&sign_grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_rejects_invalid_frame_size() {
        assert!(AudioEncoder::new(AudioConfig::new(60, 1000, 0.4, "seed")).is_err());
    }
}
