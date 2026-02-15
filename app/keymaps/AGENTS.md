<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# keymaps

## Purpose
Platform-specific keyboard shortcut definitions in JSON format. Keymaps define command-to-key bindings for each OS, loaded by `KeymapManager` at startup.

## Key Files

| File | Description |
|------|-------------|
| `README.m` | Brief documentation about keymap structure |
| `base.json` | Cross-platform default keyboard shortcuts |
| `base-darwin.json` | macOS-specific keyboard shortcuts (Cmd key modifiers) |
| `base-linux.json` | Linux-specific keyboard shortcuts |
| `base-win32.json` | Windows-specific keyboard shortcuts |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `templates/` | Keymap templates for user customization |

## For AI Agents

### Working In This Directory
- Keyboard shortcuts map CSS selectors (e.g., `body`, `.thread-list`) to command objects
- Format: `{ "selector": { "keystroke": "command:name" } }`
- Use `base.json` for platform-agnostic bindings
- Platform files override or extend `base.json` for OS-specific conventions
- Cmd (⌘) on macOS maps to Ctrl on Windows/Linux automatically in some cases — check `KeymapManager`

### Common Patterns
- Commands are defined in `CommandRegistry` (see `src/registries/command-registry.ts`)
- Keystrokes use Electron's accelerator format: `ctrl+shift+n`, `cmd+k`
- Conflicts are resolved by CSS selector specificity

## Dependencies

### Internal
- `app/src/keymap-manager.ts` — Loads and resolves these keymaps
- `app/src/registries/command-registry.ts` — Defines available commands

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
