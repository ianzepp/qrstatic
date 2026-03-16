use std::collections::{BTreeMap, BTreeSet};

use crate::error::{Error, Result};
use crate::qr::gf256;

const TEMPORAL_PACKET_VERSION: u8 = 1;
const FLAG_PARITY: u8 = 0x01;
const HEADER_LEN: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TemporalPacketProfile {
    pub data_shards: usize,
    pub parity_shards: usize,
    pub payload_bytes_per_packet: usize,
}

impl TemporalPacketProfile {
    pub fn new(data_shards: usize, parity_shards: usize, payload_bytes_per_packet: usize) -> Result<Self> {
        if data_shards == 0 {
            return Err(Error::Codec(
                "temporal packet profile requires data_shards > 0".into(),
            ));
        }
        if payload_bytes_per_packet == 0 {
            return Err(Error::Codec(
                "temporal packet profile requires payload_bytes_per_packet > 0".into(),
            ));
        }
        if data_shards + parity_shards > 255 {
            return Err(Error::Codec(
                "temporal packet profile requires data_shards + parity_shards <= 255".into(),
            ));
        }

        Ok(Self {
            data_shards,
            parity_shards,
            payload_bytes_per_packet,
        })
    }

    pub fn total_shards(&self) -> usize {
        self.data_shards + self.parity_shards
    }

