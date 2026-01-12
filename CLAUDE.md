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
# Commit Requirements, Linting, and Formatting. 

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

DO NOT, under any circumstance, change this config or add allow macros without explicit approval from the user. 

# Style Guidelines

## Naming
- **Domain-specific names**: Prefer descriptive names that match the domain (`send_chat_completion` over generic `run`)
- **Common abbreviations OK**: `cfg`, `dir`, `msg`, `ctx`, `cmd` are fine; avoid obscure ones

## Error Messages
- Always include context: `"failed to parse config at {path}"` not just `"parse error"`
- Lowercase, no trailing period (Unix style, chains well with `anyhow` context)

## Comments
- Explain **why**, never **what** — the code shows what, comments explain non-obvious reasoning
- Doc comments: one-line `///` summary for public items; expand only for complex behavior

## Module Organization
- Group related types in one file (e.g., `Message`, `Role`, `ToolCall` together in `llm/mod.rs`)
- Tests: unit tests in `#[cfg(test)] mod tests` at file bottom; integration tests in `tests/`

## Visibility
- Private-first: start with no visibility modifier, add `pub(crate)` or `pub` only when needed
- Treat `pub` as a commitment — once public, it's API

## Function Signatures
- **Strings**: `&str` for read-only, `impl Into<String>` when storing, owned `String` when caller must give up ownership
- **Async**: async-first; only use sync for trivial or CPU-bound operations
- **Generics**: default to concrete types, generify at public API boundaries when flexibility is needed

## Construction
- Prefer `new()` with required args + `Default` trait for optional configuration
- Avoid builder pattern unless struct has many optional fields

## Logging (tracing)
- **error**: failures that stop an operation
- **warn**: recoverable issues, degraded behavior
- **info**: major operations (agent loop start/end, tool execution)
- **debug**: internal details, state transitions
- **trace**: verbose diagnostics (full payloads, timing)
- Use structured fields: `info!(tool = %name, "executing tool")` not string interpolation