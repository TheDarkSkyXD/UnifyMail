<!-- Parent: ../../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# workflows

## Purpose
GitHub Actions workflow files for continuous integration and deployment. Builds the application for Windows, macOS, and Linux (including ARM64).

## Key Files

| File | Description |
|------|-------------|
| `build-windows.yaml` | Windows build workflow: compiles, packages, creates NSIS installer |
| `build-macos.yaml` | macOS build workflow: compiles, packages, creates DMG |
| `build-linux.yaml` | Linux build workflow: compiles, packages, creates DEB/RPM/Snap |
| `build-linux-arm64.yaml` | Linux ARM64 build workflow for Raspberry Pi / ARM devices |

## For AI Agents

### Working In This Directory
- Workflows trigger on push/release events — test changes on a branch first
- Each platform build is independent and runs on its own runner OS
- Native dependencies are rebuilt per-platform by the build scripts
- Code signing configuration varies per platform (certificates, notarization)
- Be careful with secret references — they must be configured in GitHub repo settings

### Common Patterns
- Steps: checkout → setup Node.js → install deps → build → package → upload artifacts
- Matrix builds may be used for multiple architecture targets
- Caching `node_modules` speeds up builds significantly

## Dependencies

### Internal
- `package.json` — Root npm scripts invoked by workflows
- `scripts/postinstall.js` — Runs during `npm install` step
- `app/build/` — Grunt build tasks invoked during compilation

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
