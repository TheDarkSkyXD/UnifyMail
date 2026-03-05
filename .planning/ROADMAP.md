# Roadmap: UnifyMail

## Milestones

- **v1.0 MVP** — Phases 1-4.2 (shipped 2026-03-04)
- **v2.0 Rewrite mailsync Engine in Rust** — Phases 5-10 (planned)

## Phases

<details>
<summary>v1.0 Rewrite mailcore N-API in Rust (Phases 1-4.2) — SHIPPED 2026-03-04</summary>

- [x] Phase 1: Scaffolding and Provider Detection (2/2 plans) — completed 2026-03-03
- [x] Phase 2: IMAP Connection Testing (2/2 plans) — completed 2026-03-03
- [x] Phase 3: SMTP Testing and Account Validation (2/2 plans) — completed 2026-03-04
- [x] Phase 4: Cross-Platform Packaging and Cleanup (2/2 plans) — completed 2026-03-04
- [x] Phase 4.1: CI Hardening and Smoke Test Expansion (1/1 plan) — completed 2026-03-04
- [x] Phase 4.2: validateAccount Integration Verification (1/1 plan) — completed 2026-03-04

See: `.planning/milestones/v1.0-ROADMAP.md` for full details.

</details>

### v2.0 Rewrite mailsync Engine in Rust (In Progress)

Replace the `app/mailsync/` C++ sync engine (~16,200 LOC, 50 source files) with a standalone Rust binary that maintains exact wire-format compatibility with the existing TypeScript `MailsyncBridge` in Electron. The Rust binary is a drop-in replacement — same stdin/stdout newline-delimited JSON protocol, same SQLite schema, same process modes.

**Depends on:** v1.0 completion (Phases 1-4)

- [x] **Phase 5: Core Infrastructure and IPC Protocol** - Rust binary skeleton with correct stdin/stdout protocol, all process modes, SQLite schema creation, and delta emission pipeline (completed 2026-03-04)
- [ ] **Phase 6: SQLite Layer and Model Infrastructure** - Complete MailStore with all data models, WAL mode, single-writer pattern, and schema migrations matching the C++ baseline
- [x] **Phase 7: IMAP Background Sync Worker** - Full IMAP sync against live accounts: folder enumeration, CONDSTORE/UID-range incremental sync, body caching, OAuth2, and Gmail-specific behaviors (completed 2026-03-04)
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
**Plans**: 3 plans

Plans:
- [ ] 06-01-PLAN.md — MailModel trait and all 13 data model structs with serde rename fidelity and JSON round-trip tests
- [ ] 06-02-PLAN.md — MailStore CRUD (save/remove/find/find_all/count), WAL reader connection, transaction support with delta accumulation
- [ ] 06-03-PLAN.md — Lifecycle hooks (metadata, FTS5, join tables, ThreadCounts), end-to-end pipeline tests, schema validation

### Phase 7: IMAP Background Sync Worker
**Goal**: The background sync worker syncs a real email account end-to-end — folders enumerate, messages appear in the Electron UI via delta emission, OAuth2 tokens refresh automatically, and Gmail-specific behaviors produce correct results
**Depends on**: Phase 6
**Requirements**: ISYN-01, ISYN-02, ISYN-03, ISYN-04, ISYN-05, ISYN-06, ISYN-07, OAUT-01, OAUT-02, OAUT-03, GMAL-01, GMAL-02, GMAL-04, IMPR-05, IMPR-06
**Success Criteria** (what must be TRUE):
  1. After connecting a new IMAP account, all folders enumerate with correct role assignments (Inbox, Sent, Drafts, Trash, Spam, Archive) and appear in the Electron sidebar
  2. New messages arriving in an already-synced folder are detected on the next background sync cycle (CONDSTORE modseq-based) without fetching already-known messages again; servers without CONDSTORE fall back to UID range sync
  3. When UIDVALIDITY changes on a folder, the worker discards all cached UIDs and performs a full re-sync of that folder per RFC 4549
  4. Message body requests sent via `need-bodies` stdin command are fetched in priority order and cached; messages older than the per-folder age policy (3 months) are not pre-fetched
  5. Gmail accounts show only INBOX, All Mail, Trash, and Spam folders; X-GM-LABELS, X-GM-MSGID, and X-GM-THRID extension data are parsed and stored on message records
  6. OAuth2 tokens are checked for expiry within a 5-minute buffer before every IMAP authenticate; expired tokens refresh automatically and the updated credentials emit a `ProcessAccountSecretsUpdated` delta to the UI
  7. All IMAP network operations complete within their per-operation timeout or resolve with a structured error classifying the failure as auth, TLS, network, or server error
