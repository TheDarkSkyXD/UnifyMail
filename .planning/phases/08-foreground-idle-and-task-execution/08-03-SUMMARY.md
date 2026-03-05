---
phase: 08-foreground-idle-and-task-execution
plan: 03
subsystem: imap
tags: [rust, async-imap, idle, imap, tokio, mpsc, task-queue, foreground-worker]

# Dependency graph
requires:
  - phase: 08-01
    provides: execute_task(), parse_task_kind(), TaskKind enum, reset_stuck_tasks(), expire_completed_tasks()
  - phase: 07-imap-background-sync-worker
    provides: ImapSession connect/authenticate, background_sync function, sync.rs structure
provides:
  - foreground_worker.rs with IDLE loop, 25-minute re-IDLE, task interruption via relay pattern
  - Updated sync.rs spawning foreground_worker as Task 4 alongside background_sync
  - Updated stdin_loop.rs routing queue-task to foreground worker via bounded mpsc channel
affects: [08-04, phase-9]

# Tech tracking
tech-stack:
  added: [async-imap IDLE extension (IdleResponse, Handle), stop_token StopSource interrupt pattern]
  patterns:
    - Relay-task pattern for IDLE interruption: spawn task that owns task_rx and StopSource, dropping StopSource triggers ManualInterrupt
    - idle.done() called unconditionally after every IDLE exit to reclaim session
    - Multiple senders on wake_tx: clone before moving into stdin_loop so fg worker also signals bg sync

key-files:
  created:
    - app/mailsync-rs/src/imap/foreground_worker.rs
  modified:
    - app/mailsync-rs/src/imap/mod.rs
    - app/mailsync-rs/src/imap/session.rs
    - app/mailsync-rs/src/modes/sync.rs
    - app/mailsync-rs/src/stdin_loop.rs

key-decisions:
  - "into_inner() added to ImapSession: idle() consumes Session<T>, so foreground worker needs raw inner session; into_inner() provides clean conversion"
  - "fg_wake_tx is a clone of wake_tx before stdin_loop move: mpsc::Sender is cheaply clonable, both senders deliver to same wake_rx owned by background_sync"
  - "Relay task pattern for IDLE interrupt: relay task owns task_rx and StopSource; dropping StopSource triggers ManualInterrupt without cancelling idle_future"
  - "task_tx try_send() used in dispatch_command: matches existing pattern for wake_tx/body_queue_tx; bounded channel capacity 32 makes drops unlikely in practice"

patterns-established:
  - "IDLE relay pattern: spawn relay task owning task_rx + interrupt (StopSource), task arriving drops interrupt triggering ManualInterrupt, relay returns (task_rx, maybe_task) after IDLE exits"
  - "idle.done() unconditional: always called after idle_future completes regardless of result to avoid IMAP session desync"
  - "Foreground/background session separation: foreground worker calls ImapSession::connect() independently from background_sync"

requirements-completed: [IDLE-01, IDLE-02, IDLE-03, TASK-02]

# Metrics
duration: 22min
completed: 2026-03-05
---

# Phase 8 Plan 03: Foreground IDLE Worker Summary

**Foreground IMAP IDLE loop with 25-minute re-IDLE, task interruption via relay pattern, crash recovery, and queue-task routing from stdin through dedicated mpsc channel**

## Performance

- **Duration:** 22 min
- **Started:** 2026-03-05T01:41:39Z
- **Completed:** 2026-03-05T02:03:00Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments
- Foreground IDLE worker (`foreground_worker.rs`) monitors INBOX in real time, re-issues IDLE every 25 minutes to prevent 29-minute server timeout (IDLE-01)
- IDLE interruption via relay-task pattern: relay drops StopSource when task arrives, triggering ManualInterrupt without cancelling idle_future; idle.done() always called to reclaim session
- queue-task stdin commands now route through mpsc channel to foreground worker instead of being stub-logged; tasks are parsed via parse_task_kind() and executed via execute_task()
- sync.rs spawns foreground_worker as Task 4 with its own IMAP session (IDLE-03), separate from background_sync's session

## Task Commits

Each task was committed atomically:

1. **Task 1: Foreground IDLE worker with task interruption** - `81388b7` (feat)
2. **Task 2: Wire foreground worker into sync.rs and stdin_loop.rs** - `b8a69e2` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified
- `app/mailsync-rs/src/imap/foreground_worker.rs` - run_foreground_worker() with IDLE loop, relay pattern, crash recovery, task drain, exponential backoff reconnect
- `app/mailsync-rs/src/imap/mod.rs` - Added `pub mod foreground_worker;`
- `app/mailsync-rs/src/imap/session.rs` - Added `into_inner()` to ImapSession for IDLE handle access
- `app/mailsync-rs/src/modes/sync.rs` - Task channel creation, wake_tx clone, Task 4 spawn, fg_handle abort on shutdown
- `app/mailsync-rs/src/stdin_loop.rs` - task_tx parameter added, QueueTask handler implemented with parse_task_kind() + try_send()

## Decisions Made
- **into_inner() on ImapSession:** async_imap's `idle()` method consumes the `Session<T>`, and `done()` returns it. Since ImapSession wraps the inner session privately, `into_inner()` was added to let the foreground worker take ownership of the raw session for IDLE operations. This is cleaner than restructuring ImapSession around IDLE.
- **fg_wake_tx clone before stdin_loop move:** `mpsc::Sender` is cheaply clonable. Cloning `wake_tx` before passing it to stdin_loop allows the foreground worker to also signal background_sync on new mail, without needing a new channel pair.
- **Relay task pattern for IDLE interrupt:** The relay task owns both `task_rx` and the `StopSource` (interrupt). When a task arrives, dropping the StopSource triggers ManualInterrupt in idle_future. After idle_future completes, the relay is awaited to recover `task_rx` and `maybe_task`. This correctly handles the borrowing constraint of idle_future.
- **task_tx try_send() in QueueTask handler:** Matches the non-blocking pattern used for wake_tx and body_queue_tx throughout stdin_loop. The bounded channel (capacity 32) makes drops very unlikely under normal load; a warning is logged if a task is dropped.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added into_inner() accessor to ImapSession**
- **Found during:** Task 1 (foreground_worker.rs implementation)
- **Issue:** Plan noted that `ImapSession.session` is private and foreground worker needs direct access to call `idle()` (which consumes the session). The plan mentioned adding `inner_mut()` but `idle()` consumes self — `into_inner()` is the correct pattern.
- **Fix:** Added `pub fn into_inner(self) -> Session<ImapTlsStream>` to ImapSession in session.rs
- **Files modified:** app/mailsync-rs/src/imap/session.rs
- **Verification:** cargo check passes, foreground_worker compiles
- **Committed in:** 81388b7 (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (Rule 2 - missing accessor for IDLE session ownership)
**Impact on plan:** The accessor is required for correct IMAP IDLE operation. No scope creep.

## Issues Encountered
- None - plan executed as specified aside from the into_inner() accessor needed for session ownership.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Foreground worker, background_sync, and stdin routing all wired; ready for Plan 04 (remote task execution implementations per TaskKind variant)
- execute_task() stub will be replaced in Plan 04 with real IMAP/SMTP dispatch per task type
- No blockers

---
*Phase: 08-foreground-idle-and-task-execution*
*Completed: 2026-03-05*
