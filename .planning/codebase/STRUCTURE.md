# Codebase Structure

**Analysis Date:** 2026-03-01

## Directory Layout

```
UnifyMail/
├── app/                              # Main application directory
│   ├── unifymail-backend/            # Electron main process code
│   │   └── src/                      # Backend source files
│   ├── unifymail-frontend/           # Electron renderer process code
│   │   └── src/                      # Frontend source files
│   │       ├── components/           # Reusable React UI components (~70 files)
│   │       ├── flux/                 # Flux architecture core
│   │       │   ├── actions.ts        # All Reflux actions with scopes
│   │       │   ├── models/           # Data model classes (~25 files)
│   │       │   ├── stores/           # Flux stores (~35 files)
│   │       │   ├── tasks/            # Task classes for sync engine (~28 files)
│   │       │   ├── attributes/       # Attribute type definitions
│   │       │   ├── mailsync-bridge.ts  # Sync engine communication bridge
│   │       │   └── action-bridge.ts  # Cross-window action IPC bridge
│   │       ├── global/               # Global module exports for plugins
│   │       ├── registries/           # Component, extension, command registries
│   │       ├── services/             # HTML transformers, search query system
│   │       ├── extensions/           # Extension base classes
│   │       ├── decorators/           # Higher-order component decorators
│   │       ├── types/                # TypeScript type declarations
│   │       ├── quickpreview/         # File preview (PDF, XLSX, etc.)
│   │       ├── searchable-components/  # Searchable component framework
│   │       ├── error-logger-extensions/  # Error logger plugins
│   │       └── compile-support/      # TypeScript compile-cache support
│   ├── internal_packages/            # Built-in plugins (~50 packages)
│   │   ├── composer/                 # Email composition
│   │   ├── thread-list/              # Thread list view
│   │   ├── message-list/             # Message list/detail view
│   │   ├── account-sidebar/          # Account/folder sidebar
│   │   ├── preferences/              # Settings UI
│   │   ├── onboarding/               # Account setup wizard
│   │   ├── main-calendar/            # Calendar view
│   │   ├── contacts/                 # Contacts management
│   │   ├── notifications/            # In-app notifications
│   │   ├── undo-redo/                # Undo/redo UI
│   │   ├── ui-dark/                  # Dark theme
│   │   ├── ui-light/                 # Light theme
│   │   └── ...                       # Many more feature packages
│   ├── internal_packages_disabled/   # Disabled plugins
│   │   └── thread-unsubscribe/       # Disabled unsubscribe feature
│   ├── spec/                         # Jasmine test specs
│   │   ├── components/               # Component specs
│   │   ├── models/                   # Model specs
│   │   ├── stores/                   # Store specs
│   │   ├── tasks/                    # Task specs
│   │   ├── registries/               # Registry specs
│   │   ├── services/                 # Service specs
│   │   ├── fixtures/                 # Test fixtures and data
│   │   ├── spec-runner/              # Spec runner bootstrap
│   │   └── utils/                    # Test utility specs
│   ├── static/                       # Static assets loaded at runtime
│   │   ├── index.html                # Window HTML shell
│   │   ├── index.js                  # Window bootstrap loader
│   │   ├── style/                    # Global CSS (tailwind.css)
│   │   ├── images/                   # Icons and images
│   │   ├── fonts/                    # Font files
│   │   ├── sounds/                   # Notification sounds
│   │   ├── animations/               # CSS animations
│   │   └── extensions/               # Chrome extensions (i18n)
│   ├── keymaps/                      # Keyboard shortcut definitions
│   │   ├── base.json                 # Cross-platform keymaps
│   │   ├── base-darwin.json          # macOS keymaps
│   │   ├── base-linux.json           # Linux keymaps
│   │   ├── base-win32.json           # Windows keymaps
│   │   └── templates/                # Keymap templates
│   ├── menus/                        # Application menu definitions
│   │   ├── darwin.js                 # macOS menus
│   │   ├── linux.js                  # Linux menus
│   │   └── win32.js                  # Windows menus
│   ├── lang/                         # Localization files (100+ locales)
│   ├── default-config/               # Default configuration
│   │   ├── config.json               # Default app config
│   │   ├── keymap.json               # Default user keymaps
│   │   └── packages/                 # Default package configs
│   ├── mailsync                      # C++ sync engine binary (Linux/macOS)
│   ├── mailsync.cmd                  # Sync engine launcher (Windows)
│   ├── mailcore/                     # Native IMAP/SMTP bindings
│   ├── script/                       # Build/packaging scripts (grunt)
│   ├── node_modules/                 # App-level dependencies
│   ├── package.json                  # App package manifest (productName: UnifyMail)
│   └── tsconfig.json                 # TypeScript configuration
├── build/                            # Build system
│   ├── Gruntfile.js                  # Grunt task definitions
│   ├── tasks/                        # Grunt task implementations
│   ├── resources/                    # Build resources (icons, etc.)
│   └── docs_src/                     # Documentation source
├── scripts/                          # Development scripts
│   ├── start-dev.js                  # Dev mode launcher
│   ├── postinstall.js                # Post-install setup
│   ├── mock-mailsync.js              # Mock sync engine for testing
│   └── utils/                        # Script utilities
├── workers/                          # Background workers
│   └── auth-proxy/                   # OAuth proxy (Cloudflare Worker)
│       ├── src/                      # Worker source code
│       └── wrangler.jsonc            # Wrangler deployment config
├── docs/                             # Developer documentation
├── snap/                             # Snap package config (Linux)
├── package.json                      # Root package manifest
├── package-lock.json                 # Root lockfile
└── CLAUDE.md                         # AI assistant instructions
```

