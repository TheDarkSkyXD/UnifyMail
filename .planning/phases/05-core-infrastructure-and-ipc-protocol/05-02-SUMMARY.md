---
phase: 05-core-infrastructure-and-ipc-protocol
plan: 02
subsystem: infra
tags: [rust, tokio, ipc, delta, mpsc, stdin-loop, sync-mode, serde, indexmap]

# Dependency graph
requires:
  - phase: 05-01
    provides: "MailStore, SyncError, Account, Identity, Args, Cargo workspace, SQLite schema"
provides:
  - "DeltaStreamItem struct with serde renames for exact IPC wire format (type, modelClass, modelJSONs)"
  - "process_state() constructor producing ProcessState delta for OnlineStatusStore"
  - "Delta coalescing: concatenate() + upsert_model_json() implementing C++ DeltaStream key-merge algorithm"
  - "DeltaStream Arc-shared sender wrapper with emit() and emit_process_state() helpers"
  - "delta_flush_task: dedicated tokio task, exclusive stdout ownership, 500ms coalescing window, explicit flush()"
  - "stdin_loop: shared Lines iterator, 10 StdinCommand variants, EOF->shutdown signal, unknown-command warn-and-continue"
  - "modes/sync.rs: 3-task tokio skeleton (stdin_loop, delta_flush_task, background_sync_stub), graceful shutdown"
  - "Single shared BufReader/Lines for entire process stdin lifecycle (no data loss from multiple readers)"
  - "IPC contract test suite: 6 tests covering handshake, ProcessState field names, EOF exit 141, flush timing, deadlock, unknown commands"
  - "Delta coalescing test suite: 9 tests covering field names, key-merge, type isolation, class isolation"
  - "scripts/start-dev.js: cargo build -p unifymail-sync step before Electron launch"
affects:
  - "06+ (IMAP/SMTP phases): sync mode skeleton, delta pipeline, and stdin command dispatch all ready"
  - "07 (IMAP Engine): background_sync_stub placeholder is where IMAP workers plug in"
  - "Phase 10 (rename): binary structure complete, only renaming/integration work remains"

# Tech tracking
tech-stack:
  added:
    - "tokio::sync::broadcast — shutdown coordination across multiple tokio tasks"
    - "tokio::sync::mpsc::unbounded_channel — delta emission pipeline (DeltaStream -> flush_task)"
    - "indexmap::IndexMap — ordered coalesce buffer for deterministic delta output"
  patterns:
    - "Single shared BufReader/Lines for stdin: created once in main.rs, passed to sync::run(), then to stdin_loop — prevents data loss from multiple BufReader instances on the same OS pipe"
    - "Graceful shutdown: stdin_loop returns after EOF -> drop DeltaStream closes channel -> await flush_handle ensures all deltas flushed -> process::exit(141)"
    - "Delta flush uses serde_json::json! macro with string literal keys as second-layer defense beyond serde renames"
    - "process::exit(141) called from sync mode (not stdin_loop) to guarantee flush task completion first"
    - "All command handlers at Phase 5: stub with debug-level log, no response — per 05-CONTEXT.md 'No response for stubs'"

key-files:
  created:
    - "app/mailsync-rs/src/delta/mod.rs — delta module re-exports"
    - "app/mailsync-rs/src/delta/item.rs — DeltaStreamItem with serde renames, coalescing logic"
    - "app/mailsync-rs/src/delta/stream.rs — DeltaStream mpsc sender wrapper"
    - "app/mailsync-rs/src/delta/flush.rs — delta_flush_task with 500ms buffer and explicit stdout flush"
    - "app/mailsync-rs/src/stdin_loop.rs — stdin_loop with 10 StdinCommand variants, shared Lines iterator"
    - "app/mailsync-rs/src/modes/sync.rs — sync mode with 3-task tokio skeleton"
    - "app/mailsync-rs/tests/delta_coalesce.rs — 9 tests for coalescing algorithm"
    - "app/mailsync-rs/tests/ipc_contract.rs — 6 IPC contract tests (handshake, delta names, EOF, flush, deadlock, unknown)"
  modified:
    - "app/mailsync-rs/src/main.rs — single shared BufReader/Lines, mod delta + stdin_loop, sync mode wired"
    - "app/mailsync-rs/src/modes/mod.rs — added pub mod sync"
    - "scripts/start-dev.js — added cargo build -p unifymail-sync step before Electron launch"

key-decisions:
  - "Single shared BufReader/Lines for stdin lifecycle: multiple BufReader instances on tokio::io::stdin() cause data loss — first BufReader buffers ahead from OS pipe; when dropped, buffered data is lost to subsequent readers. Solution: create ONE Lines iterator in main.rs, pass through handshake functions, then into stdin_loop."
  - "process::exit(141) called from sync::run() after awaiting flush_handle, NOT from stdin_loop — ensures ProcessState delta (and any pending deltas) are flushed before process terminates"
  - "scripts/start-dev.js mailsync-rs build failure is non-fatal — C++ mailsync binary stays functional through Phase 9 as fallback; Rust binary is additive during Phases 5-9"
  - "background_sync_stub awaits broadcast shutdown receiver — placeholder for Phase 7 IMAP workers"
  - "delta_flush_task flushes on channel close (None arm) in addition to 500ms tick — ensures ProcessState reaches pipe before process::exit(141) during normal shutdown"

