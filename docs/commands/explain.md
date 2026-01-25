# explain

Explain code from stdin or a file.

## Syntax

```bash
usx explain [OPTIONS]
cat file.rs | usx explain
```

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--from` | PATH | - | Read input from file (use `-` for stdin) |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

1. Reads code from stdin or `--from` file
2. Single LLM call for explanation
3. Returns structured explanation

### Chunked Mode

Not supported. The `--chunk` flag issues a warning as `explain` requires full context for coherent explanations.

## Output

### Human Format

```
Explanation:
[formatted explanation text]
```

### JSON Format (default)

```json
{
  "explanation": "...",
  "parse_warning": "..."
}
```

## Examples

```bash
# Explain a file via pipe (JSON output by default)
cat src/main.rs | usx explain

# Read from file directly
usx explain --from src/main.rs

# Human-readable output
cat src/main.rs | usx explain --text

# Use specific model
cat src/main.rs | usx explain --model gpt-4
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | User error (empty input, file not found) |
| 4 | Permanent error (parse error) |
