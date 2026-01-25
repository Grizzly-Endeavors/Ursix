# Ursix TODO

Organized into phases for systematic completion. Each phase builds on the previous.

## Decisions Needed

These block implementation work. Resolve before starting the related phase.

### JSON output schema standardization

Blocks: Phase 1 (error structure), Phase 5 (documentation)

Questions to resolve:
- **Error object structure**: Is `{"error": {"code", "message", "details", "retryable"}}` the right shape? Should `details` be typed per error code or freeform?
- **Versioning strategy**: How do we evolve schemas without breaking scripts? Options: semver in output, `/v2/` style command variants, additive-only changes, explicit `schema_version` field
- **Metadata fields**: Should all commands include common fields like `tokens_used`, `duration_ms`, `model`? Where do these live in the structure?
- **Consistency audit**: Current output types (`ExplainResult`, `ReviewResult`, `FixResult`, `CommitResult`) may have inconsistent patterns - review before standardizing

### Commit chunking strategy

Blocks: Phase 2 (commit command chunking)

Options:
1. **Chunk by file** → generate per-file summaries → synthesize into final message (most accurate, multiple LLM calls)
2. **Truncate diff** with "and N more files changed" summary (single call, loses detail)
3. **Fail fast** with "staged changes too large, consider splitting commit" (simplest, punts to user)

Considerations: What's the typical large-commit use case? Is accuracy or speed more important?

---

## Guiding Principles

- **Stateless execution**: Every invocation is a single-pass operation. No session state, no conversation history, no tool loops.
- **Unix-style contracts**: Same command + same flags = same output structure. Scripts should be able to rely on output format.
- **No silent failures**: If something doesn't work, the user must know. Warnings in logs are not sufficient.
- **Structural output guarantee**: Output is ALWAYS valid JSON matching documented schema. Exit 0 = complete result, exit non-zero = valid error JSON. Partial failures include results in error object for recovery.
- **Lean over feature-rich**: Remove half-baked features rather than ship them incomplete.

---

## Phase 1: Error Handling Foundation

These establish patterns used by later phases. Do in order.

### [ ] Define standard error JSON structure

**Blocked by: JSON output schema standardization decision**

Create shared error types in `src/output/mod.rs` based on decided schema. This unblocks "parse failures should fail" and "structural output guarantee."

### [ ] Silent failure audit

Eliminate all silent failures. Known locations:
- `src/input.rs:34` - empty stdin silently ignored → warn or fail
- `src/context.rs:73` - partial file read logged at debug → fail the command

Audit for any other swallowed errors. Every failure must be visible without `RUST_LOG`.

### [ ] Make parse failures hard errors

Remove `parse_warning` fallback pattern. Parse failures should fail the command, not return raw response.

Affected:
- `src/commands/explain.rs:59-62`
- `src/commands/fix.rs:119-128`
- `src/commands/review.rs`

Depends on: standard error structure (for consistent error response).

### [ ] Add --verbose flag for error details

Output full error chain (anyhow context layers), relevant state, and suggestions. Include in JSON `error.details` field.

### [ ] Refactor exit codes to semantic categories

Simplify from 11 granular codes to 5 actionable categories:
```
0 = Success
1 = IssuesFound (review found issues - not an error)
2 = UserError (config, input, usage - user must fix)
3 = TransientError (network, parse, rate limit - retry may help)
4 = PermanentError (auth failure, API rejection - won't work without changes)
5 = InternalError (bug)
```

Update `ExitCode` enum in `src/output/mod.rs` and `ToExitCode` implementations. Specific error source exposed via JSON `error.code` for scripts needing granularity.

---

## Phase 2: LLM Reliability

These improve robustness of LLM interactions. Can be done in parallel after Phase 1.

### [ ] Configurable timeouts

LLM calls can hang indefinitely. Add:
- Default timeout (60s or 120s)
- `--timeout` CLI flag
- Config file option
- Clear timeout error message

Locations: `src/llm/ollama.rs`, `src/llm/openai.rs`

### [ ] Parse retry logic

Retry malformed JSON responses before failing. Especially valuable for local LLMs.

Considerations:
- 1-2 retries for remote APIs, more for local
- Prompt adjustment on retry ("respond with valid JSON only")
- Immediate retry for local, exponential backoff for remote
- Provider detection for strategy selection

Depends on: parse failures as hard errors (Phase 1).

### [ ] Token estimation and --dry-run

Every LLM command must:
1. Include `tokens_estimated` in JSON output
2. Support `--dry-run` showing token count and chunking plan without LLM call

Enables cost prediction, early "too large" detection, CI validation without cost.

Affected: `explain`, `review`, `fix`, `commit` commands.

### [ ] Commit command chunking

**Blocked by: Commit chunking strategy decision**

`commit` fails on large staged diffs. Implement chosen strategy. Must produce coherent single commit message.

### [ ] Structural output guarantee for partial failures

When chunk N of M fails:
- Exit non-zero
- Return valid error JSON with `partial_results` for recovery

