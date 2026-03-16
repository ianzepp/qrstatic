use qrstatic::codec::binary::{
    BinaryDecoder, BinaryEncoder, BinaryStreamDecoder, BinaryStreamEncoder, DEFAULT_BINARY_WINDOW,
};
use qrstatic::{BinaryDecodeResult, Grid, Result};

pub fn binary_roundtrip(
    n_frames: usize,
    frame_shape: (usize, usize),
    seed: &str,
    base_bias: f32,
    payload_bias_delta: f32,
    qr_key: &str,
    payload: &[u8],
) -> Result<BinaryDecodeResult> {
    let encoder = BinaryEncoder::new(
        n_frames,
        frame_shape,
        seed,
        base_bias,
        payload_bias_delta,
    )?;
    let frames = encoder.encode_message(qr_key, payload)?;
    BinaryDecoder::new(payload.len(), base_bias)?.decode_message(&frames)
}

pub fn binary_frames_for_message(
    n_frames: usize,
    frame_shape: (usize, usize),
    seed: &str,
    base_bias: f32,
    payload_bias_delta: f32,
    qr_key: &str,
    payload: &[u8],
) -> Result<Vec<Grid<i8>>> {
    let encoder = BinaryEncoder::new(
        n_frames,
        frame_shape,
        seed,
        base_bias,
        payload_bias_delta,
    )?;
    encoder.encode_message(qr_key, payload)
}

pub fn binary_stream_roundtrip(
    n_frames: usize,
    frame_shape: (usize, usize),
    seed: &str,
    base_bias: f32,
    payload_bias_delta: f32,
    messages: &[(&str, &[u8])],
) -> Result<Vec<BinaryDecodeResult>> {
    let mut encoder = BinaryStreamEncoder::new(
        n_frames,
        frame_shape,
        seed,
        base_bias,
        payload_bias_delta,
    )?;
    for (qr_key, payload) in messages {
        encoder.queue_message(*qr_key, payload.to_vec());
    }

    let mut decoder = BinaryStreamDecoder::new(
        n_frames,
        messages.first().map(|(_, payload)| payload.len()).unwrap_or(0),
        base_bias,
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

pub fn binary_default_window_roundtrip(
    frame_shape: (usize, usize),
    seed: &str,
    base_bias: f32,
    payload_bias_delta: f32,
    qr_key: &str,
) -> Result<Option<String>> {
    let mut encoder = BinaryStreamEncoder::with_default_window(
        frame_shape,
        seed,
        base_bias,
        payload_bias_delta,
    )?;
    encoder.queue_message(qr_key.to_owned(), Vec::new());

    let mut decoder = BinaryStreamDecoder::with_default_window(0, base_bias)?;
    let mut decoded = None;
    for _ in 0..DEFAULT_BINARY_WINDOW {
        let frame = encoder.next_frame()?;
        if let Some(result) = decoder.push_frame(frame)? {
            decoded = result.message;
        }
    }
    Ok(decoded)
}
