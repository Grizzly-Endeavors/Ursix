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
| `--chunk` | boolean | false | Enable chunked processing for large inputs |
| `--max-concurrency` | usize | 4 | Maximum concurrent chunk executions |
| `--tokenizer` | enum | heuristic | Tokenizer mode (`heuristic`, `full`) |
| `--no-retry` | boolean | false | Disable automatic retry on transient failures |
| `--max-retries` | u32 | 3 | Maximum retry attempts for transient failures |
| `--partial` | boolean | false | Return partial results when some chunks fail |

### Environment Variables

| Variable | Description |
|----------|-------------|
| `URSIX_OPENAI_API_KEY` | API key for OpenAI-compatible endpoints |

## Commands

- [`explain`](commands/explain.md) - Explain code, files, or concepts
- [`review`](commands/review.md) - Review code changes
- [`fix`](commands/fix.md) - Fix issues in code
- [`commit`](commands/commit.md) - Generate commit messages
- [`config`](commands/config.md) - View configuration

## Execution Mode

Single LLM call with pre-gathered context. No tools, no message history.

- Fast and predictable
- Suitable for all tasks
- Stateless: same input always produces consistent output

## Chunked Processing

Enable with `--chunk` flag (available on `review`, `fix`).

- Splits input by file boundaries
- Processes files in parallel
- Controlled by `--max-concurrency`
- Aggregates results from all chunks
- Useful for large codebases

**Not supported on:** `explain`, `commit` (require full context)

### Category Chunking (review only)

When using `--checks` with the review command, each category is processed with a separate LLM call:

| `--checks` | `--chunk` | Behavior |
|------------|-----------|----------|
| No | No | Single call, all rules |
| No | Yes | File chunking only |
| Yes | No | Category chunking (one call per category) |
| Yes | Yes | Nested (categories × files) |

This allows you to control the cost/detail tradeoff. See [review command docs](commands/review.md) for details.

## Exit Codes

Simplified 5-category system for reliable automation:

| Code | Name | Meaning |
|------|------|---------|
| 0 | Success | Command completed successfully with no issues |
| 1 | IssuesFound | Command completed but found issues (review problems, partial failures) |
| 2 | UserError | User-fixable errors: bad arguments, config, missing files |
| 3 | TransientError | Transient errors: network issues, rate limits (retry may help) |
| 4 | PermanentError | Permanent errors: auth failures, parse errors (retry won't help) |

Scripts can use these to make retry decisions:
```bash
usx review
case $? in
  0) echo "Clean" ;;
  1) echo "Issues found" ;;
  2) echo "Check your arguments" ;;
  3) echo "Retry later" ;;
  4) echo "Fix configuration" ;;
esac
```

## Error Handling & Retry

### Automatic Retry

By default, Ursix automatically retries LLM calls on transient failures (network errors, rate limits). This improves reliability without user intervention.

```bash
# Default: retries up to 3 times with exponential backoff
usx review

# Disable retry (for debugging or when you want fast failures)
usx review --no-retry

# Custom retry count
usx review --max-retries 5
```

### Partial Results

When using chunked processing (`--chunk`), some chunks may fail while others succeed. By default, failures cause the entire command to fail. Use `--partial` to get partial results:

```bash
# Default: fail if any chunk fails
usx review --chunk

# Return successful results even if some chunks failed
usx review --chunk --partial
```

With `--partial`, the output includes:
- `partial_results` — data from successful chunks
- `chunks_processed` — list of successfully processed chunks
- `chunks_failed` — list of failed chunks with error details
- `partial: true` — flag indicating this is a partial result

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

## Input Handling

For commands with multiple input methods, precedence is:

1. Explicit file input (`--from FILE`)
2. Piped stdin (if no explicit input and stdin is piped)
3. Command arguments (positional, `--diff`, files)
4. Default behavior (git context, filesystem scan)
