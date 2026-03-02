# Architecture

**Analysis Date:** 2026-03-01

## Pattern Overview

**Overall:** Multi-process Electron application using Flux (Reflux) unidirectional data flow with a plugin architecture. The UI is strictly read-only with respect to the database; all writes are performed by an external C++ sync engine (UnifyMail-Sync / mailsync) that communicates with the Electron app via stdin/stdout JSON streams.

**Key Characteristics:**
- Two-process model: Electron main process (backend) + renderer process (frontend), plus per-account C++ mailsync child processes
- Flux pattern: Actions -> MailsyncBridge -> Sync Engine -> Database deltas -> DatabaseStore -> Stores -> React Components
- Plugin system: Internal packages register components/extensions at runtime via registries
- Multi-window: Main window, onboarding, calendar, contacts, composer popout, thread popout, spec runner -- coordinated via IPC bridges
- Observable database: QuerySubscription + RxJS provide live-updating reactive queries

## Layers

**Main Process (Backend):**
- Purpose: Application lifecycle, window management, system integration, IPC routing
- Location: `app/backend/`
- Contains: `Application` singleton, `WindowManager`, `AutoUpdateManager`, `SystemTrayManager`, menu/touchbar management, protocol handlers
- Depends on: Electron APIs, some shared frontend modules (config, intl, mailsync-process)
- Used by: Renderer processes via IPC (`ipcMain`/`ipcRenderer`)

**Renderer Process (Frontend):**
- Purpose: UI rendering, state management, plugin hosting, sync engine communication
- Location: `app/frontend/`
- Contains: React components, Flux stores/actions/models/tasks, services, registries, AppEnv global
- Depends on: Electron renderer APIs, `@electron/remote`, better-sqlite3 (read-only), React, Reflux, RxJS
- Used by: Internal packages (plugins) via `unifymail-exports` and `unifymail-component-kit` modules

**Flux Layer:**
- Purpose: Unidirectional data flow -- Actions, Stores, Models, Tasks
- Location: `app/frontend/flux/`
- Contains:
  - `actions.ts` -- Reflux actions with scopes (window/global/main)
  - `stores/` -- Flux stores extending `UnifyMailStore`
  - `models/` -- Data models extending `Model` with attribute-based serialization
  - `tasks/` -- Async operation descriptors sent to the sync engine
  - `mailsync-bridge.ts` -- Bridge between Flux actions and mailsync child processes
  - `action-bridge.ts` -- IPC bridge for cross-window action propagation
- Depends on: Reflux, better-sqlite3, underscore
- Used by: All UI components and plugins

**Plugin Layer (Internal Packages):**
- Purpose: Feature implementation as self-contained packages with lifecycle hooks
- Location: `app/internal_packages/`
- Contains: 50+ packages implementing composer, thread-list, message-list, preferences, themes, onboarding, calendar, contacts, notifications, tracking features, etc.
- Depends on: `unifymail-exports` and `unifymail-component-kit` modules for core APIs
- Used by: PackageManager discovers and activates packages based on `windowTypes` in their `package.json`

**Component Layer:**
- Purpose: Reusable UI primitives and shared React components
- Location: `app/frontend/components/`
- Contains: 70+ React components (lists, editors, modals, popovers, scroll regions, attachment items, etc.)
- Depends on: React, classnames, Flux stores
- Used by: Exported via `unifymail-component-kit` module for use by internal packages

**Registry Layer:**
- Purpose: Dynamic registration of components, extensions, database objects, commands, services, and sounds
- Location: `app/frontend/registries/`
- Contains:
  - `component-registry.ts` -- Maps React components to UI locations/roles
  - `extension-registry.ts` -- Registers composer/message-view/thread-list/sidebar extensions
  - `database-object-registry.ts` -- Registers Model and Task subclasses for deserialization
  - `command-registry.ts` -- Registers keyboard commands
  - `service-registry.ts` -- Registers named services
  - `sound-registry.ts` -- Registers notification sounds
