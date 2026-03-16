use std::env;
use std::time::{Duration, Instant};

use eframe::egui::{
    self, Align, Color32, ColorImage, Context, Frame as UiFrame, Grid as UiGrid, Margin,
    RichText, ScrollArea, Sense, Stroke, TextureHandle, TextureOptions, Vec2,
};
use qrstatic::codec::temporal::{
    TemporalConfig, TemporalDecodePolicy, TemporalDecoder, TemporalEncoder, naive_field,
    try_extract_qr,
};
use qrstatic::{Grid, qr};

const DEFAULT_WIDTH: usize = 41;
const DEFAULT_HEIGHT: usize = 41;
const DEFAULT_FRAMES: usize = 64;
const DEFAULT_NOISE_AMPLITUDE: f32 = 0.42;
const DEFAULT_L1_AMPLITUDE: f32 = 0.22;
const DEFAULT_FPS: f32 = 12.0;
const DEFAULT_MASTER_KEY: &str = "qrstatic-debug";
const DEFAULT_QR_PAYLOAD: &str = "temporal-bootstrap";
const DEFAULT_STREAM_WINDOWS: usize = 24;
const MAX_WINDOW_HISTORY: usize = 12;
const MAX_LAYER2_SAMPLES: usize = 512;

fn main() -> eframe::Result<()> {
    let args = Args::parse(env::args().skip(1)).map_err(eframe_error)?;
    let app = DebugViewerApp::new(args).map_err(eframe_error)?;
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 640.0])
            .with_min_inner_size([780.0, 520.0])
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
    stream_windows: usize,
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
            stream_windows: DEFAULT_STREAM_WINDOWS,
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
                "--stream-windows" => {
                    parsed.stream_windows =
                        parse_usize(&next_value(&mut args, "--stream-windows")?, "--stream-windows")?
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
        if parsed.stream_windows == 0 {
            return Err("--stream-windows must be greater than zero".into());
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
        "    --stream-windows <count>",
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
    stream_windows: Vec<StreamWindow>,
    naive_accumulator: Grid<f32>,
    correlation_field: Grid<f32>,
    frame_index: usize,
    current_window_index: usize,
    stream_passes: usize,
    last_tick: Instant,
    tick_interval: Duration,
    is_playing: bool,
    stats: StatsSnapshot,
    raw_texture: Option<TextureHandle>,
    active_window_texture: Option<TextureHandle>,
    layer1_windows: Vec<Layer1WindowThumb>,
    layer2_samples: Vec<f32>,
}

struct StreamWindow {
    key: String,
    qr_payload: String,
    frames: Vec<Grid<f32>>,
}

struct Layer1WindowThumb {
    window_number: usize,
    image: ColorImage,
    key: String,
    decoded_message: Option<String>,
    texture: Option<TextureHandle>,
}

#[derive(Debug, Clone)]
struct ViewerConfig {
    width: usize,
    height: usize,
    n_frames: usize,
    noise_amplitude: f32,
    l1_amplitude: f32,
    min_detector_score: f32,
}

#[derive(Debug, Clone)]
struct StatsSnapshot {
    display_frame: usize,
    window_number: usize,
    stream_position: usize,
    naive_min: f32,
    naive_max: f32,
    naive_mean_abs: f32,
    corr_min: f32,
    corr_max: f32,
    corr_mean_abs: f32,
    corr_mean_signed: f32,
    current_key: String,
    current_qr_payload: String,
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
            raw_texture: None,
            active_window_texture: None,
            layer1_windows: Vec::new(),
            layer2_samples: Vec::with_capacity(MAX_LAYER2_SAMPLES),
        })
    }

    fn advance(&mut self) {
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
        self.correlation_field =
            compute_correlation_prefix(self.current_window(), &self.config, &self.current_frames()[..self.frame_index])
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

    fn current_frame(&self) -> &Grid<f32> {
        let index = self.frame_index.saturating_sub(1).min(self.current_frames().len() - 1);
        &self.current_frames()[index]
    }

    fn current_window(&self) -> &StreamWindow {
        &self.stream_windows[self.current_window_index]
    }

    fn current_frames(&self) -> &[Grid<f32>] {
        &self.current_window().frames
    }

    fn current_window_number(&self) -> usize {
        self.stream_passes * self.stream_windows.len() + self.current_window_index + 1
    }

    fn update_textures(&mut self, ctx: &Context) {
        let raw_image = render_static_image(self.current_frame(), self.config.noise_amplitude);
        let active_window_image = render_field_image(
            &self.correlation_field,
            self.config.l1_amplitude * self.config.n_frames as f32,
        );

        if let Some(texture) = &mut self.raw_texture {
            texture.set(raw_image, TextureOptions::NEAREST);
        } else {
            self.raw_texture =
                Some(ctx.load_texture("raw-frame", raw_image, TextureOptions::NEAREST));
        }

        if let Some(texture) = &mut self.active_window_texture {
            texture.set(active_window_image, TextureOptions::NEAREST);
        } else {
            self.active_window_texture = Some(ctx.load_texture(
                "active-layer1-window",
                active_window_image,
                TextureOptions::NEAREST,
            ));
        }

        for thumb in &mut self.layer1_windows {
            if thumb.texture.is_none() {
                thumb.texture = Some(ctx.load_texture(
                    format!("layer1-window-{}", thumb.window_number),
                    thumb.image.clone(),
                    TextureOptions::NEAREST,
                ));
            }
        }
    }

    fn finish_window(&mut self) {
        if self.frame_index == 0 {
            return;
        }

        let image = render_field_image(
            &self.correlation_field,
            self.config.l1_amplitude * self.config.n_frames as f32,
        );
        let window_number = self.current_window_number();
        self.layer1_windows.push(Layer1WindowThumb {
            window_number,
            image,
            key: self.current_window().key.clone(),
            decoded_message: self.stats.decoded_message.clone(),
            texture: None,
        });
        if self.layer1_windows.len() > MAX_WINDOW_HISTORY {
            let overflow = self.layer1_windows.len() - MAX_WINDOW_HISTORY;
            self.layer1_windows.drain(..overflow);
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
            if now.duration_since(self.last_tick) >= self.tick_interval {
                self.last_tick += self.tick_interval;
                self.advance();
            }
        } else {
            self.last_tick = Instant::now();
        }

        self.update_textures(ctx);
        ctx.request_repaint_after(self.tick_interval);

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical(|ui| {
                let available = ui.available_size();
                let stats_width = 260.0;
                let top_height = (available.y * 0.50).max(240.0);

                ui.horizontal_top(|ui| {
                    let video_size = Vec2::new((available.x - stats_width - 12.0).max(320.0), top_height);
                    draw_video_panel(
                        ui,
                        self.raw_texture.as_ref(),
                        video_size,
                        &self.stats,
                        self.config.n_frames,
                    );
                    ui.add_space(8.0);
                    ui.vertical(|ui| {
                        ui.set_width(stats_width);
                        draw_stats(
                            ui,
                            &self.config,
                            &self.stats,
                            &mut self.is_playing,
                            &mut self.last_tick,
                        );
                    });
                });

                ui.add_space(8.0);
                ScrollArea::vertical().id_salt("bottom-tracks").show(ui, |ui| {
                    draw_layer1_track(
                        ui,
                        &self.layer1_windows,
                        self.active_window_texture.as_ref(),
                        self.current_window_number(),
                        self.frame_index,
                        self.config.n_frames,
                        self.stats.decoded_message.as_deref(),
                    );
                    ui.add_space(8.0);
                    draw_layer2_track(
                        ui,
                        &self.layer2_samples,
                        self.config.n_frames,
                    );
                });
            });
        });
    }
}

