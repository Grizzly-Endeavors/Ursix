# CLI Reference

Complete reference for all Ursix commands and flags.

## Global Flags

These flags apply to all commands:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--text` | boolean | false | Output as plain text instead of JSON |
| `--provider` | 1-3 args | config | LLM provider: `NAME [URL] [API-KEY]` |
| `-m, --model` | string | config | Model to use |
| `--retries` | u32 | 3 | Max retry attempts (0 to disable) |
| `--timeout` | u64 | 60 | Timeout for LLM requests in seconds |
| `--dry-run` | boolean | false | Show token estimation without making LLM calls |

### Provider Flag Examples

The `--provider` flag accepts 1-3 space-separated values. Place it after the command:

```bash
# Just provider name (uses default URL)
usx review --provider ollama

# Provider with custom URL
usx review --provider ollama http://localhost:11434

# Provider with URL and API key
usx review --provider openai https://api.openai.com/v1 sk-xxx
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `URSIX_API_KEY` | API key for authenticated providers |
| `URSIX_PROVIDER_URL` | Provider API base URL |
| `OPENAI_API_KEY` | Fallback API key (if `URSIX_API_KEY` not set) |

## Commands

- [`derive`](commands/derive.md) - Derive content from input (commit-msg, explanation, summary)
- [`review`](commands/review.md) - Review code changes from stdin or file
- [`fix`](commands/fix.md) - Transform code snippets using structured input
- [`config`](commands/config.md) - View configuration

## Input Handling

All commands read from stdin by default or from a positional file argument:

```bash
# Pipe input via stdin
cat src/main.rs | usx derive explanation
git diff --staged | usx review

# Read from file directly (positional argument)
usx derive explanation src/main.rs
usx review changes.diff

# Explicit stdin (use - as file argument)
usx derive explanation -
```

If no input is provided, commands will block waiting for stdin.

## Execution Mode

Single LLM call with piped input. No tools, no message history.

- Fast and predictable
- Suitable for all tasks
- Stateless: same input always produces consistent output

## Chunked Processing

### Derive Command

The `derive` command supports `--chunk` for processing large inputs:

```bash
# Process large input with automatic chunking
cat src/**/*.rs | usx derive summary --chunk

# Estimate token usage
cat src/**/*.rs | usx derive summary --chunk --dry-run
```

When chunked processing is enabled:
1. Input is split into ~4k token chunks
2. Each chunk is processed independently
3. Results are synthesized into final output

### Review Command

The `review` command supports `--chunk` for file-based diff chunking:

```bash
# Chunked review of large diff
git diff HEAD~10 | usx review --chunk

# With custom concurrency
git diff | usx review --chunk --concurrency 8

# Continue on partial failures
git diff | usx review --chunk --partial

# Preview chunk plan without LLM calls
git diff | usx review --chunk --dry-run
```

Chunking flags (shared by derive and review):

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--chunk` | boolean | false | Enable chunking for large inputs |
| `--concurrency` | usize | 4 | Max concurrent chunk executions |
| `--partial` | boolean | false | Continue when some chunks fail (review only) |

**Important:** The `--chunk` flag for review only works with diff input. Single files cannot be chunked and will fall back to single-pass processing.

Note: The `fix` command operates on single snippets and does not support chunking. It uses structured JSON input to specify exact code locations.

## Exit Codes

Simplified 5-category system for reliable automation:

| Code | Name | Meaning |
|------|------|---------|
| 0 | Success | Command completed successfully with no issues |
| 1 | IssuesFound | Command completed but found issues (review problems) |
| 2 | UserError | User-fixable errors: bad arguments, empty input, missing files |
| 3 | TransientError | Transient errors: network issues, rate limits (retry may help) |
| 4 | PermanentError | Permanent errors: auth failures, parse errors (retry won't help) |

Scripts can use these to make retry decisions:
```bash
git diff | usx review
case $? in
  0) echo "Clean" ;;
  1) echo "Issues found" ;;
  2) echo "Check your input" ;;
  3) echo "Retry later" ;;
  4) echo "Fix configuration" ;;
esac
```

## Error Handling & Retry

### Automatic Retry

By default, Ursix automatically retries LLM calls on transient failures (network errors, rate limits). This improves reliability without user intervention.

```bash
# Default: retries up to 3 times with exponential backoff
git diff | usx review

# Disable retry
git diff | usx review --retries 0

# Custom retry count
git diff | usx review --retries 5
```

### JSON Error Output

Errors are output as JSON to stderr, keeping stdout clean for piping:

```json
{
  "_meta": { "schema_version": "1" },
  "error": {
    "code": "network_error",
    "message": "connection timeout",
    "retryable": true
  }
}
```

## Configuration

Configuration is loaded from multiple sources (highest to lowest precedence):

1. CLI flags
2. Environment variables
3. `.ursix.toml` in working directory
4. `~/.ursix.toml` in home directory
5. Built-in defaults

### Configuration File Format

```toml
# .ursix.toml
provider = "openai"
model = "gpt-4"
provider_url = "https://api.openai.com/v1"
tokenizer_mode = "heuristic"
timeout_secs = 60
```

Note: API keys should be set via environment variables (`URSIX_API_KEY`) for security, not in config files.

### Available Settings

| Key | Type | Description |
|-----|------|-------------|
| `provider` | enum | LLM provider (`ollama`, `openai`) |
| `model` | string | Model name to use |
| `provider_url` | string | Provider API base URL (overrides default for selected provider) |
| `tokenizer_mode` | enum | Token counting mode (`heuristic`, `full`) |
| `timeout_secs` | u64 | Timeout for LLM requests in seconds |
