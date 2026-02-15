<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# menus

## Purpose
Platform-specific application menu definitions. Contains JSON files that define the native menu bar structure (File, Edit, View, etc.) for each operating system. Loaded by `MenuManager`.

## Key Files

| File | Description |
|------|-------------|
| `darwin.js` | macOS application menu (includes app menu with About, Preferences, Quit) |
| `win32.js` | Windows application menu |
| `linux.js` | Linux application menu |

## For AI Agents

### Working In This Directory
- Menu definitions export a function returning menu template arrays
- Each menu item specifies `label`, `command`, `accelerator`, and `submenu`
- Commands must match those registered in `CommandRegistry`
- macOS menu differs significantly (app-name menu, Window menu conventions)
- Electron's `role` property can be used for standard items (copy, paste, etc.)

### Common Patterns
- Use `command: 'application:action-name'` to wire to registered commands
- Conditional items use `visible` or `enabled` properties
- Submenu separators: `{ type: 'separator' }`

## Dependencies

### Internal
- `app/src/menu-manager.ts` — Loads and constructs menus from these definitions
- `app/src/menu-helpers.ts` — Template merging utilities
- `app/src/registries/command-registry.ts` — Commands triggered by menu items

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
