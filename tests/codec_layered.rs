#[path = "common/layered.rs"]
mod layered_common;

use layered_common::{layered_frames, layered_roundtrip, layered_stream_roundtrip};
use qrstatic::codec::layered::{LayeredConfig, LayeredDecoder, LayeredEncoder, LayeredStreamDecoder};
use qrstatic::{Error, Grid};

#[test]
fn l1_only_roundtrip() {
    let decoded = layered_roundtrip((41, 41), 4, 3, "layer1-visible", "layer2-hidden", b"").unwrap();
    assert_eq!(decoded.layer1_message.as_deref(), Some("layer1-visible"));
}

#[test]
fn l1_and_l2_roundtrip() {
    let decoded = layered_roundtrip((41, 41), 4, 4, "outer", "inner", b"").unwrap();
    assert_eq!(decoded.layer1_message.as_deref(), Some("outer"));
    assert_eq!(decoded.layer2_message.as_deref(), Some("inner"));
}

#[test]
fn l1_l2_and_payload_roundtrip() {
    let payload = b"deep secret";
    let decoded = layered_roundtrip((41, 41), 4, 4, "outer-key", "inner-key", payload).unwrap();
    assert_eq!(decoded.layer1_message.as_deref(), Some("outer-key"));
    assert_eq!(decoded.layer2_message.as_deref(), Some("inner-key"));
    assert_eq!(decoded.payload.as_deref(), Some(&payload[..]));
}

#[test]
fn default_30_by_30_shape_is_supported() {
    let frames = layered_frames((41, 41), 30, 30, "l1-default", "l2-default", b"x").unwrap();
    assert_eq!(frames.len(), 900);
}

#[test]
fn streaming_decodes_l1_each_n1_and_l2_at_full_window() {
    let payload = b"stream payload";
    let outputs = layered_stream_roundtrip((41, 41), 4, 3, "stream-l1", "stream-l2", payload).unwrap();
    assert_eq!(outputs.len(), 3);
    assert_eq!(outputs[0].layer1_message.as_deref(), Some("stream-l1"));
    assert!(outputs[0].layer2_message.is_none());
    assert_eq!(outputs[2].layer1_message.as_deref(), Some("stream-l1"));
    assert_eq!(outputs[2].layer2_message.as_deref(), Some("stream-l2"));
    assert_eq!(outputs[2].payload.as_deref(), Some(&payload[..]));
}

#[test]
fn partial_window_stream_does_not_emit_l2() {
    let frames = layered_frames((41, 41), 4, 3, "partial-l1", "partial-l2", b"payload").unwrap();
    let mut decoder = LayeredStreamDecoder::new(LayeredConfig::new((41, 41), 4, 3), 7).unwrap();
    let mut seen = Vec::new();
    for frame in frames.into_iter().take(8) {
        if let Some(result) = decoder.push_frame(frame).unwrap() {
            seen.push(result);
        }
    }
    assert_eq!(seen.len(), 2);
    assert!(seen.iter().all(|result| result.layer2_message.is_none()));
}

#[test]
fn constructor_rejects_invalid_counts() {
    assert!(LayeredEncoder::new(LayeredConfig::new((41, 41), 1, 2)).is_err());
}

#[test]
fn mismatched_dimensions_fail() {
    let decoder = LayeredDecoder::new(LayeredConfig::new((21, 21), 2, 2), 0).unwrap();
    let a = Grid::filled(21, 21, 1.0f32);
    let b = Grid::filled(25, 25, -1.0f32);
    assert!(matches!(decoder.decode(&[a, b]), Err(Error::GridMismatch { .. })));
}
