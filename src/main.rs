//! PCLI2-RIG: A beautiful TUI-based local AI agent powered by Rig and Ollama
//!
//! Features:
//! - Beautiful gradient banner with tui-banner
//! - Chat interface with ratatui
//! - Tool calling with confirmation (and --yolo mode)
//! - Ollama integration for local LLM inference

use anyhow::Result;
use clap::Parser;
use serde_json::Value;
use std::sync::{Arc, Mutex};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use app::{App, LOG_BUFFER};
use config::{Config, McpServerConfig};
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
    #[arg(short, long, env = "OLLAMA_MODEL")]
    model: Option<String>,

    /// Ollama server URL
    #[arg(
        short = 'H',
        long,
        env = "OLLAMA_HOST",
        default_value = "http://localhost:11434"
    )]
    host: String,

    /// YOLO mode: skip confirmation for destructive tools
    #[arg(long, default_value = "false")]
    yolo: bool,

    /// Enable verbose logging
    #[arg(short, long, default_value = "false")]
    verbose: bool,

    /// Load MCP servers from pcli2-mcp config JSON (file path or "-" for stdin)
    #[arg(long, value_name = "FILE")]
    mcp_config: Option<String>,

    /// Add MCP server URL directly (can be used multiple times)
    #[arg(long, value_name = "URL")]
    mcp_remote: Vec<String>,

    /// Configure MCP servers from pcli2-mcp and save to config file (one-time setup)
    /// This will read the pcli2-mcp config and save it to ~/.config/pcli2-rig/config.toml
    #[arg(long, value_name = "FILE")]
    setup_mcp: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Handle --setup-mcp: one-time MCP configuration
    if let Some(config_path) = &args.setup_mcp {
        return setup_mcp_config(config_path);
    }

    // Initialize logging to file and shared buffer
    let filter = if args.verbose {
        EnvFilter::new("debug")
    } else {
        // Filter out noisy warnings and verbose OpenTelemetry logs
        EnvFilter::new("info,pulldown_cmark=off,tui_markdown=off,rig_core::pipeline::agent=off,rmcp=off")
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
            .without_time()
            .with_ansi(true);  // Enable ANSI colors for TUI parsing

        tracing_subscriber::registry()
            .with(file_layer)
            .with(filter)
            .init();
    } else {
        tracing_subscriber::registry().with(filter).init();
    }

    tracing::debug!("Starting PCLI2-RIG with model: {}", args.model.as_deref().unwrap_or("config default"));

    // Load configuration from file (if exists)
    let mut config = Config::load();

    // Override with CLI arguments only if explicitly provided
    if let Some(model) = args.model {
        config.model = model;
    }
    config.host = args.host.clone();
    config.yolo = args.yolo;

    tracing::info!("Using model: {}", config.model);

    // Parse MCP configuration
    let mut mcp_servers = Vec::new();

    // Load from pcli2-mcp config file/stdin
    if let Some(config_path) = &args.mcp_config {
        let json_content = if config_path == "-" {
            // Read from stdin
            use std::io::Read;
            let mut buffer = String::new();
            std::io::stdin().read_to_string(&mut buffer)?;
            buffer
        } else {
            // Read from file
            std::fs::read_to_string(config_path)?
        };

        // Parse pcli2-mcp JSON format
        if let Ok(mcp_config) = parse_mcp_config(&json_content) {
            mcp_servers.extend(mcp_config);
            tracing::debug!("Loaded {} MCP servers from config", mcp_servers.len());
        }
    }

    // Add direct MCP remote URLs
    for url in &args.mcp_remote {
        mcp_servers.push(McpServerConfig {
            name: format!("remote-{}", mcp_servers.len()),
            url: url.clone(),
            token: None,
            enabled: true,
        });
    }

    // If MCP servers were provided via CLI, use them; otherwise keep loaded config
    if !mcp_servers.is_empty() {
        config.mcp_servers = mcp_servers;
    }

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
            if !line.is_empty()
                && let Ok(mut buffer) = self.buffer.lock()
            {
                // Add emoji prefix based on log level
                let prefixed_line = if line.contains("ERROR") {
                    format!("✗ {}", line)
                } else if line.contains("WARN") {
                    format!("⚠ {}", line)
                } else if line.contains("INFO") {
                    format!("✓ {}", line)
                } else if line.contains("DEBUG") {
                    format!("• {}", line)
                } else {
                    line
                };

                buffer.push(prefixed_line);
                // Keep only last 100 lines
                if buffer.len() > 100 {
                    buffer.remove(0);
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

/// Parse pcli2-mcp JSON configuration format
/// Expected format:
/// {
///   "mcpServers": {
///     "server_name": {
///       "command": "npx",
///       "args": ["-y", "mcp-remote", "http://localhost:8080/mcp"]
///     }
///   }
/// }
fn parse_mcp_config(json: &str) -> Result<Vec<McpServerConfig>> {
    let value: Value = serde_json::from_str(json)?;
    let mut servers = Vec::new();

    if let Some(mcp_servers) = value.get("mcpServers").and_then(|v| v.as_object()) {
        for (name, config) in mcp_servers {
            // Extract URL from args (look for http:// or https:// URLs)
            let mut url = None;
            if let Some(args) = config.get("args").and_then(|a| a.as_array()) {
                for arg in args {
                    if let Some(arg_str) = arg.as_str()
                        && (arg_str.starts_with("http://") || arg_str.starts_with("https://")) {
                            url = Some(arg_str.to_string());
                            break;
                        }
                }
            }

            if let Some(server_url) = url {
                tracing::debug!("Parsed MCP server: {} -> {}", name, server_url);
                servers.push(McpServerConfig {
                    name: name.clone(),
                    url: server_url,
                    token: None,
                    enabled: true,
                });
            }
        }
    }

    Ok(servers)
}

/// Setup MCP configuration from pcli2-mcp and save to config file
/// This is a one-time setup command
fn setup_mcp_config(config_path: &str) -> Result<()> {
    use std::fs;
    use std::io::Read;

    // Read the pcli2-mcp config
    let json_content = if config_path == "-" {
        // Read from stdin
        let mut buffer = String::new();
        std::io::stdin().read_to_string(&mut buffer)?;
        buffer
    } else {
        // Read from file
        fs::read_to_string(config_path)?
    };

    // Parse MCP servers from pcli2-mcp format
    let mcp_servers = parse_mcp_config(&json_content)?;

    if mcp_servers.is_empty() {
        eprintln!("No MCP servers found in configuration");
        std::process::exit(1);
    }

    // Determine config directory and file path
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().map(|h| h.join(".config")).unwrap())
        .join("pcli2-rig");
    let config_file = config_dir.join("config.toml");

    // Create config directory if it doesn't exist
    fs::create_dir_all(&config_dir)?;

    // Load existing config or create default
    let mut config = if config_file.exists() {
        let content = fs::read_to_string(&config_file)?;
        toml::from_str::<Config>(&content).unwrap_or_default()
    } else {
        Config::default()
    };

    // Update MCP servers
    config.mcp_servers = mcp_servers.clone();

    // Save configuration
    let toml_content = toml::to_string_pretty(&config)?;
    fs::write(&config_file, &toml_content)?;

    println!("✓ MCP configuration saved to {}", config_file.display());
    println!();
    println!("Configured {} MCP server(s):", mcp_servers.len());
    for server in &mcp_servers {
        println!("  • {} → {}", server.name, server.url);
    }
    println!();
    println!("You can now run: pcli2-rig");
    println!("Or edit config at: {}", config_file.display());
    println!();

    Ok(())
}
