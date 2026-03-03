---
phase: 1
slug: scaffolding-and-provider-detection
status: draft
nyquist_compliant: false
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
| 01-01-01 | 01 | 0 | SCAF-01 | manual | `ls app/mailcore-rs/{Cargo.toml,build.rs,package.json}` | ❌ W0 | ⬜ pending |
| 01-01-02 | 01 | 1 | SCAF-02 | integration | `electron test/electron-integration-test.js` | ❌ W0 | ⬜ pending |
| 01-02-01 | 02 | 1 | PROV-01 | integration | `cargo test --test provider_tests -- test_register_providers` | ❌ W0 | ⬜ pending |
| 01-02-02 | 02 | 1 | PROV-02 | integration | `cargo test --test provider_tests -- test_auto_init` | ❌ W0 | ⬜ pending |
| 01-02-03 | 02 | 1 | PROV-03 | unit | `cargo test --test provider_tests -- test_provider_for_email` | ❌ W0 | ⬜ pending |
| 01-03-01 | 03 | 2 | PROV-04 | integration | `node app/mailcore-rs/tests/cross-validate-providers.js` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `app/mailcore-rs/tests/provider_tests.rs` — stubs for PROV-01, PROV-02, PROV-03
- [ ] `app/mailcore-rs/tests/cross-validate-providers.js` — covers PROV-04 (requires both addons built)
- [ ] `test/electron-integration-test.js` — covers SCAF-02 (requires Electron)
- [ ] `app/mailcore-rs/resources/providers.json` — required for SCAF-01 build

*Existing infrastructure covers `cargo test` — no additional test framework install needed.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Addon loads in Electron without crashes | SCAF-02 | Requires running Electron process | Run `npm start`, open DevTools console, verify no crash and `$m.MailcoreRs` accessible |
| No BoringSSL/OpenSSL symbol conflicts | SCAF-01 | Runtime symbol check | Run `cargo tree \| grep openssl` — must return nothing |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
