use qrstatic::codec::xor::{XorDecoder, XorEncoder, XorStreamDecoder, XorStreamEncoder};
use qrstatic::{DecodeResult, Grid, Result};

pub const TEST_MESSAGE: &str = "hello";

pub fn xor_roundtrip(n_frames: usize, seed: &str, message: &str) -> Result<DecodeResult> {
    let encoder = XorEncoder::new(n_frames, seed)?;
    let frames = encoder.encode_message(message)?;
    XorDecoder::decode_message(&frames)
}

pub fn xor_frames_for_message(n_frames: usize, seed: &str, message: &str) -> Result<Vec<Grid<u8>>> {
    let encoder = XorEncoder::new(n_frames, seed)?;
    encoder.encode_message(message)
}

pub fn xor_stream_roundtrip(
    n_frames: usize,
    seed: &str,
    messages: &[&str],
) -> Result<Vec<Option<String>>> {
    let mut encoder = XorStreamEncoder::new(n_frames, seed)?;
    for message in messages {
        encoder.queue_message(*message);
    }

    let mut decoder = XorStreamDecoder::new(n_frames)?;
    let mut decoded = Vec::new();
    for _ in 0..(n_frames * messages.len()) {
        let frame = encoder.next_frame()?;
        if let Some(result) = decoder.push_frame(frame)? {
            decoded.push(result.message);
        }
    }

    Ok(decoded)
}
