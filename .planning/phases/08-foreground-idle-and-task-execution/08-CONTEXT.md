# Phase 8: Foreground IDLE and Task Execution - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

IMAP IDLE monitoring on the primary folder for real-time new mail detection, task processor executing all email-related task types (SendDraft, DestroyDraft, ChangeLabels, ChangeFolder, ChangeStarred, ChangeUnread, SyncbackMetadata) with local+remote phases, SMTP send via lettre with TLS/STARTTLS/OAuth2 support and multipart MIME construction, and crash recovery for tasks interrupted mid-execution. CalDAV/CardDAV task types (SyncbackEvent, EventRsvp, contact tasks) belong to Phase 9.

</domain>

<decisions>
## Implementation Decisions

### Claude's Discretion

All implementation decisions for Phase 8 are delegated to Claude's discretion, guided by C++ behavior patterns and best practices. The user trusts Claude to make the right calls based on the existing C++ codebase (`TaskProcessor.cpp`, `SyncWorker.hpp`, `main.cpp`) and the research document (`08-RESEARCH.md`).

**IDLE behavior:**
- Which folder(s) to IDLE on (Inbox-only vs re-evaluate after task interruption)
- IDLE reconnection strategy on network failure (auto-reconnect with backoff vs relying on background sync)
- Whether IDLE uses a dedicated IMAP session or shares with background sync
- What happens when IDLE detects new mail (quick sync on IDLE session vs signal background worker)
- 25-minute re-IDLE cadence to avoid 29-minute server timeout

**Task execution model:**
- Task concurrency (sequential vs parallel remote phases)
- IMAP session for task operations (reuse IDLE session vs separate connection)
- Which task types to include in Phase 8 vs defer to Phase 9 (core email tasks recommended; CalDAV/CardDAV deferred)
- Optimistic UI via local phase DB writes + immediate delta emission
- Task queue ordering and priority

**SMTP send strategy:**
- Connection lifecycle (fresh per send vs pooling)
- MIME multipart construction including inline images (CID references) — STATE.md research flag applies: validate lettre's CID support during research
- TLS preference (fixed order vs per-account from provider config)
- Sent folder handling after send (IMAP APPEND always vs provider-aware for Gmail)

**Crash recovery:**
- Stuck remote task handling on startup (reset to local and re-queue vs verify server state)
- Completed task expiration window (C++ uses ~15 minutes)
- SendDraft crash recovery (mark failed for user review vs auto-retry)
- Task failure error surfacing (task status field vs dedicated error delta)

</decisions>

<specifics>
## Specific Ideas

- The C++ `main.cpp` lines 659-679 show the queue-task → performLocal → 300ms delay → idleInterrupt pattern — this is the reference implementation for task-IDLE interaction
- C++ `TaskProcessor.cpp` has per-task-type `performRemote*` methods — Rust equivalent needed
- C++ `SyncWorker.hpp` exposes `idleInterrupt()`, `idleQueueBodiesToSync()`, `idleCycleIteration()` — the IDLE API contract
- async-imap 0.11 has IDLE capability via `Session::idle()` returning a `Handle` with `wait_with_timeout()`
- STATE.md blocker: "Validate lettre multipart MIME builder API coverage for inline images (CID references) and text/html + text/plain alternatives before Phase 8 coding begins"
- Phase 7 deferred priority body_queue drain to Phase 8 — message-ID-to-UID mapping requires find_all helper

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailsync-rs/src/stdin_loop.rs`: QueueTask command already parsed (stub at line 80-82) — wire to real task processor
- `app/mailsync-rs/src/models/task_model.rs`: Task struct with status (local/remote/complete/cancelled) and __cls preservation
- `app/mailsync-rs/src/imap/session.rs`: ImapSession with connect/auth/role detection — extend with IDLE and task IMAP operations
- `app/mailsync-rs/src/imap/sync_worker.rs`: Background sync worker — IDLE runs alongside this
- `app/mailsync-rs/src/oauth2.rs`: TokenManager handles expiry check and HTTP refresh — reuse for SMTP OAuth2
- `app/mailsync-rs/src/delta/stream.rs`: DeltaStream with emit() and emit_process_state() — task status deltas flow through this
- `app/mailsync-rs/src/store/mail_store.rs`: MailStore CRUD (save/remove/find/find_all) — task processor uses for local phase DB writes
- `app/mailsync-rs/src/error.rs`: SyncError enum with all C++ error keys — reuse for task and SMTP errors

### Established Patterns
- Fat-row pattern: `data` JSON blob + indexed projection columns for all models (Phase 6)
- Delta emission: DeltaStream → mpsc → 500ms coalescing → stdout flush task (Phase 5)
- tokio-rusqlite `call()` for all DB access (Phase 6)
- ProcessState + ProcessAccountSecretsUpdated bypass coalescing (Phase 6)
- Structured logging via tracing crate (Phase 5)
- CONDSTORE incremental sync + UID-range fallback (Phase 7) — IDLE triggers re-sync

### Integration Points
- `app/mailsync-rs/src/modes/sync.rs`: Entry point — IDLE tokio task spawned here alongside background sync
- `app/mailsync-rs/src/stdin_loop.rs`: `dispatch_command()` routes QueueTask to task processor, WakeWorkers to sync worker
- `app/frontend/flux/mailsync-bridge.ts`: Sends `queue-task` JSON via stdin; observes task status deltas
- `app/frontend/flux/stores/task-queue.ts`: Observes Task model changes for UI updates (queue, completed, waitForPerformLocal/Remote)
- `app/frontend/flux/tasks/*.ts`: 13+ TypeScript task types — reference for field names and expected behavior
- `app/mailsync/MailSync/TaskProcessor.cpp`: C++ reference for all performLocal/performRemote implementations
- `app/mailsync/MailSync/SyncWorker.hpp`: C++ IDLE interface (idleInterrupt, idleCycleIteration)

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 08-foreground-idle-and-task-execution*
*Context gathered: 2026-03-04*
