---
phase: 1
slug: scaffolding-and-provider-detection
status: finalized
nyquist_compliant: true
wave_0_complete: false
created: 2026-03-03
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test runner (`cargo test`) + standalone Node.js scripts |
| **Config file** | None — `cargo test` runs `tests/*.rs` by default |
| **Quick run command** | `cd app/mailcore-rs && cargo test` |
| **Full suite command** | `cd app/mailcore-rs && cargo test && node tests/cross-validate-providers.js` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailcore-rs && cargo test`
- **After every plan wave:** Run `cd app/mailcore-rs && cargo test && node tests/cross-validate-providers.js`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 01-01-01 | 01 | 1 | SCAF-01 | manual | `ls app/mailcore-rs/{Cargo.toml,build.rs,package.json}` | MISSING — Wave 0 must create scaffolding first | pending |
| 01-01-02 | 01 | 1 | PROV-01, PROV-02, PROV-03 | integration | `cargo test --test provider_tests` | MISSING — Wave 0 must create `tests/provider_tests.rs` first | pending |
| 01-02-01 | 02 | 2 | SCAF-02, PROV-04 | integration | `npx electron test/electron-integration-test.js && node app/mailcore-rs/tests/cross-validate-providers.js` | MISSING — Wave 0 must create `test/electron-integration-test.js` and `app/mailcore-rs/tests/cross-validate-providers.js` first | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

- [ ] `app/mailcore-rs/tests/provider_tests.rs` — covers PROV-01, PROV-02, PROV-03 (created in Plan 01 Task 2)
- [ ] `app/mailcore-rs/tests/cross-validate-providers.js` — covers PROV-04 (created in Plan 02 Task 1)
- [ ] `test/electron-integration-test.js` — covers SCAF-02 (created in Plan 02 Task 1)
- [ ] `app/mailcore-rs/resources/providers.json` — required for SCAF-01 build (created in Plan 01 Task 1)

*Existing infrastructure covers `cargo test` — no additional test framework install needed.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Addon loads in Electron without crashes | SCAF-02 | Requires running Electron process | Run `npm start`, open DevTools console, verify no crash and `$m.MailcoreRs` accessible |
| No BoringSSL/OpenSSL symbol conflicts | SCAF-01 | Runtime symbol check | Run `cargo tree | grep openssl` — must return nothing |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 15s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** finalized (revised 2026-03-03 — fixed Plan 03 phantom reference, corrected task-to-plan mapping, set nyquist_compliant)
