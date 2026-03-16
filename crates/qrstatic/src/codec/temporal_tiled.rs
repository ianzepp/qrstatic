use crate::codec::temporal::{
    TemporalConfig, TemporalDecodePolicy, TemporalDecoder, TemporalEncoder,
};
use crate::codec::temporal_packet::{
    TemporalPacketProfile, crc32, invert_matrix, systematic_generator_rows,
};
use crate::prng::Prng;
use crate::qr::encode::version_for_number;
use crate::qr::gf256;
use crate::{Grid, Result};

const PAYLOAD_LEN_SIZE: usize = 4; // u32 LE logical payload length
const TILED_STREAM_BLOCK_MAGIC: &[u8; 4] = b"QTT1";
const TILED_STREAM_BLOCK_VERSION: u8 = 1;
const TILED_STREAM_BLOCK_HEADER_LEN: usize = 29;

// ── Tile payload header ────────────────────────────────────────────────

const TILE_CHECK_SIZE: usize = 2; // shard_crc16: u16 LE
const CONTROL_HEADER_LEN: usize = 24; // session_id + block_index + block_count + payload_len + payload_crc32

fn shard_crc16(bytes: &[u8]) -> u16 {
    (crc32(bytes) & 0xffff) as u16
}

fn encode_tile_payload(shard_data: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(TILE_CHECK_SIZE + shard_data.len());
    raw.extend_from_slice(&shard_crc16(shard_data).to_le_bytes());
    raw.extend_from_slice(shard_data);
    raw
}

fn decode_tile_payload(raw: &[u8]) -> Result<Vec<u8>> {
    if raw.len() < TILE_CHECK_SIZE {
        return Err(crate::Error::Codec(
            "tile payload too short for header".into(),
        ));
    }
    let expected_crc = u16::from_le_bytes([raw[0], raw[1]]);
    let shard_data = raw[TILE_CHECK_SIZE..].to_vec();
    let actual_crc = shard_crc16(&shard_data);
    if actual_crc != expected_crc {
        return Err(crate::Error::Codec(format!(
            "tile shard CRC mismatch: expected {expected_crc:#06x}, got {actual_crc:#06x}"
        )));
    }
    Ok(shard_data)
}

fn encode_control_header(header: &TiledStreamBlockHeader) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(CONTROL_HEADER_LEN);
    bytes.extend_from_slice(&header.session_id.to_le_bytes());
    bytes.extend_from_slice(&header.block_index.to_le_bytes());
    bytes.extend_from_slice(&header.block_count.to_le_bytes());
    bytes.extend_from_slice(&header.payload_len.to_le_bytes());
    bytes.extend_from_slice(&header.payload_crc32.to_le_bytes());
    bytes
}

fn decode_control_header(raw: &[u8]) -> Result<TiledStreamBlockHeader> {
    if raw.len() < CONTROL_HEADER_LEN {
        return Err(crate::Error::Codec(format!(
            "control header requires {CONTROL_HEADER_LEN} bytes, got {}",
            raw.len()
        )));
    }

    let mut session_id_bytes = [0u8; 8];
    session_id_bytes.copy_from_slice(&raw[0..8]);
    let mut block_index_bytes = [0u8; 4];
    block_index_bytes.copy_from_slice(&raw[8..12]);
    let mut block_count_bytes = [0u8; 4];
    block_count_bytes.copy_from_slice(&raw[12..16]);
    let mut payload_len_bytes = [0u8; 4];
    payload_len_bytes.copy_from_slice(&raw[16..20]);
    let mut payload_crc_bytes = [0u8; 4];
    payload_crc_bytes.copy_from_slice(&raw[20..24]);

    Ok(TiledStreamBlockHeader {
        version: TILED_STREAM_BLOCK_VERSION,
        session_id: u64::from_le_bytes(session_id_bytes),
        block_index: u32::from_le_bytes(block_index_bytes),
        block_count: u32::from_le_bytes(block_count_bytes),
        payload_len: u32::from_le_bytes(payload_len_bytes),
        payload_crc32: u32::from_le_bytes(payload_crc_bytes),
    })
}

// ── Stream block framing ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TiledStreamBlockHeader {
    pub version: u8,
    pub session_id: u64,
    pub block_index: u32,
    pub block_count: u32,
    pub payload_len: u32,
    pub payload_crc32: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TiledStreamBlock {
    pub header: TiledStreamBlockHeader,
    pub payload: Vec<u8>,
}