patterns-established:
  - "Tokio task architecture: 3 independent tasks per account (stdin_loop, delta_flush_task, background_sync_stub) — no shared mutable state between tasks"
  - "Delta pipeline: emit() -> unbounded_channel -> coalesce_into(buffer) -> flush_buffer() -> stdout.lock() + writeln! + flush()"
  - "IPC contract test pattern: setup_migrated_tempdir() + spawn binary with piped stdin/stdout + writer thread + wait_with_output"
  - "Shared stdin reader pattern: Lines<BufReader<tokio::io::Stdin>> created once, passed by value through the call chain"

requirements-completed:
  - IPC-01
  - IPC-02
  - IPC-03
  - IPC-05
  - IPC-06
  - IMPR-08

# Metrics
duration: 24min
completed: 2026-03-04
---

# Phase 5 Plan 02: Delta Emission Pipeline, Stdin Loop, and Sync Mode Skeleton Summary

**Delta pipeline with 500ms coalescing, stdin handshake loop with 10 command types, 3-task tokio sync skeleton emitting ProcessState to Electron, and IPC contract tests verifying wire format fidelity**

## Performance

- **Duration:** ~24 min
- **Started:** 2026-03-04T14:06:18Z
- **Completed:** 2026-03-04T14:30:00Z
- **Tasks:** 2 completed
- **Files modified:** 11 created/modified

## Accomplishments

- Sync mode completes two-line stdin handshake, emits ProcessState delta with exact C++ field names (type/modelClass/modelJSONs), and exits 141 on stdin EOF — wire-format compatible with TypeScript mailsync-process.ts
- 44 total tests pass: 20 unit tests, 9 delta coalescing algorithm tests, 6 IPC contract tests (including deadlock and large-stdin stress test), 9 offline mode tests from Plan 01
- Delta coalescing algorithm from C++ DeltaStream.cpp faithfully reproduced: same-type/same-class items merge via key-level upsert, different-type or different-class items isolated into separate buffer entries

## Task Commits

Each task was committed atomically:

1. **Task 1: Delta emission pipeline (item, stream, flush task) and coalescing tests** - `96c1038` (feat)
2. **Task 2: Stdin loop, sync mode skeleton, IPC contract tests, and build integration** - `fc64d35` (feat)

## Files Created/Modified

- `app/mailsync-rs/src/delta/mod.rs` — module re-exports for delta pipeline
- `app/mailsync-rs/src/delta/item.rs` — DeltaStreamItem with per-field serde renames, coalescing via concatenate()/upsert_model_json()
- `app/mailsync-rs/src/delta/stream.rs` — DeltaStream Arc-shared sender with emit()/emit_process_state()
- `app/mailsync-rs/src/delta/flush.rs` — delta_flush_task: 500ms coalescing buffer, exclusive stdout lock, explicit flush()
- `app/mailsync-rs/src/stdin_loop.rs` — stdin_loop with shared Lines iterator, 10 StdinCommand variants, EOF shutdown signal
- `app/mailsync-rs/src/modes/sync.rs` — sync mode 3-task skeleton, graceful shutdown sequence
- `app/mailsync-rs/src/main.rs` — single shared BufReader/Lines, sync mode wired, delta+stdin_loop modules declared
- `app/mailsync-rs/src/modes/mod.rs` — added pub mod sync
- `app/mailsync-rs/tests/delta_coalesce.rs` — 9 coalescing algorithm tests
- `app/mailsync-rs/tests/ipc_contract.rs` — 6 IPC contract tests
- `scripts/start-dev.js` — mailsync-rs build step before Electron launch

## Decisions Made

- **Single shared BufReader/Lines for stdin:** Multiple `BufReader<tokio::io::Stdin>` instances cause data loss — the OS delivers pipe data in chunks, and each BufReader maintains its own internal buffer. When the first BufReader reads account JSON, it buffers the identity JSON line AND potentially command lines. Dropping the BufReader loses that buffered data. Fix: create ONE `Lines` iterator in `main.rs` and pass it by value through `read_account_json -> read_identity_json -> sync::run() -> stdin_loop`. This ensures zero data loss between handshake and command reading phases.

- **Exit sequence: await flush_handle before process::exit(141):** The ProcessState delta is emitted to the mpsc channel before the 500ms flush tick fires. If `process::exit(141)` is called immediately after stdin EOF, the ProcessState delta may still be in the coalescing buffer. Fix: on EOF, `stdin_loop` signals shutdown and returns; `sync::run()` drops `DeltaStream` (closing the mpsc channel sender); the `delta_flush_task` detects channel closure and flushes its buffer in the `None` arm; `sync::run()` awaits the flush handle; then calls `process::exit(141)`.

