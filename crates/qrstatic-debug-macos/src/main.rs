use std::env;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, ColorImage, Context, Grid as UiGrid, RichText, TextureHandle,
    TextureOptions, Vec2,
};
use qrstatic::codec::temporal::{TemporalConfig, TemporalDecoder, TemporalEncoder};
use qrstatic::{Grid, qr};

const DEFAULT_WIDTH: usize = 41;
const DEFAULT_HEIGHT: usize = 41;
const DEFAULT_FRAMES: usize = 64;
const DEFAULT_NOISE_AMPLITUDE: f32 = 0.3;
const DEFAULT_L1_AMPLITUDE: f32 = 0.35;
const DEFAULT_FPS: f32 = 12.0;
const DEFAULT_MASTER_KEY: &str = "qrstatic-debug";
const DEFAULT_QR_PAYLOAD: &str = "temporal-bootstrap";

fn main() -> eframe::Result<()> {
    let args = Args::parse(env::args().skip(1)).map_err(eframe_error)?;
    let app = DebugViewerApp::new(args).map_err(eframe_error)?;
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([260.0, 320.0])
            .with_min_inner_size([240.0, 280.0])
            .with_title("qrstatic debug viewer"),
        ..Default::default()
    };

    eframe::run_native(
        "qrstatic debug viewer",
        native_options,
        Box::new(move |_cc| Ok(Box::new(app))),
    )
}

fn eframe_error(message: String) -> eframe::Error {
    eframe::Error::AppCreation(Box::new(SimpleError(message)))
}

#[derive(Debug)]
struct SimpleError(String);

impl std::fmt::Display for SimpleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for SimpleError {}

#[derive(Debug, Clone)]
struct Args {
    master_key: String,
    qr_payload: String,
    width: usize,
    height: usize,
    n_frames: usize,
    noise_amplitude: f32,
    l1_amplitude: f32,
    fps: f32,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self {
            master_key: DEFAULT_MASTER_KEY.to_string(),
            qr_payload: DEFAULT_QR_PAYLOAD.to_string(),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            n_frames: DEFAULT_FRAMES,
            noise_amplitude: DEFAULT_NOISE_AMPLITUDE,
            l1_amplitude: DEFAULT_L1_AMPLITUDE,
            fps: DEFAULT_FPS,
        };

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
                    parsed.n_frames = parse_usize(&next_value(&mut args, "--frames")?, "--frames")?
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
                "--fps" => parsed.fps = parse_f32(&next_value(&mut args, "--fps")?, "--fps")?,
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

        let qr_grid = qr::encode::encode(&parsed.qr_payload)
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
        "qrstatic-debug-macos",
        "",
        "USAGE:",
        "    cargo run -p qrstatic-debug-macos -- [options]",
        "",
        "TEMPORAL ONLY:",
        "    This viewer renders the fixed-window Layer 1 temporal codec.",
        "",
        "OPTIONS:",
        "    --master-key <text>",
        "    --qr-payload <text>",
        "    --width <cells>",
        "    --height <cells>",
        "    --frames <count>",
        "    --noise-amplitude <float>",
        "    --l1-amplitude <float>",
        "    --fps <float>",
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

struct DebugViewerApp {
    config: ViewerConfig,
    frames: Vec<Grid<f32>>,
    naive_accumulator: Grid<f32>,
    correlation_field: Grid<f32>,
    frame_index: usize,
    loop_count: usize,
    last_tick: Instant,
    tick_interval: Duration,
    is_playing: bool,
    stats: StatsSnapshot,
    static_texture: Option<TextureHandle>,
    correlation_texture: Option<TextureHandle>,
}

#[derive(Debug, Clone)]
struct ViewerConfig {
    master_key: String,
    qr_payload: String,
    width: usize,
    height: usize,
    n_frames: usize,
    noise_amplitude: f32,
    l1_amplitude: f32,
}

#[derive(Debug, Clone)]
struct StatsSnapshot {
    display_frame: usize,
    loop_count: usize,
    naive_min: f32,
    naive_max: f32,
    naive_mean_abs: f32,
    corr_min: f32,
    corr_max: f32,
    corr_mean_abs: f32,
    decoded_message: Option<String>,
    naive_qr_visible: bool,
    detector_score: Option<f32>,
}

impl DebugViewerApp {
    fn new(args: Args) -> Result<Self, String> {
        let config = TemporalConfig::new(
            (args.width, args.height),
            args.n_frames,
            args.noise_amplitude,
            args.l1_amplitude,
        )
        .map_err(|err| format!("failed to construct temporal config: {err}"))?;
        let encoder =
            TemporalEncoder::new(config.clone()).map_err(|err| format!("failed to construct temporal encoder: {err}"))?;
        let frames = encoder
            .encode_message(&args.master_key, &args.qr_payload)
            .map_err(|err| format!("failed to generate temporal debug frames: {err}"))?;

        let viewer_config = ViewerConfig {
            master_key: args.master_key,
            qr_payload: args.qr_payload,
            width: args.width,
            height: args.height,
            n_frames: args.n_frames,
            noise_amplitude: args.noise_amplitude,
            l1_amplitude: args.l1_amplitude,
        };

        Ok(Self {
            config: viewer_config,
            frames,
            naive_accumulator: Grid::new(args.width, args.height),
            correlation_field: Grid::new(args.width, args.height),
            frame_index: 0,
            loop_count: 0,
            last_tick: Instant::now(),
            tick_interval: Duration::from_secs_f32(1.0 / args.fps),
            is_playing: true,
            stats: StatsSnapshot {
                display_frame: 0,
                loop_count: 0,
                naive_min: 0.0,
                naive_max: 0.0,
                naive_mean_abs: 0.0,
                corr_min: 0.0,
                corr_max: 0.0,
                corr_mean_abs: 0.0,
                decoded_message: None,
                naive_qr_visible: false,
                detector_score: None,
            },
            static_texture: None,
            correlation_texture: None,
        })
    }

