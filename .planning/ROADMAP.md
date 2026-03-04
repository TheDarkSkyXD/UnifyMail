# Roadmap: UnifyMail v1.0 — Rewrite mailcore N-API in Rust

## Overview

Replace the `app/mailcore/` C++ N-API addon (backed by the full mailcore2 library) with a minimal Rust napi-rs implementation that exposes the same 5-function API. Work proceeds in strict dependency order: prove the Electron integration is sound before writing any network code, then implement the hardest async function (IMAP) first, then compose SMTP and account validation on top of proven patterns, and finally lock in cross-platform CI builds and delete all C++ artifacts.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [x] **Phase 1: Scaffolding and Provider Detection** - Prove Electron integration is sound; implement sync provider lookup with full regex cross-validation (completed 2026-03-03)
- [x] **Phase 2: IMAP Connection Testing** - Implement testIMAPConnection with all three TLS paths, XOAUTH2 auth, and 7 capability detections (completed 2026-03-03)
- [x] **Phase 3: SMTP Testing and Account Validation** - Implement testSMTPConnection and validateAccount composing all proven components (completed 2026-03-04)
- [x] **Phase 4: Cross-Platform Packaging and Cleanup** - GitHub Actions CI for all 5 targets, binary size validation, remove all C++ code (completed 2026-03-04)
- [ ] **Phase 4.1: CI Hardening and Smoke Test Expansion** - Expand CI smoke tests to cover all 5 exports, fix macOS cache key, delete stale index.js (Gap Closure)
- [ ] **Phase 4.2: validateAccount Integration Verification** - Verify validateAccount call site field mapping and E2E credential passing (Gap Closure)

## Phase Details

### Phase 1: Scaffolding and Provider Detection
**Goal**: The napi-rs addon loads cleanly in Electron main process and provider lookup works correctly against all 37 providers
**Depends on**: Nothing (first phase)
**Requirements**: SCAF-01, SCAF-02, PROV-01, PROV-02, PROV-03, PROV-04
**Success Criteria** (what must be TRUE):
  1. The Rust addon loads in Electron main process without crashes — `cargo tree | grep openssl` returns nothing, tokio runtime initializes, and no BoringSSL symbol conflicts appear
  2. Calling `registerProviders(jsonPath)` loads the provider database from a JSON file without error
  3. Provider database auto-initializes on module load via embedded `providers.json` (no explicit registerProviders call needed)
  4. Calling `providerForEmail(email)` returns a provider object with IMAP/SMTP configs for recognized domains
  5. Domain-regex matching produces identical results to the C++ addon for 50 representative email addresses (cross-validation test passes)
**Plans:** 2/2 plans complete

Plans:
- [x] 01-01-PLAN.md — Scaffold Rust napi-rs crate and implement provider detection logic with tests
- [ ] 01-02-PLAN.md — Wrapper module, Electron integration, cross-validation, and documentation

### Phase 2: IMAP Connection Testing
**Goal**: testIMAPConnection handles all three TLS paths, both auth methods, and correctly detects all 7 IMAP capabilities
**Depends on**: Phase 1
**Requirements**: IMAP-01, IMAP-02, IMAP-03, IMAP-04, IMAP-05, IMAP-06
**Success Criteria** (what must be TRUE):
  1. Calling testIMAPConnection with port 993 establishes a direct TLS connection and returns capability data
  2. Calling testIMAPConnection with STARTTLS negotiates the stream upgrade from plain TCP to TLS without hanging
  3. Calling testIMAPConnection with clear/unencrypted connects without TLS
  4. Both password and XOAUTH2 (SASL) authentication methods succeed against a live server
  5. The returned capabilities object correctly reports idle, condstore, qresync, compress, namespace, xoauth2, and gmail flags
  6. Any connection or auth attempt that takes longer than 15 seconds resolves with a timeout error rather than hanging
**Plans:** 2/2 plans complete

Plans:
- [x] 02-01-PLAN.md — Implement testIMAPConnection in Rust: all TLS paths, auth methods, capability detection, timeout, error classification
- [ ] 02-02-PLAN.md — Mock IMAP server test suite and wrapper switchover from C++ to Rust

