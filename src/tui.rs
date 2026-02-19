//! TUI handling with ratatui and crossterm

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Terminal event stream
pub type EventStream = mpsc::UnboundedReceiver<Result<Event>>;

/// TUI wrapper
pub struct Tui {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    event_rx: EventStream,
}

impl Tui {
    /// Create a new TUI
    pub fn new() -> Result<Self> {
        // Create event channel
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        // Spawn event reader thread
        std::thread::spawn(move || {
            loop {
                if let Ok(event) = event::read() {
                    if event_tx.send(Ok(event)).is_err() {
                        break;
                    }
                }
            }
        });

        // Create terminal backend
        let backend = CrosstermBackend::new(io::stdout());
        let terminal = Terminal::new(backend)?;

        Ok(Self { terminal, event_rx })
    }

    /// Enter alternate screen and enable raw mode
    pub fn enter(&mut self) -> Result<()> {
        info!("Entering TUI mode");

        enable_raw_mode().context("Failed to enable raw mode")?;
        execute!(io::stdout(), EnterAlternateScreen)
            .context("Failed to enter alternate screen")?;

        // Hide cursor and enable mouse capture
        execute!(io::stdout(), crossterm::cursor::Hide)?;
        execute!(io::stdout(), crossterm::event::EnableMouseCapture)?;

        Ok(())
    }

    /// Leave alternate screen and disable raw mode
    pub fn exit(&mut self) -> Result<()> {
        info!("Exiting TUI mode");

        // Show cursor and disable mouse capture
        execute!(io::stdout(), crossterm::cursor::Show)?;
        execute!(io::stdout(), crossterm::event::DisableMouseCapture)?;

        disable_raw_mode().context("Failed to disable raw mode")?;
        execute!(io::stdout(), LeaveAlternateScreen)
            .context("Failed to leave alternate screen")?;

        Ok(())
    }

    /// Draw the next frame
    pub fn draw<F>(&mut self, render: F) -> Result<()>
    where
        F: FnOnce(&mut ratatui::Frame),
    {
        self.terminal.draw(render)?;
        Ok(())
    }

    /// Get the terminal area
    pub fn area(&mut self) -> ratatui::layout::Rect {
        self.terminal.get_frame().area()
    }

    /// Get the next event
    pub async fn next_event(&mut self) -> Result<Option<Event>> {
        match self.event_rx.recv().await {
            Some(Ok(event)) => {
                debug!("Received event: {:?}", event);
                Ok(Some(event))
            }
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}
