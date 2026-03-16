use crate::bits::{bits_to_bytes, bytes_to_bits};
use crate::codec::common::{
    embed_qr_in_frame, extract_qr_from_sign_grid, qr_signs_in_frame, validate_matching_frames,
};
use crate::error::{Error, Result};
use crate::grid::accumulate_f32;
use crate::{Grid, Prng, qr};

/// Shared decode output for the sliding-window codec.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingDecodeResult {
    pub layer1_qr: Grid<u8>,
    pub layer1_message: Option<String>,
    pub layer2_qr: Option<Grid<u8>>,
    pub layer2_message: Option<String>,
    pub payload: Option<Vec<u8>>,
}

type SlidingL2Decode = (Option<Grid<u8>>, Option<String>, Option<Vec<u8>>);

/// Shared configuration for the sliding-window codec.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingConfig {
    pub frame_shape: (usize, usize),
    pub n1: usize,
    pub stride: usize,
    pub n2: usize,
    pub l1_signal: f32,
    pub l1_noise: f32,
    pub l2_signal: f32,
    pub l2_noise: f32,
    pub payload_delta: f32,
}

impl SlidingConfig {
    pub fn new(frame_shape: (usize, usize), n1: usize, stride: usize, n2: usize) -> Self {
        Self {
            frame_shape,
            n1,
            stride,
            n2,
            l1_signal: 5.0,
            l1_noise: 0.2,
            l2_signal: 2.0,
            l2_noise: 0.05,
            payload_delta: 0.5,
        }
    }
}

/// Batch sliding-window encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingEncoder {
    config: SlidingConfig,
}

impl SlidingEncoder {
    pub fn new(config: SlidingConfig) -> Result<Self> {
        validate_sliding_config(&config)?;
        Ok(Self { config })
    }

    pub fn encode_l1(&self, l1_key: &str, total_frames: usize) -> Result<Vec<Grid<f32>>> {
        if total_frames < self.config.n1 {
            return Err(Error::Codec(
                "sliding encoding requires total_frames >= n1".into(),
            ));
        }
        let qr1 = embed_qr_in_frame(&qr::encode::encode(l1_key)?, self.config.frame_shape)?;
        let qr1_signs = qr_signs_in_frame(&qr1);
        let l1_per_frame = Grid::from_vec(
            qr1_signs
                .data()
                .iter()
                .map(|&sign| sign as f32 * self.config.l1_signal / self.config.n1 as f32)
                .collect(),
            self.config.frame_shape.0,
            self.config.frame_shape.1,
        );

        let mut frames = Vec::with_capacity(total_frames);
        for frame_index in 0..total_frames {
            let noise = noise_frame(
                self.config.frame_shape,
                l1_key,
                frame_index as u64,
                self.config.l1_noise,
            );
            frames.push(Grid::from_vec(
                l1_per_frame
                    .data()
                    .iter()
                    .zip(noise.data().iter())
                    .map(|(&signal, &noise)| signal + noise)
                    .collect(),
                self.config.frame_shape.0,
                self.config.frame_shape.1,
            ));
        }
        Ok(frames)
    }

    pub fn encode(
        &self,
        l1_key: &str,
        l2_key: Option<&str>,
        payload: &[u8],
        total_frames: usize,
    ) -> Result<Vec<Grid<f32>>> {
        let l1_frames = self.encode_l1(l1_key, total_frames)?;
        match l2_key {
            Some(key) => self.apply_l2_overlay(l1_frames, key, payload),
            None => Ok(l1_frames),
        }
    }

