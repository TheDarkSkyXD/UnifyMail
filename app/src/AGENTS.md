<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# src

## Purpose
Core application source code for the Mailspring email client. Contains the Electron main/browser process bootstrap, the renderer process UI (React components), the Flux-based state management system (actions, stores, models, tasks), platform services, extension APIs, registries for dependency injection, and utility modules.

## Key Files

| File | Description |
|------|-------------|
| `app-env.ts` | Application environment setup — global `AppEnv` singleton providing access to config, packages, themes, commands |
| `config.ts` | Configuration management system with JSON schema validation, observers, and persistence |
| `config-schema.ts` | JSON schema defining all application configuration options |
| `mailsync-process.ts` | Manages the native mailsync C++ child process lifecycle |
| `mailbox-perspective.ts` | Defines mailbox views (inbox, starred, drafts, etc.) and their query logic |
| `window-bootstrap.ts` | Renderer window initialization and bootstrapping |
| `secondary-window-bootstrap.ts` | Bootstrap for secondary windows (composer popout, etc.) |
| `sheet-container.tsx` | Root React component that renders the application sheets/views |
| `sheet-toolbar.tsx` | Toolbar component for the main application sheet |
| `sheet.tsx` | Sheet (view pane) component with animation and layout support |
| `native-notifications.ts` | Desktop notification system (native OS notifications) |
| `mail-rules-processor.ts` | Engine for processing user-defined mail rules on incoming messages |
| `mail-rules-templates.ts` | Built-in mail rule template definitions |
| `key-manager.ts` | Encryption key management for secure credential storage |
| `keymap-manager.ts` | Keyboard shortcut management and keybinding resolution |
| `intl.ts` | Internationalization (i18n) — locale detection, string translation via `localized()` |
| `ics-event-helpers.ts` | ICS/iCalendar file parsing and event creation helpers |
| `calendar-utils.ts` | Calendar utility functions (date math, recurrence helpers) |
| `date-utils.ts` | Date formatting and manipulation utilities |
| `dom-utils.ts` | DOM manipulation helper functions |
| `canvas-utils.ts` | HTML canvas rendering utilities (used for thread list rendering) |
| `regexp-utils.ts` | Common regex patterns for email parsing (URLs, emails, quotes) |
| `theme-manager.ts` | Theme loading, activation, and hot-reloading |
| `style-manager.ts` | LESS/CSS stylesheet management |
| `package-manager.ts` | Internal package discovery, loading, activation, and lifecycle |
| `package.ts` | Package model — represents a single internal package |
| `menu-manager.ts` | Application menu construction and management |
| `menu-helpers.ts` | Menu template merging helpers |
| `spellchecker.ts` | Spell checking integration |
| `system-start-service.ts` | Auto-start on system boot configuration |
| `default-client-helper.ts` | Default mail client registration helpers |
| `error-logger.js` | Global error logging and crash reporting |
| `compile-cache.js` | TypeScript/CoffeeScript compilation cache for development |
| `promise-extensions.ts` | Promise utility extensions (timeout, retry, etc.) |
| `backoff-schedulers.ts` | Exponential backoff scheduling for retry logic |
| `less-compile-cache.ts` | LESS stylesheet compilation cache |
| `linux-theme-utils.ts` | Linux-specific GTK theme integration |
| `linux-dnd-utils.ts` | Linux-specific drag-and-drop utilities (Wayland/X11 compat) |
| `fs-utils.ts` | File system utility functions |
| `virtual-dom-utils.ts` | Virtual DOM comparison and diffing utilities |
| `chrome-user-agent-stylesheet-string.ts` | Chrome user agent stylesheet as a string constant |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `browser/` | Electron main (browser) process: Application class, window management, auto-updater (see `browser/AGENTS.md`) |
| `components/` | Reusable React UI components: buttons, lists, modals, editors, popovers (see `components/AGENTS.md`) |
| `flux/` | Flux architecture: actions, stores, models, tasks, database, attributes (see `flux/AGENTS.md`) |
| `services/` | Application services: search, undo manager, inline transformers (see `services/AGENTS.md`) |
| `global/` | Global module exports: `mailspring-exports`, `mailspring-component-kit`, `mailspring-store` (see `global/AGENTS.md`) |
| `registries/` | Dependency injection registries: component, command, extension, sound, service (see `registries/AGENTS.md`) |
| `extensions/` | Extension point APIs: composer, message-view, thread-list, account-sidebar (see `extensions/AGENTS.md`) |
| `types/` | TypeScript type declaration files for global extensions |
| `decorators/` | React component decorators (HOCs) |
| `quickpreview/` | File attachment quick preview system (PDF, XLSX, code) |
| `searchable-components/` | UI text search infrastructure (find-in-page for React components) |
| `compile-support/` | TypeScript compilation support for development mode |
| `error-logger-extensions/` | Error reporting extensions (Sentry/Raven) |

## For AI Agents

### Working In This Directory
- **DO NOT modify `app-env.ts` casually** — it defines the global `AppEnv` that all code depends on
- When adding new utilities, follow the existing pattern of focused, single-purpose `.ts` files
- The `global/` directory defines the public API surface — `mailspring-exports` and `mailspring-component-kit` are what plugins import
- New registries or services should be registered in the appropriate registry file
- Platform-specific code should be gated with `process.platform` checks

### Testing Requirements
- Unit tests for src files go in `app/spec/` mirroring this directory structure
- Complex logic (mail rules, date utils, regex) should have thorough test coverage
- Test utilities are in `app/spec/mailspring-test-utils.ts`

### Common Patterns
- **Global singleton**: `AppEnv` (from `app-env.ts`) is the application root — access config, packages, windows, etc.
- **Flux pattern**: Actions → Stores → Components (see `flux/AGENTS.md`)
- **Registry pattern**: `ComponentRegistry`, `CommandRegistry`, `ExtensionRegistry` provide plugin injection points
- **Observable subscriptions**: Stores emit change events; components subscribe via `FluxContainer` or `listenTo()`
- **Localization**: Use `localized('key')` from `intl.ts` for all user-facing strings

## Dependencies

### Internal
- `app/internal_packages/` — Plugins that utilize these core APIs
- `app/static/` — HTML entry points, CSS base styles, images

### External
- React 16.x, ReactDOM — UI framework
- Electron APIs — Window management, IPC, native features
- better-sqlite3 — Local database
- rx-lite — Observable streams for reactive queries
- moment — Date/time formatting
- underscore — Utility belt

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
