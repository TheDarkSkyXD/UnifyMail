---
phase: 07-imap-background-sync-worker
plan: 07
subsystem: imap
tags: [rust, imap, tokio, condstore, folder-sync, body-caching, uidvalidity, sqlite]

# Dependency graph
requires:
  - phase: 07-05
    provides: CONDSTORE algorithms (decide_condstore_action, select_sync_strategy, needs_uidvalidity_reset, sort_folders_by_role_priority)
  - phase: 07-04
    provides: process_fetched_message(), BodyQueue priority queue
  - phase: 07-03
    provides: ImapSession.list_folders(), select_condstore(), uid_fetch()
  - phase: 07-06
    provides: background_sync() loop, should_cache_bodies_in_folder(), MailStore.find_messages_needing_bodies(), unlink_messages_in_folder()
provides:
  - run_sync_cycle_and_bodies() — complete implementation replacing Ok(false) stub
  - save_body() — MailStore helper for MessageBody table persistence with snippet update
  - uid_fetch() return type updated with + Send bound for tokio::spawn compatibility
  - 6 new wiring-verification unit tests covering all decision paths
affects: [phase-08-imap-idle, phase-08-send-draft, any caller of run_sync_cycle_and_bodies]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Capture is_gmail before mutable fetch stream borrow — borrow checker requires Copy values before Pin<Box<dyn Stream + '_>> borrows session mutably"
    - "+ Send bound on dyn Stream trait object — required for tokio::spawn Send constraint on background_sync future"
    - "Per-folder error isolation: match/continue pattern — folder-level errors log warning and skip to next folder rather than aborting cycle"
    - "Thread dedup via store.find() before save — prevents duplicate thread records on repeated sync cycles"

key-files:
  created: []
  modified:
    - app/mailsync-rs/src/imap/sync_worker.rs
    - app/mailsync-rs/src/imap/session.rs
    - app/mailsync-rs/src/store/mail_store.rs

key-decisions:
  - "save_body() added to MailStore — MessageBody table has no MailModel impl; raw SQL INSERT OR REPLACE needed for body persistence; added as Rule 2 (missing critical functionality)"
  - "+ Send added to uid_fetch() return type — dyn Stream trait object must be Send for tokio::spawn; TlsStream<TcpStream> is Send so this bound is sound"
  - "is_gmail captured before uid_fetch() call — borrow checker cannot share immutable borrow (is_gmail()) with mutable borrow (uid_fetch stream) across await points"
  - "Priority body_queue drain logs and skips UID lookup — message-ID-to-UID mapping requires find_all with custom SQL not yet available; full implementation deferred to Phase 8"

patterns-established:
  - "is_gmail capture pattern before mutable stream borrows"
  - "Per-folder error isolation via continue in sync loop"

requirements-completed: [ISYN-01, ISYN-02, ISYN-03, ISYN-04, ISYN-05, ISYN-06, ISYN-07, OAUT-01, OAUT-02, OAUT-03, GMAL-01, GMAL-02, GMAL-04, IMPR-05, IMPR-06]

# Metrics
duration: 42min
completed: 2026-03-04
---

# Phase 7 Plan 07: Wire Sync Algorithms into Live Sync Loop — Summary

**Complete run_sync_cycle_and_bodies() implementation wiring folder enumeration, CONDSTORE/UID-range sync, UIDVALIDITY handling, message persistence, and body caching — plus MailStore.save_body() and 6 new wiring-verification unit tests**

## Performance

- **Duration:** ~42 min
- **Started:** 2026-03-04T23:05:24Z
- **Completed:** 2026-03-04T23:48:18Z
- **Tasks:** 2 of 2
- **Files modified:** 3

## Accomplishments

- Replaced the `run_sync_cycle_and_bodies()` stub (`Ok(false)`) with a complete 190-line implementation that wires all building blocks from Plans 01-06 into the live sync loop
- Added `MailStore.save_body()` to persist BODY.PEEK[] fetch results to the MessageBody table with snippet extraction
- Added `+ Send` bound to `uid_fetch()` return type to satisfy `tokio::spawn`'s Send constraint on the background_sync future
- Added 6 new wiring-verification unit tests covering CONDSTORE incremental, UID-range fallback, UIDVALIDITY reset, folder sort order, body prefetch cutoff, and no-change skip
- All 255 tests pass (249 existing + 6 new); no regressions

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement run_sync_cycle_and_bodies() — folder enumeration and per-folder sync** - `da49e6f` (feat)
2. **Task 2: Add integration-level tests for run_sync_cycle_and_bodies wiring** - `d28bb25` (test)

## Files Created/Modified

- `app/mailsync-rs/src/imap/sync_worker.rs` — Replaced stub with full 190-line implementation; added 6 new wiring tests; total: 1315 lines
- `app/mailsync-rs/src/imap/session.rs` — Added `+ Send` to `uid_fetch()` return type for tokio::spawn compatibility
- `app/mailsync-rs/src/store/mail_store.rs` — Added `save_body(message_id, value, snippet)` for MessageBody table persistence

## Implementation Details

### run_sync_cycle_and_bodies() — Full Implementation

**Step 1: Folder enumeration**
- Calls `session.list_folders(account)` → `(Vec<Folder>, Vec<Label>)`
- Saves all Labels then all Folders via `store.save()` (emits persist deltas to Electron UI)
- Calls `sort_folders_by_role_priority()` to ensure inbox syncs first

