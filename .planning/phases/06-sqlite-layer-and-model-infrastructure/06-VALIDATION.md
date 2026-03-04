---
phase: 6
slug: sqlite-layer-and-model-infrastructure
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-04
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust built-in) + tokio::test for async |
| **Config file** | `app/mailsync-rs/Cargo.toml` (dev-dependencies section) |
| **Quick run command** | `cd app/mailsync-rs && cargo test --lib` |
| **Full suite command** | `cd app/mailsync-rs && cargo test` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailsync-rs && cargo test --lib`
- **After every plan wave:** Run `cd app/mailsync-rs && cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | DATA-01 | unit | `cargo test models::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DATA-02 | unit | `cargo test store::wal` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DATA-03 | unit+integration | `cargo test delta::` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DATA-04 | unit | `cargo test store::schema` | ❌ W0 | ⬜ pending |
| TBD | TBD | TBD | DATA-05 | unit | `cargo test store::writer` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Task IDs will be filled once plans are generated.*

---

## Wave 0 Requirements

- [ ] `app/mailsync-rs/src/store/tests/` — test module stubs for DATA-01 through DATA-05
- [ ] `app/mailsync-rs/src/models/tests/` — round-trip test stubs for all 13 model types
- [ ] Test helper: in-memory SQLite database factory with migrations applied
- [ ] Test helper: mock delta channel receiver for verifying delta emission

*Existing infrastructure: cargo test framework present from Phase 5. Phase 6 adds model-specific test modules.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| *None* | — | — | — |

*All phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
