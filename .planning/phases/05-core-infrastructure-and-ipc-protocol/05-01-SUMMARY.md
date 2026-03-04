---
phase: 05-core-infrastructure-and-ipc-protocol
plan: 01
subsystem: infra
tags: [rust, cargo-workspace, sqlite, clap, tokio, serde, rusqlite, tokio-rusqlite, tracing, thiserror, fts5]

# Dependency graph
requires: []
provides:
  - "Cargo workspace at app/Cargo.toml with mailcore-rs and mailsync-rs members"
  - "mailsync-rs binary crate (name=unifymail-sync, bin=mailsync-rs)"
  - "CLI argument parsing: --mode (migrate/install-check/reset/test/sync), --verbose, --orphan, --info, --account, --identity"
  - "SyncError enum with all ~20 C++ error key variants and error_key() method returning exact C++ strings"
  - "Account + Identity deserialization structs with serde flatten for forward-compatibility"
  - "--mode migrate: creates edgehill.db with schema version 9, 22+ tables, 3 FTS5 virtual tables"
  - "--mode install-check: exits 0 with JSON health check result"
  - "--mode reset: deletes all data for specified account, preserves other accounts"
  - "--mode test: stub returning ErrorNotImplemented (exits 1)"
  - "MailStore: open() with WAL/PRAGMA settings, migrate(), reset_for_account()"
  - "V1-V9 SQL migration arrays as exact copies from C++ constants.h"
  - "Integration tests: 9 tests covering all offline modes"
affects:
  - "05-02 (IPC protocol): sync mode will use MailStore, SyncError, Account, and cli.rs"
  - "06+ (IMAP/SMTP phases): Account fields (email_address, provider) used in connection code"
  - "All Phase 5+ plans: Cargo workspace established, shared dep versions locked"

# Tech tracking
tech-stack:
  added:
    - "tokio 1.50.0 (workspace) — async runtime with io-std, sync, fs features"
    - "clap 4 with derive — CLI argument parsing"
    - "rusqlite 0.37 with bundled — SQLite with FTS5 (workspace-compatible version)"
    - "tokio-rusqlite 0.7 — async SQLite wrapper"
    - "tracing 0.1 + tracing-subscriber 0.3 + tracing-appender 0.2 — structured logging"
    - "thiserror 2 — error type derivation"
    - "indexmap 2 — ordered map for delta coalesce buffer (future plan)"
    - "tempfile 3 (dev) — tempdir isolation for integration tests"
  patterns:
    - "tokio-rusqlite Connection::call closure requires explicit return type annotation (Result<T, rusqlite::Error>)"
    - "SyncError uses #[allow(dead_code)] on full enum — all ~20 C++ variants pre-defined per 05-CONTEXT.md"
    - "Account/Identity structs use #[allow(dead_code)] — fields used by future IMAP/SMTP phases"
    - "Integration tests locate binary via CARGO_MANIFEST_DIR/../target/debug/ (workspace binary location)"
    - "Multi-line stdout parsing in tests: parse_last_json_line() handles handshake prompts before JSON"
    - "Cargo workspace profile.release defined at workspace root — per-crate profiles ignored"

key-files:
  created:
    - "app/Cargo.toml — workspace root with members, shared deps, release profile"
    - "app/Cargo.lock — workspace dependency lock file"
    - "app/mailsync-rs/Cargo.toml — binary crate definition"
    - "app/mailsync-rs/src/main.rs — tokio::main entry point with mode dispatch"
    - "app/mailsync-rs/src/cli.rs — Args struct and Mode enum with clap derive"
    - "app/mailsync-rs/src/error.rs — SyncError with all C++ error key mappings"
    - "app/mailsync-rs/src/account.rs — Account + Identity deserialization structs"
    - "app/mailsync-rs/src/modes/mod.rs — modes module re-exports"
    - "app/mailsync-rs/src/modes/install_check.rs — --mode install-check handler"
    - "app/mailsync-rs/src/modes/test_auth.rs — --mode test stub handler"
    - "app/mailsync-rs/src/modes/migrate.rs — --mode migrate handler"
    - "app/mailsync-rs/src/modes/reset.rs — --mode reset handler"
    - "app/mailsync-rs/src/store/mod.rs — store module re-exports"
    - "app/mailsync-rs/src/store/mail_store.rs — MailStore with open/migrate/reset_for_account"
    - "app/mailsync-rs/src/store/migrations.rs — V1-V9 SQL arrays + ACCOUNT_RESET_QUERIES"
    - "app/mailsync-rs/tests/mode_tests.rs — 9 integration tests for all offline modes"
  modified:
    - "app/mailcore-rs/Cargo.toml — updated to use workspace dep references, removed per-crate profile"

