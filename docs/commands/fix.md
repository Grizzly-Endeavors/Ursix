# fix

Suggest fixes for issues in code from stdin or a file.

## Syntax

```bash
usx fix [OPTIONS]
cat file.rs | usx fix
```

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--from` | PATH | - | Read input from file (use `-` for stdin) |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Input Sources

1. Piped stdin - Default input method
2. `--from FILE` - Read from specified file
3. `--from -` - Explicitly read from stdin

### Execution

1. Reads code (optionally with lint output) from stdin or `--from`
2. Single LLM call for fix suggestions
3. Returns suggested fixes

### Chunked Mode (`--chunk`)

Not supported in stdin mode. The `--chunk` flag issues a warning.

## Output

### Human Format

```
Diagnosis:
[diagnosis text]

Fixes:
1. Line 42
   Original: [code snippet]
   Fix: [replacement code]
   Reason: [explanation]

2. Line 67
   [...]
```

### JSON Format (default)

```json
{
  "diagnosis": "...",
  "fixes": [
    {
      "file": "src/main.rs",
      "line": 42,
      "original": "let x = unwrap()",
      "replacement": "let x = ok_or(...)?",
      "explanation": "Replace unwrap with proper error handling"
    }
  ],
  "unfixable_count": 0,
  "parse_warning": "..."
}
```

### Fix Structure

Each fix contains:

| Field | Type | Description |
|-------|------|-------------|
| `file` | string | Path to file (if provided in input) |
| `line` | number | Line number |
| `original` | string | Code snippet to replace |
| `replacement` | string | Replacement code |
| `explanation` | string | Why this fix is needed |

## Examples

```bash
# Analyze code and suggest fixes
cat src/main.rs | usx fix

# Read from file directly
usx fix --from src/main.rs

# Combine code with lint errors for smarter fixes
{ cat src/lib.rs; echo "---"; cargo clippy 2>&1; } | usx fix

# Human-readable output
cat src/main.rs | usx fix --text

# Use specific model
cat src/main.rs | usx fix --model gpt-4
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (no issues found) |
| 1 | Issues found |
| 2 | User error (empty input) |
| 4 | Permanent error (parse error) |
