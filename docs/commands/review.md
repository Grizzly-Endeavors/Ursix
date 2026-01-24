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
| `--agent` | boolean | false | Use agentic mode for thorough multi-file analysis |
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

### Pipeline Mode (default)

1. Gathers diff or file content
2. Single LLM call for review
3. Returns issues and summary

### Agent Mode (`--agent`)

1. Multi-turn analysis
2. Can explore related files for context
3. Uses tools for thorough review
4. Provides comprehensive analysis

### Checks (`--checks`)

Filter review to specific check types. Checks are loaded from `.ursix-rules.toml` if present.

Common check types:
- `style` - Code style and formatting
- `security` - Security vulnerabilities
- `performance` - Performance issues
- `bugs` - Potential bugs
- `tests` - Test coverage

### Chunked Mode (`--chunk`)

Processes files in parallel for large codebases:

```bash
usx review --chunk --max-concurrency 8
```

## Output

### Human Format

```
Review Summary:
[summary text]

Issues:
- [severity] file.rs:42 - Description
- [severity] file.rs:67 - Description
```

### JSON Format (`--json`)

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
# Review staged changes
usx review

# Review specific files
usx review src/main.rs src/cli.rs

# Review against a git reference
usx review --diff HEAD~1

# Review with specific checks
usx review --checks style,security

# Deep analysis with agent mode
usx review --agent

# Review piped diff
cat changes.diff | usx review

# Parallel processing for large repos
usx review --chunk --max-concurrency 8

# JSON output for CI integration
usx review --json
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (no issues) |
| 1 | Issues found |
| 3 | Input error |
| 4 | Git error |
| 5 | Parse error |
| 6 | Agent limit exceeded (agent mode) |

## Integration

### CI Pipeline

```bash
# Fail CI if issues found
usx review --json | jq -e '.passed'
```

### Pre-commit Hook

```bash
#!/bin/bash
usx review || exit 1
```

### Piping to Fix

```bash
# Review and fix in one pipeline
usx review --json | usx fix src/ --from -
```
