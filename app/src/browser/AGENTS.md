<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# browser

## Purpose
Electron main (browser) process code. This is the entry point of the application — it creates and manages windows, handles auto-updates, manages the system tray, processes native notifications, controls the application lifecycle, and communicates with the renderer process via IPC.

## Key Files

| File | Description |
|------|-------------|
| `main.js` | **Application entry point**: Electron bootstrapping, single-instance lock, crash reporter, command-line parsing |
| `application.ts` | Core `Application` class: window lifecycle, IPC handlers, global state, protocol registration |
| `mailspring-window.ts` | `MailspringWindow` class: wraps `BrowserWindow`, manages loading, devtools, web preferences |
| `window-manager.ts` | Manages multiple windows: main, composer popout, onboarding, preferences |
| `window-launcher.ts` | Factory for launching new windows with correct configuration |
| `application-menu.ts` | Constructs native application menu from templates (darwin/win32/linux) |
| `application-touch-bar.ts` | macOS Touch Bar integration |
| `autoupdate-manager.ts` | Auto-update lifecycle: check, download, install, notify |
| `autoupdate-impl-base.ts` | Base class for platform-specific auto-update implementations |
| `autoupdate-impl-win32.ts` | Windows-specific (Squirrel) auto-update implementation |
| `system-tray-manager.ts` | System tray icon management and context menu |
| `windows-taskbar-manager.ts` | Windows taskbar integration (jump lists, progress indicators) |
| `windows-updater.js` | Windows Squirrel updater event handling (install/update/uninstall hooks) |
| `notification-ipc.ts` | IPC bridge for native notifications between main and renderer |
| `quickpreview-ipc.ts` | IPC bridge for file quick preview (PDF, docs) between main and renderer |
| `config-persistence-manager.ts` | Saves and loads application config (JSON) with debouncing and error recovery |
| `config-migrator.ts` | Migrates configuration from older versions |
| `file-list-cache.ts` | Caches file lists for performance |
| `mailspring-protocol-handler.ts` | Handles `mailspring://` protocol URLs (deep linking) |
| `move-to-applications.ts` | macOS: prompts user to move app to /Applications |
| `is-wayland.ts` | Linux: detects Wayland display server |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `types/` | TypeScript type declarations specific to the main process |

## For AI Agents

### Working In This Directory
- **`main.js` is the application entry point** — modifications here affect startup. Be very careful.
- **`application.ts` is the god object** — it coordinates everything in the main process. Changes here are high-risk.
- IPC handlers defined here are called from renderer via `ipcRenderer.send()` / `ipcRenderer.invoke()`
- Main process code runs in Node.js context (no DOM) — use Electron APIs only
- Window lifecycle events (close, focus, blur) are handled in `mailspring-window.ts`
- Auto-update behavior differs per platform — test on the target OS

### Testing Requirements
- Main process tests are in `app/spec/autoupdate-manager-spec.ts` and similar
- Testing main process code is harder — consider integration tests
- Mock `BrowserWindow` and IPC for unit tests

### Common Patterns
- **IPC handlers**: `ipcMain.on('channel', handler)` or `ipcMain.handle('channel', handler)`
- **Window creation**: Use `WindowLauncher` to create windows with correct webPreferences
- **Config access**: Use `ConfigPersistenceManager` for reading/writing config JSON
- **Platform branching**: `process.platform === 'darwin' | 'win32' | 'linux'`

## Dependencies

### Internal
- `app/src/config.ts` — Configuration system shared with renderer
- `app/menus/` — Menu template definitions loaded by `ApplicationMenu`
- `app/src/mailsync-process.ts` — Spawns mailsync from main process

### External
- Electron (`app`, `BrowserWindow`, `ipcMain`, `Menu`, `Tray`, `dialog`, etc.)
- `@electron/packager` — Used during build
- `electron-winstaller` — Windows installer support

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
