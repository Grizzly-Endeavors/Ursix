# commit

Generate commit messages from diff provided via stdin or file.

## Syntax

```bash
usx commit [OPTIONS]
git diff --staged | usx commit
```

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--from` | PATH | - | Read diff from file (use `-` for stdin) |
| `--body` | boolean | false | Include body with detailed explanation |
| `--style` | string | conventional | Commit style (`conventional`, `simple`) |
| `--execute` | boolean | false | Auto-execute `git commit` with generated message |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Input Sources

1. Piped stdin - Default input method
2. `--from FILE` - Read from specified file
3. `--from -` - Explicitly read from stdin

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
git diff --staged | usx commit --execute
```

### Chunked Mode

Not supported. The `--chunk` flag issues a warning as commit message generation requires full context of all changes.

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
git diff --staged | usx commit

# Human-readable output
git diff --staged | usx commit --text

# Include detailed body
git diff --staged | usx commit --body

# Use simple style
git diff --staged | usx commit --style simple

# Generate and execute commit
git diff --staged | usx commit --execute

# Conventional with body and execute
git diff --staged | usx commit --body --execute

# Read from file
usx commit --from staged.diff
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | User error (empty input) |
| 4 | Permanent error (git commit failed, parse error) |

## Integration

### Git Alias

Add to `.gitconfig`:

```gitconfig
[alias]
    ai = "!git diff --staged | usx commit --execute"
    aim = "!git diff --staged | usx commit --body --execute"
```

### Pre-commit Workflow

```bash
# Stage changes
git add -p

# Generate and review message
git diff --staged | usx commit

# Execute if satisfied
git diff --staged | usx commit --execute
```

### CI Commit

```bash
# Generate message for CI commits (JSON is default)
MESSAGE=$(git diff HEAD~1 | usx commit | jq -r '.message')
git commit -m "$MESSAGE"
```
