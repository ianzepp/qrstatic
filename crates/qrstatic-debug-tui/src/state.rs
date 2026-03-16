use std::time::{Duration, Instant};

use qrstatic::codec::temporal::{
    TemporalConfig, TemporalDecodePolicy, TemporalDecoder, TemporalEncoder, naive_field,
    try_extract_qr,
};
use qrstatic::Grid;

const MAX_WINDOW_HISTORY: usize = 24;
const MAX_LAYER2_SAMPLES: usize = 512;

pub struct AppState {
    pub config: ViewerConfig,
    pub stream_windows: Vec<StreamWindow>,
    pub naive_accumulator: Grid<f32>,
    pub correlation_field: Grid<f32>,
    pub frame_index: usize,
    pub current_window_index: usize,
    pub stream_passes: usize,
    pub last_tick: Instant,
    pub tick_interval: Duration,
    pub is_playing: bool,
    pub stats: StatsSnapshot,
    pub layer1_windows: Vec<Layer1WindowStatus>,
    pub layer2_samples: Vec<f32>,
    pub decoded_bytes: Vec<u8>,
    pub last_qr_decode: Option<LastQrDecode>,
}

#[derive(Debug, Clone)]
pub struct LastQrDecode {
    pub window_number: usize,
    pub key: String,
    pub message: String,
    pub detector_score: f32,
}

pub struct StreamWindow {
    pub key: String,
    pub qr_payload: String,
    pub frames: Vec<Grid<f32>>,
}

#[allow(dead_code)]
pub struct Layer1WindowStatus {
    pub window_number: usize,
    pub key: String,
    pub decoded_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ViewerConfig {
    pub width: usize,
    pub height: usize,
    pub n_frames: usize,
    pub noise_amplitude: f32,
    pub l1_amplitude: f32,
    pub min_detector_score: f32,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct StatsSnapshot {
    pub display_frame: usize,
    pub window_number: usize,
    pub stream_position: usize,
    pub naive_min: f32,
    pub naive_max: f32,
    pub naive_mean_abs: f32,
    pub corr_min: f32,
    pub corr_max: f32,
    pub corr_mean_abs: f32,
    pub corr_mean_signed: f32,
    pub current_key: String,
    pub current_qr_payload: String,
    pub decoded_message: Option<String>,
    pub naive_qr_visible: bool,
    pub detector_score: Option<f32>,
}

#[derive(Debug, Clone)]
pub struct Args {
    pub master_key: String,
    pub qr_payload: String,
    pub width: usize,
    pub height: usize,
    pub n_frames: usize,
    pub stream_windows: usize,
    pub noise_amplitude: f32,
    pub l1_amplitude: f32,
    pub fps: f32,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            master_key: "qrstatic-debug".to_string(),
            qr_payload: "temporal-bootstrap".to_string(),
            width: 41,
            height: 41,
            n_frames: 64,
            stream_windows: 24,
            noise_amplitude: 0.42,
            l1_amplitude: 0.22,
            fps: 12.0,
        }
    }
}

