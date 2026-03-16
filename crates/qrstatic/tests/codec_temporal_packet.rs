use qrstatic::codec::temporal_packet::{
    TemporalPacket, TemporalPacketProfile, packetize_payload, recover_payload,
};

#[test]
fn temporal_packet_binary_roundtrip() {
    let packet = TemporalPacket {
        version: 1,
        flags: 0,
        block_id: 7,
        packet_id: 0,
        data_shards: 4,
        parity_shards: 2,
        payload_bytes_per_packet: 16,
        block_payload_len: 1,
        payload_crc32: 0xCBF4_3926,
        payload: b"a".to_vec(),
    };

    let encoded = packet.encode().unwrap_err();
    assert!(encoded.to_string().contains("CRC mismatch"));

    let packet = TemporalPacket {
        version: 1,
        flags: 0,
        block_id: 7,
        packet_id: 0,
        data_shards: 4,
        parity_shards: 2,
        payload_bytes_per_packet: 16,
        block_payload_len: 5,
        payload_crc32: crc32_for_test(b"hello"),
        payload: b"hello".to_vec(),
    };
    let encoded = packet.encode().unwrap();
    let decoded = TemporalPacket::decode(&encoded).unwrap();
    assert_eq!(decoded, packet);
}

#[test]
fn packetize_and_recover_payload_across_blocks() {
    let profile = TemporalPacketProfile::new(4, 2, 8).unwrap();
    let payload =
        b"Temporal Stage 2 packetization should recover this payload even with erasures.".to_vec();
    let packets = packetize_payload(&payload, profile).unwrap();

    let mut survivors = Vec::new();
    for packet in packets {
        if packet.block_id == 0 && packet.packet_id == 1 {
            continue;
        }
        if packet.block_id == 1 && packet.packet_id == 4 {
            continue;
        }
        survivors.push(packet);
    }

    let recovered = recover_payload(&survivors).unwrap();
    assert_eq!(recovered, payload);
}

#[test]
fn recover_payload_fails_when_block_is_missing_too_many_shards() {
    let profile = TemporalPacketProfile::new(4, 2, 8).unwrap();
    let payload = b"insufficient shards should fail recovery".to_vec();
    let packets = packetize_payload(&payload, profile).unwrap();

    let survivors: Vec<_> = packets
        .into_iter()
        .filter(|packet| !(packet.block_id == 0 && matches!(packet.packet_id, 0 | 1 | 4)))
        .collect();

    let err = recover_payload(&survivors).unwrap_err();
    assert!(err
        .to_string()
        .contains("only has 3 unique shards, need 4"));
}

fn crc32_for_test(bytes: &[u8]) -> u32 {
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
