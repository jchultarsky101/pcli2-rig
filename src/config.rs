//! Configuration for PCLI2-RIG

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// MCP Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// Server name (e.g., "filesystem", "github")
    pub name: String,

    /// Server URL (e.g., "http://localhost:3000")
    pub url: String,

    /// Optional authentication token
    #[serde(default)]
    pub token: Option<String>,

    /// Whether the server is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Ollama model to use
    pub model: String,

    /// Ollama server host URL
    pub host: String,

    /// YOLO mode: skip confirmation for destructive tools
    #[serde(default)]
    pub yolo: bool,

    /// MCP servers configuration
    #[serde(default)]
    pub mcp_servers: Vec<McpServerConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "qwen2.5-coder:3b".to_string(),
            host: "http://localhost:11434".to_string(),
            yolo: false,
            mcp_servers: Vec::new(),
        }
    }
}

impl Config {
    /// Create a new configuration
    #[allow(dead_code)]
    pub fn new(model: String, host: String, yolo: bool) -> Self {
        Self {
            model,
            host,
            yolo,
            mcp_servers: Vec::new(),
        }
    }

    /// Get enabled MCP servers
    pub fn enabled_mcp_servers(&self) -> Vec<&McpServerConfig> {
        self.mcp_servers.iter().filter(|s| s.enabled).collect()
    }

    /// Get the config file path
    pub fn config_file_path() -> Option<PathBuf> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().map(|h| h.join(".config")).unwrap())
            .join("pcli2-rig");
        Some(config_dir.join("config.toml"))
    }

    /// Load configuration from file, or return default if not found
    pub fn load() -> Self {
        if let Some(config_path) = Self::config_file_path() {
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(&config_path) {
                    if let Ok(config) = toml::from_str::<Config>(&content) {
                        tracing::info!("Loaded config from {:?}", config_path);
                        tracing::info!("Loaded {} MCP servers from config", config.mcp_servers.len());
                        for server in &config.mcp_servers {
                            tracing::info!("  MCP server: {} -> {}", server.name, server.url);
                        }
                        return config;
                    }
                }
            }
        }
        tracing::info!("Using default configuration");
        Config::default()
    }
}