### Phase 3: SMTP Testing and Account Validation
**Goal**: testSMTPConnection and validateAccount complete the 5-function API surface with full parity to the C++ addon
**Depends on**: Phase 2
**Requirements**: SMTP-01, SMTP-02, SMTP-03, SMTP-04, SMTP-05, VALD-01, VALD-02, VALD-03, VALD-04
**Success Criteria** (what must be TRUE):
  1. Calling testSMTPConnection with TLS, STARTTLS, or clear connects and issues a NOOP test via lettre transport
  2. Both password and XOAUTH2 SMTP authentication methods succeed against a live server
  3. Any SMTP connection attempt exceeding 15 seconds resolves with a timeout error
  4. Calling validateAccount runs IMAP and SMTP tests concurrently and returns a result shape matching the C++ output (success/error with server details)
  5. validateAccount resolves MX records via DNS for provider identifier matching in the result
  6. TypeScript type declarations auto-generated by napi-rs compile without errors against the existing consumer files
**Plans:** 2/2 plans complete

Plans:
- [ ] 03-01-PLAN.md — Implement testSMTPConnection in Rust with lettre transport, mock SMTP test suite
- [ ] 03-02-PLAN.md — Implement validateAccount with concurrent IMAP+SMTP+MX, wrapper switchover to Rust

### Phase 4: Cross-Platform Packaging and Cleanup
**Goal**: All 5 platform binaries build in CI, the release binary meets size targets, and every C++ artifact is deleted from the repository
**Depends on**: Phase 3
**Requirements**: SCAF-03, SCAF-04, INTG-01, INTG-02, INTG-03, INTG-04
**Success Criteria** (what must be TRUE):
  1. GitHub Actions CI produces binaries for all 5 targets: win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64
  2. The stripped Linux x64 release binary is under 8MB with LTO enabled
  3. onboarding-helpers.ts and mailsync-process.ts import the Rust addon via the existing `require('mailcore-napi')` path without modification
  4. All C++ source files, node-gyp configs, and vendored mailcore2 source are deleted from the repository
  5. node-addon-api and node-gyp are removed from package.json with no remaining references
**Plans:** 2/2 plans complete

Plans:
- [ ] 04-01-PLAN.md — Cargo release profile optimization, wrapper removal, C++ deletion, package.json rewiring
- [ ] 04-02-PLAN.md — Insert Rust build steps into all 4 CI workflows, remove C++ build infrastructure, add smoke tests

### Phase 4.1: CI Hardening and Smoke Test Expansion
**Goal**: All CI workflows smoke-test the complete 5-function API surface and build caches work correctly on all platforms
**Depends on**: Phase 4
**Requirements**: SCAF-03 (strengthened), IMAP-01, SMTP-01, VALD-01
**Gap Closure**: Closes INTG-CI-SMOKE, INTG-MACOS-CACHE from v1.0 audit
**Success Criteria** (what must be TRUE):
  1. All 4 CI workflows smoke-test all 5 exports: providerForEmail, registerProviders, testIMAPConnection, testSMTPConnection, validateAccount
  2. macOS CI cache key uses hashFiles('package-lock.json') instead of hashFiles('yarn.lock')
  3. Stale auto-generated index.js deleted from app/mailcore-rs/ (loader.js is the canonical entry point)
**Plans**: 1 plan

Plans:
- [ ] 04.1-01-PLAN.md — Shared smoke test script, CI workflow updates, macOS cache fix, index.js deletion

### Phase 4.2: validateAccount Integration Verification
**Goal**: The validateAccount call site in mailsync-process.ts is verified to pass all required fields including username, and the E2E credential flow is proven correct
**Depends on**: Phase 4
**Requirements**: INTG-02 (strengthened), VALD-01, VALD-03
**Gap Closure**: Closes INTG-VALIDATE-USERNAME, FLOW-VALIDATE-RUNTIME from v1.0 audit
**Success Criteria** (what must be TRUE):
  1. The validateAccount call site in mailsync-process.ts passes username field correctly to Rust ValidateAccountOptions
  2. An integration test verifies credential passing from TypeScript to Rust validateAccount matches expected field mapping
  3. The account validation E2E flow (mailsync-process.ts -> Rust validateAccount -> concurrent IMAP+SMTP+MX) is verified end-to-end
**Plans**: 1 plan

