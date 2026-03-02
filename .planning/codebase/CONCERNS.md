# Codebase Concerns

**Analysis Date:** 2026-03-01

## Tech Debt

**CoffeeScript-to-TypeScript Migration Remnants:**
- Issue: 22 files still contain `decaffeinate suggestions` comments indicating they were auto-converted from CoffeeScript and may have non-idiomatic patterns (null checks, constructor workarounds).
- Files:
  - `app/unifymail-frontend/src/components/evented-iframe.tsx`
  - `app/unifymail-frontend/src/components/multiselect-list-interaction-handler.ts`
  - `app/unifymail-frontend/src/components/multiselect-split-interaction-handler.ts`
  - `app/unifymail-frontend/src/components/tab-group-region.tsx`
  - `app/unifymail-frontend/src/config.ts`
  - `app/unifymail-frontend/src/flux/models/utils.ts`
  - `app/unifymail-frontend/src/flux/stores/workspace-store.ts`
  - `app/unifymail-frontend/src/extensions/composer-extension.ts`
  - `app/unifymail-frontend/src/menu-helpers.ts`
  - `app/unifymail-backend/src/application-menu.ts`
  - `app/internal_packages/account-sidebar/lib/sidebar-section.ts`
  - `app/internal_packages/account-sidebar/lib/account-commands.ts`
  - `app/internal_packages/account-sidebar/lib/sidebar-actions.ts`
  - (and 9 more in spec files)
- Impact: Decaffeinated code often uses verbose null checks, unusual variable naming, and patterns that are harder to maintain. The `config.ts` file (965 lines) is particularly complex.
- Fix approach: Refactor files one at a time, replacing decaffeinate patterns with idiomatic TypeScript. Prioritize `config.ts` and `utils.ts` (831 lines) as they are widely imported.

**Lazy `require()` Calls Instead of Static Imports:**
- Issue: Multiple files use lazy `require()` to avoid circular dependencies or defer expensive module loading. At least 10 uses in `app/unifymail-frontend/src/mailbox-perspective.ts` alone, plus similar patterns in `draft-editing-session.ts`, `mailsync-bridge.ts`, and elsewhere.
- Files:
  - `app/unifymail-frontend/src/mailbox-perspective.ts` (lines 24-29, 153, 272, 306, 445-449, 535-537, 587, 601)
  - `app/unifymail-frontend/src/flux/stores/draft-editing-session.ts` (line 30)
  - `app/unifymail-frontend/src/flux/mailsync-bridge.ts` (lines 122, 145-146, 524-526)
  - `app/unifymail-frontend/src/flux/stores/account-store.ts` (line 56)
- Impact: Makes dependency graph opaque, prevents tree-shaking, complicates TypeScript type checking, and is a sign of circular dependency issues in the architecture.
- Fix approach: Resolve circular dependencies by extracting shared types/interfaces into separate modules. Replace lazy requires with proper ES module imports. For performance-sensitive lazy loading (like Slate in `draft-editing-session.ts`), use dynamic `import()` syntax instead.

**Outdated `fs.statSyncNoException` Monkey-Patching:**
- Issue: Both bootstrap files monkey-patch `fs` to add a removed Node.js API, with a TODO comment saying "Remove when upgrading to Electron 4" -- the project now uses Electron 39.
- Files:
  - `app/unifymail-frontend/src/window-bootstrap.ts` (lines 3-9)
  - `app/unifymail-frontend/src/secondary-window-bootstrap.ts` (lines 4-10)
- Impact: Pollutes the `fs` module globally, may mask real errors by silently swallowing exceptions.
- Fix approach: Find all callers of `fs.statSyncNoException` and replace with `fs.existsSync` or try/catch around `fs.statSync`. Remove the monkey-patch.

**Unimplemented `startsWith` Matcher Throws SQL Error:**
- Issue: The `startsWith` comparator in the query attribute matcher returns `RAISE 'TODO'` SQL, which would cause a database error if ever triggered.
- Files: `app/unifymail-frontend/src/flux/attributes/matcher.ts` (line 192)
- Impact: Any query using `startsWith` will crash. Currently unused but creates a latent trap for future developers.
- Fix approach: Implement the `startsWith` comparator using `LIKE 'value%'` SQL syntax, or throw a clear JavaScript error at query-build time rather than generating invalid SQL.