## Directory Purposes

**`app/unifymail-backend/src/`:**
- Purpose: Electron main process -- everything that runs before any window opens
- Contains: Application singleton, window management, auto-updater, system tray, protocol handlers, notification IPC, quickpreview IPC
- Key files:
  - `main.js` -- Application entry point, CLI parsing, Squirrel handling
  - `application.ts` -- Main `Application` class, IPC event handlers, window lifecycle
  - `window-manager.ts` -- Manages all `UnifyMailWindow` instances by key
  - `window-launcher.ts` -- Creates new `BrowserWindow` instances with hot-window optimization
  - `unifymail-window.ts` -- Wrapper around `BrowserWindow` with load settings
  - `autoupdate-manager.ts` -- Cross-platform auto-update handling
  - `system-tray-manager.ts` -- System tray icon and menu
  - `config-persistence-manager.ts` -- Saves config to disk
  - `windows-taskbar-manager.ts` -- Windows taskbar integration
  - `notification-ipc.ts` -- Windows toast notification handling

**`app/unifymail-frontend/src/`:**
- Purpose: Everything that runs inside renderer processes (the actual UI and business logic)
- Contains: AppEnv global, Flux architecture, React components, services, registries
- Key files:
  - `app-env.ts` -- `AppEnv` singleton (config, packages, keymaps, commands, themes, styles, bridges)
  - `window-bootstrap.ts` -- Main window initialization
  - `secondary-window-bootstrap.ts` -- Secondary/hot window initialization
  - `mailsync-process.ts` -- Spawns and communicates with C++ sync engine child processes
  - `package-manager.ts` -- Discovers and activates plugin packages
  - `package.ts` -- Package class encapsulating a plugin
  - `config.ts` -- Configuration manager
  - `intl.ts` -- Internationalization (localized strings)
  - `key-manager.ts` -- Credential/secret storage
  - `mailbox-perspective.ts` -- Mailbox perspective (inbox, starred, sent, custom)
  - `spellchecker.ts` -- Spell checking integration

**`app/unifymail-frontend/src/flux/`:**
- Purpose: Core Flux architecture -- the data layer of the application
- Contains: Actions, stores, models, tasks, sync bridges
- Key files:
  - `actions.ts` -- All Reflux actions with scope (window/global/main)
  - `mailsync-bridge.ts` -- Routes tasks to sync engine, processes incoming deltas
  - `action-bridge.ts` -- IPC bridge for cross-window action propagation
  - `unifymail-api-request.ts` -- HTTP request helper for UnifyMail API

**`app/unifymail-frontend/src/flux/models/`:**
- Purpose: Data model definitions with attribute-based serialization
- Contains: All entity models used throughout the app
- Key files:
  - `model.ts` -- Base `Model` class with `fromJSON`/`toJSON`/`matches`
  - `thread.ts` -- Email thread model
  - `message.ts` -- Email message model
  - `contact.ts` -- Contact model
  - `account.ts` -- Email account model
  - `folder.ts` -- Folder model
  - `label.ts` -- Label model (Gmail-style)
  - `category.ts` -- Abstract parent of Folder and Label
  - `event.ts` -- Calendar event model
  - `calendar.ts` -- Calendar model
  - `query.ts` -- `ModelQuery` builder for SQL queries
  - `query-subscription.ts` -- Live-updating reactive query subscription
  - `model-with-metadata.ts` -- Base for models supporting plugin metadata

