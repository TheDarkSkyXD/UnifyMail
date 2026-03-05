---
phase: 08-foreground-idle-and-task-execution
plan: "01"
subsystem: task-processing
tags: [rust, tasks, imap, tdd, sqlite]
dependency_graph:
  requires:
    - 07-06-PLAN.md  # MailStore, DeltaStream, Task model from Phase 7
  provides:
    - TaskKind enum with serde tag dispatch for all 8 task types
    - execute_task two-phase orchestration function
    - reset_stuck_tasks crash recovery
    - expire_completed_tasks completed task expiry
    - save_task_status and find_local_tasks task store helpers
  affects:
    - app/mailsync-rs/src/tasks/mod.rs
    - app/mailsync-rs/src/tasks/recovery.rs
    - app/mailsync-rs/src/store/task_store.rs
tech_stack:
  added:
    - lettre 0.11 (email SMTP builder, smtp-transport, tokio1, rustls-tls, hostname)
  patterns:
    - TDD (RED then GREEN)
    - serde tagged enum dispatch (__cls discriminant)
    - tokio-rusqlite call() for async SQLite access
    - Two-phase task orchestration (local write + delta, then remote stub)
key_files:
  created:
    - app/mailsync-rs/src/tasks/mod.rs
    - app/mailsync-rs/src/tasks/recovery.rs
    - app/mailsync-rs/src/store/task_store.rs
  modified:
    - app/mailsync-rs/Cargo.toml
    - app/mailsync-rs/src/main.rs
    - app/mailsync-rs/src/store/mod.rs
decisions:
  - "execute_task _delta param uses underscore prefix (stub for Plan 03) — store.save() already emits persist deltas; Plan 03 will add direct delta emissions for task status updates outside save()"
  - "find_local_tasks uses store.writer not reader — Task writes and reads are tightly coupled during task execution; using the writer avoids WAL visibility lag for tasks just written"
  - "TaskKind flatten+extra captures unknown fields — preserves full task blob for DB round-trips without losing C++ fields not typed in Rust variants"
metrics:
  duration_seconds: 666
  completed_date: "2026-03-04"
  tasks_completed: 2
  files_created: 3
  files_modified: 3
  tests_added: 21
  tests_total: 276
---

# Phase 08 Plan 01: Task Processing Infrastructure Summary

**One-liner:** TaskKind serde-tagged enum for 8 C++ task types, two-phase execute_task orchestration, crash recovery reset_stuck_tasks/expire_completed_tasks, and task_store SQL helpers with lettre SMTP dependency added.

## What Was Built

### Task 1: TaskKind enum, execute_task, task_store helpers

**`app/mailsync-rs/src/tasks/mod.rs`**

Defines `TaskKind` — a serde-tagged enum using `#[serde(tag = "__cls")]` to match all 8 C++ task type names exactly:

| Variant | C++ Name | Key Fields |
|---|---|---|
| `SendDraftTask` | SendDraftTask | headerMessageId, draftId |
| `DestroyDraftTask` | DestroyDraftTask | messageId, folderId |
| `ChangeStarredTask` | ChangeStarredTask | starred, threadIds/messageIds |
| `ChangeUnreadTask` | ChangeUnreadTask | unread, threadIds/messageIds |
| `ChangeFolderTask` | ChangeFolderTask | fromFolderId, toFolderId |
| `ChangeLabelsTask` | ChangeLabelsTask | labelsToAdd, labelsToRemove |
| `SyncbackMetadataTask` | SyncbackMetadataTask | modelId, modelClass, pluginId |
| `SyncbackEventTask` | SyncbackEventTask | calendarId, eventId (Phase 9 placeholder) |

Each variant uses `#[serde(flatten)] extra` to capture and preserve unknown C++ fields for DB round-trips.

`execute_task()` two-phase orchestration:
- Phase A (local): task.status = "remote", `store.save()` (emits persist delta)
- Phase B (remote): `execute_remote()` stub returns `Ok(())` — Plan 04 fills in
- Completion: task.status = "complete" (with optional error JSON on failure), `store.save()`

**`app/mailsync-rs/src/store/task_store.rs`**

- `save_task_status()`: UPDATE Task SET status + json_set data.status WHERE id
- `find_local_tasks()`: SELECT WHERE status='local' AND accountId=? ORDER BY rowid ASC

### Task 2: Crash recovery and completed task expiry

**`app/mailsync-rs/src/tasks/recovery.rs`**

- `TASK_RETENTION_SECS = 900` (15 min, matching C++ TaskProcessor.cpp)
- `reset_stuck_tasks()`: UPDATE Task SET status='local' WHERE status='remote'; returns count, logs warning if > 0
- `expire_completed_tasks()`: DELETE WHERE status='complete' AND completed_at < datetime('now', '-N seconds'); uses json_extract, skips rows without completed_at

### Cargo.toml

Added lettre 0.11 with features: builder, smtp-transport, tokio1, tokio1-rustls-tls, hostname.

## Test Results

All 276 unit tests pass + 9 delta coalesce integration tests + 6 IPC contract tests = 291 total tests passing.

New tests added:
- 10 TaskKind deserialization tests (all 8 variants + unknown cls error + edge cases)
- 2 execute_task integration tests
- 3 task_store tests (save_task_status, find_local_tasks ordering, find_local_tasks account filter)
- 7 recovery tests (reset_stuck_tasks 3 tests, expire_completed_tasks 4 tests)

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing] execute_task _delta param prefixed with underscore**
- **Found during:** Task 1 implementation
- **Issue:** The plan signature includes `delta: &DeltaStream` but the stub body doesn't use it; Plan 03 will add direct delta usage
- **Fix:** Prefixed as `_delta` to suppress unused warning while preserving the API contract for Plan 03
- **Files modified:** app/mailsync-rs/src/tasks/mod.rs

**2. [Rule 1 - Bug] Test `execute_task_completes_successfully` used wrong delta channel**
- **Found during:** Task 1 test run (1 test failed initially)
- **Issue:** Test created a separate DeltaStream but checked its channel; store.save() emits to the store's own delta_tx
- **Fix:** Test now shares one channel with both the store and the delta arg, drains initial save before execute_task, asserts >= 2 deltas from execute_task's two save() calls
- **Files modified:** app/mailsync-rs/src/tasks/mod.rs

**3. [Rule 3 - Blocking] lettre hostname feature requires MSYS2 dlltool.exe**
- **Found during:** Task 1 first test run
- **Issue:** `cargo test` failed with "error calling dlltool 'dlltool.exe': program not found" because MSYS2 was not in PATH
- **Fix:** Added `/c/msys64/mingw64/bin` to PATH before running cargo commands (CLAUDE.md documents this requirement); no code changes needed
- **Files modified:** None (environment PATH issue)

## Self-Check: PASSED

- app/mailsync-rs/src/tasks/mod.rs: FOUND
- app/mailsync-rs/src/tasks/recovery.rs: FOUND
- app/mailsync-rs/src/store/task_store.rs: FOUND
- Commit 963b769: FOUND
- All 276 unit tests + 15 integration tests: PASSED
