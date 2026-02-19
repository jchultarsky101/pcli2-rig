//! UI rendering with ratatui

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
};
use tui_markdown::from_str;
use ansi_to_tui::IntoText;

use crate::app::App;

/// Colors for the dark theme (warm color palette)
mod colors {
    use ratatui::style::Color;

    pub const BACKGROUND: Color = Color::Rgb(0, 0, 0);
    pub const FOREGROUND: Color = Color::Rgb(230, 220, 200);
    pub const DIM: Color = Color::Rgb(120, 110, 100);

    // Accent colors (warm palette)
    pub const ACCENT_CYAN: Color = Color::Rgb(100, 200, 210);
    pub const ACCENT_PURPLE: Color = Color::Rgb(180, 130, 200);
    pub const ACCENT_GREEN: Color = Color::Rgb(120, 200, 120);
    pub const ACCENT_YELLOW: Color = Color::Rgb(255, 180, 60);
    pub const ACCENT_ORANGE: Color = Color::Rgb(255, 150, 50);
    pub const ACCENT_WARM_ORANGE: Color = Color::Rgb(255, 130, 60);
    pub const ACCENT_DARK_WARM_RED: Color = Color::Rgb(200, 80, 60);
    pub const ERROR_RED: Color = Color::Rgb(255, 100, 100);
    pub const USER_BG: Color = Color::Rgb(18, 18, 18);
    pub const ASSISTANT_BG: Color = Color::Rgb(12, 12, 12);

    // Cursor colors - warm orange for high visibility
    #[allow(dead_code)]
    pub const CURSOR_BG: Color = Color::Rgb(255, 150, 50);
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

    // Render help modal if active
    if app.show_help() {
        render_help_modal(frame, app, area);
    }

    // Render tool confirmation dialog if needed
    if app.has_pending_tool_call() {
        render_tool_confirmation(frame, app, area);
    }
}