impl Args {
    pub fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self::default();

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--master-key" => parsed.master_key = next_value(&mut args, "--master-key")?,
                "--qr-payload" => parsed.qr_payload = next_value(&mut args, "--qr-payload")?,
                "--width" => {
                    parsed.width = parse_usize(&next_value(&mut args, "--width")?, "--width")?
                }
                "--height" => {
                    parsed.height = parse_usize(&next_value(&mut args, "--height")?, "--height")?
                }
                "--frames" => {
                    parsed.n_frames =
                        parse_usize(&next_value(&mut args, "--frames")?, "--frames")?
                }
                "--stream-windows" => {
                    parsed.stream_windows = parse_usize(
                        &next_value(&mut args, "--stream-windows")?,
                        "--stream-windows",
                    )?
                }
                "--noise-amplitude" => {
                    parsed.noise_amplitude = parse_f32(
                        &next_value(&mut args, "--noise-amplitude")?,
                        "--noise-amplitude",
                    )?
                }
                "--l1-amplitude" => {
                    parsed.l1_amplitude = parse_f32(
                        &next_value(&mut args, "--l1-amplitude")?,
                        "--l1-amplitude",
                    )?
                }
                "--fps" => {
                    parsed.fps = parse_f32(&next_value(&mut args, "--fps")?, "--fps")?
                }
                "--help" | "-h" => return Err(help_text()),
                other => return Err(format!("unknown flag: {other}\n\n{}", help_text())),
            }
        }

        if parsed.width == 0 || parsed.height == 0 {
            return Err("viewer dimensions must be non-zero".into());
        }
        if parsed.fps <= 0.0 {
            return Err("--fps must be greater than zero".into());
        }
        if parsed.stream_windows == 0 {
            return Err("--stream-windows must be greater than zero".into());
        }

        let qr_grid = qrstatic::qr::encode::encode(&parsed.qr_payload)
            .map_err(|err| format!("failed to encode qr-payload for sizing: {err}"))?;
        if parsed.width < qr_grid.width() || parsed.height < qr_grid.height() {
            return Err(format!(
                "frame size {}x{} is smaller than the QR size {}x{} required for the current key",
                parsed.width,
                parsed.height,
                qr_grid.width(),
                qr_grid.height()
            ));
        }

        Ok(parsed)
    }
}

fn help_text() -> String {
    [
        "qrstatic-debug-tui",
        "",
        "USAGE:",
        "    cargo run -p qrstatic-debug-tui -- [options]",
        "",
        "OPTIONS:",
        "    --master-key <text>",
        "    --qr-payload <text>",
        "    --width <cells>",
        "    --height <cells>",
        "    --frames <count>",
        "    --stream-windows <count>",
        "    --noise-amplitude <float>",
        "    --l1-amplitude <float>",
        "    --fps <float>",
        "",
        "KEYS:",
        "    q / Ctrl+C    quit",
        "    Space          play/pause",
        "    n / Right      step forward",
    ]
    .join("\n")
}

fn next_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("missing value for {flag}"))
}

fn parse_usize(value: &str, flag: &str) -> Result<usize, String> {
    value
        .parse::<usize>()
        .map_err(|_| format!("invalid usize for {flag}: {value}"))
}

fn parse_f32(value: &str, flag: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .map_err(|_| format!("invalid f32 for {flag}: {value}"))
}

impl AppState {
    pub fn new(args: Args) -> Result<Self, String> {
        let config = TemporalConfig::new(
            (args.width, args.height),
            args.n_frames,
            args.noise_amplitude,
            args.l1_amplitude,
        )
        .map_err(|err| format!("failed to construct temporal config: {err}"))?;
        let encoder = TemporalEncoder::new(config.clone())
            .map_err(|err| format!("failed to construct temporal encoder: {err}"))?;
        let stream_windows = build_stream_windows(&encoder, &args)
            .map_err(|err| format!("failed to generate temporal debug stream: {err}"))?;

        let viewer_config = ViewerConfig {
            width: args.width,
            height: args.height,
            n_frames: args.n_frames,
            noise_amplitude: args.noise_amplitude,
            l1_amplitude: args.l1_amplitude,
            min_detector_score: 6.0,
        };

        Ok(Self {
            config: viewer_config,
            stream_windows,
            naive_accumulator: Grid::new(args.width, args.height),
            correlation_field: Grid::new(args.width, args.height),
            frame_index: 0,
            current_window_index: 0,
            stream_passes: 0,
            last_tick: Instant::now(),
            tick_interval: Duration::from_secs_f32(1.0 / args.fps),
            is_playing: true,
            stats: StatsSnapshot {
                display_frame: 0,
                window_number: 1,
                stream_position: 0,
                naive_min: 0.0,
                naive_max: 0.0,
                naive_mean_abs: 0.0,
                corr_min: 0.0,
                corr_max: 0.0,
                corr_mean_abs: 0.0,
                corr_mean_signed: 0.0,
                current_key: String::new(),
                current_qr_payload: String::new(),
                decoded_message: None,
                naive_qr_visible: false,
                detector_score: None,
            },
            layer1_windows: Vec::new(),
            layer2_samples: Vec::with_capacity(MAX_LAYER2_SAMPLES),
            decoded_bytes: Vec::new(),
            last_qr_decode: None,
        })
    }

