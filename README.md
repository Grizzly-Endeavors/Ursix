<div align="center">

# Ursix

**LLM-powered development tools that fit your workflow**

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![Ollama](https://img.shields.io/badge/ollama-compatible-blue.svg)](https://ollama.ai/)
[![OpenAI](https://img.shields.io/badge/openai--compatible-API-green.svg)](https://platform.openai.com/)

[Getting Started](#getting-started) •
[Commands](#commands) •
[Review Rules](#review-rules) •
[Scripting & CI/CD](#scripting--cicd) •
[Contributing](#contributing)

</div>

---

## What is Ursix?

Ursix (`usx`) is a command-line tool that brings LLM capabilities directly into your development workflow. Instead of copy-pasting code into chat windows or context-switching between tools, Ursix provides purpose-built commands for common development tasks—code review, explanation, commit messages, and fixes—all from your terminal.

```bash
# Review your staged changes before committing
usx review

# Generate a conventional commit message
usx commit --execute

# Explain unfamiliar code
usx explain src/auth/jwt.rs

# Fix clippy warnings automatically
usx fix src/lib.rs --lint --apply
```

## Why Ursix?

### The Problem

AI coding assistants are powerful, but using them often means:
- **Context-switching**: leaving your terminal to paste code into a web interface
- **Copy-paste overhead**: manually moving code and diffs back and forth
- **Generic responses**: chat interfaces don't know about your project structure
- **No automation**: can't integrate into CI/CD or git hooks
- **Inconsistent standards**: code review feedback varies by reviewer mood and memory

### The Solution

Ursix brings LLM capabilities to where you already work—your terminal—with commands designed for specific development tasks:

| Pain Point | Ursix Solution |
|------------|----------------|
| "I need to review these changes" | `usx review` reads your git diff automatically |
| "What does this code do?" | `usx explain src/module.rs` with full file context |
| "Write me a commit message" | `usx commit` analyzes staged changes and formats properly |
| "Fix these lint errors" | `usx fix --lint --apply` diagnoses and patches files |
| "Automate in CI" | JSON output by default with exit codes for pipelines |
| "Enforce team standards" | Configurable `rules.yml` makes reviews consistent |

### Key Design Principles

- **Unix philosophy**: stdin/stdout, exit codes, pipes—Ursix composes with your existing tools
- **Predictable by default**: single-pass pipeline mode for fast, deterministic results
- **Powerful when needed**: opt-in `--agent` mode for complex multi-step tasks
- **Works offline**: first-class support for local models via Ollama
- **CI-native**: JSON output, meaningful exit codes, and configurable rules for automation
- **Codified standards**: turn subjective review feedback into enforceable, versioned rules

## Getting Started

### Prerequisites

- **Rust 1.85+** (2024 edition)
- **An LLM provider** — choose one:
  - [Ollama](https://ollama.ai/) for local models (default)
  - Any OpenAI-compatible API endpoint

### Installation

**From source:**

```bash
git clone https://github.com/your-org/ursix.git
cd ursix
cargo install --path .
```

**Verify installation:**

```bash
usx --version
usx models  # List available Ollama models
```

### First Steps

```bash
# Start Ollama if using local models
ollama serve

# Ask a question
usx ask "What is the idiomatic way to handle errors in Rust?"

# Explain a file in your project
usx explain src/main.rs

# Review your staged changes
git add -p  # Stage some changes
usx review
```

## Commands

### `usx ask` — General Queries

Ask questions with optional file context.

```bash
# Simple question
usx ask "How do I parse JSON in Rust?"

# With file context
usx ask --from src/config.rs "How can I improve error handling here?"

# Pipe in context
cat error.log | usx ask --stdin "What's causing this error?"

# Complex tasks with tool access
usx ask --agent "Refactor the error handling in src/cli.rs"
```

### `usx explain` — Code Explanation

Get clear explanations of code with full file context.

```bash
# Explain a file
usx explain src/auth/middleware.rs

# Deep exploration mode
usx explain src/database/ --agent
```

### `usx review` — Code Review

Review code changes with actionable feedback.

```bash
# Review staged changes (default)
usx review

# Review specific files
usx review src/api.rs src/handlers.rs

# Review a specific diff
usx review --diff HEAD~3

# Focus on specific concerns
usx review --checks security,performance

# Thorough multi-file analysis
usx review --agent
```

**Exit codes:**
- `0` — Review passed (no errors or warnings)
- `1` — Review found issues

### `usx fix` — Code Fixes

Identify and fix issues in your code.

```bash
# Fix issues in a file
usx fix src/lib.rs

# Fix clippy/lint issues
usx fix src/main.rs --lint

# Auto-apply fixes
usx fix src/lib.rs --lint --apply

# Complex fixes with tool access
usx fix src/auth.rs --agent
```

### `usx commit` — Commit Messages

Generate conventional commit messages from staged changes.

```bash
# Generate commit message
usx commit

# Include detailed body
usx commit --body

# Generate and execute immediately
usx commit --execute

# Different styles
usx commit --style simple
```

### `usx config` — Configuration

Manage Ursix configuration.

```bash
# List all settings
usx config --list

# Get a specific value
usx config model

# Set a value (coming soon)
usx config model qwen2.5-coder:7b
```

### `usx models` — List Models

List available models from your Ollama instance.

```bash
usx models
```

## Execution Modes

Ursix supports two execution modes, letting you choose between speed and capability:

### Pipeline Mode (Default)

Fast, single-pass LLM calls with pre-gathered context. No tool execution, predictable behavior.

```bash
usx review              # Gathers diff, single LLM call
usx commit              # Gathers staged changes, generates message
usx explain src/lib.rs  # Reads file, explains in one pass
```

**Best for:** Quick tasks, CI/CD pipelines, deterministic output.

### Agent Mode (`--agent`)

Multi-turn execution with full tool access. The LLM can read files, run commands, and iterate on solutions.

```bash
usx ask --agent "Find and fix all TODO comments in src/"
usx fix src/main.rs --agent  # Can run clippy, edit files, verify fixes
usx review --agent           # Can explore related files for context
```

**Best for:** Complex tasks requiring exploration, multi-file changes, iterative fixes.

## Configuration

Ursix uses layered configuration (highest priority first):

1. **CLI flags** — `--model qwen2.5-coder:7b`
2. **Environment variables** — `URSIX_MODEL=qwen2.5-coder:7b`
3. **Project config** — `.ursix.toml` in current or parent directories
4. **Global config** — `~/.config/ursix/config.toml`

### Configuration File

Create `.ursix.toml` in your project root:

```toml
# LLM Provider: "ollama" or "openai"
provider = "ollama"

# Model selection
model = "qwen2.5-coder:7b"

# Provider URLs
ollama_url = "http://localhost:11434"
openai_url = "https://api.openai.com/v1"

# Agent mode settings
max_turns = 50
```

### Environment Variables

```bash
export URSIX_PROVIDER=openai
export URSIX_MODEL=gpt-4
export URSIX_OPENAI_API_KEY=sk-...
export URSIX_OPENAI_URL=https://api.openai.com/v1
export URSIX_OLLAMA_URL=http://localhost:11434
export URSIX_MAX_TURNS=50
```

### CLI Reference

```
Global Flags:
    --text              Output as plain text instead of JSON (default: JSON)
    --provider <NAME>   LLM provider (ollama, openai)
-m, --model <MODEL>     Model to use
    --ollama-url <URL>  Ollama API base URL
    --openai-url <URL>  OpenAI-compatible API base URL
    --openai-api-key    API key for OpenAI endpoints
    --max-turns <N>     Maximum agent turns (default: 50)
-v, --verbose           Enable verbose output
```

## Review Rules

**Turn subjective nitpicks into enforceable standards.**

Code review feedback is often inconsistent—what one reviewer catches, another misses. Senior developers carry implicit knowledge about "how we do things here" that isn't documented anywhere. Ursix solves this with a versioned, declarative rules system that makes team standards explicit and enforceable.

### Why Rules Matter

Without codified rules:
- Review quality depends on reviewer attention and mood
- New team members don't know unwritten conventions
- The same issues get flagged (or missed) inconsistently
- "We should do X" discussions never become enforced policy

With Ursix rules:
- Standards are versioned alongside your code
- Every review applies the same checks consistently
- Onboarding is faster—rules document team expectations
- Discussions become PRs to `rules.yml`, not repeated comments

### Creating Rules

Create `rules.yml` (or `.ursix/rules.yml`) in your project root:

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

    - name: no-println
      description: "Use tracing macros instead of println! for logging"
      severity: warning
      files: "src/**/*.rs"

  performance:
    - name: avoid-unnecessary-clone
      description: "Prefer borrowing over cloning when possible"
      severity: info
      files: "*.rs"
```

### Rule Structure

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique identifier (appears in JSON output) |
| `description` | Yes | Natural language description for the LLM |
| `severity` | No | `error`, `warning`, or `info` (default: `warning`) |
| `files` | No | Glob pattern to limit scope (default: all files) |

### Using Rules

```bash
# Apply all rules
usx review

# Apply only security rules
usx review --checks security

# Apply multiple categories
usx review --checks security,performance

# Rules auto-filter by file type
usx review src/api.rs  # Only applies rules matching src/api.rs
```

### Rule Hierarchy

Rules load from multiple locations and merge:

1. **Global rules**: `~/.config/ursix/rules.yml` (your personal defaults)
2. **Project rules**: `rules.yml` or `.ursix/rules.yml` (team standards)

Project rules extend global rules—they don't replace them.

### Built-in Defaults

If no `rules.yml` exists, Ursix applies sensible defaults:

| Category | Rules |
|----------|-------|
| **Security** | no-unwrap, no-hardcoded-secrets, input-validation |
| **Style** | doc-comments, naming-conventions |
| **Performance** | avoid-clone, efficient-collections |
| **Correctness** | error-handling, boundary-conditions |

### Output with Rules

When rules trigger issues, the JSON output includes the rule name:

```json
{
  "summary": "Found 2 issues",
  "issues": [
    {
      "severity": "error",
      "file": "src/auth.rs",
      "line": 42,
      "message": "Using .unwrap() on user input—this will panic on invalid data",
      "rule": "no-unwrap"
    }
  ],
  "passed": false
}
```

This enables filtering and tracking by rule in CI:

```bash
# Count issues by rule
usx review | jq '[.issues[].rule] | group_by(.) | map({rule: .[0], count: length})'

# Fail only on security rules
usx review --checks security | jq -e '.passed'
```

## Scripting & CI/CD

Ursix is designed for automation. Every command supports structured output, meaningful exit codes, and Unix-style composition.

### JSON Output

All commands output JSON by default for machine-readable output (use `--text` for human-readable):

```bash
# Structured review results
usx review | jq '.issues[] | select(.severity == "error")'

# Parse commit message components
usx commit | jq -r '.title'

# Extract explanation for documentation
usx explain src/api.rs | jq -r '.explanation'
```

### Exit Codes

Commands return meaningful exit codes for scripting:

| Exit Code | Meaning |
|-----------|---------|
| `0` | Success (review passed, no issues) |
| `1` | Issues found (review failed, unfixable problems) |
| `2` | Execution error (invalid args, LLM failure) |

```bash
# Conditional execution
usx review --text && echo "Review passed" || echo "Issues found"

# In CI pipelines
usx review > review.json
if [ $? -ne 0 ]; then
  cat review.json | jq '.issues[]'
  exit 1
fi
```

### Pipe Integration

Commands read from stdin and write to stdout:

```bash
# Pipe file content for analysis
cat src/complex.rs | usx ask --stdin "What are the potential bugs here?"

# Chain review and fix
usx review | usx fix --from - src/main.rs --apply

# Process multiple files
find src -name "*.rs" -exec usx explain {} \; | jq -s '.'

# Use with other tools
git diff HEAD~1 | usx ask --stdin "Summarize these changes"
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

          # Always output the summary
          jq -r '.summary' review.json

          # Fail if issues found
          if [ $(jq '.passed' review.json) = "false" ]; then
            echo "::group::Review Issues"
            jq -r '.issues[] | "[\(.severity)] \(.file):\(.line // "?") - \(.message)"' review.json
            echo "::endgroup::"
            exit 1
          fi

      - name: Upload Review
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: code-review
          path: review.json
```

### GitLab CI

```yaml
code-review:
  stage: test
  script:
    - usx review --diff $CI_MERGE_REQUEST_DIFF_BASE_SHA...$CI_COMMIT_SHA > review.json
    - |
      if [ $(jq '.passed' review.json) = "false" ]; then
        jq '.issues[]' review.json
        exit 1
      fi
  artifacts:
    reports:
      codequality: review.json
```

### Git Hooks

**prepare-commit-msg** — Auto-generate commit messages:

```bash
#!/bin/bash
# .git/hooks/prepare-commit-msg
usx commit > "$1"
```

**pre-commit** — Review staged changes:

```bash
#!/bin/bash
# .git/hooks/pre-commit
usx review --checks security > /tmp/review.json
if [ $(jq '.passed' /tmp/review.json) = "false" ]; then
  echo "Security issues found:"
  jq -r '.issues[] | "  \(.file):\(.line) - \(.message)"' /tmp/review.json
  exit 1
fi
```

### Composing with Unix Tools

```bash
# Review only changed files
git diff --name-only HEAD~1 | xargs usx review

# Batch explain all modules
for f in src/*.rs; do
  echo "=== $f ==="
  usx explain "$f" | jq -r '.explanation'
done

# Generate changelog from commits
git log --oneline HEAD~10..HEAD | while read sha msg; do
  git show $sha --stat | usx ask --stdin "Summarize this commit"
done

# Find files needing documentation
usx review --checks style | jq -r '.issues[] | select(.rule == "doc-comments") | .file' | sort -u
```

### Shell Integration

Add to `.bashrc` or `.zshrc`:

```bash
# Aliases
alias review='usx review'
alias explain='usx explain'
alias commit='usx commit --execute'

# Function: review and fix in one go
fix-review() {
  usx review > /tmp/review.json
  if [ $(jq '.passed' /tmp/review.json) = "false" ]; then
    usx fix --from /tmp/review.json "$@" --apply
  fi
}

# Function: explain with less paging
explain() {
  usx explain "$@" | less
}
```

## Architecture

```
src/
├── main.rs          # Entry point, tokio runtime
├── cli.rs           # Argument parsing, command dispatch
├── config.rs        # Layered configuration (files, env, CLI)
├── rules.rs         # Review rules loading and resolution
├── pipeline.rs      # Stateless single-pass executor
├── agent.rs         # Multi-turn agentic loop
├── output.rs        # Human/JSON output formatting
├── prompts.rs       # Command-specific system prompts
├── llm/
│   ├── mod.rs       # LlmClient trait and types
│   ├── ollama.rs    # Ollama API implementation
│   └── openai.rs    # OpenAI-compatible API implementation
└── tools/
    ├── mod.rs       # Tool trait and types
    ├── bash.rs      # Shell execution
    ├── file.rs      # Read, write, edit operations
    └── search.rs    # Glob and grep operations
```

### Agent Tools

When running in `--agent` mode, the LLM has access to:

| Tool | Description |
|------|-------------|
| `bash` | Execute shell commands with timeout |
| `read` | Read file contents |
| `write` | Create or overwrite files |
| `edit` | Make precise edits to existing files |
| `glob` | Find files matching patterns |
| `grep` | Search file contents with regex |

## Contributing

Contributions are welcome. Please read the guidelines below before submitting.

### Development Setup

```bash
git clone https://github.com/your-org/ursix.git
cd ursix
./.githooks/install.sh  # Install pre-commit hooks
cargo build
cargo test
```

### Code Quality

The project enforces strict quality standards via pre-commit hooks:

- **Formatting**: `cargo fmt --check`
- **Linting**: Pedantic Clippy with strict error handling rules
- **Testing**: Full test suite must pass

Key lint rules:
- `unsafe_code` — forbidden
- `unwrap_used`, `expect_used`, `panic` — denied
- No `todo!()` or `unimplemented!()` in committed code

### Running Tests

```bash
# Unit and integration tests
cargo test

# With output
cargo test -- --nocapture
```

### Adding New Commands

1. Add the command variant to `Command` enum in `cli.rs`
2. Create the handler function `cmd_<name>()`
3. Add system prompts in `prompts.rs` (both agent and pipeline variants)
4. Add output types in `output.rs`
5. Write tests

### Adding LLM Providers

Implement the `LlmClient` trait:

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn send_chat_completion(
        &self,
        messages: &[Message],
        tools: Option<&[ToolDefinition]>,
        json_mode: bool,
    ) -> Result<ChatResponse, LlmError>;

    fn model_name(&self) -> &str;
}
```

## Acknowledgments

Ursix is built with:

- [Clap](https://github.com/clap-rs/clap) — Command-line argument parsing
- [Tokio](https://tokio.rs/) — Async runtime
- [Reqwest](https://github.com/seanmonstar/reqwest) — HTTP client
- [Serde](https://serde.rs/) — Serialization framework

---

<div align="center">

**[Report a Bug](https://github.com/your-org/ursix/issues)** •
**[Request a Feature](https://github.com/your-org/ursix/issues)** •
**[Discussions](https://github.com/your-org/ursix/discussions)**

</div>
