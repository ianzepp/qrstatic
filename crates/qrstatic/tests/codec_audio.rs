use qrstatic::codec::audio::{
    AudioConfig, AudioDecoder, AudioEncoder, AudioStreamDecoder, AudioStreamEncoder,
};

fn synthetic_noise(len: usize, seed: &str) -> Vec<f32> {
    let mut rng = qrstatic::Prng::from_str_seed(seed);
    (0..len).map(|_| rng.next_range(-1.0, 1.0)).collect()
}

#[test]
fn roundtrip_with_synthetic_samples() {
    let config = AudioConfig::new(60, 64 * 64, 0.4, "audio-seed");
    let cover = synthetic_noise(config.n_frames * config.frame_size, "cover");
    let encoded = AudioEncoder::new(config.clone())
        .unwrap()
        .encode_samples(&cover, "audio-static")
        .unwrap();
    let decoded = AudioDecoder::new(config)
        .unwrap()
        .decode_samples(&encoded)
        .unwrap();
    assert_eq!(decoded.message.as_deref(), Some("audio-static"));
}

#[test]
fn different_frame_sizes() {
    for frame_size in [32 * 32, 64 * 64] {
        let config = AudioConfig::new(60, frame_size, 0.4, "frame-size");
        let cover = synthetic_noise(config.n_frames * config.frame_size, "cover");
        let encoded = AudioEncoder::new(config.clone())
            .unwrap()
            .encode_samples(&cover, "frame-key")
            .unwrap();
        let decoded = AudioDecoder::new(config)
            .unwrap()
            .decode_samples(&encoded)
            .unwrap();
        assert_eq!(decoded.message.as_deref(), Some("frame-key"));
    }
}

#[test]
fn streaming_sample_by_sample() {
    let config = AudioConfig::new(60, 64 * 64, 0.4, "stream-seed");
    let cover = synthetic_noise(config.n_frames * config.frame_size, "cover");
    let mut encoder = AudioStreamEncoder::new(config.clone(), "stream-key").unwrap();
    let mut decoder = AudioStreamDecoder::new(config).unwrap();
    let mut final_result = None;
    for sample in cover {
        let encoded = encoder.encode_sample(sample);
        if let Some(result) = decoder.push_sample(encoded).unwrap() {
            final_result = Some(result);
        }
    }
    assert_eq!(
        final_result.and_then(|result| result.message),
        Some("stream-key".to_string())
    );
}

#[test]
fn streaming_chunk_based() {
    let config = AudioConfig::new(60, 64 * 64, 0.4, "chunk-seed");
    let cover = synthetic_noise(config.n_frames * config.frame_size, "cover");
    let mut encoder = AudioStreamEncoder::new(config.clone(), "chunk-key").unwrap();
    let encoded = encoder.encode_chunk(&cover);
    let mut decoder = AudioStreamDecoder::new(config).unwrap();
    let mut results = Vec::new();
    for chunk in encoded.chunks(997) {
        results.extend(decoder.push_chunk(chunk).unwrap());
    }
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].message.as_deref(), Some("chunk-key"));
}

#[test]
fn qr_is_recoverable_after_n_frames_of_samples() {
    let config = AudioConfig::new(60, 64 * 64, 0.4, "emerge-seed");
    let cover = synthetic_noise(config.n_frames * config.frame_size, "cover");
    let mut encoder = AudioStreamEncoder::new(config.clone(), "emerge-key").unwrap();
    let mut decoder = AudioStreamDecoder::new(config.clone()).unwrap();

    for sample in cover.iter().take((config.n_frames - 1) * config.frame_size) {
        let encoded = encoder.encode_sample(*sample);
        assert!(decoder.push_sample(encoded).unwrap().is_none());
    }

    for sample in cover.iter().skip((config.n_frames - 1) * config.frame_size) {
        let encoded = encoder.encode_sample(*sample);
        if let Some(result) = decoder.push_sample(encoded).unwrap() {
            assert_eq!(result.message.as_deref(), Some("emerge-key"));
            return;
        }
    }

    panic!("audio QR was not recoverable after the full sample window");
}
