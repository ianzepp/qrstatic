use qrstatic::codec::temporal::{TemporalConfig, TemporalDecoder, TemporalEncoder};
use qrstatic::qr;

fn test_config() -> TemporalConfig {
    TemporalConfig::new((41, 41), 64, 0.5, 0.35).unwrap()
}

#[test]
fn keyed_correlation_recovers_layer1_qr_message() {
    let config = test_config();
    let encoder = TemporalEncoder::new(config.clone()).unwrap();
    let decoder = TemporalDecoder::new(config).unwrap();

    let frames = encoder
        .encode_message("temporal-master", "temporal-visible")
        .unwrap();
    let decoded = decoder.decode_qr(&frames, "temporal-master").unwrap();

    assert_eq!(decoded.message.as_deref(), Some("temporal-visible"));
    assert!(decoded.detector_score > 1.0);
}

#[test]
fn wrong_key_fails_closed() {
    let config = test_config();
    let encoder = TemporalEncoder::new(config.clone()).unwrap();
    let decoder = TemporalDecoder::new(config).unwrap();

    let frames = encoder
        .encode_message("correct-master", "visible-bootstrap")
        .unwrap();
    let wrong = decoder.decode_qr(&frames, "wrong-master");

    assert!(wrong.is_err());
}

#[test]
fn wrong_window_fails_closed() {
    let config = test_config();
    let encoder = TemporalEncoder::new(config.clone()).unwrap();
    let decoder = TemporalDecoder::new(config).unwrap();

    let frames_a = encoder.encode_message("window-master", "window-qr").unwrap();
    let frames_b = encoder.encode_message("other-master", "other-qr").unwrap();

    let mut shifted = frames_a[1..].to_vec();
    shifted.push(frames_b[0].clone());

    let wrong = decoder.decode_qr(&shifted, "window-master");
    assert!(wrong.is_err());
}

#[test]
fn naive_accumulation_does_not_decode_layer1() {
    let config = test_config();
    let encoder = TemporalEncoder::new(config.clone()).unwrap();
    let decoder = TemporalDecoder::new(config).unwrap();

    let frames = encoder
        .encode_message("naive-master", "naive-bootstrap")
        .unwrap();
    let naive = decoder.naive_decode_qr(&frames).unwrap();

    assert!(naive.is_none());
}

#[test]
fn correct_key_score_exceeds_wrong_key_and_naive_baselines() {
    let config = test_config();
    let encoder = TemporalEncoder::new(config.clone()).unwrap();
    let decoder = TemporalDecoder::new(config).unwrap();

    let frames = encoder
        .encode_message("score-master", "score-bootstrap")
        .unwrap();

    let correct = decoder.correlation_score(&frames, "score-master").unwrap();
    let wrong = decoder.correlation_score(&frames, "other-master").unwrap();
    let naive = decoder.naive_score(&frames).unwrap();

    assert!(correct > wrong, "correct score {correct} should exceed wrong-key score {wrong}");
    assert!(correct > naive, "correct score {correct} should exceed naive score {naive}");
}

#[test]
fn single_frame_does_not_reveal_centered_qr_layout() {
    let config = test_config();
    let encoder = TemporalEncoder::new(config.clone()).unwrap();

    let frames = encoder
        .encode_message("single-frame-master", "single-frame-bootstrap")
        .unwrap();
    let first = &frames[0];

    let qr_grid = qr::encode::encode("single-frame-bootstrap").unwrap();
    let sign_grid = first.map(|&value| u8::from(value < 0.0));
    let row_offset = (config.frame_shape.1 - qr_grid.height()) / 2;
    let col_offset = (config.frame_shape.0 - qr_grid.width()) / 2;
    let mut centered_data = Vec::with_capacity(qr_grid.len());
    for row in 0..qr_grid.height() {
        for col in 0..qr_grid.width() {
            centered_data.push(sign_grid[(row + row_offset, col + col_offset)]);
        }
    }
    let centered_crop = qrstatic::Grid::from_vec(centered_data, qr_grid.width(), qr_grid.height());
    assert!(qr::decode::decode(&centered_crop).is_err());

    let mut agreement = 0usize;
    let mut total = 0usize;
    for row in 0..qr_grid.height() {
        for col in 0..qr_grid.width() {
            total += 1;
            if sign_grid[(row + row_offset, col + col_offset)] == qr_grid[(row, col)] {
                agreement += 1;
            }
        }
    }

    assert!(agreement < total * 7 / 10, "single-frame centered agreement too high: {agreement}/{total}");
}
