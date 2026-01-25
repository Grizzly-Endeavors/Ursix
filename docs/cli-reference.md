# CLI Reference

Complete reference for all Ursix commands and flags.

## Global Flags

These flags apply to all commands:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--text` | boolean | false | Output as plain text instead of JSON |
| `--provider` | enum | config | LLM provider (`ollama`, `openai`) |
| `-m, --model` | string | config | Model to use |
| `--ollama-url` | string | config | Ollama API base URL |
| `--openai-url` | string | config | OpenAI-compatible API base URL |
| `--openai-api-key` | string | config | API key for OpenAI-compatible endpoints |
| `--chunk` | boolean | false | Enable chunked processing (warns if not supported) |
| `--max-concurrency` | usize | 4 | Maximum concurrent chunk executions |
| `--tokenizer` | enum | heuristic | Tokenizer mode (`heuristic`, `full`) |
| `--no-retry` | boolean | false | Disable automatic retry on transient failures |
| `--max-retries` | u32 | 3 | Maximum retry attempts for transient failures |
| `--partial` | boolean | false | Return partial results when some chunks fail |
| `--timeout` | u64 | 60 | Timeout for LLM requests in seconds |
| `--dry-run` | boolean | false | Show token estimation without making LLM calls |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `URSIX_OPENAI_API_KEY` | API key for OpenAI-compatible endpoints |

## Commands

- [`explain`](commands/explain.md) - Explain code from stdin or file
- [`review`](commands/review.md) - Review code changes from stdin or file
- [`fix`](commands/fix.md) - Suggest fixes for code from stdin or file
- [`commit`](commands/commit.md) - Generate commit messages from diff
- [`config`](commands/config.md) - View configuration

## Input Handling

All commands read from stdin by default or from a file via `--from`:

```bash
# Pipe input via stdin
cat src/main.rs | usx explain
git diff --staged | usx review

# Read from file directly
usx explain --from src/main.rs
usx review --from changes.diff

# Explicit stdin (same as piped)
usx explain --from -
```

If no input is provided, commands will block waiting for stdin.

## Execution Mode

Single LLM call with piped input. No tools, no message history.

- Fast and predictable
- Suitable for all tasks
- Stateless: same input always produces consistent output

## Chunked Processing

The `--chunk` flag is currently not supported in stdin mode and will issue a warning. All commands process their entire input in a single LLM call.

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

# Disable retry (for debugging or when you want fast failures)
git diff | usx review --no-retry

# Custom retry count
git diff | usx review --max-retries 5
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
ollama_url = "http://localhost:11434"
openai_url = "https://api.openai.com/v1"
openai_api_key = "sk-..."
tokenizer_mode = "heuristic"
timeout_secs = 60
```

### Available Settings

| Key | Type | Description |
|-----|------|-------------|
| `provider` | enum | LLM provider (`ollama`, `openai`) |
| `model` | string | Model name to use |
| `ollama_url` | string | Ollama API base URL |
| `openai_url` | string | OpenAI-compatible API base URL |
| `openai_api_key` | string | OpenAI API key |
| `tokenizer_mode` | enum | Token counting mode (`heuristic`, `full`) |
| `timeout_secs` | u64 | Timeout for LLM requests in seconds |