Plans:
- [ ] 04.2-01-PLAN.md — Split ValidateAccountOptions to per-protocol credentials, fix TS call site, field echo integration test

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 4.1 → 4.2

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Scaffolding and Provider Detection | 2/2 | Complete   | 2026-03-03 |
| 2. IMAP Connection Testing | 2/2 | Complete   | 2026-03-03 |
| 3. SMTP Testing and Account Validation | 2/2 | Complete   | 2026-03-04 |
| 4. Cross-Platform Packaging and Cleanup | 2/2 | Complete   | 2026-03-04 |
| 4.1 CI Hardening and Smoke Test Expansion | 0/1 | Not started | - |
| 4.2 validateAccount Integration Verification | 0/1 | Not started | - |

---

# Roadmap: UnifyMail v2.0 — Rewrite mailsync Engine in Rust

## Overview

Replace the `app/mailsync/` C++ sync engine (~16,200 LOC, 50 source files) with a standalone Rust binary that maintains exact wire-format compatibility with the existing TypeScript `MailsyncBridge` in Electron. The Rust binary is a drop-in replacement — same stdin/stdout newline-delimited JSON protocol, same SQLite schema, same process modes. Work proceeds in strict dependency order: protocol and database infrastructure first (Phases 5-6), then IMAP background sync (Phase 7), then IDLE and task execution (Phase 8), then CalDAV/CardDAV and metadata (Phase 9), and finally cross-platform packaging and C++ deletion (Phase 10).

**Depends on:** v1.0 completion (Phases 1-4)

## Phases

- [ ] **Phase 5: Core Infrastructure and IPC Protocol** - Rust binary skeleton with correct stdin/stdout protocol, all process modes, SQLite schema creation, and delta emission pipeline — the foundation all IMAP phases depend on
- [ ] **Phase 6: SQLite Layer and Model Infrastructure** - Complete MailStore with all data models, WAL mode, single-writer pattern, and schema migrations matching the C++ baseline
- [ ] **Phase 7: IMAP Background Sync Worker** - Full IMAP sync against live accounts: folder enumeration, CONDSTORE/UID-range incremental sync, body caching, OAuth2, and Gmail-specific behaviors
- [ ] **Phase 8: Foreground IDLE and Task Execution** - IMAP IDLE monitoring, task processor for all 13+ task types, SMTP send via lettre, and crash recovery
- [ ] **Phase 9: CalDAV, CardDAV, and Metadata Workers** - Calendar and contact sync via libdav, Gmail Google People API contacts, and metadata HTTP long-polling worker
- [ ] **Phase 10: Cross-Platform Builds, Packaging, and C++ Deletion** - Verified binaries for all 5 targets, asar unpacking, binary size validation, and complete C++ source deletion

## Phase Details

### Phase 5: Core Infrastructure and IPC Protocol
**Goal**: The Rust binary skeleton handles all process modes correctly with a proven stdin/stdout protocol, delta emission pipeline, and SQLite schema — every subsequent phase can be tested end-to-end with the Electron UI
**Depends on**: Phase 4 (v1.0 complete)
**Requirements**: IPC-01, IPC-02, IPC-03, IPC-04, IPC-05, IPC-06, IMPR-08
**Success Criteria** (what must be TRUE):
  1. The binary starts up, reads the two-line stdin handshake (account JSON then identity JSON), and emits a valid `ProcessState` delta to Electron without crashing
  2. Running the binary with `--mode migrate` creates the SQLite schema with all tables and indexes; running with `--mode install-check` exits 0; running with `--mode reset` clears state; all modes exit with expected codes
  3. Delta messages emitted to stdout have exact field names `modelJSONs`, `modelClass`, and `type` — a contract test validates these against the TypeScript parser before any IMAP code is written
  4. The binary detects stdin EOF (parent process closed pipe) and exits with code 141
  5. stdout is explicitly flushed after every message with no block buffering — the Electron UI receives deltas in real time during a 10-second idle test
  6. The stdin reader and stdout writer run as independent tokio tasks — a large payload (500KB+) on stdin does not deadlock with concurrent stdout writes
**Plans**: 2 plans

Plans:
- [ ] 05-01-PLAN.md — Binary crate scaffold, CLI parsing, error types, SQLite schema migrations, and offline modes (migrate, install-check, reset)
- [ ] 05-02-PLAN.md — Delta emission pipeline with coalescing, stdin handshake and command loop, sync mode skeleton, npm start integration, and mailsync-process.ts coexistence

