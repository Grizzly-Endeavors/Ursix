# rust-code

An extensible agentic CLI tool for testing open source LLM capabilities.

## Overview

rust-code provides an agent framework that connects to local LLM backends and executes tools based on model responses. It's designed for experimentation and benchmarking of open source language models in agentic workflows.

### Features

- **Pluggable LLM backends** — Abstract trait-based design allows swapping inference providers
- **Extensible tool system** — Add custom tools by implementing a simple trait
- **Built-in tools** — Shell execution, file operations, and search utilities included
- **Async-first** — Built on Tokio for efficient concurrent operations
- **Structured logging** — Tracing-based observability with configurable verbosity

## Requirements

- Rust 1.85+ (2024 edition)
- A compatible LLM backend (currently Ollama)

## Getting Started

### Setup

After cloning, install the git hooks:

```bash
./.githooks/install.sh
```

### Build

```bash
cargo build --release
```

### Run

```bash
cargo run -- --help
```

### Test

```bash
cargo test
```

## Architecture

The project follows a modular design:

- **CLI** — Argument parsing and configuration
- **Agent** — Core loop orchestrating LLM interactions and tool execution
- **LLM** — Trait definitions and backend implementations
- **Tools** — Executable capabilities exposed to the model

The agent is generic over the LLM client, allowing different backends to be used interchangeably.

## Development

### Code Quality

The project enforces strict quality gates via pre-commit hooks:

- Format checking (`cargo fmt`)
- Linting with pedantic Clippy rules
- Full test suite

Unsafe code is forbidden. Error handling shortcuts (`unwrap`, `expect`, `panic`) are denied in production code.

### Adding Tools

Tools implement a common trait that defines:
- Name and description for the model
- Parameter schema
- Execution logic

See the existing tool implementations for reference.

### Adding LLM Backends

LLM backends implement the client trait, providing:
- Chat completion requests
- Tool definition formatting
- Response parsing

## License

MIT
