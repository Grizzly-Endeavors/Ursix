# derive

Derive content from input. Unified command for generative output types.

## Synopsis

```bash
usx derive <TYPE> [FILE] [--chunk] [--concurrency N]
```

## Description

The `derive` command generates different types of content from input:

- **commit-msg**: Generate conventional commit messages from diffs
- **explanation**: Generate explanations of code
- **summary**: Generate summaries of content

## Arguments

| Argument | Description |
|----------|-------------|
| `TYPE` | Content type to derive: `commit-msg`, `explanation`, `summary` |
| `FILE` | Input file (omit for stdin, use `-` for explicit stdin) |

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--chunk` | boolean | false | Split large inputs into chunks and synthesize results |
| `--concurrency` | usize | 4 | Max concurrent chunk executions |

Plus all [global flags](../cli-reference.md#global-flags).

## Derive Types

### commit-msg

Generate a conventional commit message from a git diff.

```bash
git diff --staged | usx derive commit-msg
```

### explanation

Generate an explanation of code.

```bash
cat src/main.rs | usx derive explanation
usx derive explanation src/main.rs
```

### summary

Generate a summary of content.

```bash
cat large_doc.md | usx derive summary
cat src/**/*.rs | usx derive summary --chunk
```

## Chunked Processing

For large inputs that exceed token limits, use `--chunk`:

```bash
# Process large codebase with chunking
cat src/**/*.rs | usx derive summary --chunk

# Estimate token usage without LLM calls
cat src/**/*.rs | usx derive summary --chunk --dry-run
```

The chunked workflow:
1. Split input into ~4k token chunks
2. Process each chunk independently
3. Synthesize results into final output

## Output Format

### JSON (default)

```json
// commit-msg
{
  "type": "commit_msg",
  "message": "feat: add user authentication",
  "title": "feat: add user authentication",
  "body": "Implement login/logout with session management"
}

// explanation
{
  "type": "explanation",
  "explanation": "This code implements..."
}

// summary
{
  "type": "summary",
  "summary": "A high-level overview of..."
}
```

### Text (--text)

Returns the raw content (message, explanation, or summary) without JSON wrapper.

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | User error (invalid type, empty input) |
| 3 | Transient error (network, rate limit) |
| 4 | Permanent error (auth, parse failure) |

## Examples

```bash
# Generate commit message
git diff --staged | usx derive commit-msg

# Explain code (file argument)
usx derive explanation src/lib.rs

# Explain code (stdin)
cat src/lib.rs | usx derive explanation

# Summarize with chunking for large input
find src -name "*.rs" -exec cat {} \; | usx derive summary --chunk

# Dry run to estimate tokens
git diff --staged | usx derive commit-msg --dry-run

# Human-readable output
usx derive explanation src/main.rs --text
```