**`app/unifymail-frontend/src/flux/stores/`:**
- Purpose: Application state management -- Flux stores
- Contains: 35+ stores managing all application state
- Key files:
  - `database-store.ts` -- Read-only SQLite access, triggers change events
  - `account-store.ts` -- Account management
  - `draft-store.ts` -- Draft composition state
  - `draft-editing-session.ts` -- Active draft editing session
  - `message-store.ts` -- Messages for the focused thread
  - `workspace-store.ts` -- Sheet stack, layout modes, location definitions
  - `focused-content-store.ts` -- Currently focused thread/message
  - `focused-perspective-store.ts` -- Currently active mailbox perspective
  - `task-queue.ts` -- Active and completed task tracking
  - `undo-redo-store.ts` -- Undo/redo stack for tasks
  - `identity-store.ts` -- User identity/authentication
  - `category-store.ts` -- Folders and labels
  - `contact-store.ts` -- Contact lookup and search
  - `online-status-store.ts` -- Network/sync status
  - `attachment-store.ts` -- File attachment management
  - `modal-store.tsx` -- Modal dialog state
  - `popover-store.tsx` -- Popover state

**`app/unifymail-frontend/src/flux/tasks/`:**
- Purpose: Task definitions for operations executed by the sync engine
- Contains: 28 task classes for all mail operations
- Key files:
  - `task.ts` -- Base `Task` class with status lifecycle and undo support
  - `send-draft-task.ts` -- Send an email
  - `destroy-draft-task.ts` -- Discard a draft
  - `change-folder-task.ts` -- Move messages between folders
  - `change-labels-task.ts` -- Add/remove Gmail labels
  - `change-starred-task.ts` -- Star/unstar messages
  - `change-unread-task.ts` -- Mark read/unread
  - `syncback-event-task.ts` -- Create/update calendar events
  - `syncback-metadata-task.ts` -- Sync plugin metadata
  - `task-factory.ts` -- Helper factory for creating common tasks

**`app/unifymail-frontend/src/components/`:**
- Purpose: Shared React UI component library
- Contains: ~70 reusable components
- Key files:
  - `composer-editor/` -- Rich text editor (Slate.js-based) for email composition
  - `multiselect-list.tsx` -- Virtualized list with multi-select
  - `list-tabular.tsx` -- Tabular list rendering
  - `scroll-region.tsx` -- Custom scrollbar component
  - `injected-component.tsx` -- Renders dynamically registered components by role/location
  - `injected-component-set.tsx` -- Renders all components registered at a location
  - `tokenizing-text-field.tsx` -- Token-based input (for To/CC/BCC fields)
  - `participants-text-field.tsx` -- Participant autocomplete field
  - `fixed-popover.tsx` -- Positioned popover component
  - `modal.tsx` -- Modal dialog component
  - `menu.tsx` -- Context/dropdown menu
  - `scenario-editor.tsx` -- Rule condition editor (for mail rules)
  - `attachment-items.tsx` -- Attachment display components
  - `retina-img.tsx` -- Retina-aware image component

**`app/unifymail-frontend/src/registries/`:**
- Purpose: Dynamic registration systems for extensibility
- Contains: 7 registry classes
- Key files:
  - `component-registry.ts` -- Maps React components to locations/roles/modes
  - `extension-registry.ts` -- `ComposerExtension`, `MessageViewExtension`, `ThreadListExtension`, `AccountSidebarExtension`
  - `database-object-registry.ts` -- Maps class names to constructors for JSON deserialization
  - `command-registry.ts` -- Keyboard command registration
  - `service-registry.ts` -- Named service registration
  - `sound-registry.ts` -- Sound effect registration

**`app/unifymail-frontend/src/global/`:**
- Purpose: Module barrel files that plugins import from
- Contains: Lazy-loaded exports accessible via `require('unifymail-exports')` and `require('unifymail-component-kit')`
- Key files:
  - `unifymail-exports.js` -- All core APIs: Actions, Models, Tasks, Stores, Utils, DatabaseStore, etc.
  - `unifymail-exports.d.ts` -- TypeScript declarations for exports
  - `unifymail-component-kit.js` -- All reusable UI components
  - `unifymail-component-kit.d.ts` -- TypeScript declarations for component kit
  - `unifymail-store.ts` -- Base `UnifyMailStore` class
  - `unifymail-observables.ts` -- RxJS observable helpers (`Rx.Observable.fromQuery`)

