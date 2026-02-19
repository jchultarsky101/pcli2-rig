//! AI Agent module using Rig and Ollama

use anyhow::{Context, Result};
use rig::{
    client::{CompletionClient, Nothing},
    completion::Prompt,
    providers::ollama,
    tool::server::ToolServer,
};
use serde_json::json;
use tracing::debug;

use crate::config::{Config, McpServerConfig};

/// Simple MCP client for HTTP POST-based servers like pcli2-mcp
struct SimpleMcpClient {
    client: reqwest::Client,
    url: String,
}

impl Clone for SimpleMcpClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            url: self.url.clone(),
        }
    }
}

impl SimpleMcpClient {
    fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }

    async fn initialize(&self) -> Result<()> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "pcli2-rig",
                    "version": "0.1.0"
                }
            }
        });

        let response = self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .context("Failed to send initialize request")?;

        if !response.status().is_success() {
            anyhow::bail!("Initialize failed with status: {}", response.status());
        }

        Ok(())
    }

    async fn list_tools(&self) -> Result<Vec<rmcp::model::Tool>> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list",
            "params": {}
        });

        let response = self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .context("Failed to send tools/list request")?;

        if !response.status().is_success() {
            anyhow::bail!("tools/list failed with status: {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        
        // Parse the response to extract tools
        if let Some(result_value) = result.get("result").and_then(|r| r.get("tools")) {
            let tools: Vec<rmcp::model::Tool> = serde_json::from_value(result_value.clone())
                .context("Failed to parse tools response")?;
            Ok(tools)
        } else {
            Ok(Vec::new())
        }
    }

    async fn call_tool(&self, name: &str, arguments: serde_json::Value) -> Result<String> {
        let request = json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        });

        let response = self.client
            .post(&self.url)
            .json(&request)
            .send()
            .await
            .context("Failed to send tools/call request")?;

        if !response.status().is_success() {
            anyhow::bail!("tools/call failed with status: {}", response.status());
        }

        let result: serde_json::Value = response.json().await?;
        
        // Parse the response to extract tool result
        if let Some(result_value) = result.get("result") {
            // Convert result to string representation
            Ok(serde_json::to_string_pretty(result_value)?)
        } else if let Some(error) = result.get("error") {
            anyhow::bail!("Tool call error: {}", error);
        } else {
            Ok("Tool executed successfully (no result)".to_string())
        }
    }
}

/// A Rig tool that wraps an MCP tool
#[derive(Clone)]
struct McpRigTool {
    definition: rmcp::model::Tool,
    client: SimpleMcpClient,
    server_name: String,
}

impl McpRigTool {
    fn new(definition: rmcp::model::Tool, client: SimpleMcpClient, server_name: String) -> Self {
        Self {
            definition,
            client,
            server_name,
        }
    }
}

#[derive(Debug)]
struct McpToolError(String);

impl std::fmt::Display for McpToolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for McpToolError {}

impl rig::tool::Tool for McpRigTool {
    const NAME: &'static str = "mcp_tool";
    type Error = McpToolError;
    type Args = serde_json::Value;
    type Output = String;

    async fn definition(&self, _prompt: String) -> rig::completion::ToolDefinition {
        rig::completion::ToolDefinition {
            name: self.definition.name.to_string(),
            description: self.definition.description.clone().unwrap_or_default().to_string(),
            parameters: self.definition.schema_as_json_value(),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        tracing::info!("Calling MCP tool '{}' on server '{}'", self.definition.name, self.server_name);
        self.client.call_tool(&self.definition.name, args)
            .await
            .map_err(|e| McpToolError(e.to_string()))
    }

    fn name(&self) -> String {
        self.definition.name.to_string()
    }
}

/// Represents a chat message in the conversation
#[derive(Debug, Clone, PartialEq)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    #[allow(dead_code)]
    System,
    ToolResult,
}

/// Tool call request from the model
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ToolCallRequest {
    pub tool_name: String,
    pub arguments: String,
    pub call_id: String,
}

