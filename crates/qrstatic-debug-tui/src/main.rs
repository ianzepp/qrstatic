use std::env;
use std::io;
use std::time::Instant;

use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use ratatui::DefaultTerminal;

mod state;
mod theme;
mod ui;

fn main() -> io::Result<()> {
    let args = state::Args::parse(env::args().skip(1))
        .map_err(|msg| io::Error::new(io::ErrorKind::InvalidInput, msg))?;

    let mut app =
        state::AppState::new(args).map_err(|msg| io::Error::new(io::ErrorKind::Other, msg))?;

    // Bootstrap the first frame
    app.advance();

    let mut terminal = ratatui::init();
    let result = run_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run_loop(terminal: &mut DefaultTerminal, state: &mut state::AppState) -> io::Result<()> {
    loop {
        terminal.draw(|frame| ui::render(frame, state))?;

        // Poll with a short timeout matching the tick interval
        let poll_duration = if state.is_playing {
            state.tick_interval
        } else {
            std::time::Duration::from_millis(100)
        };

        if event::poll(poll_duration)? {
            if let Event::Key(key) = event::read()? {
                match (key.code, key.modifiers) {
                    (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        return Ok(());
                    }
                    (KeyCode::Char(' '), _) => {
                        state.is_playing = !state.is_playing;
                        state.last_tick = Instant::now();
                    }
                    (KeyCode::Char('n'), _) | (KeyCode::Right, _) => {
                        if !state.is_playing {
                            state.advance();
                        }
                    }
                    _ => {}
                }
            }
        }

        // Advance if playing and tick elapsed
        if state.is_playing {
            let now = Instant::now();
            if now.duration_since(state.last_tick) >= state.tick_interval {
                state.last_tick += state.tick_interval;
                state.advance();
            }
        }
    }
}
