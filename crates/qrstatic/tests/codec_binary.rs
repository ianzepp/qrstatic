#[path = "common/binary.rs"]
mod binary_common;

use binary_common::{
    binary_default_window_roundtrip, binary_frames_for_message, binary_roundtrip,
    binary_stream_roundtrip,
};
use qrstatic::codec::binary::{BinaryDecoder, BinaryEncoder, BinaryStreamDecoder};
use qrstatic::{Error, Grid};

#[test]
fn qr_only_roundtrip() {
    let decoded =
        binary_roundtrip(24, (41, 41), "binary-seed", 0.8, 0.1, "binary-only", b"").unwrap();
    assert_eq!(decoded.message.as_deref(), Some("binary-only"));
    assert_eq!(decoded.payload.as_deref(), Some(&[][..]));
}

#[test]
fn qr_and_payload_roundtrip() {
    let payload = b"binary payload";
    let decoded =
        binary_roundtrip(60, (41, 41), "binary-seed", 0.8, 0.1, "binary-key", payload).unwrap();
    assert_eq!(decoded.message.as_deref(), Some("binary-key"));
    assert_eq!(decoded.payload.as_deref(), Some(&payload[..]));
}

#[test]
fn different_bias_values() {
    let payload = b"bias";
    for base_bias in [0.7, 0.8] {
        let decoded = binary_roundtrip(
            60,
            (41, 41),
            "bias-seed",
            base_bias,
            0.1,
            "bias-key",
            payload,
        )
        .unwrap();
        assert_eq!(decoded.message.as_deref(), Some("bias-key"));
        assert_eq!(decoded.payload.as_deref(), Some(&payload[..]));
    }
}

#[test]
fn streaming_roundtrip() {
    let messages: [(&str, &[u8]); 2] = [("stream-a", b"alpha"), ("stream-b", b"bravo")];
    let decoded =
        binary_stream_roundtrip(60, (41, 41), "stream-seed", 0.8, 0.1, &messages).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].message.as_deref(), Some("stream-a"));
    assert_eq!(decoded[0].payload.as_deref(), Some(&b"alpha"[..]));
    assert_eq!(decoded[1].message.as_deref(), Some("stream-b"));
    assert_eq!(decoded[1].payload.as_deref(), Some(&b"bravo"[..]));
}

#[test]
fn default_window_is_60_frames() {
    let decoded =
        binary_default_window_roundtrip((41, 41), "default-window", 0.8, 0.1, "window-key")
            .unwrap();
    assert_eq!(decoded.as_deref(), Some("window-key"));
}

#[test]
fn partial_frames_do_not_decode() {
    let frames =
        binary_frames_for_message(12, (41, 41), "partial", 0.8, 0.1, "partial-key", b"payload")
            .unwrap();

    let partial = BinaryDecoder::new(7, 0.8)
        .unwrap()
        .decode_message(&frames[..6])
        .unwrap();
    assert!(partial.message.is_none());

    let mut stream_decoder = BinaryStreamDecoder::new(12, 7, 0.8).unwrap();
    for frame in frames.into_iter().take(11) {
        assert!(stream_decoder.push_frame(frame).unwrap().is_none());
    }
}

#[test]
fn random_frames_do_not_produce_valid_qr() {
    let frames: Vec<_> = (0..8).map(|_| Grid::filled(41, 41, 1i8)).collect();
    let decoded = BinaryDecoder::new(0, 0.8)
        .unwrap()
        .decode_message(&frames)
        .unwrap();
    assert!(decoded.message.is_none());
}

#[test]
fn mismatched_dimensions_fail() {
    let mut decoder = BinaryStreamDecoder::new(2, 0, 0.8).unwrap();
    let a = Grid::filled(21, 21, 1i8);
    let b = Grid::filled(25, 25, -1i8);
    assert!(decoder.push_frame(a).unwrap().is_none());
    assert!(matches!(
        decoder.push_frame(b),
        Err(Error::GridMismatch { .. })
    ));
}

#[test]
fn constructor_rejects_zero_frames() {
    assert!(BinaryEncoder::new(0, (41, 41), "seed", 0.8, 0.1).is_err());
}
