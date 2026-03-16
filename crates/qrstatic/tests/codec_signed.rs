#[path = "common/signed.rs"]
mod signed_common;

use qrstatic::codec::signed::{SignedDecoder, SignedEncoder, SignedStreamDecoder};
use signed_common::{signed_frames_for_message, signed_roundtrip, signed_stream_roundtrip};

#[test]
fn qr_only_roundtrip() {
    let decoded = signed_roundtrip(8, (41, 41), 3, "seed-only", b"").unwrap();
    assert_eq!(decoded.message.as_deref(), Some("seed-only"));
    assert_eq!(decoded.payload.as_deref(), Some(&[][..]));
}

#[test]
fn qr_and_payload_roundtrip() {
    let payload = b"hello payload";
    let decoded = signed_roundtrip(8, (41, 41), 3, "signed-key", payload).unwrap();
    assert_eq!(decoded.message.as_deref(), Some("signed-key"));
    assert_eq!(decoded.payload.as_deref(), Some(&payload[..]));
}

#[test]
fn different_signal_strengths() {
    let payload = b"signal";
    for signal_strength in [2, 4] {
        let decoded =
            signed_roundtrip(8, (41, 41), signal_strength, "signal-key", payload).unwrap();
        assert_eq!(decoded.message.as_deref(), Some("signal-key"));
        assert_eq!(decoded.payload.as_deref(), Some(&payload[..]));
    }
}

#[test]
fn streaming_roundtrip() {
    let messages: [(&str, &[u8]); 2] = [("key-a", b"alpha"), ("key-b", b"bravo")];
    let decoded = signed_stream_roundtrip(8, (41, 41), 3, &messages).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].message.as_deref(), Some("key-a"));
    assert_eq!(decoded[0].payload.as_deref(), Some(&b"alpha"[..]));
    assert_eq!(decoded[1].message.as_deref(), Some("key-b"));
    assert_eq!(decoded[1].payload.as_deref(), Some(&b"bravo"[..]));
}

#[test]
fn partial_frames_do_not_decode() {
    let frames = signed_frames_for_message(8, (41, 41), 3, "partial-key", b"payload").unwrap();
    let partial = SignedDecoder::new(7, 3)
        .decode_message(&frames[..4])
        .unwrap();
    assert!(partial.message.is_none());

    let mut stream_decoder = SignedStreamDecoder::new(8, 7, 3).unwrap();
    for frame in frames.into_iter().take(7) {
        assert!(stream_decoder.push_frame(frame).unwrap().is_none());
    }
}

#[test]
fn minimum_frame_count_is_rejected() {
    assert!(SignedEncoder::new(3, (41, 41), "seed", 3).is_err());
}

#[test]
fn payload_at_maximum_capacity() {
    let payload = vec![0xA5; (41 * 41) / 8];
    let decoded = signed_roundtrip(8, (41, 41), 4, "max-payload", &payload).unwrap();
    assert_eq!(decoded.message.as_deref(), Some("max-payload"));
    assert_eq!(decoded.payload.as_deref(), Some(payload.as_slice()));
}
