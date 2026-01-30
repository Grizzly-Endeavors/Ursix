# Ursix TODO

Organized into phases for systematic completion. Each phase builds on the previous.

## Decisions Needed

These block implementation work. Resolve before starting the related phase.

### JSON output schema standardization

[x] Blocks: Phase 1 (error structure), Phase 5 (documentation)

Questions to resolve:
- **Error object structure**: Is `{"error": {"code", "message", "details", "retryable"}}` the right shape? Should `details` be typed per error code or freeform?
- **Versioning strategy**: How do we evolve schemas without breaking scripts? Options: semver in output, `/v2/` style command variants, additive-only changes, explicit `schema_version` field
- **Metadata fields**: Should all commands include common fields like `tokens_used`, `duration_ms`, `model`? Where do these live in the structure?
- **Consistency audit**: Current output types (`ExplainResult`, `ReviewResult`, `FixResult`, `CommitResult`) may have inconsistent patterns - review before standardizing

### ~~Commit chunking strategy~~ RESOLVED

**Decision: Token-based chunking with synthesis**

The `derive` command now supports `--chunk-recursive` which:
1. Splits input into ~4k token chunks at logical boundaries
2. Processes each chunk independently
3. Synthesizes results into final output

Future enhancement: Add `--chunk-mode` argument to support file-based splitting (Option 1 above) as an alternative to token-based splitting.

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

### [x] Define standard error JSON structure

**Status: COMPLETE** (Commit 4650519)

Created `src/output/error.rs` with:
- `TypedError` enum with serde(tag = "code") for flat JSON structure
- `ErrorResponse` for structured error output
- `OutputMeta` struct with schema_version, model, provider, duration, tokens, chunks
- 5-category exit codes (Success, IssuesFound, UserError, TransientError, PermanentError)

### [x] Silent failure audit

**Status: COMPLETE** (Commit 5d0e253)

Eliminated silent failures across:
- `context.rs` - Track file read failures explicitly, warn to stderr
- `fix.rs` - Handle JSON serialization errors explicitly
- `review.rs` - Make output_chunked_review_results return Result
- `resolver.rs` - Warn about invalid glob patterns and unknown --checks
- `input.rs` - Warn to stderr on stdin read failure

### [x] Make parse failures hard errors

**Status: COMPLETE** (Commit 62217c3)

Removed `parse_warning` and `raw_response` from result types. Parse failures now:
- Return errors instead of partial success
- Exit with code 4 (PermanentError) instead of 0
- All commands properly propagate parse failures

### [x] Refactor exit codes to semantic categories

**Status: COMPLETE** (Commit 4650519)

Simplified from 11 granular codes to 5 categories:
```
0 = Success
1 = IssuesFound (review found issues - not an error)
2 = UserError (config, input, usage - user must fix)
3 = TransientError (network, parse, rate limit - retry may help)
4 = PermanentError (auth failure, API rejection - won't work without changes)
```

Updated `ExitCode` enum in `src/output/mod.rs`. Specific error source exposed via JSON `error.code` for scripts needing granularity.

---

## Phase 2: LLM Reliability

These improve robustness of LLM interactions. Can be done in parallel after Phase 1.

### [x] Configurable timeouts

**Status: COMPLETE**

LLM calls now have configurable timeouts with:
- Default 60-second timeout (constant `DEFAULT_TIMEOUT_SECS` in `config.rs`)
- `--timeout` CLI flag for per-invocation override
- `timeout_secs` config file option
- `URSIX_TIMEOUT` environment variable
- `LlmError::Timeout` variant with clear error message ("request timed out after N seconds")
- Timeout errors are classified as transient (retryable)

Configuration layers (highest to lowest priority):
1. `--timeout` CLI flag
2. `URSIX_TIMEOUT` environment variable
3. `timeout_secs` in project config (`.ursix.toml`)
4. `timeout_secs` in global config (`~/.config/ursix/config.toml`)
5. Default: 60 seconds

### [x] Parse retry logic

**Status: COMPLETE** (Commit 62217c3)

Added `src/llm/retry.rs` with:
- `RetryConfig` for configurable retry behavior
- Exponential backoff with jitter (using rand crate)
- `--no-retry` and `--max-retries` CLI flags
- `is_retryable()` to distinguish transient vs permanent errors
- Retry logic integrated into pipeline LLM calls

Completed as part of "add retry infrastructure" commit.

### [x] Token estimation and --dry-run

**Status: COMPLETE**

Added `--dry-run` global flag to all commands:
- `DryRunResult` output type showing token estimation without LLM calls
- `tokens_estimated` field in dry-run JSON output
- Chunking plan preview when `--chunk` is enabled
- Token threshold warnings (8k warn, 16k error)
- Provider, model, and timeout configuration display

