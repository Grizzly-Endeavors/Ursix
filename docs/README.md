# Ursix Documentation

Comprehensive documentation for the Ursix CLI tool.

## Quick Links

- [CLI Reference](cli-reference.md) - Complete command and flag reference
- [Writing Effective Rules](writing-effective-rules.md) - Best practices for LLM-based review rules
- [Commands](commands/) - Detailed documentation for each command:
  - [init](commands/init.md) - Initialize Ursix configuration
  - [status](commands/status.md) - Check configuration and provider connectivity
  - [derive](commands/derive.md) - Derive content from input (commit-msg, explanation, summary)
  - [review](commands/review.md) - Review code changes from stdin or file
  - [fix](commands/fix.md) - Transform code snippets using structured input
  - [config](commands/config.md) - View configuration

## Overview

Ursix is a CLI tool providing Unix utilities powered by LLMs. It uses a stateless pipeline model for fast, predictable results.

### Execution

Single LLM call with piped input. No tools, no iteration, no message history. Same input always produces consistent output.

### Installation

```bash
cargo install ursix
```

### Getting Started

Run the interactive setup wizard:

```bash
usx init
```

This creates `.ursix.toml` (configuration) and `.ursix/rules.yml` (review rules).

### Basic Usage

```bash
# Generate commit message from diff
git diff --staged | usx derive commit-msg

# Explain code (positional file argument)
usx derive explanation src/main.rs

# Summarize content
cat doc.md | usx derive summary

# Review staged changes
git diff --staged | usx review

# Review a specific file
usx review src/main.rs

# Fix specific issue (structured input)
echo '{"issue": "unused variable", "file": "src/lib.rs", "lines": [10, 10]}' | usx fix
```

### Large Input Handling

For large inputs that exceed token limits, use `--chunk`:

```bash
# Process large codebase
cat src/**/*.rs | usx derive summary --chunk

# Generate commit for large diff
git diff | usx derive commit-msg --chunk
```

## Configuration

Ursix loads configuration from (highest to lowest precedence):

1. CLI flags
2. Environment variables (`URSIX_API_KEY`, `URSIX_PROVIDER_URL`)
3. `.ursix.toml` in working directory
4. `~/.ursix.toml` in home directory
5. Built-in defaults

See [CLI Reference](cli-reference.md#configuration) for available settings.