**Broken `forStandardCategories` Method:**
- Issue: `MailboxPerspective.forStandardCategories()` has a TODO comment explicitly stating "this method is broken."
- Files: `app/unifymail-frontend/src/mailbox-perspective.ts` (line 53)
- Impact: Any code path calling `forStandardCategories` may produce incorrect perspectives. The method is used by `forInbox()` (line 67) which is a critical navigation path.
- Fix approach: Investigate what `getCategoriesWithRoles` returns and fix the method to properly resolve categories. This is a high-priority fix since `forInbox()` depends on it.

**Unimplemented Navigation Actions:**
- Issue: Three navigation actions (`go-to-contacts`, `go-to-tasks`, `go-to-label`) are bound to empty no-op functions with TODO comments.
- Files: `app/unifymail-frontend/src/flux/stores/focused-perspective-store.ts` (lines 53-55)
- Impact: Keyboard shortcuts or menu items for these actions silently do nothing. Users may think the feature is broken.
- Fix approach: Either implement the navigation handlers or remove the keybindings/menu entries. For `go-to-contacts`, the contacts package exists at `app/internal_packages/contacts/`.

**Stale `hasAttachment` Query Using Slow LIKE Clause:**
- Issue: The search backend uses `data NOT LIKE '%"attachmentCount":0%'` to check for attachments instead of querying the dedicated `hasAttachment` column, with a TODO from 2018 saying to switch after DB version > 4.
- Files: `app/unifymail-frontend/src/services/search/search-query-backend-local.ts` (lines 249-253)
- Impact: Full-text scan of the JSON `data` column is significantly slower than an indexed column lookup. Affects search performance for `has:attachment` queries.
- Fix approach: Replace the LIKE clause with a direct query on `Thread.attachmentCount > 0` now that the column is populated by the sync engine.

**Duplicated `showIconForAttachments` Logic:**
- Issue: The `showIconForAttachments` utility function is duplicated in both the TypeScript frontend and the C++ sync engine, with a TODO noting it should be removed.
- Files: `app/unifymail-frontend/src/flux/models/utils.ts` (lines 31-38)
- Impact: Maintenance burden -- changes to attachment icon logic must be made in two places or they'll diverge.
- Fix approach: Remove the TypeScript implementation and rely solely on `Thread.attachmentCount` from the sync engine.

**Incomplete Undo for Multi-Folder Moves:**
- Issue: `ChangeFolderTask` cannot represent an undo operation when threads come from multiple folders. The undo task only supports a single `previousFolder`, so multi-folder moves are marked `canBeUndone = false`.
- Files: `app/unifymail-frontend/src/flux/tasks/change-folder-task.ts` (lines 59-69)
- Impact: Users who select threads from different folders and move them lose the ability to undo, with no notification about why.
- Fix approach: As the TODO suggests, make `createUndoTask()` return an array of tasks, one per original folder. Requires changes to `UndoRedoStore`.

**Remote Search (IMAP) Not Implemented:**
- Issue: `SearchQuerySubscription.performRemoteSearch()` is a complete no-op with a comment saying "Come back and implement this soon!"
- Files: `app/internal_packages/thread-search/lib/search-query-subscription.ts` (lines 78-86)
- Impact: Search only works against the local SQLite cache. Messages not yet synced locally will never appear in search results. This is a significant limitation for users with large mailboxes.
- Fix approach: Implement IMAP SEARCH integration through the mailsync bridge to query the server for results not available locally.

**Empty/Incomplete TODO Comments:**
- Issue: Several TODO comments are empty or cryptic, providing no guidance on what needs to be done.
- Files:
  - `app/unifymail-frontend/src/flux/stores/account-store.ts` (line 61): `// TODO:`
  - `app/internal_packages/draft-list/lib/draft-list-send-status.tsx` (line 18): `// TODO BG`
  - `app/unifymail-frontend/src/components/composer-editor/conversion.tsx` (line 238): `// TODO BG`
  - `app/internal_packages/thread-search/lib/search-query-subscription.ts` (line 27): `// TODO`
  - `app/internal_packages/thread-list/lib/thread-toolbar-buttons.tsx` (line 200): `// TODO BG REPLACE TASK FACTORY`
- Impact: Technical debt cannot be tracked or prioritized if the TODO provides no context.
- Fix approach: Either add meaningful descriptions or resolve them. The `REPLACE TASK FACTORY` one may indicate an incomplete migration.

