# Ursix

Unix utilities powered by LLMs. Stateless, composable, automation-first.

## Architecture

The CLI uses a stateless pipeline model: single-pass LLM calls with pre-gathered context.

```
src/
├── main.rs              # Entry point, tokio runtime
├── cli.rs               # clap argument parsing, command dispatch
├── config.rs            # Runtime configuration (layered: files, env, CLI)
├── input.rs             # Input source handling (--from, stdin)
├── context.rs           # Pre-LLM context gathering (files, git state)
├── pipeline.rs          # Stateless single-pass executor
├── chunk.rs             # Token-aware parallel chunking
├── tokens.rs            # Token counting (heuristic and full modes)
├── parsers.rs           # Response parsing utilities
├── prompts.rs           # System prompts for each command
├── commands/
│   ├── mod.rs
│   ├── explain.rs
│   ├── review.rs
│   ├── fix.rs
│   ├── commit.rs
│   └── config.rs
├── output/
│   ├── mod.rs           # Exit codes, output modes, traits
│   ├── explain.rs
│   ├── review.rs
│   ├── fix.rs
│   ├── commit.rs
│   └── config.rs
├── rules/
│   ├── mod.rs           # Rule types and resolution
│   ├── defaults.rs      # Built-in default rules
│   ├── loader.rs        # YAML loading
│   └── resolver.rs      # Category-based filtering
└── llm/
    ├── mod.rs           # LlmClient trait, Message/ToolCall types
    ├── ollama.rs        # Ollama API implementation
    └── openai.rs        # OpenAI-compatible API implementation
```

## Usage

Single LLM call with pre-gathered context. No tools, no message history.
```bash
usx explain src/main.rs           # Gather file, single LLM call
usx commit                        # Gather staged diff, generate message
usx review --checks style,security
usx fix src/main.rs --lint        # Fix clippy issues
```

## Key CLI Flags

| Flag | Description |
|------|-------------|
| `--from FILE` | Read context from file (use `-` for stdin) |
| `--execute` | Auto-execute git commit (commit command) |
| `--checks LIST` | Comma-separated checks to focus on (review command) |
| `--text` | Output as plain text instead of JSON (default: JSON) |
| `--chunk` | Enable chunked processing for large inputs |
| `--max-concurrency N` | Parallel chunk limit (default: 4) |
| `--tokenizer MODE` | Token counting: heuristic (fast) or full (accurate) |

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
- **info**: major operations (LLM calls, chunked processing)
- **debug**: internal details, state transitions
- **trace**: verbose diagnostics (full payloads, timing)
- Use structured fields: `info!(chunks = count, "starting chunked review")` not string interpolation

# Misc Notes
- Testing is a first class operation, NEVER skip test implementation.
- Commits should be made frequently, especially for large multi-phase tasks.
- All changes must be pushed before giving the user a completion summary.
