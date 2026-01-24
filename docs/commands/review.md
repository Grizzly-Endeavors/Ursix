# review

Review code changes.

## Syntax

```bash
usx review [FILES...] [OPTIONS]
```

## Arguments

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `files` | string[] | No | Specific files to review |

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--diff` | string | None | Review specific git diff (e.g., `HEAD~1`, `branch-name`) |
| `--from` | path | None | Read input from a file (use `-` for stdin) |
| `--checks` | string[] | empty | Comma-separated checks to perform |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Input Sources (by precedence)

1. `--from FILE` - Read from specified file
2. Piped stdin - If no explicit input and stdin is piped
3. `--diff REF` - Review git diff against reference
4. `files...` - Review specific files
5. Default - Review staged changes (`git diff --cached`)

### Execution

1. Gathers diff or file content
2. Single LLM call for review
3. Returns issues and summary

### Checks (`--checks`)

Filter review to specific check types. Checks are loaded from `.ursix-rules.toml` if present.

Common check types:
- `style` - Code style and formatting
- `security` - Security vulnerabilities
- `performance` - Performance issues
- `correctness` - Potential bugs and logic errors
- `tests` - Test coverage

When `--checks` is specified, the review uses per-category LLM calls for more focused analysis.

### Chunked Mode (`--chunk`)

Processes files in parallel for large codebases:

```bash
usx review --chunk --max-concurrency 8
```

### Behavior Matrix

The combination of `--checks` and `--chunk` flags determines the execution mode:

| `--checks` | `--chunk` | Behavior | Example (3 cats, 4 files) |
|------------|-----------|----------|---------------------------|
| No | No | Single call, all rules | 1 call |
| No | Yes | File chunking only | 4 calls |
| Yes | No | Category chunking | 3 calls |
| Yes | Yes | Nested (categories × files) | 12 calls |

This gives you control over the cost/detail tradeoff:
- **No flags**: Broad review, single LLM call (cheapest)
- **`--checks` only**: Focused review per category (more calls, better focus)
- **`--chunk` only**: File-level parallelism for large codebases
- **Both flags**: Maximum detail with nested parallelism (most calls, most thorough)

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
usx review

# Human-readable output
usx review --text

# Review specific files
usx review src/main.rs src/cli.rs

# Review against a git reference
usx review --diff HEAD~1

# Review with specific checks
usx review --checks style,security

# Review piped diff
cat changes.diff | usx review

# Parallel processing for large repos
usx review --chunk --max-concurrency 8
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (no issues) |
| 1 | Issues found |
| 4 | Input error |
| 5 | Git error |
| 8 | Parse error |

## Integration

### CI Pipeline

```bash
# Fail CI if issues found (JSON is default)
usx review | jq -e '.passed'
```

### Pre-commit Hook

```bash
#!/bin/bash
usx review || exit 1
```

### Piping to Fix

```bash
# Review and fix in one pipeline (JSON is default)
usx review | usx fix src/ --from -
```