fn draw_video_panel(
    ui: &mut egui::Ui,
    texture: Option<&TextureHandle>,
    pane_size: Vec2,
    stats: &StatsSnapshot,
    total_frames: usize,
) {
    panel_frame().show(ui, |ui| {
        ui.label(RichText::new("Live Frame").strong());
        let desired = Vec2::new(pane_size.x - 16.0, pane_size.y - 40.0);
        if let Some(texture) = texture {
            ui.add(egui::Image::new(texture).fit_to_exact_size(desired));
        } else {
            ui.allocate_space(desired);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "frame {}/{}  stream {}  window {}  detector {:.2}",
                stats.display_frame,
                total_frames,
                stats.stream_position,
                stats.window_number,
                stats.detector_score.unwrap_or(0.0)
            ))
            .small()
            .color(Color32::LIGHT_GRAY),
        );
    });
}

fn draw_layer1_track(
    ui: &mut egui::Ui,
    completed_windows: &[Layer1WindowThumb],
    active_texture: Option<&TextureHandle>,
    active_window_number: usize,
    active_frame: usize,
    total_frames: usize,
    active_decode: Option<&str>,
) {
    panel_frame().show(ui, |ui| {
    ui.label(RichText::new("Layer 1 Decode Track").strong());
    ScrollArea::horizontal().id_salt("layer1-track").show(ui, |ui| {
        ui.horizontal(|ui| {
            for thumb in completed_windows {
                ui.vertical(|ui| {
                    ui.set_max_width(60.0);
                    ui.label(RichText::new(format!("W{:02}", thumb.window_number)).small());
                    if let Some(texture) = &thumb.texture {
                        ui.add(
                            egui::Image::new(texture)
                                .fit_to_exact_size(Vec2::splat(56.0))
                                .sense(Sense::hover()),
                        );
                    }
                    let msg = thumb.decoded_message.as_deref().unwrap_or(&thumb.key);
                    let label = truncate_label(msg, 12);
                    ui.label(
                        RichText::new(label)
                            .small()
                            .color(Color32::LIGHT_GRAY),
                    );
                });
                ui.add_space(4.0);
            }

            ui.vertical(|ui| {
                ui.set_max_width(60.0);
                ui.label(
                    RichText::new(format!("W{:02} {:>2}/{}", active_window_number, active_frame, total_frames))
                        .small(),
                );
                if let Some(texture) = active_texture {
                    ui.add(
                        egui::Image::new(texture)
                            .fit_to_exact_size(Vec2::splat(56.0))
                            .sense(Sense::hover()),
                    );
                }
                let label = truncate_label(active_decode.unwrap_or("active"), 12);
                ui.label(
                    RichText::new(label)
                        .small()
                        .color(Color32::from_rgb(150, 210, 255)),
                );
            });
        });
    });
    });
}

