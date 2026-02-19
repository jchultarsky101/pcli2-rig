//! UI rendering with ratatui

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};
use tui_markdown::from_str;

use crate::app::App;

/// Colors for the dark theme
mod colors {
    use ratatui::style::Color;

    pub const BACKGROUND: Color = Color::Rgb(15, 15, 20);
    pub const FOREGROUND: Color = Color::Rgb(220, 220, 220);
    pub const DIM: Color = Color::Rgb(100, 100, 100);
    
    // Accent colors
    pub const ACCENT_CYAN: Color = Color::Rgb(0, 229, 255);
    pub const ACCENT_PURPLE: Color = Color::Rgb(123, 92, 255);
    pub const ACCENT_GREEN: Color = Color::Rgb(80, 250, 123);
    pub const ACCENT_YELLOW: Color = Color::Rgb(245, 158, 11);
    pub const ACCENT_ORANGE: Color = Color::Rgb(255, 140, 0);
    pub const ERROR_RED: Color = Color::Rgb(255, 85, 85);
    pub const USER_BG: Color = Color::Rgb(30, 30, 40);
    pub const ASSISTANT_BG: Color = Color::Rgb(25, 25, 35);
    
    // Cursor colors - bright orange for high visibility
    #[allow(dead_code)]
    pub const CURSOR_BG: Color = Color::Rgb(255, 140, 0);
    pub const CURSOR_FG: Color = Color::Rgb(0, 0, 0);
}

/// Render the main UI
pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Main layout: chat, input, logs, status
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(8),    // Chat history
            Constraint::Length(3), // Input
            Constraint::Length(6), // Log panel (6 lines)
            Constraint::Length(1), // Status
        ])
        .split(area);

    render_chat(frame, app, chunks[0], app.focus_pane() == 0);
    render_input(frame, app, chunks[1], app.focus_pane() == 1);
    render_logs(frame, app, chunks[2], app.focus_pane() == 2);
    render_status(frame, app, chunks[3]);

    // Render tool confirmation dialog if needed
    if app.has_pending_tool_call() {
        render_tool_confirmation(frame, app, area);
    }
}