- Depends on: `UnifyMailStore`
- Used by: Plugins (register in `activate()`), framework (lookup at render/deserialize time)

**Services Layer:**
- Purpose: HTML transformation, search, sanitization
- Location: `app/frontend/services/`
- Contains: `autolinker.ts`, `sanitize-transformer.ts`, `quoted-html-transformer.ts`, `inline-style-transformer.ts`, search query parser/AST/backends
- Depends on: DOMPurify, cheerio
- Used by: Message rendering pipeline, composer, search

**Sync Engine (External C++ Process):**
- Purpose: IMAP/SMTP communication, local database writes, task execution
- Location: Binary at `app/mailsync` (or `app/mailsync.cmd` on Windows), spawned as child process
- Contains: C++ compiled binary (`UnifyMail-Sync`)
- Depends on: libcurl, SQLite (write access), IMAP/SMTP libraries
- Used by: `MailsyncProcess` class in `app/frontend/mailsync-process.ts`

## Data Flow

**User Action to Database Update (Task Flow):**

1. User performs action in UI (e.g., stars a thread)
2. Plugin calls `Actions.queueTask(new ChangeStarredTask({ thread, starred: true }))`
3. `MailsyncBridge._onQueueTask()` validates the task and sends JSON via `client.sendMessage()` to the account's mailsync child process stdin
4. Sync engine receives task, executes local database changes, then syncs remotely with mail server
5. Sync engine streams JSON deltas (DatabaseChangeRecords) back via stdout
6. `MailsyncBridge._onIncomingMessages()` parses deltas, creates `DatabaseChangeRecord` objects
7. `DatabaseStore.trigger(record)` notifies all subscribers
8. `QuerySubscription` instances re-evaluate their queries and push updated results to React components
9. React components re-render with new data

**Cross-Window Action Propagation:**

1. Action fired in secondary window (e.g., composer popout)
2. `ActionBridge` serializes action args to JSON
3. IPC message sent via `ipcRenderer.send('action-bridge-rebroadcast-to-default', ...)` or `action-bridge-rebroadcast-to-all`
4. Main process `Application.handleEvents()` routes to appropriate windows via `windowManager.sendToAllWindows()`
5. `ActionBridge.onIPCMessage()` in target window deserializes and re-fires the action locally

**Database Query Flow (Read-Only):**

1. Component creates a `QuerySubscription` with a `ModelQuery`: `new QuerySubscription(DatabaseStore.findAll(Thread).where({ unread: true }))`
2. `QuerySubscription` executes the SQL query against the read-only SQLite database via `DatabaseStore`
3. When `DatabaseStore.trigger()` fires with a `DatabaseChangeRecord`, subscription checks if the change is relevant
4. If relevant, subscription re-queries and pushes new results to callbacks
5. `Rx.Observable.fromQuery()` wraps this as an RxJS observable for reactive composition

**State Management:**
- Global application state is managed by Flux stores (30+ stores in `app/frontend/flux/stores/`)
- Each store extends `UnifyMailStore` which provides `listen()` and `trigger()` methods built on `EventEmitter`
- Stores listen to Actions and DatabaseStore changes, compute derived state, and trigger UI updates
- `AppEnv` singleton (assigned to `window.AppEnv`) provides access to config, packages, keymaps, commands, themes, styles
- No Redux or Context API -- pure Reflux/Flux pattern throughout

## Key Abstractions

**Model (`app/frontend/flux/models/model.ts`):**
- Purpose: Base class for all data entities (Thread, Message, Contact, Account, etc.)
- Examples: `app/frontend/flux/models/thread.ts`, `app/frontend/flux/models/message.ts`, `app/frontend/flux/models/contact.ts`
- Pattern: Declarative `static attributes` define schema for JSON serialization/deserialization, SQL query generation, and attribute matching. `__cls` field enables polymorphic deserialization via `DatabaseObjectRegistry`.

