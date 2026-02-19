# PCLI2-RIG

A beautiful TUI-based local AI agent powered by [Rig](https://github.com/0xPlaygrounds/rig) and [Ollama](https://ollama.com/).

## Features

- 🎨 **Beautiful Dark TUI** - Inspired by Qwen Code with gradient banner and emoji-rich interface
- 🤖 **Local AI** - Runs entirely on your laptop using Ollama
- 🛠️ **Tool Support** - Built-in tools for file operations, command execution, and code search
- 🔒 **Privacy First** - All inference happens locally, no data leaves your machine
- ⚡ **Lightweight** - Optimized for quick startup and minimal resource usage
- 🎯 **YOLO Mode** - Skip tool confirmation with `--yolo` flag for faster workflows

## Screenshot

```
╔══════════════════════════════════════════════════════╗
║  🤖  PCLI2-RIG  ·  Local AI Agent                    ║
╚══════════════════════════════════════════════════════╝

┌─ Chat History ─────────────────────────────────────┐
│ 👤 You: Hello, can you help me with my code?       │
│                                                     │
│ 🤖 Assistant: Of course! I'd be happy to help.     │
│    What language are you working with?              │
│                                                     │
└─────────────────────────────────────────────────────┘

┌─ Input ────────────────────────────────────────────┐
│ I'm working on a Rust project...                   │
└─────────────────────────────────────────────────────┘

 Ready │ Model: qwen2.5-coder:3b │ Messages: 2
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

### Environment Variables

```bash
# Set default model
export OLLAMA_MODEL=qwen2.5-coder:3b

# Set default host
export OLLAMA_HOST=http://localhost:11434
```

### Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `Enter` | Send message |
| `Ctrl+C` | Quit |
| `Ctrl+K` | Clear chat history |
| `↑/↓` | Scroll through history |
| `PageUp/PageDown` | Scroll faster |
| `Y/n` | Confirm/cancel tool execution |
| `Esc` | Cancel tool execution |

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

### Cargo.toml Dependencies

The project uses these key dependencies:

- `rig-core` - AI agent framework
- `ratatui` - TUI framework
- `crossterm` - Terminal backend
- `tokio` - Async runtime
- `clap` - CLI argument parsing
- `tracing` - Logging

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
- [tui-banner](https://github.com/coolbeevip/tui-banner) - Beautiful ANSI banners
