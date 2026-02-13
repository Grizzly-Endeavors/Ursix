# fix

Transform code snippets using structured input. The fix command takes a specific issue description and location, then generates a unified diff with the fix.

> **Experimental — not recommended for production use.**
>
> Generated code is not validated for compilation, type correctness, or test compatibility. Always review diffs before applying.

## Syntax

```bash
echo '{"issue": "...", "file": "...", "lines": [...]}' | usx fix
usx fix input.json
usx fix --mode whole-file input.json
```

## Modes

### Atomic Mode (default)

Fix a single issue at a specific location. The snippet is automatically inferred from the file content at the specified lines.

**Input Schema:**
```json
{
  "issue": "description of the problem to fix",
  "file": "path/to/file.rs",
  "lines": [start_line, end_line]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `issue` | string | yes | Description of the problem to fix |
| `file` | string | yes | Path to the file containing the code |
| `lines` | [number, number] | yes | Line range [start, end], 1-indexed, inclusive |

### Whole-File Mode (`--mode whole-file`)

Fix multiple issues in a single file. Issues are processed bottom-to-top to preserve line numbers as fixes are applied.

**Input Schema:**
```json
{
  "file": "path/to/file.rs",
  "issues": [
    {"issue": "unused var", "lines": [10, 10]},
    {"issue": "missing error handling", "lines": [25, 28]}
  ]
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `file` | string | yes | Path to the file containing the code |
| `issues` | array | yes | List of issues to fix |
| `issues[].issue` | string | yes | Description of the problem |
| `issues[].lines` | [number, number] | yes | Line range [start, end], 1-indexed, inclusive |

**Constraints:**
- Issues list must not be empty
- Line ranges must not overlap (adjacent ranges are OK)
- All line ranges must be within file bounds

## Arguments

| Argument | Description |
|----------|-------------|
| `FILE` | JSON input file (omit for stdin, use `-` for explicit stdin) |

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--mode` | `atomic` \| `whole-file` | `atomic` | Fix mode |
| `--context` | number | 3 | Diff context lines |
| `--retry` | boolean | false | Retry on validation failure with error context |
| `--partial` | boolean | false | Return partial results on failure |

Plus all [global flags](../cli-reference.md#global-flags).

## Behavior

### Input Validation

Before making an LLM call, the command validates:
1. JSON structure (all required fields present)
2. File exists and is readable
3. Line range is valid (start <= end, within file bounds, 1-indexed)
4. For whole-file mode: no overlapping line ranges

### Snippet Inference

The snippet to fix is always inferred from the file content at the specified lines. This ensures:
- No mismatch between provided snippet and actual file content
- Simpler input contract
- Reliable diff generation

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

**Atomic mode:** When validation fails, error output includes raw LLM response for inspection.

**Whole-file mode:** Continue processing remaining issues when some fail. Result includes both successful fixes and failure details.

### Whole-File Processing Order

Issues are processed bottom-to-top (by descending line number). This ensures earlier line numbers remain valid as later lines are modified.

## Output

### Atomic Mode Success (JSON, default)

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

### Whole-File Mode Success (JSON, default)

```json
{
  "diff": "--- a/src/main.rs\n+++ b/src/main.rs\n@@ ...",
  "file": "src/main.rs",
  "issues_fixed": 3,
  "issues_failed": 0,
  "lines_added": 5,
  "lines_removed": 8,
  "issue_failures": []
}
```

### Whole-File Mode Partial Success (with `--partial`)

```json
{
  "diff": "--- a/src/main.rs\n+++ b/src/main.rs\n@@ ...",
  "file": "src/main.rs",
  "issues_fixed": 2,
  "issues_failed": 1,
  "lines_added": 3,
  "lines_removed": 4,
  "issue_failures": [
    {
      "issue": "fix complex logic",
      "lines": [50, 55],
      "error": "validation failed: replacement too long"
    }
  ]
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
- `input_validation` - Bad input JSON, missing file, overlapping ranges
- `llm_error` - LLM call failed (network, timeout)
- `output_validation` - LLM output failed validation

## Limitations

- **No semantic validation** — does not check that generated code compiles, passes type checks, or is test-compatible
- **No cross-file context** — each fix is isolated to the specified lines in a single file
- **Length limit** — replacements longer than 10x the original snippet are rejected, which may prevent legitimate refactorings
- **Single-pass only** — no iterative refinement; the LLM gets one attempt (or two with `--retry`)
- **Indentation preservation** — depends on LLM behavior; whitespace may shift
- **Whole-file mode ordering** — if an intermediate fix fails, remaining line numbers may be misaligned

## When to Use Fix

**Good for:**
- Exploring potential fixes interactively
- Learning how an issue might be addressed
- Prototyping quick experiments

**Not for:**
- Production CI/CD automation — fixes are not validated for correctness
- Batch fixing across a codebase — no cross-file awareness
- Refactoring — length limits and single-pass design are too constraining

**Recommendation:** Use `usx review` to find issues, then fix them manually or with language-specific tooling (e.g., `cargo fix`, `eslint --fix`). Use `usx fix` for exploration, not automation.

## Examples

### Atomic Mode

```bash
# Fix an unused variable
cat <<'EOF' | usx fix
{
  "issue": "unused variable",
  "file": "src/main.rs",
  "lines": [42, 42]
}
EOF

# Read input from file (positional argument)
echo '{"issue": "...", ...}' > fix-input.json
usx fix fix-input.json

# Get more context in diff
usx fix input.json --context 5

# Debug with partial output
usx fix input.json --partial

# Retry on failure
usx fix input.json --retry

# Dry run to validate input without LLM call
usx fix input.json --dry-run

# Apply the fix using patch
usx fix input.json | jq -r '.diff' | patch -p1
```

### Whole-File Mode

```bash
# Fix multiple issues in one file
cat <<'EOF' | usx fix --mode whole-file
{
  "file": "src/main.rs",
  "issues": [
    {"issue": "unused variable", "lines": [10, 10]},
    {"issue": "missing error handling", "lines": [25, 28]},
    {"issue": "inefficient loop", "lines": [50, 55]}
  ]
}
EOF

# Continue on failures
usx fix input.json --mode whole-file --partial

# Dry run to see processing plan
usx fix input.json --mode whole-file --dry-run
```

## Exit Codes

| Code | Name | Meaning |
|------|------|---------|
| 0 | Success | Fix generated successfully |
| 2 | UserError | Input validation failed (bad JSON, file not found, overlapping ranges) |
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

For multiple issues in one file, use whole-file mode:

```bash
# Collect all issues for a file and fix them together
cargo clippy --message-format=json 2>&1 \
  | jq -s '[.[] | select(.file == "src/main.rs")] | {file: "src/main.rs", issues: .}' \
  | usx fix --mode whole-file
```

## Comparison with Review

| Aspect | fix | review |
|--------|-----|--------|
| Input | Structured JSON with exact location | Raw diff or code |
| Scope | Single snippet or multiple issues in one file | Whole-file/diff analysis |
| Output | Unified diff | Issue list with suggestions |
| Use case | Apply specific fixes | Identify potential issues |