/// Render the chat history
fn render_chat(frame: &mut Frame, app: &App, area: Rect, is_focused: bool) {
    let border_color = if is_focused { colors::ACCENT_GREEN } else { colors::DIM };
    let history = app.agent().chat_history();

    // Build all lines with background colors
    let mut all_lines: Vec<(Line, Option<ratatui::style::Color>)> = Vec::new();

    for msg in history {
        let bg_color = match msg.role {
            crate::agent::MessageRole::User => Some(colors::USER_BG),
            crate::agent::MessageRole::Assistant => Some(colors::ASSISTANT_BG),
            crate::agent::MessageRole::System => Some(colors::ASSISTANT_BG),
            crate::agent::MessageRole::ToolResult => Some(colors::USER_BG),
        };

        let (prefix, style) = match msg.role {
            crate::agent::MessageRole::User => (
                "👤 You:",
                Style::default()
                    .fg(colors::ACCENT_GREEN)
                    .add_modifier(Modifier::BOLD),
            ),
            crate::agent::MessageRole::Assistant => (
                "🤖 Assistant:",
                Style::default()
                    .fg(colors::ACCENT_CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            crate::agent::MessageRole::System => (
                "⚙️ System:",
                Style::default()
                    .fg(colors::ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            ),
            crate::agent::MessageRole::ToolResult => (
                "🔧 Tool:",
                Style::default()
                    .fg(colors::ACCENT_PURPLE)
                    .add_modifier(Modifier::BOLD),
            ),
        };

        // Add prefix line
        all_lines.push((Line::from(Span::styled(prefix, style)), bg_color));
        
        // Render content - use markdown for assistant messages
        if msg.role == crate::agent::MessageRole::Assistant {
            let markdown_text = from_str(&msg.content);
            for line in markdown_text.lines {
                all_lines.push((line, bg_color));
            }
        } else {
            let content = format_msg_content(&msg.content, 80);
            for line in content.lines() {
                all_lines.push((Line::from(Span::styled(
                    line.to_string(),
                    Style::default().fg(colors::FOREGROUND),
                )), bg_color));
            }
        }
        
        all_lines.push((Line::from(""), bg_color));
    }

    // Add thinking indicator
    if app.is_thinking() {
        let elapsed = app.thinking_start().elapsed().as_secs();
        let spinner = match elapsed % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            3 => "⠸",
            _ => "⠋",
        };
        all_lines.push((Line::from(Span::styled(
            format!("  {} thinking...", spinner),
            Style::default().fg(colors::ACCENT_YELLOW),
        )), None));
    }

    // Calculate visible height (subtract 2 for borders/title)
    let visible_height = area.height.saturating_sub(2) as usize;
    let total_lines = all_lines.len();

    // Calculate scroll position (0 = at bottom showing newest lines)
    let scroll_start = if total_lines <= visible_height {
        0
    } else {
        // When scroll_offset=0, show the last visible_height lines
        // When scroll_offset>0, scroll up by that many lines
        total_lines.saturating_sub(visible_height + app.scroll_offset())
    };

    // Get visible lines
    let visible_lines: Vec<(Line, Option<ratatui::style::Color>)> =
        all_lines.into_iter().skip(scroll_start).take(visible_height).collect();

    // Group consecutive lines with same background into ListItems
    let mut items: Vec<ListItem> = Vec::new();
    let mut current_bg: Option<ratatui::style::Color> = None;
    let mut current_item_lines: Vec<Line> = Vec::new();

    for (line, bg) in visible_lines {
        if current_bg != bg && !current_item_lines.is_empty() {
            items.push(ListItem::new(current_item_lines)
                .style(Style::default().bg(current_bg.unwrap_or(colors::BACKGROUND))));
            current_item_lines = Vec::new();
        }
        current_bg = bg;
        current_item_lines.push(line);
    }

    if !current_item_lines.is_empty() {
        items.push(ListItem::new(current_item_lines)
            .style(Style::default().bg(current_bg.unwrap_or(colors::BACKGROUND))));
    }

    let mut block = Block::default()
        .title(format!(" Chat History [{}] ", history.len()))
        .title_style(Style::default().fg(border_color))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(colors::BACKGROUND));

    // Show mini-help only when focused
    if is_focused {
        block = block.title_bottom(Line::from(Span::styled(
            " ↑/↓ scroll, PgUp/PgDown fast ",
            Style::default().fg(colors::DIM),
        )));
    }

    let chat = List::new(items).block(block);

    frame.render_widget(chat, area);
}

/// Render the input area with visible blinking cursor
fn render_input(frame: &mut Frame, app: &App, area: Rect, is_focused: bool) {
    let border_color = if is_focused { colors::ACCENT_CYAN } else { colors::DIM };
    
    let input_text = if app.input().is_empty() {
        // Show placeholder with blinking block cursor at the start
        vec![Line::from(vec![
            Span::styled(
                "█",
                Style::default()
                    .fg(colors::CURSOR_FG)
                    .bg(colors::ACCENT_ORANGE)
                    .add_modifier(Modifier::RAPID_BLINK),
            ),
            Span::styled(
                " Type your message...",
                Style::default().fg(colors::DIM),
            ),
        ])]
    } else {
        // Show actual input with visible blinking block cursor
        let cursor_pos = app.cursor_pos();
        let (before_cursor, after_cursor) = app.input().split_at(cursor_pos);
        let cursor_char = if cursor_pos < app.input().len() {
            &app.input()[cursor_pos..cursor_pos + 1]
        } else {
            " "
        };

        vec![Line::from(vec![
            Span::raw(before_cursor),
            Span::styled(
                cursor_char,
                Style::default()
                    .fg(colors::CURSOR_FG)
                    .bg(colors::ACCENT_ORANGE)
                    .add_modifier(Modifier::RAPID_BLINK),
            ),
            Span::raw(after_cursor),
        ])]
    };

    let input = Paragraph::new(input_text)
        .block(
            Block::default()
                .title({
                    let model = app.agent().model_name();
                    let mcp_count = app.agent().mcp_server_count();
                    if mcp_count > 0 {
                        format!(" Input │ {} │ 🔌{} ", model, mcp_count)
                    } else {
                        format!(" Input │ {} ", model)
                    }
                })
                .title_style(Style::default().fg(border_color))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(Style::default().bg(colors::BACKGROUND)),
        );

    frame.render_widget(input, area);
}

/// Render the log panel
fn render_logs(frame: &mut Frame, app: &App, area: Rect, is_focused: bool) {
    let border_color = if is_focused { colors::ACCENT_PURPLE } else { colors::DIM };
    let logs = app.logs();

    // Calculate visible lines (subtract 2 for borders/title)
    let visible_lines = area.height.saturating_sub(2) as usize;
    let total_logs = logs.len();

    // Calculate scroll position (0 = at bottom showing newest logs)
    let scroll_start = if total_logs <= visible_lines {
        0
    } else {
        total_logs.saturating_sub(visible_lines + app.log_scroll_offset())
    };

    let log_lines: Vec<Line> = logs
        .iter()
        .skip(scroll_start)
        .take(visible_lines)
        .map(|line| {
            // Add emoji based on log content
            let (emoji, color) = if line.contains("ERROR") || line.contains("Error") || line.contains("failed") {
                ("✗ ", colors::ERROR_RED)
            } else if line.contains("WARN") || line.contains("Empty") {
                ("⚠ ", colors::ACCENT_YELLOW)
            } else if line.contains("INFO") || line.contains("Ready") || line.contains("success") {
                ("✓ ", colors::ACCENT_GREEN)
            } else if line.contains("DEBUG") || line.contains("Sending") || line.contains("Received") {
                ("⋯ ", colors::ACCENT_CYAN)
            } else {
                ("• ", colors::DIM)
            };

            Line::from(vec![
                Span::styled(emoji, Style::default().fg(color)),
                Span::styled(line, Style::default().fg(colors::FOREGROUND)),
            ])
        })
        .collect();

    let mut block = Block::default()
        .title(" Logs ")
        .title_style(Style::default().fg(border_color))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(colors::BACKGROUND));

    // Show mini-help only when focused
    if is_focused {
        block = block.title_bottom(Line::from(Span::styled(
            " ↑/↓ scroll, PgUp/PgDown fast ",
            Style::default().fg(colors::DIM),
        )));
    }

    let logs_paragraph = Paragraph::new(log_lines).block(block);

    frame.render_widget(logs_paragraph, area);
}