/// The AI agent
pub struct Agent {
    client: ollama::Client,
    model_name: String,
    preamble: String,
    chat_history: Vec<ChatMessage>,
    /// Connected MCP servers
    mcp_connected: Vec<String>,
    /// Tool server handle for MCP tools
    tool_server_handle: Option<rig::tool::server::ToolServerHandle>,
}

impl Agent {
    /// Create a new agent
    pub fn new(config: &Config) -> Result<Self> {
        debug!("Creating Ollama client with host: {}", config.host);

        // Create Ollama client
        let client = ollama::Client::new(Nothing)
            .map_err(|e| anyhow::anyhow!("Failed to create Ollama client: {}", e))?;

        Ok(Self {
            client,
            model_name: config.model.clone(),
            preamble: Self::default_preamble(),
            chat_history: Vec::new(),
            mcp_connected: Vec::new(),
            tool_server_handle: None,
        })
    }

    /// Connect to MCP servers and discover tools
    pub async fn connect_mcp_servers(&mut self, servers: &[McpServerConfig]) {
        debug!("Connecting to {} MCP servers", servers.len());

        let mut tool_server = ToolServer::new();

        for server in servers {
            if !server.enabled {
                continue;
            }

            debug!(
                "Connecting to MCP server: {} at {}",
                server.name, server.url
            );

            // Try to connect to the MCP server using simple HTTP client
            match self.connect_mcp_server(&server.url, &server.name).await {
                Ok((client, tools)) => {
                    debug!("Connected to MCP server '{}': {} tools", server.name, tools.len());

                    // Create custom Rig tools for each MCP tool
                    for tool in &tools {
                        debug!("Registering MCP tool: {} - {}", tool.name, tool.description.as_ref().unwrap_or(&"".into()));
                        let mcp_tool = McpRigTool::new(
                            tool.clone(),
                            client.clone(),
                            server.name.clone(),
                        );
                        tool_server = tool_server.tool(mcp_tool);
                    }

                    self.mcp_connected.push(server.name.clone());
                }
                Err(e) => {
                    tracing::warn!("Failed to connect to MCP server '{}': {}", server.name, e);
                }
            }
        }

        // Start the tool server and get a handle
        if !self.mcp_connected.is_empty() {
            let handle = tool_server.run();

            // Update preamble to mention MCP tools
            let tool_defs = match handle.get_tool_defs(None).await {
                Ok(defs) => defs,
                Err(e) => {
                    tracing::warn!("Failed to get tool definitions: {}", e);
                    Vec::new()
                }
            };

            if !tool_defs.is_empty() {
                let tool_names: Vec<&str> = tool_defs.iter().map(|t| t.name.as_str()).collect();
                let tools_str = tool_names.join(", ");
                tracing::debug!("Registered MCP tools: {}", tools_str);
                self.preamble = format!(
                    r#"You are PCLI2-RIG, a helpful AI coding assistant running in a terminal TUI.

You have access to these MCP tools: {}

IMPORTANT: When the user asks about folders, assets, tenants, configuration, or any pcli2-related task, YOU MUST call the appropriate MCP tool directly. DO NOT just tell the user what command to run - actually execute the tool for them.

For example:
- If asked to "list folders", call the pcli2 folder list tool
- If asked to "show tenants", call the pcli2 tenant list tool
- If asked about configuration, call the appropriate pcli2 config tool

Always prefer using MCP tools over suggesting shell commands. Only suggest shell commands if no relevant MCP tool exists.

When using tools:
1. Call the appropriate tool immediately
2. Wait for the tool result
3. Present the results to the user in a clear format

Be concise but helpful. You are running on the user's local machine via Ollama."#,
                    tools_str
                );
            }

            self.tool_server_handle = Some(handle);
        }
    }

    /// Connect to a single MCP server using simple HTTP client
    async fn connect_mcp_server(&self, url: &str, _name: &str) -> Result<(SimpleMcpClient, Vec<rmcp::model::Tool>)> {
        let client = SimpleMcpClient::new(url.to_string());
        
        // Initialize the connection
        client.initialize().await?;
        
        // List available tools
        let tools = client.list_tools().await?;
        
        Ok((client, tools))
    }