All four commands (`explain`, `review`, `fix`, `commit`) support `--dry-run`.

Enables cost prediction, early "too large" detection, and CI validation without API cost.

### [x] Commit/explain consolidated into derive command

**Status: COMPLETE**

Replaced `commit` and `explain` commands with unified `derive` command:
- `usx derive commit-msg` - generate commit messages from diffs
- `usx derive explanation` - explain code
- `usx derive summary` - summarize content

Supports `--chunk-recursive` for large inputs with token-based splitting and synthesis.

### [ ] Add --chunk-mode argument for derive command

Allow file-based splitting as alternative to token-based splitting:
- `--chunk-mode tokens` (default) - current behavior, split at ~4k token boundaries
- `--chunk-mode files` - split by file (for diffs), generate per-file summaries then synthesize

### [x] Review command file-based chunking

**Status: COMPLETE**

Added `--chunk` flag to review command for file-based diff chunking:
- Diffs are split at file boundaries (`diff --git` markers)
- Each file is processed independently with bounded concurrency (`--max-concurrency`)
- `--partial` flag allows continuing when some chunks fail
- Dry-run shows chunk plan with per-file token estimates

Supported input types:
- `git diff | usx review` - diff format (auto-detected)
- `usx review --from file.rs` - single file review

Not supported:
- Arbitrary stdin text (error: "use 'usx derive explanation' for arbitrary text")
- Chunking single files via `--from` (warning: falls back to single-pass)

### [ ] Review command: chunking for large single files

Currently, `--chunk` only supports diff input (splits by file). Large single files
passed via `--from` are processed as single-pass to avoid:
- Loss of line number context when splitting mid-file
- Issues reported with line numbers relative to chunk, not file
- Poor boundaries (splitting within functions/structs)

Potential solutions:
- AST-aware splitting (chunk at function/module boundaries)
- Relative line numbers with chunk offset tracking
- Accept the limitation (most review happens on diffs, not full files)

### [x] Structural output guarantee for partial failures

**Status: COMPLETE** (Commit 1baf361)

Implemented via `--partial` global flag:
- Created `PartialReviewResult` and `PartialFixResult` types
- Created `PartialFailureResponse` wrapper with metadata
- Created `ChunkFailureInfo` for tracking failed chunks
- When `--partial` enabled and chunked processing has failures, returns partial results with:
  - Details about which chunks succeeded vs failed
  - `partial_results` field for recovery
  - Proper exit codes (non-zero for failures)

Affected: `src/commands/fix.rs`, `src/commands/review.rs`, `src/output/partial.rs` (new).

See: integration test in `tests/integration.rs` for --partial flag acceptance.

---

## Phase 3: Performance

High-impact optimizations, especially for git hook usage. Independent of other phases.

### [x] HTTP client reuse (HIGH IMPACT)

**Status: COMPLETE** (Commit 02831c9)

Added `SharedHttpClient` wrapper around `Arc<Client>` for connection reuse:
- `HttpClientConfig` for configuring timeout, pool size, and keep-alive
- `with_http_client()` constructors on `OllamaClient` and `OpenAiClient`
- Shared client created once per pipeline execution in `cli/dispatch.rs`
- Saves 200-500ms TCP/TLS handshake overhead per subsequent request

### [x] Arc-wrap context in chunking

**Status: COMPLETE** (Commit 02831c9)

Changed `InputContext.content` from `String` to `Arc<str>`:
- Private field with `content()` accessor method
- `content_arc()` method for Arc access when needed
- Arc-wrapped system prompts in review chunking for cheap clones
- Reduces memory allocations for chunked operations

### [x] Optimize token counting in chunking

**Status: COMPLETE** (Commit 02831c9)

Fixed tokenizer mode consistency in review dry-run:
- Now respects configured `tokenizer_mode` instead of hardcoded heuristic
- Falls back to heuristic on error for robustness

### [x] Configure connection pooling

**Status: COMPLETE** (Commit 02831c9)

Added `HttpClientConfig` with explicit settings:
- `pool_max_idle_per_host`: 10 (default)
- `http2_keep_alive_secs`: 30 (default)
- `timeout_secs`: configurable (default 60)
- Integrated with Phase 2 timeout configuration

---

## Phase 4: Provider Support

Currently supports Ollama and OpenAI-compatible APIs. Expand to major providers.

### [ ] Anthropic API support

Native Claude API integration via `src/llm/anthropic.rs`.

- Messages API with proper role formatting
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

### [x] SECURITY.md

**Status: COMPLETE**

Documents responsible disclosure, credential handling, and dependency security.

### [x] CONTRIBUTING.md

**Status: COMPLETE**

Extracted from CLAUDE.md: development setup, code style, testing, PR process.