- **scripts/start-dev.js mailsync-rs build is non-fatal:** The C++ mailsync binary remains fully functional through Phase 9. A build failure for mailsync-rs logs a warning but does not abort Electron startup. This prevents developer workflow disruption during the incremental Rust migration.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Multiple BufReader instances on tokio::io::stdin() caused data loss**
- **Found during:** Task 2 (test_unknown_command_continues and test_no_deadlock_large_stdin failing with exit code 1)
- **Issue:** The plan described stdin_loop creating its own `BufReader(tokio::io::stdin())` after the handshake. But `read_account_json` and `read_identity_json` each created their own BufReaders. The first BufReader buffered the identity JSON line from the OS pipe. When dropped, this data was lost. The second BufReader then received the command lines, which failed to deserialize as Identity JSON, causing exit code 1.
- **Fix:** Created ONE `Lines<BufReader<tokio::io::Stdin>>` in `main.rs::run_mode()`, passed it by mutable reference through `read_account_json` and `read_identity_json`, then passed by value into `sync::run()` and finally into `stdin_loop`. The Lines iterator maintains a single internal buffer across all reads.
- **Files modified:** `app/mailsync-rs/src/main.rs`, `app/mailsync-rs/src/stdin_loop.rs`, `app/mailsync-rs/src/modes/sync.rs`
- **Verification:** test_unknown_command_continues and test_no_deadlock_large_stdin pass (exit 141)
- **Committed in:** `fc64d35` (Task 2 commit)

**2. [Rule 1 - Bug] ProcessState delta not flushed before process::exit(141)**
- **Found during:** Task 2 (test_stdout_flush_timing failing — ProcessState not in stdout)
- **Issue:** stdin_loop was calling `process::exit(141)` after a 100ms sleep. The delta_flush_task uses a 500ms tick. At 100ms, the ProcessState was still in the coalescing buffer and never reached stdout.
- **Fix:** Restructured shutdown: stdin_loop sends broadcast signal and returns; sync::run() drops DeltaStream (closes mpsc channel); delta_flush_task detects channel close and flushes remaining buffer; sync::run() awaits flush_handle; then calls process::exit(141).
- **Files modified:** `app/mailsync-rs/src/stdin_loop.rs`, `app/mailsync-rs/src/modes/sync.rs`
- **Verification:** test_stdout_flush_timing passes (ProcessState in stdout within 2s)
- **Committed in:** `fc64d35` (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (both Rule 1 bugs)
**Impact on plan:** Both fixes essential for correct IPC protocol operation. No scope creep.

## Issues Encountered

- `tokio::io::Lines::next_line()` doesn't require explicit `AsyncBufReadExt` trait import at call site (the method is available on the `Lines` type directly through its internal impl). Removing the unused import was required for clippy -D warnings to pass.
- `cargo build` at workspace root fails for mailcore-rs (requires MSYS2 dlltool.exe) — pre-existing constraint documented in Plan 01 SUMMARY. `cargo build -p unifymail-sync` succeeds. This is not a regression.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Sync mode skeleton is complete: binary starts, handshakes, emits ProcessState, reads commands, exits cleanly
- The delta pipeline (DeltaStream + delta_flush_task) is production-ready; later phases just call `delta.emit()` with their model JSONs
- stdin_loop StdinCommand enum has all 10 C++ command types; Phase 6+ implements real handlers by replacing the stub dispatch arms
- background_sync_stub in sync.rs is the plug-in point for Phase 7 IMAP workers — replace the `shutdown_rx.recv()` wait with actual IMAP sync loop
- scripts/start-dev.js builds mailsync-rs before Electron launch — npm start integration is complete

---
*Phase: 05-core-infrastructure-and-ipc-protocol*
*Completed: 2026-03-04*

## Self-Check: PASSED

- `app/mailsync-rs/src/delta/mod.rs`: FOUND
- `app/mailsync-rs/src/delta/item.rs`: FOUND
- `app/mailsync-rs/src/delta/stream.rs`: FOUND
- `app/mailsync-rs/src/delta/flush.rs`: FOUND
- `app/mailsync-rs/src/stdin_loop.rs`: FOUND
- `app/mailsync-rs/src/modes/sync.rs`: FOUND
- `app/mailsync-rs/tests/delta_coalesce.rs`: FOUND
- `app/mailsync-rs/tests/ipc_contract.rs`: FOUND
- Commits 96c1038 (Task 1) and fc64d35 (Task 2): FOUND in git log
- 44/44 tests pass (20 unit + 9 delta_coalesce + 6 ipc_contract + 9 mode_tests): VERIFIED
- cargo clippy -- -D warnings: PASSED (no errors)
