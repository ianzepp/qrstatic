use std::collections::VecDeque;

use crate::bits::{bits_to_bytes, bytes_to_bits};
use crate::codec::common::{
    embed_qr_in_frame, extract_qr_from_sign_grid, qr_signs_in_frame, validate_matching_frames,
};
use crate::error::{Error, Result};
use crate::grid::accumulate_f32;
use crate::{Grid, Prng, qr};

/// Shared decode output for the two-layer recursive codec.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredDecodeResult {
    pub layer1_qr: Grid<u8>,
    pub layer1_message: Option<String>,
    pub layer2_qr: Option<Grid<u8>>,
    pub layer2_message: Option<String>,
    pub payload: Option<Vec<u8>>,
}

/// Shared configuration for the two-layer recursive codec.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredConfig {
    pub frame_shape: (usize, usize),
    pub n1: usize,
    pub n2: usize,
    pub layer1_signal: f32,
    pub layer1_noise: f32,
    pub layer2_signal: f32,
    pub layer2_noise: f32,
    pub payload_delta: f32,
}

impl LayeredConfig {
    pub fn new(frame_shape: (usize, usize), n1: usize, n2: usize) -> Self {
        Self {
            frame_shape,
            n1,
            n2,
            layer1_signal: 5.0,
            layer1_noise: 0.2,
            layer2_signal: 2.0,
            layer2_noise: 0.1,
            payload_delta: 0.5,
        }
    }
}

/// Batch two-layer recursive encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredEncoder {
    config: LayeredConfig,
}

impl LayeredEncoder {
    pub fn new(config: LayeredConfig) -> Result<Self> {
        validate_layered_params(&config)?;
        Ok(Self { config })
    }

    pub fn encode(
        &self,
        layer1_key: &str,
        layer2_key: &str,
        payload: &[u8],
    ) -> Result<Vec<Grid<f32>>> {
        let qr1 = embed_qr_in_frame(&qr::encode::encode(layer1_key)?, self.config.frame_shape)?;
        let qr2 = embed_qr_in_frame(&qr::encode::encode(layer2_key)?, self.config.frame_shape)?;
        let qr1_signs = qr_signs_in_frame(&qr1);
        let qr2_signs = qr_signs_in_frame(&qr2);
        let payload_bias =
            payload_bias_map(self.config.frame_shape, payload, self.config.payload_delta);

        let target_layer2_deviation = Grid::from_vec(
            qr2_signs
                .data()
                .iter()
                .zip(payload_bias.data().iter())
                .map(|(&sign, &bias)| sign as f32 * (self.config.layer2_signal + bias))
                .collect(),
            self.config.frame_shape.0,
            self.config.frame_shape.1,
        );

        let mut frames = Vec::with_capacity(self.config.n1 * self.config.n2);
        for l2_idx in 0..self.config.n2 {
            let l2_noise = noise_frame(
                self.config.frame_shape,
                &format!("layer2:{layer2_key}"),
                l2_idx as u64,
                self.config.layer2_noise,
            );
            let target_l1_magnitude = Grid::from_vec(
                target_layer2_deviation
                    .data()
                    .iter()
                    .zip(l2_noise.data().iter())
                    .map(|(&deviation, &noise)| {
                        self.config.layer1_signal + deviation / self.config.n2 as f32 + noise
                    })
                    .collect(),
                self.config.frame_shape.0,
                self.config.frame_shape.1,
            );

            let target_l1_sum = Grid::from_vec(
                qr1_signs
                    .data()
                    .iter()
                    .zip(target_l1_magnitude.data().iter())
                    .map(|(&sign, &magnitude)| sign as f32 * magnitude)
                    .collect(),
                self.config.frame_shape.0,
                self.config.frame_shape.1,
            );

            for l1_idx in 0..self.config.n1 {
                let global_frame_index = (l2_idx * self.config.n1 + l1_idx) as u64;
                let l1_noise = noise_frame(
                    self.config.frame_shape,
                    &format!("layer1:{layer1_key}"),
                    global_frame_index,
                    self.config.layer1_noise,
                );
                let frame = Grid::from_vec(
                    target_l1_sum
                        .data()
                        .iter()
                        .zip(l1_noise.data().iter())
                        .map(|(&signal, &noise)| signal / self.config.n1 as f32 + noise)
                        .collect(),
                    self.config.frame_shape.0,
                    self.config.frame_shape.1,
                );
                frames.push(frame);
            }
        }

        Ok(frames)
    }
}

