use crate::codec::common::{extract_qr_from_sign_grid, validate_matching_frames};
use crate::codec::temporal_packet::{
    TemporalPacket, TemporalPacketProfile, decode_packet_stream, encode_packet_stream, packet_stream_layout,
    recover_payload,
};
use crate::error::{Error, Result};
use crate::{Grid, Prng, bits, qr};

/// Shared configuration for the fixed-window Layer 1 temporal codec.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalConfig {
    pub frame_shape: (usize, usize),
    pub n_frames: usize,
    pub noise_amplitude: f32,
    pub l1_amplitude: f32,
}

impl TemporalConfig {
    pub fn new(
        frame_shape: (usize, usize),
        n_frames: usize,
        noise_amplitude: f32,
        l1_amplitude: f32,
    ) -> Result<Self> {
        validate_temporal_params(frame_shape, n_frames, noise_amplitude, l1_amplitude)?;
        Ok(Self {
            frame_shape,
            n_frames,
            noise_amplitude,
            l1_amplitude,
        })
    }
}

/// Layer 1 decode policy for the temporal codec.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalDecodePolicy {
    pub min_detector_score: f32,
}

impl TemporalDecodePolicy {
    pub fn fixed_threshold(min_detector_score: f32) -> Result<Self> {
        if min_detector_score <= 0.0 {
            return Err(Error::Codec(
                "temporal decode policy requires min_detector_score > 0".into(),
            ));
        }
        Ok(Self { min_detector_score })
    }
}

/// Fixed-window Layer 1 correlation output for the temporal codec.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalCorrelation {
    pub field: Grid<f32>,
    pub detector_score: f32,
}

/// Fixed-window Layer 1 decode output for the temporal codec.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalDecodeResult {
    pub correlation: TemporalCorrelation,
    pub detector_score: f32,
    pub qr: Grid<u8>,
    pub message: Option<String>,
}

/// Layer 2 payload settings carried under the fixed-window temporal carrier.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalLayer2Config {
    pub amplitude: f32,
    pub payload_len: usize,
    pub packet_profile: TemporalPacketProfile,
}

impl TemporalLayer2Config {
    pub fn new(
        amplitude: f32,
        payload_len: usize,
        packet_profile: TemporalPacketProfile,
    ) -> Result<Self> {
        if amplitude <= 0.0 {
            return Err(Error::Codec(
                "temporal Layer 2 requires amplitude > 0".into(),
            ));
        }
        Ok(Self {
            amplitude,
            payload_len,
            packet_profile,
        })
    }
}

/// Fixed-window Layer 2 decode output for the temporal codec.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalLayer2DecodeResult {
    pub layer1: TemporalDecodeResult,
    pub residual_field: Grid<f32>,
    pub packets: Vec<TemporalPacket>,
    pub payload: Vec<u8>,
}

/// Fixed-window Layer 1 temporal encoder.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalEncoder {
    config: TemporalConfig,
}