key-decisions:
  - "rusqlite pinned to 0.37 (not 0.38) to match tokio-rusqlite 0.7's internal dependency — prevents libsqlite3-sys conflict"
  - "tokio workspace feature set includes io-std — required for tokio::io::stdin() in mailsync-rs (not needed by mailcore-rs)"
  - "workspace [profile.release] uses mailcore-rs settings (panic=abort, opt-level=z) — unified profile acceptable for both cdylib and binary"
  - "ThreadListSortIndex on lastMessageReceivedTimestamp moved to V8 migration — column doesn't exist in V1 schema"
  - "parse_last_json_line() helper in tests — handles mixed stdout (handshake prompts + JSON error) from test mode"
  - "All ~20 SyncError variants defined upfront with #[allow(dead_code)] — per 05-CONTEXT.md design decision"
  - "test_auth.rs returns Err without printing — main.rs handles stdout JSON error to avoid double-printing"

patterns-established:
  - "Binary crate in workspace: run tests from crate dir (cargo test --test mode_tests -- --test-threads=1)"
  - "Binary path in integration tests: CARGO_MANIFEST_DIR/../target/debug/mailsync-rs.exe (workspace target)"
  - "tokio-rusqlite type inference: closure must have explicit return type -> Result<T, rusqlite::Error>"
  - "PRAGMA setup pattern: execute_batch with WAL, page_size, cache_size, synchronous; separate busy_timeout call"
  - "Migration idempotency: CREATE TABLE IF NOT EXISTS + version guard (if version < N)"

requirements-completed:
  - IPC-04

# Metrics
duration: 15min
completed: 2026-03-04
---

# Phase 5 Plan 01: Cargo Workspace, Binary Scaffold, SQLite Schema, and Offline Modes Summary

**mailsync-rs binary crate with Cargo workspace, clap CLI, SyncError with C++ error key parity, SQLite V1-V9 schema migrations with FTS5, and all 4 offline modes (migrate/install-check/reset/test) fully implemented and tested**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-04T13:47:38Z
- **Completed:** 2026-03-04T14:02:27Z
- **Tasks:** 2 completed
- **Files modified:** 17 created, 1 modified

## Accomplishments

- Cargo workspace at `app/Cargo.toml` compiles both mailcore-rs (workspace dep references) and mailsync-rs (new binary) with shared dep versions and unified release profile
- `--mode migrate` creates edgehill.db at CONFIG_DIR_PATH with schema version 9, all 22+ tables (including 3 FTS5 virtual tables: ThreadSearch, EventSearch, ContactSearch), all indexes — idempotent
- `--mode install-check` exits 0 with JSON health result; `--mode reset` deletes all account data while preserving other accounts; `--mode test` exits 1 with ErrorNotImplemented JSON
- SyncError enum has all ~20 C++ error key variants with `error_key()` returning exact C++ strings (ErrorAuthentication, ErrorConnection, ErrorTLSNotAvailable, etc.) and `to_json_error()` for stdout IPC shape
- 9 integration tests pass covering all offline modes with tempdir isolation

## Task Commits

Each task was committed atomically:

1. **Task 1: Cargo workspace, binary crate scaffold, CLI, errors, and account types** - `47d1552` (feat)
2. **Task 2: SQLite schema migrations and reset mode with mode integration tests** - `fa797b8` (feat)

## Files Created/Modified

- `app/Cargo.toml` — workspace root with members, shared dependency versions, unified [profile.release]
- `app/Cargo.lock` — workspace dependency lock file
- `app/mailcore-rs/Cargo.toml` — updated to use workspace dep references (tokio, serde, serde_json)
- `app/mailsync-rs/Cargo.toml` — binary crate definition (name=unifymail-sync, bin=mailsync-rs)
- `app/mailsync-rs/src/main.rs` — tokio::main entry point with mode dispatch and error handling
- `app/mailsync-rs/src/cli.rs` — Args + Mode enum with clap derive (all 5 modes, all flags)
- `app/mailsync-rs/src/error.rs` — SyncError with all ~20 C++ error key variants and JSON error serialization
- `app/mailsync-rs/src/account.rs` — Account + Identity with serde flatten for forward-compatibility
- `app/mailsync-rs/src/modes/install_check.rs` — install-check stub exits 0 with JSON
- `app/mailsync-rs/src/modes/test_auth.rs` — test stub returns ErrorNotImplemented
- `app/mailsync-rs/src/modes/migrate.rs` — opens MailStore, runs migrate(), exits 0
- `app/mailsync-rs/src/modes/reset.rs` — opens MailStore, migrates, resets account data
- `app/mailsync-rs/src/store/migrations.rs` — V1-V9 SQL arrays + ACCOUNT_RESET_QUERIES from constants.h
- `app/mailsync-rs/src/store/mail_store.rs` — MailStore: open() (WAL/PRAGMAs), migrate(), reset_for_account()
- `app/mailsync-rs/tests/mode_tests.rs` — 9 integration tests for all offline modes

