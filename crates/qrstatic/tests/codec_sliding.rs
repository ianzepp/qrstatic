#[path = "common/sliding.rs"]
mod sliding_common;

use qrstatic::codec::sliding::{SlidingConfig, SlidingDecoder, SlidingEncoder};
use qrstatic::{Error, Grid};
use sliding_common::{sliding_decode, sliding_frames, sliding_stream};

#[test]
fn l1_roundtrip_at_offset_zero() {
    let config = SlidingConfig::new((41, 41), 8, 4, 4);
    let frames = sliding_frames(config.clone(), "slide-key", None, b"", 20).unwrap();
    let decoded = sliding_decode(config, &frames, 0, 0).unwrap();
    assert_eq!(decoded.layer1_message.as_deref(), Some("slide-key"));
}

#[test]
fn l1_roundtrip_at_arbitrary_offset() {
    let config = SlidingConfig::new((41, 41), 8, 4, 4);
    let frames = sliding_frames(config.clone(), "slide-key", None, b"", 24).unwrap();
    let decoded = SlidingDecoder::new(SlidingConfig::new((41, 41), 8, 4, 4), 0)
        .unwrap()
        .decode_l1_at_offset(&frames, 5, Some("slide-key"))
        .unwrap();
    assert_eq!(decoded.layer1_message.as_deref(), Some("slide-key"));
}

#[test]
fn l1_and_l2_roundtrip() {
    let total_frames = 8 * 4 + 4;
    let config = SlidingConfig::new((41, 41), 8, 4, 4);
    let frames = sliding_frames(
        config.clone(),
        "slide-l1",
        Some("slide-l2"),
        b"",
        total_frames,
    )
    .unwrap();
    let decoded = sliding_decode(config, &frames, 0, 0).unwrap();
    assert_eq!(decoded.layer1_message.as_deref(), Some("slide-l1"));
    assert_eq!(decoded.layer2_message.as_deref(), Some("slide-l2"));
}

#[test]
fn different_stride_values() {
    for stride in [2, 4, 8] {
        let config = SlidingConfig::new((41, 41), 8, stride, 3);
        let frames = sliding_frames(config.clone(), "stride-key", None, b"", 24).unwrap();
        let decoded = sliding_decode(config, &frames, 0, 0).unwrap();
        assert_eq!(decoded.layer1_message.as_deref(), Some("stride-key"));
    }
}

#[test]
fn streaming_l1_periodic_and_l2_after_full_window() {
    let config = SlidingConfig::new((41, 41), 8, 4, 3);
    let outputs = sliding_stream(
        config,
        "stream-l1",
        Some("stream-l2"),
        b"stream payload",
        8 * 3 + 8,
    )
    .unwrap();
    assert!(!outputs.is_empty());
    assert_eq!(outputs[0].layer1_message.as_deref(), Some("stream-l1"));
    assert!(
        outputs
            .iter()
            .any(|result| result.layer2_message.as_deref() == Some("stream-l2"))
    );
}

#[test]
fn constructor_rejects_invalid_stride() {
    assert!(SlidingEncoder::new(SlidingConfig::new((41, 41), 8, 0, 3)).is_err());
}

#[test]
fn mismatched_dimensions_fail() {
    let decoder = SlidingDecoder::new(SlidingConfig::new((21, 21), 4, 2, 2), 0).unwrap();
    let a = Grid::filled(21, 21, 1.0f32);
    let b = Grid::filled(25, 25, -1.0f32);
    assert!(matches!(
        decoder.decode(&[a, b], 0),
        Err(Error::GridMismatch { .. })
    ));
}