impl TiledStreamBlock {
    pub fn new(
        session_id: u64,
        block_index: u32,
        block_count: u32,
        payload: Vec<u8>,
    ) -> Result<Self> {
        let payload_len = u32::try_from(payload.len())
            .map_err(|_| crate::Error::Codec("tiled stream block payload too large".into()))?;
        let header = TiledStreamBlockHeader {
            version: TILED_STREAM_BLOCK_VERSION,
            session_id,
            block_index,
            block_count,
            payload_len,
            payload_crc32: crc32(&payload),
        };
        let block = Self { header, payload };
        block.validate()?;
        Ok(block)
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;

        let mut bytes = Vec::with_capacity(TILED_STREAM_BLOCK_HEADER_LEN + self.payload.len());
        bytes.extend_from_slice(TILED_STREAM_BLOCK_MAGIC);
        bytes.push(self.header.version);
        bytes.extend_from_slice(&self.header.session_id.to_le_bytes());
        bytes.extend_from_slice(&self.header.block_index.to_le_bytes());
        bytes.extend_from_slice(&self.header.block_count.to_le_bytes());
        bytes.extend_from_slice(&self.header.payload_len.to_le_bytes());
        bytes.extend_from_slice(&self.header.payload_crc32.to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() < TILED_STREAM_BLOCK_HEADER_LEN {
            return Err(crate::Error::Codec(format!(
                "tiled stream block requires at least {} bytes, got {}",
                TILED_STREAM_BLOCK_HEADER_LEN,
                encoded.len()
            )));
        }
        if &encoded[..4] != TILED_STREAM_BLOCK_MAGIC {
            return Err(crate::Error::Codec(
                "tiled stream block magic mismatch".into(),
            ));
        }

        let version = encoded[4];
        let mut session_id_bytes = [0u8; 8];
        session_id_bytes.copy_from_slice(&encoded[5..13]);
        let session_id = u64::from_le_bytes(session_id_bytes);

        let mut block_index_bytes = [0u8; 4];
        block_index_bytes.copy_from_slice(&encoded[13..17]);
        let block_index = u32::from_le_bytes(block_index_bytes);

        let mut block_count_bytes = [0u8; 4];
        block_count_bytes.copy_from_slice(&encoded[17..21]);
        let block_count = u32::from_le_bytes(block_count_bytes);

        let mut payload_len_bytes = [0u8; 4];
        payload_len_bytes.copy_from_slice(&encoded[21..25]);
        let payload_len = u32::from_le_bytes(payload_len_bytes);

        let mut payload_crc32_bytes = [0u8; 4];
        payload_crc32_bytes.copy_from_slice(&encoded[25..29]);
        let payload_crc32 = u32::from_le_bytes(payload_crc32_bytes);

        let payload = encoded[29..].to_vec();
        let block = Self {
            header: TiledStreamBlockHeader {
                version,
                session_id,
                block_index,
                block_count,
                payload_len,
                payload_crc32,
            },
            payload,
        };
        block.validate()?;
        Ok(block)
    }

    fn validate(&self) -> Result<()> {
        if self.header.version != TILED_STREAM_BLOCK_VERSION {
            return Err(crate::Error::Codec(format!(
                "unsupported tiled stream block version {}, expected {}",
                self.header.version, TILED_STREAM_BLOCK_VERSION
            )));
        }
        if self.header.block_count == 0 {
            return Err(crate::Error::Codec(
                "tiled stream block requires block_count > 0".into(),
            ));
        }
        if self.header.block_index >= self.header.block_count {
            return Err(crate::Error::Codec(format!(
                "block_index {} must be less than block_count {}",
                self.header.block_index, self.header.block_count
            )));
        }
        if self.payload.len() != self.header.payload_len as usize {
            return Err(crate::Error::Codec(format!(
                "tiled stream block payload length mismatch: header says {}, actual {}",
                self.header.payload_len,
                self.payload.len()
            )));
        }
        if crc32(&self.payload) != self.header.payload_crc32 {
            return Err(crate::Error::Codec(
                "tiled stream block payload CRC mismatch".into(),
            ));
        }
        Ok(())
    }
}

// ── Config ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TiledConfig {
    pub video_shape: (usize, usize),
    pub qr_version: u8,
    pub n_frames: usize,
    pub noise_amplitude: f32,
    pub l1_amplitude: f32,
    pub data_shards: usize,
    pub parity_shards: usize,
}

impl TiledConfig {
    pub fn new(
        video_shape: (usize, usize),
        qr_version: u8,
        n_frames: usize,
        noise_amplitude: f32,
        l1_amplitude: f32,
        data_shards: usize,
        parity_shards: usize,
    ) -> Result<Self> {
        if video_shape.0 == 0 || video_shape.1 == 0 {
            return Err(crate::Error::Codec(
                "video dimensions must be non-zero".into(),
            ));
        }
        if version_for_number(qr_version).is_none() {
            return Err(crate::Error::Codec(format!(
                "unsupported QR version: {} (must be 1-6)",
                qr_version
            )));
        }
        if data_shards == 0 {
            return Err(crate::Error::Codec("data_shards must be > 0".into()));
        }
        if data_shards + parity_shards > 255 {
            return Err(crate::Error::Codec(
                "data_shards + parity_shards must be <= 255".into(),
            ));
        }
        // Validate temporal params by constructing a TemporalConfig
        // Safe: version_for_number was already validated above
        let tile_size = match version_for_number(qr_version) {
            Some(v) => v.size,
            None => {
                return Err(crate::Error::Codec(
                    "unreachable: QR version already validated".into(),
                ));
            }
        };
        TemporalConfig::new(
            (tile_size, tile_size),
            n_frames,
            noise_amplitude,
            l1_amplitude,
        )?;

        let config = Self {
            video_shape,
            qr_version,
            n_frames,
            noise_amplitude,
            l1_amplitude,
            data_shards,
            parity_shards,
        };

        // Validate that one control group plus payload groups fit.
        let layout = compute_layout(&config)?;
        if layout.shard_data_bytes == 0 {
            return Err(crate::Error::Codec(
                "QR version too small for tiled payload header overhead".into(),
            ));
        }
        if layout.n_groups < 2 {
            return Err(crate::Error::Codec(
                "video too small for one control group plus one payload group".into(),
            ));
        }
        if layout.control_data_bytes < CONTROL_HEADER_LEN {
            return Err(crate::Error::Codec(format!(
                "control group capacity {} bytes is smaller than required control header {} bytes",
                layout.control_data_bytes, CONTROL_HEADER_LEN
            )));
        }
        if layout.payload_groups == 0 {
            return Err(crate::Error::Codec(
                "tiled layout has no payload groups after reserving control tiles".into(),
            ));
        }
        Ok(config)
    }
}

// ── Layout ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct TiledLayout {
    pub tile_size: usize,
    pub tiles_x: usize,
    pub tiles_y: usize,
    pub total_tiles: usize,
    pub dead_x: usize,
    pub dead_y: usize,
    pub group_size: usize,
    pub n_groups: usize,
    pub payload_groups: usize,
    pub active_tiles: usize,
    pub shard_data_bytes: usize,
    pub control_data_bytes: usize,
    pub max_payload_bytes: usize,
    /// tile_assignments[tile_index] = Some((group_id, shard_index)) for active tiles, None for inactive.
    pub tile_assignments: Vec<Option<(usize, usize)>>,
}

