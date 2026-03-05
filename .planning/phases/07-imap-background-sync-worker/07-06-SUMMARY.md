---
phase: 07-imap-background-sync-worker
plan: 06
subsystem: imap
tags: [rust, imap, tokio, background-sync, body-caching, oauth2, sqlite, mpsc-channels]

# Dependency graph
requires:
  - phase: 07-02
    provides: TokenManager with get_valid_token() and refresh_token_with_retry()
  - phase: 07-04
    provides: BodyQueue priority queue, process_fetched_message()
  - phase: 07-05
    provides: CONDSTORE sync algorithms, FolderSyncState, sort_folders_by_role_priority()
provides:
  - background_sync() function with backoff scheduling and OAuth2 integration
  - should_cache_bodies_in_folder() body age policy (excludes spam/trash)
  - BASE_SYNC_INTERVAL_SECS (60s) and MAX_BACKOFF_ADDITION_SECS (240s) constants
  - stdin_loop updated with wake_tx and body_queue_tx channel parameters
  - dispatch_command() wires WakeWorkers and NeedBodies via try_send
  - sync.rs background_sync_stub replaced with real background_sync spawn
  - DeltaStream.emit_sync_progress() for per-folder progress reporting
  - MailStore.find_messages_needing_bodies() with LEFT JOIN MessageBody SQL
  - MailStore.unlink_messages_in_folder() for UIDVALIDITY reset (RFC 4549)
affects: [phase-08-imap-idle, phase-08-send-draft, any code using stdin_loop signature]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Non-blocking channel dispatch: try_send() for WakeWorkers/NeedBodies — drops on full/closed, logs warning"
    - "Backoff formula: BASE_SYNC_INTERVAL_SECS + min(streak*30, MAX_BACKOFF_ADDITION_SECS)"
    - "Auth error gate: emit connectionError ProcessState then wait on wake_rx or shutdown_rx"
    - "body_queue_rx drain loop: try_recv() loop before each sync cycle to collect priority IDs"

key-files:
  created: []
  modified:
    - app/mailsync-rs/src/imap/sync_worker.rs
    - app/mailsync-rs/src/modes/sync.rs
    - app/mailsync-rs/src/stdin_loop.rs
    - app/mailsync-rs/src/delta/stream.rs
    - app/mailsync-rs/src/store/mail_store.rs

key-decisions:
  - "background_sync stub replaced with full function — signature matches plan exactly (account, store, delta, token_manager, shutdown_rx, wake_rx, body_queue_rx)"
  - "Account has no Clone derive — Arc::new(account) consumes the owned account in sync.rs (no clone needed since ID is already extracted earlier)"
  - "stdin_loop WakeWorkers uses try_send (non-blocking) — drops signal if channel full, which is safe since the worker already has a pending wake"
  - "run_sync_cycle_and_bodies() returns Ok(false) stub — full IMAP folder enumeration deferred to Phase 8 per plan boundary"
  - "emit_sync_progress() emits ProcessState with syncProgress object — matches OnlineStatusStore expected shape"

patterns-established:
  - "mpsc::channel(32) for sync coordination channels (wake, body_queue)"
  - "Non-blocking try_send() for stdin dispatch to avoid blocking the stdin reader task"
  - "Body age policy: spam and trash return false from should_cache_bodies_in_folder()"

requirements-completed: [ISYN-06, ISYN-07]

# Metrics
duration: 6min
completed: 2026-03-04
---

# Phase 7 Plan 06: IMAP Background Sync Wiring Summary

**Background sync loop with 60s/300s backoff, body caching age policy, WakeWorkers/NeedBodies stdin dispatch via mpsc channels, MailStore body query helpers, and background_sync_stub replaced with real implementation**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-03-04T19:53:38Z
- **Completed:** 2026-03-04T19:59:00Z
- **Tasks:** 3 of 3
- **Files modified:** 5

## Accomplishments
- Replaced `background_sync_stub` with full `background_sync()` function including auth error handling, retryable error sleep-30s, fatal error exit(1), and tokio::select! backoff scheduling
- Wired `WakeWorkers` and `NeedBodies` stdin commands to mpsc channel dispatch in `stdin_loop.rs` — non-blocking `try_send()` prevents blocking the stdin reader on a slow sync worker
- Added `find_messages_needing_bodies()` and `unlink_messages_in_folder()` to MailStore, matching C++ SyncWorker.cpp SQL queries exactly
- Added `emit_sync_progress()` to DeltaStream for per-folder progress reporting to OnlineStatusStore
- All 249 existing tests pass; 11 new body age policy and backoff constant tests added

## Task Commits

Each task was committed atomically:

1. **Task 1: Body caching age policy, backoff constants, background_sync skeleton** - `e13997a` (feat)
2. **Task 2: Wire background_sync loop, stdin dispatch channels, replace stub** - `7f749f3` (feat)
3. **Task 3: MailStore body caching helpers** - `8c94d43` (feat)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified
- `app/mailsync-rs/src/imap/sync_worker.rs` - Added `should_cache_bodies_in_folder()`, `BASE_SYNC_INTERVAL_SECS`, `MAX_BACKOFF_ADDITION_SECS`, full `background_sync()` function, `connect_and_authenticate()` helper, `run_sync_cycle_and_bodies()` stub; 11 new tests
- `app/mailsync-rs/src/modes/sync.rs` - Replaced `background_sync_stub` with real `background_sync` spawn; added `wake_tx/body_queue_tx` channels; `Arc<Mutex<TokenManager>>`; removed stub function
- `app/mailsync-rs/src/stdin_loop.rs` - Added `wake_tx: mpsc::Sender<()>` and `body_queue_tx: mpsc::Sender<Vec<String>>` params to `stdin_loop()` and `dispatch_command()`; `WakeWorkers` → `try_send(wake_tx)`; `NeedBodies` → `try_send(body_queue_tx)`
- `app/mailsync-rs/src/delta/stream.rs` - Added `emit_sync_progress(account_id, folder_path, progress)` method
- `app/mailsync-rs/src/store/mail_store.rs` - Added `find_messages_needing_bodies()` (LEFT JOIN MessageBody query) and `unlink_messages_in_folder()` (UIDVALIDITY reset)

## Decisions Made

- **Account no Clone**: Account struct does not derive Clone. Used `Arc::new(account)` consuming ownership (account.id already used before this point), avoiding need to derive Clone on a serde-heavy struct.
- **Non-blocking stdin dispatch**: `try_send()` used instead of `send().await` for WakeWorkers/NeedBodies — blocking the stdin reader task on a full channel would cause command backpressure. Dropped signals are safe since the worker already has a pending wake signal.
- **run_sync_cycle_and_bodies() stub**: Full folder enumeration (run_sync_cycle) is Phase 8 scope per plan boundary. The stub returns Ok(false) to drive the backoff scheduler correctly.
- **emit_sync_progress shape**: ProcessState delta with syncProgress nested object matches the shape TypeScript OnlineStatusStore expects for progress tracking.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - all tasks completed cleanly on first attempt.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- `background_sync()` function signature and shutdown protocol are in place for Phase 8 to add real IMAP folder enumeration via `run_sync_cycle()`
- `BodyQueue` priority queue is wired end-to-end: stdin NeedBodies → body_queue_tx → body_queue_rx → background_sync loop
- `TokenManager` (Arc<Mutex<>>) is shared and ready for Phase 8 IDLE session to use without race conditions
- `find_messages_needing_bodies()` and `unlink_messages_in_folder()` ready for Phase 8 to call from the sync cycle

---
*Phase: 07-imap-background-sync-worker*
*Completed: 2026-03-04*