/// Render the status bar with animated thinking indicator
fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    // Create fixed-width spinner for thinking status (same as chat history)
    let status_text = if app.is_thinking() {
        let elapsed = app.thinking_start().elapsed().as_secs();
        let spinner = match elapsed % 4 {
            0 => "⠋",
            1 => "⠙",
            2 => "⠹",
            3 => "⠸",
            _ => "⠋",
        };
        format!(" {} {}",
            spinner,
            app.status()
        )
    } else {
        format!(" {} ",
            app.status()
        )
    };

    let status_style = if app.status().contains("Error") {
        Style::default().fg(colors::ERROR_RED)
    } else if app.status().contains("✓") {
        Style::default().fg(colors::ACCENT_GREEN)
    } else if app.is_thinking() {
        Style::default().fg(colors::ACCENT_YELLOW)
    } else {
        Style::default().fg(colors::DIM)
    };

    let status = Paragraph::new(Line::from(Span::styled(
        status_text,
        status_style,
    )));

    frame.render_widget(status, area);
}

/// Render tool confirmation dialog
fn render_tool_confirmation(frame: &mut Frame, app: &App, area: Rect) {
    // Create centered dialog
    let dialog_width = 60.min(area.width - 4);
    let dialog_height = 10.min(area.height - 4);
    let dialog_area = Rect::new(
        (area.width - dialog_width) / 2,
        (area.height - dialog_height) / 2,
        dialog_width,
        dialog_height,
    );

    // Clear the area behind the dialog
    frame.render_widget(ratatui::widgets::Clear, dialog_area);

    if let Some(pending) = app.pending_tool_call() {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "🔧 Tool Execution Requested",
                Style::default()
                    .fg(colors::ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(format!("Tool: {}", pending.tool_name)),
            Line::from(format!("Arguments: {}", pending.arguments)),
            Line::from(""),
            Line::from(Span::styled(
                "Execute this tool? (Y/n)",
                Style::default().fg(colors::FOREGROUND),
            )),
            Line::from(""),
        ];

        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Confirmation Required ")
                    .title_style(Style::default().fg(colors::ACCENT_YELLOW))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(colors::ACCENT_YELLOW))
                    .style(Style::default().bg(colors::BACKGROUND)),
            );

        frame.render_widget(dialog, dialog_area);
    }
}

/// Format message content for display
fn format_msg_content(content: &str, max_width: usize) -> String {
    // Simple word wrapping
    let mut result = String::new();
    let mut current_line = String::new();
    
    for word in content.split_whitespace() {
        if current_line.len() + word.len() + 1 > max_width {
            result.push_str(&current_line);
            result.push('\n');
            current_line.clear();
        }
        if !current_line.is_empty() {
            current_line.push(' ');
        }
        current_line.push_str(word);
    }
    
    if !current_line.is_empty() {
        result.push_str(&current_line);
    }
    
    result
}
