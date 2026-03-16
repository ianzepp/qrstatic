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

// ── Base64url utilities ────────────────────────────────────────────────

const BASE64URL_CHARS: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const PAYLOAD_LEN_SIZE: usize = 4; // u32 LE logical payload length

fn base64url_encoded_len(raw_len: usize) -> usize {
    let full_chunks = raw_len / 3;
    let remainder = raw_len % 3;
    full_chunks * 4
        + match remainder {
            0 => 0,
            1 => 2,
            2 => 3,
            _ => 0,
        }
}

fn base64url_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(base64url_encoded_len(bytes.len()));
    let mut i = 0usize;
    while i + 3 <= bytes.len() {
        let chunk =
            ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | (bytes[i + 2] as u32);
        s.push(BASE64URL_CHARS[((chunk >> 18) & 0x3f) as usize] as char);
        s.push(BASE64URL_CHARS[((chunk >> 12) & 0x3f) as usize] as char);
        s.push(BASE64URL_CHARS[((chunk >> 6) & 0x3f) as usize] as char);
        s.push(BASE64URL_CHARS[(chunk & 0x3f) as usize] as char);
        i += 3;
    }

    match bytes.len() - i {
        1 => {
            let chunk = (bytes[i] as u32) << 16;
            s.push(BASE64URL_CHARS[((chunk >> 18) & 0x3f) as usize] as char);
            s.push(BASE64URL_CHARS[((chunk >> 12) & 0x3f) as usize] as char);
        }
        2 => {
            let chunk = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
            s.push(BASE64URL_CHARS[((chunk >> 18) & 0x3f) as usize] as char);
            s.push(BASE64URL_CHARS[((chunk >> 12) & 0x3f) as usize] as char);
            s.push(BASE64URL_CHARS[((chunk >> 6) & 0x3f) as usize] as char);
        }
        _ => {}
    }

    s
}

fn base64url_decode(s: &str) -> Result<Vec<u8>> {
    if s.len() % 4 == 1 {
        return Err(crate::Error::Codec(
            "base64url string has invalid length".into(),
        ));
    }

    let mut out = Vec::with_capacity((s.len() * 3) / 4);
    let bytes = s.as_bytes();
    let mut i = 0usize;

    while i + 4 <= bytes.len() {
        let a = base64url_value(bytes[i])? as u32;
        let b = base64url_value(bytes[i + 1])? as u32;
        let c = base64url_value(bytes[i + 2])? as u32;
        let d = base64url_value(bytes[i + 3])? as u32;
        let chunk = (a << 18) | (b << 12) | (c << 6) | d;
        out.push(((chunk >> 16) & 0xff) as u8);
        out.push(((chunk >> 8) & 0xff) as u8);
        out.push((chunk & 0xff) as u8);
        i += 4;
    }

    match bytes.len() - i {
        0 => {}
        2 => {
            let a = base64url_value(bytes[i])? as u32;
            let b = base64url_value(bytes[i + 1])? as u32;
            let chunk = (a << 18) | (b << 12);
            out.push(((chunk >> 16) & 0xff) as u8);
        }
        3 => {
            let a = base64url_value(bytes[i])? as u32;
            let b = base64url_value(bytes[i + 1])? as u32;
            let c = base64url_value(bytes[i + 2])? as u32;
            let chunk = (a << 18) | (b << 12) | (c << 6);
            out.push(((chunk >> 16) & 0xff) as u8);
            out.push(((chunk >> 8) & 0xff) as u8);
        }
        _ => {
            return Err(crate::Error::Codec(
                "base64url string has invalid trailing length".into(),
            ));
        }
    }

    Ok(out)
}

fn base64url_value(b: u8) -> Result<u8> {
    match b {
        b'A'..=b'Z' => Ok(b - b'A'),
        b'a'..=b'z' => Ok(b - b'a' + 26),
        b'0'..=b'9' => Ok(b - b'0' + 52),
        b'-' => Ok(62),
        b'_' => Ok(63),
        _ => Err(crate::Error::Codec(format!(
            "invalid base64url char: {}",
            b as char
        ))),
    }
}

// ── Tile payload header ────────────────────────────────────────────────

const TILE_HEADER_SIZE: usize = 5; // group_id: u16 LE + shard_id: u8 + shard_crc16: u16 LE

fn shard_crc16(bytes: &[u8]) -> u16 {
    (crc32(bytes) & 0xffff) as u16
}

fn encode_tile_payload(group_id: u16, shard_id: u8, shard_data: &[u8]) -> String {
    let mut raw = Vec::with_capacity(TILE_HEADER_SIZE + shard_data.len());
    raw.extend_from_slice(&group_id.to_le_bytes());
    raw.push(shard_id);
    raw.extend_from_slice(&shard_crc16(shard_data).to_le_bytes());
    raw.extend_from_slice(shard_data);
    base64url_encode(&raw)
}