    fn advance(&mut self) {
        if self.frame_index == self.frames.len() {
            self.frame_index = 0;
            self.loop_count += 1;
            self.naive_accumulator = Grid::new(self.config.width, self.config.height);
            self.correlation_field = Grid::new(self.config.width, self.config.height);
        }

        let frame = &self.frames[self.frame_index];
        for (sum, &cell) in self
            .naive_accumulator
            .data_mut()
            .iter_mut()
            .zip(frame.data().iter())
        {
            *sum += cell;
        }

        self.frame_index += 1;
        self.correlation_field =
            compute_correlation_prefix(&self.config, &self.frames[..self.frame_index]).unwrap_or_else(
                |_| Grid::new(self.config.width, self.config.height),
            );
        self.stats = compute_stats(
            &self.config,
            &self.naive_accumulator,
            &self.correlation_field,
            &self.frames[..self.frame_index],
            self.frame_index,
            self.loop_count,
        );
    }

    fn current_frame(&self) -> &Grid<f32> {
        let index = self.frame_index.saturating_sub(1).min(self.frames.len() - 1);
        &self.frames[index]
    }

    fn update_textures(&mut self, ctx: &Context) {
        let static_image = render_static_image(self.current_frame(), self.config.noise_amplitude);
        let correlation_image = render_field_image(
            &self.correlation_field,
            self.config.l1_amplitude * self.config.n_frames as f32,
        );

        if let Some(texture) = &mut self.static_texture {
            texture.set(static_image, TextureOptions::NEAREST);
        } else {
            self.static_texture =
                Some(ctx.load_texture("static-frame", static_image, TextureOptions::NEAREST));
        }

        if let Some(texture) = &mut self.correlation_texture {
            texture.set(correlation_image, TextureOptions::NEAREST);
        } else {
            self.correlation_texture = Some(ctx.load_texture(
                "correlation-frame",
                correlation_image,
                TextureOptions::NEAREST,
            ));
        }
    }
}

impl eframe::App for DebugViewerApp {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        if self.frame_index == 0 {
            self.advance();
        }

        if self.is_playing {
            let now = Instant::now();
            while now.duration_since(self.last_tick) >= self.tick_interval {
                self.last_tick += self.tick_interval;
                self.advance();
            }
        }

        self.update_textures(ctx);
        ctx.request_repaint_after(self.tick_interval);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                let pane_size = Vec2::splat(ui.available_width().max(1.0));

                if let Some(texture) = &self.static_texture {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(pane_size));
                }

                ui.add_space(8.0);
                draw_stats(ui, &self.config, &self.stats, &mut self.is_playing);
                ui.add_space(8.0);

                if let Some(texture) = &self.correlation_texture {
                    ui.add(egui::Image::new(texture).fit_to_exact_size(pane_size));
                }
            });
        });
    }
}