fn compute_layout(config: &TiledConfig) -> Result<TiledLayout> {
    let version_info = version_for_number(config.qr_version)
        .ok_or_else(|| crate::Error::Codec(format!("unknown QR version: {}", config.qr_version)))?;
    let tile_size = version_info.size;

    let tiles_x = config.video_shape.0 / tile_size;
    let tiles_y = config.video_shape.1 / tile_size;
    let total_tiles = tiles_x * tiles_y;
    let dead_x = config.video_shape.0 % tile_size;
    let dead_y = config.video_shape.1 % tile_size;

    let group_size = config.data_shards + config.parity_shards;
    let n_groups = total_tiles / group_size;
    let active_tiles = n_groups * group_size;

    // Raw byte-mode QR payload capacity, minus the per-tile CRC bytes.
    let qr_capacity_bytes = crate::qr::encode::max_payload_bytes(version_info);
    let shard_data_bytes = qr_capacity_bytes.saturating_sub(TILE_CHECK_SIZE);
    let payload_groups = n_groups.saturating_sub(1);
    let control_data_bytes = config.data_shards * shard_data_bytes;
    let raw_payload_capacity = payload_groups * config.data_shards * shard_data_bytes;
    let max_payload_bytes = raw_payload_capacity.saturating_sub(PAYLOAD_LEN_SIZE);

    // Tile assignments start empty, filled by scatter_assign
    let tile_assignments = vec![None; total_tiles];

    Ok(TiledLayout {
        tile_size,
        tiles_x,
        tiles_y,
        total_tiles,
        dead_x,
        dead_y,
        group_size,
        n_groups,
        payload_groups,
        active_tiles,
        shard_data_bytes,
        control_data_bytes,
        max_payload_bytes,
        tile_assignments,
    })
}

fn scatter_assign(layout: &mut TiledLayout, master_key: &str) {
    if layout.active_tiles == 0 {
        return;
    }

    // Build a permutation of tile indices for scattering
    let mut indices: Vec<usize> = (0..layout.total_tiles).collect();
    let mut rng = Prng::from_str_seed(&format!(
        "qrstatic:temporal:v1:tiled:scatter:{}",
        master_key
    ));

    // Fisher-Yates shuffle
    for i in (1..indices.len()).rev() {
        let j = (rng.next_u64() % (i as u64 + 1)) as usize;
        indices.swap(i, j);
    }

    // Reset all assignments
    for a in layout.tile_assignments.iter_mut() {
        *a = None;
    }

    // Round-robin assign: permuted_tiles[0] → group 0 shard 0,
    // permuted_tiles[1] → group 1 shard 0, etc.
    for (assign_idx, &tile_idx) in indices.iter().take(layout.active_tiles).enumerate() {
        let group_id = assign_idx % layout.n_groups;
        let shard_index = assign_idx / layout.n_groups;
        layout.tile_assignments[tile_idx] = Some((group_id, shard_index));
    }
}

fn tile_origin(tile_index: usize, tiles_x: usize, tile_size: usize) -> (usize, usize) {
    let tile_col = tile_index % tiles_x;
    let tile_row = tile_index / tiles_x;
    (tile_col * tile_size, tile_row * tile_size)
}

fn derive_tile_key(master_key: &str, tile_index: usize) -> String {
    format!("{}:tile:{}", master_key, tile_index)
}

fn validate_tiled_carrier_frames(frames: &[Grid<f32>], config: &TiledConfig) -> Result<()> {
    if frames.len() != config.n_frames {
        return Err(crate::Error::Codec(format!(
            "expected {} carrier frames, got {}",
            config.n_frames,
            frames.len()
        )));
    }
    for frame in frames {
        if frame.width() != config.video_shape.0 || frame.height() != config.video_shape.1 {
            return Err(crate::Error::GridMismatch {
                expected: config.video_shape.0 * config.video_shape.1,
                actual: frame.len(),
            });
        }
    }
    Ok(())
}

// ── RS encoding/decoding at tile level ─────────────────────────────────

fn rs_encode_group(
    data_chunks: &[Vec<u8>],
    data_shards: usize,
    parity_shards: usize,
    shard_data_bytes: usize,
) -> Result<Vec<Vec<u8>>> {
    if parity_shards == 0 {
        return Ok(Vec::new());
    }

    let profile = TemporalPacketProfile::new(data_shards, parity_shards, shard_data_bytes)?;
    let generator = systematic_generator_rows(profile)?;

    let mut parity_chunks = Vec::with_capacity(parity_shards);
    for parity_idx in 0..parity_shards {
        let row = &generator[data_shards + parity_idx];
        let mut parity = vec![0u8; shard_data_bytes];
        for byte_idx in 0..shard_data_bytes {
            let mut acc = 0u8;
            for (data_idx, chunk) in data_chunks.iter().enumerate() {
                let data_byte = if byte_idx < chunk.len() {
                    chunk[byte_idx]
                } else {
                    0
                };
                acc ^= gf256::mul(row[data_idx], data_byte);
            }
            parity[byte_idx] = acc;
        }
        parity_chunks.push(parity);
    }

    Ok(parity_chunks)
}

fn rs_recover_group(
    received: &[(usize, Vec<u8>)], // (shard_index, shard_data)
    data_shards: usize,
    parity_shards: usize,
    shard_data_bytes: usize,
) -> Result<Vec<Vec<u8>>> {
    if received.len() < data_shards {
        return Err(crate::Error::Codec(format!(
            "need at least {} shards for recovery, got {}",
            data_shards,
            received.len()
        )));
    }

    let profile = TemporalPacketProfile::new(data_shards, parity_shards, shard_data_bytes)?;
    let generator = systematic_generator_rows(profile)?;
    let total_shards = data_shards + parity_shards;

    for (shard_idx, payload) in received {
        if *shard_idx >= total_shards {
            return Err(crate::Error::Codec(format!(
                "shard index {} out of range for {} total shards",
                shard_idx, total_shards
            )));
        }
        if payload.len() != shard_data_bytes {
            return Err(crate::Error::Codec(format!(
                "shard {} expected {} bytes, got {}",
                shard_idx,
                shard_data_bytes,
                payload.len()
            )));
        }
    }

    // Select the first data_shards received shards
    let selected: Vec<_> = received.iter().take(data_shards).collect();

    // Build the decode matrix from generator rows of selected shards
    let decode_matrix: Vec<Vec<u8>> = selected
        .iter()
        .map(|(shard_idx, _)| generator[*shard_idx].clone())
        .collect();

    let inverse = invert_matrix(&decode_matrix)?;

    // Recover data shards
    let mut recovered = Vec::with_capacity(data_shards);
    for inverse_row in inverse.iter().take(data_shards) {
        let mut shard = vec![0u8; shard_data_bytes];
        for byte_idx in 0..shard_data_bytes {
            let mut acc = 0u8;
            for (row_idx, (_, payload)) in selected.iter().enumerate() {
                acc ^= gf256::mul(inverse_row[row_idx], payload[byte_idx]);
            }
            shard[byte_idx] = acc;
        }
        recovered.push(shard);
    }

    Ok(recovered)
}

