# commit

Generate commit messages from staged changes.

## Syntax

```bash
usx commit [OPTIONS]
```

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--body` | boolean | false | Include body with detailed explanation |
| `--style` | string | conventional | Commit style (`conventional`, `simple`) |
| `--execute` | boolean | false | Auto-execute `git commit` with generated message |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Pipeline Mode Only

The `commit` command always runs in pipeline mode. Agent mode (`--agent`) is not available as commit generation is a simple single-pass operation.

### Input Sources

1. Piped stdin - If stdin is piped, uses piped content as diff
2. Default - Reads staged changes via `git diff --cached`

### Commit Styles

#### Conventional (default)

Follows [Conventional Commits](https://www.conventionalcommits.org/) format:

```
type(scope): subject

[optional body]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`

#### Simple

Plain single-line format:

```
Subject line describing the change
```

### Execute Mode (`--execute`)

Automatically runs `git commit -m "<message>"` with the generated message:

```bash
usx commit --execute
```

### Chunked Mode

Not supported. The `--chunk` flag is ignored as commit message generation requires full context of all staged changes.

## Output

### Human Format

```
Commit Message:
feat(cli): add review command with configurable checks

[optional body if --body]
```

### JSON Format (default)

```json
{
  "message": "feat(cli): add review command with configurable checks",
  "body": "Detailed explanation of the changes..."
}
```

## Examples

```bash
# Generate conventional commit message (JSON output by default)
usx commit

# Human-readable output
usx commit --text

# Include detailed body
usx commit --body

# Use simple style
usx commit --style simple

# Generate and execute commit
usx commit --execute

# Conventional with body and execute
usx commit --body --execute

# Simple style with execute
usx commit --style simple --execute

# Generate from piped diff
cat staged.diff | usx commit --body
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 4 | Git error (no staged changes, commit failed) |
| 5 | Parse error |

## Integration

### Git Alias

Add to `.gitconfig`:

```gitconfig
[alias]
    ai = !usx commit --execute
    aim = !usx commit --body --execute
```

### Pre-commit Workflow

```bash
# Stage changes
git add -p

# Generate and review message
usx commit

# Execute if satisfied
usx commit --execute
```

### CI Commit

```bash
# Generate message for CI commits (JSON is default)
MESSAGE=$(usx commit | jq -r '.message')
git commit -m "$MESSAGE"
```
