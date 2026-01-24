# fix

Fix issues in code.

## Syntax

```bash
usx fix <target> [OPTIONS]
```

## Arguments

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `target` | string | Yes | Target file or directory |

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--lint` | boolean | false | Fix lint/clippy issues (runs `cargo clippy` first) |
| `--apply` | boolean | false | Apply fixes automatically without confirmation |
| `--agent` | boolean | false | Use agentic mode with full tool access |
| `--from` | path | None | Read issues from a file (use `-` for stdin) |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Issue Sources (by precedence)

1. `--from FILE` - Read issues from specified file
2. Piped stdin - If no explicit input and stdin is piped
3. `--lint` - Run `cargo clippy` to identify issues
4. Default - Code analysis only (no lint info)

### Pipeline Mode (default)

1. Gathers code context
2. Identifies issues from configured source
3. Single LLM call for fix suggestions
4. Optionally applies fixes with `--apply`

### Agent Mode (`--agent`)

1. Multi-turn agentic mode with full tool access
2. Can run clippy, explore code, edit files
3. Iteratively refines fixes
4. Useful for complex refactoring

### Fix Application (`--apply`)

Applies fixes automatically via string replacement:

```bash
usx fix src/main.rs --lint --apply
```

Each fix is applied by replacing the `original` code snippet with the `replacement`.

### Chunked Mode (`--chunk`)

Processes files in parallel for large codebases:

```bash
usx fix src/ --lint --chunk --max-concurrency 8
```

## Output

### Human Format

```
Diagnosis:
[diagnosis text]

Fixes:
1. src/main.rs:42
   Original: [code snippet]
   Fix: [replacement code]
   Reason: [explanation]

2. src/main.rs:67
   [...]

Applied: 2/2 fixes
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
| `file` | string | Path to file |
| `line` | number | Line number |
| `original` | string | Code snippet to replace |
| `replacement` | string | Replacement code |
| `explanation` | string | Why this fix is needed |

## Examples

```bash
# Analyze and suggest fixes
usx fix src/main.rs

# Fix clippy issues
usx fix src/main.rs --lint

# Auto-apply fixes
usx fix src/main.rs --apply

# Run clippy and auto-apply
usx fix src/main.rs --lint --apply

# Deep agentic fix
usx fix src/main.rs --agent

# Fix issues from review output (JSON is default)
usx review | usx fix src/main.rs --from -

# Parallel processing
usx fix src/ --lint --chunk --max-concurrency 8

# Human-readable output
usx fix src/main.rs --text
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success (all fixes applied or no issues) |
| 1 | Issues found (unfixable or apply failed) |
| 3 | Input error |
| 5 | Parse error |
| 6 | Agent limit exceeded (agent mode) |

## Integration

### Review to Fix Pipeline

```bash
# Find issues and fix them (JSON is default)
usx review | usx fix src/ --from - --apply
```

### Lint Fix Workflow

```bash
# Fix all clippy issues
usx fix . --lint --apply

# Verify fixes
cargo clippy
```