    pub fn advance(&mut self) {
        if self.frame_index == self.current_frames().len() {
            self.finish_window();
            self.frame_index = 0;
            self.current_window_index += 1;
            if self.current_window_index == self.stream_windows.len() {
                self.current_window_index = 0;
                self.stream_passes += 1;
            }
            self.naive_accumulator = Grid::new(self.config.width, self.config.height);
            self.correlation_field = Grid::new(self.config.width, self.config.height);
        }

        let frame = self.current_frames()[self.frame_index].clone();
        for (sum, &cell) in self
            .naive_accumulator
            .data_mut()
            .iter_mut()
            .zip(frame.data().iter())
        {
            *sum += cell;
        }

        self.frame_index += 1;
        self.correlation_field = compute_correlation_prefix(
            self.current_window(),
            &self.config,
            &self.current_frames()[..self.frame_index],
        )
        .unwrap_or_else(|_| Grid::new(self.config.width, self.config.height));
        self.stats = compute_stats(
            self.current_window(),
            &self.config,
            &self.naive_accumulator,
            &self.correlation_field,
            &self.current_frames()[..self.frame_index],
            self.frame_index,
            self.current_window_number(),
        );
        self.layer2_samples.push(self.stats.corr_mean_signed);
        if self.layer2_samples.len() > MAX_LAYER2_SAMPLES {
            let overflow = self.layer2_samples.len() - MAX_LAYER2_SAMPLES;
            self.layer2_samples.drain(..overflow);
        }
    }

    pub fn current_frame(&self) -> &Grid<f32> {
        let index = self
            .frame_index
            .saturating_sub(1)
            .min(self.current_frames().len() - 1);
        &self.current_frames()[index]
    }

    pub fn current_window(&self) -> &StreamWindow {
        &self.stream_windows[self.current_window_index]
    }

    pub fn current_frames(&self) -> &[Grid<f32>] {
        &self.current_window().frames
    }

    pub fn current_window_number(&self) -> usize {
        self.stream_passes * self.stream_windows.len() + self.current_window_index + 1
    }

    fn finish_window(&mut self) {
        if self.frame_index == 0 {
            return;
        }
        let window_number = self.current_window_number();
        let decoded_message = self.stats.decoded_message.clone();

        // Capture last successful QR decode (persists across resets)
        if let Some(ref msg) = decoded_message {
            self.last_qr_decode = Some(LastQrDecode {
                window_number,
                key: self.current_window().key.clone(),
                message: msg.clone(),
                detector_score: self.stats.detector_score.unwrap_or(0.0),
            });
            // Append decoded message bytes to hex buffer
            self.decoded_bytes.extend_from_slice(msg.as_bytes());
        }

        self.layer1_windows.push(Layer1WindowStatus {
            window_number,
            key: self.current_window().key.clone(),
            decoded_message,
        });
        if self.layer1_windows.len() > MAX_WINDOW_HISTORY {
            let overflow = self.layer1_windows.len() - MAX_WINDOW_HISTORY;
            self.layer1_windows.drain(..overflow);
        }
    }
}

