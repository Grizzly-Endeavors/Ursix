# Ursix

Unix utilities powered by LLMs. Stateless, composable, automation-first.

## Architecture

The CLI uses a stateless pipeline model: single-pass LLM calls with piped input.

```
src/
├── main.rs              # Entry point, tokio runtime
├── cli.rs               # clap argument parsing, command dispatch
├── config.rs            # Runtime configuration (layered: files, env, CLI)
├── input.rs             # Input source handling (--from, stdin)
├── context.rs           # Input context wrapper for LLM calls
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

Single LLM call with piped input. No tools, no message history.
```bash
cat src/main.rs | usx explain           # Explain code
git diff --staged | usx review          # Review staged changes
git diff --staged | usx commit          # Generate commit message
cat src/main.rs | usx fix               # Suggest fixes
```

## Key CLI Flags

| Flag | Description |
|------|-------------|
| `--from FILE` | Read input from file (use `-` for stdin) |
| `--execute` | Auto-execute git commit (commit command) |
| `--checks LIST` | Comma-separated checks to focus on (review command) |
| `--text` | Output as plain text instead of JSON (default: JSON) |
| `--dry-run` | Show token estimation without making LLM calls |
| `--timeout N` | Timeout for LLM requests in seconds (default: 60) |
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

## No Silent Failures

**Every failure must be visible to the user.** This is non-negotiable.

- If an operation fails, it must either return an error or print a warning to stderr
- Debug/trace logging is NOT sufficient - users don't run with `RUST_LOG=debug` by default
- Partial failures (e.g., reading 2 of 3 files) must be reported, not silently ignored
- Empty input that causes unexpected behavior must warn the user
- "Graceful degradation" that hides errors is not acceptable - fail explicitly instead

## Structural Output Guarantee

**Output is ALWAYS valid JSON matching a documented schema** - whether success or failure.

- Scripts must be able to parse output reliably with `jq` or similar
- Exit 0 = complete, valid result JSON
- Exit non-zero = valid error JSON (never malformed output)
- Partial failures return error JSON with `partial_results` field for recovery
- Never return unparseable or structurally inconsistent output

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

# Agent Usage

When spawning sub-agents for parallel or delegated work, always include these instructions in the agent prompt:

> **Do NOT run tests, linting, or formatting checks.** Do NOT attempt to commit changes. Focus only on implementing the requested changes. Verification (tests, clippy, fmt) will be run after all agents complete.

This prevents agents from:
- Wasting cycles on verification that will be done centrally
- Creating conflicting commits from parallel work
- Blocking on test failures that may depend on other agents' changes

The orchestrating agent is responsible for running `cargo fmt`, `cargo clippy`, and `cargo test` after all sub-agent work is complete, then creating a single commit.

# Misc Notes
- Testing is a first class operation, NEVER skip test implementation.
- Commits should be made frequently, especially for large multi-phase tasks.
- All changes must be pushed before giving the user a completion summary.