fn split_data_chunks(data: &[u8], data_shards: usize, shard_data_bytes: usize) -> Vec<Vec<u8>> {
    let mut chunks = Vec::with_capacity(data_shards);
    for shard_idx in 0..data_shards {
        let chunk_offset = shard_idx * shard_data_bytes;
        let mut chunk = vec![0u8; shard_data_bytes];
        if chunk_offset < data.len() {
            let chunk_end = (chunk_offset + shard_data_bytes).min(data.len());
            let src = &data[chunk_offset..chunk_end];
            chunk[..src.len()].copy_from_slice(src);
        }
        chunks.push(chunk);
    }
    chunks
}

fn assemble_group_shards(
    data_chunks: &[Vec<u8>],
    data_shards: usize,
    parity_shards: usize,
    shard_data_bytes: usize,
) -> Result<Vec<Vec<u8>>> {
    let parity_chunks = rs_encode_group(data_chunks, data_shards, parity_shards, shard_data_bytes)?;
    let mut shards = Vec::with_capacity(data_shards + parity_shards);
    shards.extend(data_chunks.iter().cloned());
    shards.extend(parity_chunks);
    Ok(shards)
}

// ── Encoder ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TiledEncoder {
    config: TiledConfig,
    layout: TiledLayout,
}

impl TiledEncoder {
    pub fn new(config: TiledConfig, master_key: &str) -> Result<Self> {
        let mut layout = compute_layout(&config)?;
        scatter_assign(&mut layout, master_key);
        Ok(Self { config, layout })
    }

    pub fn config(&self) -> &TiledConfig {
        &self.config
    }
    pub fn layout(&self) -> &TiledLayout {
        &self.layout
    }

    pub fn encode_payload(&self, master_key: &str, payload: &[u8]) -> Result<Vec<Grid<f32>>> {
        let header = TiledStreamBlockHeader {
            version: TILED_STREAM_BLOCK_VERSION,
            session_id: 0,
            block_index: 0,
            block_count: 1,
            payload_len: u32::try_from(payload.len())
                .map_err(|_| crate::Error::Codec("payload too large for tiled stream".into()))?,
            payload_crc32: crc32(payload),
        };
        let tile_payloads = self.build_tile_payloads(payload, &header)?;
        let all_tile_frames = self.encode_tile_frames(master_key, &tile_payloads, false)?;
        Ok(self.compose_standalone_frames(master_key, &all_tile_frames))
    }

    pub fn encode_payload_over_carrier(
        &self,
        master_key: &str,
        payload: &[u8],
        carrier_frames: &[Grid<f32>],
        clip_limit: f32,
    ) -> Result<Vec<Grid<f32>>> {
        if clip_limit <= 0.0 {
            return Err(crate::Error::Codec(
                "clip_limit must be greater than zero".into(),
            ));
        }
        validate_tiled_carrier_frames(carrier_frames, &self.config)?;
        let header = TiledStreamBlockHeader {
            version: TILED_STREAM_BLOCK_VERSION,
            session_id: 0,
            block_index: 0,
            block_count: 1,
            payload_len: u32::try_from(payload.len())
                .map_err(|_| crate::Error::Codec("payload too large for tiled stream".into()))?,
            payload_crc32: crc32(payload),
        };
        let tile_payloads = self.build_tile_payloads(payload, &header)?;
        let all_tile_frames = self.encode_tile_frames(master_key, &tile_payloads, true)?;
        self.overlay_on_carrier_frames(&all_tile_frames, carrier_frames, clip_limit)
    }

    fn build_tile_payloads(
        &self,
        payload: &[u8],
        header: &TiledStreamBlockHeader,
    ) -> Result<Vec<Option<Vec<u8>>>> {
        let layout = &self.layout;
        if payload.len() > layout.max_payload_bytes {
            return Err(crate::Error::Codec(format!(
                "payload {} bytes exceeds tiled capacity {} bytes",
                payload.len(),
                layout.max_payload_bytes
            )));
        }

        let group_payload_size = self.config.data_shards * layout.shard_data_bytes;
        let logical_len = u32::try_from(payload.len())
            .map_err(|_| crate::Error::Codec("payload too large for tiled length header".into()))?;
        let mut framed_payload = Vec::with_capacity(PAYLOAD_LEN_SIZE + payload.len());
        framed_payload.extend_from_slice(&logical_len.to_le_bytes());
        framed_payload.extend_from_slice(payload);

        let mut group_shards: Vec<Vec<Vec<u8>>> = Vec::with_capacity(layout.n_groups);

        let control_chunks = split_data_chunks(
            &encode_control_header(header),
            self.config.data_shards,
            layout.shard_data_bytes,
        );
        group_shards.push(assemble_group_shards(
            &control_chunks,
            self.config.data_shards,
            self.config.parity_shards,
            layout.shard_data_bytes,
        )?);

        for payload_group in 0..layout.payload_groups {
            let group_offset = payload_group * group_payload_size;
            let group_end = (group_offset + group_payload_size).min(framed_payload.len());
            let group_data = if group_offset < framed_payload.len() {
                &framed_payload[group_offset..group_end]
            } else {
                &[]
            };
            let data_chunks =
                split_data_chunks(group_data, self.config.data_shards, layout.shard_data_bytes);
            group_shards.push(assemble_group_shards(
                &data_chunks,
                self.config.data_shards,
                self.config.parity_shards,
                layout.shard_data_bytes,
            )?);
        }

        let mut tile_payloads: Vec<Option<Vec<u8>>> = vec![None; layout.total_tiles];
        for (tile_idx, assignment) in layout.tile_assignments.iter().enumerate() {
            if let Some((group_id, shard_idx)) = assignment {
                tile_payloads[tile_idx] =
                    Some(encode_tile_payload(&group_shards[*group_id][*shard_idx]));
            }
        }
        Ok(tile_payloads)
    }

