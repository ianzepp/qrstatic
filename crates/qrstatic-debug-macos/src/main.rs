use std::env;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, ColorImage, Context, Grid as UiGrid, RichText, TextureHandle,
    TextureOptions, Vec2,
};
use qrstatic::codec::analog::{AnalogDecoder, AnalogEncoder};
use qrstatic::{Grid, qr};

const DEFAULT_WIDTH: usize = 41;
const DEFAULT_HEIGHT: usize = 41;
const DEFAULT_FRAMES: usize = 60;
const DEFAULT_NOISE_AMPLITUDE: f32 = 0.3;
const DEFAULT_SIGNAL_STRENGTH: f32 = 0.5;
const DEFAULT_PAYLOAD_DELTA: f32 = 0.1;
const DEFAULT_FPS: f32 = 12.0;
const DEFAULT_SEED: &str = "qrstatic-debug";
const DEFAULT_QR_KEY: &str = "debug-window";
const DEFAULT_PAYLOAD_TEXT: &str = "tracking tests";

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
    qr_key: String,
    payload_text: String,
    width: usize,
    height: usize,
    n_frames: usize,
    seed: String,
    noise_amplitude: f32,
    signal_strength: f32,
    payload_delta: f32,
    fps: f32,
}

impl Args {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Self, String> {
        let mut parsed = Self {
            qr_key: DEFAULT_QR_KEY.to_string(),
            payload_text: DEFAULT_PAYLOAD_TEXT.to_string(),
            width: DEFAULT_WIDTH,
            height: DEFAULT_HEIGHT,
            n_frames: DEFAULT_FRAMES,
            seed: DEFAULT_SEED.to_string(),
            noise_amplitude: DEFAULT_NOISE_AMPLITUDE,
            signal_strength: DEFAULT_SIGNAL_STRENGTH,
            payload_delta: DEFAULT_PAYLOAD_DELTA,
            fps: DEFAULT_FPS,
        };

