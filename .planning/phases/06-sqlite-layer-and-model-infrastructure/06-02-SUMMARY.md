---
phase: 06-sqlite-layer-and-model-infrastructure
plan: 02
subsystem: database
tags: [rust, sqlite, tokio-rusqlite, wal, crud, delta-emission, transactions]

# Dependency graph
requires:
  - phase: 06-01-sqlite-layer-and-model-infrastructure
    provides: MailModel trait with bind_to_statement, to_json, columns_for_query
  - phase: 05-02-core-infrastructure-and-ipc-protocol
    provides: DeltaStream, DeltaStreamItem for delta emission pipeline
provides:
  - MailStore with save/remove/find/find_all/count generic CRUD methods
  - SqlParam enum for type-safe query parameters in tokio-rusqlite closures
  - Writer + reader WAL connection split for concurrent read access
  - MailStoreTransaction with delta accumulation, commit, and rollback
  - begin_transaction() on MailStore activating delta-gated transaction mode
affects:
  - phase-07-imap-sync (will call save/remove for Message, Thread, Folder)
  - phase-08-smtp-send (will call save/remove for Message, Draft)
  - phase-09-caldav-carddav (will call save/remove for Event, Calendar, Contact)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "SqlParam enum for owned ToSql values safe to move into tokio-rusqlite closures"
    - "Dual-connection pattern: writer for all writes, reader for all reads (WAL concurrency)"
    - "INSERT vs UPDATE determined by version: version==1 after increment = INSERT, version>1 = UPDATE"
    - "Delta accumulation in Arc<Mutex<Option<Vec<DeltaStreamItem>>>> shared between MailStore and MailStoreTransaction"
    - "RAII Drop on MailStoreTransaction clears delta accumulator via try_lock if not committed"
    - "prepare_cached() used in all writer.call() closures for repeated-save performance"

key-files:
  created:
    - app/mailsync-rs/src/store/transaction.rs
  modified:
    - app/mailsync-rs/src/store/mail_store.rs
    - app/mailsync-rs/src/store/mod.rs
    - app/mailsync-rs/src/modes/sync.rs

key-decisions:
  - "SqlParam enum (not dyn ToSql refs) — tokio-rusqlite closures must be Send + 'static, reference params cannot be used"
  - "reader field is Option<Connection> so open() (offline mode) can skip reader creation; find/find_all/count fall back to writer if reader is None"
  - "transaction_deltas is Arc<Mutex<...>> shared between MailStore and MailStoreTransaction — MailStore checks it on every save/remove to gate emission"
  - "MailStoreTransaction::commit/rollback take store by &ref rather than embedding Connection — avoids ownership conflict with MailStore's writer"
  - "busy_timeout changed from 10s (Phase 5 value) to 5000ms to match C++ MailStore.cpp behavior per DATA-01 spec"

patterns-established:
  - "All MailStore CRUD methods generic over T: MailModel — single implementation covers all 13 model types"
  - "Delta emission always goes through emit_or_accumulate() — centralizes transaction-awareness"
  - "SQL closures in writer.call/reader.call never capture &self — only owned or cloned values"

requirements-completed:
  - DATA-01
  - DATA-02
  - DATA-05

# Metrics
duration: 15min
completed: 2026-03-04
---

# Phase 06 Plan 02: MailStore CRUD and MailStoreTransaction Summary

**Generic save/remove/find/find_all/count CRUD over MailModel trait with WAL dual-connection and transaction-gated delta emission**

## Performance

- **Duration:** 15 min
- **Started:** 2026-03-04T15:52:23Z
- **Completed:** 2026-03-04T16:07:10Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments
- MailStore extended with five generic CRUD methods (save, remove, find, find_all, count) working for any T: MailModel type
- WAL dual-connection pattern: writer for all writes, reader for all reads, preventing SQLITE_BUSY under concurrent access
- MailStoreTransaction with delta accumulation: saves within a transaction buffer deltas; commit emits all at once; rollback discards all
- sync.rs updated to use open_with_delta() so the production sync mode has reader connection and delta emission from the start
- 21 store tests + 7 transaction tests all passing; full 137-test suite green; clippy -D warnings clean

## Task Commits

Each task was committed atomically:

1. **Task 1: MailStore CRUD, reader connection, and delta emission wiring** - `f0956e5` (feat)
2. **Task 2: MailStoreTransaction with delta accumulation and commit/rollback** - `4704d4d` (feat)

## Files Created/Modified
- `app/mailsync-rs/src/store/mail_store.rs` - Extended with SqlParam, open_with_delta(), save, remove, find, find_all, count, begin_transaction, emit_or_accumulate, execute_commit, execute_rollback; 14 tests
- `app/mailsync-rs/src/store/transaction.rs` - New: MailStoreTransaction with commit, rollback, RAII Drop; 7 tests
- `app/mailsync-rs/src/store/mod.rs` - Added transaction module, re-exports MailStoreTransaction and SqlParam; #[allow(dead_code)] for forward-declared public API
- `app/mailsync-rs/src/modes/sync.rs` - Updated to open_with_delta() with DeltaStream Arc, removing redundant channel creation

## Decisions Made
- SqlParam enum for owned query params: tokio-rusqlite closures need `Send + 'static` so `&dyn ToSql` references cannot be captured. SqlParam is Clone+Debug with ToSql impl, safe to move.
- reader is `Option<Connection>` not mandatory: offline modes (migrate, reset) need only one connection; find/find_all/count fall back to writer when reader is None. Only open_with_delta() creates reader.
- transaction_deltas shared via Arc<Mutex>: MailStoreTransaction needs to signal MailStore to stop direct emission, and to retrieve the accumulated deltas on commit. Sharing the same Arc avoids a separate callback channel.
- commit/rollback take `&MailStore` not embedding Connection: MailStoreTransaction cannot own the connection (MailStore needs it between begin/commit), so store methods execute_commit/execute_rollback are pub(crate) for transaction to call.
- busy_timeout 5000ms: DATA-01 specifies matching C++ behavior. Phase 5 used 10s as a conservative default; corrected here per plan spec.

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None - all methods implemented cleanly following the plan's design. The SqlParam approach described in the plan worked exactly as specified.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- MailStore CRUD backbone complete — Phase 7 (IMAP sync) can call store.save(&mut message) and store.find() immediately
- MailStoreTransaction available for Phase 7 batch sync operations (save multiple messages in one transaction)
- All Phase 6 plan 01 tests still passing (113 unit tests + 9 integration tests + 6 IPC contract tests)

---
*Phase: 06-sqlite-layer-and-model-infrastructure*
*Completed: 2026-03-04*
