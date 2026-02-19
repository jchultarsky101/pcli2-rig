//! TUI handling with ratatui and crossterm

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
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
                if let Ok(event) = event::read()
                    && event_tx.send(Ok(event)).is_err()
                {
                    break;
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
        execute!(io::stdout(), EnterAlternateScreen).context("Failed to enter alternate screen")?;

        // Hide cursor - mouse capture disabled by default to allow text selection
        execute!(io::stdout(), crossterm::cursor::Hide)?;

        Ok(())
    }

    /// Leave alternate screen and disable raw mode
    pub fn exit(&mut self) -> Result<()> {
        info!("Exiting TUI mode");

        // Show cursor
        execute!(io::stdout(), crossterm::cursor::Show)?;

        disable_raw_mode().context("Failed to disable raw mode")?;
        execute!(io::stdout(), LeaveAlternateScreen).context("Failed to leave alternate screen")?;

        Ok(())
    }

    /// Enable mouse capture for clicking/scrolling
    pub fn enable_mouse_capture(&self) -> Result<()> {
        execute!(io::stdout(), crossterm::event::EnableMouseCapture)?;
        Ok(())
    }

    /// Disable mouse capture to allow text selection
    pub fn disable_mouse_capture(&self) -> Result<()> {
        execute!(io::stdout(), crossterm::event::DisableMouseCapture)?;
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
    pub fn area(&self) -> ratatui::layout::Rect {
        let size = self.terminal.size().unwrap_or_default();
        ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: size.width,
            height: size.height,
        }
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
