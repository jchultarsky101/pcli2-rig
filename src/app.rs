//! Main application state and logic

use anyhow::Result;
use crossterm::event::{KeyEvent, KeyModifiers};
use ratatui::Frame;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::agent::{self, Agent};
use crate::config::Config;
use crate::tui::Tui;
use crate::ui;

/// Shared log buffer accessible from tracing layer
pub static LOG_BUFFER: once_cell::sync::Lazy<Arc<Mutex<Vec<String>>>> =
    once_cell::sync::Lazy::new(|| Arc::new(Mutex::new(Vec::new())));

/// Number of CPU samples to keep for sparkline
const CPU_HISTORY_SIZE: usize = 20;

/// Messages for the app loop
#[derive(Debug)]
pub enum AppMessage {
    Response(Result<String>),
}

/// Pending tool call awaiting confirmation
#[derive(Debug, Clone)]
pub struct PendingToolCall {
    pub tool_name: String,
    pub arguments: String,
    #[allow(dead_code)]
    pub call_id: String,
}

/// Application state
pub struct App {
    /// The AI agent
    agent: Agent,
    /// Configuration
    #[allow(dead_code)]
    config: Config,
    /// Current input text
    input: String,
    /// Cursor position in input
    cursor_pos: usize,
    /// Whether the app should quit
    should_quit: bool,
    /// Current status message
    status: String,
    /// Whether currently waiting for response
    is_thinking: bool,
    /// When thinking started (for animation)
    thinking_start: std::time::Instant,
    /// Pending tool call awaiting confirmation
    pending_tool_call: Option<PendingToolCall>,
    /// Log buffer for displaying in UI
    logs: Vec<String>,
    /// Max log lines to keep
    max_logs: usize,
    /// Scroll offset for chat history (0 = at bottom)
    scroll_offset: usize,
    /// Scroll offset for logs (0 = at bottom)
    log_scroll_offset: usize,
    /// Horizontal scroll offset for logs
    log_hscroll_offset: usize,
    /// Which pane has focus (0=chat, 1=input, 2=logs)
    focus_pane: usize,
    /// Queue of messages waiting to be sent
    message_queue: Vec<String>,
    /// Whether help modal is shown
    show_help: bool,
    /// Scroll offset for help modal
    help_scroll_offset: usize,
    /// Whether mouse capture is enabled (for click/scroll vs text selection)
    mouse_enabled: bool,
    /// CPU usage history for sparkline (percentage values 0-100)
    cpu_history: Vec<f32>,
    /// System monitor for CPU usage
    sys: sysinfo::System,
    /// Cancellation token for in-flight requests
    cancel_token: Option<CancellationToken>,
    /// Input history for Up/Down navigation (bash-style)
    input_history: Vec<String>,
    /// Current position in input history (0 = newest, higher = older)
    history_index: usize,
    /// Temporary storage for original input when browsing history
    history_original: String,
    /// Horizontal scroll offset for input (when text exceeds width)
    input_hscroll_offset: usize,
}

impl App {
    /// Create a new application
    pub fn new(config: Config) -> Self {
        let agent = Agent::new(&config).expect("Failed to create agent");
        let mut sys = sysinfo::System::new();
        sys.refresh_cpu_usage();

        Self {
            agent,
            config,
            input: String::new(),
            cursor_pos: 0,
            should_quit: false,
            status: "Ready".to_string(),
            is_thinking: false,
            thinking_start: std::time::Instant::now(),
            pending_tool_call: None,
            logs: Vec::new(),
            max_logs: 100,
            scroll_offset: 0,
            log_scroll_offset: 0,
            log_hscroll_offset: 0,
            focus_pane: 1, // Start with input focused
            message_queue: Vec::new(),
            show_help: false,
            help_scroll_offset: 0,
            mouse_enabled: false,
            cpu_history: Vec::new(),
            sys,
            cancel_token: None,
            input_history: Vec::new(),
            history_index: 0,
            history_original: String::new(),
            input_hscroll_offset: 0,
        }
    }

