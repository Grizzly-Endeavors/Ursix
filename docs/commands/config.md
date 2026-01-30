# config

View configuration values.

## Syntax

```bash
usx config [KEY] [OPTIONS]
```

## Arguments

| Argument | Type | Required | Description |
|----------|------|----------|-------------|
| `key` | string | No | Configuration key to display |

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--list` | boolean | false | List all configuration values |

Plus [global flags](../cli-reference.md#global-flags) (only `--text` is relevant for human-readable output).

## Behavior

### Usage Modes

- **No arguments**: Shows usage help
- **With `key`**: Displays value for specific configuration key
- **With `--list`**: Displays all configuration values

### Security

API keys are displayed as `[set]` or `[not set]` for security. The actual key value is never shown.

### Editing Configuration

The `config` command is read-only. To change settings, directly edit `.ursix/config.toml`:

```bash
# Edit project config
$EDITOR .ursix/config.toml

# Edit global config
$EDITOR ~/.config/ursix/config.toml
```

## Available Keys

| Key | Type | Description |
|-----|------|-------------|
| `provider` | enum | LLM provider (`ollama`, `openai`) |
| `model` | string | Model name to use |
| `ollama_url` | string | Ollama API base URL |
| `openai_url` | string | OpenAI-compatible API base URL |
| `openai_api_key` | boolean | API key status (`[set]`/`[not set]`) |
| `tokenizer_mode` | enum | Token counting mode |
| `working_dir` | path | Current working directory |

## Output

### Human Format (single key)

```
provider: openai
```

### Human Format (`--list`)

```
Configuration:
  provider:       openai
  model:          gpt-4
  ollama_url:     http://localhost:11434
  openai_url:     https://api.openai.com/v1
  openai_api_key: [set]
  tokenizer_mode: heuristic
  working_dir:    /home/user/project
```

### JSON Format (default)

```json
{
  "entries": [
    {"key": "provider", "value": "openai"},
    {"key": "model", "value": "gpt-4"},
    {"key": "openai_api_key", "value": "[set]"}
  ]
}
```

## Examples

```bash
# Show usage help
usx config

# List all configuration (JSON output by default)
usx config --list

# Human-readable output
usx config --list --text

# Get specific value
usx config provider
usx config model

# Check if API key is set
usx config openai_api_key
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | Config error (invalid key, file not found) |

## Configuration Files

Configuration is loaded from (highest to lowest precedence):

1. CLI flags
2. Environment variables (`URSIX_API_KEY`)
3. `.ursix/config.toml` in working directory or parent directories (project config)
4. `~/.config/ursix/config.toml` in home directory (global config)
5. Built-in defaults

### Example Configuration File

```toml
# .ursix/config.toml

# LLM Provider
provider = "openai"
model = "gpt-4"

# Optional: Override the default provider URL
provider_url = "https://api.openai.com/v1"

# Tokenizer
tokenizer_mode = "heuristic"
```