**`app/internal_packages/`:**
- Purpose: Feature implementation as self-contained plugins
- Contains: ~50 packages, each with `package.json`, `lib/`, optional `styles/`, `keymaps/`, `specs/`
- Key packages:
  - `composer/` -- Email composition UI and logic
  - `thread-list/` -- Thread list view, data source, toolbar
  - `message-list/` -- Message detail view, thread tracking
  - `account-sidebar/` -- Sidebar with accounts, folders, labels
  - `preferences/` -- Settings panels
  - `onboarding/` -- Account setup wizard (IMAP, Gmail OAuth, Exchange, etc.)
  - `main-calendar/` -- Calendar view
  - `contacts/` -- Contact management
  - `notifications/` -- Desktop notification handling
  - `undo-redo/` -- Undo/redo toast UI
  - `draft-list/` -- Drafts folder view
  - `events/` -- Calendar event rendering in email
  - `send-later/` -- Scheduled sending
  - `send-reminders/` -- Follow-up reminders
  - `link-tracking/` -- Link click tracking
  - `open-tracking/` -- Read receipt tracking
  - `translation/` -- Email translation
  - `phishing-detection/` -- Phishing warning
  - `attachments/` -- Attachment handling
  - `theme-picker/` -- Theme selection UI
  - `ui-dark/`, `ui-light/`, `ui-darkside/`, `ui-less-is-more/`, `ui-taiga/`, `ui-ubuntu/` -- Theme packages

**`app/spec/`:**
- Purpose: Jasmine test suites for core framework code
- Contains: Test files organized to mirror the source structure
- Key files:
  - `spec-runner/spec-bootstrap.ts` -- Test runner bootstrap
  - `unifymail-test-utils.ts` -- Shared test utilities
  - `fixtures/` -- Test data and fixture packages

## Key File Locations

**Entry Points:**
- `app/unifymail-backend/src/main.js`: Application entry point (main process)
- `app/unifymail-backend/src/application.ts`: Application singleton
- `app/static/index.html`: Window HTML shell
- `app/static/index.js`: Window bootstrap loader
- `app/unifymail-frontend/src/window-bootstrap.ts`: Main window renderer bootstrap
- `app/unifymail-frontend/src/secondary-window-bootstrap.ts`: Secondary window bootstrap
- `app/unifymail-frontend/src/app-env.ts`: `AppEnv` global singleton

**Configuration:**
- `app/package.json`: App-level package manifest with dependencies
- `package.json`: Root package manifest with dev dependencies and scripts
- `app/tsconfig.json`: TypeScript configuration
- `app/default-config/config.json`: Default application configuration
- `app/keymaps/base.json`: Default keyboard shortcuts
- `app/menus/darwin.js` / `linux.js` / `win32.js`: Platform menus

**Core Logic:**
- `app/unifymail-frontend/src/flux/actions.ts`: All Reflux actions
- `app/unifymail-frontend/src/flux/mailsync-bridge.ts`: Sync engine communication
- `app/unifymail-frontend/src/flux/action-bridge.ts`: Cross-window IPC
- `app/unifymail-frontend/src/mailsync-process.ts`: Sync engine process management
- `app/unifymail-frontend/src/global/unifymail-exports.js`: Plugin API surface
- `app/unifymail-frontend/src/global/unifymail-component-kit.js`: Component API surface

**Testing:**
- `app/spec/`: All test specs
- `app/spec/spec-runner/`: Test runner infrastructure
- `app/spec/fixtures/`: Test fixtures

## Naming Conventions

**Files:**
- Kebab-case for all source files: `draft-editing-session.ts`, `query-subscription.ts`
- `.ts` for TypeScript logic, `.tsx` for files containing JSX
- `.js` for legacy JavaScript files and entry points (`main.js`, `index.js`)
- `*.less` for stylesheets within plugins
- Test files use `-spec` suffix: `mail-rules-processor-spec.ts`

**Directories:**
- Kebab-case for all directories: `internal_packages`, `composer-editor`, `flux`
- Exception: `internal_packages` and `internal_packages_disabled` use underscores (legacy)
- Plugin directories match their `name` in `package.json`

**Classes and Types:**
- PascalCase for classes: `DatabaseStore`, `MailsyncBridge`, `QuerySubscription`
- PascalCase for React components: `ComposerView`, `ThreadList`, `InjectedComponent`
- Static `displayName` property required on all registered React components

