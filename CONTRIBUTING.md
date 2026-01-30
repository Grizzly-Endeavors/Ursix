# Contributing to Ursix

## Development Setup

### Prerequisites

- Rust 1.85+ (2024 edition)
- An LLM provider for testing:
  - [Ollama](https://ollama.ai/) (recommended for local development)
  - Any OpenAI-compatible API

### Getting Started

```bash
git clone https://github.com/grizzly-endeavors/ursix.git
cd ursix
cargo build
cargo test
```

### Git Hooks

The repository includes pre-commit hooks that enforce quality gates:

- **pre-commit**: `cargo fmt --check`, `cargo clippy`, `cargo test`
- **commit-msg**: validates conventional commit format
- **pre-push**: full test suite

These hooks are mandatory. Do not bypass them.

## Code Style

### Linting

The lint rules in `Cargo.toml` are intentionally aggressive. This project is primarily developed and maintained by AI coding agents, which will take every shortcut that isn't a hard error. Most rules are standard Rust best practices with the severity turned up — if you write idiomatic Rust, you'll rarely hit them.

**Key rules and how to work with them:**

| Rule | What it means for you |
|---|---|
| `unwrap_used`, `expect_used`, `panic` = deny | Use `?`, `ok_or()`, `ok_or_else()`, or pattern matching. Never panic in production code. |
| `indexing_slicing`, `string_slice` = deny | Use `.get()` with proper error handling instead of `vec[i]` or `&s[0..4]`. |
| `get_unwrap` = deny | `.get(i).unwrap()` is still an unwrap. Use `.get(i).ok_or()?` or match. |
| `exit` = deny | Only `main()` calls `process::exit`. Return errors up the call stack instead. |
| `dbg_macro`, `todo`, `unimplemented` = deny | No debug or placeholder code in commits. |
| `dead_code`, `unreachable_pub` = deny | Remove unused code. Use `pub(crate)` instead of `pub` for internal items. |
| `wildcard_enum_match_arm` = deny | Match all enum variants explicitly — no `_ =>` catch-alls on enums. |
| `allow_attributes` = deny | Use `#[expect(lint, reason = "...")]` instead of `#[allow(lint)]`. This ensures stale suppressions get flagged automatically. |
| `missing_errors_doc`, `missing_panics_doc` = deny | Document error conditions and panic behavior on public functions. |

**Test modules** use `#[expect(clippy::unwrap_used, reason = "...")]` for readability — `unwrap()` is fine in tests, just suppress it explicitly.

If a lint feels wrong for your use case, suppress it locally with `#[expect]` and a reason — don't fight the linter globally. If you think a rule should be changed project-wide, open an issue.

### Error Handling

- **No silent failures**: Every failure must be visible to the user
- **Include context**: `"failed to parse config at {path}"` not `"parse error"`
- **Lowercase, no trailing period**: Unix style, chains well with `anyhow::Context`

### Output Guarantees

- Output is **always** valid JSON (success or failure)
- Exit 0 = complete result, exit non-zero = valid error JSON
- Scripts must be able to parse output reliably with `jq`

### Comments

- Explain **why**, not **what**
- Doc comments: one-line `///` summary for public items

### Naming

- Prefer domain-specific names (`send_chat_completion` over `run`)
- Common abbreviations are fine: `cfg`, `dir`, `msg`, `ctx`, `cmd`

## Testing

Testing is a first-class concern. Every change should include tests.

- **Unit tests**: `#[cfg(test)] mod tests` at the bottom of each file
- **Integration tests**: `tests/` directory
- Test modules may use `unwrap()` for readability

Run tests:

```bash
cargo test              # All tests
cargo test --lib        # Unit tests only
cargo test --test '*'   # Integration tests only
```

## Pull Request Process

1. Fork the repository and create a feature branch
2. Make your changes with tests
3. Ensure all checks pass: `cargo fmt && cargo clippy && cargo test`
4. Write a clear commit message following [Conventional Commits](https://www.conventionalcommits.org/):
   - `feat:` new features
   - `fix:` bug fixes
   - `docs:` documentation
   - `refactor:` code changes that don't add features or fix bugs
   - `test:` adding or updating tests
   - `chore:` maintenance tasks
5. Open a PR with a clear description of the changes

## Architecture Overview

```
src/
├── main.rs              # Entry point
├── cli/                 # Argument parsing, command dispatch
├── config/              # Layered configuration (files, env, CLI)
├── commands/            # Command implementations
├── output/              # Result types, JSON/text rendering
├── llm/                 # LLM client trait and implementations
├── rules/               # Semantic linting rules
├── pipeline.rs          # Single-pass LLM execution
├── chunk.rs             # Token-aware chunking
└── tokens.rs            # Token counting
```

Key principles:

- **Stateless**: Single-pass LLM calls, no conversation history
- **Composable**: Pipe-friendly, works with Unix tools
- **Explicit**: No magic, no implicit behavior

See [PHILOSOPHY.md](PHILOSOPHY.md) for design rationale.

## Questions?

Open an issue for questions about contributing. We're happy to help.
