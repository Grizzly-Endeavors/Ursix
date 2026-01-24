# Ursix Documentation

Comprehensive documentation for the Ursix CLI tool.

## Quick Links

- [CLI Reference](cli-reference.md) - Complete command and flag reference
- [Commands](commands/) - Detailed documentation for each command:
  - [explain](commands/explain.md) - Explain code and concepts
  - [review](commands/review.md) - Review code changes
  - [fix](commands/fix.md) - Fix issues in code
  - [commit](commands/commit.md) - Generate commit messages
  - [config](commands/config.md) - View configuration

## Overview

Ursix is an extensible CLI tool for LLM-powered development tasks, supporting both stateless pipeline and agentic modes.

### Execution Modes

| Mode | Description | When to Use |
|------|-------------|-------------|
| Pipeline (default) | Single LLM call, no tools, fast | Most tasks |
| Agent (`--agent`) | Multi-turn with tool access | Complex exploration |

### Installation

```bash
cargo install ursix
```

### Basic Usage

```bash
# Explain code
usx explain src/main.rs

# Review staged changes
usx review

# Fix issues
usx fix src/lib.rs --lint

# Generate commit message
usx commit --execute
```

## Configuration

Ursix loads configuration from (highest to lowest precedence):

1. CLI flags
2. Environment variables (`URSIX_OPENAI_API_KEY`)
3. `.ursix.toml` in working directory
4. `~/.ursix.toml` in home directory
5. Built-in defaults

See [CLI Reference](cli-reference.md#configuration) for available settings.
