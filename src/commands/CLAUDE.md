# Commands Module

This directory contains the implementation for all Ursix CLI commands.

## Documentation Requirement

**IMPORTANT**: Any additions or changes to commands in this directory **MUST** be accompanied by corresponding updates to the documentation in `docs/`.

### Required Documentation Updates

When modifying commands, update:

1. **Command-specific docs** (`docs/commands/<command>.md`):
   - New/changed flags and their descriptions
   - New/changed behavior
   - Updated examples
   - Exit code changes

2. **CLI reference** (`docs/cli-reference.md`):
   - New global flags
   - New commands
   - Changes to execution modes
   - Exit code updates

3. **README** (`docs/README.md`):
   - New commands in quick links
   - Updated overview if behavior changes significantly

### Checklist

Before committing command changes, verify:

- [ ] New flags are documented with type, default, and description
- [ ] Behavior changes are explained in the relevant command doc
- [ ] Examples demonstrate new functionality
- [ ] Exit codes are documented if changed
- [ ] JSON output format is documented if modified
- [ ] Global flags are updated in `cli-reference.md` if applicable

## Module Structure

| File | Command | Description |
|------|---------|-------------|
| `commit.rs` | `usx commit` | Generate commit messages |
| `config.rs` | `usx config` | View configuration |
| `explain.rs` | `usx explain` | Explain code and concepts |
| `fix.rs` | `usx fix` | Fix code issues |
| `review.rs` | `usx review` | Review code changes |
| `mod.rs` | - | Module exports |

## Adding a New Command

1. Create `<command>.rs` in this directory
2. Add export to `mod.rs`
3. Register in `src/cli.rs`
4. Create `docs/commands/<command>.md`
5. Add to `docs/README.md` quick links
6. Add to `docs/cli-reference.md` commands list