## Decisions Made

- Pinned rusqlite to 0.37 (not 0.38) — tokio-rusqlite 0.7 requires rusqlite 0.37; using 0.38 causes libsqlite3-sys link conflict
- Added `io-std` tokio feature to workspace — required for `tokio::io::stdin()` in mailsync-rs binary (mailcore-rs doesn't need it)
- Moved `ThreadListSortIndex` from V1 to V8 — index references `lastMessageReceivedTimestamp` which only exists after the V8 ALTER TABLE
- Defined all ~20 SyncError variants upfront with `#[allow(dead_code)]` — per 05-CONTEXT.md "Full error enum defined upfront" design decision
- test_auth.rs returns Err without printing — main.rs handles the JSON output to avoid double-printing error JSON

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] ThreadListSortIndex referenced non-existent V1 column**
- **Found during:** Task 2 (migrate mode testing)
- **Issue:** V1_SETUP included `CREATE INDEX ThreadListSortIndex ON Thread (accountId, lastMessageReceivedTimestamp)` but `lastMessageReceivedTimestamp` is only added in V8 via ALTER TABLE, causing runtime SQL error
- **Fix:** Moved `ThreadListSortIndex` creation to V8_SETUP (after the column is added)
- **Files modified:** `app/mailsync-rs/src/store/migrations.rs`
- **Verification:** `--mode migrate` exits 0 with 9 integration tests passing
- **Committed in:** `fa797b8` (Task 2 commit)

**2. [Rule 1 - Bug] Binary path in tests pointed to crate-local target (wrong for workspace)**
- **Found during:** Task 1 (integration test first run)
- **Issue:** `binary_path()` used `CARGO_MANIFEST_DIR/target/debug/` but workspace puts binary at `CARGO_MANIFEST_DIR/../target/debug/`
- **Fix:** Changed path to `CARGO_MANIFEST_DIR/../target/debug/` (parent directory = workspace root)
- **Files modified:** `app/mailsync-rs/tests/mode_tests.rs`
- **Verification:** Integration tests now locate and spawn binary successfully
- **Committed in:** `47d1552` (Task 1 commit)

**3. [Rule 1 - Bug] test_auth.rs double-printed JSON error (once in mode, once in main.rs error handler)**
- **Found during:** Task 1 (test_test_mode_exits_1 test parsing)
- **Issue:** test_auth.rs printed JSON error before returning Err, then main.rs also printed it — stdout contained two JSON objects; test couldn't parse it
- **Fix:** Removed println from test_auth.rs — let main.rs handle stdout JSON error uniformly. Added `parse_last_json_line()` helper to tests for robustness
- **Files modified:** `app/mailsync-rs/src/modes/test_auth.rs`, `app/mailsync-rs/tests/mode_tests.rs`
- **Verification:** test_test_mode_exits_1 passes
- **Committed in:** `47d1552` (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (all Rule 1 bugs)
**Impact on plan:** All fixes required for correct operation. No scope creep.

## Issues Encountered

- `tokio-rusqlite::Connection::call` closures require explicit `-> Result<T, rusqlite::Error>` return type annotation — Rust cannot infer the error type when multiple `impl From<rusqlite::Error>` exist. Resolved by adding explicit type annotations.
- `tokio::io::stdin()` requires the `io-std` feature in tokio — workspace tokio dep inherited mailcore-rs feature set (no `io-std`). Resolved by adding `io-std` to workspace tokio features.
- `cargo clippy -- -D warnings` failed on dead_code for pre-defined SyncError variants and Account fields that future phases will use. Resolved with targeted `#[allow(dead_code)]` on the enum and structs per 05-CONTEXT.md design intent.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- mailsync-rs binary foundation complete: Cargo workspace, binary compiles, all offline modes functional
- Plan 02 (IPC Protocol + Sync Mode Skeleton) can build directly on this: `MailStore`, `SyncError`, `Account`, `cli::Args` are all ready
- The Cargo workspace lock is established — adding crates (async-imap, lettre) in future plans will not require workspace restructuring
- mailcore-rs workspace migration: pre-existing `LIBNODE_PATH` / `libnode.dll` requirement unchanged (not a regression from our changes)

---
*Phase: 05-core-infrastructure-and-ipc-protocol*
*Completed: 2026-03-04*

## Self-Check: PASSED

- All 17 key files created: FOUND
- Commits 47d1552 (Task 1) and fa797b8 (Task 2): FOUND in git log
- 9/9 integration tests pass: VERIFIED
- cargo clippy -- -D warnings: PASSED (no errors)