**Exports:**
- Models and Tasks are PascalCase named exports: `export class Thread extends ModelWithMetadata`
- Stores are PascalCase default exports: `export default class DatabaseStore extends UnifyMailStore`
- Actions are camelCase: `Actions.queueTask`, `Actions.focusThread`

## Where to Add New Code

**New Feature Plugin:**
- Create directory: `app/internal_packages/{feature-name}/`
- Add `package.json` with `name`, `main: "./lib/main"`, `windowTypes`, and optionally `syncInit: true`
- Add `lib/main.ts` (or `main.tsx`) with `export function activate()` and `export function deactivate()`
- Add `lib/` directory for plugin source files
- Add `styles/` directory for LESS stylesheets (optional)
- Add `keymaps/` directory for keyboard shortcuts (optional)
- Register components via `ComponentRegistry.register()` in `activate()`

**New React Component (shared/reusable):**
- Add to: `app/unifymail-frontend/src/components/{component-name}.tsx`
- Export in: `app/unifymail-frontend/src/global/unifymail-component-kit.js` using `lazyLoad('ComponentName', 'component-name')`
- Add TypeScript declaration in: `app/unifymail-frontend/src/global/unifymail-component-kit.d.ts`

**New Flux Model:**
- Add model class to: `app/unifymail-frontend/src/flux/models/{model-name}.ts`
- Extend `Model` or `ModelWithMetadata`
- Define `static attributes` with `Attributes.*` types
- Register in: `app/unifymail-frontend/src/global/unifymail-exports.js` using `lazyLoadAndRegisterModel('ModelName', 'model-name')`

**New Flux Task:**
- Add task class to: `app/unifymail-frontend/src/flux/tasks/{task-name}.ts`
- Extend `Task`
- Register in: `app/unifymail-frontend/src/global/unifymail-exports.js` using `lazyLoadAndRegisterTask('TaskName', 'task-name')`

**New Flux Store:**
- Add store class to: `app/unifymail-frontend/src/flux/stores/{store-name}.ts`
- Extend `UnifyMailStore` (from `unifymail-store`)
- Use `listenTo()` to subscribe to actions/other stores
- Register in: `app/unifymail-frontend/src/global/unifymail-exports.js` using `load('StoreName', 'flux/stores/store-name')`
- Use `load()` (not `lazyLoad()`) so store is instantiated immediately on startup

**New Service:**
- Add to: `app/unifymail-frontend/src/services/{service-name}.ts`
- Register in `unifymail-exports.js` if plugins need access

**New Extension Type:**
- Add base class to: `app/unifymail-frontend/src/extensions/{extension-name}.ts`
- Create registry in: `app/unifymail-frontend/src/registries/extension-registry.ts` (add new `Registry` instance)

**New Test Spec:**
- Add to: `app/spec/{category}/{thing-being-tested}-spec.ts`
- Mirror the source directory structure under `app/spec/`
- For plugin specs: `app/internal_packages/{package-name}/specs/`

**New Localization:**
- Add locale JSON to: `app/lang/{locale-code}.json`
- Use `localized('string key')` in code (imported from `unifymail-exports`)

## Special Directories

**`app/node_modules/`:**
- Purpose: App-level npm dependencies (React, Electron modules, better-sqlite3, etc.)
- Generated: Yes (via `npm install` in `app/`)
- Committed: No

**`node_modules/`:**
- Purpose: Root dev dependencies (TypeScript, ESLint, Electron, Grunt, etc.)
- Generated: Yes (via `npm install` at root)
- Committed: No

**`app/mailsync` / `app/mailcore/`:**
- Purpose: Native C++ sync engine binary and IMAP/SMTP bindings
- Generated: Downloaded/built during setup (mailsync.tar.gz)
- Committed: Binary not committed; `mailcore/` contains native addon build config

**`app/default-config/`:**
- Purpose: Default configuration files copied to user's config directory on first run
- Generated: No
- Committed: Yes

**`app/internal_packages_disabled/`:**
- Purpose: Packages that are available but not loaded by default
- Generated: No
- Committed: Yes

**`workers/auth-proxy/`:**
- Purpose: Cloudflare Worker that proxies OAuth authentication requests
- Generated: No
- Committed: Yes
- Deployed separately via `wrangler`

**`build/`:**
- Purpose: Grunt-based build system for packaging the app for distribution
- Generated: No
- Committed: Yes

**`snap/`:**
- Purpose: Snapcraft packaging configuration for Linux Snap distribution
- Generated: No
- Committed: Yes

---

*Structure analysis: 2026-03-01*
