# review

Review code changes from stdin or a file.

## Syntax

```bash
usx review [OPTIONS]
git diff | usx review
```

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--from` | PATH | - | Read input from file (use `-` for stdin) |
| `--checks` | string[] | empty | Comma-separated checks to perform |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Input Sources

1. Piped stdin - Default input method
2. `--from FILE` - Read from specified file
3. `--from -` - Explicitly read from stdin

### Execution

1. Reads code or diff from stdin or `--from`
2. Single LLM call for review
3. Returns issues and summary

### Checks (`--checks`)

Filter review to specific check types. Checks are loaded from `rules.yml` or `.ursix/rules.yml` if present.

Common check types:
- `style` - Code style and formatting
- `security` - Security vulnerabilities
- `performance` - Performance issues
- `correctness` - Potential bugs and logic errors
- `tests` - Test coverage

### Chunked Mode (`--chunk`)

Not supported in stdin mode. The `--chunk` flag issues a warning.

## Output

### Human Format

```
Review Summary:
[summary text]

Issues:
- [severity] file.rs:42 - Description
- [severity] file.rs:67 - Description
```

### JSON Format (default)

```json
{
  "summary": "...",
  "issues": [
    {
      "file": "src/main.rs",
      "line": 42,
      "severity": "warning",
      "message": "..."
    }
  ],
  "passed": false,
  "parse_warning": "..."
}
```

## Examples

```bash
# Review staged changes (JSON output by default)
git diff --staged | usx review

# Human-readable output
git diff --staged | usx review --text

# Review a specific diff range
git diff HEAD~3 | usx review

# Review with specific checks
git diff | usx review --checks style,security

# Read from file
usx review --from changes.diff
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (no issues) |
| 1 | Issues found |
| 2 | User error (empty input) |
| 4 | Permanent error (parse error) |

## Integration

### CI Pipeline

```bash
# Fail CI if issues found (JSON is default)
git diff origin/main...HEAD | usx review | jq -e '.passed'
```

### Pre-commit Hook

```bash
#!/bin/bash
git diff --staged | usx review || exit 1
```
