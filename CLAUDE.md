# rust-code

Extensible agentic CLI tool for testing open source LLM capabilities.

## Architecture

```
src/
├── main.rs          # Entry point, tokio runtime
├── cli.rs           # clap argument parsing
├── config.rs        # Runtime configuration
├── agent.rs         # Agent loop (generic over LlmClient)
├── llm/
│   ├── mod.rs       # LlmClient trait, Message/ToolCall/ToolDefinition types
│   └── ollama.rs    # Ollama API implementation
└── tools/
    ├── mod.rs       # Tool trait, ToolResult, ToolError
    ├── bash.rs      # Shell execution with timeout
    ├── file.rs      # read, write, edit operations
    └── search.rs    # glob, grep operations
```

## Commands

```bash
cargo build          # Build
cargo test           # Run tests
cargo clippy         # Lint - fix all warnings before committing
cargo fmt            # Format - run before committing
```

## Lint Rules

Clippy pedantic is enabled. Handle errors properly:
- `unsafe_code` is forbidden
- `unwrap_used`, `expect_used`, `panic`, `todo` emit warnings - use `?` or proper error handling

## Tool Return Values

Return `Result<ToolResult, ToolError>` from tool functions:
- `ToolResult::success(output)` - tool succeeded
- `ToolResult::failure(msg)` - tool-level failure (file not found, invalid args)
- `ToolError` - system-level errors only (IO failures that bubble up)

## Adding a New LLM Backend

1. Create `src/llm/newbackend.rs`
2. Implement `LlmClient` trait with `#[async_trait]`
3. Add `pub mod newbackend;` to `src/llm/mod.rs`
4. Add constructor and conversion tests

## Adding a New Tool

1. Add async function to `src/tools/*.rs` (new file requires `pub mod` in `mod.rs`)
2. Return `Result<ToolResult, ToolError>`
3. Add tests for success, failure, and edge cases