    /// Run the application main loop
    pub async fn run(&mut self, tui: &mut Tui) -> Result<()> {
        debug!("Starting application main loop");

        // Create channel for async responses
        let (tx, mut rx) = mpsc::channel::<AppMessage>(32);

        // Add welcome banner as first message in chat history
        self.add_welcome_banner();

        // Connect to MCP servers
        let mcp_servers = self.config.mcp_servers.clone();
        if !mcp_servers.is_empty() {
            self.status = "Connecting to MCP servers...".to_string();
            // Connect to MCP servers asynchronously
            self.agent.connect_mcp_servers(&mcp_servers).await;
            let connected_count = self.agent.mcp_server_count();
            self.status = format!("Ready | {} MCP server(s) connected", connected_count);
            debug!("Connected to {} MCP servers", connected_count);
        }

        // Timer for spinner animation (500ms interval)
        let mut spinner_timer = tokio::time::interval(std::time::Duration::from_millis(500));

        // Timer for syncing logs (200ms interval)
        let mut log_timer = tokio::time::interval(std::time::Duration::from_millis(200));

        // Timer for CPU sampling (1 second interval)
        let mut cpu_timer = tokio::time::interval(std::time::Duration::from_secs(1));

        // Main event loop
        loop {
            // Draw the UI
            tui.draw(|frame| self.render(frame))?;

            // Handle events and messages
            tokio::select! {
                // Handle UI events
                event_result = tui.next_event() => {
                    if let Ok(Some(event)) = event_result {
                        self.handle_event(event, &tx, tui).await?;
                    }
                }
                // Handle async responses and logs
                Some(msg) = rx.recv() => {
                    self.handle_response(msg, &tx).await?;
                }
                // Timer for spinner animation
                _ = spinner_timer.tick() => {
                    // Force redraw when thinking to animate spinner
                    if self.is_thinking {
                        continue;
                    }
                }
                // Timer for syncing logs from shared buffer
                _ = log_timer.tick() => {
                    self.sync_logs();
                }
                // Timer for CPU sampling
                _ = cpu_timer.tick() => {
                    self.sample_cpu();
                }
            }

            // Check if we should quit
            if self.should_quit {
                break;
            }
        }

        Ok(())
    }

    /// Add the welcome banner to chat history
    fn add_welcome_banner(&mut self) {
        let banner = r#"
🤖 PCLI2-RIG - Local AI Agent · Powered by Ollama

Type /help for available commands · Type /quit to exit
"#
        .to_string();

        self.agent.add_assistant_message(banner);
    }