    /// Default system preamble
    fn default_preamble() -> String {
        r#"You are PCLI2-RIG, a helpful AI coding assistant running in a terminal TUI.

You have access to tools that allow you to:
- Read and write files
- List directory contents  
- Run shell commands
- Search code with grep

When using tools:
1. Think carefully about what the user is asking
2. Use the appropriate tool(s) to help
3. Explain what you're doing and what the results mean

Be concise but helpful. Use formatting like code blocks when appropriate.
You are running on the user's local machine via Ollama."#
            .to_string()
    }

    /// Add a user message to the chat
    pub fn add_user_message(&mut self, content: String) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::User,
            content,
        });
    }

    /// Add an assistant message to the chat
    pub fn add_assistant_message(&mut self, content: String) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::Assistant,
            content,
        });
    }

    /// Add a tool result message
    pub fn add_tool_result(&mut self, result: String) {
        self.chat_history.push(ChatMessage {
            role: MessageRole::ToolResult,
            content: result,
        });
    }

    /// Get the chat history
    pub fn chat_history(&self) -> &[ChatMessage] {
        &self.chat_history
    }

    /// Clear the chat history
    pub fn clear_history(&mut self) {
        self.chat_history.clear();
    }

    /// Get the model name
    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    /// Get connected MCP servers
    pub fn mcp_connected(&self) -> &[String] {
        &self.mcp_connected
    }

    /// Get count of connected MCP servers
    pub fn mcp_server_count(&self) -> usize {
        self.mcp_connected.len()
    }

    /// Get tool server handle
    #[allow(dead_code)]
    pub fn tool_server_handle(&self) -> Option<&rig::tool::server::ToolServerHandle> {
        self.tool_server_handle.as_ref()
    }

    /// Get preamble
    #[allow(dead_code)]
    pub fn preamble(&self) -> &str {
        &self.preamble
    }

    /// Set tool server handle (for cloning agent state)
    pub fn set_tool_server_handle(&mut self, handle: rig::tool::server::ToolServerHandle) {
        self.tool_server_handle = Some(handle);
    }

    /// Set preamble (for cloning agent state)
    pub fn set_preamble(&mut self, preamble: String) {
        self.preamble = preamble;
    }

    /// Send a message and get a response (without adding user message to history)
    pub async fn chat_without_history(&mut self, _user_message: String) -> Result<String> {
        // Send request and get response
        let response = self.send_request().await?;

        // Add assistant response to history
        self.add_assistant_message(response.clone());

        Ok(response)
    }

    /// Send a message and get a response
    pub async fn chat(&mut self, user_message: String) -> Result<String> {
        // Add user message to history
        self.add_user_message(user_message.clone());

        // Send request and get response
        let response = self.send_request().await?;

        // Add assistant response to history
        self.add_assistant_message(response.clone());

        Ok(response)
    }

    /// Send a request to the model
    async fn send_request(&self) -> Result<String> {
        debug!("Sending request to Ollama model: {}", self.model_name);
        debug!("Chat history has {} messages", self.chat_history.len());
        debug!("Tool server handle present: {}", self.tool_server_handle.is_some());
        
        if let Some(handle) = &self.tool_server_handle {
            match handle.get_tool_defs(None).await {
                Ok(defs) => {
                    debug!("Available tools: {}", defs.len());
                    for def in &defs {
                        debug!("  Tool: {} - {}", def.name, def.description);
                    }
                }
                Err(e) => {
                    debug!("Failed to get tool defs: {}", e);
                }
            }
        }

        // Build conversation history for prompt
        let mut prompt_text = String::new();

        // Add context from chat history
        for msg in &self.chat_history {
            match msg.role {
                MessageRole::User => {
                    prompt_text.push_str(&format!("\n\nUser: {}", msg.content));
                }
                MessageRole::Assistant => {
                    prompt_text.push_str(&format!("\n\nAssistant: {}", msg.content));
                }
                MessageRole::System => {
                    prompt_text.push_str(&format!("\n\nSystem: {}", msg.content));
                }
                MessageRole::ToolResult => {
                    prompt_text.push_str(&format!("\n\nTool Result: {}", msg.content));
                }
            }
        }

        debug!("Prompt text length: {} chars", prompt_text.len());

        // Build the agent with or without tools
        let response = if let Some(tool_handle) = &self.tool_server_handle {
            debug!("Attaching tool server handle with {} MCP servers connected", self.mcp_connected.len());
            debug!("Creating agent with model: {}", self.model_name);
            let agent = self
                .client
                .agent(&self.model_name)
                .preamble(&self.preamble)
                .tool_server_handle(tool_handle.clone())
                .build();

            debug!("Sending prompt to agent with model: {}", self.model_name);
            agent.prompt(prompt_text).await
        } else {
            debug!("Creating agent (no tools) with model: {}", self.model_name);
            let agent = self
                .client
                .agent(&self.model_name)
                .preamble(&self.preamble)
                .build();

            debug!("Sending prompt to agent with model: {}", self.model_name);
            agent.prompt(prompt_text).await
        }.map_err(|e| {
            anyhow::anyhow!(
                "Ollama request failed: {}\n\n\
                     Make sure Ollama is running (`ollama serve`) and \
                     the model is pulled (`ollama pull {}`).",
                e,
                self.model_name
            )
        })?;

        debug!("Received response: {} chars", response.len());

        Ok(response)
    }
}