**Task (`app/frontend/flux/tasks/task.ts`):**
- Purpose: Represents an async mutation to be executed by the sync engine
- Examples: `app/frontend/flux/tasks/send-draft-task.ts`, `app/frontend/flux/tasks/change-starred-task.ts`, `app/frontend/flux/tasks/change-folder-task.ts`
- Pattern: Extends `Model` (so it is persisted in the database). Has status lifecycle (local -> remote -> complete/cancelled). Supports undo via `canBeUndone` + `createUndoTask()`. Queued via `Actions.queueTask()`.

**UnifyMailStore (`app/frontend/global/unifymail-store.ts`):**
- Purpose: Base class for Flux stores providing listen/trigger pub-sub
- Examples: `app/frontend/flux/stores/account-store.ts`, `app/frontend/flux/stores/draft-store.ts`
- Pattern: Stores extend `UnifyMailStore`, use `listenTo()` to subscribe to Actions/other stores, call `trigger()` to notify UI. React components subscribe in `componentDidMount` and unsubscribe in `componentWillUnmount`.

**QuerySubscription (`app/frontend/flux/models/query-subscription.ts`):**
- Purpose: Live-updating reactive query that automatically refreshes when underlying data changes
- Examples: Used throughout stores and components for data binding
- Pattern: Wraps a `ModelQuery`, executes it, listens to `DatabaseStore` for change records, re-evaluates when relevant changes arrive. Can be composed with `Rx.Observable.fromQuery()`.

**Package (`app/frontend/package.ts`):**
- Purpose: Encapsulates a plugin with metadata, lifecycle hooks, and window targeting
- Examples: Every directory in `app/internal_packages/` is a Package
- Pattern: `package.json` declares `windowTypes`, `main` entry point, `syncInit` flag. Entry module exports `activate()`, `deactivate()`, and optionally `serialize()`. Packages register components/extensions in `activate()` and unregister in `deactivate()`.

**ComponentRegistry (`app/frontend/registries/component-registry.ts`):**
- Purpose: Runtime registry mapping React components to named locations, roles, and workspace modes
- Examples: `ComponentRegistry.register(ComposerView, { role: 'Composer' })`, `ComponentRegistry.register(ComposeButton, { location: WorkspaceStore.Location.RootSidebar.Toolbar })`
- Pattern: Plugins register components by location (toolbar slot, sidebar, center) or role (Composer, MessageHeader). `InjectedComponent` and `InjectedComponentSet` render whatever is registered at a given location/role.

**WorkspaceStore (`app/frontend/flux/stores/workspace-store.ts`):**
- Purpose: Manages sheet stack (navigation), layout modes (list/split), and location definitions
- Examples: `WorkspaceStore.Sheet.Thread`, `WorkspaceStore.Location.ThreadList`, `WorkspaceStore.Location.RootSidebar.Toolbar`
- Pattern: Defines `Sheet` and `Location` objects dynamically. Components use `SheetContainer` to render the current sheet stack with transitions. Layout mode (list vs split) can be toggled.

**Global Module Exports:**
- `unifymail-exports` (`app/frontend/global/unifymail-exports.js`): Lazy-loaded barrel exposing all core APIs (Actions, Models, Tasks, Stores, Utils, etc.) to plugins via `require('unifymail-exports')`
- `unifymail-component-kit` (`app/frontend/global/unifymail-component-kit.js`): Lazy-loaded barrel exposing reusable UI components to plugins via `require('unifymail-component-kit')`
- Both use `Object.defineProperty` getters for lazy loading to improve startup performance

## Entry Points

**Application Entry (`app/backend/main.js`):**
- Location: `app/backend/main.js`
- Triggers: Electron `app.ready` event
- Responsibilities: Parse CLI args, set up config directory, initialize compile cache, create `Application` singleton, configure CSP headers, handle Squirrel (Windows installer) events

**Application Singleton (`app/backend/application.ts`):**
- Location: `app/backend/application.ts`
- Triggers: Created in `main.js` on app ready
- Responsibilities: Initialize all main-process managers (WindowManager, AutoUpdateManager, SystemTrayManager, ApplicationMenu, etc.), handle IPC events, manage window lifecycle, route URL/file opens