    fn encode_tile_frames(
        &self,
        master_key: &str,
        tile_payloads: &[Option<Vec<u8>>],
        signal_only: bool,
    ) -> Result<Vec<Option<Vec<Grid<f32>>>>> {
        let layout = &self.layout;
        let tile_config = TemporalConfig::new(
            (layout.tile_size, layout.tile_size),
            self.config.n_frames,
            self.config.noise_amplitude,
            self.config.l1_amplitude,
        )?;
        let tile_encoder = TemporalEncoder::new(tile_config)?;

        // Encode each active tile's frames
        let mut all_tile_frames: Vec<Option<Vec<Grid<f32>>>> = vec![None; layout.total_tiles];
        for (tile_idx, qr_payload) in tile_payloads.iter().enumerate() {
            if let Some(payload_bytes) = qr_payload {
                let tile_key = derive_tile_key(master_key, tile_idx);
                let qr_grid = crate::qr::encode::encode_bytes(payload_bytes)?;
                let frames = if signal_only {
                    tile_encoder.encode_qr_signal(&tile_key, &qr_grid)?
                } else {
                    tile_encoder.encode_qr(&tile_key, &qr_grid)?
                };
                all_tile_frames[tile_idx] = Some(frames);
            }
        }

        Ok(all_tile_frames)
    }

    fn compose_standalone_frames(
        &self,
        master_key: &str,
        all_tile_frames: &[Option<Vec<Grid<f32>>>],
    ) -> Vec<Grid<f32>> {
        let layout = &self.layout;

        // Compose into video-sized frames
        let (video_w, video_h) = self.config.video_shape;
        let mut video_frames = Vec::with_capacity(self.config.n_frames);

        for frame_idx in 0..self.config.n_frames {
            let mut video_frame = Grid::new(video_w, video_h);

            // Fill with noise for dead zones and inactive tiles
            let mut rng = Prng::from_str_seed(&format!(
                "qrstatic:temporal:v1:tiled:deadzone:{}:frame:{}",
                master_key, frame_idx
            ));
            for val in video_frame.data_mut() {
                *val = rng.next_range(-self.config.noise_amplitude, self.config.noise_amplitude);
            }

            // Overlay tile frames
            for (tile_idx, tile_frames) in
                all_tile_frames.iter().enumerate().take(layout.total_tiles)
            {
                if let Some(frames) = tile_frames {
                    let (ox, oy) = tile_origin(tile_idx, layout.tiles_x, layout.tile_size);
                    let tile_frame = &frames[frame_idx];
                    for row in 0..layout.tile_size {
                        for col in 0..layout.tile_size {
                            video_frame[(oy + row, ox + col)] = tile_frame[(row, col)];
                        }
                    }
                }
            }

            video_frames.push(video_frame);
        }

        video_frames
    }

    fn overlay_on_carrier_frames(
        &self,
        all_tile_frames: &[Option<Vec<Grid<f32>>>],
        carrier_frames: &[Grid<f32>],
        clip_limit: f32,
    ) -> Result<Vec<Grid<f32>>> {
        let layout = &self.layout;
        let mut output = Vec::with_capacity(self.config.n_frames);

        for (frame_idx, carrier_frame) in carrier_frames.iter().enumerate() {
            let mut frame = carrier_frame.clone();

            for (tile_idx, tile_frames) in
                all_tile_frames.iter().enumerate().take(layout.total_tiles)
            {
                if let Some(frames) = tile_frames {
                    let (ox, oy) = tile_origin(tile_idx, layout.tiles_x, layout.tile_size);
                    let tile_frame = &frames[frame_idx];
                    for row in 0..layout.tile_size {
                        for col in 0..layout.tile_size {
                            let value = frame[(oy + row, ox + col)] + tile_frame[(row, col)];
                            frame[(oy + row, ox + col)] = value.clamp(-clip_limit, clip_limit);
                        }
                    }
                }
            }

            output.push(frame);
        }

        Ok(output)
    }

    pub fn encode_stream_block(
        &self,
        master_key: &str,
        block: &TiledStreamBlock,
    ) -> Result<Vec<Grid<f32>>> {
        let tile_payloads = self.build_tile_payloads(&block.payload, &block.header)?;
        let all_tile_frames = self.encode_tile_frames(master_key, &tile_payloads, false)?;
        Ok(self.compose_standalone_frames(master_key, &all_tile_frames))
    }

    pub fn encode_stream_block_over_carrier(
        &self,
        master_key: &str,
        block: &TiledStreamBlock,
        carrier_frames: &[Grid<f32>],
        clip_limit: f32,
    ) -> Result<Vec<Grid<f32>>> {
        if clip_limit <= 0.0 {
            return Err(crate::Error::Codec(
                "clip_limit must be greater than zero".into(),
            ));
        }
        validate_tiled_carrier_frames(carrier_frames, &self.config)?;
        let tile_payloads = self.build_tile_payloads(&block.payload, &block.header)?;
        let all_tile_frames = self.encode_tile_frames(master_key, &tile_payloads, true)?;
        self.overlay_on_carrier_frames(&all_tile_frames, carrier_frames, clip_limit)
    }
}

