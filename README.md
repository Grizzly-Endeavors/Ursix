<div align="center">

# Ursix

**LLM as a boring Unix utility**

[![Rust](https://img.shields.io/badge/rust-2024_edition-orange.svg)](https://www.rust-lang.org/)
[![Ollama](https://img.shields.io/badge/ollama-compatible-blue.svg)](https://ollama.ai/)
[![OpenAI](https://img.shields.io/badge/openai--compatible-API-green.svg)](https://platform.openai.com/)

[Getting Started](#getting-started) •
[Commands](#commands) •
[Semantic Linting](#semantic-linting-with-rulesyml) •
[CI/CD](#cicd-integration) •
[Philosophy](PHILOSOPHY.md)

</div>

---

## What is Ursix?

Ursix treats LLMs as compute primitives. Pipe text in, get structured output, compose with the rest of your toolkit.

```bash
# These are commands, not conversations
git diff --staged | usx derive commit-msg    # Generate a commit message
cat src/auth.rs | usx derive explanation     # Explain what code does
git diff --staged | usx review               # Find issues in changes
```

**The JSON Guarantee**: Despite LLMs under the hood, output is always valid, parseable JSON. Exit codes are always meaningful. Your scripts won't break.

```bash
# Reliable enough for CI
git diff | usx review | jq '.passed'          # true or false, never broken JSON
git diff | usx review && echo "Clean" || echo "Issues found (exit $?)"
```

## Why Ursix?

Most AI coding tools are chat interfaces, or basic auto-complete. Ursix is different:

| Chat Wrapper | Ursix |
|--------------|-------|
| "Can you review my code?" | `git diff \| usx review` |
| Copy-paste into chat | Pipe directly to stdin |
| Parse prose response manually | JSON output, structured fields |
| Hope it remembers context | Stateless—same input, same output |
| Can't automate | Exit codes, stdin/stdout, CI-native |

**No magic. No batteries.** Ursix won't assume you want anything. If you don't provide input, it won't look for it. If you don't enable chunking, it won't chunk. Explicit over implicit, always.

## Getting Started

### Prerequisites

- **Rust 1.85+** (2024 edition)
- **An LLM provider**:
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
usx status --text
```

### Quick Start

```bash
# Start Ollama if using local models
ollama serve

# Verify connectivity
usx status --text

# Explain a file
cat src/main.rs | usx derive explanation --text

# Review staged changes
git diff --staged | usx review --text

# Generate a commit message
git diff --staged | usx derive commit-msg --text
```

## Commands

### `usx derive` — Transform Text

Generic text transformation. The LLM derives content based on the mode you specify.

**Modes:**
- `explanation` — Explain what code does
- `commit-msg` — Generate a conventional commit message from a diff
- `summary` — Summarize content

```bash
usx derive explanation src/lib.rs                    # Explain code (file argument)
cat src/lib.rs | usx derive explanation              # Explain code (stdin)
git diff --staged | usx derive commit-msg            # Generate commit message
cat README.md | usx derive summary                   # Summarize content
```

**Chunking for large inputs:**
```bash
cat huge_file.rs | usx derive explanation --chunk
```

**Options:**
| Flag | Description |
|------|-------------|
| `[FILE]` | Input file (omit for stdin, use `-` for explicit stdin) |
| `--text` | Human-readable output instead of JSON |
| `--chunk` | Split large inputs, process in parallel, synthesize |
| `--concurrency N` | Parallel chunk limit (default: 4) |
| `--dry-run` | Show token estimate without calling LLM |

---

### `usx review` — Semantic Linting

Review code changes and output structured issues. Designed for CI pipelines.

```bash
git diff --staged | usx review                       # Review staged changes
usx review src/auth.rs                               # Review a single file
git diff HEAD~3 | usx review                         # Review commit range
git diff | usx review --checks security              # Focus on specific rules
```

**Output (JSON):**
```json
{
  "_meta": { "schema_version": "1", "model": "qwen2.5-coder:7b", "duration_ms": 1200 },
  "summary": "Found 2 issues",
  "issues": [
    { "severity": "error", "file": "src/auth.rs", "line": 42, "message": "Hardcoded secret", "rule": "no-hardcoded-secrets" }
  ],
  "passed": false
}
```

**Exit codes for CI:**
- `0` — Passed (no issues or warnings only)
- `1` — Failed (errors found)

**Options:**
| Flag | Description |
|------|-------------|
| `[FILE]` | Input file (omit for stdin, use `-` for explicit stdin) |
| `--checks LIST` | Comma-separated rule categories (e.g., `security,style`) |
| `--chunk` | Split diff by file, process in parallel |
| `--concurrency N` | Max concurrent chunks (default: 4) |
| `--partial` | Return partial results if some chunks fail |
| `--dry-run` | Show token estimate and chunking plan |

---

### `usx fix` — Atomic Code Transformation

Transform specific code snippets with structured input. The fix command takes exact location information and outputs a unified diff. The snippet is automatically inferred from the file content at the specified lines.

```bash
# Fix a specific issue (atomic mode - default)
echo '{"issue": "unused variable", "file": "src/main.rs", "lines": [42, 42]}' | usx fix

# Fix multiple issues in one file (whole-file mode)
echo '{"file": "src/main.rs", "issues": [{"issue": "a", "lines": [10,10]}, {"issue": "b", "lines": [20,20]}]}' | usx fix --mode whole-file

# Apply the fix (from file argument)
usx fix input.json | jq -r '.diff' | patch -p1
```

**Atomic Mode Input (JSON):**
```json
{
  "issue": "description of the problem",
  "file": "path/to/file.rs",
  "lines": [start, end]
}
```

**Whole-File Mode Input (JSON):**
```json
{
  "file": "path/to/file.rs",
  "issues": [
    {"issue": "first issue", "lines": [10, 10]},
    {"issue": "second issue", "lines": [25, 28]}
  ]
}
```

**Atomic Mode Output (JSON):**
```json
{
  "diff": "--- a/src/main.rs\n+++ b/src/main.rs\n@@ ...",
  "file": "src/main.rs",
  "lines": [42, 42],
  "lines_added": 1,
  "lines_removed": 1
}
```

**Options:**
| Flag | Description |
|------|-------------|
| `[FILE]` | JSON input file (omit for stdin, use `-` for explicit stdin) |
| `--mode {atomic,whole-file}` | Fix mode (default: atomic) |
| `--context N` | Diff context lines (default: 3) |
| `--retry` | Retry on validation failure |
| `--partial` | Return partial results on failure |
| `--dry-run` | Validate input without LLM call |

---

### `usx status` — Pre-flight Check

Validate configuration and test provider connectivity. Returns exit code 0 when all checks pass.

```bash
usx status                                   # JSON output
usx status --text                            # Human-readable output
usx status --verbose --text                  # Include response time and model list
```

**Human output:**
```
Ursix Status: OK

Provider:  ollama (http://localhost:11434)
Model:     llama3.2
API Key:   not set

Checks:
  [ok] config: configuration loaded successfully
  [ok] provider: ollama at http://localhost:11434
  [ok] api_key: not required for Ollama
  [ok] connectivity: connected, model 'llama3.2' available
```

**Use in scripts:**
```bash
# Pre-flight check before running commands
usx status || exit 1

# CI/CD validation
if ! usx status > /dev/null 2>&1; then
  echo "Ursix not configured correctly"
  exit 1
fi
```

**Options:**
| Flag | Description |
|------|-------------|
| `--verbose` | Show response time and available models |
| `--text` | Human-readable output instead of JSON |

---

### `usx config` — View Configuration

```bash
usx config --list                           # Show all settings
usx config model                            # Show specific value
```

## Semantic Linting with rules.yml

Codify your team's standards in version-controlled YAML. The LLM enforces them.

### Why Rules?

Traditional linters check syntax. Ursix checks intent:

```yaml
# .ursix/rules.yml
categories:
  security:
    - name: no-hardcoded-secrets
      description: "Never hardcode API keys, passwords, or secrets"
      severity: error

    - name: no-unwrap-in-handlers
      description: "HTTP handlers must use proper error handling, not .unwrap()"
      severity: error

  style:
    - name: why-not-what-comments
      description: "Comments should explain WHY, not WHAT. Flag comments that just restate the code."
      severity: warning
```

These aren't regex patterns—they're instructions the LLM understands and applies contextually.

### Using Rules

```bash
usx review                                  # Apply all rules
usx review --checks security                # Only security rules
usx review --checks security,style          # Multiple categories
```

Filter output by rule:
```bash
usx review | jq '.issues[] | select(.rule == "no-unwrap-in-handlers")'
```

### Rule Locations

Rules are loaded from (in order of precedence):
1. `.ursix/rules.yml` (project)
2. `rules.yml` (project root)
3. `~/.config/ursix/rules.yml` (global)

Project rules extend global rules per-category.

## CI/CD Integration

### Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success (command completed, no issues) |
| `1` | Issues found (review problems, errors to report) |
| `2` | User error (bad args, config, missing files) |
| `3` | Transient error (network, rate limit—retry may help) |
| `4` | Permanent error (auth, parse—retry won't help) |

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
          git diff origin/${{ github.base_ref }}...HEAD | usx review > review.json
          jq -r '.summary' review.json
          if [ $(jq '.passed' review.json) = "false" ]; then
            jq -r '.issues[] | "[\(.severity)] \(.file):\(.line // "?") - \(.message)"' review.json
            exit 1
          fi
```

### Git Hooks

**pre-commit:**
```bash
#!/bin/bash
git diff --staged | usx review --checks security > /tmp/review.json
if [ $(jq '.passed' /tmp/review.json) = "false" ]; then
  echo "Security issues found:"
  jq -r '.issues[] | "\(.file):\(.line) - \(.message)"' /tmp/review.json
  exit 1
fi
```

### Composing with Unix Tools

```bash
# Filter to errors only
git diff | usx review | jq '.issues[] | select(.severity == "error")'

# Count issues by rule
git diff | usx review | jq '[.issues[].rule] | group_by(.) | map({rule: .[0], count: length})'

# Process multiple files
find src -name "*.rs" | xargs -I{} sh -c 'cat {} | usx derive explanation --text'

# Dry run to estimate tokens before committing to API calls
git diff | usx review --dry-run
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
provider_url = "http://localhost:11434"  # Optional: override default for selected provider
```

Note: API keys should be set via environment variables for security.

### Environment Variables

```bash
export URSIX_PROVIDER=openai
export URSIX_MODEL=gpt-4
export URSIX_API_KEY=sk-...              # API key for authenticated providers
export URSIX_PROVIDER_URL=https://api.openai.com/v1
```

### Global Flags

```
--text                           Human-readable output (default: JSON)
--provider <NAME> [URL] [KEY]    LLM provider with optional URL and API key
-m, --model <MODEL>              Model to use
--retries <N>                    Max retry attempts (default: 3, 0 to disable)
--timeout <N>                    Timeout for LLM requests in seconds (default: 60)
--dry-run                        Show token estimation without making LLM calls
```

**Examples** (place after command):
```bash
usx review --provider ollama                              # Just provider
usx review --provider openai https://api.openai.com/v1 sk-xxx  # With URL and key
```

## Philosophy

Ursix is a link in a chain, not an end-to-end solution. It won't gather context, maintain state, or execute actions. You provide input, it processes, you handle output.

For the full design philosophy, see [PHILOSOPHY.md](PHILOSOPHY.md).

---

<div align="center">

**[Report a Bug](https://github.com/grizzly-endeavors/ursix/issues)** •
**[Request a Feature](https://github.com/grizzly-endeavors/ursix/issues)**

</div>
