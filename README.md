# Ursus.rs

An extensible command-based CLI for LLM-powered development tasks.

## Overview

Ursus.rs (`ur`) provides focused commands for common development tasks, powered by local LLMs. Unlike interactive chat interfaces, each command is optimized for a specific workflow with tailored prompts and tool access.

### Commands

| Command | Description |
|---------|-------------|
| `ur ask <prompt>` | General-purpose LLM query with full tool access |
| `ur explain <file>` | Explain code, files, or concepts |
| `ur review [files]` | Review code changes (staged, specific files, or diffs) |
| `ur fix <target>` | Fix issues in code (lint errors, bugs) |
| `ur commit` | Generate commit message from staged changes |
| `ur config` | Manage configuration |
| `ur models` | List available Ollama models |

### Features

- **Command-focused** — Each command has an optimized system prompt
- **Automation-friendly** — `--json` flag for structured output in scripts
- **Layered configuration** — CLI flags, env vars, project config, global config
- **Pluggable LLM backends** — Currently supports Ollama
- **Built-in tools** — Shell execution, file operations, search utilities

## Requirements

- Rust 1.85+ (2024 edition)
- [Ollama](https://ollama.ai/) running locally (or remote with `--ollama-url`)

## Installation

```bash
cargo install --path .
```

Or build from source:

```bash
cargo build --release
# Binary at target/release/ur
```

## Quick Start

```bash
# Ask a question
ur ask "What files are in this project?"

# Explain code
ur explain src/main.rs

# Review staged changes
ur review

# Generate a commit message
ur commit

# List available models
ur models
```

## Configuration

Configuration is loaded from multiple sources (highest priority first):

1. **CLI flags** — `--model`, `--ollama-url`, `--max-turns`
2. **Environment variables** — `URSUS_MODEL`, `URSUS_OLLAMA_URL`, `URSUS_MAX_TURNS`
3. **Project config** — `.ursus.toml` in current or parent directories
4. **Global config** — `~/.config/ursus/config.toml`

### Example `.ursus.toml`

```toml
model = "qwen2.5-coder:7b"
ollama_url = "http://localhost:11434"
max_turns = 50
```

### Global Flags

```
--json              Output as JSON for scripting
-m, --model         Model to use (default: llama3.2)
--ollama-url        Ollama API base URL (default: http://localhost:11434)
--max-turns         Maximum agent turns (default: 50)
-v, --verbose       Enable verbose output
```

## Architecture

```
src/
├── main.rs      # Entry point
├── cli.rs       # Clap subcommands and dispatch
├── config.rs    # Layered configuration loading
├── output.rs    # Human/JSON output formatting
├── prompts.rs   # Command-specific system prompts
├── agent.rs     # Core agent loop
├── llm/         # LLM client trait and implementations
└── tools/       # Tool definitions and execution
```

### Agent Loop

The agent orchestrates LLM calls and tool execution:

1. Send user message with system prompt and tool definitions
2. Receive LLM response (content and/or tool calls)
3. Execute any tool calls, append results
4. Repeat until LLM responds without tool calls (completion)

### Available Tools

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands |
| `read` | Read file contents |
| `write` | Create or overwrite files |
| `edit` | Make precise edits to existing files |
| `glob` | Find files matching patterns |
| `grep` | Search file contents with regex |

## Development

### Setup

```bash
git clone <repo>
cd rust-code
./.githooks/install.sh  # Install pre-commit hooks
```

### Code Quality

Pre-commit hooks enforce:
- Format checking (`cargo fmt`)
- Pedantic Clippy lints
- Full test suite

Unsafe code is forbidden. Error handling shortcuts (`unwrap`, `expect`, `panic`) are denied.

### Testing

```bash
cargo test
```

### Adding Tools

Implement the tool trait with:
- Name and description for the model
- JSON schema for parameters
- Async execution logic

### Adding LLM Backends

Implement `LlmClient` trait:
- `chat()` — Send messages, receive response with optional tool calls
- `model_name()` — Return the model identifier

## License

MIT