fn draw_layer2_track(
    ui: &mut egui::Ui,
    samples: &[f32],
    n_frames: usize,
) {
    panel_frame().show(ui, |ui| {
        ui.label(RichText::new("Layer 2 Data Track").strong());
        let desired_size = Vec2::new(ui.available_width(), 100.0);
        let (rect, _response) = ui.allocate_exact_size(desired_size, Sense::hover());

        if !samples.is_empty() {
            let painter = ui.painter_at(rect);
            let max_mag = samples
                .iter()
                .map(|value| value.abs())
                .fold(1e-6, f32::max);
            let mid_y = rect.center().y;
            let to_screen = |index: usize, value: f32| {
                let x = if n_frames <= 1 {
                    rect.left()
                } else {
                    rect.left()
                        + rect.width() * index as f32
                            / (samples.len().saturating_sub(1).max(1)) as f32
                };
                let y = mid_y - (rect.height() * 0.45) * (value / max_mag).clamp(-1.0, 1.0);
                egui::pos2(x, y)
            };

            painter.line_segment(
                [egui::pos2(rect.left(), mid_y), egui::pos2(rect.right(), mid_y)],
                Stroke::new(1.0, Color32::from_gray(50)),
            );

            let bar_width = (rect.width() / samples.len().max(1) as f32).max(2.0);
            for (index, value) in samples.iter().enumerate() {
                let x = rect.left() + rect.width() * index as f32 / samples.len().max(1) as f32;
                let top = to_screen(index, *value).y;
                let bar_rect = egui::Rect::from_min_max(
                    egui::pos2(x, top.min(mid_y)),
                    egui::pos2((x + bar_width).min(rect.right()), top.max(mid_y)),
                );
                let color = if *value >= 0.0 {
                    Color32::from_rgb(120, 200, 255)
                } else {
                    Color32::from_rgb(255, 170, 120)
                };
                painter.rect_filled(bar_rect, 0.0, color);
            }
        }

        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "signed accumulation proxy  current {:.3}  samples {}",
                samples.last().copied().unwrap_or(0.0),
                samples.len()
            ))
            .small()
            .color(Color32::LIGHT_GRAY),
        );
    });
}

