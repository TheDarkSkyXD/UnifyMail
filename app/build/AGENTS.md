<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# build

## Purpose
Grunt-based build system for compiling, packaging, and documenting the UnifyMail application. Contains Grunt task definitions, build resources (icons, platform configs), and documentation generation source files.

## Key Files

| File | Description |
|------|-------------|
| `Gruntfile.js` | Main Grunt configuration: registers build tasks (`build-client`, `lint`, etc.) |
| `create-signed-windows-installer.js` | Windows installer creation with code signing support |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `tasks/` | Grunt task definitions (compile TypeScript, copy files, package, etc.) |
| `resources/` | Platform-specific build resources: app icons, installer graphics, Info.plist |
| `docs_src/` | Documentation source files for API docs generation |
| `docs_templates/` | Handlebars templates for generated documentation |

## For AI Agents

### Working In This Directory
- Run build with `npm run build` from the root (invokes `grunt build-client`)
- Run lint with `npm run lint` from the root
- The Gruntfile delegates to task files in `tasks/`
- Windows installer signing requires valid certificates — test without signing first
- Build output goes to a directory outside this repo (configured in Grunt tasks)

### Common Patterns
- Grunt tasks are loaded from `tasks/` using `load-grunt-parent-tasks`
- Build process: Compile TypeScript → Copy assets → Package with Electron Packager
- Platform-specific resources are selected based on `process.platform`

## Dependencies

### Internal
- `app/src/` — Source code compiled during build
- `app/static/` — Assets copied during build
- `app/internal_packages/` — Packages included in build output

### External
- `grunt` / `grunt-cli` — Build task runner
- `@electron/packager` — Electron application packaging
- `electron-winstaller` — Windows installer creation

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