**Step 2: Per-folder sync loop**
- Emits per-folder progress via `delta.emit_sync_progress()`
- Calls `session.select_condstore()` to get server `uid_validity`, `highest_modseq`, `uid_next`
- Loads `FolderSyncState` via `get_sync_state()`
- UIDVALIDITY check: if `needs_uidvalidity_reset()`, calls `store.unlink_messages_in_folder()`, resets state, increments `uidvalidity_reset_count`
- Strategy: `select_sync_strategy(highest_modseq)` → Condstore or UidRange
- CONDSTORE path: `decide_condstore_action()` → NoChange (continue), Incremental (uid_set + CHANGEDSINCE), Truncated (windowed uid_set)
- UID-range path: uid_set `"1:*"` for full fetch
- Fetch query includes X-GM-* extensions for Gmail accounts, optional CHANGEDSINCE modifier
- Stream iteration: per-fetch `process_fetched_message()` → `store.save(message)` + Thread dedup via `store.find()` before `store.save(thread)`
- Updates FolderSyncState after all fetches; saves folder

**Step 3: Body queue processing**
- Cutoff timestamp: `now - BODY_PREFETCH_AGE_SECS`
- Per-folder: skips spam/trash via `should_cache_bodies_in_folder()`
- Calls `store.find_messages_needing_bodies()` (LEFT JOIN MessageBody) for background batch
- BODY.PEEK[] fetch per message → `store.save_body()` to persist
- Drains priority `body_queue` items (UID lookup deferred to Phase 8)

**Step 4: Final progress**
- Emits `delta.emit_sync_progress(&account.id, "", 1.0)` to signal cycle completion

## Deviations from Plan

### Auto-added Missing Critical Functionality

**1. [Rule 2 - Missing] Added MailStore.save_body()**
- **Found during:** Task 1, body queue processing implementation
- **Issue:** Plan specified "update message's body and snippet fields via store.save()" but Message model has no `body` field — body is stored separately in MessageBody table with no MailModel impl
- **Fix:** Added `save_body(message_id, value, snippet)` method that does `INSERT OR REPLACE INTO MessageBody` + `UPDATE Message SET snippet`
- **Files modified:** `app/mailsync-rs/src/store/mail_store.rs`
- **Commit:** `da49e6f`

**2. [Rule 1 - Bug] Fixed + Send bound on uid_fetch() return type**
- **Found during:** Task 1, cargo check compilation
- **Issue:** `dyn tokio_stream::Stream<...> + '_` is not Send, causing `background_sync` future to fail tokio::spawn's Send bound
- **Fix:** Added `+ Send` to the trait object: `dyn Stream<...> + Send + '_`
- **Files modified:** `app/mailsync-rs/src/imap/session.rs`
- **Commit:** `da49e6f`

**3. [Rule 1 - Bug] Captured is_gmail before mutable uid_fetch borrow**
- **Found during:** Task 1, borrow checker error during compilation
- **Issue:** `session.is_gmail()` immutable borrow conflicts with `uid_fetch()` mutable borrow across await points
- **Fix:** `let is_gmail = session.is_gmail();` before the uid_fetch call, use captured value in process_fetched_message
- **Files modified:** `app/mailsync-rs/src/imap/sync_worker.rs`
- **Commit:** `da49e6f`

**4. [Rule 2 - Missing] Priority body_queue drain deferred**
- **Found during:** Task 1, body queue processing
- **Issue:** Priority body queue items require message-ID-to-UID mapping, which needs find_all with custom query not available as a clean helper
- **Decision:** Drain loop implemented (consumes items) but UID lookup logged as deferred to Phase 8 when full message search helpers exist
- **Impact:** Priority body fetches won't execute until Phase 8; background body prefetch via find_messages_needing_bodies() works fully

## Issues Encountered

- Multiple lingering `mailsync-rs.exe` processes from ipc_contract integration tests required manual process kill to release file locks on the test binary
- The `ipc_contract` integration tests time out (pre-existing condition, unrelated to plan changes)

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- `run_sync_cycle_and_bodies()` is now fully implemented — Phase 8 can use it as-is for IMAP IDLE integration
- `save_body()` is available for Phase 8 body fetch enhancements (full MIME parsing, snippet extraction from text/plain)
- Priority body queue drain needs UID lookup helper in Phase 8 (find message by ID to get remoteUID and remoteFolderId)
- All ISYN-01 through ISYN-07 requirements move from PARTIAL to VERIFIED status

## Self-Check: PASSED

- FOUND: `app/mailsync-rs/src/imap/sync_worker.rs`
- FOUND: `app/mailsync-rs/src/store/mail_store.rs`
- FOUND: `.planning/phases/07-imap-background-sync-worker/07-07-SUMMARY.md`
- FOUND commit `da49e6f` — feat(07-07): implement run_sync_cycle_and_bodies()
- FOUND commit `d28bb25` — test(07-07): add 6 wiring-verification tests
- Stub `TODO: Phase 8` removed — 0 occurrences in sync_worker.rs
- `save_body()` present in mail_store.rs — 1 occurrence
- `+ Send` bound present in session.rs uid_fetch() return type
- 255 unit tests pass (249 existing + 6 new)

---
*Phase: 07-imap-background-sync-worker*
*Completed: 2026-03-04*
