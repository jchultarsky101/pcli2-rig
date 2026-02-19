//! Configuration for PCLI2-RIG

use serde::{Deserialize, Serialize};

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
}
