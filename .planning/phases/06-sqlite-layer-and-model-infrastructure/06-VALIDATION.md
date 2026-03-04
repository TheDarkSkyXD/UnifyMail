---
phase: 6
slug: sqlite-layer-and-model-infrastructure
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-04
validated: 2026-03-04
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) + tokio::test for async |
| **Config file** | `app/mailsync-rs/Cargo.toml` (dev-dependencies section) |
| **Quick run command** | `cd app/mailsync-rs && cargo test --bin mailsync-rs` |
| **Full suite command** | `cd app/mailsync-rs && cargo test` |
| **Total tests** | 145 unit + 9 integration (delta coalesce) + 6 IPC contract |
| **Runtime** | ~1s (unit), ~60s (full with IPC) |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailsync-rs && cargo test --bin mailsync-rs`
- **After every plan wave:** Run `cd app/mailsync-rs && cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** ~1 second (unit tests)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Tests | Status |
|---------|------|------|-------------|-----------|-------------------|-------|--------|
| 06-01-T1 | 01 | 1 | DATA-03 | unit | `cargo test models::` | 40 model tests (Message, Thread, Folder, Label, Contact) | ✅ green |
| 06-01-T2 | 01 | 1 | DATA-03, DATA-04 | unit | `cargo test models::` | 32 model tests (ContactBook, ContactGroup, Calendar, Event, Task, File, Identity, ModelPluginMetadata) | ✅ green |
| 06-02-T1 | 02 | 2 | DATA-01, DATA-02 | unit | `cargo test store::mail_store` | 21 store CRUD tests (save, remove, find, find_all, count, WAL, delta emission) | ✅ green |
| 06-02-T2 | 02 | 2 | DATA-05 | unit | `cargo test store::transaction` | 7 transaction tests (commit, rollback, delta accumulation, RAII drop) | ✅ green |
| 06-03-T1 | 03 | 3 | DATA-02, DATA-04 | unit | `cargo test lifecycle` | 16 lifecycle hook tests (metadata, FTS5, join tables, ThreadCounts) | ✅ green |
| 06-03-T2 | 03 | 3 | DATA-01..05 | integration | `cargo test --bin mailsync-rs` | 18 tests (3 e2e pipeline, 4 schema validation, 11 round-trips) | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Requirement Coverage

| Req ID | Description | Covering Tests | Status |
|--------|-------------|----------------|--------|
| DATA-01 | WAL mode, busy_timeout=5000ms, single-writer | `test_wal_reader_can_read_while_writer_active`, `test_open_with_delta_creates_writer_and_reader`, schema validation tests | ✅ COVERED |
| DATA-02 | Generic CRUD + lifecycle hooks | 21 store CRUD tests + 16 lifecycle hook tests | ✅ COVERED |
| DATA-03 | All 13 data model types with serde fidelity | 72 model tests + 11 round-trip tests | ✅ COVERED |
| DATA-04 | Lifecycle hooks (FTS5, join tables, metadata) | 16 lifecycle tests (metadata join table, ContactSearch, EventSearch, ThreadCategory, ThreadCounts, ContactContactGroup) | ✅ COVERED |
| DATA-05 | Transaction with delta accumulation | 7 transaction tests (commit emits, rollback discards, RAII drop, multi-model) | ✅ COVERED |

---

## Wave 0 Requirements

- [x] `app/mailsync-rs/src/store/` — test modules for DATA-01 through DATA-05 (inline `#[cfg(test)] mod tests`)
- [x] `app/mailsync-rs/src/models/` — round-trip tests for all 13 model types (72 model tests + 11 store round-trips)
- [x] Test helper: in-memory SQLite database factory with migrations (`setup_test_store()`, `setup_test_store_no_delta()`)
- [x] Test helper: delta channel receiver for verifying delta emission (`mpsc::UnboundedReceiver<DeltaStreamItem>`)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| *None* | — | — | — |

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 15s (unit tests run in ~1s)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated 2026-03-04

---

## Validation Audit 2026-03-04

| Metric | Count |
|--------|-------|
| Gaps found | 0 |
| Resolved | 0 |
| Escalated | 0 |
| Total tests verified | 145 unit + 9 integration |
| All requirements | COVERED |