fn draw_stats(
    ui: &mut egui::Ui,
    config: &ViewerConfig,
    stats: &StatsSnapshot,
    is_playing: &mut bool,
    last_tick: &mut Instant,
) {
    panel_frame().show(ui, |ui| {
    ui.horizontal(|ui| {
        ui.label(RichText::new("Temporal Stats").strong());
        let label = if *is_playing { "Pause" } else { "Play" };
        if ui.button(label).clicked() {
            *is_playing = !*is_playing;
            *last_tick = Instant::now();
        }
    });

    UiGrid::new("stats-grid")
        .num_columns(2)
        .spacing([8.0, 1.0])
        .show(ui, |ui| {
            stat_row(ui, "state", if *is_playing { "playing" } else { "paused" });
            stat_row(ui, "frame", &format!("{}/{}", stats.display_frame, config.n_frames));
            stat_row(ui, "window", &stats.window_number.to_string());
            stat_row(ui, "stream", &stats.stream_position.to_string());
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
            stat_row(ui, "key", &stats.current_key);
            stat_row(ui, "qr", &stats.current_qr_payload);
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
                    .unwrap_or_else(|| "0.00".to_string()),
            );
            stat_row(ui, "threshold", &format!("{:.2}", config.min_detector_score));
            stat_row(
                ui,
                "temporal",
                &format!("{:.2} / {:.2}", config.noise_amplitude, config.l1_amplitude),
            );
        });
    });
}

fn stat_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.with_layout(egui::Layout::left_to_right(Align::Min), |ui| {
        ui.label(RichText::new(label).small().color(Color32::LIGHT_GRAY));
    });
    ui.label(RichText::new(value).small().monospace());
    ui.end_row();
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
    let decoder = temporal_config
        .and_then(|cfg| TemporalDecoder::new(cfg).ok());
    let decoded = decoder
        .as_ref()
        .and_then(|decoder| {
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
    let decoder =
        TemporalDecoder::new(temporal_config).map_err(|err| format!("failed to construct temporal decoder: {err}"))?;
    decoder
        .correlate_prefix(frames, &window.key)
        .map(|correlation| correlation.field)
        .map_err(|err| format!("failed to correlate temporal prefix: {err}"))
}

fn build_stream_windows(encoder: &TemporalEncoder, args: &Args) -> Result<Vec<StreamWindow>, String> {
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

fn panel_frame() -> UiFrame {
    UiFrame::NONE
        .fill(Color32::from_gray(30))
        .stroke(Stroke::new(1.0, Color32::from_gray(50)))
        .corner_radius(6.0)
        .inner_margin(Margin::same(8))
}

fn truncate_label(text: &str, max_chars: usize) -> String {
    if text.len() <= max_chars {
        text.to_string()
    } else {
        format!("{}...", &text[..max_chars])
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
