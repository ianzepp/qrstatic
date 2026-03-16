#[path = "common/analog.rs"]
mod analog_common;

use analog_common::{analog_frames_for_message, analog_roundtrip, analog_stream_roundtrip};
use qrstatic::codec::analog::{AnalogEncoder, AnalogStreamDecoder};
use qrstatic::{Error, Grid};

#[test]
fn qr_only_roundtrip() {
    let decoded = analog_roundtrip(8, (41, 41), 0.3, 5.0, 0.5, "analog-only", b"").unwrap();
    assert_eq!(decoded.message.as_deref(), Some("analog-only"));
    assert_eq!(decoded.payload.as_deref(), Some(&[][..]));
}

#[test]
fn qr_and_payload_roundtrip() {
    let payload = b"analog payload";
    let decoded =
        analog_roundtrip(8, (41, 41), 0.3, 5.0, 0.5, "analog-key", payload).unwrap();
    assert_eq!(decoded.message.as_deref(), Some("analog-key"));
    assert_eq!(decoded.payload.as_deref(), Some(&payload[..]));
}

#[test]
fn snr_improves_with_more_frames() {
    let short = analog_frames_for_message(4, (41, 41), 0.3, 2.0, 0.5, "snr-key", b"x").unwrap();
    let long = analog_frames_for_message(16, (41, 41), 0.3, 2.0, 0.5, "snr-key", b"x").unwrap();

    let short_acc = short
        .iter()
        .fold(Grid::<f32>::new(41, 41), |mut acc, frame| {
            for (lhs, rhs) in acc.data_mut().iter_mut().zip(frame.data().iter()) {
                *lhs += *rhs;
            }
            acc
        });
    let long_acc = long
        .iter()
        .fold(Grid::<f32>::new(41, 41), |mut acc, frame| {
            for (lhs, rhs) in acc.data_mut().iter_mut().zip(frame.data().iter()) {
                *lhs += *rhs;
            }
            acc
        });

    let short_mean =
        short_acc.data().iter().map(|v| v.abs()).sum::<f32>() / short_acc.len() as f32;
    let long_mean = long_acc.data().iter().map(|v| v.abs()).sum::<f32>() / long_acc.len() as f32;

    assert!(long_mean > short_mean);
}

#[test]
fn different_signal_noise_parameters() {
    let payload = b"signal";
    for (noise_amplitude, signal_strength) in [(0.2, 4.0), (0.5, 6.0)] {
        let decoded = analog_roundtrip(
            8,
            (41, 41),
            noise_amplitude,
            signal_strength,
            0.5,
            "signal-key",
            payload,
        )
        .unwrap();
        assert_eq!(decoded.message.as_deref(), Some("signal-key"));
        assert_eq!(decoded.payload.as_deref(), Some(&payload[..]));
    }
}

#[test]
fn streaming_roundtrip() {
    let messages: [(&str, &[u8]); 2] = [("analog-a", b"alpha"), ("analog-b", b"bravo")];
    let decoded = analog_stream_roundtrip(8, (41, 41), 0.3, 5.0, 0.5, &messages).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0].message.as_deref(), Some("analog-a"));
    assert_eq!(decoded[0].payload.as_deref(), Some(&b"alpha"[..]));
    assert_eq!(decoded[1].message.as_deref(), Some("analog-b"));
    assert_eq!(decoded[1].payload.as_deref(), Some(&b"bravo"[..]));
}

#[test]
fn partial_frames_do_not_decode() {
    let frames =
        analog_frames_for_message(8, (41, 41), 0.3, 5.0, 0.5, "partial-key", b"payload").unwrap();

    let mut stream_decoder = AnalogStreamDecoder::new(8, 7, 0.3, 5.0).unwrap();
    for frame in frames.into_iter().take(7) {
        assert!(stream_decoder.push_frame(frame).unwrap().is_none());
    }
}

#[test]
fn constructor_rejects_minimum_frame_count() {
    assert!(AnalogEncoder::new(1, (41, 41), 0.3, 5.0, 0.5).is_err());
}

#[test]
fn mismatched_dimensions_fail() {
    let mut decoder = AnalogStreamDecoder::new(2, 0, 0.3, 5.0).unwrap();
    let a = Grid::filled(21, 21, 1.0f32);
    let b = Grid::filled(25, 25, -1.0f32);
    assert!(decoder.push_frame(a).unwrap().is_none());
    assert!(matches!(
        decoder.push_frame(b),
        Err(Error::GridMismatch { .. })
    ));
}