    fn apply_l2_overlay(
        &self,
        l1_frames: Vec<Grid<f32>>,
        l2_key: &str,
        payload: &[u8],
    ) -> Result<Vec<Grid<f32>>> {
        let qr2 = embed_qr_in_frame(&qr::encode::encode(l2_key)?, self.config.frame_shape)?;
        let qr2_signs = qr_signs_in_frame(&qr2);
        let payload_bias =
            payload_bias_map(self.config.frame_shape, payload, self.config.payload_delta);
        let total_l2_frames = self.config.n1 * self.config.n2;
        let l2_per_frame = Grid::from_vec(
            qr2_signs
                .data()
                .iter()
                .zip(payload_bias.data().iter())
                .map(|(&sign, &bias)| {
                    sign as f32 * (self.config.l2_signal + bias) / total_l2_frames as f32
                })
                .collect(),
            self.config.frame_shape.0,
            self.config.frame_shape.1,
        );

        let mut result = Vec::with_capacity(l1_frames.len());
        for (frame_index, frame) in l1_frames.into_iter().enumerate() {
            let noise = noise_frame(
                self.config.frame_shape,
                &format!("l2:{l2_key}"),
                frame_index as u64,
                self.config.l2_noise,
            );
            let data: Vec<f32> = frame
                .data()
                .iter()
                .zip(l2_per_frame.data().iter())
                .zip(noise.data().iter())
                .map(|((&base, &overlay), &noise)| {
                    if frame_index < total_l2_frames {
                        base + overlay + noise
                    } else {
                        base
                    }
                })
                .collect();
            result.push(Grid::from_vec(
                data,
                self.config.frame_shape.0,
                self.config.frame_shape.1,
            ));
        }
        Ok(result)
    }
}

/// Batch sliding-window decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingDecoder {
    config: SlidingConfig,
    expected_payload_len: usize,
}

impl SlidingDecoder {
    pub fn new(config: SlidingConfig, expected_payload_len: usize) -> Result<Self> {
        validate_sliding_config(&config)?;
        Ok(Self {
            config,
            expected_payload_len,
        })
    }

    pub fn decode_l1_at_offset(
        &self,
        frames: &[Grid<f32>],
        start: usize,
        l1_key: Option<&str>,
    ) -> Result<SlidingDecodeResult> {
        validate_matching_frames(frames, "cannot decode zero sliding frames")?;
        if start + self.config.n1 > frames.len() {
            return Err(Error::Codec(
                "not enough frames for sliding L1 window".into(),
            ));
        }
        let accumulated = accumulate_f32(&frames[start..start + self.config.n1]);
        let cleaned = match l1_key {
            Some(key) => subtract_noise(
                &accumulated,
                &expected_l1_noise_sum(
                    self.config.frame_shape,
                    key,
                    start,
                    self.config.n1,
                    self.config.l1_noise,
                ),
            ),
            None => accumulated,
        };
        let layer1_qr = extract_qr(&cleaned).ok_or_else(|| {
            Error::Codec("could not extract a valid QR crop from sliding L1 output".into())
        })?;
        let layer1_message = qr::decode::decode(&layer1_qr).ok();
        Ok(SlidingDecodeResult {
            layer1_qr,
            layer1_message,
            layer2_qr: None,
            layer2_message: None,
            payload: None,
        })
    }

    pub fn decode(&self, frames: &[Grid<f32>], l1_start: usize) -> Result<SlidingDecodeResult> {
        let l1 = self.decode_l1_at_offset(frames, l1_start, None)?;
        let Some(l1_key) = l1.layer1_message.clone() else {
            return Ok(l1);
        };

        let (layer2_qr, layer2_message, payload) =
            self.decode_l2(frames, &l1_key, self.expected_payload_len)?;

        Ok(SlidingDecodeResult {
            layer1_qr: l1.layer1_qr,
            layer1_message: Some(l1_key),
            layer2_qr,
            layer2_message,
            payload,
        })
    }