**Main Window Bootstrap (`app/frontend/window-bootstrap.ts`):**
- Location: `app/frontend/window-bootstrap.ts`
- Triggers: Loaded by `app/static/index.js` when main window's HTML page loads
- Responsibilities: Create `AppEnv` singleton, call `AppEnv.startRootWindow()` which initializes all renderer-side managers, loads packages, creates ActionBridge and MailsyncBridge, mounts React root

**Secondary Window Bootstrap (`app/frontend/secondary-window-bootstrap.ts`):**
- Location: `app/frontend/secondary-window-bootstrap.ts`
- Triggers: Loaded by hot windows (composer popouts, onboarding, calendar, contacts, thread popouts)
- Responsibilities: Create `AppEnv` singleton, call `AppEnv.startSecondaryWindow()` -- lighter initialization, no MailsyncBridge (receives changes via IPC rebroadcast from main window)

**Static HTML Shell (`app/static/index.html`):**
- Location: `app/static/index.html`
- Triggers: Loaded by each `BrowserWindow`
- Responsibilities: Loads `app/static/index.js` which reads load settings from URL query params, sets up compile cache, and requires the appropriate bootstrap script

**Package Entry Points (`app/internal_packages/*/lib/main.ts` or `main.tsx`):**
- Location: `app/internal_packages/{package-name}/lib/main.ts`
- Triggers: `PackageManager.activatePackage()` calls `require(pkg.main)`
- Responsibilities: Export `activate()` function that registers components/extensions, and `deactivate()` to clean up

## Error Handling

**Strategy:** Multi-layered error handling with global error logger, unrecoverable database error recovery, and crash tracking for sync workers.

**Patterns:**
- Global `process.on('uncaughtException')` and `process.on('unhandledRejection')` in main process route to `ErrorLogger` (`app/frontend/error-logger.js`)
- `handleUnrecoverableDatabaseError()` in `DatabaseStore` sends IPC to main process to reset database and relaunch
- `CrashTracker` in `MailsyncBridge` monitors sync process crashes -- if >5 crashes in 5 minutes, marks account as error state and stops relaunching
- Task errors are handled via `task.onError()` callback when sync engine reports completion with error
- React `componentDidCatch` in `SheetContainer` catches rendering errors and reports them
- `AppEnv.reportError()` available globally for renderer-side error reporting
- Sentry (`raven` package) integration for remote error reporting

## Cross-Cutting Concerns

**Logging:** `debug` module (`createDebug`) for debug-level logging in stores/database. `console.log`/`console.warn`/`console.error` throughout. `electron-log` for production logging in main process.

**Validation:** Task validation in `willBeQueued()` method. `MailsyncBridge._onQueueTask()` validates task is registered in `DatabaseObjectRegistry` and has `id` and `accountId`. Model attribute validation via typed `Attributes` (String, Boolean, Number, DateTime, Collection, etc.).

**Authentication:** Account credentials managed by `KeyManager` (`app/frontend/key-manager.ts`) which stores secrets separately from account JSON. OAuth handled via onboarding flow (`app/internal_packages/onboarding/`). Google auth uses `google-auth-library`. Microsoft OAuth uses direct HTTP with CSP header manipulation to remove Origin header.

**Localization:** `app/frontend/intl.ts` provides `localized()` function. 100+ locale JSON files in `app/lang/`. Supports RTL detection via `isRTL()`.

**Security:** CSP headers enforced both in `index.html` meta tag and via `session.defaultSession.webRequest.onHeadersReceived()`. `window.eval` disabled. HTML sanitization via DOMPurify (`app/frontend/services/sanitize-transformer.ts`). Inline style parsing via `juice` in main process IPC handler.

**Theming:** Theme packages in `app/internal_packages/ui-*` (dark, light, darkside, less-is-more, taiga, ubuntu). LESS stylesheets compiled at runtime. Tailwind CSS added as supplementary styling (`app/static/style/tailwind.css`).

---

*Architecture analysis: 2026-03-01*
