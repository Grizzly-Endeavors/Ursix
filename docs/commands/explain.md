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

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--agent` | boolean | false | Use agentic mode for deep exploration |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Pipeline Mode (default)

1. Reads target file content
2. Gathers context (file metadata, structure)
3. Single LLM call for explanation
4. Returns structured explanation

### Agent Mode (`--agent`)

1. Multi-turn agentic exploration
2. Can explore related files
3. Uses tools for deeper analysis
4. Provides comprehensive explanation

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

### JSON Format (`--json`)

```json
{
  "explanation": "...",
  "parse_warning": "..."
}
```

## Examples

```bash
# Explain a file
usx explain src/main.rs

# Deep exploration with agent mode
usx explain src/cli.rs --agent

# Explain piped content
cat complex_function.rs | usx explain the_function

# JSON output for scripting
usx explain src/main.rs --json

# Use specific model
usx explain src/main.rs --model gpt-4
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 3 | Input error (file not found) |
| 5 | Parse error |
| 6 | Agent limit exceeded (agent mode) |