## Known Bugs

**`Thread.snippet` Attribute Marked NONFUNCTIONAL:**
- Symptoms: The `snippet` attribute on Thread is declared with a TODO comment `// TODO NONFUNCTIONAL`, suggesting it may not be populated or queryable correctly.
- Files: `app/unifymail-frontend/src/flux/models/thread.ts` (line 43)
- Trigger: Any code relying on `Thread.snippet` for search or display may get empty/stale data.
- Workaround: Snippet appears to still be set via `fromJSON`, so display likely works, but the attribute may lack queryable indexing.

**Spellchecker `learnWord` May Not Work on Linux:**
- Symptoms: Custom words added via "Learn Spelling" may not persist on Ubuntu.
- Files: `app/unifymail-frontend/src/spellchecker.ts` (line 66)
- Trigger: Right-click misspelled word > Learn Spelling on Ubuntu 20.12+
- Workaround: None documented. The TODO has been present since the code was written.

## Security Considerations

**Main Window Runs with `nodeIntegration: true` and `contextIsolation: false`:**
- Risk: The main renderer window has full Node.js access without context isolation. Any XSS vulnerability in rendered email content could lead to arbitrary code execution.
- Files: `app/unifymail-backend/src/unifymail-window.ts` (lines 108-109)
- Current mitigation: Email bodies are rendered in sandboxed iframes (EventedIFrame), and DOMPurify is used for HTML sanitization in the quick preview. The message-item-body component uses `nodeIntegration: false` for its webview.
- Recommendations: Migrate to `contextIsolation: true` with a preload script. This is a major effort but is the Electron security best practice. At minimum, audit all `dangerouslySetInnerHTML` usages (30+ across the codebase).

