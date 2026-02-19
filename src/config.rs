//! Configuration for PCLI2-RIG

use serde::{Deserialize, Serialize};

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Ollama model to use
    pub model: String,

    /// Ollama server host URL
    pub host: String,

    /// YOLO mode: skip confirmation for destructive tools
    pub yolo: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            model: "qwen2.5-coder:3b".to_string(),
            host: "http://localhost:11434".to_string(),
            yolo: false,
        }
    }
}

impl Config {
    /// Create a new configuration
    #[allow(dead_code)]
    pub fn new(model: String, host: String, yolo: bool) -> Self {
        Self { model, host, yolo }
    }
}
