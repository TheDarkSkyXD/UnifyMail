<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# UnifyMail (Mailspring)

## Purpose
UnifyMail is a fork of [Mailspring](https://github.com/Foundry376/Mailspring), a desktop email client built with Electron, React, and TypeScript. It features a plugin-based architecture, a C++ sync engine (mailsync via Mailcore2), and supports IMAP/SMTP providers. The app is cross-platform (Windows, macOS, Linux) and includes features like unified inbox, snooze, send-later, mail rules, templates, link/open tracking, CalDAV/CardDAV calendar and contacts integration, and theming.

## Key Files

| File | Description |
|------|-------------|
| `package.json` | Root workspace dependencies, build scripts (`npm start`, `npm run build`), Electron 39.x |
| `book.json` | GitBook configuration for developer documentation |
| `.eslintrc` | ESLint configuration (TypeScript, React, JSX-a11y rules) |
| `.prettierrc` | Prettier code formatting configuration |
| `.gitmodules` | Git submodule references (mailsync native engine) |
| `README.md` | Project overview, feature list, contributing guide |
| `CONTRIBUTING.md` | Development setup and contribution guidelines |
| `CHANGELOG.md` | Version history and release notes |
| `SECURITY.md` | Security reporting policy |
| `CODE_OF_CONDUCT.md` | Contributor Covenant code of conduct |
| `LICENSE.md` | GPLv3 license |
| `LOCALIZATION.md` | Translation/localization guide |
| `PLUGIN_SYSTEM_ARCHITECTURE.md` | Plugin system design documentation |
| `CLAUDE.md` | AI assistant context file |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `app/` | Main Electron application: source code, internal packages/plugins, build system, specs, static assets (see `app/AGENTS.md`) |
| `docs/` | Feature plans and task specifications for CalDAV, CardDAV, calendar, and event features (see `docs/AGENTS.md`) |
| `plans/` | Implementation plans for CalDAV/CardDAV provider quirks, recurring events, and vTodo support (see `plans/AGENTS.md`) |
| `scripts/` | Build/install helper scripts: postinstall, localization formatting (see `scripts/AGENTS.md`) |
| `mailsync/` | Git submodule placeholder for the C++ Mailcore2-based sync engine |
| `screenshots/` | Project screenshots for README and marketing |
| `.github/` | GitHub Actions CI/CD workflows and issue templates (see `.github/AGENTS.md`) |
| `.circleci/` | Legacy CircleCI configuration |
| `.snapcraft/` | Snapcraft encrypted credentials for Snap publishing |
| `snap/` | Snap packaging configuration |
| `.vscode/` | VS Code workspace settings |

## For AI Agents

### Working In This Directory
- Run `npm install` after modifying `package.json` — the postinstall script handles native deps and app setup
- Use `npm start` to launch the Electron app in dev mode (`electron ./app --enable-logging --dev`)
- Use `npm test` to run the Jasmine test suite inside Electron
- TypeScript compilation: `npm run tsc-watch` for incremental type checking
- The build system uses Grunt (see `app/build/Gruntfile.js`)
- Electron version: **39.2.7** — check Electron API compatibility before using new APIs

### Testing Requirements
- Run `npm test` before committing changes
- Tests use Jasmine 2.x and run inside the Electron renderer process
- Test specs live in `app/spec/`
- Lint with `npm run lint` (Grunt-based ESLint)

### Common Patterns
- **Plugin architecture**: Features are implemented as internal packages in `app/internal_packages/`
- **Flux pattern**: State management via actions, stores, and models in `app/src/flux/`
- **Component injection**: UI extensibility via `ComponentRegistry` — plugins register React components for named roles
- **TypeScript + JSX/TSX**: All source code uses TypeScript with React component files as `.tsx`
- **LESS for styling**: Themes and component styles use LESS preprocessor

## Dependencies

### External
- **Electron 39.x** — Desktop application framework
- **React 16.x** — UI component library
- **TypeScript 5.7** — Type safety
- **Grunt** — Build orchestration
- **Jasmine 2.x** — Test framework
- **Mailcore2** (via mailsync C++ binary) — IMAP/SMTP sync engine

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