/// Batch two-layer recursive decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredDecoder {
    expected_payload_len: usize,
    config: LayeredConfig,
}

impl LayeredDecoder {
    pub fn new(config: LayeredConfig, expected_payload_len: usize) -> Result<Self> {
        validate_layered_decode_params(&config)?;
        Ok(Self {
            expected_payload_len,
            config,
        })
    }

    pub fn decode(&self, frames: &[Grid<f32>]) -> Result<LayeredDecodeResult> {
        validate_matching_frames(frames, "cannot decode zero layered frames")?;
        let l1_outputs = decode_layer1_outputs(frames, self.config.n1)?;
        let Some(first_l1) = l1_outputs.first() else {
            return Err(Error::Codec("not enough frames for one layered L1 output".into()));
        };

        let layer1_qr = extract_qr(first_l1).ok_or_else(|| {
            Error::Codec("could not extract a valid QR crop from layered L1 output".into())
        })?;
        let layer1_message = qr::decode::decode(&layer1_qr).ok();

        let Some(layer1_key) = layer1_message.clone() else {
            return Ok(LayeredDecodeResult {
                layer1_qr,
                layer1_message: None,
                layer2_qr: None,
                layer2_message: None,
                payload: None,
            });
        };

        if l1_outputs.len() < self.config.n2 {
            return Ok(LayeredDecodeResult {
                layer1_qr,
                layer1_message: Some(layer1_key),
                layer2_qr: None,
                layer2_message: None,
                payload: None,
            });
        }

        let layer2_field = decode_layer2_field(
            &l1_outputs[..self.config.n2],
            &layer1_key,
            self.config.n1,
            self.config.layer1_signal,
            self.config.layer1_noise,
        )?;
        let layer2_qr = extract_qr(&layer2_field);
        let layer2_message = layer2_qr.as_ref().and_then(|grid| qr::decode::decode(grid).ok());
        let payload = match layer2_message.as_deref() {
            Some(layer2_key) => Some(decode_layer2_payload(
                &layer2_field,
                layer2_key,
                self.config.n2,
                self.expected_payload_len,
                self.config.layer2_signal,
                self.config.layer2_noise,
            )?),
            None => None,
        };

        Ok(LayeredDecodeResult {
            layer1_qr,
            layer1_message: Some(layer1_key),
            layer2_qr,
            layer2_message,
            payload,
        })
    }
}

/// Streaming two-layer recursive encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredStreamEncoder {
    encoder: LayeredEncoder,
    queue: VecDeque<(String, String, Vec<u8>)>,
    active_frames: Vec<Grid<f32>>,
    frame_index: usize,
}

impl LayeredStreamEncoder {
    pub fn new(config: LayeredConfig) -> Result<Self> {
        Ok(Self {
            encoder: LayeredEncoder::new(config)?,
            queue: VecDeque::new(),
            active_frames: Vec::new(),
            frame_index: 0,
        })
    }

    pub fn queue_message(
        &mut self,
        layer1_key: impl Into<String>,
        layer2_key: impl Into<String>,
        payload: impl Into<Vec<u8>>,
    ) {
        self.queue
            .push_back((layer1_key.into(), layer2_key.into(), payload.into()));
    }

    pub fn next_frame(&mut self) -> Result<Option<Grid<f32>>> {
        if self.frame_index >= self.active_frames.len() {
            self.start_next_cycle()?;
        }
        if self.frame_index >= self.active_frames.len() {
            return Ok(None);
        }

        let frame = self.active_frames[self.frame_index].clone();
        self.frame_index += 1;
        Ok(Some(frame))
    }

    fn start_next_cycle(&mut self) -> Result<()> {
        self.frame_index = 0;
        self.active_frames = if let Some((l1, l2, payload)) = self.queue.pop_front() {
            self.encoder.encode(&l1, &l2, &payload)?
        } else {
            Vec::new()
        };
        Ok(())
    }
}

/// Streaming two-layer recursive decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct LayeredStreamDecoder {
    decoder: LayeredDecoder,
    frame_buffer: Vec<Grid<f32>>,
    l1_outputs: Vec<Grid<f32>>,
}

