# Ursix Examples

This directory contains reference configurations and templates for Ursix.

## Configuration Files

### `config.toml`

Example configuration file showing all available options. Copy to `.ursix/config.toml` in your project:

```bash
cp examples/config.toml .ursix/config.toml
```

Or use the interactive setup wizard:

```bash
usx init
```

### `rules.yml`

Example review rules for semantic linting. These rules focus on issues that LLMs can detect beyond traditional linters:

- **Clarity**: vague naming, misleading names, redundant comments
- **Robustness**: error message quality, missing edge cases
- **Maintainability**: magic values, boolean parameter blindness

Copy to `.ursix/rules.yml` in your project:

```bash
cp examples/rules.yml .ursix/rules.yml
```

Or let `usx init` create starter rules for you.

## Playbooks (Coming Soon)

The `playbooks/` directory is a placeholder for future workflow templates that will provide reusable automation for common development tasks.

See `playbooks/README.md` for planned features.