impl TemporalEncoder {
    pub fn new(config: TemporalConfig) -> Result<Self> {
        validate_temporal_params(
            config.frame_shape,
            config.n_frames,
            config.noise_amplitude,
            config.l1_amplitude,
        )?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &TemporalConfig {
        &self.config
    }

    pub fn encode_message(&self, master_key: &str, qr_payload: &str) -> Result<Vec<Grid<f32>>> {
        let qr_grid = qr::encode::encode(qr_payload)?;
        self.encode_qr(master_key, &qr_grid)
    }

    pub fn encode_qr(&self, master_key: &str, qr_grid: &Grid<u8>) -> Result<Vec<Grid<f32>>> {
        let signal_map = build_l1_signal_map(qr_grid, self.config.frame_shape)?;
        let schedule =
            build_temporal_schedule(master_key, self.config.frame_shape, self.config.n_frames);
        let mut frames = Vec::with_capacity(self.config.n_frames);

        for frame_index in 0..self.config.n_frames {
            let mut frame = noise_frame(
                master_key,
                frame_index,
                self.config.frame_shape,
                self.config.noise_amplitude,
            );
            let permutation =
                frame_permutation(master_key, frame_index, self.config.frame_shape);

            for (logical_idx, (&chip, &signal)) in schedule[frame_index]
                .data()
                .iter()
                .zip(signal_map.data().iter())
                .enumerate()
            {
                let physical_idx = permutation[logical_idx];
                frame.data_mut()[physical_idx] += self.config.l1_amplitude * signal * chip;
            }

            frames.push(frame);
        }

        Ok(frames)
    }

    pub fn encode_message_with_payload(
        &self,
        master_key: &str,
        qr_payload: &str,
        payload: &[u8],
        layer2: &TemporalLayer2Config,
    ) -> Result<Vec<Grid<f32>>> {
        if payload.len() != layer2.payload_len {
            return Err(Error::Codec(format!(
                "temporal Layer 2 payload length mismatch: expected {}, got {}",
                layer2.payload_len,
                payload.len()
            )));
        }

        let qr_grid = qr::encode::encode(qr_payload)?;
        let signal_map = build_l1_signal_map(&qr_grid, self.config.frame_shape)?;
        let l1_schedule = build_temporal_schedule(master_key, self.config.frame_shape, self.config.n_frames);
        let l2_signal_map = build_l2_signal_map(payload, layer2, self.config.frame_shape)?;
        let l2_schedule =
            build_temporal_schedule_domain(master_key, self.config.frame_shape, self.config.n_frames, "l2");
        let mut frames = Vec::with_capacity(self.config.n_frames);

        for frame_index in 0..self.config.n_frames {
            let mut frame = noise_frame(
                master_key,
                frame_index,
                self.config.frame_shape,
                self.config.noise_amplitude,
            );
            let permutation =
                frame_permutation(master_key, frame_index, self.config.frame_shape);

            for logical_idx in 0..signal_map.len() {
                let physical_idx = permutation[logical_idx];
                let l1 = self.config.l1_amplitude
                    * signal_map.data()[logical_idx]
                    * l1_schedule[frame_index].data()[logical_idx];
                let l2 = layer2.amplitude
                    * l2_signal_map.data()[logical_idx]
                    * l2_schedule[frame_index].data()[logical_idx];
                frame.data_mut()[physical_idx] += l1 + l2;
            }

            frames.push(frame);
        }

        Ok(frames)
    }
}

/// Fixed-window Layer 1 temporal decoder.
#[derive(Debug, Clone, PartialEq)]
pub struct TemporalDecoder {
    config: TemporalConfig,
}

impl TemporalDecoder {
    pub fn new(config: TemporalConfig) -> Result<Self> {
        validate_temporal_params(
            config.frame_shape,
            config.n_frames,
            config.noise_amplitude,
            config.l1_amplitude,
        )?;
        Ok(Self { config })
    }

    pub fn config(&self) -> &TemporalConfig {
        &self.config
    }

    pub fn correlate_prefix(
        &self,
        frames: &[Grid<f32>],
        temporal_key: &str,
    ) -> Result<TemporalCorrelation> {
        validate_temporal_prefix_frames(frames, &self.config)?;
        let schedule =
            build_temporal_schedule(temporal_key, self.config.frame_shape, self.config.n_frames);
        let mut data = vec![0.0; self.config.frame_shape.0 * self.config.frame_shape.1];

        for frame_index in 0..frames.len() {
            let permutation = frame_permutation(temporal_key, frame_index, self.config.frame_shape);
            let logical_frame = unpermute_grid(&frames[frame_index], &permutation);

            for ((acc, &sample), &chip) in data
                .iter_mut()
                .zip(logical_frame.data().iter())
                .zip(schedule[frame_index].data().iter())
            {
                *acc += sample * chip;
            }
        }

        let field = Grid::from_vec(data, self.config.frame_shape.0, self.config.frame_shape.1);
        let detector_score = detector_score(&field);

        Ok(TemporalCorrelation {
            field,
            detector_score,
        })
    }

    pub fn correlate(&self, frames: &[Grid<f32>], temporal_key: &str) -> Result<TemporalCorrelation> {
        validate_temporal_frames(frames, &self.config)?;
        self.correlate_prefix(frames, temporal_key)
    }

    pub fn correlation_score(&self, frames: &[Grid<f32>], temporal_key: &str) -> Result<f32> {
        let correlation = self.correlate(frames, temporal_key)?;
        Ok(correlation.detector_score)
    }

    pub fn decode_qr(
        &self,
        frames: &[Grid<f32>],
        temporal_key: &str,
        policy: &TemporalDecodePolicy,
    ) -> Result<TemporalDecodeResult> {
        let correlation = self.correlate(frames, temporal_key)?;
        if correlation.detector_score < policy.min_detector_score {
            return Err(Error::Codec(format!(
                "temporal detector score {:.3} did not reach threshold {:.3}",
                correlation.detector_score, policy.min_detector_score
            )));
        }

        let sign_grid = sign_grid_from_field(&correlation.field);
        let qr = extract_qr_from_sign_grid(&sign_grid)
            .ok_or_else(|| Error::Codec("could not extract a valid QR crop from temporal field".into()))?;
        let message = qr::decode::decode(&qr).ok();
        let detector_score = correlation.detector_score;

        Ok(TemporalDecodeResult {
            correlation,
            detector_score,
            qr,
            message,
        })
    }

