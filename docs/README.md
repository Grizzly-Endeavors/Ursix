# Ursix Documentation

Comprehensive documentation for the Ursix CLI tool.

## Quick Links

- [CLI Reference](cli-reference.md) - Complete command and flag reference
- [Commands](commands/) - Detailed documentation for each command:
  - [derive](commands/derive.md) - Derive content from input (commit-msg, explanation, summary)
  - [review](commands/review.md) - Review code changes from stdin or file
  - [fix](commands/fix.md) - Suggest fixes for code from stdin or file
  - [config](commands/config.md) - View configuration

## Overview

Ursix is a CLI tool providing Unix utilities powered by LLMs. It uses a stateless pipeline model for fast, predictable results.

### Execution

Single LLM call with piped input. No tools, no iteration, no message history. Same input always produces consistent output.

### Installation

```bash
cargo install ursix
```

### Basic Usage

```bash
# Generate commit message from diff
git diff --staged | usx derive commit-msg

# Explain code
cat src/main.rs | usx derive explanation

# Summarize content
cat doc.md | usx derive summary

# Review staged changes
git diff --staged | usx review

# Fix issues
cat src/lib.rs | usx fix
```

### Large Input Handling

For large inputs that exceed token limits, use `--chunk-recursive`:

```bash
# Process large codebase
cat src/**/*.rs | usx derive summary --chunk-recursive

# Generate commit for large diff
git diff | usx derive commit-msg --chunk-recursive
```

## Configuration

Ursix loads configuration from (highest to lowest precedence):

1. CLI flags
2. Environment variables (`URSIX_OPENAI_API_KEY`)
3. `.ursix.toml` in working directory
4. `~/.ursix.toml` in home directory
5. Built-in defaults

See [CLI Reference](cli-reference.md#configuration) for available settings.
