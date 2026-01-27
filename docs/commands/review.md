# review

Review code changes from stdin or a file.

## Syntax

```bash
usx review [FILE] [OPTIONS]
git diff | usx review
```

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | Input file (omit for stdin, use `-` for explicit stdin) |

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--checks` | string[] | empty | Comma-separated checks to perform |
| `--chunk` | boolean | false | Split by file, process in parallel |
| `--concurrency` | usize | 4 | Max concurrent chunk executions |
| `--partial` | boolean | false | Continue when some chunks fail |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Supported Input Types

The review command accepts two types of input:

1. **Diff format** (stdin or file) - Unified diff format is auto-detected
2. **Single file** (positional argument) - Review a specific file

**Not supported:**
- Arbitrary text piped via stdin (use `usx derive explanation` instead)

If you pipe non-diff content via stdin without a file argument, you'll see:
```
error: review expects diff input or FILE; use 'usx derive explanation' for arbitrary text
```

### Execution

1. Reads code or diff from stdin or file argument
2. Validates input type (diff format or single file)
3. Single LLM call for review (or multiple calls with `--chunk`)
4. Returns issues and summary

### Checks (`--checks`)

Filter review to specific check types. Checks are loaded from `rules.yml` or `.ursix/rules.yml` if present.

Common check types:
- `style` - Code style and formatting
- `security` - Security vulnerabilities
- `performance` - Performance issues
- `correctness` - Potential bugs and logic errors
- `tests` - Test coverage

### Chunked Mode (`--chunk`)

When `--chunk` is enabled with diff input:

1. The diff is split at file boundaries (`diff --git` markers)
2. Each file is processed independently with bounded concurrency
3. Results are aggregated into a single response

**Requirements:**
- Input must be in diff format (auto-detected)
- Only works with stdin or diff files, not single source files

**Limitations:**
- Single files via `--from` cannot be chunked (falls back to single-pass with warning)
- Non-diff stdin content with `--chunk` produces an error

### Partial Mode (`--partial`)

When combined with `--chunk`, the `--partial` flag allows processing to continue when some file chunks fail:

- Failed chunks are recorded in `chunk_failures` array
- Successful results are still returned
- Exit code reflects whether any issues were found in successful chunks

Without `--partial`, the first chunk failure stops all processing.

## Output

### Human Format

```
Processed 3 file(s)

Review Summary:
[summary text]

Issues (2):
  [warning] src/main.rs:42 - Description
  [error] src/lib.rs:67 - Description
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
  "chunks_processed": 3,
  "chunk_failures": []
}
```

When chunks fail with `--partial`:

```json
{
  "summary": "Reviewed 3 file(s) (2 succeeded, 1 failed)",
  "issues": [...],
  "passed": false,
  "chunks_processed": 3,
  "chunk_failures": [
    {
      "file_path": "src/broken.rs",
      "error": "LLM timeout"
    }
  ]
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

# Read from file (positional argument)
usx review changes.diff

# Review a single source file
usx review src/main.rs

# Chunked review of large diff (parallel processing)
git diff HEAD~10 | usx review --chunk

# Chunked review with custom concurrency
git diff | usx review --chunk --concurrency 8

# Continue on partial failures
git diff | usx review --chunk --partial

# Dry-run to see chunk plan
git diff | usx review --chunk --dry-run
```

### Invalid Usage (Errors)

```bash
# Error: arbitrary stdin is not supported
echo "fn main() {}" | usx review
# error: review expects diff input or FILE; use 'usx derive explanation' for arbitrary text

# Error: --chunk requires diff input
usx review src/main.rs --chunk
# warning: chunking single files not supported; processing as single-pass
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (no issues) |
| 1 | Issues found |
| 2 | User error (empty input, invalid input type) |
| 3 | Transient error (network, timeout) |
| 4 | Permanent error (parse error) |

## Integration

### CI Pipeline

```bash
# Fail CI if issues found (JSON is default)
git diff origin/main...HEAD | usx review | jq -e '.passed'

# Chunked review for large PRs
git diff origin/main...HEAD | usx review --chunk | jq -e '.passed'
```

### Pre-commit Hook

```bash
#!/bin/bash
git diff --staged | usx review || exit 1
```

### Large Diff Review

```bash
#!/bin/bash
# Review large diffs with chunking, continue on partial failures
git diff HEAD~20 | usx review --chunk --partial --text
```
