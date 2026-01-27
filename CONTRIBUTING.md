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

Clippy pedantic is enabled with strict error handling:

```toml
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
unimplemented = "deny"
unsafe_code = "forbid"
```

Test modules may use `#[allow(clippy::unwrap_used)]` for readability.

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