    pub fn decode_l2(
        &self,
        frames: &[Grid<f32>],
        l1_key: &str,
        payload_length: usize,
    ) -> Result<SlidingL2Decode> {
        validate_matching_frames(frames, "cannot decode zero sliding frames")?;
        let mut l1_outputs = Vec::with_capacity(self.config.n2);
        for i in 0..self.config.n2 {
            let start = i * self.config.n1;
            if start + self.config.n1 > frames.len() {
                break;
            }
            let accumulated = accumulate_f32(&frames[start..start + self.config.n1]);
            let cleaned = subtract_noise(
                &accumulated,
                &expected_l1_noise_sum(
                    self.config.frame_shape,
                    l1_key,
                    start,
                    self.config.n1,
                    self.config.l1_noise,
                ),
            );
            l1_outputs.push(cleaned);
        }
        if l1_outputs.len() < self.config.n2 {
            return Ok((None, None, None));
        }

        let qr1 = embed_qr_in_frame(&qr::encode::encode(l1_key)?, self.config.frame_shape)?;
        let qr1_signs = qr_signs_in_frame(&qr1);
        let mut l2_accumulated = Grid::new(self.config.frame_shape.0, self.config.frame_shape.1);
        for l1_output in &l1_outputs {
            for ((acc, &value), &sign) in l2_accumulated
                .data_mut()
                .iter_mut()
                .zip(l1_output.data().iter())
                .zip(qr1_signs.data().iter())
            {
                let magnitude = value * sign as f32;
                *acc += magnitude - self.config.l1_signal;
            }
        }
        let corrected = Grid::from_vec(
            l2_accumulated
                .data()
                .iter()
                .zip(qr1_signs.data().iter())
                .map(|(&value, &sign)| value * sign as f32)
                .collect(),
            self.config.frame_shape.0,
            self.config.frame_shape.1,
        );

        let layer2_qr = extract_qr(&corrected);
        let layer2_message = layer2_qr
            .as_ref()
            .and_then(|grid| qr::decode::decode(grid).ok());
        let payload = match layer2_message.as_deref() {
            Some(l2_key) => Some(decode_l2_payload(
                &l2_accumulated,
                l2_key,
                self.config.n1 * self.config.n2,
                payload_length,
                self.config.l2_signal,
                self.config.l2_noise,
                self.config.frame_shape,
            )?),
            None => None,
        };

        Ok((layer2_qr, layer2_message, payload))
    }
}

/// Streaming sliding-window encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingStreamEncoder {
    config: SlidingConfig,
    l1_key: String,
    l2_key: Option<String>,
    payload: Vec<u8>,
    frame_index: usize,
    l1_per_frame: Grid<f32>,
    l2_per_frame: Option<Grid<f32>>,
}

impl SlidingStreamEncoder {
    pub fn new(config: SlidingConfig, l1_key: impl Into<String>) -> Result<Self> {
        validate_sliding_config(&config)?;
        let l1_key = l1_key.into();
        let l1_per_frame = l1_signal_per_frame(&config, &l1_key)?;
        Ok(Self {
            config,
            l1_key,
            l2_key: None,
            payload: Vec::new(),
            frame_index: 0,
            l1_per_frame,
            l2_per_frame: None,
        })
    }

    pub fn set_l2_message(&mut self, l2_key: impl Into<String>, payload: impl Into<Vec<u8>>) {
        self.l2_key = Some(l2_key.into());
        self.payload = payload.into();
        self.frame_index = 0;
        self.l2_per_frame = self
            .l2_key
            .as_deref()
            .and_then(|key| l2_signal_per_frame(&self.config, key, &self.payload).ok());
    }

    pub fn next_frame(&mut self) -> Result<Grid<f32>> {
        let l1_noise = noise_frame(
            self.config.frame_shape,
            &self.l1_key,
            self.frame_index as u64,
            self.config.l1_noise,
        );
        let mut data: Vec<f32> = self
            .l1_per_frame
            .data()
            .iter()
            .zip(l1_noise.data().iter())
            .map(|(&signal, &noise)| signal + noise)
            .collect();

        if let (Some(l2_key), Some(l2_per_frame)) =
            (self.l2_key.as_deref(), self.l2_per_frame.as_ref())
            && self.frame_index < self.config.n1 * self.config.n2
        {
            let l2_noise = noise_frame(
                self.config.frame_shape,
                &format!("l2:{l2_key}"),
                self.frame_index as u64,
                self.config.l2_noise,
            );
            for ((value, &overlay), &noise) in data
                .iter_mut()
                .zip(l2_per_frame.data().iter())
                .zip(l2_noise.data().iter())
            {
                *value += overlay + noise;
            }
        }

        let frame = Grid::from_vec(data, self.config.frame_shape.0, self.config.frame_shape.1);
        self.frame_index += 1;
        Ok(frame)
    }
}

/// Streaming sliding-window decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct SlidingStreamDecoder {
    decoder: SlidingDecoder,
    frames: Vec<Grid<f32>>,
    last_l1_emit_end: usize,
    l1_outputs: Vec<(String, Grid<f32>)>,
}

