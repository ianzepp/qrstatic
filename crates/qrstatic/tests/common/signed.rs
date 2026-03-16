use qrstatic::codec::signed::{
    SignedDecoder, SignedEncoder, SignedStreamDecoder, SignedStreamEncoder,
};
use qrstatic::{Grid, Result, SignedDecodeResult};

pub fn signed_roundtrip(
    n_frames: usize,
    frame_shape: (usize, usize),
    signal_strength: i16,
    qr_seed: &str,
    payload: &[u8],
) -> Result<SignedDecodeResult> {
    let encoder = SignedEncoder::new(n_frames, frame_shape, "signed", signal_strength)?;
    let frames = encoder.encode_message(qr_seed, payload)?;
    SignedDecoder::new(payload.len(), signal_strength).decode_message(&frames)
}

pub fn signed_frames_for_message(
    n_frames: usize,
    frame_shape: (usize, usize),
    signal_strength: i16,
    qr_seed: &str,
    payload: &[u8],
) -> Result<Vec<Grid<i8>>> {
    let encoder = SignedEncoder::new(n_frames, frame_shape, "signed", signal_strength)?;
    encoder.encode_message(qr_seed, payload)
}

pub fn signed_stream_roundtrip(
    n_frames: usize,
    frame_shape: (usize, usize),
    signal_strength: i16,
    messages: &[(&str, &[u8])],
) -> Result<Vec<SignedDecodeResult>> {
    let mut encoder = SignedStreamEncoder::new(n_frames, frame_shape, signal_strength)?;
    for (seed, payload) in messages {
        encoder.queue_message(*seed, payload.to_vec());
    }

    let mut decoder = SignedStreamDecoder::new(
        n_frames,
        messages
            .first()
            .map(|(_, payload)| payload.len())
            .unwrap_or(0),
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
