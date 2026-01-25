<div align="center">

# Ursix

**LLM as a boring Unix utility**

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![Ollama](https://img.shields.io/badge/ollama-compatible-blue.svg)](https://ollama.ai/)
[![OpenAI](https://img.shields.io/badge/openai--compatible-API-green.svg)](https://platform.openai.com/)

[Getting Started](#getting-started) •
[Commands](#commands) •
[Scripting & CI/CD](#scripting--cicd) •
[Review Rules](#review-rules) •
[Contributing](#contributing)

</div>

---

## What is Ursix?

Ursix (`usx`) is a set of Unix utilities that happen to use an LLM under the hood. It's not a chat interface. It's not an AI assistant. It's `grep`, `sed`, and `lint`—if they could understand intent.

```bash
# These are commands, not conversations
usx review                        # Like `lint`, but for code changes
usx commit --execute              # Like `git commit`, but writes the message for you
usx explain src/auth/jwt.rs       # Like `man`, but for your actual code
usx fix src/lib.rs --lint --apply # Like `cargo fix`, but smarter
```

Ursix commands read from stdin, write to stdout, return meaningful exit codes, and output JSON by default. They compose with `jq`, `xargs`, `find`, and everything else in your toolkit. No chat history. No memory. No magic—just predictable, scriptable tools.

## Why Ursix?

### The Problem with AI Coding Tools

Most AI coding tools are chat interfaces bolted onto an LLM:
- **Interactive-first**: designed for back-and-forth conversation, not scripts
- **Unpredictable output**: prose mixed with code, formatting varies by mood
- **Not composable**: can't pipe to `jq`, can't use in CI, can't automate
- **Session-dependent**: relies on chat history and context that doesn't persist
- **Black box behavior**: unclear what the model sees or why it responds as it does

### The Unix Way

Ursix takes a different approach:

| Chat Wrapper | Ursix |
|--------------|-------|
| "Can you review my code?" | `usx review` |
| Copy-paste diff into chat | Reads git diff automatically |
| Parse prose response manually | JSON output, structured fields |
| Hope it remembers context | Stateless—same input, same output |
| Can't automate | Exit codes, stdin/stdout, CI-native |

**Design principles:**

- **Stateless by default** — Each command is a pure function: input → LLM → output. No session, no memory, no surprises.
- **JSON-first** — Machine-readable output by default. Use `--text` when you want human-readable.
- **Meaningful exit codes** — 5 distinct codes for reliable automation (success, issues found, user error, transient error, permanent error).
- **Offline-capable** — First-class Ollama support. Your code never leaves your machine.
- **Predictable** — Pipeline mode (default) makes a single LLM call with pre-gathered context. No tool loops, no iteration, no runaway agents.
- **Composable** — Works with `jq`, `xargs`, `find`, `parallel`, `watch`, and the rest of your Unix toolkit.

## Getting Started

### Prerequisites

- **Rust 1.85+** (2024 edition)
- **An LLM provider** — choose one:
  - [Ollama](https://ollama.ai/) for local models (default)
  - Any OpenAI-compatible API endpoint

### Installation

```bash
git clone https://github.com/your-org/ursix.git
cd ursix
cargo install --path .
```

Verify:

```bash
usx --version
usx config --list
```

### Quick Start

```bash
# Start Ollama if using local models
ollama serve

# Explain a file
usx explain src/main.rs --text

# Review staged changes
git add -p
usx review --text

# Generate and apply a commit message
usx commit --execute
```

## Commands

### `usx explain` — Code Explanation

Reads a file and explains what it does.

```bash
usx explain src/auth/middleware.rs          # Explain a file
usx explain src/database/schema.rs          # Explain another file
```

### `usx review` — Code Review

Reviews code changes and outputs structured feedback.

```bash
usx review                                  # Review staged changes
usx review src/api.rs src/handlers.rs       # Review specific files
usx review --diff HEAD~3                    # Review a specific diff range
usx review --checks security,performance    # Focus on specific rule categories
```

**Output (JSON by default):**
```json
{
  "summary": "Found 2 issues",
  "issues": [
    {"severity": "error", "file": "src/auth.rs", "line": 42, "message": "...", "rule": "no-unwrap"}
  ],
  "passed": false
}
```

### `usx fix` — Code Fixes

Identifies and fixes issues in code.

```bash
usx fix src/lib.rs                          # Suggest fixes
usx fix src/main.rs --lint                  # Fix clippy/lint issues
usx fix src/lib.rs --lint --apply           # Apply fixes automatically
usx fix src/lib.rs --from review.json       # Fix issues from a previous review
```

### `usx commit` — Commit Messages

Generates conventional commit messages from staged changes.

```bash
usx commit                                  # Generate message (JSON)
usx commit --text                           # Human-readable output
usx commit --body                           # Include detailed body
usx commit --execute                        # Generate and run `git commit`
usx commit --style simple                   # Non-conventional format
```

### `usx config` — Configuration

View current configuration.

```bash
usx config --list                           # Show all settings
usx config model                            # Show specific value
```

## Execution Mode

Single LLM call with pre-gathered context. No tools, no iteration, deterministic.

```bash
usx review              # Gathers diff → single LLM call → JSON output
usx commit              # Gathers staged changes → generates message
usx explain src/lib.rs  # Reads file → explains in one pass
```

Designed for CI/CD, git hooks, scripts, and any automation.

## Scripting & CI/CD

Ursix is designed for automation first.

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success (command completed, no issues) |
| `1` | Issues found (review problems, partial failures) |
| `2` | User error (bad args, config, missing files) |
| `3` | Transient error (network, rate limit—retry may help) |
| `4` | Permanent error (auth, parse—retry won't help) |

```bash
usx review && echo "Clean" || echo "Issues found (exit: $?)"
```

### Retry & Partial Results

LLM calls automatically retry on transient failures:

```bash
usx review                    # Default: up to 3 retries
usx review --no-retry         # Fail immediately on errors
usx review --max-retries 5    # Custom retry count
```

When using chunked processing, get partial results even if some chunks fail:

```bash
usx review --chunk --partial  # Return successful results from processed chunks
```

### Token-Aware Chunking

Large codebases can exceed LLM context limits. Ursix handles this with parallel chunked processing:

```bash
usx review --chunk                          # Process files in parallel chunks
usx review --chunk --max-concurrency 8      # Control parallelism
```

Each chunk stays within token limits. Results are aggregated. Failures are tracked per-chunk.

### Composing with Unix Tools

```bash
# Review only changed files
git diff --name-only HEAD~1 | xargs usx review

# Batch process modules
find src -name "*.rs" -exec usx explain {} \; | jq -s '.'

# Filter review issues
usx review | jq '.issues[] | select(.severity == "error")'

# Count issues by rule
usx review | jq '[.issues[].rule] | group_by(.) | map({rule: .[0], count: length})'

# Chain review and fix
usx review > review.json
usx fix src/ --from review.json --apply
```

### GitHub Actions

```yaml
name: Code Review
on: [pull_request]

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Ursix
        run: cargo install --path .

      - name: Review Changes
        run: |
          usx review --diff origin/${{ github.base_ref }}...HEAD > review.json
          jq -r '.summary' review.json
          if [ $(jq '.passed' review.json) = "false" ]; then
            jq -r '.issues[] | "[\(.severity)] \(.file):\(.line // "?") - \(.message)"' review.json
            exit 1
          fi

      - name: Upload Review
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: code-review
          path: review.json
```

### Git Hooks

**prepare-commit-msg:**
```bash
#!/bin/bash
usx commit --text > "$1"
```

**pre-commit:**
```bash
#!/bin/bash
usx review --checks security > /tmp/review.json
if [ $(jq '.passed' /tmp/review.json) = "false" ]; then
  echo "Security issues found:"
  jq -r '.issues[] | "\(.file):\(.line) - \(.message)"' /tmp/review.json
  exit 1
fi
```

## Review Rules

Codify your team's standards in version-controlled YAML.

### Why Rules?

- **Consistent reviews** — Same checks every time, not dependent on reviewer mood
- **Documented standards** — New team members see expectations immediately
- **Auditable** — Changes to standards are PRs, not hallway conversations

### Creating Rules

Create `rules.yml` or `.ursix/rules.yml`:

```yaml
categories:
  security:
    - name: no-unwrap
      description: "Avoid .unwrap() outside of tests—use proper error handling"
      severity: error
      files: "src/**/*.rs"

    - name: no-hardcoded-secrets
      description: "Never hardcode API keys, passwords, or secrets"
      severity: error

  style:
    - name: doc-comments
      description: "Public functions and types must have doc comments"
      severity: warning
      files: "src/lib.rs"
```

### Using Rules

```bash
usx review                          # Apply all rules
usx review --checks security        # Only security rules
usx review --checks security,style  # Multiple categories
```

Rules output includes the rule name for filtering:
```bash
usx review | jq '.issues[] | select(.rule == "no-unwrap")'
```

## Configuration

Layered configuration (highest priority first):

1. **CLI flags** — `--model qwen2.5-coder:7b`
2. **Environment variables** — `URSIX_MODEL=qwen2.5-coder:7b`
3. **Project config** — `.ursix.toml` in current or parent directories
4. **Global config** — `~/.config/ursix/config.toml`

### Configuration File

```toml
# .ursix.toml
provider = "ollama"
model = "qwen2.5-coder:7b"
ollama_url = "http://localhost:11434"
openai_url = "https://api.openai.com/v1"
```

### Environment Variables

```bash
export URSIX_PROVIDER=openai
export URSIX_MODEL=gpt-4
export URSIX_OPENAI_API_KEY=sk-...
export URSIX_OPENAI_URL=https://api.openai.com/v1
export URSIX_OLLAMA_URL=http://localhost:11434
```

### CLI Reference

```
Global Flags:
    --text                 Human-readable output (default: JSON)
    --provider <NAME>      LLM provider (ollama, openai)
-m, --model <MODEL>        Model to use
    --ollama-url <URL>     Ollama API base URL
    --openai-url <URL>     OpenAI-compatible API base URL
    --openai-api-key <KEY> API key for OpenAI endpoints
    --chunk                Enable chunked processing for large inputs
    --max-concurrency <N>  Parallel chunk limit (default: 4)
    --tokenizer <MODE>     Token counting: heuristic (fast) or full (accurate)
    --no-retry             Disable automatic retry on transient failures
    --max-retries <N>      Maximum retry attempts (default: 3)
    --partial              Return partial results when some chunks fail
```

## Architecture

```
src/
├── main.rs              # Entry point, tokio runtime
├── cli.rs               # Argument parsing, command dispatch
├── config.rs            # Layered configuration (files, env, CLI)
├── context.rs           # Pre-LLM context gathering (files, git state)
├── input.rs             # Input source handling (stdin, files)
├── pipeline.rs          # Stateless single-pass executor
├── chunk.rs             # Token-aware parallel chunking
├── tokens.rs            # Token counting (heuristic and full modes)
├── parsers.rs           # Response parsing utilities
├── prompts.rs           # Command-specific system prompts
├── commands/
│   ├── mod.rs
│   ├── explain.rs
│   ├── review.rs
│   ├── fix.rs
│   ├── commit.rs
│   └── config.rs
├── output/
│   ├── mod.rs           # Exit codes, output modes
│   ├── explain.rs
│   ├── review.rs
│   ├── fix.rs
│   └── commit.rs
├── rules/
│   ├── mod.rs           # Rule types and resolution
│   ├── defaults.rs      # Built-in default rules
│   ├── loader.rs        # YAML loading
│   └── resolver.rs      # Category-based filtering
└── llm/
    ├── mod.rs           # LlmClient trait and types
    ├── ollama.rs        # Ollama API implementation
    └── openai.rs        # OpenAI-compatible API implementation
```

## Contributing

### Development Setup

```bash
git clone https://github.com/your-org/ursix.git
cd ursix
./.githooks/install.sh
cargo build
cargo test
```

### Code Quality

Pre-commit hooks enforce:
- `cargo fmt --check`
- Pedantic Clippy (no `unwrap`, `expect`, `panic`, `todo`, `unimplemented`)
- Full test suite

### Adding Commands

1. Add variant to `Command` enum in `cli.rs`
2. Create handler in `commands/`
3. Add prompts in `prompts.rs`
4. Add output types in `output/`
5. Write tests

---

<div align="center">

**[Report a Bug](https://github.com/your-org/ursix/issues)** •
**[Request a Feature](https://github.com/your-org/ursix/issues)**

</div>