    pub fn decode_payload(
        &self,
        frames: &[Grid<f32>],
        temporal_key: &str,
        policy: &TemporalDecodePolicy,
        layer2: &TemporalLayer2Config,
    ) -> Result<TemporalLayer2DecodeResult> {
        let layer1 = self.decode_qr(frames, temporal_key, policy)?;
        let residual_field = correlate_layer2_residual(
            frames,
            temporal_key,
            &self.config,
            &layer1.qr,
            layer2,
        )?;
        let packet_stream = decode_l2_packet_stream(&residual_field, layer2, self.config.frame_shape)?;
        let packets = decode_packet_stream(&packet_stream, layer2.payload_len, layer2.packet_profile)?;
        let payload = recover_payload(&packets)?;

        Ok(TemporalLayer2DecodeResult {
            layer1,
            residual_field,
            packets,
            payload,
        })
    }
}

fn validate_temporal_params(
    frame_shape: (usize, usize),
    n_frames: usize,
    noise_amplitude: f32,
    l1_amplitude: f32,
) -> Result<()> {
    if frame_shape.0 == 0 || frame_shape.1 == 0 {
        return Err(Error::Codec(
            "temporal encoding requires non-empty frames".into(),
        ));
    }
    if n_frames < 4 {
        return Err(Error::Codec(
            "temporal encoding requires at least 4 frames".into(),
        ));
    }
    if n_frames % 2 != 0 {
        return Err(Error::Codec(
            "temporal encoding requires an even frame count for balanced schedules".into(),
        ));
    }
    if noise_amplitude < 0.0 {
        return Err(Error::Codec(
            "temporal encoding requires noise_amplitude >= 0".into(),
        ));
    }
    if l1_amplitude <= 0.0 {
        return Err(Error::Codec(
            "temporal encoding requires l1_amplitude > 0".into(),
        ));
    }
    Ok(())
}

fn validate_temporal_frames(frames: &[Grid<f32>], config: &TemporalConfig) -> Result<()> {
    validate_matching_frames(frames, "cannot decode zero temporal frames")?;
    if frames.len() != config.n_frames {
        return Err(Error::Codec(format!(
            "temporal decode requires exactly {} frames, got {}",
            config.n_frames,
            frames.len()
        )));
    }
    if frames[0].width() != config.frame_shape.0 || frames[0].height() != config.frame_shape.1 {
        return Err(Error::GridMismatch {
            expected: config.frame_shape.0 * config.frame_shape.1,
            actual: frames[0].len(),
        });
    }
    Ok(())
}

fn validate_temporal_prefix_frames(frames: &[Grid<f32>], config: &TemporalConfig) -> Result<()> {
    validate_matching_frames(frames, "cannot decode zero temporal frames")?;
    if frames.len() > config.n_frames {
        return Err(Error::Codec(format!(
            "temporal prefix correlation requires at most {} frames, got {}",
            config.n_frames,
            frames.len()
        )));
    }
    if frames[0].width() != config.frame_shape.0 || frames[0].height() != config.frame_shape.1 {
        return Err(Error::GridMismatch {
            expected: config.frame_shape.0 * config.frame_shape.1,
            actual: frames[0].len(),
        });
    }
    Ok(())
}

fn build_l1_signal_map(qr_grid: &Grid<u8>, frame_shape: (usize, usize)) -> Result<Grid<f32>> {
    if qr_grid.width() > frame_shape.0 || qr_grid.height() > frame_shape.1 {
        return Err(Error::Codec(format!(
            "frame shape {:?} is smaller than QR size {}x{}",
            frame_shape,
            qr_grid.width(),
            qr_grid.height()
        )));
    }

    let mut signal = Grid::filled(frame_shape.0, frame_shape.1, 0.0f32);
    let row_offset = (frame_shape.1 - qr_grid.height()) / 2;
    let col_offset = (frame_shape.0 - qr_grid.width()) / 2;

    for row in 0..qr_grid.height() {
        for col in 0..qr_grid.width() {
            signal[(row + row_offset, col + col_offset)] = if qr_grid[(row, col)] == 0 {
                1.0
            } else {
                -1.0
            };
        }
    }

    Ok(signal)
}

fn build_temporal_schedule(
    master_key: &str,
    frame_shape: (usize, usize),
    n_frames: usize,
) -> Vec<Grid<f32>> {
    build_temporal_schedule_domain(master_key, frame_shape, n_frames, "l1")
}

fn build_temporal_schedule_domain(
    master_key: &str,
    frame_shape: (usize, usize),
    n_frames: usize,
    domain: &str,
) -> Vec<Grid<f32>> {
    let n_cells = frame_shape.0 * frame_shape.1;
    let mut schedule =
        vec![Grid::filled(frame_shape.0, frame_shape.1, 0.0f32); n_frames];

    for cell_idx in 0..n_cells {
        let mut chips = vec![1.0f32; n_frames / 2];
        chips.extend(vec![-1.0f32; n_frames / 2]);
        let mut rng = Prng::from_str_seed(&format!(
            "qrstatic:temporal:v1:{domain}:{master_key}:cell:{cell_idx}"
        ));
        for idx in (1..chips.len()).rev() {
            let swap_idx = (rng.next_u64() as usize) % (idx + 1);
            chips.swap(idx, swap_idx);
        }

        for frame_index in 0..n_frames {
            schedule[frame_index].data_mut()[cell_idx] = chips[frame_index];
        }
    }

    schedule
}

fn build_l2_signal_map(
    payload: &[u8],
    layer2: &TemporalLayer2Config,
    frame_shape: (usize, usize),
) -> Result<Grid<f32>> {
    if layer2.payload_len == 0 {
        return Ok(Grid::filled(frame_shape.0, frame_shape.1, 0.0f32));
    }

    let packet_stream = encode_packet_stream(payload, layer2.packet_profile)?;
    let bits = bits::bytes_to_bits(&packet_stream);
    let n_cells = frame_shape.0 * frame_shape.1;
    if bits.len() > n_cells {
        return Err(Error::Codec(format!(
            "temporal Layer 2 needs {} cells for packet stream bits, but frame only has {} cells",
            bits.len(),
            n_cells
        )));
    }

    let mut data = vec![0.0f32; n_cells];
    for (cell_index, cell) in data.iter_mut().enumerate() {
        let bit = bits[cell_index % bits.len()];
        *cell = if bit == 1 { 1.0 } else { -1.0 };
    }
    Ok(Grid::from_vec(data, frame_shape.0, frame_shape.1))
}

fn correlate_layer2_residual(
    frames: &[Grid<f32>],
    temporal_key: &str,
    config: &TemporalConfig,
    qr: &Grid<u8>,
    layer2: &TemporalLayer2Config,
) -> Result<Grid<f32>> {
    validate_temporal_frames(frames, config)?;
    let l1_signal_map = build_l1_signal_map(qr, config.frame_shape)?;
    let l1_schedule = build_temporal_schedule(temporal_key, config.frame_shape, config.n_frames);
    let l2_schedule =
        build_temporal_schedule_domain(temporal_key, config.frame_shape, config.n_frames, "l2");
    let mut data = vec![0.0; config.frame_shape.0 * config.frame_shape.1];

    for frame_index in 0..frames.len() {
        let permutation = frame_permutation(temporal_key, frame_index, config.frame_shape);
        let logical_frame = unpermute_grid(&frames[frame_index], &permutation);

        for logical_idx in 0..logical_frame.len() {
            let l1 = config.l1_amplitude
                * l1_signal_map.data()[logical_idx]
                * l1_schedule[frame_index].data()[logical_idx];
            let residual = logical_frame.data()[logical_idx] - l1;
            data[logical_idx] += residual * l2_schedule[frame_index].data()[logical_idx];
        }
    }

    // Keep the residual field on the same scale as the matched-filter field.
    for value in &mut data {
        *value /= layer2.amplitude.max(1e-6);
    }
    Ok(Grid::from_vec(data, config.frame_shape.0, config.frame_shape.1))
}

fn decode_l2_packet_stream(
    residual_field: &Grid<f32>,
    layer2: &TemporalLayer2Config,
    frame_shape: (usize, usize),
) -> Result<Vec<u8>> {
    if layer2.payload_len == 0 {
        return Ok(Vec::new());
    }

    let layout = packet_stream_layout(layer2.payload_len, layer2.packet_profile)?;
    let stream_len: usize = layout.iter().sum();
    let n_bits = stream_len * 8;
    let n_cells = frame_shape.0 * frame_shape.1;
    if n_bits > n_cells {
        return Err(Error::Codec(format!(
            "temporal Layer 2 decode needs {} cells for packet stream bits, but frame only has {} cells",
            n_bits,
            n_cells
        )));
    }

    let mapping = bits::spread_bits(n_cells, n_bits);
    let mut votes = vec![Vec::new(); n_bits];
    for (bit_index, cells) in mapping.iter().enumerate() {
        for &cell_index in cells {
            votes[bit_index].push(residual_field.data()[cell_index]);
        }
    }
    let decoded_bits = bits::majority_vote_f32(&votes);
    let mut decoded_bytes = bits::bits_to_bytes(&decoded_bits);
    decoded_bytes.truncate(stream_len);
    Ok(decoded_bytes)
}

fn frame_permutation(master_key: &str, frame_index: usize, frame_shape: (usize, usize)) -> Vec<usize> {
    let len = frame_shape.0 * frame_shape.1;
    let mut permutation: Vec<usize> = (0..len).collect();
    let mut rng = Prng::from_str_seed(&format!(
        "qrstatic:temporal:v1:spatial:{master_key}:frame:{frame_index}"
    ));

    for idx in (1..len).rev() {
        let swap_idx = (rng.next_u64() as usize) % (idx + 1);
        permutation.swap(idx, swap_idx);
    }

    permutation
}

fn unpermute_grid<T: Clone>(grid: &Grid<T>, permutation: &[usize]) -> Grid<T> {
    assert_eq!(grid.len(), permutation.len(), "permutation length mismatch");
    let mut data = vec![grid.data()[0].clone(); grid.len()];
    for (logical_idx, &physical_idx) in permutation.iter().enumerate() {
        data[logical_idx] = grid.data()[physical_idx].clone();
    }
    Grid::from_vec(data, grid.width(), grid.height())
}

fn noise_frame(
    master_key: &str,
    frame_index: usize,
    frame_shape: (usize, usize),
    noise_amplitude: f32,
) -> Grid<f32> {
    let mut rng = Prng::from_str_seed(&format!(
        "qrstatic:temporal:v1:noise:{master_key}:frame:{frame_index}"
    ));
    let data = (0..(frame_shape.0 * frame_shape.1))
        .map(|_| rng.next_range(-noise_amplitude, noise_amplitude))
        .collect();
    Grid::from_vec(data, frame_shape.0, frame_shape.1)
}

fn naive_accumulate(frames: &[Grid<f32>]) -> Grid<f32> {
    let width = frames[0].width();
    let height = frames[0].height();
    let mut data = vec![0.0; width * height];
    for frame in frames {
        for (acc, &sample) in data.iter_mut().zip(frame.data().iter()) {
            *acc += sample;
        }
    }
    Grid::from_vec(data, width, height)
}

fn sign_grid_from_field(field: &Grid<f32>) -> Grid<u8> {
    field.map(|&value| u8::from(value < 0.0))
}

pub fn detector_score(field: &Grid<f32>) -> f32 {
    field.data().iter().map(|value| value.abs()).sum::<f32>() / field.len() as f32
}

pub fn naive_field(frames: &[Grid<f32>]) -> Result<Grid<f32>> {
    validate_matching_frames(frames, "cannot decode zero temporal frames")?;
    Ok(naive_accumulate(frames))
}

pub fn try_extract_qr(field: &Grid<f32>) -> Option<Grid<u8>> {
    let sign_grid = sign_grid_from_field(field);
    extract_qr_from_sign_grid(&sign_grid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_rejects_odd_frame_count() {
        assert!(TemporalConfig::new((41, 41), 63, 0.5, 0.35).is_err());
    }

    #[test]
    fn constructor_rejects_small_frame_count() {
        assert!(TemporalConfig::new((41, 41), 2, 0.5, 0.35).is_err());
    }

    #[test]
    fn schedules_are_balanced_per_cell() {
        let schedule = build_temporal_schedule("balance", (7, 7), 8);
        for cell_idx in 0..49 {
            let sum: f32 = schedule
                .iter()
                .map(|frame| frame.data()[cell_idx])
                .sum();
            assert_eq!(sum, 0.0);
        }
    }

    #[test]
    fn frame_permutation_is_total_and_deterministic() {
        let permutation_a = frame_permutation("permute", 3, (5, 5));
        let permutation_b = frame_permutation("permute", 3, (5, 5));
        let mut sorted = permutation_a.clone();
        sorted.sort_unstable();
        assert_eq!(permutation_a, permutation_b);
        assert_eq!(sorted, (0..25).collect::<Vec<_>>());
    }
}