impl LayeredStreamDecoder {
    pub fn new(config: LayeredConfig, expected_payload_len: usize) -> Result<Self> {
        Ok(Self {
            frame_buffer: Vec::with_capacity(config.n1),
            l1_outputs: Vec::with_capacity(config.n2),
            decoder: LayeredDecoder::new(config, expected_payload_len)?,
        })
    }

    pub fn push_frame(&mut self, frame: Grid<f32>) -> Result<Option<LayeredDecodeResult>> {
        self.frame_buffer.push(frame);
        if self.frame_buffer.len() < self.decoder.config.n1 {
            return Ok(None);
        }

        let l1_output = accumulate_f32(&self.frame_buffer);
        self.frame_buffer.clear();
        self.l1_outputs.push(l1_output.clone());

        let layer1_qr = extract_qr(&l1_output).ok_or_else(|| {
            Error::Codec("could not extract a valid QR crop from layered L1 output".into())
        })?;
        let layer1_message = qr::decode::decode(&layer1_qr).ok();

        if self.l1_outputs.len() < self.decoder.config.n2 {
            return Ok(Some(LayeredDecodeResult {
                layer1_qr,
                layer1_message,
                layer2_qr: None,
                layer2_message: None,
                payload: None,
            }));
        }

        let full_outputs = std::mem::take(&mut self.l1_outputs);
        let full_result = self.decoder.decode_from_layer1_outputs(full_outputs)?;
        Ok(Some(full_result))
    }
}

impl LayeredDecoder {
    fn decode_from_layer1_outputs(&self, l1_outputs: Vec<Grid<f32>>) -> Result<LayeredDecodeResult> {
        let first_l1 = l1_outputs
            .first()
            .ok_or_else(|| Error::Codec("missing layered L1 outputs".into()))?;
        let layer1_qr = extract_qr(first_l1).ok_or_else(|| {
            Error::Codec("could not extract a valid QR crop from layered L1 output".into())
        })?;
        let layer1_message = qr::decode::decode(&layer1_qr).ok();
        let Some(layer1_key) = layer1_message.clone() else {
            return Ok(LayeredDecodeResult {
                layer1_qr,
                layer1_message: None,
                layer2_qr: None,
                layer2_message: None,
                payload: None,
            });
        };

        let layer2_field = decode_layer2_field(
            &l1_outputs,
            &layer1_key,
            self.config.n1,
            self.config.layer1_signal,
            self.config.layer1_noise,
        )?;
        let layer2_qr = extract_qr(&layer2_field);
        let layer2_message = layer2_qr.as_ref().and_then(|grid| qr::decode::decode(grid).ok());
        let payload = match layer2_message.as_deref() {
            Some(layer2_key) => Some(decode_layer2_payload(
                &layer2_field,
                layer2_key,
                self.config.n2,
                self.expected_payload_len,
                self.config.layer2_signal,
                self.config.layer2_noise,
            )?),
            None => None,
        };

        Ok(LayeredDecodeResult {
            layer1_qr,
            layer1_message: Some(layer1_key),
            layer2_qr,
            layer2_message,
            payload,
        })
    }
}

fn validate_layered_params(config: &LayeredConfig) -> Result<()> {
    if config.frame_shape.0 == 0 || config.frame_shape.1 == 0 {
        return Err(Error::Codec(
            "layered encoding requires non-empty frames".into(),
        ));
    }
    validate_layered_decode_params(config)?;
    if config.payload_delta <= 0.0 {
        return Err(Error::Codec("layered payload_delta must be > 0".into()));
    }
    Ok(())
}

