# derive

Derive content from input. Unified command for generative output types.

## Synopsis

```bash
usx derive <TYPE> [--from FILE] [--style STYLE] [--chunk-recursive] [--max-concurrency N]
```

## Description

The `derive` command generates different types of content from input:

- **commit-msg**: Generate commit messages from diffs
- **explanation**: Generate explanations of code
- **summary**: Generate summaries of content

## Arguments

| Argument | Description |
|----------|-------------|
| `TYPE` | Content type to derive: `commit-msg`, `explanation`, `summary` |

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--from FILE` | path | stdin | Read input from file (use `-` for stdin) |
| `--style STYLE` | string | conventional | Commit style (only for commit-msg): `conventional`, `simple` |
| `--chunk-recursive` | boolean | false | Split large inputs into chunks and synthesize results |
| `--max-concurrency N` | usize | 4 | Maximum concurrent chunk executions |

## Derive Types

### commit-msg

Generate a commit message from a git diff.

```bash
git diff --staged | usx derive commit-msg
git diff --staged | usx derive commit-msg --style simple
```

### explanation

Generate an explanation of code.

```bash
cat src/main.rs | usx derive explanation
```

### summary

Generate a summary of content.

```bash
cat large_doc.md | usx derive summary
cat src/**/*.rs | usx derive summary --chunk-recursive
```

## Chunked Processing

For large inputs that exceed token limits, use `--chunk-recursive`:

```bash
# Process large codebase with chunking
cat src/**/*.rs | usx derive summary --chunk-recursive

# Estimate token usage without LLM calls
cat src/**/*.rs | usx derive summary --chunk-recursive --dry-run
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

# Explain code
cat src/lib.rs | usx derive explanation

# Summarize with chunking for large input
find src -name "*.rs" -exec cat {} \; | usx derive summary --chunk-recursive

# Dry run to estimate tokens
git diff --staged | usx derive commit-msg --dry-run

# Human-readable output
cat src/main.rs | usx derive explanation --text
```
