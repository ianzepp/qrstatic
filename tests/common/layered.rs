use qrstatic::codec::layered::{
    LayeredConfig, LayeredDecoder, LayeredEncoder, LayeredStreamDecoder, LayeredStreamEncoder,
};
use qrstatic::{Grid, LayeredDecodeResult, Result};

pub fn layered_roundtrip(
    frame_shape: (usize, usize),
    n1: usize,
    n2: usize,
    layer1_key: &str,
    layer2_key: &str,
    payload: &[u8],
) -> Result<LayeredDecodeResult> {
    let encoder = LayeredEncoder::new(LayeredConfig::new(frame_shape, n1, n2))?;
    let frames = encoder.encode(layer1_key, layer2_key, payload)?;
    LayeredDecoder::new(LayeredConfig::new(frame_shape, n1, n2), payload.len())?.decode(&frames)
}

pub fn layered_frames(
    frame_shape: (usize, usize),
    n1: usize,
    n2: usize,
    layer1_key: &str,
    layer2_key: &str,
    payload: &[u8],
) -> Result<Vec<Grid<f32>>> {
    LayeredEncoder::new(LayeredConfig::new(frame_shape, n1, n2))?
        .encode(layer1_key, layer2_key, payload)
}

pub fn layered_stream_roundtrip(
    frame_shape: (usize, usize),
    n1: usize,
    n2: usize,
    layer1_key: &str,
    layer2_key: &str,
    payload: &[u8],
) -> Result<Vec<LayeredDecodeResult>> {
    let mut encoder = LayeredStreamEncoder::new(LayeredConfig::new(frame_shape, n1, n2))?;
    encoder.queue_message(
        layer1_key.to_owned(),
        layer2_key.to_owned(),
        payload.to_vec(),
    );

    let mut decoder =
        LayeredStreamDecoder::new(LayeredConfig::new(frame_shape, n1, n2), payload.len())?;
    let mut outputs = Vec::new();
    while let Some(frame) = encoder.next_frame()? {
        if let Some(result) = decoder.push_frame(frame)? {
            outputs.push(result);
        }
    }
    Ok(outputs)
}