**`contextIsolation: false` in Print Window:**
- Risk: The print window also runs without context isolation.
- Files: `app/internal_packages/print/lib/print-window.ts` (lines 84-85)
- Current mitigation: Print window content is generated from trusted data (the user's own emails).
- Recommendations: Enable `contextIsolation: true` as the print window still loads potentially untrusted HTML.

**Potential SQL Injection in Search:**
- Risk: The `visitText` method in `MatchQueryExpressionVisitor` directly uses the token string without sanitization, with a TODO asking "Should we do anything about possible SQL injection attacks?"
- Files: `app/unifymail-frontend/src/services/search/search-query-backend-local.ts` (lines 74-76)
- Current mitigation: The search text goes into an FTS5 MATCH clause (line 273) which has its own quoting (single quotes escaped on line 271), but the intermediate `visitText` does not escape. The database is read-only in the renderer, limiting damage.
- Recommendations: Sanitize all user input before incorporating into SQL strings. Use parameterized queries or at minimum escape special characters in the text visitor.

**`dangerouslySetInnerHTML` with Unsanitized Content:**
- Risk: The `UneditableNode` composer plugin renders raw HTML from Slate node data via `dangerouslySetInnerHTML` without sanitization.
- Files: `app/unifymail-frontend/src/components/composer-editor/uneditable-plugins.tsx` (lines 15, 33)
- Current mitigation: The HTML comes from the user's own draft content (pasted tables, images, etc.), not from external sources.
- Recommendations: Run the HTML through DOMPurify before rendering, especially since email content being replied to or forwarded could contain malicious HTML.

**Hardcoded OAuth Client IDs in Source:**
- Risk: Google and Microsoft OAuth client IDs are committed to the repository.
- Files: `app/internal_packages/onboarding/lib/onboarding-constants.ts` (lines 6-8, 19-20)
- Current mitigation: OAuth client IDs are considered semi-public (they are embedded in distributed client applications). Client secrets are not present. Environment variables can override via `MS_GMAIL_CLIENT_ID` and `MS_O365_CLIENT_ID`.
- Recommendations: Document that these are public client IDs. Ensure no client secrets are ever added to this file.

**Sentry DSN Hardcoded:**
- Risk: The Sentry DSN is hardcoded in the error reporter, allowing anyone to send error reports to the project's Sentry instance.
- Files: `app/unifymail-frontend/src/error-logger-extensions/raven-error-reporter.js` (line 53)
- Current mitigation: Sentry DSNs are considered semi-public and rate limiting is typically configured server-side.
- Recommendations: Consider using environment-specific DSNs or at minimum ensuring Sentry rate limits are configured.

**Heavy Use of `@electron/remote`:**
- Risk: `@electron/remote` exposes the main process to the renderer, effectively bypassing any security boundary. 30+ files use it directly.
- Files: Pervasive across `app/unifymail-frontend/src/` and `app/internal_packages/`
- Current mitigation: None -- the module is required for core functionality (dialogs, file operations, window management).
- Recommendations: Gradually migrate to IPC-based communication using `ipcRenderer.invoke()` / `ipcMain.handle()`. This is a large-scale effort but significantly improves security posture. Start with high-risk usages like `dialog.showOpenDialog` and `shell.openExternal`.

## Performance Bottlenecks

**Large File Complexity:**
- Problem: Several core files are very large, making them slow to parse and difficult to maintain.
- Files:
  - `app/unifymail-frontend/src/components/composer-editor/categorized-emoji.ts` (1311 lines) - static data, low risk
  - `app/unifymail-frontend/src/components/tokenizing-text-field.tsx` (1026 lines)
  - `app/unifymail-frontend/src/config.ts` (965 lines)
  - `app/unifymail-frontend/src/app-env.ts` (951 lines)
  - `app/unifymail-frontend/src/flux/models/utils.ts` (831 lines)
  - `app/internal_packages/main-calendar/lib/core/unifymail-calendar.tsx` (684 lines)
  - `app/unifymail-frontend/src/flux/models/contact.ts` (654 lines)
- Cause: Organic growth without decomposition. Many of these files serve as "god objects" combining multiple responsibilities.
- Improvement path: Extract logical sub-modules. For example, `utils.ts` contains model serialization, deep comparison, email parsing, and date formatting -- each could be its own module.

**Slow hasAttachment Search Query:**
- Problem: `has:attachment` search queries scan the JSON `data` column using `NOT LIKE '%"attachmentCount":0%'` instead of using the indexed `hasAttachment` column.
- Files: `app/unifymail-frontend/src/services/search/search-query-backend-local.ts` (line 253)
- Cause: Historical -- the indexed column was added later but the query was never updated.
- Improvement path: Replace with `Thread.attachmentCount > 0` column check.

**Background Query Agent Spawning:**
- Problem: `DatabaseStore` spawns a child process (`database-agent.js`) for background queries. If this process dies, it falls back to in-process execution silently, potentially blocking the UI thread.
- Files: `app/unifymail-frontend/src/flux/stores/database-store.ts` (lines 347-384)
- Cause: The agent pattern is a valid optimization, but error recovery is silent and could lead to unnoticed performance degradation.
- Improvement path: Add monitoring/logging when the agent dies and falls back to in-process mode. Consider using a worker thread instead of a child process for lower overhead.

**Slate Editor Lazy Loading Hack:**
- Problem: The draft editing session uses a lazy `require()` to defer loading Slate (the rich text editor), acknowledging it as a "hack" that saves ~400ms of startup time.
- Files: `app/unifymail-frontend/src/flux/stores/draft-editing-session.ts` (lines 22-31)
- Cause: Slate and its dependencies are heavy but only needed when composing.
- Improvement path: Use proper dynamic `import()` with code splitting. This would be cleaner and allow webpack/bundler optimizations.

## Fragile Areas

**Draft Body State Management (`hotwireDraftBodyState`):**
- Files: `app/unifymail-frontend/src/flux/stores/draft-editing-session.ts` (lines 49-119)
- Why fragile: Uses JavaScript property descriptors to monkey-patch the `body` property on draft Message objects, creating a bidirectional sync between HTML strings and Slate editor state. The `try/catch` on line 98 catches Slate schema errors where inserting a document fragment fails, falling back to blowing away undo history.
- Safe modification: Do not modify the body getter/setter logic without thorough testing of compose, reply, forward, and template insertion flows.
- Test coverage: `app/spec/stores/draft-editing-session-spec.ts` (282 lines) exists but may not cover all edge cases of the HTML-to-Slate conversion.

**Mailsync Process Communication:**
- Files:
  - `app/unifymail-frontend/src/mailsync-process.ts` (487 lines)
  - `app/unifymail-frontend/src/flux/mailsync-bridge.ts` (544 lines)
- Why fragile: Communication with the C++ sync engine happens via stdin/stdout JSON streaming. Buffer overflow, partial JSON messages, and process crashes are all handled but the code has many edge cases (EPIPE handling, buffer concatenation with `+=` on what's declared as a Buffer, cleanup guards).
- Safe modification: Any changes to the stdin/stdout protocol must be coordinated with the C++ mailsync binary. The mock at `scripts/mock-mailsync.js` must also be updated.
- Test coverage: No dedicated spec files found for `mailsync-process.ts` or `mailsync-bridge.ts`.

**Mailbox Perspective System:**
- Files: `app/unifymail-frontend/src/mailbox-perspective.ts` (609 lines)
- Why fragile: Uses a class cluster pattern with 5 subclasses (Empty, Drafts, Starred, Category, Unread). Each subclass has different behaviors for `tasksForRemovingItems`, `actionsForReceivingThreads`, and `threads()`. The interaction between perspective type, account type (Gmail labels vs IMAP folders), and task creation is complex and has many branches.
- Safe modification: Always test with both Gmail (label-based) and IMAP (folder-based) accounts. The `forStandardCategories` method is explicitly broken (line 53).
- Test coverage: `app/spec/mailbox-perspective-spec.ts` (240 lines) provides some coverage but may not cover all subclass interactions.

**Composer Editor Plugin System:**
- Files: `app/unifymail-frontend/src/components/composer-editor/` (16 files)
- Why fragile: The Slate editor integration relies on forked/pinned GitHub versions of Slate packages (`slate-react`, `slate-edit-list`) rather than npm releases. The Slate API is at version 0.4x which is several major versions behind current (0.9x+). Any Slate upgrade would require rewriting all plugins.
- Safe modification: Pin all Slate-related dependencies. Do not attempt to upgrade Slate without a full rewrite plan for all composer plugins.
- Test coverage: No dedicated spec files found for composer-editor components.

## Scaling Limits

**SQLite Local Database:**
- Current capacity: SQLite with WAL mode, 8KB page size, 20000 page cache. Read-only from renderer.
- Limit: Very large mailboxes (>100k threads) may experience slow queries, particularly full-text search and queries without proper indexes. The `LIKE '%..%'` attachment search pattern is O(n).
- Scaling path: Ensure all frequently-queried columns are indexed. Replace LIKE-based queries with indexed column lookups. The background query agent helps offload work but dies silently.

**Sync Process Per Account:**
- Current capacity: One mailsync C++ process spawned per configured email account.
- Limit: Each process consumes memory and CPU. Users with many accounts (>5-10) may experience resource pressure.
- Scaling path: The crash tracker already limits respawns (5 crashes in 5 minutes). Monitor memory usage per sync process.

## Dependencies at Risk

**React 16.9.0 (Severely Outdated):**
- Risk: React 16.9 is from 2019. It lacks concurrent features, hooks improvements, and security patches from React 17/18. The `componentWillMount` and `UNSAFE_componentWillReceiveProps` lifecycle methods (found in 4 files) are deprecated.
- Impact: Cannot use modern React patterns (hooks, Suspense, concurrent mode). Will become increasingly difficult to find compatible third-party components.
- Migration plan: Upgrade to React 18.x. This requires updating `react-dom`, `react-test-renderer`, enzyme (replace with React Testing Library), and auditing all class component lifecycle methods.

**Slate Rich Text Editor (Pinned to Forked GitHub Refs):**
- Risk: Multiple Slate packages are pinned to specific GitHub commits of a fork (`github:bengotow/slate#cd6f40e8`, `github:bengotow/slate#0.45.1-react`). This fork is unmaintained.
- Impact: No security updates, no bug fixes, no compatibility with modern React. If the GitHub repo is deleted, dependencies become unresolvable.
- Migration plan: Migrate to official Slate 0.9x (breaking changes) or switch to an alternative like TipTap, ProseMirror, or Lexical. This is a major effort affecting the entire composer.

**Raven (Sentry SDK) Version 2.1.2:**
- Risk: `raven` is the legacy Sentry SDK, deprecated since 2019. The current SDK is `@sentry/electron`.
- Impact: Missing modern error tracking features, source map support improvements, and performance monitoring.
- Migration plan: Replace `raven` with `@sentry/electron`. Update `app/unifymail-frontend/src/error-logger-extensions/raven-error-reporter.js` (currently still plain JS).
- Files: `app/unifymail-frontend/src/error-logger-extensions/raven-error-reporter.js`

**Underscore.js (Legacy Utility Library):**
- Risk: `underscore` is used extensively across the codebase when modern JavaScript/TypeScript has native equivalents for most utilities.
- Impact: Adds bundle size and creates inconsistency (some code uses `_.isEqual`, `_.difference`, `_.compact`, others use native methods).
- Migration plan: Gradually replace with native JS methods or lodash (which has better TypeScript support). Priority replacements: `_.compact` -> `.filter(Boolean)`, `_.difference` -> `Set`-based operations, `_.isEqual` -> deep-equal utility.

**Reflux 0.1.13 (Extremely Outdated):**
- Risk: Reflux 0.1.13 is from 2014. The `UnifyMailStore` base class wraps this for the Flux architecture.
- Impact: No modern state management patterns. The entire store system depends on this ancient library.
- Migration plan: This is deeply embedded in the architecture. A pragmatic approach is to keep it working but not add new stores using this pattern. New features could use React Context or a modern state library.

**Prettier 1.x:**
- Risk: Prettier 1.x is from 2018. Current is 3.x with significantly different formatting rules and TypeScript support improvements.
- Impact: Missing modern TypeScript formatting capabilities. May conflict with newer editor integrations.
- Migration plan: Upgrade to Prettier 3.x and run a full reformat. This will create a large diff but is a one-time cost.

## Missing Critical Features

**No Bundler/Build Optimization:**
- Problem: The application uses a custom compile-cache (`app/unifymail-frontend/src/compile-cache.js`) that transpiles TypeScript at runtime rather than pre-bundling with webpack, esbuild, or similar tools.
- Blocks: Tree-shaking, code splitting, minification, and other optimization techniques that would reduce startup time and memory usage.

**No Strict TypeScript:**
- Problem: The `tsconfig.json` does not enable `strict`, `noImplicitAny`, or `strictNullChecks`. The `skipLibCheck` is enabled.
- Blocks: TypeScript provides minimal safety -- `any` types are used 105+ times in the flux layer alone. Null reference errors at runtime that strict mode would catch at compile time.

## Test Coverage Gaps

**No Tests for Core Communication Layer:**
- What's not tested: `mailsync-process.ts` (487 lines) and `mailsync-bridge.ts` (544 lines) -- the entire sync engine communication layer.
- Files: `app/unifymail-frontend/src/mailsync-process.ts`, `app/unifymail-frontend/src/flux/mailsync-bridge.ts`
- Risk: Changes to process spawning, JSON parsing, crash recovery, or task queuing could introduce regressions undetected.
- Priority: High -- this is the most critical data path in the application.

**No Tests for Composer Editor:**
- What's not tested: The 16 files in `app/unifymail-frontend/src/components/composer-editor/` including conversion, plugin system, emoji handling, link editing, and markdown support.
- Files: `app/unifymail-frontend/src/components/composer-editor/*.tsx`
- Risk: Composer is user-facing and handles complex HTML manipulation. The `hotwireDraftBodyState` monkey-patching is particularly risky.
- Priority: High -- composing and sending email is the primary user action.

**No Tests for Backend Process Management:**
- What's not tested: `app/unifymail-backend/src/application.ts` (927 lines), `unifymail-window.ts` (460 lines), `window-manager.ts` (316 lines).
- Files: All files in `app/unifymail-backend/src/`
- Risk: Window lifecycle, auto-update, system tray, and application startup issues would go undetected.
- Priority: Medium -- these are critical but change less frequently.

**Limited Coverage Ratio:**
- What's not tested: With 103 spec files covering 614+ source files (280 frontend + 334 internal packages), roughly 83% of source files have no corresponding test.
- Risk: Broad regression risk across the application.
- Priority: Medium -- focus on high-traffic code paths first (mailsync, composer, database store, mailbox perspectives).

**Test Framework is Outdated:**
- What's not tested: Modern testing patterns. The project uses Jasmine 2.x with enzyme (for React 16), both of which are legacy choices.
- Files: `app/spec/` (61 files), internal_packages specs (42 files)
- Risk: Enzyme is no longer maintained and does not support React 18+. A React upgrade would require migrating all 103 test files.
- Priority: Low for now, but becomes blocking when React is upgraded.

---

*Concerns audit: 2026-03-01*