```json
{
  "error": {
    "code": "partial_failure",
    "message": "2 of 5 chunks failed",
    "retryable": true,
    "failed_chunks": ["src/large.rs", "src/complex.rs"],
    "partial_results": { ... }
  }
}
```

Affected: `src/chunk.rs`, all command output types.

Depends on: standard error structure (above).

---

## Phase 3: Performance

High-impact optimizations, especially for git hook usage. Independent of other phases.

### [ ] HTTP client reuse (HIGH IMPACT)

Fresh `reqwest::Client` per invocation adds 200-500ms TCP/TLS overhead.

Fix:
- Create client once, reuse across chunks
- `Arc<Client>` for async task sharing
- Explicit connection pooling config

Locations: `src/cli.rs:284-292`, `src/llm/ollama.rs:37`, `src/llm/openai.rs:43,66`

### [ ] Arc-wrap context in chunking

Context cloned per chunk = memory bloat. 5 categories × 5 files = 25 copies of git_diff, etc.

Use `Arc<GatheredContext>` instead.

Location: `src/chunk.rs` lines 109, 112, 134-135, 328-330, 368, 371

### [ ] Optimize token counting in chunking

Token counting per chunk is redundant - shared context (git_diff, git_status) is identical.

Pre-calculate shared tokens once, count only per-chunk deltas.

Location: `src/chunk.rs:115-123`, `src/chunk.rs:324-325`

### [ ] Configure connection pooling

Explicit settings for:
- `pool_max_idle_per_host`
- `http2_keep_alive_interval`
- Timeouts (coordinate with Phase 2 timeout work)

Location: `src/llm/ollama.rs`, `src/llm/openai.rs`

---

## Phase 4: Provider Support

Currently supports Ollama and OpenAI-compatible APIs. Expand to major providers.

### [ ] Anthropic API support

Native Claude API integration via `src/llm/anthropic.rs`.

- Messages API with proper role formatting
- Streaming support (optional, aligns with future streaming work)
- Token counting via API response
- Handle Anthropic-specific errors (overloaded, rate limits)

Config: `provider = "anthropic"`, `ANTHROPIC_API_KEY` env var.

### [ ] Google Gemini support

`src/llm/gemini.rs` for Gemini API.

- generateContent endpoint
- Handle safety filters and blocked responses
- Token counting from response metadata

Config: `provider = "gemini"`, `GOOGLE_API_KEY` or `GEMINI_API_KEY` env var.

### [ ] AWS Bedrock support

`src/llm/bedrock.rs` for Bedrock runtime API.

- AWS SDK integration (aws-sdk-bedrockruntime)
- Support Claude, Titan, and other Bedrock models
- AWS credential chain (env, profile, IAM role)
- Region configuration

Config: `provider = "bedrock"`, `model = "anthropic.claude-3-sonnet..."`, standard AWS env vars.

### [ ] Provider abstraction cleanup

After adding providers, review `LlmClient` trait for consistency:
- Unified error types across providers
- Common retry/timeout behavior
- Consistent token counting interface

---

## Phase 5: Documentation & Polish

Lower priority. Do after core functionality is solid.

### [ ] JSON output contract documentation

**Blocked by: JSON output schema standardization decision**

Document the API that scripts depend on:
- JSON schema for each command's output
- Schema files for validation
- Test coverage for schema stability

Implement decided versioning strategy and metadata patterns.

### [ ] SECURITY.md

Document:
- Responsible disclosure process
- Vulnerability reporting
- Credential handling considerations

### [ ] CONTRIBUTING.md

Extract from CLAUDE.md:
- Development setup
- Code style
- PR process

### [ ] Example configuration files

Provide `.ursix.toml.example` and `rules.example.yml` for onboarding.

### [ ] CHANGELOG.md

Track changes between versions for upgrade visibility.

---

## Open Source Prep

Defer until ready to publish. Not blocking current development.

### [ ] Add LICENSE file

MIT license is specified in Cargo.toml but no LICENSE file exists.

### [ ] Complete Cargo.toml metadata

For crates.io: `repository`, `authors`, `keywords`, `categories`, `readme`, `rust-version`.

### [ ] Enable CI workflow triggers

`.github/workflows/ci.yml` triggers are commented out (lines 3-7).

### [ ] Switch to GitHub-hosted runners

Change `[self-hosted, kubernetes]` to `ubuntu-latest` for public accessibility.

---

## Future Consideration

Lower priority items to revisit later.

### Rate limiting handling

HTTP 429 handling: specific messaging, exponential backoff, Retry-After parsing.

### Large category sub-chunking

When category has 10+ rules, sub-chunk to avoid overwhelming LLM.

### TTY detection

`isatty()` checks for colors and progress indicators.

---

## Completed

- [x] Remove config command setter (was stub printing "not yet implemented")
- [x] Remove unused `--context` flag from explain command
- [x] Add signal handling for graceful Ctrl+C interruption
- [x] Deprecate `ask` command - removed entirely (doesn't fit Unix utility model)
- [x] Deprecate `models` command - removed entirely (provider-specific, not core functionality)
- [x] Remove agent mode entirely - pipeline mode only for simpler, more predictable behavior
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