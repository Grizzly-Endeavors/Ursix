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

## Git Hooks

Pre-commit hooks enforce quality gates:
- **pre-commit**: `cargo fmt --check`, `cargo clippy`, `cargo test`
- **commit-msg**: validates message format
- **pre-push**: full test suite

Bypass is **FORBIDDEN**.

## Lint Rules

Clippy pedantic is enabled with strict error handling:
- `unsafe_code` - forbidden
- `unwrap_used`, `expect_used`, `panic`, `todo`, `unimplemented` - **denied**
- `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate` - warnings

Test modules have `#[allow(clippy::unwrap_used)]` for readability.

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
