---
phase: 08-foreground-idle-and-task-execution
plan: 05
subsystem: imap
tags: [rust, async-imap, imap-idle, task-execution, imap-session, smtp]

# Dependency graph
requires:
  - phase: 08-04
    provides: ImapTaskOps trait, execute_remote_phase with all 8 handlers, mock-based tests
  - phase: 08-03
    provides: foreground IDLE worker, relay task pattern, into_inner() on ImapSession
provides:
  - ImapSession::from_inner() constructor for round-tripping raw session through IDLE
  - foreground_worker.rs passes real ImapSession/Account/TokenManager to all execute_task calls
  - execute_remote_phase reachable at runtime (not just in mock tests)
  - All 8 task types execute real IMAP flag changes, folder moves, SMTP sends at runtime
affects: [phase-09-caldav, any-future-imap-task-types]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "IDLE round-trip: ImapSession.into_inner() before IDLE, ImapSession::from_inner() after IDLE for task execution"
    - "Reconnect tuple: reconnect() returns (raw_session, capabilities, is_gmail) to preserve ImapSession metadata across reconnects"
    - "Task drain scope: wrap/unwrap ImapSession once per task batch, not per task"

key-files:
  created: []
  modified:
    - app/mailsync-rs/src/imap/session.rs
    - app/mailsync-rs/src/imap/foreground_worker.rs
    - app/mailsync-rs/src/imap/task_executor.rs
    - app/mailsync-rs/src/tasks/mod.rs

key-decisions:
  - "from_inner() constructor added to ImapSession: enables session reconstruction after IDLE.done().await returns raw session; preserves capabilities and is_gmail across the into_inner()/from_inner() round-trip"
  - "connect_session() returns ImapSession not raw: capabilities and is_gmail preserved for from_inner() usage; callers extract raw_session via into_inner() immediately before IDLE entry"
  - "reconnect() returns (raw_session, capabilities, is_gmail) tuple: caller updates mut variables so from_inner() always uses current connection metadata after reconnect"
  - "Single wrap/unwrap scope per task batch: ImapSession constructed once before first task, shared for drain loop, unwrapped once after last task — avoids repeated struct reconstruction"

patterns-established:
  - "IDLE session toggle: into_inner() for IDLE, from_inner(raw, caps, gmail) for execute_task — explicit toggle rather than smart pointer or RefCell"
  - "execute_task always receives Some(session)/Some(account)/Some(token_manager) in production foreground worker — None path reserved for tests only"

requirements-completed: [TASK-02, TASK-03]

# Metrics
duration: 9min
completed: 2026-03-04
---

# Phase 8 Plan 05: Wire Real IMAP Session into Task Execution Summary

**ImapSession::from_inner() closes the IDLE/task session gap — all 8 task remote phases now execute real IMAP and SMTP commands at runtime via the foreground worker**

## Performance

- **Duration:** 9 min
- **Started:** 2026-03-04T15:37:52Z
- **Completed:** 2026-03-04T15:47:00Z
- **Tasks:** 1
- **Files modified:** 4

## Accomplishments

- Added `ImapSession::from_inner()` that reconstructs a full ImapSession from a raw async-imap Session plus preserved `capabilities` and `is_gmail` metadata
- Refactored `connect_session()` in foreground_worker.rs to return `ImapSession` instead of the raw inner session, preserving session metadata for later round-trips
- Replaced all `execute_task(..., None, None, None)` calls with `execute_task(..., Some(&mut imap_session), Some(&account), Some(&token_manager))` — both the initial task path and the drain loop
- Updated `reconnect()` to return `(raw_session, capabilities, is_gmail)` tuple so callers can keep metadata in sync after reconnects
- Removed `#![allow(dead_code)]` from `task_executor.rs` and both `#![allow(dead_code)]` and `#![allow(unused_imports)]` from `tasks/mod.rs`
- Cleaned up unused imports (`DeltaStreamItem`, `task_store`) from `tasks/mod.rs`
- Removed the TODO comment block (lines 270-283) about the IDLE/task session sharing architecture

## Task Commits

1. **Task 1: Add ImapSession::from_inner() and wire real session into foreground worker execute_task calls** - `58d9632` (feat)

## Files Created/Modified

- `app/mailsync-rs/src/imap/session.rs` - Added `from_inner(session, capabilities, is_gmail) -> Self` constructor
- `app/mailsync-rs/src/imap/foreground_worker.rs` - connect_session returns ImapSession; reconnect returns tuple; all execute_task calls use Some(session)/Some(account)/Some(token_manager); IDLE round-trip via into_inner/from_inner
- `app/mailsync-rs/src/imap/task_executor.rs` - Removed module-level `#![allow(dead_code)]`
- `app/mailsync-rs/src/tasks/mod.rs` - Removed `#![allow(dead_code)]`, `#![allow(unused_imports)]`, and two unused imports

## Decisions Made

- `from_inner()` takes `Vec<String>` for capabilities (clone from outer scope) and `bool` for is_gmail — same field types as the ImapSession struct — straightforward reconstruction without needing to re-query the server
- Single wrap/unwrap scope per task batch: ImapSession constructed once before the first task (initial or first drain), shared for all drain tasks, unwrapped once after all tasks complete. This avoids repeated struct creation overhead.
- `reconnect()` return type changed from `Option<async_imap::Session<ImapTlsStream>>` to `Option<(async_imap::Session<ImapTlsStream>, Vec<String>, bool)>` so capabilities/is_gmail stay accurate across reconnects (new server connection may have different capabilities)

## Deviations from Plan

None - plan executed exactly as written. The four surgical changes (from_inner, connect_session return type, execute_task wiring, dead_code removal) all landed as specified.

## Issues Encountered

None. `cargo check` passed on the first attempt after writing the changes, and all 310 existing tests passed without modification.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phase 08 (Foreground IDLE and Task Execution) is functionally complete: all 8 task types execute real IMAP/SMTP operations at runtime
- The 08-VERIFICATION.md verification gaps ("execute_remote_phase unreachable at runtime") are now closed
- Phase 09 (CalDAV sync) can build on SyncbackEventTask remote phase stub — the wiring infrastructure is ready
- No blockers or concerns for Phase 09

---
*Phase: 08-foreground-idle-and-task-execution*
*Completed: 2026-03-04*
