# init

Interactive setup wizard to initialize Ursix configuration.

## Syntax

```bash
usx init [OPTIONS]
```

## Flags

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--skip-test` | boolean | false | Skip connectivity test after setup |
| `--force` | boolean | false | Overwrite existing configuration files |

## Behavior

The `init` command guides you through first-time setup:

1. **Detect Ollama**: Checks if Ollama is running on localhost:11434
2. **Provider selection**: Choose between `ollama` (local) or `openai` (cloud)
3. **API key guidance**: For OpenAI, explains how to set `URSIX_API_KEY`
4. **Model selection**: Choose model with sensible defaults per provider
5. **Create config**: Writes `.ursix.toml` with your selections
6. **Create rules**: Writes `.ursix/rules.yml` with LLM-focused starter rules
7. **Test connectivity**: Verifies the provider is accessible (unless `--skip-test`)

### Created Files

| File | Purpose |
|------|---------|
| `.ursix.toml` | Project configuration |
| `.ursix/rules.yml` | Review rules (LLM-focused, beyond linters) |

### Starter Rules

The generated `rules.yml` includes rules that LLMs can detect but traditional linters cannot:

**Clarity:**
- `vague-naming`: Variables like `data`, `temp`, `result` that don't convey purpose
- `misleading-names`: Names that don't match behavior (e.g., `getUser` that modifies state)
- `redundant-comments`: Comments that restate what code does instead of adding context

**Robustness:**
- `error-message-quality`: Error messages without debugging context
- `missing-edge-cases`: Unhandled empty input, null, boundary conditions

**Maintainability:**
- `magic-values`: Unexplained numbers/strings that should be constants
- `boolean-parameter-blindness`: Boolean params unclear at call site

## Output

### Human Format

```
Welcome to Ursix! Let's set up your configuration.

? Select LLM provider
> ollama (local) - Recommended, detected on your system
  openai (cloud) - Requires API key

? Model to use: llama3.2

Testing connectivity... OK

Ursix initialized successfully!

Created files:
  - .ursix.toml
  - .ursix/rules.yml

Provider: ollama
Model: llama3.2
Connectivity test: passed
```

### JSON Format

```json
{
  "success": true,
  "files_created": [".ursix.toml", ".ursix/rules.yml"],
  "provider": "ollama",
  "model": "llama3.2",
  "connectivity_test": true,
  "warnings": []
}
```

## Examples

```bash
# Interactive setup
usx init

# Force overwrite existing config
usx init --force

# Skip connectivity test
usx init --skip-test

# Non-interactive with pre-configured environment
URSIX_PROVIDER=openai URSIX_MODEL=gpt-4o usx init --skip-test
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 2 | User error (existing config without --force, user cancelled) |

## Configuration Files

After running `usx init`, you'll have:

### .ursix.toml

```toml
provider = "ollama"
model = "llama3.2"
```

### .ursix/rules.yml

```yaml
categories:
  clarity:
    - name: vague-naming
      description: "Flag variables/functions with unclear names..."
      severity: warning
    # ... more rules
```

## API Key Setup (OpenAI)

For OpenAI, set your API key via:

1. **`.env` file** (recommended for projects):
   ```
   URSIX_API_KEY=sk-...
   ```

2. **Environment variable**:
   ```bash
   export URSIX_API_KEY=sk-...
   ```

3. **CLI flag** (for one-off commands):
   ```bash
   usx review --provider openai https://api.openai.com/v1 sk-...
   ```

## Customizing Rules

After initialization, edit `.ursix/rules.yml` to add project-specific rules:

```yaml
categories:
  # Add your own category
  project:
    - name: no-print-statements
      description: "Remove debug print statements before committing"
      severity: error
      files: "*.py"
```

See [review](review.md) for how rules affect code reviews.
