<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# scripts

## Purpose
Build and maintenance scripts for the project. Includes the postinstall hook for native dependency building, and localization formatting/improvement utilities.

## Key Files

| File | Description |
|------|-------------|
| `postinstall.js` | npm postinstall hook: rebuilds native modules for Electron, copies mailsync binary, runs `npm install` in `app/` |
| `format-localizations.js` | Formats and normalizes localization JSON files for consistency |
| `improve-localization.js` | Utility for improving/updating localization strings across all language files |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `utils/` | Shared utility functions used by the scripts |

## For AI Agents

### Working In This Directory
- `postinstall.js` is critical — it runs after every `npm install` and handles:
  - Electron native module rebuilding via `@electron/rebuild`
  - Copying the correct mailsync binary for the current platform
  - Running `npm install` in the `app/` subdirectory
- Localization scripts operate on the `app/lang/` directory

### Testing Requirements
- After modifying `postinstall.js`, test by running `npm install` from the root directory
- Verify that native modules build correctly and mailsync binary is in place

### Common Patterns
- Scripts are CommonJS Node.js files (not TypeScript)
- Use `child_process.execSync` for build commands

## Dependencies

### Internal
- `app/package.json` — Nested install triggered by postinstall
- `app/lang/` — Localization files processed by format/improve scripts

### External
- `@electron/rebuild` — Native module rebuilding
- `fs-extra` — File system operations

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
