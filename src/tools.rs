//! Built-in tools for the AI agent
//!
//! Note: Tool argument structs are defined here for future expansion.
//! Currently tools are executed directly in the agent module.

/// Read a file's contents arguments
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ReadFileArgs {
    /// Path to the file to read
    pub path: String,
}

/// Write contents to a file arguments
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct WriteFileArgs {
    /// Path to the file to write
    pub path: String,
    /// Contents to write to the file
    pub content: String,
}

/// List directory contents arguments
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ListDirectoryArgs {
    /// Path to the directory to list
    pub path: String,
}

/// Run a shell command arguments
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct RunCommandArgs {
    /// Command to run
    pub command: String,
    /// Working directory for the command
    pub cwd: Option<String>,
}

/// Search code with grep arguments
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct SearchCodeArgs {
    /// Pattern to search for
    pub pattern: String,
    /// Directory to search in
    pub path: Option<String>,
    /// File pattern to filter (e.g., "*.rs")
    pub glob: Option<String>,
}