    /// Handle an event
    async fn handle_event(
        &mut self,
        event: crossterm::event::Event,
        tx: &mpsc::Sender<AppMessage>,
        tui: &crate::tui::Tui,
    ) -> Result<()> {
        use crossterm::event::KeyCode;

        // If there's a pending tool call, handle confirmation first
        if self.pending_tool_call.is_some() {
            if let crossterm::event::Event::Key(key) = event {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                        // Confirm tool execution
                        self.execute_pending_tool().await?;
                        return Ok(());
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        // Cancel tool execution
                        self.pending_tool_call = None;
                        self.status = "Tool execution cancelled".to_string();
                        return Ok(());
                    }
                    _ => {}
                }
            }
            return Ok(());
        }

        match event {
            crossterm::event::Event::Key(key) => {
                // Toggle mouse capture with Ctrl+M
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('m') {
                    if self.mouse_enabled {
                        tui.disable_mouse_capture()?;
                        self.mouse_enabled = false;
                        self.status = "Mouse disabled (text selection enabled)".to_string();
                    } else {
                        tui.enable_mouse_capture()?;
                        self.mouse_enabled = true;
                        self.status = "Mouse enabled (click/scroll enabled)".to_string();
                    }
                    return Ok(());
                }
                self.handle_key_event(key, tx).await?;
            }
            crossterm::event::Event::Mouse(mouse) => {
                // Only handle mouse events if mouse is enabled
                if self.mouse_enabled {
                    let area = tui.area();
                    self.handle_mouse(mouse, area);
                }
            }
            crossterm::event::Event::Resize(_, _) => {
                // Terminal was resized
            }
            _ => {}
        }

        Ok(())
    }

    /// Handle a key event
    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        tx: &mpsc::Sender<AppMessage>,
    ) -> Result<()> {
        use crossterm::event::{KeyCode, KeyModifiers};

        // Handle help modal
        if self.show_help {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                    self.show_help = false;
                    return Ok(());
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(1);
                    return Ok(());
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.help_scroll_offset += 1;
                    return Ok(());
                }
                KeyCode::PageUp => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(10);
                    return Ok(());
                }
                KeyCode::PageDown => {
                    self.help_scroll_offset += 10;
                    return Ok(());
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            // Cancel in-flight request (Esc)
            KeyCode::Esc => {
                if self.is_thinking {
                    self.cancel_request();
                }
            }

            // Quit
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

            // Clear chat (Ctrl+K)
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.agent.clear_history();
                self.status = "Chat history cleared".to_string();
            }

            // Enter - send message (only when input is focused)
            KeyCode::Enter => {
                if self.focus_pane == 1 && !self.input.trim().is_empty() {
                    self.send_message(tx).await?;
                }
            }

            // Text input (only when input pane is focused)
            KeyCode::Char(c) => {
                if self.focus_pane == 1 {
                    self.input.insert(self.cursor_pos, c);
                    self.cursor_pos += 1;
                    // Auto-scroll to keep cursor visible
                    self.adjust_input_scroll();
                }
            }

            // Backspace (only when input pane is focused)
            KeyCode::Backspace => {
                if self.focus_pane == 1 && self.cursor_pos > 0 {
                    self.input.remove(self.cursor_pos - 1);
                    self.cursor_pos -= 1;
                    // Auto-scroll to keep cursor visible
                    self.adjust_input_scroll();
                }
            }

            // Delete (only when input pane is focused)
            KeyCode::Delete => {
                if self.focus_pane == 1 && self.cursor_pos < self.input.len() {
                    self.input.remove(self.cursor_pos);
                    // Auto-scroll to keep cursor visible
                    self.adjust_input_scroll();
                }
            }

            // Arrow keys for cursor navigation (only when input pane is focused)
            KeyCode::Left => {
                if self.focus_pane == 1 && self.cursor_pos > 0 {
                    self.cursor_pos -= 1;
                    // Auto-scroll to keep cursor visible
                    self.adjust_input_scroll();
                } else if self.focus_pane == 2 {
                    // Logs: horizontal scroll left
                    self.log_hscroll_offset = self.log_hscroll_offset.saturating_sub(1);
                }
            }
            KeyCode::Right => {
                if self.focus_pane == 1 && self.cursor_pos < self.input.len() {
                    self.cursor_pos += 1;
                    // Auto-scroll to keep cursor visible
                    self.adjust_input_scroll();
                } else if self.focus_pane == 2 {
                    // Logs: horizontal scroll right
                    self.log_hscroll_offset = self.log_hscroll_offset.saturating_add(1);
                }
            }
            KeyCode::Home => {
                if self.focus_pane == 1 {
                    self.cursor_pos = 0;
                    self.input_hscroll_offset = 0;
                } else if self.focus_pane == 2 {
                    // Logs: scroll to beginning of line
                    self.log_hscroll_offset = 0;
                }
            }
            KeyCode::End => {
                if self.focus_pane == 1 {
                    self.cursor_pos = self.input.len();
                    // Scroll to show end of input
                    self.input_hscroll_offset = self.input.len().saturating_sub(80);
                } else if self.focus_pane == 2 {
                    // Logs: scroll to end of line (max value)
                    self.log_hscroll_offset = usize::MAX;
                }
            }

            // Scroll through chat/logs history with arrow keys (when those panes are focused)
            KeyCode::Up => {
                if self.focus_pane == 0 {
                    // Chat: scroll up to see older messages
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                } else if self.focus_pane == 1 {
                    // Input: navigate to previous command in history
                    self.navigate_history(-1);
                } else if self.focus_pane == 2 {
                    // Logs: scroll up
                    self.log_scroll_offset = self.log_scroll_offset.saturating_add(1);
                }
            }
            KeyCode::Down => {
                if self.focus_pane == 0 {
                    // Chat: scroll down to see newer messages
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                } else if self.focus_pane == 1 {
                    // Input: navigate to next command in history
                    self.navigate_history(1);
                } else if self.focus_pane == 2 {
                    // Logs: scroll down
                    self.log_scroll_offset = self.log_scroll_offset.saturating_sub(1);
                }
            }
            KeyCode::PageUp => {
                if self.focus_pane == 0 {
                    // Chat: scroll up faster (5 lines)
                    self.scroll_offset = self.scroll_offset.saturating_add(5);
                } else if self.focus_pane == 2 {
                    // Logs: scroll up faster
                    self.log_scroll_offset = self.log_scroll_offset.saturating_add(5);
                }
            }
            KeyCode::PageDown => {
                if self.focus_pane == 0 {
                    // Chat: scroll down faster (5 lines)
                    self.scroll_offset = self.scroll_offset.saturating_sub(5);
                } else if self.focus_pane == 2 {
                    // Logs: scroll down faster
                    self.log_scroll_offset = self.log_scroll_offset.saturating_sub(5);
                }
            }

            // Focus navigation
            KeyCode::Tab => {
                // Cycle through panes: chat(0) -> input(1) -> logs(2) -> chat(0)
                self.focus_pane = (self.focus_pane + 1) % 3;
            }
            KeyCode::BackTab => {
                // Shift+Tab: cycle backwards
                self.focus_pane = (self.focus_pane + 2) % 3;
            }

            _ => {}
        }

        Ok(())
    }

    /// Send the current input as a message
    async fn send_message(&mut self, tx: &mpsc::Sender<AppMessage>) -> Result<()> {
        let input = self.input.clone();
        self.input.clear();
        self.cursor_pos = 0;

        // Check for internal commands
        if input.trim().starts_with('/') {
            self.handle_command(&input).await?;
            return Ok(());
        }

        // Save non-empty input to history
        if !input.trim().is_empty() {
            self.input_history.push(input.clone());
            self.history_index = 0; // Reset history position
        }

        // If already thinking, queue the message
        if self.is_thinking {
            self.message_queue.push(input);
            self.status = format!("Thinking... ({} queued)", self.message_queue.len());
            return Ok(());
        }

        // Add user message to history immediately
        self.agent.add_user_message(input.clone());

        // Set thinking status
        self.status = "Thinking...".to_string();
        self.is_thinking = true;
        self.thinking_start = std::time::Instant::now();

        // Create cancellation token for this request
        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        debug!("Sending message to agent: {}", input);

        // Clone the input for the spawned task
        let input_clone = input.clone();
        let tx = tx.clone();

        // Clone agent state for the spawned task
        let model_name = self.agent.model_name().to_string();
        let preamble = self.agent.preamble().to_string();
        let tool_server_handle = self.agent.tool_server_handle().cloned();
        let mut chat_history = Vec::new();
        for msg in self.agent.chat_history() {
            chat_history.push((msg.role.clone(), msg.content.clone()));
        }

        tokio::spawn(async move {
            // Create agent with correct model
            use crate::config::Config;
            let config = Config {
                model: model_name,
                host: "http://localhost:11434".to_string(),
                yolo: false,
                mcp_servers: vec![],
            };
            let mut agent = Agent::new(&config).expect("Failed to create agent");

            // Restore agent state
            agent.set_preamble(preamble);
            if let Some(handle) = tool_server_handle {
                agent.set_tool_server_handle(handle);
            }

            // Restore chat history
            for (role, content) in chat_history {
                match role {
                    crate::agent::MessageRole::User => {
                        agent.add_user_message(content);
                    }
                    crate::agent::MessageRole::Assistant => {
                        agent.add_assistant_message(content);
                    }
                    crate::agent::MessageRole::ToolResult => {
                        agent.add_tool_result(content);
                    }
                    _ => {}
                }
            }

            // Add timeout and cancellation support
            let result = tokio::select! {
                // Normal request with timeout
                result = tokio::time::timeout(
                    std::time::Duration::from_secs(600),
                    agent.chat_without_history(input_clone),
                ) => {
                    result.unwrap_or(Err(anyhow::anyhow!("Request timed out after 10 minutes")))
                }
                // Cancellation requested
                _ = cancel_token.cancelled() => {
                    Err(anyhow::anyhow!("Request cancelled by user"))
                }
            };

            if let Err(e) = tx.send(AppMessage::Response(result)).await {
                tracing::error!("Failed to send response: {}", e);
            }
        });

        Ok(())
    }

    /// Handle the response from the async task
    async fn handle_response(
        &mut self,
        msg: AppMessage,
        tx: &mpsc::Sender<AppMessage>,
    ) -> Result<()> {
        match msg {
            AppMessage::Response(Ok(response)) => {
                self.is_thinking = false;
                self.cancel_token = None;
                debug!("Received response: {} chars", response.len());
                if response.trim().is_empty() {
                    // Empty response - report as error
                    self.status = "⚠ Empty response from model".to_string();
                    self.agent.add_assistant_message("⚠ The model returned an empty response. This may indicate a problem with the model or the request.".to_string());
                    tracing::warn!("Received empty response from model");
                } else {
                    self.status = "✓ Ready".to_string();
                    self.agent.add_assistant_message(response.clone());
                    debug!("Chat history now has {} messages", self.agent.chat_history().len());
                }
                debug!("Agent response: {}", response);
            }
            AppMessage::Response(Err(e)) => {
                self.is_thinking = false;
                self.cancel_token = None;
                
                // Clean up repetitive error messages
                let error_msg = e.to_string();
                let clean_error = if error_msg.contains("Tool call error:") {
                    // Extract just the essential error
                    error_msg.split("Tool call error:").last().unwrap_or(&error_msg).trim().to_string()
                } else if error_msg.contains("ToolCallError:") {
                    // Remove repetitive ToolCallError prefixes
                    error_msg.split("ToolCallError:").last().unwrap_or(&error_msg).trim().to_string()
                } else {
                    error_msg
                };
                
                self.status = format!("✗ Error: {}", clean_error);
                self.agent
                    .add_assistant_message(format!("⚠ **Error:** {}", clean_error));
                tracing::error!("Received error: {}", e);
            }
        }

        // Reset scroll to bottom to show new message
        self.reset_scroll();

        // Check if there are queued messages to send
        if !self.message_queue.is_empty() {
            // Concatenate all queued messages with newlines
            let combined = self.message_queue.join("\n\n");
            self.message_queue.clear();

            // Send the combined message
            self.input = combined;
            self.send_message(tx).await?;
        }

        Ok(())
    }

    /// Handle internal commands
    async fn handle_command(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input.split_whitespace().collect();
        let command = parts.first().map(|s| s.to_lowercase()).unwrap_or_default();
        let args: Vec<&str> = parts.iter().skip(1).copied().collect();

        match command.as_str() {
            "/help" | "/h" | "/?" => {
                self.show_help = true;
                self.help_scroll_offset = 0;
            }
            "/quit" | "/exit" | "/q" => {
                self.should_quit = true;
            }
            "/clear" | "/cls" => {
                self.agent.clear_history();
                self.status = "Chat history cleared".to_string();
                self.agent
                    .add_assistant_message("Chat history has been cleared.".to_string());
            }
            "/model" => {
                if args.is_empty() {
                    self.agent.add_assistant_message(format!(
                        "Current model: {}",
                        self.agent.model_name()
                    ));
                } else {
                    self.status = format!("Model changed to: {}", args[0]);
                    self.agent.add_assistant_message(format!(
                        "Model setting updated to '{}' (requires restart to apply)",
                        args[0]
                    ));
                }
            }
            "/history" | "/hist" => {
                let count = self.agent.chat_history().len();
                self.agent
                    .add_assistant_message(format!("Chat history contains {} messages.", count));
            }
            "/status" => {
                self.agent.add_assistant_message(format!(
                    "Status: {}\nModel: {}\nMessages: {}",
                    self.status,
                    self.agent.model_name(),
                    self.agent.chat_history().len()
                ));
            }
            "/yolo" => {
                self.status = "YOLO mode toggled (feature pending)".to_string();
                self.agent.add_assistant_message(
                    "YOLO mode toggle requested. This feature is coming soon!".to_string(),
                );
            }
            "/mcp" | "/mcp-servers" => {
                self.handle_mcp_command(&args).await?;
            }
            _ => {
                self.agent.add_assistant_message(format!(
                    "Unknown command: {}. Type /help for available commands.",
                    command
                ));
            }
        }

        Ok(())
    }

    /// Handle MCP commands
    async fn handle_mcp_command(&mut self, args: &[&str]) -> Result<()> {
        if args.is_empty() {
            // Show MCP status
            let connected = self.agent.mcp_connected();
            let total = self.config.mcp_servers.len();
            let enabled = self.config.enabled_mcp_servers().len();

            let mut msg = "MCP Configuration:\n".to_string();
            msg.push_str(&format!("  Configured servers: {}\n", total));
            msg.push_str(&format!("  Enabled: {}\n", enabled));
            msg.push_str(&format!("  Connected: {}\n\n", connected.len()));

            if !connected.is_empty() {
                msg.push_str("Connected servers:\n");
                for server in connected {
                    msg.push_str(&format!("  ✓ {}\n", server));
                }
            }

            if total > 0 {
                msg.push_str("\nCommands:\n");
                msg.push_str("  /mcp list     - List all configured servers\n");
                msg.push_str("  /mcp tools    - Show available MCP tools\n");
                msg.push_str("  /mcp add <name> <url> - Add a new server\n");
            } else {
                msg.push_str("\nNo MCP servers configured.\n");
                msg.push_str("Add servers by editing ~/.config/pcli2-rig/config.toml\n");
            }

            self.agent.add_assistant_message(msg);
            return Ok(());
        }

        match args[0] {
            "list" => {
                let mut msg = String::from("Configured MCP servers:\n");
                for server in &self.config.mcp_servers {
                    let status = if server.enabled { "✓" } else { "✗" };
                    msg.push_str(&format!("  {} {} ({})\n", status, server.name, server.url));
                }
                self.agent.add_assistant_message(msg);
            }
            "tools" => {
                // Try to get actual tool definitions from ToolServer
                if let Some(handle) = self.agent.tool_server_handle() {
                    match handle.get_tool_defs(None).await {
                        Ok(tool_defs) => {
                            if tool_defs.is_empty() {
                                self.agent.add_assistant_message(
                                    "No MCP tools available.".to_string(),
                                );
                            } else {
                                let mut msg = String::new();
                                msg.push_str(&format!("**Available MCP tools** ({} total):\n\n", tool_defs.len()));
                                for (i, tool) in tool_defs.iter().enumerate() {
                                    msg.push_str(&format!("**{}.** `{}`\n", i + 1, tool.name));
                                    msg.push_str(&format!("    {}\n\n", tool.description));
                                }
                                self.agent.add_assistant_message(msg);
                            }
                        }
                        Err(e) => {
                            self.agent.add_assistant_message(
                                format!("Failed to get tool definitions: {}", e),
                            );
                        }
                    }
                } else {
                    self.agent.add_assistant_message(
                        "No MCP server connected. Configure with --setup-mcp or --mcp-remote.".to_string(),
                    );
                }
            }
            "add" => {
                if args.len() < 3 {
                    self.agent
                        .add_assistant_message("Usage: /mcp add <name> <url>".to_string());
                } else {
                    self.agent.add_assistant_message(format!(
                        "To add MCP server '{}', edit ~/.config/pcli2-rig/config.toml\n\
                         Add this section:\n\
                         [[mcp_servers]]\n\
                         name = \"{}\"\n\
                         url = \"{}\"\n\
                         enabled = true",
                        args[1], args[1], args[2]
                    ));
                }
            }
            _ => {
                self.agent.add_assistant_message(format!(
                    "Unknown MCP command: {}. Type /mcp for help.",
                    args[0]
                ));
            }
        }

        Ok(())
    }

    /// Execute the pending tool call
    async fn execute_pending_tool(&mut self) -> Result<()> {
        if let Some(pending) = self.pending_tool_call.take() {
            self.status = format!("Executing {}...", pending.tool_name);

            match agent::execute_tool_call(&pending.tool_name, &pending.arguments).await {
                Ok(result) => {
                    self.agent.add_tool_result(result.clone());
                    self.status = "Tool executed successfully".to_string();

                    // Send the tool result back to get the agent's response
                    let follow_up = format!("The tool returned:\n{}", result);
                    match self.agent.chat(follow_up).await {
                        Ok(response) => {
                            debug!("Agent follow-up response: {}", response);
                        }
                        Err(e) => {
                            self.status = format!("Error in follow-up: {}", e);
                        }
                    }
                }
                Err(e) => {
                    // Clean up repetitive error messages
                    let error_msg = e.to_string();
                    let clean_error = if error_msg.contains("Tool call error:") {
                        error_msg.split("Tool call error:").last().unwrap_or(&error_msg).trim().to_string()
                    } else if error_msg.contains("ToolCallError:") {
                        error_msg.split("ToolCallError:").last().unwrap_or(&error_msg).trim().to_string()
                    } else {
                        error_msg
                    };
                    
                    self.status = format!("Tool execution failed: {}", clean_error);
                    self.agent.add_tool_result(format!("Error: {}", clean_error));
                }
            }
        }

        Ok(())
    }

    /// Render the UI
    fn render(&mut self, frame: &mut Frame) {
        ui::render(frame, self);
    }

    /// Get the current input
    pub fn input(&self) -> &str {
        &self.input
    }

    /// Get the current status
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Get whether there's a pending tool call
    pub fn has_pending_tool_call(&self) -> bool {
        self.pending_tool_call.is_some()
    }

    /// Get the pending tool call
    pub fn pending_tool_call(&self) -> Option<&PendingToolCall> {
        self.pending_tool_call.as_ref()
    }

    /// Get the agent
    pub fn agent(&self) -> &Agent {
        &self.agent
    }

    /// Get cursor position
    pub fn cursor_pos(&self) -> usize {
        self.cursor_pos
    }

    /// Get input horizontal scroll offset
    pub fn input_hscroll_offset(&self) -> usize {
        self.input_hscroll_offset
    }

    /// Get logs
    pub fn logs(&self) -> &[String] {
        &self.logs
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get focus pane
    pub fn focus_pane(&self) -> usize {
        self.focus_pane
    }

    /// Get log scroll offset
    pub fn log_scroll_offset(&self) -> usize {
        self.log_scroll_offset
    }

    /// Get log horizontal scroll offset
    pub fn log_hscroll_offset(&self) -> usize {
        self.log_hscroll_offset
    }

    /// Reset horizontal scroll to beginning
    pub fn reset_log_hscroll(&mut self) {
        self.log_hscroll_offset = 0;
    }

    /// Reset scroll to bottom
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
        self.log_scroll_offset = 0;
        self.reset_log_hscroll();
    }

    /// Sync logs from shared buffer
    pub fn sync_logs(&mut self) {
        if let Ok(buffer) = LOG_BUFFER.lock() {
            for line in buffer.iter() {
                if !self.logs.contains(line) {
                    self.logs.push(line.clone());
                }
            }
            // Trim old logs if exceeding max
            if self.logs.len() > self.max_logs {
                let excess = self.logs.len() - self.max_logs;
                self.logs.drain(0..excess);
            }
        }
    }

    /// Sample CPU usage and add to history
    pub fn sample_cpu(&mut self) {
        self.sys.refresh_cpu_usage();
        let cpu_usage = self.sys.global_cpu_usage();

        self.cpu_history.push(cpu_usage);

        // Keep only the last N samples
        if self.cpu_history.len() > CPU_HISTORY_SIZE {
            self.cpu_history.remove(0);
        }
    }

    /// Navigate input history (bash-style with Up/Down arrows)
    pub fn navigate_history(&mut self, direction: i32) {
        if self.input_history.is_empty() {
            return;
        }

        if direction < 0 {
            // Up arrow: go to older command
            if self.history_index == 0 {
                // Save current input before browsing history
                self.history_original = self.input.clone();
            }
            self.history_index = (self.history_index + 1).min(self.input_history.len());
        } else {
            // Down arrow: go to newer command
            if self.history_index > 0 {
                self.history_index -= 1;
            }
        }

        // Load the command from history
        if self.history_index > 0 {
            let idx = self.input_history.len() - self.history_index;
            self.input = self.input_history[idx].clone();
        } else {
            // Restore original input
            self.input = self.history_original.clone();
        }
        self.cursor_pos = self.input.len(); // Move cursor to end
    }

    /// Adjust input horizontal scroll to keep cursor visible
    pub fn adjust_input_scroll(&mut self) {
        // If cursor is before scroll offset, scroll left
        if self.cursor_pos < self.input_hscroll_offset {
            self.input_hscroll_offset = self.cursor_pos;
        }
        // If cursor is beyond visible area, scroll right
        // Assume ~80 chars visible as a reasonable default
        // The UI will handle the actual visible width
        let visible_chars = 80; // Approximate, UI handles actual width
        if self.cursor_pos >= self.input_hscroll_offset + visible_chars {
            self.input_hscroll_offset = self.cursor_pos.saturating_sub(visible_chars - 1);
        }
    }

    /// Get CPU history for sparkline rendering
    pub fn cpu_history(&self) -> &[f32] {
        &self.cpu_history
    }

    /// Cancel the current in-flight request
    pub fn cancel_request(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
            self.is_thinking = false;
            self.status = "Request cancelled".to_string();
            debug!("Request cancelled by user");
        }
    }

    /// Handle mouse event for focus and scrolling
    pub fn handle_mouse(
        &mut self,
        event: crossterm::event::MouseEvent,
        area: ratatui::layout::Rect,
    ) {
        use crossterm::event::{MouseButton, MouseEventKind};

        // Calculate pane boundaries (same as in ui.rs)
        let chunks = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                ratatui::layout::Constraint::Min(8),    // Chat history
                ratatui::layout::Constraint::Length(3), // Input
                ratatui::layout::Constraint::Length(6), // Log panel
                ratatui::layout::Constraint::Length(1), // Status
            ])
            .split(area);

        match event.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                // Click to change focus
                if event.row >= chunks[0].y && event.row < chunks[0].y + chunks[0].height {
                    self.focus_pane = 0; // Chat
                } else if event.row >= chunks[1].y && event.row < chunks[1].y + chunks[1].height {
                    self.focus_pane = 1; // Input
                } else if event.row >= chunks[2].y && event.row < chunks[2].y + chunks[2].height {
                    self.focus_pane = 2; // Logs
                }
            }
            MouseEventKind::ScrollUp => {
                // Scroll up in the focused pane
                if self.focus_pane == 0 {
                    self.scroll_offset = self.scroll_offset.saturating_add(3);
                } else if self.focus_pane == 2 {
                    self.log_scroll_offset = self.log_scroll_offset.saturating_add(3);
                }
            }
            MouseEventKind::ScrollDown => {
                // Scroll down in the focused pane
                if self.focus_pane == 0 {
                    self.scroll_offset = self.scroll_offset.saturating_sub(3);
                } else if self.focus_pane == 2 {
                    self.log_scroll_offset = self.log_scroll_offset.saturating_sub(3);
                }
            }
            MouseEventKind::ScrollLeft => {
                // Horizontal scroll left in logs pane
                if self.focus_pane == 2 {
                    self.log_hscroll_offset = self.log_hscroll_offset.saturating_sub(5);
                }
            }
            MouseEventKind::ScrollRight => {
                // Horizontal scroll right in logs pane
                if self.focus_pane == 2 {
                    self.log_hscroll_offset = self.log_hscroll_offset.saturating_add(5);
                }
            }
            _ => {}
        }
    }

    /// Get thinking start time
    pub fn thinking_start(&self) -> std::time::Instant {
        self.thinking_start
    }

    /// Check if currently thinking
    pub fn is_thinking(&self) -> bool {
        self.is_thinking
    }

    /// Check if help modal is shown
    pub fn show_help(&self) -> bool {
        self.show_help
    }

    /// Get help scroll offset
    pub fn help_scroll_offset(&self) -> usize {
        self.help_scroll_offset
    }

    /// Get detailed help text
    pub fn get_help_text() -> String {
        let config_path = crate::config::Config::config_file_path()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "~/.config/pcli2-rig/config.toml".to_string());
        
        format!(r#"PCLI2-RIG - Local AI Agent
═══════════════════════════════════════════════════════════

A beautiful TUI-based AI coding assistant powered by Ollama.

COMMANDS
───────────────────────────────────────────────────────────

/help, /h, /?     Show this help message
/quit, /exit, /q  Exit the application
/clear, /cls      Clear chat history
/model [name]     Show or set the current model
/history, /hist   Show message count
/status           Show current status
/mcp              Show MCP server status
/mcp list         List configured MCP servers
/mcp tools        Show available MCP tools
/yolo             Toggle YOLO mode (skip tool confirmation)

MOUSE CONTROLS
───────────────────────────────────────────────────────────

Left Click       Focus on clicked pane
Scroll Wheel     Scroll in focused pane (3 lines)

KEYBOARD SHORTCUTS
───────────────────────────────────────────────────────────

Global:
  Ctrl+C          Quit application
  Ctrl+K          Clear chat history
  Tab             Switch focus between panes
  Shift+Tab       Switch focus backwards
  Esc             Close modal dialogs

Input Pane (when focused):
  Enter           Send message
  ↑/↓             Previous/next command in history (or move cursor)
  Home/End        Jump to start/end of input
  Backspace       Delete character before cursor
  Delete          Delete character at cursor
  Ctrl+←/→        Jump by word (if supported)

Chat History Pane (when focused):
  ↑/↓             Scroll 1 line
  PageUp/PageDown Scroll 5 lines

Logs Pane (when focused):
  ↑/↓             Scroll 1 line
  PageUp/PageDown Scroll 5 lines

Tool Confirmation:
  Y/Enter         Confirm tool execution
  N/Esc           Cancel tool execution

PANES
───────────────────────────────────────────────────────────

Chat History [N]  - Conversation with AI (N = message count)
                  - Shows user messages and AI responses
                  - Markdown rendered for AI responses

Input │ model │ 🔌N - Text input for messages
                  - Model name shown in title
                  - 🔌N shows N connected MCP servers

Logs              - Real-time application logs
                  - Color-coded by log level:
                    ✗ Red = Errors
                    ⚠ Yellow = Warnings
                    ✓ Green = Info/Success
                    ⋯ Cyan = Debug
                    • Gray = Other

Status            - Current application status
                  - Animated spinner when processing
                  - CPU sparkline during LLM requests

CONFIGURATION
───────────────────────────────────────────────────────────

Config file: {}

Example configuration:
  model = "qwen2.5-coder:3b"
  host = "http://localhost:11434"
  yolo = false

  [[mcp_servers]]
  name = "filesystem"
  url = "http://localhost:3000"
  enabled = true

LOGS
───────────────────────────────────────────────────────────

Application logs: ~/.local/state/pcli2-rig/pcli2-rig.log

Press Esc, Enter, or 'q' to close this help."#, config_path)
    }
}
