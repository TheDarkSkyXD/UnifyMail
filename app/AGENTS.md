<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# app

## Purpose
The main Electron application directory. Contains all source code for the UnifyMail desktop email client, including the renderer process UI (React/TypeScript), the browser/main process bootstrapping, the internal plugin packages, the build system, test specs, static assets, localization files, keyboard shortcuts, and menu definitions.

## Key Files

| File | Description |
|------|-------------|
| `package.json` | App-level dependencies (React, Slate editor, Moment, etc.) and scripts |
| `tsconfig.json` | TypeScript compiler configuration for the app |
| `result-counter.js` | Utility script for counting test results |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `src/` | Core application source code: main/browser process, components, flux stores/models, services (see `src/AGENTS.md`) |
| `internal_packages/` | Active internal plugins (49 packages): composer, thread-list, calendar, themes, etc. (see `internal_packages/AGENTS.md`) |
| `internal_packages_disabled/` | Disabled/deprecated internal plugins |
| `build/` | Grunt build system: tasks, resources, documentation generation (see `build/AGENTS.md`) |
| `spec/` | Jasmine test specifications (see `spec/AGENTS.md`) |
| `static/` | Static assets: HTML entry points, fonts, images, sounds, CSS styles, animations (see `static/AGENTS.md`) |
| `keymaps/` | Platform-specific keyboard shortcut definitions (JSON format) (see `keymaps/AGENTS.md`) |
| `menus/` | Platform-specific application menu definitions (see `menus/AGENTS.md`) |
| `lang/` | Localization JSON files for 100+ languages |
| `script/` | Platform-specific build scripts (Windows, macOS, Linux) |
| `dot-UnifyMail/` | Default UnifyMail user configuration and local package directory |

## For AI Agents

### Working In This Directory
- This is the primary application directory — most code changes happen here
- The Electron app boots from `src/browser/main.js` (main process) and loads `static/index.html` (renderer)
- Internal packages are auto-loaded from `internal_packages/` — each is a self-contained plugin with its own `package.json`
- When adding new features, prefer creating or extending an internal package over modifying core `src/` code
- The build process is orchestrated by Grunt via `build/Gruntfile.js`

### Testing Requirements
- Test specs go in `spec/` mirroring the `src/` directory structure
- Run tests with `npm test` from the root directory
- Tests execute inside the Electron process (not Node.js directly)

### Common Patterns
- **Plugin loading**: Packages in `internal_packages/` are discovered and loaded by `PackageManager`
- **Component registration**: Plugins register React components with `ComponentRegistry` for UI injection points
- **Store-driven state**: Use Flux stores from `src/flux/stores/` for shared state
- **LESS stylesheets**: Component styles use LESS with variables from the active theme

## Dependencies

### Internal
- `mailsync` — Native C++ sync engine binary (spawned as child process)
- Root `package.json` — Development dependencies and build tools

### External
- React 16.x, React DOM, React Transition Group
- Slate 0.x — Rich text editor for composer
- Moment.js — Date/time handling
- Underscore — Utility library
- Better-SQLite3 — Local database
- RxJS (rx-lite) — Observable streams

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