impl SlidingStreamDecoder {
    pub fn new(config: SlidingConfig, expected_payload_len: usize) -> Result<Self> {
        Ok(Self {
            decoder: SlidingDecoder::new(config, expected_payload_len)?,
            frames: Vec::new(),
            last_l1_emit_end: 0,
            l1_outputs: Vec::new(),
        })
    }

    pub fn push_frame(&mut self, frame: Grid<f32>) -> Result<Option<SlidingDecodeResult>> {
        self.frames.push(frame);
        let len = self.frames.len();
        if len < self.decoder.config.n1 {
            return Ok(None);
        }

        let current_end = len;
        if current_end - self.last_l1_emit_end < self.decoder.config.stride {
            return Ok(None);
        }

        let start = len - self.decoder.config.n1;
        let l1 = self
            .decoder
            .decode_l1_at_offset(&self.frames, start, None)?;
        self.last_l1_emit_end = current_end;

        if let Some(message) = l1.layer1_message.clone() {
            let cleaned = subtract_noise(
                &accumulate_f32(&self.frames[start..start + self.decoder.config.n1]),
                &expected_l1_noise_sum(
                    self.decoder.config.frame_shape,
                    &message,
                    start,
                    self.decoder.config.n1,
                    self.decoder.config.l1_noise,
                ),
            );
            self.l1_outputs.push((message.clone(), cleaned));

            if self.l1_outputs.len() >= self.decoder.config.n2 {
                let l1_key = self.l1_outputs[0].0.clone();
                let mut sampled_frames =
                    Vec::with_capacity(self.decoder.config.n1 * self.decoder.config.n2);
                for i in 0..self.decoder.config.n2 {
                    let block_start = i * self.decoder.config.n1;
                    if block_start + self.decoder.config.n1 <= self.frames.len() {
                        sampled_frames.extend_from_slice(
                            &self.frames[block_start..block_start + self.decoder.config.n1],
                        );
                    }
                }
                let (layer2_qr, layer2_message, payload) = self.decoder.decode_l2(
                    &sampled_frames,
                    &l1_key,
                    self.decoder.expected_payload_len,
                )?;
                return Ok(Some(SlidingDecodeResult {
                    layer1_qr: l1.layer1_qr,
                    layer1_message: Some(l1_key),
                    layer2_qr,
                    layer2_message,
                    payload,
                }));
            }
        }

        Ok(Some(l1))
    }
}

fn validate_sliding_config(config: &SlidingConfig) -> Result<()> {
    if config.frame_shape.0 == 0 || config.frame_shape.1 == 0 {
        return Err(Error::Codec("sliding requires non-empty frames".into()));
    }
    if config.n1 < 2 || config.n2 < 1 {
        return Err(Error::Codec("sliding requires n1 >= 2 and n2 >= 1".into()));
    }
    if config.stride == 0 || config.stride > config.n1 {
        return Err(Error::Codec("sliding requires 0 < stride <= n1".into()));
    }
    if config.l1_signal <= config.l1_noise || config.l2_signal <= config.l2_noise {
        return Err(Error::Codec(
            "sliding signal strengths must exceed their noise amplitudes".into(),
        ));
    }
    Ok(())
}

