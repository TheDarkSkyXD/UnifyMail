---
phase: 5
slug: core-infrastructure-and-ipc-protocol
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-04
updated: 2026-03-04
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`cargo test`) |
| **Config file** | None — standard `cargo test` conventions |
| **Quick run command** | `cd app/mailsync-rs && cargo test` |
| **Full suite command** | `cd app/mailsync-rs && cargo test --test-threads=1` |
| **Estimated runtime** | ~4 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailsync-rs && cargo test && cargo clippy -- -D warnings`
- **After every plan wave:** Run `cd app && cargo build && cd mailsync-rs && cargo test --test-threads=1`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 4 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 1 | IPC-04 | integration | `cd app/mailsync-rs && cargo test --test mode_tests` | ✅ | ✅ green |
| 05-01-02 | 01 | 1 | IPC-04 | integration | `cd app/mailsync-rs && cargo test --test mode_tests` | ✅ | ✅ green |
| 05-02-01 | 02 | 2 | IPC-01, IPC-02, IPC-05, IPC-06, IMPR-08 | integration | `cd app/mailsync-rs && cargo test --test ipc_contract --test delta_coalesce` | ✅ | ✅ green |
| 05-02-02 | 02 | 2 | IPC-03 | integration | `cd app/mailsync-rs && cargo test --test ipc_contract -- test_unknown_command` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Requirement Coverage Detail

| Req ID | Test Files | Key Tests | Status |
|--------|-----------|-----------|--------|
| IPC-01 | `ipc_contract.rs` | `test_handshake_and_process_state_delta`, `test_startup_prompt_printed_to_stdout` | COVERED |
| IPC-02 | `delta_coalesce.rs`, `ipc_contract.rs` | `test_delta_field_names_match_typescript`, `test_process_state_delta_shape`, `test_flush_buffer_json_output_format`, `test_handshake_and_process_state_delta` | COVERED |
| IPC-03 | `ipc_contract.rs` | `test_unknown_command_continues` | COVERED |
| IPC-04 | `mode_tests.rs` | `test_install_check_exits_0`, `test_test_mode_exits_1`, `test_migrate_creates_schema`, `test_migrate_creates_all_tables`, `test_migrate_creates_fts5_tables`, `test_migrate_idempotent`, `test_reset_isolates_account_data`, `test_reset_exits_0`, `test_sync_error_keys_match_cpp` | COVERED |
| IPC-05 | `ipc_contract.rs` | `test_stdin_eof_exit_141` | COVERED |
| IPC-06 | `ipc_contract.rs` | `test_stdout_flush_timing` | COVERED |
| IMPR-08 | `ipc_contract.rs` | `test_no_deadlock_large_stdin` | COVERED |

---

## Wave 0 Requirements

- [x] `app/mailsync-rs/tests/ipc_contract.rs` — covers IPC-01, IPC-02, IPC-05, IPC-06, IMPR-08 (child process spawn pattern)
- [x] `app/mailsync-rs/tests/mode_tests.rs` — covers IPC-04 all modes, IPC-03 command parsing
- [x] `app/mailsync-rs/tests/delta_coalesce.rs` — covers IPC-02 coalescing correctness
- [x] Dev-dependencies: `rusqlite = { version = "0.37", features = ["bundled"] }`, `tempfile = "3"`, `serde_json`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Electron UI receives deltas in real time during idle | IPC-06 | Requires running Electron + Rust binary end-to-end | 1. `npm start` 2. Add test account 3. Observe delta messages in DevTools console within 500ms |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 4s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-03-04

---

## Validation Audit 2026-03-04

| Metric | Count |
|--------|-------|
| Requirements audited | 7 |
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
| Total tests | 44 (20 unit + 9 delta_coalesce + 6 ipc_contract + 9 mode_tests) |
| All green | ✅ |