    pub fn max_block_payload_len(&self) -> usize {
        self.data_shards * self.payload_bytes_per_packet
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemporalPacket {
    pub version: u8,
    pub flags: u8,
    pub block_id: u32,
    pub packet_id: u16,
    pub data_shards: u8,
    pub parity_shards: u8,
    pub payload_bytes_per_packet: u16,
    pub block_payload_len: u32,
    pub payload_crc32: u32,
    pub payload: Vec<u8>,
}

impl TemporalPacket {
    pub fn is_parity(&self) -> bool {
        self.flags & FLAG_PARITY != 0
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate()?;

        let mut bytes = Vec::with_capacity(HEADER_LEN + self.payload.len());
        bytes.push(self.version);
        bytes.push(self.flags);
        bytes.extend_from_slice(&self.block_id.to_le_bytes());
        bytes.extend_from_slice(&self.packet_id.to_le_bytes());
        bytes.push(self.data_shards);
        bytes.push(self.parity_shards);
        bytes.extend_from_slice(&self.payload_bytes_per_packet.to_le_bytes());
        bytes.extend_from_slice(&self.block_payload_len.to_le_bytes());
        bytes.extend_from_slice(&self.payload_crc32.to_le_bytes());
        bytes.extend_from_slice(&self.payload);
        Ok(bytes)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self> {
        if encoded.len() < HEADER_LEN {
            return Err(Error::Codec(format!(
                "temporal packet requires at least {HEADER_LEN} bytes, got {}",
                encoded.len()
            )));
        }

        let version = encoded[0];
        let flags = encoded[1];
        let mut block_id_bytes = [0u8; 4];
        block_id_bytes.copy_from_slice(&encoded[2..6]);
        let block_id = u32::from_le_bytes(block_id_bytes);
        let mut packet_id_bytes = [0u8; 2];
        packet_id_bytes.copy_from_slice(&encoded[6..8]);
        let packet_id = u16::from_le_bytes(packet_id_bytes);
        let data_shards = encoded[8];
        let parity_shards = encoded[9];
        let mut payload_bytes_bytes = [0u8; 2];
        payload_bytes_bytes.copy_from_slice(&encoded[10..12]);
        let payload_bytes_per_packet = u16::from_le_bytes(payload_bytes_bytes);
        let mut block_payload_len_bytes = [0u8; 4];
        block_payload_len_bytes.copy_from_slice(&encoded[12..16]);
        let block_payload_len = u32::from_le_bytes(block_payload_len_bytes);
        let mut payload_crc_bytes = [0u8; 4];
        payload_crc_bytes.copy_from_slice(&encoded[16..20]);
        let payload_crc32 = u32::from_le_bytes(payload_crc_bytes);
        let payload = encoded[20..].to_vec();

        let packet = Self {
            version,
            flags,
            block_id,
            packet_id,
            data_shards,
            parity_shards,
            payload_bytes_per_packet,
            block_payload_len,
            payload_crc32,
            payload,
        };
        packet.validate()?;
        Ok(packet)
    }

    fn validate(&self) -> Result<()> {
        if self.version != TEMPORAL_PACKET_VERSION {
            return Err(Error::Codec(format!(
                "unsupported temporal packet version {}, expected {}",
                self.version, TEMPORAL_PACKET_VERSION
            )));
        }

        let profile = TemporalPacketProfile::new(
            self.data_shards as usize,
            self.parity_shards as usize,
            self.payload_bytes_per_packet as usize,
        )?;
        if self.packet_id as usize >= profile.total_shards() {
            return Err(Error::Codec(format!(
                "packet_id {} exceeds shard count {}",
                self.packet_id,
                profile.total_shards()
            )));
        }
        if self.block_payload_len as usize > profile.max_block_payload_len() {
            return Err(Error::Codec(format!(
                "block payload length {} exceeds block capacity {}",
                self.block_payload_len,
                profile.max_block_payload_len()
            )));
        }
        if self.is_parity() {
            if self.payload.len() != profile.payload_bytes_per_packet {
                return Err(Error::Codec(format!(
                    "parity packet payload must be exactly {} bytes, got {}",
                    profile.payload_bytes_per_packet,
                    self.payload.len()
                )));
            }
        } else {
            let expected_len = expected_data_payload_len(
                profile,
                self.block_payload_len as usize,
                self.packet_id as usize,
            )?;
            if self.payload.len() != expected_len {
                return Err(Error::Codec(format!(
                    "data packet {} expected payload length {}, got {}",
                    self.packet_id,
                    expected_len,
                    self.payload.len()
                )));
            }
        }
        if self.payload_crc32 != crc32(&self.payload) {
            return Err(Error::Codec(format!(
                "packet {} CRC mismatch",
                self.packet_id
            )));
        }

        Ok(())
    }
}

pub fn packetize_payload(payload: &[u8], profile: TemporalPacketProfile) -> Result<Vec<TemporalPacket>> {
    let generator_rows = systematic_generator_rows(profile)?;
    let mut packets = Vec::new();
    let mut block_id = 0u32;

    for block in payload.chunks(profile.max_block_payload_len()) {
        let mut data_shards = vec![vec![0u8; profile.payload_bytes_per_packet]; profile.data_shards];
        for (shard_index, shard) in data_shards.iter_mut().enumerate() {
            let start = shard_index * profile.payload_bytes_per_packet;
            let end = (start + profile.payload_bytes_per_packet).min(block.len());
            if start < block.len() {
                shard[..end - start].copy_from_slice(&block[start..end]);
            }
        }

        for (packet_id, shard) in data_shards.iter().enumerate() {
            let actual_len = expected_data_payload_len(profile, block.len(), packet_id)?;
            let payload = shard[..actual_len].to_vec();
            packets.push(TemporalPacket {
                version: TEMPORAL_PACKET_VERSION,
                flags: 0,
                block_id,
                packet_id: packet_id as u16,
                data_shards: profile.data_shards as u8,
                parity_shards: profile.parity_shards as u8,
                payload_bytes_per_packet: profile.payload_bytes_per_packet as u16,
                block_payload_len: block.len() as u32,
                payload_crc32: crc32(&payload),
                payload,
            });
        }

        for parity_index in 0..profile.parity_shards {
            let row = &generator_rows[profile.data_shards + parity_index];
            let mut shard = vec![0u8; profile.payload_bytes_per_packet];
            for byte_index in 0..profile.payload_bytes_per_packet {
                let mut acc = 0u8;
                for (data_index, data_shard) in data_shards.iter().enumerate() {
                    acc ^= gf256::mul(row[data_index], data_shard[byte_index]);
                }
                shard[byte_index] = acc;
            }

            packets.push(TemporalPacket {
                version: TEMPORAL_PACKET_VERSION,
                flags: FLAG_PARITY,
                block_id,
                packet_id: (profile.data_shards + parity_index) as u16,
                data_shards: profile.data_shards as u8,
                parity_shards: profile.parity_shards as u8,
                payload_bytes_per_packet: profile.payload_bytes_per_packet as u16,
                block_payload_len: block.len() as u32,
                payload_crc32: crc32(&shard),
                payload: shard,
            });
        }

        block_id = block_id.wrapping_add(1);
    }

    Ok(packets)
}

pub fn encode_packet_stream(payload: &[u8], profile: TemporalPacketProfile) -> Result<Vec<u8>> {
    let packets = packetize_payload(payload, profile)?;
    let mut stream = Vec::new();
    for packet in packets {
        stream.extend_from_slice(&packet.encode()?);
    }
    Ok(stream)
}

pub fn packet_stream_layout(payload_len: usize, profile: TemporalPacketProfile) -> Result<Vec<usize>> {
    let template_payload = vec![0u8; payload_len];
    let packets = packetize_payload(&template_payload, profile)?;
    let mut layout = Vec::with_capacity(packets.len());
    for packet in packets {
        layout.push(packet.encode()?.len());
    }
    Ok(layout)
}

pub fn decode_packet_stream(
    encoded: &[u8],
    payload_len: usize,
    profile: TemporalPacketProfile,
) -> Result<Vec<TemporalPacket>> {
    let layout = packet_stream_layout(payload_len, profile)?;
    let expected_len: usize = layout.iter().sum();
    if encoded.len() != expected_len {
        return Err(Error::Codec(format!(
            "temporal packet stream length mismatch: expected {expected_len}, got {}",
            encoded.len()
        )));
    }

    let mut packets = Vec::with_capacity(layout.len());
    let mut offset = 0usize;
    for packet_len in layout {
        packets.push(TemporalPacket::decode(&encoded[offset..offset + packet_len])?);
        offset += packet_len;
    }
    Ok(packets)
}

pub fn recover_payload(packets: &[TemporalPacket]) -> Result<Vec<u8>> {
    if packets.is_empty() {
        return Ok(Vec::new());
    }

    let mut blocks = BTreeMap::<u32, Vec<TemporalPacket>>::new();
    for packet in packets {
        packet.validate()?;
        blocks.entry(packet.block_id).or_default().push(packet.clone());
    }

    let mut payload = Vec::new();
    for block_packets in blocks.into_values() {
        let recovered_data = recover_block_data(&block_packets)?;
        for packet in recovered_data {
            payload.extend_from_slice(&packet.payload);
        }
    }

    Ok(payload)
}

fn recover_block_data(block_packets: &[TemporalPacket]) -> Result<Vec<TemporalPacket>> {
    let first = block_packets
        .first()
        .ok_or_else(|| Error::Codec("cannot recover empty temporal packet block".into()))?;
    let profile = TemporalPacketProfile::new(
        first.data_shards as usize,
        first.parity_shards as usize,
        first.payload_bytes_per_packet as usize,
    )?;
    let block_payload_len = first.block_payload_len as usize;

    let mut unique_ids = BTreeSet::new();
    let mut selected = Vec::new();
    for packet in block_packets {
        if packet.block_id != first.block_id {
            return Err(Error::Codec("mixed block ids in temporal packet recovery".into()));
        }
        if packet.data_shards != first.data_shards
            || packet.parity_shards != first.parity_shards
            || packet.payload_bytes_per_packet != first.payload_bytes_per_packet
            || packet.block_payload_len != first.block_payload_len
        {
            return Err(Error::Codec(
                "inconsistent temporal packet headers within block".into(),
            ));
        }
        if unique_ids.insert(packet.packet_id) {
            selected.push(packet);
        }
    }

    if selected.len() < profile.data_shards {
        return Err(Error::Codec(format!(
            "block {} only has {} unique shards, need {}",
            first.block_id,
            selected.len(),
            profile.data_shards
        )));
    }

    let generator_rows = systematic_generator_rows(profile)?;
    let chosen = &selected[..profile.data_shards];
    let mut decode_matrix = Vec::with_capacity(profile.data_shards);
    let mut chosen_payloads = Vec::with_capacity(profile.data_shards);
    for packet in chosen {
        decode_matrix.push(generator_rows[packet.packet_id as usize].clone());
        chosen_payloads.push(padded_packet_payload(packet, profile, block_payload_len)?);
    }
    let inverse = invert_matrix(&decode_matrix)?;

    let mut recovered_shards = vec![vec![0u8; profile.payload_bytes_per_packet]; profile.data_shards];
    for byte_index in 0..profile.payload_bytes_per_packet {
        for data_index in 0..profile.data_shards {
            let mut acc = 0u8;
            for (row_index, payload) in chosen_payloads.iter().enumerate() {
                acc ^= gf256::mul(inverse[data_index][row_index], payload[byte_index]);
            }
            recovered_shards[data_index][byte_index] = acc;
        }
    }

    let mut recovered_packets = Vec::with_capacity(profile.data_shards);
    for (data_index, shard) in recovered_shards.into_iter().enumerate() {
        let actual_len = expected_data_payload_len(profile, block_payload_len, data_index)?;
        let payload = shard[..actual_len].to_vec();
        recovered_packets.push(TemporalPacket {
            version: TEMPORAL_PACKET_VERSION,
            flags: 0,
            block_id: first.block_id,
            packet_id: data_index as u16,
            data_shards: profile.data_shards as u8,
            parity_shards: profile.parity_shards as u8,
            payload_bytes_per_packet: profile.payload_bytes_per_packet as u16,
            block_payload_len: block_payload_len as u32,
            payload_crc32: crc32(&payload),
            payload,
        });
    }

    Ok(recovered_packets)
}

fn expected_data_payload_len(
    profile: TemporalPacketProfile,
    block_payload_len: usize,
    packet_id: usize,
) -> Result<usize> {
    if packet_id >= profile.data_shards {
        return Err(Error::Codec(format!(
            "packet_id {} is not a data shard for data_shards {}",
            packet_id, profile.data_shards
        )));
    }

    let start = packet_id * profile.payload_bytes_per_packet;
    if start >= block_payload_len {
        return Ok(0);
    }
    Ok((block_payload_len - start).min(profile.payload_bytes_per_packet))
}

fn padded_packet_payload(
    packet: &TemporalPacket,
    profile: TemporalPacketProfile,
    block_payload_len: usize,
) -> Result<Vec<u8>> {
    if packet.is_parity() {
        return Ok(packet.payload.clone());
    }

    let expected_len = expected_data_payload_len(profile, block_payload_len, packet.packet_id as usize)?;
    if packet.payload.len() != expected_len {
        return Err(Error::Codec(format!(
            "data packet {} expected {} payload bytes, got {}",
            packet.packet_id,
            expected_len,
            packet.payload.len()
        )));
    }

    let mut padded = vec![0u8; profile.payload_bytes_per_packet];
    padded[..packet.payload.len()].copy_from_slice(&packet.payload);
    Ok(padded)
}

fn systematic_generator_rows(profile: TemporalPacketProfile) -> Result<Vec<Vec<u8>>> {
    let total_shards = profile.total_shards();
    let mut vandermonde = Vec::with_capacity(total_shards);
    for row_index in 0..total_shards {
        let x = (row_index + 1) as u8;
        let mut row = Vec::with_capacity(profile.data_shards);
        for power in 0..profile.data_shards {
            row.push(gf256::pow(x, power as u32));
        }
        vandermonde.push(row);
    }

    let data_matrix = vandermonde[..profile.data_shards].to_vec();
    let data_inverse = invert_matrix(&data_matrix)?;
    let mut generator_rows = Vec::with_capacity(total_shards);
    for row in vandermonde {
        generator_rows.push(multiply_row_by_matrix(&row, &data_inverse));
    }
    Ok(generator_rows)
}

fn multiply_row_by_matrix(row: &[u8], matrix: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0u8; matrix[0].len()];
    for (col_index, out_cell) in out.iter_mut().enumerate() {
        let mut acc = 0u8;
        for (row_index, row_value) in row.iter().enumerate() {
            acc ^= gf256::mul(*row_value, matrix[row_index][col_index]);
        }
        *out_cell = acc;
    }
    out
}

fn invert_matrix(matrix: &[Vec<u8>]) -> Result<Vec<Vec<u8>>> {
    let n = matrix.len();
    if n == 0 || matrix.iter().any(|row| row.len() != n) {
        return Err(Error::Codec("cannot invert non-square GF(256) matrix".into()));
    }

    let mut augmented = vec![vec![0u8; n * 2]; n];
    for row in 0..n {
        for col in 0..n {
            augmented[row][col] = matrix[row][col];
        }
        augmented[row][n + row] = 1;
    }

    for pivot in 0..n {
        let Some(swap_row) = (pivot..n).find(|&row| augmented[row][pivot] != 0) else {
            return Err(Error::Codec("temporal packet matrix is not invertible".into()));
        };
        if swap_row != pivot {
            augmented.swap(pivot, swap_row);
        }

        let pivot_value = augmented[pivot][pivot];
        let inv_pivot = gf256::div(1, pivot_value);
        for col in 0..(n * 2) {
            augmented[pivot][col] = gf256::mul(augmented[pivot][col], inv_pivot);
        }

        for row in 0..n {
            if row == pivot {
                continue;
            }
            let factor = augmented[row][pivot];
            if factor == 0 {
                continue;
            }
            for col in 0..(n * 2) {
                augmented[row][col] ^= gf256::mul(factor, augmented[pivot][col]);
            }
        }
    }

    let mut inverse = vec![vec![0u8; n]; n];
    for row in 0..n {
        inverse[row].copy_from_slice(&augmented[row][n..]);
    }
    Ok(inverse)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg() & 0xEDB8_8320;
            crc = (crc >> 1) ^ mask;
        }
    }
    !crc
}
