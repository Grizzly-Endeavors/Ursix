# status

Validate configuration and test provider connectivity.

## Syntax

```bash
usx status [OPTIONS]
```

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--verbose` | boolean | false | Show detailed status information (response time, available models) |

Plus [global flags](../cli-reference.md#global-flags) (`--text` for human-readable output, `--provider` to override).

## Behavior

The `status` command performs four checks in sequence:

1. **config** - Configuration loads successfully
2. **provider** - Provider type and URL are valid
3. **api_key** - API key is set (required for remote OpenAI endpoints)
4. **connectivity** - Provider endpoint responds correctly

If the API key check fails, connectivity is skipped (cannot test without authentication).

### API Key Requirements

| Provider | Local URL | Remote URL |
|----------|-----------|------------|
| Ollama | Not required | Not required |
| OpenAI | Optional | Required |

"Local URL" means `localhost`, `127.0.0.1`, or `[::1]`. All other URLs are considered remote.

### Connectivity Check

The connectivity check calls the provider's "list models" endpoint:
- **Ollama**: `GET /api/tags`
- **OpenAI**: `GET /v1/models`

This is a read-only operation that doesn't consume API quota.

## Output

### Human Format (success)

```
Ursix Status: OK

Provider:  ollama (http://localhost:11434)
Model:     llama3.2
API Key:   not set

Checks:
  [ok] config: configuration loaded successfully
  [ok] provider: ollama at http://localhost:11434
  [ok] api_key: not required for Ollama
  [ok] connectivity: connected, model 'llama3.2' available
```

### Human Format (failure)

```
Ursix Status: FAILED

Provider:  openai (https://api.openai.com/v1)
Model:     gpt-4
API Key:   not set

Checks:
  [ok] config: configuration loaded successfully
  [ok] provider: openai at https://api.openai.com/v1
  [FAIL] api_key: API key required for remote OpenAI endpoint
      Set URSIX_API_KEY or OPENAI_API_KEY environment variable
  [FAIL] connectivity: cannot test connectivity without API key
```

### Human Format (verbose)

With `--verbose`, additional details are shown:

```
Ursix Status: OK

Provider:  ollama (http://localhost:11434)
Model:     llama3.2
API Key:   not set

Checks:
  [ok] config: configuration loaded successfully
  [ok] provider: ollama at http://localhost:11434
  [ok] api_key: not required for Ollama
  [ok] connectivity: connected, model 'llama3.2' available

Details:
  Response time: 45ms
  Model status: available
  Available models: llama3.2, codellama, mistral
```

### JSON Format (default)

```json
{
  "ok": true,
  "provider": "ollama",
  "provider_url": "http://localhost:11434",
  "model": "llama3.2",
  "api_key_set": false,
  "checks": [
    {"name": "config", "passed": true, "message": "configuration loaded successfully"},
    {"name": "provider", "passed": true, "message": "ollama at http://localhost:11434"},
    {"name": "api_key", "passed": true, "message": "not required for Ollama"},
    {"name": "connectivity", "passed": true, "message": "connected, model 'llama3.2' available"}
  ]
}
```

### JSON Format (with verbose)

```json
{
  "ok": true,
  "provider": "ollama",
  "provider_url": "http://localhost:11434",
  "model": "llama3.2",
  "api_key_set": false,
  "checks": [...],
  "verbose": {
    "response_time_ms": 45,
    "available_models": ["llama3.2", "codellama", "mistral"],
    "model_available": true
  }
}
```

## Examples

```bash
# Basic status check (JSON output)
usx status

# Human-readable output
usx status --text

# Verbose mode with response time and model list
usx status --verbose --text

# Check status with specific provider
usx status --provider openai --text

# Check status with custom URL
usx status --provider ollama http://remote-server:11434 --text

# Use in scripts
if usx status > /dev/null 2>&1; then
  echo "Ursix is ready"
else
  echo "Ursix configuration issue"
fi

# Parse JSON for automation
usx status | jq '.ok'
```

## Exit Codes

| Code | Condition |
|------|-----------|
| 0 | All checks pass |
| 2 | User error (missing API key, invalid configuration) |
| 3 | Transient error (provider unreachable, timeout) |

## Use Cases

### Pre-flight Check in CI

```bash
# Fail fast if LLM isn't available
usx status || exit 1

# Run actual commands
git diff --staged | usx review
```

### Debugging Configuration Issues

```bash
# See what configuration is being used
usx status --verbose --text

# Check with different providers
usx status --provider ollama --text
usx status --provider openai https://api.openai.com/v1 sk-xxx --text
```

### Health Check in Scripts

```bash
#!/bin/bash
if ! usx status > /dev/null 2>&1; then
  echo "ERROR: Ursix not configured correctly" >&2
  usx status --text >&2
  exit 1
fi
```