fn validate_layered_decode_params(config: &LayeredConfig) -> Result<()> {
    if config.n1 < 2 || config.n2 < 2 {
        return Err(Error::Codec(
            "layered decoding requires n1 >= 2 and n2 >= 2".into(),
        ));
    }
    if config.layer1_signal <= config.layer1_noise || config.layer2_signal <= config.layer2_noise {
        return Err(Error::Codec(
            "layered signal strengths must exceed their noise amplitudes".into(),
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
        .map(|idx| if bits[idx % bits.len()] == 1 { payload_delta } else { -payload_delta })
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn decode_layer1_outputs(frames: &[Grid<f32>], n1: usize) -> Result<Vec<Grid<f32>>> {
    validate_matching_frames(frames, "cannot decode zero layered frames")?;
    let mut outputs = Vec::new();
    for chunk in frames.chunks(n1) {
        if chunk.len() == n1 {
            outputs.push(accumulate_f32(chunk));
        }
    }
    Ok(outputs)
}

fn expected_l1_noise_sum(
    frame_shape: (usize, usize),
    layer1_key: &str,
    n1: usize,
    output_index: usize,
    layer1_noise: f32,
) -> Grid<f32> {
    let mut sum = Grid::new(frame_shape.0, frame_shape.1);
    for offset in 0..n1 {
        let global = (output_index * n1 + offset) as u64;
        let frame = noise_frame(frame_shape, &format!("layer1:{layer1_key}"), global, layer1_noise);
        for (lhs, rhs) in sum.data_mut().iter_mut().zip(frame.data().iter()) {
            *lhs += *rhs;
        }
    }
    sum
}

fn decode_layer2_field(
    l1_outputs: &[Grid<f32>],
    layer1_key: &str,
    n1: usize,
    layer1_signal: f32,
    layer1_noise: f32,
) -> Result<Grid<f32>> {
    validate_matching_frames(l1_outputs, "cannot decode zero layered L1 outputs")?;
    let qr1 = embed_qr_in_frame(&qr::encode::encode(layer1_key)?, (l1_outputs[0].width(), l1_outputs[0].height()))?;
    let expected_signs = qr_signs_in_frame(&qr1);
    let mut layer2 = Grid::new(l1_outputs[0].width(), l1_outputs[0].height());

    for (output_index, l1_output) in l1_outputs.iter().enumerate() {
        let expected_noise = expected_l1_noise_sum(
            (l1_output.width(), l1_output.height()),
            layer1_key,
            n1,
            output_index,
            layer1_noise,
        );
        for (((acc, &value), &noise), &sign) in layer2
            .data_mut()
            .iter_mut()
            .zip(l1_output.data().iter())
            .zip(expected_noise.data().iter())
            .zip(expected_signs.data().iter())
        {
            let actual_magnitude = (value - noise) * sign as f32;
            *acc += actual_magnitude - layer1_signal;
        }
    }

    Ok(layer2)
}

fn expected_l2_noise_sum(
    frame_shape: (usize, usize),
    layer2_key: &str,
    n2: usize,
    layer2_noise: f32,
) -> Grid<f32> {
    let mut sum = Grid::new(frame_shape.0, frame_shape.1);
    for output_index in 0..n2 {
        let frame = noise_frame(
            frame_shape,
            &format!("layer2:{layer2_key}"),
            output_index as u64,
            layer2_noise,
        );
        for (lhs, rhs) in sum.data_mut().iter_mut().zip(frame.data().iter()) {
            *lhs += *rhs;
        }
    }
    sum
}

fn decode_layer2_payload(
    layer2_field: &Grid<f32>,
    layer2_key: &str,
    n2: usize,
    payload_length: usize,
    layer2_signal: f32,
    layer2_noise: f32,
) -> Result<Vec<u8>> {
    if payload_length == 0 {
        return Ok(Vec::new());
    }
    let qr2 = embed_qr_in_frame(&qr::encode::encode(layer2_key)?, (layer2_field.width(), layer2_field.height()))?;
    let expected_signs = qr_signs_in_frame(&qr2);
    let expected_noise = expected_l2_noise_sum(
        (layer2_field.width(), layer2_field.height()),
        layer2_key,
        n2,
        layer2_noise,
    );
    let n_bits = payload_length * 8;
    let mut votes = vec![Vec::new(); n_bits];

    for (flat_idx, ((&value, &noise), &sign)) in layer2_field
        .data()
        .iter()
        .zip(expected_noise.data().iter())
        .zip(expected_signs.data().iter())
        .enumerate()
    {
        let magnitude = (value - noise) * sign as f32;
        votes[flat_idx % n_bits].push(u8::from(magnitude > layer2_signal));
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
    fn constructor_rejects_invalid_params() {
        assert!(LayeredEncoder::new(LayeredConfig::new((41, 41), 1, 2)).is_err());
        assert!(LayeredEncoder::new(LayeredConfig::new((41, 41), 2, 1)).is_err());
        assert!(LayeredEncoder::new(LayeredConfig::new((0, 41), 2, 2)).is_err());
    }

    #[test]
    fn decode_rejects_empty_input() {
        assert!(LayeredDecoder::new(LayeredConfig::new((41, 41), 2, 2), 0)
            .unwrap()
            .decode(&[])
            .is_err());
    }
}
