//! PCLI2-RIG: A beautiful TUI-based local AI agent powered by Rig and Ollama
//!
//! Features:
//! - Beautiful gradient banner with tui-banner
//! - Chat interface with ratatui
//! - Tool calling with confirmation (and --yolo mode)
//! - Ollama integration for local LLM inference

use anyhow::Result;
use clap::Parser;
use std::sync::{Arc, Mutex};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use app::{App, LOG_BUFFER};
use config::Config;
use tui::Tui;

mod agent;
mod app;
mod config;
mod error;
mod tools;
mod tui;
mod ui;

/// PCLI2-RIG: Local AI Agent with beautiful TUI
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Args {
    /// Ollama model to use (e.g., "qwen2.5-coder:3b")
    #[arg(short, long, env = "OLLAMA_MODEL", default_value = "qwen2.5-coder:3b")]
    model: String,

    /// Ollama server URL
    #[arg(short = 'H', long, env = "OLLAMA_HOST", default_value = "http://localhost:11434")]
    host: String,

    /// YOLO mode: skip confirmation for destructive tools
    #[arg(long, default_value = "false")]
    yolo: bool,

    /// Enable verbose logging
    #[arg(short, long, default_value = "false")]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize logging to file and shared buffer
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else {
        EnvFilter::new("info")
    };

    // Set up file logging with custom writer that also updates shared buffer
    if let Some(home) = dirs::home_dir() {
        let log_dir = home.join(".local").join("state").join("pcli2-rig");
        let _ = std::fs::create_dir_all(&log_dir);
        let log_file = std::fs::File::create(log_dir.join("pcli2-rig.log")).unwrap_or_else(|_| {
            std::fs::File::create(std::env::temp_dir().join("pcli2-rig.log")).unwrap()
        });

        // Create a writer that writes to both file and shared buffer
        let log_buffer = LOG_BUFFER.clone();
        let dual_writer = DualWriter::new(log_file, log_buffer);

        let file_layer = fmt::layer()
            .with_writer(dual_writer)
            .with_target(false)
            .with_thread_ids(false)
            .with_file(false)
            .with_line_number(false)
            .without_time();

        tracing_subscriber::registry()
            .with(file_layer)
            .with(filter)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .init();
    }

    tracing::info!("Starting PCLI2-RIG with model: {}", args.model);

    // Create configuration
    let config = Config {
        model: args.model.clone(),
        host: args.host.clone(),
        yolo: args.yolo,
    };

    // Create the application
    let mut app = App::new(config);

    // Create and run the TUI
    let mut tui = Tui::new()?;
    tui.enter()?;

    // Run the application
    let result = app.run(&mut tui).await;

    // Restore terminal
    tui.exit()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

/// Custom writer that writes to both a file and a shared buffer
#[derive(Clone)]
struct DualWriter {
    file: Arc<std::fs::File>,
    buffer: Arc<Mutex<Vec<String>>>,
}

impl DualWriter {
    fn new(file: std::fs::File, buffer: Arc<Mutex<Vec<String>>>) -> Self {
        Self {
            file: Arc::new(file),
            buffer,
        }
    }
}

impl std::io::Write for DualWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Write to file
        let mut file = self.file.as_ref();
        file.write(buf)?;
        
        // Write to shared buffer (for UI display)
        if let Ok(line) = std::str::from_utf8(buf) {
            let line = line.trim().to_string();
            if !line.is_empty() {
                if let Ok(mut buffer) = self.buffer.lock() {
                    buffer.push(line);
                    // Keep only last 100 lines
                    if buffer.len() > 100 {
                        buffer.remove(0);
                    }
                }
            }
        }
        
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let mut file = self.file.as_ref();
        file.flush()
    }
}

impl<'a> tracing_subscriber::fmt::writer::MakeWriter<'a> for DualWriter {
    type Writer = DualWriter;
    fn make_writer(&'a self) -> Self::Writer {
        self.clone()
    }
}
