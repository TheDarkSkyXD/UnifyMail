---
phase: 4
slug: cross-platform-packaging-and-cleanup
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-03
---

# Phase 4 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Jasmine 2.x (existing) + cargo test (Rust) |
| **Config file** | None for CI smoke test (inline script) |
| **Quick run command** | `node -e "require('mailcore-napi').providerForEmail('test@gmail.com')"` |
| **Full suite command** | `cd app/mailcore-rs && cargo test` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `node -e "require('mailcore-napi').providerForEmail('test@gmail.com')"`
- **After every plan wave:** Run `cd app/mailcore-rs && cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 04-01-01 | 01 | 1 | SCAF-03 | CI pass/fail | All 4 workflows complete successfully | ❌ W0 | ⬜ pending |
| 04-01-02 | 01 | 1 | SCAF-04 | automated size gate | `du -k *.node \| awk '$1>8192{exit 1}'` | ❌ W0 | ⬜ pending |
| 04-02-01 | 02 | 1 | INTG-01 | smoke | `node -e "require('mailcore-napi').providerForEmail('test@gmail.com')"` | ❌ W0 | ⬜ pending |
| 04-02-02 | 02 | 1 | INTG-02 | smoke | Same require path as INTG-01 | ❌ W0 | ⬜ pending |
| 04-02-03 | 02 | 1 | INTG-03 | automated | C++ deletion check script | ❌ W0 | ⬜ pending |
| 04-02-04 | 02 | 1 | INTG-04 | automated | `grep -v node-gyp package.json && grep -v node-addon-api app/package.json` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Size gate step in `build-linux.yaml` — covers SCAF-04
- [ ] Smoke test step in each workflow — covers INTG-01, INTG-02
- [ ] C++ deletion verification script — covers INTG-03, INTG-04

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| CI produces binaries for all 5 targets | SCAF-03 | Requires GitHub Actions runners | Push to branch, verify all 4 workflow runs pass |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