/// Render the chat history
fn render_chat(frame: &mut Frame, app: &App, area: Rect, is_focused: bool) {
    let border_color = if is_focused {
        colors::ACCENT_GREEN
    } else {
        colors::DIM
    };
    let history = app.agent().chat_history();

    // ASCII art banner (62 chars wide, 6 lines tall)
    const ASCII_BANNER: &str = r#"
██████╗  ██████╗██╗     ██╗██████╗     ██████╗ ██╗ ██████╗
██╔══██╗██╔════╝██║     ██║╚════██╗    ██╔══██╗██║██╔════╝
██████╔╝██║     ██║     ██║ █████╔╝    ██████╔╝██║██║  ███╗
██╔═══╝ ██║     ██║     ██║██╔═══╝     ██╔══██╗██║██║   ██║
██║     ╚██████╗███████╗██║███████╗    ██║  ██║██║╚██████╔╝
╚═╝      ╚═════╝╚══════╝╚═╝╚══════╝    ╚═╝  ╚═╝╚═╝ ╚═════╝"#;

    // Gradient colors for ASCII banner (left to right: warm orange → golden yellow)
    // Start: RGB(255, 130, 60) - warm orange
    // End:   RGB(255, 200, 80) - golden yellow

    // Build all lines with background colors
    let mut all_lines: Vec<(Line, Option<ratatui::style::Color>)> = Vec::new();
    let total_messages = history.len();

    // Add ASCII banner if terminal is wide enough (64+ chars) and tall enough (10+ lines)
    if area.width >= 64 && area.height >= 10 {
        for line in ASCII_BANNER.lines() {
            if line.is_empty() {
                all_lines.push((Line::from(""), None));
                continue;
            }

            // Create smooth gradient effect by coloring each character
            let chars: Vec<char> = line.chars().collect();
            let max_len = chars.len().saturating_sub(1);
            let mut spans: Vec<Span> = Vec::new();

            for (i, &ch) in chars.iter().enumerate() {
                // Calculate interpolation factor (0.0 to 1.0)
                let t = if max_len == 0 {
                    0.0
                } else {
                    i as f32 / max_len as f32
                };

                // Interpolate RGB values from warm orange to golden yellow
                let r = (255.0 + (255.0 - 255.0) * t) as u8;
                let g = (130.0 + (200.0 - 130.0) * t) as u8;
                let b = (60.0 + (80.0 - 60.0) * t) as u8;

                spans.push(Span::styled(
                    ch.to_string(),
                    Style::default()
                        .fg(ratatui::style::Color::Rgb(r, g, b))
                        .add_modifier(Modifier::BOLD),
                ));
            }

            all_lines.push((Line::from(spans), None));
        }
        all_lines.push((Line::from(""), None)); // Spacing after banner
    }

    for (idx, msg) in history.iter().enumerate() {
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
                    .fg(colors::ACCENT_WARM_ORANGE)
                    .add_modifier(Modifier::BOLD),
            ),
            crate::agent::MessageRole::Assistant => (
                "🤖 Assistant:",
                Style::default()
                    .fg(colors::ACCENT_WARM_ORANGE)
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
                all_lines.push((
                    Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(colors::FOREGROUND),
                    )),
                    bg_color,
                ));
            }
        }

        // Add single spacing line between messages (not after the last one)
        if idx < total_messages - 1 {
            all_lines.push((Line::from(""), bg_color));
        }
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
    let visible_lines: Vec<(Line, Option<ratatui::style::Color>)> = all_lines
        .into_iter()
        .skip(scroll_start)
        .take(visible_height)
        .collect();

    // Group consecutive lines with same background into ListItems
    let mut items: Vec<ListItem> = Vec::new();
    let mut current_bg: Option<ratatui::style::Color> = None;
    let mut current_item_lines: Vec<Line> = Vec::new();

    for (line, bg) in visible_lines {
        if current_bg != bg && !current_item_lines.is_empty() {
            items.push(
                ListItem::new(current_item_lines)
                    .style(Style::default().bg(current_bg.unwrap_or(colors::BACKGROUND))),
            );
            current_item_lines = Vec::new();
        }
        current_bg = bg;
        current_item_lines.push(line);
    }

    if !current_item_lines.is_empty() {
        items.push(
            ListItem::new(current_item_lines)
                .style(Style::default().bg(current_bg.unwrap_or(colors::BACKGROUND))),
        );
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
    let border_color = if is_focused {
        colors::ACCENT_DARK_WARM_RED
    } else {
        colors::DIM
    };

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
            Span::styled(" Type your message...", Style::default().fg(colors::DIM)),
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

    let input = Paragraph::new(input_text).block(
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
    let border_color = if is_focused {
        colors::ACCENT_PURPLE
    } else {
        colors::DIM
    };
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
        .flat_map(|line| {
            // Parse ANSI color codes and convert to ratatui Lines
            line.into_text()
                .map(|text| text.lines.into_iter())
                .unwrap_or_else(|_| vec![Line::from(line.as_str())].into_iter())
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
        format!(" {} {}", spinner, app.status())
    } else {
        format!(" {} ", app.status())
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

    let status = Paragraph::new(Line::from(Span::styled(status_text, status_style)));

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

        let dialog = Paragraph::new(lines).block(
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

/// Render help modal dialog with scrollable text
fn render_help_modal(frame: &mut Frame, app: &App, area: Rect) {
    // Create centered dialog (80% width, 90% height)
    let dialog_width = (area.width * 80) / 100;
    let dialog_height = (area.height * 90) / 100;
    let dialog_area = Rect::new(
        (area.width - dialog_width) / 2,
        (area.height - dialog_height) / 2,
        dialog_width,
        dialog_height,
    );

    // Clear the area behind the dialog
    frame.render_widget(ratatui::widgets::Clear, dialog_area);

    let help_text = App::get_help_text();
    let scroll_offset = app.help_scroll_offset();

    // Parse help text into lines
    let all_lines: Vec<&str> = help_text.lines().collect();
    let total_lines = all_lines.len();

    // Calculate visible range
    let visible_lines = dialog_height.saturating_sub(4) as usize; // Subtract borders and title
    let start = scroll_offset.min(total_lines.saturating_sub(visible_lines));
    let end = (start + visible_lines).min(total_lines);

    // Build visible lines with styling
    let mut styled_lines: Vec<Line> = Vec::new();

    for (i, line) in all_lines.iter().skip(start).take(end - start).enumerate() {
        let _line_num = start + i;

        // Style based on line content
        if line.starts_with("PCLI2-RIG") || line.starts_with("═══") {
            styled_lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default()
                    .fg(colors::ACCENT_CYAN)
                    .add_modifier(Modifier::BOLD),
            )));
        } else if line.ends_with("────────") || line.is_empty() {
            styled_lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(colors::DIM),
            )));
        } else if line.contains("MOUSE")
            || line.contains("KEYBOARD")
            || line.contains("PANES")
            || line.contains("CONFIGURATION")
            || line.contains("LOGS")
            || line.contains("COMMANDS")
        {
            styled_lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default()
                    .fg(colors::ACCENT_YELLOW)
                    .add_modifier(Modifier::BOLD),
            )));
        } else if line.trim().starts_with('/')
            || line.trim().starts_with("Ctrl+")
            || line.trim().starts_with("Tab")
            || line.trim().starts_with("Shift+")
            || line.trim().starts_with("Enter")
            || line.trim().starts_with("Esc")
            || line.trim().starts_with("↑")
            || line.trim().starts_with("↓")
            || line.trim().starts_with("PageUp")
            || line.trim().starts_with("PageDown")
            || line.trim().starts_with("Left")
            || line.trim().starts_with("Scroll")
            || line.trim().starts_with("Backspace")
            || line.trim().starts_with("Delete")
            || line.trim().starts_with("Home")
            || line.trim().starts_with("End")
            || line.trim().starts_with("Y/")
            || line.trim().starts_with("N/")
        {
            styled_lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(colors::ACCENT_GREEN),
            )));
        } else if line.contains("Chat History")
            || line.contains("Input")
            || line.contains("Logs")
            || line.contains("Status")
        {
            styled_lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(colors::ACCENT_PURPLE),
            )));
        } else if line.contains("Press") {
            styled_lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default()
                    .fg(colors::ACCENT_YELLOW)
                    .add_modifier(Modifier::ITALIC),
            )));
        } else {
            styled_lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(colors::FOREGROUND),
            )));
        }
    }

    // Add scroll indicator
    let scroll_info = if total_lines > visible_lines {
        format!(" [{}-{}/{}] ↑↓ scroll ", start + 1, end, total_lines)
    } else {
        String::new()
    };

    let dialog = Paragraph::new(styled_lines)
        .block(
            Block::default()
                .title(" Help ")
                .title_style(
                    Style::default()
                        .fg(colors::ACCENT_CYAN)
                        .add_modifier(Modifier::BOLD),
                )
                .title_bottom(Span::styled(scroll_info, Style::default().fg(colors::DIM)))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(colors::ACCENT_CYAN))
                .style(Style::default().bg(colors::BACKGROUND)),
        )
        .wrap(ratatui::widgets::Wrap { trim: false });

    frame.render_widget(dialog, dialog_area);
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
