use qrstatic::codec::sliding::{
    SlidingConfig, SlidingDecoder, SlidingEncoder, SlidingStreamDecoder, SlidingStreamEncoder,
};
use qrstatic::{Grid, Result, SlidingDecodeResult};

pub fn sliding_frames(
    config: SlidingConfig,
    l1_key: &str,
    l2_key: Option<&str>,
    payload: &[u8],
    total_frames: usize,
) -> Result<Vec<Grid<f32>>> {
    SlidingEncoder::new(config)?.encode(l1_key, l2_key, payload, total_frames)
}

pub fn sliding_decode(
    config: SlidingConfig,
    frames: &[Grid<f32>],
    l1_start: usize,
    payload_len: usize,
) -> Result<SlidingDecodeResult> {
    SlidingDecoder::new(config, payload_len)?.decode(frames, l1_start)
}

pub fn sliding_stream(
    config: SlidingConfig,
    l1_key: &str,
    l2_key: Option<&str>,
    payload: &[u8],
    total_frames: usize,
) -> Result<Vec<SlidingDecodeResult>> {
    let mut encoder = SlidingStreamEncoder::new(config.clone(), l1_key.to_owned())?;
    if let Some(l2) = l2_key {
        encoder.set_l2_message(l2.to_owned(), payload.to_vec());
    }
    let mut decoder = SlidingStreamDecoder::new(config, payload.len())?;
    let mut outputs = Vec::new();
    for _ in 0..total_frames {
        let frame = encoder.next_frame()?;
        if let Some(result) = decoder.push_frame(frame)? {
            outputs.push(result);
        }
    }
    Ok(outputs)
}
