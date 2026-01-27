# fix

Transform code snippets using structured input. The fix command takes a specific issue description, code snippet, and location, then generates a unified diff with the fix.

## Syntax

```bash
echo '{"issue": "...", "snippet": "...", "file": "...", "lines": [...]}' | usx fix
usx fix --from input.json
```

## Input Schema

The fix command requires JSON input with the following structure:

```json
{
  "issue": "description of the problem to fix",
  "snippet": "exact code lines to transform",
  "file": "path/to/file.rs",
  "lines": [start_line, end_line]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `issue` | string | yes | Description of the problem to fix |
| `snippet` | string | yes | Exact code to transform (must match file content) |
| `file` | string | yes | Path to the file containing the code |
| `lines` | [number, number] | yes | Line range [start, end], 1-indexed, inclusive |

The snippet must exactly match the content of the file at the specified lines (trailing whitespace is normalized for comparison).

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--from` | PATH | - | Read input from file (use `-` for stdin) |
| `--context` | number | 3 | Number of context lines in unified diff output |
| `--retry` | boolean | false | Retry on validation failure with error context |
| `--partial` | boolean | false | Return raw LLM output on validation failure |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Input Validation

Before making an LLM call, the command validates:
1. JSON structure (all required fields present)
2. File exists and is readable
3. Line range is valid (start <= end, within file bounds, 1-indexed)
4. Snippet matches file content at specified lines

### LLM Transformation

The LLM receives only the issue description and code snippet. It outputs the replacement code directly (no JSON, no explanation). This constrained approach:
- Prevents location ambiguity (user specifies exact location)
- Enables programmatic diff generation
- Allows output validation before applying

### Output Validation

The replacement is validated before generating the diff:
- Length sanity check (reject if >10x original length)
- Balanced delimiter warning (unclosed brackets)
- Common LLM mistake detection (code fences, explanations)

### Retry Mode (`--retry`)

When validation fails and `--retry` is enabled:
1. Error context is added to a second LLM call
2. Second attempt is validated
3. If still invalid, returns error

### Partial Mode (`--partial`)

When validation fails and `--partial` is enabled:
- Error output includes raw LLM response for inspection
- Useful for debugging prompt issues

## Output

### Success (JSON, default)

```json
{
  "diff": "--- a/src/main.rs\n+++ b/src/main.rs\n@@ -10,3 +10,3 @@...",
  "file": "src/main.rs",
  "lines": [10, 12],
  "lines_added": 1,
  "lines_removed": 2,
  "warnings": []
}
```

### Success (Human, `--text`)

```
Fixed src/main.rs (lines 10..12)
+1 -2 lines

--- a/src/main.rs
+++ b/src/main.rs
@@ -10,3 +10,3 @@
-    let x = unwrap();
+    let x = value.ok_or(Error::Missing)?;
```

### Error (JSON)

```json
{
  "message": "file does not exist: nonexistent.rs",
  "code": "input_validation",
  "warnings": []
}
```

Error codes:
- `input_validation` - Bad input JSON, missing file, snippet mismatch
- `llm_error` - LLM call failed (network, timeout)
- `output_validation` - LLM output failed validation

## Examples

```bash
# Fix an unused variable
cat <<'EOF' | usx fix
{
  "issue": "unused variable",
  "snippet": "    let x = calculate();",
  "file": "src/main.rs",
  "lines": [42, 42]
}
EOF

# Read input from file
echo '{"issue": "...", ...}' > fix-input.json
usx fix --from fix-input.json

# Get more context in diff
cat input.json | usx fix --context 5

# Debug with partial output
cat input.json | usx fix --partial

# Retry on failure
cat input.json | usx fix --retry

# Dry run to validate input without LLM call
cat input.json | usx fix --dry-run

# Apply the fix using patch
usx fix --from input.json | jq -r '.diff' | patch -p1
```

## Exit Codes

| Code | Name | Meaning |
|------|------|---------|
| 0 | Success | Fix generated successfully |
| 2 | UserError | Input validation failed (bad JSON, file not found, snippet mismatch) |
| 3 | TransientError | LLM call failed (network, rate limit) |
| 4 | PermanentError | Output validation failed (LLM returned invalid code) |

## Integration with Linters

The fix command works well with linter output. Extract issues programmatically and pipe to fix:

```bash
# Example: Extract clippy warning and fix it
# (Would require a script to parse clippy output into fix input JSON)
cargo clippy --message-format=json 2>&1 \
  | jq 'select(.reason=="compiler-message") | ...' \
  | usx fix
```

## Comparison with Review

| Aspect | fix | review |
|--------|-----|--------|
| Input | Structured JSON with exact location | Raw diff or code |
| Scope | Single snippet transformation | Whole-file analysis |
| Output | Unified diff | Issue list with suggestions |
| Use case | Apply specific fixes | Identify potential issues |
