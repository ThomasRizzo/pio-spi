# GitHub Copilot Instructions for pio-spi

## Issue Tracking with bd (beads)

**CRITICAL**: This project uses **bd (beads)** for ALL task tracking. Do NOT create markdown TODOs or other tracking methods.

### Essential Commands

```bash
# Find work
bd ready --json                    # Unblocked issues

# Create and manage
bd create "Title" -t bug|feature|task -p 0-4 --json
bd update <id> --status in_progress --json
bd close <id> --reason "Done" --json
```

### Workflow

1. **Check ready work**: `bd ready --json`
2. **Claim task**: `bd update <id> --status in_progress --json`
3. **Work on it**: Implement, test, document
4. **Discover new work?** `bd create "Found bug" -p 1 --deps discovered-from:<parent-id> --json`
5. **Complete**: `bd close <id> --reason "Done" --json`

### Priorities

- `0` - Critical (security, data loss, broken builds)
- `1` - High (major features, important bugs)
- `2` - Medium (default, nice-to-have)
- `3` - Low (polish, optimization)
- `4` - Backlog (future ideas)

### Important Rules

- ✅ Use bd for ALL task tracking
- ✅ Always use `--json` flag for programmatic use
- ✅ Link discovered work with `discovered-from` dependencies
- ❌ Do NOT create markdown TODO lists
- ❌ Do NOT use external issue trackers

For detailed workflows, see [AGENTS.md](../AGENTS.md)