fn decode_tile_payload(encoded: &str) -> Result<(u16, u8, Vec<u8>)> {
    let raw = base64url_decode(encoded)?;
    if raw.len() < TILE_HEADER_SIZE {
        return Err(crate::Error::Codec(
            "tile payload too short for header".into(),
        ));
    }
    let group_id = u16::from_le_bytes([raw[0], raw[1]]);
    let shard_id = raw[2];
    let expected_crc = u16::from_le_bytes([raw[3], raw[4]]);
    let shard_data = raw[TILE_HEADER_SIZE..].to_vec();
    let actual_crc = shard_crc16(&shard_data);
    if actual_crc != expected_crc {
        return Err(crate::Error::Codec(format!(
            "tile shard CRC mismatch: expected {expected_crc:#06x}, got {actual_crc:#06x}"
        )));
    }
    Ok((group_id, shard_id, shard_data))
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

        // Validate that at least one group fits
        let layout = compute_layout(&config)?;
        if layout.shard_data_bytes == 0 {
            return Err(crate::Error::Codec(
                "QR version too small for tiled payload header overhead".into(),
            ));
        }
        if layout.n_groups == 0 {
            return Err(crate::Error::Codec(
                "video too small for even one RS group at this QR version".into(),
            ));
        }
        if layout.n_groups > u16::MAX as usize + 1 {
            return Err(crate::Error::Codec(
                "video produces more tiled groups than fit in the u16 tile header".into(),
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
    pub active_tiles: usize,
    pub shard_data_bytes: usize,
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

    // Compute shard data capacity per tile.
    // QR capacity in bytes (byte-mode): total_data_codewords - 2 (mode + char count overhead)
    let qr_capacity_bytes = version_info.total_data_codewords().saturating_sub(2);
    // Base64url expands raw bytes to ceil(4n/3) chars without padding.
    let mut shard_data_bytes = 0usize;
    while base64url_encoded_len(TILE_HEADER_SIZE + shard_data_bytes + 1) <= qr_capacity_bytes {
        shard_data_bytes += 1;
    }

    let raw_payload_capacity = n_groups * config.data_shards * shard_data_bytes;
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
        active_tiles,
        shard_data_bytes,
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

        // Build per-tile QR payload strings
        let mut tile_payloads: Vec<Option<String>> = vec![None; layout.total_tiles];

        for group_id in 0..layout.n_groups {
            let group_offset = group_id * group_payload_size;
            let group_end = (group_offset + group_payload_size).min(framed_payload.len());
            let group_data = if group_offset < framed_payload.len() {
                &framed_payload[group_offset..group_end]
            } else {
                &[]
            };

            // Split into data chunks, padding with zeros
            let mut data_chunks: Vec<Vec<u8>> = Vec::with_capacity(self.config.data_shards);
            for shard_idx in 0..self.config.data_shards {
                let chunk_offset = shard_idx * layout.shard_data_bytes;
                let mut chunk = vec![0u8; layout.shard_data_bytes];
                if chunk_offset < group_data.len() {
                    let chunk_end = (chunk_offset + layout.shard_data_bytes).min(group_data.len());
                    let src = &group_data[chunk_offset..chunk_end];
                    chunk[..src.len()].copy_from_slice(src);
                }
                data_chunks.push(chunk);
            }

            // Compute parity chunks
            let parity_chunks = rs_encode_group(
                &data_chunks,
                self.config.data_shards,
                self.config.parity_shards,
                layout.shard_data_bytes,
            )?;

            // Assign payloads to tiles
            for (tile_idx, assignment) in layout.tile_assignments.iter().enumerate() {
                if let Some((gid, shard_idx)) = assignment {
                    if *gid != group_id {
                        continue;
                    }
                    let shard_data = if *shard_idx < self.config.data_shards {
                        &data_chunks[*shard_idx]
                    } else {
                        &parity_chunks[*shard_idx - self.config.data_shards]
                    };
                    tile_payloads[tile_idx] = Some(encode_tile_payload(
                        group_id as u16,
                        *shard_idx as u8,
                        shard_data,
                    ));
                }
            }
        }

        // Create shared temporal encoder for all tiles
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
            if let Some(payload_str) = qr_payload {
                let tile_key = derive_tile_key(master_key, tile_idx);
                let frames = tile_encoder.encode_message(&tile_key, payload_str)?;
                all_tile_frames[tile_idx] = Some(frames);
            }
        }

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

        Ok(video_frames)
    }
}

// ── Decoder ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TileDecodeOutcome {
    Success {
        detector_score: f32,
        message: String,
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

        // Decode each active tile
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
                    if let Some(ref message) = decode_result.message {
                        // Parse tile payload
                        if let Ok((group_id, shard_id, shard_data)) = decode_tile_payload(message) {
                            let gid = group_id as usize;
                            let sid = shard_id as usize;
                            if gid < layout.n_groups
                                && sid < layout.group_size
                                && shard_data.len() == layout.shard_data_bytes
                            {
                                group_shards[gid][sid].get_or_insert(shard_data);
                            }
                        }
                        tiles_decoded += 1;
                        tile_results.push(TileDecodeOutcome::Success {
                            detector_score: decode_result.detector_score,
                            message: message.clone(),
                        });
                    } else {
                        tile_results.push(TileDecodeOutcome::Failed {
                            detector_score: decode_result.detector_score,
                        });
                    }
                }
                Err(_) => {
                    tile_results.push(TileDecodeOutcome::Failed {
                        detector_score: 0.0,
                    });
                }
            }
        }

        // RS-recover each group
        let mut group_results = Vec::with_capacity(layout.n_groups);
        let mut all_recovered = true;
        let mut payload_groups: Vec<Option<Vec<Vec<u8>>>> = vec![None; layout.n_groups];

        for group_id in 0..layout.n_groups {
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
                        payload_groups[group_id] = Some(recovered);
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

        // Reassemble payload
        let payload = if all_recovered {
            let mut data = Vec::with_capacity(layout.max_payload_bytes);
            for shards in payload_groups.iter().flatten() {
                for shard in shards {
                    data.extend_from_slice(shard);
                }
            }
            if data.len() < PAYLOAD_LEN_SIZE {
                None
            } else {
                let logical_len = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
                let available = data.len() - PAYLOAD_LEN_SIZE;
                if logical_len > available {
                    None
                } else {
                    Some(data[PAYLOAD_LEN_SIZE..PAYLOAD_LEN_SIZE + logical_len].to_vec())
                }
            }
        } else {
            None
        };

        Ok(TiledDecodeResult {
            tile_results,
            tiles_decoded,
            tiles_total: layout.total_tiles,
            group_results,
            payload,
        })
    }
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_roundtrip() {
        let data = b"hello world";
        let encoded = base64url_encode(data);
        assert_eq!(encoded, "aGVsbG8gd29ybGQ");
        let decoded = base64url_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn base64url_empty() {
        assert_eq!(base64url_encode(&[]), "");
        assert_eq!(base64url_decode("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn base64url_invalid_length_fails() {
        assert!(base64url_decode("a").is_err());
    }

    #[test]
    fn base64url_invalid_char_fails() {
        assert!(base64url_decode("%A").is_err());
    }

    #[test]
    fn tile_payload_roundtrip() {
        let data = vec![0x42, 0x55, 0x99];
        let encoded = encode_tile_payload(7, 2, &data);
        let (group_id, shard_id, decoded) = decode_tile_payload(&encoded).unwrap();
        assert_eq!(group_id, 7);
        assert_eq!(shard_id, 2);
        assert_eq!(decoded, data);
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
        assert_eq!(layout.active_tiles, 3265);
        assert_eq!(layout.shard_data_bytes, 5);
        assert_eq!(layout.max_payload_bytes, 653 * 3 * 5 - PAYLOAD_LEN_SIZE); // 9791
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
        assert_eq!(layout.active_tiles, 15);
        assert_eq!(layout.shard_data_bytes, 5); // base64url: 5-byte header + 5-byte shard = 10 raw => 14 chars
        assert_eq!(layout.max_payload_bytes, 5 * 2 * 5 - PAYLOAD_LEN_SIZE);
    }

    #[test]
    fn v1_too_small_for_tiling() {
        // QR v1 has only 7 bytes capacity → shard_data_bytes = 0 → should fail
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
        let encoded = encode_tile_payload(7, 2, b"hello");
        let mut corrupted = encoded.into_bytes();
        let last = corrupted.len() - 1;
        corrupted[last] = if corrupted[last] == b'A' { b'B' } else { b'A' };
        let corrupted = String::from_utf8(corrupted).unwrap();
        assert!(decode_tile_payload(&corrupted).is_err());
    }

    #[test]
    fn small_encode_decode_roundtrip() {
        // 75x75 video, QR v2 (25x25) = 3x3 = 9 tiles
        // RS: 2 data + 1 parity = 3 per group → 3 groups
        // shard_data_bytes = 5 with base64url tile payloads and CRC16 headers
        // Use proven temporal baseline: 64 frames, 0.42/0.22 amplitudes
        let config = TiledConfig::new((75, 75), 2, 64, 0.42, 0.22, 2, 1).unwrap();
        let master_key = "test-tiled";

        let encoder = TiledEncoder::new(config.clone(), master_key).unwrap();
        let layout = encoder.layout();
        assert_eq!(layout.tiles_x, 3);
        assert_eq!(layout.tiles_y, 3);
        assert_eq!(layout.n_groups, 3);
        assert_eq!(layout.shard_data_bytes, 5);

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
}
