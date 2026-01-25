# Ursix Documentation

Comprehensive documentation for the Ursix CLI tool.

## Quick Links

- [CLI Reference](cli-reference.md) - Complete command and flag reference
- [Commands](commands/) - Detailed documentation for each command:
  - [explain](commands/explain.md) - Explain code from stdin or file
  - [review](commands/review.md) - Review code changes from stdin or file
  - [fix](commands/fix.md) - Suggest fixes for code from stdin or file
  - [commit](commands/commit.md) - Generate commit messages from diff
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
# Explain code
cat src/main.rs | usx explain

# Review staged changes
git diff --staged | usx review

# Fix issues
cat src/lib.rs | usx fix

# Generate commit message
git diff --staged | usx commit --execute
```

## Configuration

Ursix loads configuration from (highest to lowest precedence):

1. CLI flags
2. Environment variables (`URSIX_OPENAI_API_KEY`)
3. `.ursix.toml` in working directory
4. `~/.ursix.toml` in home directory
5. Built-in defaults

See [CLI Reference](cli-reference.md#configuration) for available settings.
