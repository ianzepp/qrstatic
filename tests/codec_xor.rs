mod common;

use common::{TEST_MESSAGE, xor_frames_for_message, xor_roundtrip, xor_stream_roundtrip};
use qrstatic::Error;
use qrstatic::Grid;
use qrstatic::codec::xor::{XorDecoder, XorEncoder, XorStreamDecoder};

#[test]
fn roundtrip_n_2() {
    let decoded = xor_roundtrip(2, "xor-seed", TEST_MESSAGE).unwrap();
    assert_eq!(decoded.message.as_deref(), Some(TEST_MESSAGE));
}

#[test]
fn roundtrip_n_8() {
    let decoded = xor_roundtrip(8, "xor-seed", TEST_MESSAGE).unwrap();
    assert_eq!(decoded.message.as_deref(), Some(TEST_MESSAGE));
}

#[test]
fn roundtrip_n_64() {
    let decoded = xor_roundtrip(64, "xor-seed", TEST_MESSAGE).unwrap();
    assert_eq!(decoded.message.as_deref(), Some(TEST_MESSAGE));
}

#[test]
fn determinism() {
    let encoder = XorEncoder::new(8, "deterministic").unwrap();
    let a = encoder.encode_message(TEST_MESSAGE).unwrap();
    let b = encoder.encode_message(TEST_MESSAGE).unwrap();
    assert_eq!(a, b);
}

#[test]
fn stream_roundtrip() {
    let decoded = xor_stream_roundtrip(5, "stream-seed", &["first", "second"]).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].as_deref(), Some("first"));
    assert_eq!(decoded[1].as_deref(), Some("second"));
}

#[test]
fn partial_frames_do_not_decode() {
    let frames = xor_frames_for_message(4, "partial", TEST_MESSAGE).unwrap();

    let partial = XorDecoder::decode_message(&frames[..3]).unwrap();
    assert!(partial.message.is_none());

    let mut stream_decoder = XorStreamDecoder::new(4).unwrap();
    for frame in frames.into_iter().take(3) {
        assert!(stream_decoder.push_frame(frame).unwrap().is_none());
    }
}

#[test]
fn random_frames_do_not_produce_valid_qr() {
    let frames: Vec<_> = (0..8).map(|_| Grid::filled(21, 21, 0u8)).collect();
    let decoded = XorDecoder::decode_message(&frames).unwrap();
    assert!(decoded.message.is_none());
}

#[test]
fn mismatched_dimensions_fail() {
    let mut decoder = XorStreamDecoder::new(2).unwrap();
    let a = Grid::new(21, 21);
    let b = Grid::new(25, 25);
    assert!(decoder.push_frame(a).unwrap().is_none());
    assert!(matches!(
        decoder.push_frame(b),
        Err(Error::GridMismatch { .. })
    ));
}