**Plans**: 7 plans

Plans:
- [ ] 07-01-PLAN.md — Cargo deps, imap/oauth2 module scaffold, SyncError classification methods
- [ ] 07-02-PLAN.md — OAuth2 TokenManager: expiry check, HTTP refresh, XOAUTH2 SASL, secrets delta
- [ ] 07-03-PLAN.md — ImapSession connect/auth, folder role detection (RFC 6154 + name fallback), Gmail whitelist
- [ ] 07-04-PLAN.md — Stable message ID (SHA-256+Base58), Fetch-to-Message conversion, Gmail extensions, threading, BodyQueue
- [ ] 07-05-PLAN.md — CONDSTORE incremental sync, UID-range fallback, UIDVALIDITY reset, folder priority, timeouts
- [ ] 07-06-PLAN.md — Body caching with age policy, sync loop with backoff/wake, stdin dispatch wiring, stub replacement
- [ ] 07-07-PLAN.md — Gap closure: wire sync algorithms into run_sync_cycle_and_bodies() live loop

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
**Plans**: 4 plans

Plans:
- [ ] 08-01-PLAN.md — Task infrastructure: TaskKind enum, two-phase execute_task, crash recovery, completed task expiry, lettre dependency
- [ ] 08-02-PLAN.md — SMTP sender with TLS/STARTTLS/clear + password/XOAUTH2 auth, and MIME multipart builder from draft JSON
- [ ] 08-03-PLAN.md — Foreground IDLE worker with 25-min re-IDLE, task interruption via mpsc relay, wiring into sync.rs and stdin_loop
- [ ] 08-04-PLAN.md — All 8 task type remote-phase handlers (IMAP flag/folder/label commands + SMTP send) and body sync progress emission

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
Phases execute in numeric order: 5 -> 6 -> 7 -> 8 -> 9 (can overlap with 7-8) -> 10

| Phase | Milestone | Plans Complete | Status | Completed |
|-------|-----------|----------------|--------|-----------|
| 1. Scaffolding and Provider Detection | v1.0 | 2/2 | Complete | 2026-03-03 |
| 2. IMAP Connection Testing | v1.0 | 2/2 | Complete | 2026-03-03 |
| 3. SMTP Testing and Account Validation | v1.0 | 2/2 | Complete | 2026-03-04 |
| 4. Cross-Platform Packaging and Cleanup | v1.0 | 2/2 | Complete | 2026-03-04 |
| 4.1 CI Hardening and Smoke Test Expansion | v1.0 | 1/1 | Complete | 2026-03-04 |
| 4.2 validateAccount Integration Verification | v1.0 | 1/1 | Complete | 2026-03-04 |
| 5. Core Infrastructure and IPC Protocol | 2/2 | Complete   | 2026-03-04 | - |
| 6. SQLite Layer and Model Infrastructure | 2/3 | In Progress|  | - |
| 7. IMAP Background Sync Worker | 7/7 | Complete   | 2026-03-05 | - |
| 8. Foreground IDLE and Task Execution | 2/4 | In Progress|  | - |
| 9. CalDAV, CardDAV, and Metadata Workers | v2.0 | 0/? | Not started | - |
| 10. Cross-Platform Builds, Packaging, and C++ Deletion | v2.0 | 0/? | Not started | - |