### Phase 6: SQLite Layer and Model Infrastructure
**Goal**: The complete MailStore is proven correct — all data models persist and round-trip through the database with WAL mode, tokio-rusqlite single-writer access, and delta emission with 500ms coalescing
**Depends on**: Phase 5
**Requirements**: DATA-01, DATA-02, DATA-03, DATA-04, DATA-05
**Success Criteria** (what must be TRUE):
  1. All 13 data model types (Message, Thread, Folder, Label, Contact, ContactBook, ContactGroup, Calendar, Event, Task, File, Identity, ModelPluginMetadata) serialize to and deserialize from SQLite correctly
  2. The database operates in WAL mode with `busy_timeout=5000` — concurrent reads proceed while a write is in progress without returning SQLITE_BUSY
  3. Delta emission produces persist/unpersist messages with a 500ms coalescing window — repeated saves of the same model within 500ms emit a single delta, not one per save
  4. The SQLite schema matches the C++ baseline — all tables, indexes, and FTS5 virtual tables (ThreadSearch, EventSearch, ContactSearch) exist after migration
  5. All database writes go through the tokio-rusqlite single-writer connection — no synchronous rusqlite calls appear on async tokio threads
**Plans**: TBD

Plans:
- [ ] 06-01: TBD

### Phase 7: IMAP Background Sync Worker
**Goal**: The background sync worker syncs a real email account end-to-end — folders enumerate, messages appear in the Electron UI via delta emission, OAuth2 tokens refresh automatically, and Gmail-specific behaviors produce correct results
**Depends on**: Phase 6
**Requirements**: ISYN-01, ISYN-02, ISYN-03, ISYN-04, ISYN-05, ISYN-06, ISYN-07, OAUT-01, OAUT-02, OAUT-03, GMAL-01, GMAL-02, GMAL-03, GMAL-04, IMPR-05, IMPR-06
**Success Criteria** (what must be TRUE):
  1. After connecting a new IMAP account, all folders enumerate with correct role assignments (Inbox, Sent, Drafts, Trash, Spam, Archive) and appear in the Electron sidebar
  2. New messages arriving in an already-synced folder are detected on the next background sync cycle (CONDSTORE modseq-based) without fetching already-known messages again; servers without CONDSTORE fall back to UID range sync
  3. When UIDVALIDITY changes on a folder, the worker discards all cached UIDs and performs a full re-sync of that folder per RFC 4549
  4. Message body requests sent via `need-bodies` stdin command are fetched in priority order and cached; messages older than the per-folder age policy (3 months) are not pre-fetched
  5. Gmail accounts show only INBOX, All Mail, Trash, and Spam folders; X-GM-LABELS, X-GM-MSGID, and X-GM-THRID extension data are parsed and stored on message records
  6. OAuth2 tokens are checked for expiry within a 5-minute buffer before every IMAP authenticate; expired tokens refresh automatically and the updated credentials emit a `ProcessAccountSecretsUpdated` delta to the UI
  7. All IMAP network operations complete within their per-operation timeout or resolve with a structured error classifying the failure as auth, TLS, network, or server error
**Plans**: TBD

Plans:
- [ ] 07-01: TBD

### Phase 8: Foreground IDLE and Task Execution
**Goal**: Users can send email, move messages, and change flags from the Electron UI with changes reflected immediately — the foreground IDLE worker monitors for new mail in real time and executes all task types reliably with crash recovery
**Depends on**: Phase 7
**Requirements**: IDLE-01, IDLE-02, IDLE-03, SEND-01, SEND-02, SEND-03, SEND-04, TASK-01, TASK-02, TASK-03, TASK-04, TASK-05, IMPR-07
**Success Criteria** (what must be TRUE):
  1. A new message arriving in the primary folder while IDLE is active is detected and synced to the Electron UI within a few seconds — the IDLE connection re-issues every 25 minutes to prevent the 29-minute server-side timeout from causing a silent disconnect
  2. A `queue-task` stdin command interrupts IDLE immediately via the internal tokio channel, executes the task local phase (DB write + delta) synchronously, and proceeds to the remote phase
  3. Sending a draft email succeeds via SMTP with TLS, STARTTLS, or clear connection; password and XOAUTH2 authentication both work; MIME construction handles multipart with attachments and inline images
  4. All 8+ task types (SendDraft, DestroyDraft, ChangeLabels, ChangeFolder, ChangeStarred, ChangeUnread, SyncbackMetadata, SyncbackEvent, contact/calendar tasks) execute local and remote phases correctly and emit completion deltas
  5. On startup after a crash, tasks stuck in `remote` state are reset to `local` state and re-queued; completed tasks expire after the configurable period
  6. The IDLE connection uses a dedicated IMAP session separate from the background sync session — both sessions run concurrently without protocol errors
  7. Body sync progress updates emit to the UI during large syncs so the user sees incremental message loading rather than a long pause
