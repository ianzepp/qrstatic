use qrstatic::codec::analog::{AnalogDecoder, AnalogEncoder, AnalogStreamDecoder, AnalogStreamEncoder};
use qrstatic::{AnalogDecodeResult, Grid, Result};

pub fn analog_roundtrip(
    n_frames: usize,
    frame_shape: (usize, usize),
    noise_amplitude: f32,
    signal_strength: f32,
    payload_delta: f32,
    qr_key: &str,
    payload: &[u8],
) -> Result<AnalogDecodeResult> {
    let encoder = AnalogEncoder::new(
        n_frames,
        frame_shape,
        noise_amplitude,
        signal_strength,
        payload_delta,
    )?;
    let frames = encoder.encode_message(qr_key, payload)?;
    AnalogDecoder::new(payload.len(), noise_amplitude, signal_strength)?.decode_message(&frames)
}

pub fn analog_frames_for_message(
    n_frames: usize,
    frame_shape: (usize, usize),
    noise_amplitude: f32,
    signal_strength: f32,
    payload_delta: f32,
    qr_key: &str,
    payload: &[u8],
) -> Result<Vec<Grid<f32>>> {
    let encoder = AnalogEncoder::new(
        n_frames,
        frame_shape,
        noise_amplitude,
        signal_strength,
        payload_delta,
    )?;
    encoder.encode_message(qr_key, payload)
}

pub fn analog_stream_roundtrip(
    n_frames: usize,
    frame_shape: (usize, usize),
    noise_amplitude: f32,
    signal_strength: f32,
    payload_delta: f32,
    messages: &[(&str, &[u8])],
) -> Result<Vec<AnalogDecodeResult>> {
    let mut encoder = AnalogStreamEncoder::new(
        n_frames,
        frame_shape,
        noise_amplitude,
        signal_strength,
        payload_delta,
    )?;
    for (qr_key, payload) in messages {
        encoder.queue_message(*qr_key, payload.to_vec());
    }

    let mut decoder = AnalogStreamDecoder::new(
        n_frames,
        messages.first().map(|(_, payload)| payload.len()).unwrap_or(0),
        noise_amplitude,
        signal_strength,
    )?;
    let mut decoded = Vec::new();
    for _ in 0..(n_frames * messages.len()) {
        let frame = encoder.next_frame()?;
        if let Some(result) = decoder.push_frame(frame)? {
            decoded.push(result);
        }
    }
    Ok(decoded)
}
