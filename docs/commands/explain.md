# explain

Explain code, files, or concepts.

## Syntax

```bash
usx explain <target> [OPTIONS]
```

## Arguments

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `target` | string | Yes | File path or concept to explain |

## Flags

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

1. Reads target file content
2. Gathers context (file metadata, structure)
3. Single LLM call for explanation
4. Returns structured explanation

### Stdin Support

If stdin is piped, uses piped content as file content instead of reading from filesystem:

```bash
cat file.rs | usx explain my_function
```

### Chunked Mode

Not supported. The `--chunk` flag is ignored as `explain` requires full context for coherent explanations.

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
# Explain a file (JSON output by default)
usx explain src/main.rs

# Human-readable output
usx explain src/main.rs --text

# Explain piped content
cat complex_function.rs | usx explain the_function

# Use specific model
usx explain src/main.rs --model gpt-4
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 4 | Input error (file not found) |
| 8 | Parse error |
