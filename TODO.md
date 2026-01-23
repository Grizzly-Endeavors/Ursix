# Ursix TODO

Issues and improvements needed before the core feature set is production-ready.

## Guiding Principles

- **Stateless execution**: Every invocation is a single-pass operation, even in agent mode. No session state, no back-and-forth. Agent mode just means the LLM can use tools within that single invocation.
- **Unix-style contracts**: Same command + same flags = same output structure. Scripts should be able to rely on output format.
- **No silent failures**: If something doesn't work, the user must know. Warnings in logs are not sufficient.
- **Lean over feature-rich**: Remove half-baked features rather than ship them incomplete.

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
- [x] Deprecate `ask` command - removed entirely (doesn't fit Unix utility model)
- [x] Deprecate `models` command - removed entirely (provider-specific, not core functionality)
- [x] Agent mode breaks output contracts - agent mode now produces structured output matching pipeline mode
- [x] Remove `--agent` from `commit` command (agent mode doesn't make sense for commit)
- [x] `fix --apply` fails silently - now tracks success/failure per fix with detailed results and proper exit codes
- [x] `ReviewResult.render_human()` drops issues - now displays issues in readable format (file:line, severity, message)
- [x] Parse failures silently report success - added `parse_warning` field to report failures to users
- [x] `models` command returns success on error - command removed entirely
- [x] Output bypasses render system in JSON mode - `execute_git_commit()` and `cmd_config()` now use render system
- [x] JSON serialization fallback masks errors - `render_json()` now returns error instead of silent `{}` fallback
- [x] `turns: 1` is always hardcoded - structured outputs now use proper result types without turns field
- [x] `--verbose` flag is dead - removed from CLI
- [x] `explain` JSON schema mismatch - simplified prompt to match what parser expects
- [x] Max turns error message needs improvement - now includes actionable suggestions
