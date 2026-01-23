# Ursix TODO

Issues and improvements needed before the core feature set is production-ready.

## Guiding Principles

- **Stateless execution**: Every invocation is a single-pass operation, even in agent mode. No session state, no back-and-forth. Agent mode just means the LLM can use tools within that single invocation.
- **Unix-style contracts**: Same command + same flags = same output structure. Scripts should be able to rely on output format.
- **No silent failures**: If something doesn't work, the user must know. Warnings in logs are not sufficient.
- **Lean over feature-rich**: Remove half-baked features rather than ship them incomplete.

---

## High Priority

### Deprecate `ask` command

The `ask` command doesn't fit the Unix utility model. It's a general-purpose LLM query which belongs in a chat interface, not a development CLI. Each command should have a specific, well-defined purpose.

**Action**: Remove the `ask` command entirely.

### Agent mode breaks output contracts

When using `--agent`, structured commands (`review`, `commit`, `fix`, `explain`) return `AskResult` (freeform text) instead of their proper structured types.

Example:
```bash
# Pipeline mode - structured output
usx review --json
# {"summary": "...", "issues": [...], "passed": true}

# Agent mode - loses structure
usx review --agent --json
# {"response": "Here's what I found...", "turns": 1}
```

This breaks the Unix promise. Scripts parsing `issues` fail if someone adds `--agent`.

**Action**: Agent mode must produce the same structured output as pipeline mode. The agent should be instructed to respond in the command's JSON schema, then parsed the same way.

### `--agent` flag doesn't make sense on all commands

- `commit`: Just needs staged diff. What would the agent do with tools?
- `explain`: Agent can explore related files - makes sense
- `review`: Agent can run linters, read context - makes sense
- `fix`: Agent can run diagnostics, apply fixes - makes sense

**Action**: Remove `--agent` from `commit`. Consider whether it belongs on a per-command basis rather than as a global pattern.

### `fix --apply` fails silently

The apply logic uses `content.contains(&fix.original)`. If the LLM's suggested original code doesn't exactly match the file (common), it logs a warning but the user sees nothing.

**Action**:
- Track which fixes succeeded/failed
- Report results to user in both human and JSON output
- Return appropriate exit code (1 if any fixes failed to apply)

### `ReviewResult.render_human()` drops issues

Human mode only prints the summary. All structured issues are lost. Compare to `FixResult.render_human()` which properly shows all fixes.

**Action**: Human output should display issues in a readable format (file:line, severity, message).

### Parse failures silently report success

When LLM JSON parsing fails:
- `parse_review_response` returns `passed: true` with no issues (cli.rs:676-680)
- `parse_explain_response` returns raw response with no indication of failure (cli.rs:601-603)
- `parse_fix_response` logs to tracing but returns empty fixes to user (cli.rs:741-748)

Users get seemingly valid output with no indication the LLM response was malformed.

**Action**: Parse failures must be reported to users, not silently converted to "success" results.

### `models` command returns success on error

When `list_models()` fails, the error is printed to stderr but exit code is still 0 (cli.rs:987-1000).

**Action**: Return `ExitCode::Error` when the operation fails.

### Output bypasses render system in JSON mode

Several places print directly to stdout instead of going through the render system:
- Git commit success output (cli.rs:514) - `println!("{stdout}")`
- Config help text (cli.rs:972-973) - `println!("Usage: ...")`

In `--json` mode, this mixes raw text with JSON, breaking parsers.

**Action**: All output must go through the render system. Create appropriate result types for these cases.

### JSON serialization fallback masks errors

`render_json()` silently returns `{}` if serialization fails (output.rs:59). Scripts expecting specific structure get malformed data.

**Action**: JSON serialization failures should be actual errors, not silent fallbacks.

---

## Medium Priority

### `turns: 1` is always hardcoded

Every agent result has `turns: 1` even though agents can run multiple turns. The `AgentResult` struct has `turns_completed` but it's never propagated to output.

**Action**: Either track and report actual turn count, or remove the field entirely.

### `--verbose` flag is dead

Defined in CLI but never used.

**Action**: Either wire it to control tracing output level, or remove it.

### `models` command is provider-specific

Only works for Ollama. For OpenAI it prints "see docs".

**Action**: Remove the command. It's not core functionality and doesn't work consistently.

### `explain` JSON schema mismatch

Pipeline prompt asks for `summary`, `explanation`, `key_concepts`, `complexity`. Parser only extracts `explanation`. Extra fields are thrown away.

**Action**: Either use the full schema or simplify the prompt to match what we parse.

### Max turns exceeded throws away partial work

When agent hits max turns, it returns an error and discards all progress (agent.rs:174). The agent has `last_content` from the loop but throws it away.

**Action**: Return partial results with a warning instead of an error. Users should get whatever progress was made.

---

## Low Priority / Future Consideration

### Exit code granularity

All errors return exit code 2. Scripts can't distinguish "file not found" from "API timeout" from "config error".

**Action**: Consider more specific exit codes for different error categories.

### TTY detection

No `isatty()` checks. Could adjust output (colors, progress indicators) based on terminal capabilities.

---

## Completed

- [x] Remove config command setter (was stub printing "not yet implemented")
- [x] Remove unused `--context` flag from explain command
- [x] Add signal handling for graceful Ctrl+C interruption
