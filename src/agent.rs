//! AI Agent module using Rig and Ollama

use anyhow::{Context, Result};
use rig::{
    client::{CompletionClient, Nothing},
    completion::Prompt,
    providers::ollama,
    tool::server::ToolServer,
};
use tracing::{debug, info};

use crate::config::{Config, McpServerConfig};

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

/// MCP Tool information
#[derive(Debug, Clone)]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub server: String,
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
        info!("Creating Ollama client with host: {}", config.host);

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
        info!("Connecting to {} MCP servers", servers.len());

        let mut tool_server = ToolServer::new();

        for server in servers {
            if !server.enabled {
                continue;
            }

            info!(
                "Connecting to MCP server: {} at {}",
                server.name, server.url
            );

            // Try to connect to the MCP server using Streamable HTTP transport
            match self.connect_mcp_server(&server.url, &server.name).await {
                Ok(running_service) => {
                    info!("Connected to MCP server '{}'", server.name);
                    
                    // Get the peer (server sink) for tool calls
                    let peer = running_service.peer().clone();
                    
                    // List available tools from this server
                    match peer.list_tools(Default::default()).await {
                        Ok(tools_response) => {
                            info!("Found {} tools from MCP server '{}'", tools_response.tools.len(), server.name);
                            
                            // Register each tool with the tool server
                            for tool in tools_response.tools {
                                info!("Registering MCP tool: {}", tool.name);
                                tool_server = tool_server.rmcp_tool(tool, peer.clone());
                            }
                            
                            self.mcp_connected.push(server.name.clone());
                        }
                        Err(e) => {
                            tracing::warn!("Failed to list tools from MCP server '{}': {}", server.name, e);
                        }
                    }
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
                self.preamble = format!(
                    r#"You are PCLI2-RIG, a helpful AI coding assistant running in a terminal TUI.

You have access to the following MCP tools: {}

When using tools:
1. Think carefully about what the user is asking
2. Use the appropriate tool(s) to help
3. Explain what you're doing and what the results mean

Be concise but helpful. Use formatting like code blocks when appropriate.
You are running on the user's local machine via Ollama."#,
                    tools_str
                );
            }
            
            self.tool_server_handle = Some(handle);
        }
    }

    /// Connect to a single MCP server using Streamable HTTP transport
    async fn connect_mcp_server(&self, url: &str, _name: &str) -> Result<rmcp::service::RunningService<rmcp::RoleClient, ()>> {
        use rmcp::{ServiceExt, transport::streamable_http_client::StreamableHttpClientTransport};
        
        // Create transport from URL
        let transport = StreamableHttpClientTransport::from_uri(url);
        
        // Serve the client
        let client = ().serve(transport)
            .await
            .context("Failed to serve MCP client")?;
        
        Ok(client)
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

    /// Get available MCP tools (placeholder - tools are managed by ToolServer)
    #[allow(dead_code)]
    pub fn mcp_tools(&self) -> Vec<McpTool> {
        // Tools are now managed dynamically by the ToolServer
        // This returns placeholder info based on connected servers
        self.mcp_connected.iter().map(|server| McpTool {
            name: format!("{}_tools", server),
            description: format!("Tools from MCP server '{}'", server),
            server: server.clone(),
        }).collect()
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
            let agent = self
                .client
                .agent(&self.model_name)
                .preamble(&self.preamble)
                .tool_server_handle(tool_handle.clone())
                .build();
            
            agent.prompt(prompt_text).await
        } else {
            let agent = self
                .client
                .agent(&self.model_name)
                .preamble(&self.preamble)
                .build();
            
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