/// Execute a tool call
pub async fn execute_tool_call(tool_name: &str, arguments: &str) -> Result<String> {
    debug!("Executing tool: {} with args: {}", tool_name, arguments);

    let args: serde_json::Value =
        serde_json::from_str(arguments).context("Failed to parse tool arguments")?;

    let result = match tool_name {
        "read_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let contents = std::fs::read_to_string(path).context("Failed to read file")?;
            format!("Contents of {}:\n\n{}", path, contents)
        }
        "write_file" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let content = args["content"]
                .as_str()
                .context("Missing 'content' argument")?;
            std::fs::write(path, content).context("Failed to write file")?;
            format!("Successfully wrote {} bytes to {}", content.len(), path)
        }
        "list_directory" => {
            let path = args["path"].as_str().context("Missing 'path' argument")?;
            let entries = std::fs::read_dir(path).context("Failed to read directory")?;

            let mut result = String::new();
            for entry in entries {
                let entry = entry.context("Failed to read directory entry")?;
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                let is_dir = entry.path().is_dir();
                if is_dir {
                    result.push_str(&format!("📁 {}/\n", name_str));
                } else {
                    result.push_str(&format!("📄 {}\n", name_str));
                }
            }
            format!("Contents of {}:\n\n{}", path, result)
        }
        "run_command" => {
            let command = args["command"]
                .as_str()
                .context("Missing 'command' argument")?;
            let output = std::process::Command::new("bash")
                .arg("-c")
                .arg(command)
                .output()
                .context("Failed to run command")?;

            let mut result = String::new();
            result.push_str(&format!("Command: {}\n\n", command));
            if !output.stdout.is_empty() {
                result.push_str("STDOUT:\n");
                result.push_str(&String::from_utf8_lossy(&output.stdout));
                result.push('\n');
            }
            if !output.stderr.is_empty() {
                result.push_str("STDERR:\n");
                result.push_str(&String::from_utf8_lossy(&output.stderr));
                result.push('\n');
            }
            result.push_str(&format!(
                "Exit code: {}",
                output.status.code().unwrap_or(-1)
            ));
            result
        }
        "search_code" => {
            let pattern = args["pattern"]
                .as_str()
                .context("Missing 'pattern' argument")?;
            let mut cmd = std::process::Command::new("grep");
            cmd.arg("-rn").arg("--color=never").arg(pattern).arg(".");

            if let Some(glob) = args["glob"].as_str() {
                cmd.arg("--include").arg(glob);
            }

            let output = cmd.output().context("Failed to run grep")?;

            if output.stdout.is_empty() {
                "No matches found.".to_string()
            } else {
                format!(
                    "Search results for '{}':\n\n{}",
                    pattern,
                    String::from_utf8_lossy(&output.stdout)
                )
            }
        }
        _ => {
            return Err(anyhow::anyhow!("Unknown tool: {}", tool_name));
        }
    };

    Ok(result)
}
