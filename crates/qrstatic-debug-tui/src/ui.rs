use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::state::{AppState, map_symmetric_to_u8};
use crate::theme;

pub fn render(frame: &mut Frame, state: &AppState) {
    let area = frame.area();

    let vertical = Layout::vertical([
        Constraint::Length(1), // header bar
        Constraint::Min(12),   // content area
        Constraint::Length(1), // status bar
    ])
    .split(area);

    render_header_bar(frame, vertical[0], state);
    render_content(frame, vertical[1], state);
    render_status_bar(frame, vertical[2], state);
}

fn render_header_bar(frame: &mut Frame, area: Rect, state: &AppState) {
    let width = area.width as usize;

    let title = " qrstatic debug ";
    let play_state = if state.is_playing {
        "playing"
    } else {
        "paused"
    };

    let shortcuts = [("Space", "Play/Pause"), ("N", "Step"), ("Q", "Quit")];

    let mut right_spans: Vec<Span> = Vec::new();
    for (key, label) in &shortcuts {
        right_spans.push(Span::styled(
            format!("[{key}]"),
            Style::default()
                .fg(theme::CYAN)
                .bg(theme::HEADER_BG)
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled(
            format!(" {label} "),
            Style::default().fg(theme::MUTED).bg(theme::HEADER_BG),
        ));
    }

    let right_len: usize = right_spans.iter().map(|s| s.width()).sum();
    let left = format!("{title}· {play_state}");
    let gap = width.saturating_sub(left.len() + right_len);

    let mut spans = vec![
        Span::styled(
            left,
            Style::default()
                .fg(ratatui::style::Color::White)
                .bg(theme::HEADER_BG)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(gap), Style::default().bg(theme::HEADER_BG)),
    ];
    spans.extend(right_spans);

    frame.render_widget(Line::from(spans), area);
}

fn render_content(frame: &mut Frame, area: Rect, state: &AppState) {
    // Grid is 41 rows tall; half-blocks pack 2 rows per terminal line = 21 lines
    let grid_height = ((state.config.height + 1) / 2) as u16;

    let vertical = Layout::vertical([
        Constraint::Length(1),           // section titles row
        Constraint::Length(grid_height), // grid content (fixed)
        Constraint::Length(1),           // L1 title
        Constraint::Length(1),           // L1 content
        Constraint::Length(1),           // L2 title
        Constraint::Min(3),              // L2 hex dump (fills remaining)
    ])
    .split(area);

    // Title row for the three top panels
    let title_cols = Layout::horizontal([
        Constraint::Percentage(38),
        Constraint::Percentage(38),
        Constraint::Min(24),
    ])
    .split(vertical[0]);

    frame.render_widget(
        Line::from(Span::styled(
            " Raw Stream",
            Style::default().fg(theme::LABEL),
        )),
        title_cols[0],
    );
    frame.render_widget(
        Line::from(Span::styled(
            " Correlation",
            Style::default().fg(theme::LABEL),
        )),
        title_cols[1],
    );
    frame.render_widget(
        Line::from(Span::styled(
            " QR Decode",
            Style::default().fg(theme::LABEL),
        )),
        title_cols[2],
    );

    // Grid content row
    let top = Layout::horizontal([
        Constraint::Percentage(38),
        Constraint::Percentage(38),
        Constraint::Min(24),
    ])
    .split(vertical[1]);

    render_raw_frame(frame, top[0], state);
    render_correlation_frame(frame, top[1], state);
    render_qr_decode(frame, top[2], state);

    // L1 track
    frame.render_widget(
        Line::from(Span::styled(
            " L1 Decode",
            Style::default().fg(theme::LABEL),
        )),
        vertical[2],
    );
    render_layer1_track(frame, vertical[3], state);

    // L2 hex dump
    frame.render_widget(
        Line::from(Span::styled(" L2 Data", Style::default().fg(theme::LABEL))),
        vertical[4],
    );
    render_hex_dump(frame, vertical[5], state);
}

fn render_raw_frame(frame: &mut Frame, area: Rect, state: &AppState) {
    let grid = state.current_frame();
    let amplitude = state.config.noise_amplitude;
    render_grid_halfblocks(frame, area, grid, amplitude);
}

fn render_correlation_frame(frame: &mut Frame, area: Rect, state: &AppState) {
    let grid = &state.correlation_field;
    let amplitude = state.config.l1_amplitude * state.config.n_frames as f32;
    render_grid_halfblocks(frame, area, grid, amplitude);
}

fn render_grid_halfblocks(
    frame: &mut Frame,
    area: Rect,
    grid: &qrstatic::Grid<f32>,
    amplitude: f32,
) {
    let cols = grid.width();
    let rows = grid.height();
    let term_rows = area.height as usize;
    let term_cols = area.width as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(term_rows);
    for term_row in 0..term_rows {
        let grid_row_top = term_row * 2;
        let grid_row_bot = grid_row_top + 1;

        let mut spans: Vec<Span> = Vec::with_capacity(term_cols.min(cols));
        for col in 0..term_cols.min(cols) {
            let top_val = if grid_row_top < rows {
                map_symmetric_to_u8(grid.data()[grid_row_top * cols + col], amplitude)
            } else {
                0
            };
            let bot_val = if grid_row_bot < rows {
                map_symmetric_to_u8(grid.data()[grid_row_bot * cols + col], amplitude)
            } else {
                0
            };

            let fg = ratatui::style::Color::Rgb(top_val, top_val, top_val);
            let bg = ratatui::style::Color::Rgb(bot_val, bot_val, bot_val);
            spans.push(Span::styled("▀", Style::default().fg(fg).bg(bg)));
        }
        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn render_qr_decode(frame: &mut Frame, area: Rect, state: &AppState) {
    let available_rows = area.height as usize;
    let mut lines: Vec<Line> = Vec::new();

    match &state.last_qr_decode {
        Some(qr) => {
            lines.push(Line::from(vec![
                Span::styled(" window   ", Style::default().fg(theme::LABEL)),
                Span::styled(
                    format!("W{:02}", qr.window_number),
                    Style::default().fg(theme::GREEN),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled(" score    ", Style::default().fg(theme::LABEL)),
                Span::styled(
                    format!("{:.2}", qr.detector_score),
                    Style::default().fg(theme::GREEN),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled(" key      ", Style::default().fg(theme::LABEL)),
                Span::raw(&qr.key),
            ]));
            lines.push(Line::from(vec![
                Span::styled(" message  ", Style::default().fg(theme::LABEL)),
                Span::styled(
                    &qr.message,
                    Style::default()
                        .fg(theme::GREEN)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }
        None => {
            lines.push(Line::from(Span::styled(
                " waiting for decode...",
                Style::default().fg(theme::MUTED),
            )));
        }
    }

    let paragraph = Paragraph::new(lines.into_iter().take(available_rows).collect::<Vec<_>>());
    frame.render_widget(paragraph, area);
}

fn render_layer1_track(frame: &mut Frame, area: Rect, state: &AppState) {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];

    for win in &state.layer1_windows {
        let decoded = win.decoded_message.is_some();
        let style = if decoded {
            Style::default().fg(theme::GREEN)
        } else {
            Style::default().fg(theme::RED)
        };
        let icon = if decoded { "✓" } else { "✗" };
        spans.push(Span::styled(
            format!("W{:02}{} ", win.window_number, icon),
            style,
        ));
    }

    let wn = state.current_window_number();
    let fi = state.frame_index;
    let nf = state.config.n_frames;
    let filled = (fi * 5) / nf;
    let bar: String = (0..5).map(|i| if i < filled { '█' } else { '░' }).collect();
    spans.push(Span::styled(
        format!("W{:02} ", wn),
        Style::default().fg(theme::CYAN),
    ));
    spans.push(Span::styled(bar, Style::default().fg(theme::CYAN)));
    spans.push(Span::styled(
        format!(" {}/{}", fi, nf),
        Style::default().fg(theme::MUTED),
    ));

    let paragraph = Paragraph::new(vec![Line::from(spans)]);
    frame.render_widget(paragraph, area);
}

fn render_hex_dump(frame: &mut Frame, area: Rect, state: &AppState) {
    let available_rows = area.height as usize;
    let available_cols = area.width as usize;
    let bytes = &state.decoded_bytes;

    if bytes.is_empty() {
        let paragraph = Paragraph::new(vec![Line::from(Span::styled(
            " (no decoded bytes)",
            Style::default().fg(theme::MUTED),
        ))]);
        frame.render_widget(paragraph, area);
        return;
    }

    // Adapt bytes per row to terminal width, like xxd does.
    // Layout: " XXXXXXXX: " (11) + hex region + " " (1) + ascii region
    // Hex region per byte: 2 hex chars + 0.5 space (pair grouping) = 2.5 chars avg
    // ASCII region per byte: 1 char
    // So per byte we need ~3.5 cols. Solve: bytes_per_row = (available_cols - 12) / 3.5
    // Round down to nearest multiple of 2 for pair grouping.
    let bytes_per_row = {
        let usable = available_cols.saturating_sub(12);
        // Each byte costs: 2 hex chars + 1 ascii char = 3, plus 1 space per 2 bytes = 0.5
        // Total per byte = 3.5, so multiply usable by 2/7
        let raw = (usable * 2) / 7;
        let aligned = (raw / 2) * 2; // round down to even
        aligned.max(2) // minimum 2 bytes per row
    };

    let total_rows = (bytes.len() + bytes_per_row - 1) / bytes_per_row;

    // Show the last N rows that fit in the available space
    let start_row = total_rows.saturating_sub(available_rows);

    let mut lines: Vec<Line> = Vec::with_capacity(available_rows);
    for row_idx in start_row..total_rows {
        let offset = row_idx * bytes_per_row;
        let row_bytes = &bytes[offset..bytes.len().min(offset + bytes_per_row)];

        let mut spans: Vec<Span> = Vec::new();

        // Offset column
        spans.push(Span::styled(
            format!(" {:08x}: ", offset),
            Style::default().fg(theme::MUTED),
        ));

        // Hex columns (grouped in pairs)
        let mut hex = String::with_capacity(bytes_per_row * 3);
        for (i, &b) in row_bytes.iter().enumerate() {
            hex.push_str(&format!("{:02x}", b));
            if i % 2 == 1 {
                hex.push(' ');
            }
        }
        // Pad remaining space if row is short
        let expected_hex_len = bytes_per_row * 2 + bytes_per_row / 2;
        while hex.len() < expected_hex_len {
            hex.push(' ');
        }
        spans.push(Span::styled(hex, Style::default().fg(theme::CYAN)));

        // ASCII column
        spans.push(Span::styled(" ", Style::default()));
        let ascii: String = row_bytes
            .iter()
            .map(|&b| {
                if (0x20..=0x7e).contains(&b) {
                    b as char
                } else {
                    '.'
                }
            })
            .collect();
        spans.push(Span::styled(ascii, Style::default().fg(theme::GREEN)));

        lines.push(Line::from(spans));
    }

    let paragraph = Paragraph::new(lines);
    frame.render_widget(paragraph, area);
}

fn render_status_bar(frame: &mut Frame, area: Rect, state: &AppState) {
    let width = area.width as usize;
    let s = &state.stats;
    let c = &state.config;
    let play_indicator = if state.is_playing { "▶" } else { "⏸" };

    let style = Style::default()
        .fg(ratatui::style::Color::White)
        .bg(theme::STATUS_BG)
        .add_modifier(Modifier::BOLD);
    let dim = Style::default().fg(theme::MUTED).bg(theme::STATUS_BG);
    let sep = Span::styled(" · ", dim);

    let left_spans = vec![
        Span::styled(format!(" {play_indicator} "), style),
        Span::styled(format!("frame {}/{}", s.display_frame, c.n_frames), style),
        sep.clone(),
        Span::styled(format!("stream {}", s.stream_position), style),
        sep.clone(),
        Span::styled(format!("window {}", s.window_number), style),
        sep.clone(),
        Span::styled(
            format!("detector {:.2}", s.detector_score.unwrap_or(0.0)),
            if s.detector_score.unwrap_or(0.0) >= c.min_detector_score {
                Style::default()
                    .fg(theme::GREEN)
                    .bg(theme::STATUS_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                style
            },
        ),
        sep.clone(),
        Span::styled(format!("{} bytes", state.decoded_bytes.len()), style),
    ];

    let right_spans = vec![Span::styled(
        format!("noise {:.2}  l1 {:.2} ", c.noise_amplitude, c.l1_amplitude),
        dim,
    )];

    let left_len: usize = left_spans.iter().map(|s| s.width()).sum();
    let right_len: usize = right_spans.iter().map(|s| s.width()).sum();
    let gap = width.saturating_sub(left_len + right_len);

    let mut spans = left_spans;
    spans.push(Span::styled(
        " ".repeat(gap),
        Style::default().bg(theme::STATUS_BG),
    ));
    spans.extend(right_spans);

    frame.render_widget(Line::from(spans), area);
}