### [x] Example configuration files

**Status: COMPLETE**

Created `.ursix.toml.example` and `rules.example.yml` for onboarding.

### [ ] Update integration tests to use stable assert_cmd API

Remove `#[allow(deprecated)]` and replace `Command::cargo_bin("usx")` with `cargo_bin_cmd!("usx")` in `tests/integration.rs`. The assert_cmd API is now stable, but the runtime `cargo_bin` function has been deprecated in favor of the compile-time `cargo_bin_cmd!` macro.

---

## Open Source Prep

Defer until ready to publish. Not blocking current development.

### [x] Add LICENSE file

**Status: COMPLETE**

MIT LICENSE file added to repository root.

### [x] Complete Cargo.toml metadata

**Status: COMPLETE**

Added: `repository`, `keywords`, `categories`, `readme`, `rust-version`.

### [ ] Enable CI workflow triggers

`.github/workflows/ci.yml` triggers are commented out (lines 3-7). Enable when ready for public PRs.

**Note:** CI will continue to use self-hosted runners. GitHub-hosted runners will not be used.

---

## Future Consideration

Lower priority items to revisit later.

### Revisit review→fix pipeline (dedicated apply command?)

The `fix` command now only outputs suggested fixes - it no longer applies them automatically.
Consider whether to add a dedicated `apply` command that reads fix suggestions and applies them:

```bash
cat src/main.rs | usx fix > fixes.json
usx apply --from fixes.json  # New command?
```

Alternatively, the apply functionality could be a simple shell script since fixes are now JSON.

### [x] Improve diff format detection for single-file review

**Status: COMPLETE**

Added `--diff` flag to the review command. File arguments now default to single-file mode regardless of content. If a file's content looks like a diff, a warning is printed suggesting `--diff`. Stdin auto-detection is preserved for backward compatibility.

### Rate limiting handling

HTTP 429 handling: specific messaging, exponential backoff, Retry-After parsing.

### Large category sub-chunking

When category has 10+ rules, sub-chunk to avoid overwhelming LLM.

### TTY detection

`isatty()` checks for colors and progress indicators.

---

## Completed

### Recent (2026-01-27)
- [x] Interactive `usx init` command for first-time setup (dialoguer-based wizard)
  - Auto-detects Ollama availability
  - Guides provider selection (ollama/openai)
  - Creates `.ursix.toml` and `.ursix/rules.yml` with LLM-focused starter rules
  - Tests connectivity (optional `--skip-test`)
  - Supports `--force` to overwrite existing config
- [x] `.env` file support via dotenvy - enables `URSIX_API_KEY` in project `.env` files
- [x] Configurable system prompts in `.ursix.toml` (`[prompts]` section)
  - Supports: review, commit-msg, explanation, summary, fix
  - Custom prompts override defaults while preserving rules injection
- [x] Rules now required - removed default rules fallback
  - Error message suggests running `usx init` to create starter rules
  - Encourages explicit rule configuration per project
- [x] Phase 3: HTTP client reuse with SharedHttpClient for connection pooling
- [x] Phase 3: Arc-wrap InputContext.content for memory optimization in chunking
- [x] Phase 3: Fixed tokenizer mode consistency in review dry-run
- [x] Phase 3: Connection pooling configuration (pool_max_idle_per_host, http2_keep_alive)

### Earlier (2026-01-26)
- [x] Phase 2: Review command file-based chunking with --chunk flag
- [x] Refactored fix command for atomic transformations with structured JSON input
- [x] Added whole-file mode to fix command for multi-issue fixes
- [x] Consolidated provider flags into single multi-arg `--provider NAME [URL] [API-KEY]`
- [x] Simplified CLI flags and switched to positional file arguments
- [x] Reorganized modules for improved navigation

### Earlier (2026-01-25)
- [x] Phase 2: Consolidated commit/explain into unified derive command
- [x] Phase 2: Added --chunk-recursive support for large inputs
- [x] Phase 2: Removed global --chunk and --partial flags (derive-specific now)
- [x] Updated documentation for new derive command

### Earlier (2026-01-24)
- [x] Phase 1: Define standard error JSON structure with typed errors
- [x] Phase 1: Silent failure audit - all errors now visible to users
- [x] Phase 1: Make parse failures hard errors - removed parse_warning fallbacks
- [x] Phase 1: Refactor exit codes to 5 semantic categories
- [x] Phase 2: Parse retry logic with exponential backoff
- [x] Phase 2: Structural output guarantee for partial failures via --partial flag
- [x] Phase 2: Configurable timeouts - 60s default, --timeout flag, config file, env var support
- [x] Phase 2: Token estimation and --dry-run flag for all commands
- [x] Add warnings for silent failure fallbacks in error paths

### Older
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