fn compute_stats(
    window: &StreamWindow,
    config: &ViewerConfig,
    naive_accumulator: &Grid<f32>,
    correlation_field: &Grid<f32>,
    frames: &[Grid<f32>],
    display_frame: usize,
    window_number: usize,
) -> StatsSnapshot {
    let naive_min = naive_accumulator
        .data()
        .iter()
        .copied()
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    let naive_max = naive_accumulator
        .data()
        .iter()
        .copied()
        .max_by(f32::total_cmp)
        .unwrap_or(0.0);
    let naive_mean_abs = if naive_accumulator.is_empty() {
        0.0
    } else {
        naive_accumulator
            .data()
            .iter()
            .map(|value| value.abs())
            .sum::<f32>()
            / naive_accumulator.len() as f32
    };
    let corr_min = correlation_field
        .data()
        .iter()
        .copied()
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    let corr_max = correlation_field
        .data()
        .iter()
        .copied()
        .max_by(f32::total_cmp)
        .unwrap_or(0.0);
    let corr_mean_abs = if correlation_field.is_empty() {
        0.0
    } else {
        correlation_field
            .data()
            .iter()
            .map(|value| value.abs())
            .sum::<f32>()
            / correlation_field.len() as f32
    };
    let corr_mean_signed = if correlation_field.is_empty() {
        0.0
    } else {
        correlation_field.data().iter().sum::<f32>() / correlation_field.len() as f32
    };

    let temporal_config = TemporalConfig::new(
        (config.width, config.height),
        config.n_frames,
        config.noise_amplitude,
        config.l1_amplitude,
    )
    .ok();
    let decoder = temporal_config.and_then(|cfg| TemporalDecoder::new(cfg).ok());
    let decoded = decoder.as_ref().and_then(|decoder| {
        if frames.len() == config.n_frames {
            decoder
                .decode_qr(
                    frames,
                    &window.key,
                    &TemporalDecodePolicy::fixed_threshold(config.min_detector_score).ok()?,
                )
                .ok()
        } else {
            None
        }
    });
    let detector_score = decoder
        .as_ref()
        .and_then(|decoder| decoder.correlate_prefix(frames, &window.key).ok())
        .map(|correlation| correlation.detector_score);
    let naive_qr_visible = naive_field(frames)
        .ok()
        .and_then(|field| try_extract_qr(&field))
        .is_some();
    let decoded_message = decoded.as_ref().and_then(|result| result.message.clone());

    StatsSnapshot {
        display_frame,
        window_number,
        stream_position: (window_number - 1) * config.n_frames + display_frame,
        naive_min,
        naive_max,
        naive_mean_abs,
        corr_min,
        corr_max,
        corr_mean_abs,
        corr_mean_signed,
        current_key: window.key.clone(),
        current_qr_payload: window.qr_payload.clone(),
        decoded_message,
        naive_qr_visible,
        detector_score,
    }
}

fn compute_correlation_prefix(
    window: &StreamWindow,
    config: &ViewerConfig,
    frames: &[Grid<f32>],
) -> Result<Grid<f32>, String> {
    let temporal_config = TemporalConfig::new(
        (config.width, config.height),
        config.n_frames,
        config.noise_amplitude,
        config.l1_amplitude,
    )
    .map_err(|err| format!("failed to construct temporal config: {err}"))?;
    let decoder = TemporalDecoder::new(temporal_config)
        .map_err(|err| format!("failed to construct temporal decoder: {err}"))?;
    decoder
        .correlate_prefix(frames, &window.key)
        .map(|correlation| correlation.field)
        .map_err(|err| format!("failed to correlate temporal prefix: {err}"))
}

fn build_stream_windows(
    encoder: &TemporalEncoder,
    args: &Args,
) -> Result<Vec<StreamWindow>, String> {
    let mut windows = Vec::with_capacity(args.stream_windows);
    for index in 0..args.stream_windows {
        let key = format!("{}-w{:04}", args.master_key, index + 1);
        let qr_payload = format!("{}-w{:04}", args.qr_payload, index + 1);
        let frames = encoder
            .encode_message(&key, &qr_payload)
            .map_err(|err| format!("window {}: failed to encode frames: {err}", index + 1))?;
        windows.push(StreamWindow {
            key,
            qr_payload,
            frames,
        });
    }
    Ok(windows)
}

pub fn map_symmetric_to_u8(value: f32, amplitude: f32) -> u8 {
    let normalized = if amplitude > 0.0 {
        (value / amplitude).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (128.0 + normalized * 127.0).round().clamp(0.0, 255.0) as u8
}
