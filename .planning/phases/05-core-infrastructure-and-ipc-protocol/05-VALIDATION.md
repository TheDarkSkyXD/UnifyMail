---
phase: 5
slug: core-infrastructure-and-ipc-protocol
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-04
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`cargo test`) |
| **Config file** | None — standard `cargo test` conventions |
| **Quick run command** | `cd app/mailsync-rs && cargo test --lib` |
| **Full suite command** | `cd app/mailsync-rs && cargo test --test-threads=1` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailsync-rs && cargo test --lib && cargo clippy -- -D warnings`
- **After every plan wave:** Run `cd app && cargo build && cd mailsync-rs && cargo test --test-threads=1`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 05-01-01 | 01 | 1 | IPC-04 | integration | `cd app/mailsync-rs && cargo test --test mode_tests` | ❌ W0 | ⬜ pending |
| 05-01-02 | 01 | 1 | IPC-04 | integration | `cd app/mailsync-rs && cargo test --test mode_tests` | ❌ W0 | ⬜ pending |
| 05-02-01 | 02 | 2 | IPC-01, IPC-02, IPC-05, IPC-06, IMPR-08 | integration | `cd app/mailsync-rs && cargo test --test ipc_contract` | ❌ W0 | ⬜ pending |
| 05-02-02 | 02 | 2 | IPC-03 | unit | `cd app/mailsync-rs && cargo test --lib -- stdin_loop` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `app/mailsync-rs/tests/ipc_contract.rs` — covers IPC-01, IPC-02, IPC-05, IPC-06, IMPR-08 (child process spawn pattern)
- [ ] `app/mailsync-rs/tests/mode_tests.rs` — covers IPC-04 all modes, IPC-03 command parsing
- [ ] `app/mailsync-rs/tests/delta_coalesce.rs` — covers IPC-02 coalescing correctness
- [ ] Dev-dependencies: `rusqlite = { version = "0.38", features = ["bundled"] }`, `tempfile = "3"`, `serde_json`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Electron UI receives deltas in real time during idle | IPC-06 | Requires running Electron + Rust binary end-to-end | 1. `npm start` 2. Add test account 3. Observe delta messages in DevTools console within 500ms |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