// ── Decoder ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TileDecodeOutcome {
    Success {
        detector_score: f32,
        shard_recovered: bool,
    },
    Failed {
        detector_score: f32,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupRecoveryOutcome {
    pub group_id: usize,
    pub shards_received: usize,
    pub shards_needed: usize,
    pub recovered: bool,
}

#[derive(Debug, Clone)]
pub struct TiledDecodeResult {
    pub tile_results: Vec<TileDecodeOutcome>,
    pub tiles_decoded: usize,
    pub tiles_total: usize,
    pub group_results: Vec<GroupRecoveryOutcome>,
    pub payload: Option<Vec<u8>>,
    pub stream_block: Option<TiledStreamBlock>,
}

#[derive(Debug, Clone)]
pub struct TiledDecoder {
    config: TiledConfig,
    layout: TiledLayout,
}

impl TiledDecoder {
    pub fn new(config: TiledConfig, master_key: &str) -> Result<Self> {
        let mut layout = compute_layout(&config)?;
        scatter_assign(&mut layout, master_key);
        Ok(Self { config, layout })
    }

    pub fn config(&self) -> &TiledConfig {
        &self.config
    }
    pub fn layout(&self) -> &TiledLayout {
        &self.layout
    }

    pub fn decode_payload(
        &self,
        frames: &[Grid<f32>],
        master_key: &str,
        policy: &TemporalDecodePolicy,
    ) -> Result<TiledDecodeResult> {
        let layout = &self.layout;

        if frames.len() != self.config.n_frames {
            return Err(crate::Error::Codec(format!(
                "expected {} frames, got {}",
                self.config.n_frames,
                frames.len()
            )));
        }

        let tile_config = TemporalConfig::new(
            (layout.tile_size, layout.tile_size),
            self.config.n_frames,
            self.config.noise_amplitude,
            self.config.l1_amplitude,
        )?;
        let tile_decoder = TemporalDecoder::new(tile_config)?;

        // Decode each active tile.
        let mut tile_results = Vec::with_capacity(layout.total_tiles);
        // Collect decoded shards per group as unique shard slots.
        let mut group_shards: Vec<Vec<Option<Vec<u8>>>> =
            vec![vec![None; layout.group_size]; layout.n_groups];
        let mut tiles_decoded = 0usize;

        for tile_idx in 0..layout.total_tiles {
            if layout.tile_assignments[tile_idx].is_none() {
                tile_results.push(TileDecodeOutcome::Failed {
                    detector_score: 0.0,
                });
                continue;
            }

            // Extract tile sub-frames
            let (ox, oy) = tile_origin(tile_idx, layout.tiles_x, layout.tile_size);
            let mut tile_frames = Vec::with_capacity(self.config.n_frames);
            for frame in frames {
                let mut tile_data = Vec::with_capacity(layout.tile_size * layout.tile_size);
                for row in 0..layout.tile_size {
                    for col in 0..layout.tile_size {
                        tile_data.push(frame[(oy + row, ox + col)]);
                    }
                }
                tile_frames.push(Grid::from_vec(
                    tile_data,
                    layout.tile_size,
                    layout.tile_size,
                ));
            }

            let tile_key = derive_tile_key(master_key, tile_idx);
            match tile_decoder.decode_qr(&tile_frames, &tile_key, policy) {
                Ok(decode_result) => {
                    let mut shard_recovered = false;
                    if let Ok(raw_payload) = crate::qr::decode::decode_bytes(&decode_result.qr)
                        && let Ok(shard_data) = decode_tile_payload(&raw_payload)
                        && shard_data.len() == layout.shard_data_bytes
                        && let Some((group_id, shard_id)) = layout.tile_assignments[tile_idx]
                    {
                        group_shards[group_id][shard_id].get_or_insert(shard_data);
                        shard_recovered = true;
                        tiles_decoded += 1;
                    }
                    tile_results.push(TileDecodeOutcome::Success {
                        detector_score: decode_result.detector_score,
                        shard_recovered,
                    });
                }
                Err(_) => {
                    tile_results.push(TileDecodeOutcome::Failed {
                        detector_score: 0.0,
                    });
                }
            }
        }

        // Recover the reserved control group first.
        let mut group_results = Vec::with_capacity(layout.n_groups);
        let control_group = group_shards[0]
            .iter()
            .enumerate()
            .filter_map(|(sid, data)| data.as_ref().map(|payload| (sid, payload.clone())))
            .collect::<Vec<_>>();
        let mut payload = None;
        let mut stream_block = None;

        let control_header = if control_group.len() >= self.config.data_shards {
            match rs_recover_group(
                &control_group,
                self.config.data_shards,
                self.config.parity_shards,
                layout.shard_data_bytes,
            ) {
                Ok(recovered) => {
                    group_results.push(GroupRecoveryOutcome {
                        group_id: 0,
                        shards_received: control_group.len(),
                        shards_needed: self.config.data_shards,
                        recovered: true,
                    });
                    let mut control_bytes = Vec::with_capacity(layout.control_data_bytes);
                    for shard in recovered {
                        control_bytes.extend_from_slice(&shard);
                    }
                    decode_control_header(&control_bytes).ok()
                }
                Err(_) => {
                    group_results.push(GroupRecoveryOutcome {
                        group_id: 0,
                        shards_received: control_group.len(),
                        shards_needed: self.config.data_shards,
                        recovered: false,
                    });
                    None
                }
            }
        } else {
            group_results.push(GroupRecoveryOutcome {
                group_id: 0,
                shards_received: control_group.len(),
                shards_needed: self.config.data_shards,
                recovered: false,
            });
            None
        };

        if let Some(control_header) = control_header {
            let mut all_recovered = true;
            let mut payload_groups: Vec<Option<Vec<Vec<u8>>>> = vec![None; layout.payload_groups];

            for group_id in 1..layout.n_groups {
                let shards: Vec<(usize, Vec<u8>)> = group_shards[group_id]
                    .iter()
                    .enumerate()
                    .filter_map(|(sid, data)| data.as_ref().map(|payload| (sid, payload.clone())))
                    .collect();
                let shards_received = shards.len();
                let shards_needed = self.config.data_shards;

                if shards_received >= shards_needed {
                    match rs_recover_group(
                        &shards,
                        self.config.data_shards,
                        self.config.parity_shards,
                        layout.shard_data_bytes,
                    ) {
                        Ok(recovered) => {
                            group_results.push(GroupRecoveryOutcome {
                                group_id,
                                shards_received,
                                shards_needed,
                                recovered: true,
                            });
                            payload_groups[group_id - 1] = Some(recovered);
                        }
                        Err(_) => {
                            group_results.push(GroupRecoveryOutcome {
                                group_id,
                                shards_received,
                                shards_needed,
                                recovered: false,
                            });
                            all_recovered = false;
                        }
                    }
                } else {
                    group_results.push(GroupRecoveryOutcome {
                        group_id,
                        shards_received,
                        shards_needed,
                        recovered: false,
                    });
                    all_recovered = false;
                }
            }

            if all_recovered {
                let mut data = Vec::with_capacity(layout.max_payload_bytes + PAYLOAD_LEN_SIZE);
                for shards in payload_groups.iter().flatten() {
                    for shard in shards {
                        data.extend_from_slice(shard);
                    }
                }

                if data.len() >= PAYLOAD_LEN_SIZE {
                    let logical_len =
                        u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
                    let available = data.len().saturating_sub(PAYLOAD_LEN_SIZE);
                    if logical_len <= available {
                        let recovered_payload =
                            data[PAYLOAD_LEN_SIZE..PAYLOAD_LEN_SIZE + logical_len].to_vec();
                        if crc32(&recovered_payload) == control_header.payload_crc32
                            && control_header.payload_len as usize == recovered_payload.len()
                        {
                            let block = TiledStreamBlock {
                                header: control_header,
                                payload: recovered_payload.clone(),
                            };
                            if block.validate().is_ok() {
                                payload = Some(recovered_payload);
                                stream_block = Some(block);
                            }
                        }
                    }
                }
            }
        } else {
            for (group_id, shard_slots) in group_shards
                .iter()
                .enumerate()
                .take(layout.n_groups)
                .skip(1)
            {
                let shards_received = shard_slots.iter().filter(|data| data.is_some()).count();
                group_results.push(GroupRecoveryOutcome {
                    group_id,
                    shards_received,
                    shards_needed: self.config.data_shards,
                    recovered: false,
                });
            }
        }

        Ok(TiledDecodeResult {
            tile_results,
            tiles_decoded,
            tiles_total: layout.total_tiles,
            group_results,
            payload,
            stream_block,
        })
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_payload_roundtrip() {
        let data = vec![0x42, 0x55, 0x99];
        let encoded = encode_tile_payload(&data);
        let decoded = decode_tile_payload(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn control_header_roundtrip() {
        let header = TiledStreamBlockHeader {
            version: TILED_STREAM_BLOCK_VERSION,
            session_id: 0x1122_3344_5566_7788,
            block_index: 7,
            block_count: 11,
            payload_len: 123,
            payload_crc32: 0xaabb_ccdd,
        };
        let encoded = encode_control_header(&header);
        let decoded = decode_control_header(&encoded).unwrap();
        assert_eq!(decoded, header);
    }

    #[test]
    fn stream_block_roundtrip() {
        let payload = b"hello tiled stream".to_vec();
        let block = TiledStreamBlock::new(0x1122_3344_5566_7788, 2, 9, payload.clone()).unwrap();
        let encoded = block.encode().unwrap();
        let decoded = TiledStreamBlock::decode(&encoded).unwrap();
        assert_eq!(decoded, block);
        assert_eq!(decoded.payload, payload);
    }

    #[test]
    fn stream_block_rejects_corruption() {
        let block = TiledStreamBlock::new(7, 0, 1, b"payload".to_vec()).unwrap();
        let mut encoded = block.encode().unwrap();
        *encoded.last_mut().unwrap() ^= 0x01;
        assert!(TiledStreamBlock::decode(&encoded).is_err());
    }

    #[test]
    fn layout_1920x1080_v2_3of5() {
        let config = TiledConfig::new((1920, 1080), 2, 64, 0.42, 0.22, 3, 2).unwrap();
        let layout = compute_layout(&config).unwrap();
        assert_eq!(layout.tile_size, 25);
        assert_eq!(layout.tiles_x, 76);
        assert_eq!(layout.tiles_y, 43);
        assert_eq!(layout.total_tiles, 3268);
        assert_eq!(layout.dead_x, 20);
        assert_eq!(layout.dead_y, 5);
        assert_eq!(layout.group_size, 5);
        assert_eq!(layout.n_groups, 653);
        assert_eq!(layout.payload_groups, 652);
        assert_eq!(layout.active_tiles, 3265);
        assert_eq!(layout.shard_data_bytes, 12);
        assert_eq!(layout.control_data_bytes, 36);
        assert_eq!(layout.max_payload_bytes, 652 * 3 * 12 - PAYLOAD_LEN_SIZE);
    }

    #[test]
    fn layout_small_100x100_v2() {
        let config = TiledConfig::new((100, 100), 2, 64, 0.42, 0.22, 2, 1).unwrap();
        let layout = compute_layout(&config).unwrap();
        assert_eq!(layout.tile_size, 25);
        assert_eq!(layout.tiles_x, 4);
        assert_eq!(layout.tiles_y, 4);
        assert_eq!(layout.total_tiles, 16);
        assert_eq!(layout.group_size, 3);
        assert_eq!(layout.n_groups, 5);
        assert_eq!(layout.payload_groups, 4);
        assert_eq!(layout.active_tiles, 15);
        assert_eq!(layout.shard_data_bytes, 12);
        assert_eq!(layout.control_data_bytes, 24);
        assert_eq!(layout.max_payload_bytes, 4 * 2 * 12 - PAYLOAD_LEN_SIZE);
    }

    #[test]
    fn v1_too_small_for_tiling() {
        // QR v1 cannot carry the reserved control header across the data shards.
        let result = TiledConfig::new((100, 100), 1, 64, 0.42, 0.22, 2, 1);
        assert!(result.is_err());
    }

    #[test]
    fn scatter_distributes_groups() {
        let config = TiledConfig::new((200, 200), 2, 64, 0.42, 0.22, 3, 2).unwrap();
        let mut layout = compute_layout(&config).unwrap();
        scatter_assign(&mut layout, "test-key");

        // Verify every active tile has an assignment
        let assigned: Vec<_> = layout
            .tile_assignments
            .iter()
            .filter(|a| a.is_some())
            .collect();
        assert_eq!(assigned.len(), layout.active_tiles);

        // Verify each group has exactly group_size members
        for group_id in 0..layout.n_groups {
            let count = layout
                .tile_assignments
                .iter()
                .filter(|a| matches!(a, Some((gid, _)) if *gid == group_id))
                .count();
            assert_eq!(count, layout.group_size);
        }

        // Verify each shard_index 0..group_size appears exactly once per group
        for group_id in 0..layout.n_groups {
            let mut shard_indices: Vec<usize> = layout
                .tile_assignments
                .iter()
                .filter_map(|a| match a {
                    Some((gid, sid)) if *gid == group_id => Some(*sid),
                    _ => None,
                })
                .collect();
            shard_indices.sort();
            let expected: Vec<usize> = (0..layout.group_size).collect();
            assert_eq!(shard_indices, expected);
        }
    }

    #[test]
    fn rs_encode_decode_roundtrip() {
        let data_shards = 3;
        let parity_shards = 2;
        let shard_size = 4;

        let data_chunks: Vec<Vec<u8>> = vec![
            vec![0x01, 0x02, 0x03, 0x04],
            vec![0x05, 0x06, 0x07, 0x08],
            vec![0x09, 0x0a, 0x0b, 0x0c],
        ];

        let parity = rs_encode_group(&data_chunks, data_shards, parity_shards, shard_size).unwrap();
        assert_eq!(parity.len(), 2);

        // Recover using only 2 data + 1 parity (drop shard 1)
        let received: Vec<(usize, Vec<u8>)> = vec![
            (0, data_chunks[0].clone()),
            (2, data_chunks[2].clone()),
            (3, parity[0].clone()), // first parity shard
        ];

        let recovered =
            rs_recover_group(&received, data_shards, parity_shards, shard_size).unwrap();
        assert_eq!(recovered[0], data_chunks[0]);
        assert_eq!(recovered[1], data_chunks[1]);
        assert_eq!(recovered[2], data_chunks[2]);
    }

    #[test]
    fn rs_recover_rejects_out_of_range_shard_index() {
        let err = rs_recover_group(&[(99, vec![1, 2, 3, 4])], 1, 1, 4).unwrap_err();
        assert!(err.to_string().contains("out of range"));
    }

    #[test]
    fn tile_payload_crc_rejects_corruption() {
        let encoded = encode_tile_payload(b"hello");
        let mut corrupted = encoded;
        let last = corrupted.len() - 1;
        corrupted[last] ^= 0x01;
        assert!(decode_tile_payload(&corrupted).is_err());
    }

    #[test]
    fn small_encode_decode_roundtrip() {
        // 75x75 video, QR v2 (25x25) = 3x3 = 9 tiles
        // RS: 2 data + 1 parity = 3 per group → 3 groups
        // shard_data_bytes = 12 with raw tile payloads and CRC16 headers
        // Use proven temporal baseline: 64 frames, 0.42/0.22 amplitudes
        let config = TiledConfig::new((75, 75), 2, 64, 0.42, 0.22, 2, 1).unwrap();
        let master_key = "test-tiled";

        let encoder = TiledEncoder::new(config.clone(), master_key).unwrap();
        let layout = encoder.layout();
        assert_eq!(layout.tiles_x, 3);
        assert_eq!(layout.tiles_y, 3);
        assert_eq!(layout.n_groups, 3);
        assert_eq!(layout.payload_groups, 2);
        assert_eq!(layout.shard_data_bytes, 12);

        // Create a payload that fits
        let payload: Vec<u8> = (0..layout.max_payload_bytes)
            .map(|i| (i & 0xff) as u8)
            .collect();

        let frames = encoder.encode_payload(master_key, &payload).unwrap();
        assert_eq!(frames.len(), 64);
        assert_eq!(frames[0].width(), 75);
        assert_eq!(frames[0].height(), 75);

        // Decode
        let policy = TemporalDecodePolicy::fixed_threshold(6.0).unwrap();
        let decoder = TiledDecoder::new(config, master_key).unwrap();
        let result = decoder
            .decode_payload(&frames, master_key, &policy)
            .unwrap();

        // At these small sizes with no compression, all tiles should decode
        assert!(result.tiles_decoded > 0, "no tiles decoded");

        if let Some(recovered) = &result.payload {
            assert_eq!(&recovered[..payload.len()], &payload[..]);
        } else {
            // Print diagnostics
            for (i, gr) in result.group_results.iter().enumerate() {
                eprintln!(
                    "group {}: received={} needed={} recovered={}",
                    i, gr.shards_received, gr.shards_needed, gr.recovered
                );
            }
            panic!("payload recovery failed");
        }
    }

    #[test]
    fn tiled_decode_preserves_logical_payload_length() {
        let config = TiledConfig::new((75, 75), 2, 64, 0.42, 0.22, 2, 1).unwrap();
        let master_key = "test-tiled-short";

        let encoder = TiledEncoder::new(config.clone(), master_key).unwrap();
        let payload = b"short payload".to_vec();
        let frames = encoder.encode_payload(master_key, &payload).unwrap();

        let policy = TemporalDecodePolicy::fixed_threshold(6.0).unwrap();
        let decoder = TiledDecoder::new(config, master_key).unwrap();
        let result = decoder
            .decode_payload(&frames, master_key, &policy)
            .unwrap();

        assert_eq!(result.payload, Some(payload));
    }

    #[test]
    fn tiled_stream_block_roundtrip() {
        let config = TiledConfig::new((100, 100), 2, 64, 0.42, 0.22, 2, 1).unwrap();
        let master_key = "test-tiled-stream";
        let encoder = TiledEncoder::new(config.clone(), master_key).unwrap();
        let block = TiledStreamBlock::new(42, 1, 3, b"stream payload".to_vec()).unwrap();

        let frames = encoder.encode_stream_block(master_key, &block).unwrap();

        let policy = TemporalDecodePolicy::fixed_threshold(6.0).unwrap();
        let decoder = TiledDecoder::new(config, master_key).unwrap();
        let result = decoder
            .decode_payload(&frames, master_key, &policy)
            .unwrap();

        assert_eq!(result.stream_block, Some(block));
    }

    #[test]
    fn tiled_stream_block_roundtrip_over_flat_carrier() {
        let config = TiledConfig::new((100, 100), 2, 64, 0.42, 0.22, 2, 1).unwrap();
        let master_key = "test-tiled-overlay";
        let encoder = TiledEncoder::new(config.clone(), master_key).unwrap();
        let block = TiledStreamBlock::new(99, 0, 2, b"overlay payload".to_vec()).unwrap();
        let carrier_frames = vec![Grid::new(100, 100); 64];

        let frames = encoder
            .encode_stream_block_over_carrier(master_key, &block, &carrier_frames, 1.0)
            .unwrap();

        let policy = TemporalDecodePolicy::fixed_threshold(6.0).unwrap();
        let decoder = TiledDecoder::new(config, master_key).unwrap();
        let result = decoder
            .decode_payload(&frames, master_key, &policy)
            .unwrap();

        assert_eq!(result.stream_block, Some(block));
    }
}