fn noise_frame(frame_shape: (usize, usize), seed: &str, index: u64, amplitude: f32) -> Grid<f32> {
    let mut rng = Prng::from_key(seed, index);
    let data = (0..(frame_shape.0 * frame_shape.1))
        .map(|_| rng.next_range(-amplitude, amplitude))
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn payload_bias_map(frame_shape: (usize, usize), payload: &[u8], payload_delta: f32) -> Grid<f32> {
    if payload.is_empty() {
        return Grid::filled(frame_shape.0, frame_shape.1, 0.0);
    }
    let bits = bytes_to_bits(payload);
    let data = (0..(frame_shape.0 * frame_shape.1))
        .map(|idx| {
            if bits[idx % bits.len()] == 1 {
                payload_delta
            } else {
                -payload_delta
            }
        })
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn l1_signal_per_frame(config: &SlidingConfig, l1_key: &str) -> Result<Grid<f32>> {
    let qr1 = embed_qr_in_frame(&qr::encode::encode(l1_key)?, config.frame_shape)?;
    let qr1_signs = qr_signs_in_frame(&qr1);
    Ok(Grid::from_vec(
        qr1_signs
            .data()
            .iter()
            .map(|&sign| sign as f32 * config.l1_signal / config.n1 as f32)
            .collect(),
        config.frame_shape.0,
        config.frame_shape.1,
    ))
}

fn l2_signal_per_frame(config: &SlidingConfig, l2_key: &str, payload: &[u8]) -> Result<Grid<f32>> {
    let qr2 = embed_qr_in_frame(&qr::encode::encode(l2_key)?, config.frame_shape)?;
    let qr2_signs = qr_signs_in_frame(&qr2);
    let payload_bias = payload_bias_map(config.frame_shape, payload, config.payload_delta);
    let total_l2_frames = config.n1 * config.n2;
    Ok(Grid::from_vec(
        qr2_signs
            .data()
            .iter()
            .zip(payload_bias.data().iter())
            .map(|(&sign, &bias)| sign as f32 * (config.l2_signal + bias) / total_l2_frames as f32)
            .collect(),
        config.frame_shape.0,
        config.frame_shape.1,
    ))
}

fn expected_l1_noise_sum(
    frame_shape: (usize, usize),
    l1_key: &str,
    start: usize,
    n1: usize,
    l1_noise: f32,
) -> Grid<f32> {
    let mut sum = Grid::new(frame_shape.0, frame_shape.1);
    for offset in 0..n1 {
        let frame = noise_frame(frame_shape, l1_key, (start + offset) as u64, l1_noise);
        for (lhs, rhs) in sum.data_mut().iter_mut().zip(frame.data().iter()) {
            *lhs += *rhs;
        }
    }
    sum
}

fn expected_l2_noise_sum(
    frame_shape: (usize, usize),
    l2_key: &str,
    total_frames: usize,
    l2_noise: f32,
) -> Grid<f32> {
    let mut sum = Grid::new(frame_shape.0, frame_shape.1);
    for index in 0..total_frames {
        let frame = noise_frame(frame_shape, &format!("l2:{l2_key}"), index as u64, l2_noise);
        for (lhs, rhs) in sum.data_mut().iter_mut().zip(frame.data().iter()) {
            *lhs += *rhs;
        }
    }
    sum
}

fn subtract_noise(actual: &Grid<f32>, noise: &Grid<f32>) -> Grid<f32> {
    Grid::from_vec(
        actual
            .data()
            .iter()
            .zip(noise.data().iter())
            .map(|(&a, &n)| a - n)
            .collect(),
        actual.width(),
        actual.height(),
    )
}

fn decode_l2_payload(
    l2_accumulated: &Grid<f32>,
    l2_key: &str,
    total_l2_frames: usize,
    payload_length: usize,
    l2_signal: f32,
    l2_noise: f32,
    frame_shape: (usize, usize),
) -> Result<Vec<u8>> {
    if payload_length == 0 {
        return Ok(Vec::new());
    }
    let cleaned = subtract_noise(
        l2_accumulated,
        &expected_l2_noise_sum(frame_shape, l2_key, total_l2_frames, l2_noise),
    );
    let qr2 = embed_qr_in_frame(&qr::encode::encode(l2_key)?, frame_shape)?;
    let qr2_signs = qr_signs_in_frame(&qr2);
    let n_bits = payload_length * 8;
    let mut votes = vec![Vec::new(); n_bits];
    for (idx, (&value, &sign)) in cleaned
        .data()
        .iter()
        .zip(qr2_signs.data().iter())
        .enumerate()
    {
        let magnitude = value * sign as f32;
        votes[idx % n_bits].push(u8::from(magnitude > l2_signal));
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

fn extract_qr(accumulated: &Grid<f32>) -> Option<Grid<u8>> {
    let sign_grid = accumulated.map(|&value| u8::from(value <= 0.0));
    extract_qr_from_sign_grid(&sign_grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_rejects_invalid_stride() {
        assert!(SlidingEncoder::new(SlidingConfig::new((41, 41), 4, 0, 3)).is_err());
        assert!(SlidingEncoder::new(SlidingConfig::new((41, 41), 4, 5, 3)).is_err());
    }
}