**Plans**: TBD

Plans:
- [ ] 08-01: TBD

### Phase 9: CalDAV, CardDAV, and Metadata Workers
**Goal**: Calendar events and contacts sync bidirectionally for standard CalDAV/CardDAV providers and Gmail accounts; plugin metadata long-polls and persists correctly
**Depends on**: Phase 6
**Requirements**: CDAV-01, CDAV-02, CDAV-03, CDAV-04, CDAV-05, CRDV-01, CRDV-02, CRDV-03, CRDV-04, META-01, META-02, META-03
**Success Criteria** (what must be TRUE):
  1. Connecting an account with CalDAV support enumerates all calendars via service discovery and the `sync-calendar` stdin command triggers a sync-collection REPORT that delivers new and changed events to the Electron UI
  2. Creating, updating, and deleting a calendar event from Electron executes the CalDAV PUT/DELETE request with correct ETag handling — servers that omit ETag on PUT trigger a GET-after-PUT fallback so the stored record stays consistent
  3. Connecting an account with CardDAV support enumerates contacts via service discovery; incremental sync via sync-collection REPORT delivers contact changes to the Electron UI
  4. Creating, updating, and deleting contacts via CardDAV PUT/DELETE and via the Gmail Google People API (for Gmail accounts) both work correctly
  5. A server responding with `429 Too Many Requests` or `503 Service Unavailable` including `Retry-After` causes the worker to back off for the indicated duration before retrying
  6. The metadata worker long-polls the identity server and delivers plugin metadata; stale metadata is cleaned up by the expiration worker; non-retryable metadata errors stop only the metadata worker without affecting IMAP sync
**Plans**: TBD

Plans:
- [ ] 09-01: TBD

### Phase 10: Cross-Platform Builds, Packaging, and C++ Deletion
**Goal**: The Rust mailsync binary ships to users on all 5 platforms in production packaging with the C++ engine and all vendored dependencies permanently removed
**Depends on**: Phase 9
**Requirements**: PKGN-01, PKGN-02, PKGN-03, PKGN-04, PKGN-05, PKGN-06
**Success Criteria** (what must be TRUE):
  1. CI produces verified binaries for all 5 targets: win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64 — each binary runs against a live account without errors
  2. The stripped release binary is under 15MB with LTO and strip enabled across all platforms
  3. The production Electron app bundle unpacks the Rust mailsync binary correctly via `asarUnpack` — the binary is found and executes on first launch without a "binary not found" error
  4. `mailsync-process.ts` spawns the Rust binary using the existing path resolution logic with no changes to the IPC protocol or startup handshake
  5. TLS operates exclusively via rustls — `cargo tree | grep openssl` returns nothing, confirming no OpenSSL symbols that could conflict with Electron's BoringSSL
  6. All C++ mailsync source files, vendored dependencies (libetpan, mailcore2), and CMake build configs are deleted from the repository with no remaining references in package.json or build scripts
**Plans**: TBD

Plans:
- [ ] 10-01: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 5 → 6 → 7 → 8 → 9 (can overlap with 7-8) → 10

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 5. Core Infrastructure and IPC Protocol | 0/2 | Not started | - |
| 6. SQLite Layer and Model Infrastructure | 0/? | Not started | - |
| 7. IMAP Background Sync Worker | 0/? | Not started | - |
| 8. Foreground IDLE and Task Execution | 0/? | Not started | - |
| 9. CalDAV, CardDAV, and Metadata Workers | 0/? | Not started | - |
| 10. Cross-Platform Builds, Packaging, and C++ Deletion | 0/? | Not started | - |