        while let Some(flag) = args.next() {
            match flag.as_str() {
                "--qr-key" => parsed.qr_key = next_value(&mut args, "--qr-key")?,
                "--payload-text" => {
                    parsed.payload_text = next_value(&mut args, "--payload-text")?
                }
                "--width" => {
                    parsed.width = parse_usize(&next_value(&mut args, "--width")?, "--width")?
                }
                "--height" => {
                    parsed.height = parse_usize(&next_value(&mut args, "--height")?, "--height")?
                }
                "--frames" => {
                    parsed.n_frames = parse_usize(&next_value(&mut args, "--frames")?, "--frames")?
                }
                "--seed" => parsed.seed = next_value(&mut args, "--seed")?,
                "--noise-amplitude" => {
                    parsed.noise_amplitude = parse_f32(
                        &next_value(&mut args, "--noise-amplitude")?,
                        "--noise-amplitude",
                    )?
                }
                "--signal-strength" => {
                    parsed.signal_strength = parse_f32(
                        &next_value(&mut args, "--signal-strength")?,
                        "--signal-strength",
                    )?
                }
                "--payload-delta" => {
                    parsed.payload_delta = parse_f32(
                        &next_value(&mut args, "--payload-delta")?,
                        "--payload-delta",
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

        let qr_grid = qr::encode::encode(&parsed.qr_key)
            .map_err(|err| format!("failed to encode qr-key for sizing: {err}"))?;
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
        "OPTIONS:",
        "    --qr-key <text>",
        "    --payload-text <text>",
        "    --width <cells>",
        "    --height <cells>",
        "    --frames <count>",
        "    --seed <text>",
        "    --noise-amplitude <float>",
        "    --signal-strength <float>",
        "    --payload-delta <float>",
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
    accumulator: Grid<f32>,
    frame_index: usize,
    loop_count: usize,
    last_tick: Instant,
    tick_interval: Duration,
    is_playing: bool,
    stats: StatsSnapshot,
    static_texture: Option<TextureHandle>,
    accumulation_texture: Option<TextureHandle>,
}

#[derive(Debug, Clone)]
struct ViewerConfig {
    qr_key: String,
    payload_text: String,
    seed: String,
    width: usize,
    height: usize,
    n_frames: usize,
    noise_amplitude: f32,
    signal_strength: f32,
    payload_delta: f32,
}

#[derive(Debug, Clone)]
struct StatsSnapshot {
    display_frame: usize,
    loop_count: usize,
    accum_min: f32,
    accum_max: f32,
    mean_abs: f32,
    decoded_message: Option<String>,
    payload_ok: Option<bool>,
}

impl DebugViewerApp {
    fn new(args: Args) -> Result<Self, String> {
        let encoder = AnalogEncoder::new(
            args.n_frames,
            (args.width, args.height),
            args.noise_amplitude,
            args.signal_strength,
            args.payload_delta,
        )
        .map_err(|err| format!("failed to construct analog encoder: {err}"))?;

        let payload_bytes = args.payload_text.as_bytes().to_vec();
        let frames = encoder
            .encode_message(&args.qr_key, &payload_bytes)
            .map_err(|err| format!("failed to generate analog debug frames: {err}"))?;

        let config = ViewerConfig {
            qr_key: args.qr_key,
            payload_text: args.payload_text,
            seed: args.seed,
            width: args.width,
            height: args.height,
            n_frames: args.n_frames,
            noise_amplitude: args.noise_amplitude,
            signal_strength: args.signal_strength,
            payload_delta: args.payload_delta,
        };

        Ok(Self {
            config,
            frames,
            accumulator: Grid::new(args.width, args.height),
            frame_index: 0,
            loop_count: 0,
            last_tick: Instant::now(),
            tick_interval: Duration::from_secs_f32(1.0 / args.fps),
            is_playing: true,
            stats: StatsSnapshot {
                display_frame: 0,
                loop_count: 0,
                accum_min: 0.0,
                accum_max: 0.0,
                mean_abs: 0.0,
                decoded_message: None,
                payload_ok: None,
            },
            static_texture: None,
            accumulation_texture: None,
        })
    }

    fn advance(&mut self) {
        if self.frame_index == self.frames.len() {
            self.frame_index = 0;
            self.loop_count += 1;
            self.accumulator = Grid::new(self.config.width, self.config.height);
        }

        let frame = &self.frames[self.frame_index];
        for (sum, &cell) in self
            .accumulator
            .data_mut()
            .iter_mut()
            .zip(frame.data().iter())
        {
            *sum += cell;
        }

        self.frame_index += 1;
        self.stats = compute_stats(
            &self.config,
            &self.accumulator,
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
        let accumulation_image = render_accumulation_image(&self.accumulator);

        if let Some(texture) = &mut self.static_texture {
            texture.set(static_image, TextureOptions::NEAREST);
        } else {
            self.static_texture =
                Some(ctx.load_texture("static-frame", static_image, TextureOptions::NEAREST));
        }

        if let Some(texture) = &mut self.accumulation_texture {
            texture.set(accumulation_image, TextureOptions::NEAREST);
        } else {
            self.accumulation_texture = Some(ctx.load_texture(
                "accumulation-frame",
                accumulation_image,
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

                if let Some(texture) = &self.accumulation_texture {
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
        ui.label(RichText::new("Accumulation Stats").strong());
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
                "accum",
                &format!("{:.2} .. {:.2}", stats.accum_min, stats.accum_max),
            );
            stat_row(ui, "mean|x|", &format!("{:.2}", stats.mean_abs));
            stat_row(
                ui,
                "decode",
                stats.decoded_message.as_deref().unwrap_or("none"),
            );
            stat_row(ui, "qr-key", &config.qr_key);
            stat_row(
                ui,
                "payload",
                match stats.payload_ok {
                    Some(true) => "match",
                    Some(false) => "mismatch",
                    None => "pending",
                },
            );
            stat_row(ui, "seed", &config.seed);
            stat_row(
                ui,
                "analog",
                &format!(
                    "{:.2} / {:.2} / {:.2}",
                    config.noise_amplitude, config.signal_strength, config.payload_delta
                ),
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
    accumulator: &Grid<f32>,
    frames: &[Grid<f32>],
    display_frame: usize,
    loop_count: usize,
) -> StatsSnapshot {
    let accum_min = accumulator
        .data()
        .iter()
        .copied()
        .min_by(f32::total_cmp)
        .unwrap_or(0.0);
    let accum_max = accumulator
        .data()
        .iter()
        .copied()
        .max_by(f32::total_cmp)
        .unwrap_or(0.0);
    let mean_abs = if accumulator.is_empty() {
        0.0
    } else {
        accumulator.data().iter().map(|value| value.abs()).sum::<f32>() / accumulator.len() as f32
    };

    let decoder = AnalogDecoder::new(
        config.payload_text.len(),
        config.noise_amplitude,
        config.signal_strength,
    )
    .ok();
    let decoded = decoder.and_then(|decoder| decoder.decode_message(frames).ok());
    let decoded_message = decoded.as_ref().and_then(|result| result.message.clone());
    let payload_ok = decoded
        .as_ref()
        .and_then(|result| result.payload.as_ref())
        .map(|payload| payload == config.payload_text.as_bytes());

    StatsSnapshot {
        display_frame,
        loop_count,
        accum_min,
        accum_max,
        mean_abs,
        decoded_message,
        payload_ok,
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

fn render_accumulation_image(accumulator: &Grid<f32>) -> ColorImage {
    let dynamic_range = accumulator
        .data()
        .iter()
        .map(|value| value.abs())
        .fold(0.0, f32::max)
        .max(1e-6);

    let pixels: Vec<Color32> = accumulator
        .data()
        .iter()
        .map(|&value| Color32::from_gray(map_symmetric_to_u8(value, dynamic_range)))
        .collect();
    ColorImage {
        size: [accumulator.width(), accumulator.height()],
        pixels,
    }
}

fn map_symmetric_to_u8(value: f32, amplitude: f32) -> u8 {
    let normalized = if amplitude > 0.0 {
        (value / amplitude).clamp(-1.0, 1.0)
    } else {
        0.0
    };
    (128.0 + normalized * 127.0).round().clamp(0.0, 255.0) as u8
}
