# PCLI2-RIG

A beautiful TUI-based local AI agent powered by [Rig](https://github.com/0xPlaygrounds/rig) and [Ollama](https://ollama.com/).

## Features

- 🎨 **Beautiful Dark TUI** - Inspired by Qwen Code with ASCII banner and emoji-rich interface
- 🤖 **Local AI** - Runs entirely on your laptop using Ollama
- 🛠️ **Tool Support** - Built-in tools for file operations, command execution, and code search
- 🔒 **Privacy First** - All inference happens locally, no data leaves your machine
- ⚡ **Lightweight** - Optimized for quick startup and minimal resource usage
- 🎯 **YOLO Mode** - Skip tool confirmation with `--yolo` flag for faster workflows
- 🖱️ **Mouse Support** - Click to focus panes, scroll wheel navigation (toggle with Ctrl+M)
- 📋 **Text Selection** - Copy/paste from chat history (mouse disabled by default)
- 📜 **Help Modal** - Press `/help` for detailed command reference
- 🔌 **MCP Support** - Model Context Protocol server integration

## Screenshot

```
  _____   _____ _      _____ ___    _____  _____ _____ 
 |  __ \ / ____| |    |_   _|__ \  |  __ \|_   _/ ____|
 | |__) | |    | |      | |    ) | | |__) | | || |  __ 
 |  ___/| |    | |      | |   / /  |  _  /  | || | |_ |
 | |    | |____| |____ _| |_ / /_  | | \ \ _| || |__| |
 |_|     \_____|______|_____|____| |_|  \_\_____\_____|

┌─ Chat History [5] ─────────────────────────────────┐
│ 👤 You: Hello, can you help me with my code?       │
│                                                     │
│ 🤖 Assistant: Of course! I'd be happy to help.     │
│    What language are you working with?              │
│                                                     │
└─────────────────────────────────────────────────────┘

┌─ Input │ qwen2.5-coder:3b │ 🔌0 ───────────────────┐
│ I'm working on a Rust project... █                 │
└─────────────────────────────────────────────────────┘

┌─ Logs ─────────────────────────────────────────────┐
│ ✓ INFO Starting PCLI2-RIG with model: ...          │
│ ⋯ DEBUG Sending request to Ollama model: ...       │
│ ✓ Ready                                            │
└─────────────────────────────────────────────────────┘

 ✓ Ready │ Model: qwen2.5-coder:3b 
```

## Installation

### Prerequisites

1. **Install Ollama**: Download from [ollama.com](https://ollama.com)

2. **Pull a model** (recommended: Qwen2.5 Coder):
   ```bash
   ollama pull qwen2.5-coder:3b
   ```

   Other lightweight models with tool calling support:
   - `llama3.2:3b` - Good general reasoning
   - `phi3.5:mini` - Fast responses
   - `mistral:7b-instruct-q3_K_M` - Best quality/size ratio

### Build from Source

```bash
# Clone the repository
git clone https://github.com/physna/pcli2-rig.git
cd pcli2-rig

# Build
cargo build --release

# Run
./target/release/pcli2-rig
```

## Usage

### Basic Usage

```bash
# Run with default model (qwen2.5-coder:3b)
pcli2-rig

# Specify a model
pcli2-rig --model llama3.2:3b

# Connect to remote Ollama server
pcli2-rig --host http://192.168.1.100:11434

# YOLO mode (skip tool confirmation)
pcli2-rig --yolo

# Verbose logging
pcli2-rig --verbose
```

### MCP Integration

PCLI2-RIG integrates with [pcli2-mcp](https://github.com/jchultarsky101/pcli2-mcp) to provide access to external tools and services via the Model Context Protocol (MCP).

#### Quick Setup (Recommended)

The easiest way to configure MCP is using the `--setup-mcp` option, which reads the pcli2-mcp configuration and saves it permanently:

```bash
# One-time setup: pipe pcli2-mcp config directly
pcli2-mcp config | pcli2-rig --setup-mcp -
```

This will:
1. Parse the pcli2-mcp JSON configuration
2. Extract MCP server URLs from the config
3. Save them to `~/.config/pcli2-rig/config.toml` (or platform-specific config directory)
4. Display confirmation with configured servers

After setup, just run `pcli2-rig` normally—your MCP servers will be loaded automatically.

#### Example pcli2-mcp Config Format

The pcli2-mcp config JSON looks like this:

```json
{
  "mcpServers": {
    "pcli2": {
      "args": [
        "-y",
        "mcp-remote",
        "http://localhost:8080/mcp"
      ],
      "command": "npx"
    },
    "filesystem": {
      "args": [
        "-y",
        "mcp-remote",
        "http://localhost:8081/mcp"
      ],
      "command": "npx"
    }
  }
}
```

PCLI2-RIG extracts the HTTP/HTTPS URLs from the `args` array and configures them as MCP servers.

#### Alternative: Manual Server Setup

If you prefer to run the MCP server manually:

1. **Build and start pcli2-mcp**:
   ```bash
   # Build pcli2-mcp
   cd /path/to/pcli2-mcp
   cargo build --release

   # Start the server (default port 8080)
   ./target/release/pcli2-mcp serve --port 8080
   ```

2. **Connect pcli2-rig to the MCP server**:
   ```bash
   # Add MCP server URL directly (session-only)
   pcli2-rig --mcp-remote http://localhost:8080/mcp

   # Multiple MCP servers
   pcli2-rig --mcp-remote http://localhost:8080/mcp --mcp-remote http://localhost:8081/mcp
   ```

#### Temporary Session (No Config File)

For one-off sessions without saving configuration:

```bash
# Load MCP servers from pcli2-mcp config (pipe from stdin)
pcli2-mcp config | pcli2-rig --mcp-config -

# Combine with additional direct URLs
pcli2-mcp config | pcli2-rig --mcp-config - --mcp-remote http://localhost:9000/mcp
```

#### Manual Config File Editing

You can also edit the config file directly at `~/.config/pcli2-rig/config.toml`:

```toml
model = "qwen2.5-coder:3b"
host = "http://localhost:11434"
yolo = false

[[mcp_servers]]
name = "pcli2"
url = "http://localhost:8080/mcp"
enabled = true

[[mcp_servers]]
name = "filesystem"
url = "http://localhost:8081/mcp"
enabled = true
```

#### Verifying MCP Configuration

Once configured, you can verify MCP servers are loaded:

```bash
# Inside pcli2-rig, use the /mcp command
/mcp list      # List configured MCP servers
/mcp tools     # Show available MCP tools
```

### Environment Variables

```bash
# Set default model
export OLLAMA_MODEL=qwen2.5-coder:3b

# Set default host
export OLLAMA_HOST=http://localhost:11434
```

### Keyboard Shortcuts

#### Global

| Key | Action |
|-----|--------|
| `Ctrl+C` | Quit application |
| `Ctrl+K` | Clear chat history |
| `Ctrl+M` | Toggle mouse mode (enable/disable text selection) |
| `Tab` | Switch focus between panes |
| `Shift+Tab` | Switch focus backwards |
| `Esc` | Close modal dialogs |

#### Input Pane (when focused)

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `↑/↓` | Move cursor in input |
| `Home/End` | Jump to start/end of input |
| `Backspace` | Delete character before cursor |
| `Delete` | Delete character at cursor |

#### Chat/Logs Panes (when focused)

| Key | Action |
|-----|--------|
| `↑/↓` | Scroll 1 line |
| `PageUp/PageDown` | Scroll 5 lines |

#### Mouse Controls

| Action | Effect |
|--------|--------|
| Left Click | Focus on clicked pane |
| Scroll Wheel | Scroll in focused pane (3 lines) |

#### Tool Confirmation

| Key | Action |
|-----|--------|
| `Y` or `Enter` | Confirm tool execution |
| `N` or `Esc` | Cancel tool execution |

## Built-in Tools

The agent has access to these tools:

| Tool | Description |
|------|-------------|
| `read_file` | Read contents of a file |
| `write_file` | Write/create a file |
| `list_directory` | List directory contents |
| `run_command` | Execute shell commands |
| `search_code` | Search code with grep |

### Tool Confirmation

By default, tool execution requires confirmation. You'll see a dialog like:

```
┌─ Confirmation Required ──────────────────────────┐
│                                                   │
│ 🔧 Tool Execution Requested                      │
│                                                   │
│ Tool: read_file                                  │
│ Arguments: {"path": "Cargo.toml"}                │
│                                                   │
│ Execute this tool? (Y/n)                         │
│                                                   │
└───────────────────────────────────────────────────┘
```

Use `--yolo` mode to skip confirmation for faster workflows.

## Architecture

```
┌─────────────────────────────────────────────────┐
│              PCLI2-RIG Architecture              │
├─────────────────────────────────────────────────┤
│  ┌───────────────────────────────────────────┐  │
│  │         TUI Layer (ratatui)               │  │
│  │  ┌─────────┐ ┌─────────┐ ┌────────────┐  │  │
│  │  │ Banner  │ │  Chat   │ │   Status   │  │  │
│  │  └─────────┘ └─────────┘ └────────────┘  │  │
│  └───────────────────────────────────────────┘  │
│                      │                          │
│                      ▼                          │
│  ┌───────────────────────────────────────────┐  │
│  │         Agent Core (Rig)                  │  │
│  │  ┌─────────┐ ┌─────────┐ ┌────────────┐  │  │
│  │  │ Ollama  │ │  Tool   │ │  Message   │  │  │
│  │  │ Client  │ │ Registry│ │  History   │  │  │
│  │  └─────────┘ └─────────┘ └────────────┘  │  │
│  └───────────────────────────────────────────┘  │
│                      │                          │
│                      ▼                          │
│  ┌───────────────────────────────────────────┐  │
│  │    Ollama (localhost:11434)               │  │
│  │         qwen2.5-coder:3b                  │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

## Configuration

### Config File

Create `~/.config/pcli2-rig/config.toml`:

```toml
model = "qwen2.5-coder:3b"
host = "http://localhost:11434"
yolo = false

# MCP Server Configuration (optional)
[[mcp_servers]]
name = "filesystem"
url = "http://localhost:3000"
enabled = true

[[mcp_servers]]
name = "github"
url = "http://localhost:3001"
token = "ghp_..."  # Optional auth
enabled = false
```

### MCP Commands

| Command | Description |
|---------|-------------|
| `/mcp` | Show MCP server status |
| `/mcp list` | List configured MCP servers |
| `/mcp tools` | Show available MCP tools |

### Cargo.toml Dependencies

The project uses these key dependencies:

- `rig-core` - AI agent framework (with `rmcp` feature)
- `rmcp` - Model Context Protocol client
- `ratatui` - TUI framework
- `crossterm` - Terminal backend
- `tokio` - Async runtime
- `clap` - CLI argument parsing
- `tracing` - Logging
- `tui-markdown` - Markdown rendering

### Model Selection

For best results on laptops with limited resources:

| RAM | Recommended Model | Quality |
|-----|-------------------|---------|
| 4GB | `qwen2.5-coder:1.5b` | Good |
| 8GB | `qwen2.5-coder:3b` | Better |
| 16GB | `qwen2.5-coder:7b` | Best |

## Troubleshooting

### "Failed to create Ollama client"

Make sure Ollama is running:
```bash
ollama serve
```

### "Model not found"

Pull the model first:
```bash
ollama pull qwen2.5-coder:3b
```

### TUI rendering issues

Try resizing your terminal or check that your terminal supports UTF-8.

## Commands

| Command | Description |
|---------|-------------|
| `/help`, `/h`, `/?` | Show detailed help modal |
| `/quit`, `/exit`, `/q` | Exit the application |
| `/clear`, `/cls` | Clear chat history |
| `/model [name]` | Show or set the current model |
| `/history`, `/hist` | Show message count |
| `/status` | Show current status |
| `/mcp` | Show MCP server status |
| `/mcp list` | List configured MCP servers |
| `/mcp tools` | Show available MCP tools |

## CLI Options

| Option | Description |
|--------|-------------|
| `--model <MODEL>` | Set the Ollama model to use |
| `--host <HOST>` | Set the Ollama server host (default: `http://localhost:11434`) |
| `--setup-mcp <FILE>` | **One-time setup:** Load MCP servers from pcli2-mcp config and save to config file |
| `--mcp-config <PATH>` | Load MCP servers from config file for this session only (use `-` for stdin) |
| `--mcp-remote <URL>` | Add an MCP server URL directly (can be used multiple times) |
| `--yolo` | Skip tool confirmation prompts |
| `--verbose` | Enable verbose logging |
| `--help`, `-h` | Show CLI help |

## Development

```bash
# Run in development mode
cargo run

# Run with verbose logging
RUST_LOG=debug cargo run -- --verbose

# Build release
cargo build --release

# Run tests
cargo test
```

## License

MIT License - see LICENSE file for details.

## Acknowledgments

- [Rig](https://github.com/0xPlaygrounds/rig) - Excellent Rust AI framework
- [Ollama](https://ollama.com/) - Local LLM runner
- [ratatui](https://ratatui.rs/) - Modern TUI library
- [tui-markdown](https://github.com/joshka/tui-markdown) - Markdown rendering for TUI