fn draw_stats(
    ui: &mut egui::Ui,
    config: &ViewerConfig,
    stats: &StatsSnapshot,
    is_playing: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Temporal Stats").strong());
        let label = if *is_playing { "Pause" } else { "Play" };
        if ui.button(label).clicked() {
            *is_playing = !*is_playing;
        }
    });

    UiGrid::new("stats-grid")
        .num_columns(2)
        .spacing([10.0, 2.0])
        .show(ui, |ui| {
            stat_row(ui, "state", if *is_playing { "playing" } else { "paused" });
            stat_row(ui, "frame", &format!("{}/{}", stats.display_frame, config.n_frames));
            stat_row(ui, "loop", &stats.loop_count.to_string());
            stat_row(
                ui,
                "naive",
                &format!("{:.2} .. {:.2}", stats.naive_min, stats.naive_max),
            );
            stat_row(ui, "naive |x|", &format!("{:.2}", stats.naive_mean_abs));
            stat_row(
                ui,
                "corr",
                &format!("{:.2} .. {:.2}", stats.corr_min, stats.corr_max),
            );
            stat_row(ui, "corr |x|", &format!("{:.2}", stats.corr_mean_abs));
            stat_row(
                ui,
                "decode",
                stats.decoded_message.as_deref().unwrap_or("none"),
            );
            stat_row(ui, "master", &config.master_key);
            stat_row(ui, "qr", &config.qr_payload);
            stat_row(
                ui,
                "naive qr",
                if stats.naive_qr_visible { "visible" } else { "none" },
            );
            stat_row(
                ui,
                "detector",
                &stats
                    .detector_score
                    .map(|score| format!("{score:.2}"))
                    .unwrap_or_else(|| "pending".to_string()),
            );
            stat_row(
                ui,
                "temporal",
                &format!("{:.2} / {:.2}", config.noise_amplitude, config.l1_amplitude),
            );
        });
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.with_layout(egui::Layout::left_to_right(Align::Min), |ui| {
        ui.label(RichText::new(label).color(Color32::LIGHT_GRAY));
    });
    ui.label(value);
    ui.end_row();
}

fn compute_stats(
    config: &ViewerConfig,
    naive_accumulator: &Grid<f32>,
    correlation_field: &Grid<f32>,
    frames: &[Grid<f32>],
    display_frame: usize,
    loop_count: usize,
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

    let temporal_config = TemporalConfig::new(
        (config.width, config.height),
        config.n_frames,
        config.noise_amplitude,
        config.l1_amplitude,
    )
    .ok();
    let decoder = temporal_config
        .and_then(|cfg| TemporalDecoder::new(cfg).ok());
    let decoded = decoder
        .as_ref()
        .and_then(|decoder| {
            if frames.len() == config.n_frames {
                decoder.decode_qr(frames, &config.master_key).ok()
            } else {
                None
            }
        });
    let naive_qr_visible = decoder
        .as_ref()
        .and_then(|decoder| decoder.naive_decode_qr(frames).ok())
        .flatten()
        .is_some();
    let decoded_message = decoded.as_ref().and_then(|result| result.message.clone());
    let detector_score = decoded.as_ref().map(|result| result.detector_score);

    StatsSnapshot {
        display_frame,
        loop_count,
        naive_min,
        naive_max,
        naive_mean_abs,
        corr_min,
        corr_max,
        corr_mean_abs,
        decoded_message,
        naive_qr_visible,
        detector_score,
    }
}

fn render_static_image(frame: &Grid<f32>, noise_amplitude: f32) -> ColorImage {
    let pixels: Vec<Color32> = frame
        .data()
        .iter()
        .map(|&value| Color32::from_gray(map_symmetric_to_u8(value, noise_amplitude)))
        .collect();
    ColorImage {
        size: [frame.width(), frame.height()],
        pixels,
    }
}

fn render_field_image(field: &Grid<f32>, fixed_range: f32) -> ColorImage {
    let dynamic_range = fixed_range.max(1e-6);
    let pixels: Vec<Color32> = field
        .data()
        .iter()
        .map(|&value| Color32::from_gray(map_symmetric_to_u8(value, dynamic_range)))
        .collect();
    ColorImage {
        size: [field.width(), field.height()],
        pixels,
    }
}

fn compute_correlation_prefix(config: &ViewerConfig, frames: &[Grid<f32>]) -> Result<Grid<f32>, String> {
    let temporal_config = TemporalConfig::new(
        (config.width, config.height),
        config.n_frames,
        config.noise_amplitude,
        config.l1_amplitude,
    )
    .map_err(|err| format!("failed to construct temporal config: {err}"))?;
    let decoder =
        TemporalDecoder::new(temporal_config).map_err(|err| format!("failed to construct temporal decoder: {err}"))?;
    decoder
        .correlate_prefix(frames, &config.master_key)
        .map_err(|err| format!("failed to correlate temporal prefix: {err}"))
}

fn map_symmetric_to_u8(value: f32, amplitude: f32) -> u8 {
    let normalized = if amplitude > 0.0 {
        (value / amplitude).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (128.0 + normalized * 127.0).round().clamp(0.0, 255.0) as u8
}
