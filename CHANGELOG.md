# Changelog

All notable changes to PCLI2-RIG will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-02-19

### Added
- **MCP Image Content Support** - Automatic MIME type detection for thumbnail images
  - Detects base64 image data from MCP thumbnail/image tools
  - Wraps base64 in proper MCP image content format with correct MIME type
  - Supports PNG, JPEG, GIF, and WebP image format auto-detection
  - Enables Claude Desktop to render thumbnail images inline via artifacts
  - Comprehensive unit tests for image content conversion
- **CPU Usage Sparkline** - Real-time CPU monitoring displayed in status bar during LLM requests
- **Command History** - Bash-style Up/Down arrow navigation for previous inputs
- **Horizontal Log Scrolling** - Scroll long log lines with ←/→ arrow keys
- **Home/End for Logs** - Jump to start/end of log line horizontally
- **Request Cancellation** - Press Esc to cancel in-flight LLM requests
- **Platform-Specific Config Path** - Help modal now shows actual config file path for your OS

### Changed
- **Dynamic Line Width** - User messages now wrap based on terminal width instead of hardcoded 80 chars
- **Warm Color Palette** - Updated TUI with black background and warm orange/golden accents
- **Gradient ASCII Banner** - New banner design with smooth left-to-right color gradient
- **Emoji Log Prefixes** - Log messages now have ✗/⚠/✓/• prefixes for quick severity identification
- **Input Cursor Visibility** - Orange cursor now visible in empty input field

### Fixed
- Removed redundant thinking spinner from chat history (status bar already shows it)
- Fixed extra spacing in emoji log prefixes

### Technical
- Added `sysinfo` crate for CPU monitoring (minimal features for lightweight operation)
- Added `gilt` crate for Unicode sparkline rendering
- Added `tokio-util` for request cancellation support
- Filtered noisy markdown parser warnings (HTML, unsupported syntaxes)
- Added `try_convert_to_image_content` method to `SimpleMcpClient`
- Enhanced tool result parsing to detect and convert image data
- Added 6 unit tests covering image detection, MIME type detection, and edge cases

## [Unreleased]

<!-- No unreleased changes yet -->

## [0.1.0] - 2026-02-19

### Initial Release
- Beautiful TUI interface with ratatui and crossterm
- Local AI powered by Ollama and Rig framework
- MCP (Model Context Protocol) server integration
- Tool support with confirmation dialogs
- Markdown rendering for AI responses
- Mouse support with click-to-focus and scroll
- Help modal with keyboard shortcuts
- Configuration file support
- YOLO mode for skipping tool confirmations
- Verbose logging option
