---
phase: 3
slug: smtp-testing-and-account-validation
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-03
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness (`#[tokio::test]` attribute) |
| **Config file** | None — tokio test attribute handles async setup |
| **Quick run command** | `cd app/mailcore-rs && cargo test smtp_tests 2>/dev/null` |
| **Full suite command** | `cd app/mailcore-rs && cargo test 2>/dev/null` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailcore-rs && cargo test smtp_tests 2>/dev/null`
- **After every plan wave:** Run `cd app/mailcore-rs && cargo test 2>/dev/null`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 03-01-01 | 01 | 1 | SMTP-01 | unit (mock) | `cargo test smtp_tests::test_tls_on_plain_server_returns_tls_error` | ❌ W0 | ⬜ pending |
| 03-01-02 | 01 | 1 | SMTP-02 | unit (mock) | `cargo test smtp_tests::test_starttls_connection` | ❌ W0 | ⬜ pending |
| 03-01-03 | 01 | 1 | SMTP-03 | unit (mock) | `cargo test smtp_tests::test_clear_connection_succeeds` | ❌ W0 | ⬜ pending |
| 03-01-04 | 01 | 1 | SMTP-04 | unit (mock) | `cargo test smtp_tests::test_password_auth` / `cargo test smtp_tests::test_xoauth2_auth` | ❌ W0 | ⬜ pending |
| 03-01-05 | 01 | 1 | SMTP-05 | unit (mock) | `cargo test smtp_tests::test_timeout_fires` | ❌ W0 | ⬜ pending |
| 03-02-01 | 02 | 2 | VALD-01 | unit (mock) | `cargo test smtp_tests::test_validate_concurrent_timing` | ❌ W0 | ⬜ pending |
| 03-02-02 | 02 | 2 | VALD-02 | unit | `cargo test smtp_tests::test_validate_mx_fail_silent` | ❌ W0 | ⬜ pending |
| 03-02-03 | 02 | 2 | VALD-03 | unit | `cargo test smtp_tests::test_validate_result_shape` | ❌ W0 | ⬜ pending |
| 03-02-04 | 02 | 2 | VALD-04 | integration | `cd app && npx tsc --noEmit` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `app/mailcore-rs/tests/smtp_tests.rs` — stubs for SMTP-01 through VALD-03; mock SMTP server following `imap_tests.rs` pattern (inline mock handler, random port, `TcpListener::bind("127.0.0.1:0")`)
- [ ] Mock SMTP server protocol must handle: `220` greeting, `EHLO`, `AUTH LOGIN` (base64 multi-step), `AUTH XOAUTH2` (base64 single-step), `NOOP`, `QUIT`; optional `STARTTLS` for STARTTLS tests
- [ ] TypeScript type update: `app/mailcore/types/index.d.ts` must be updated with extended `AccountValidationResult` (adding `imapResult`, `smtpResult`, `errorType` fields) and extended `IMAPConnectionResult` / `SMTPConnectionResult`; VALD-04 verified by `cd app && npx tsc --noEmit`

*Existing test infrastructure in provider_tests.rs and imap_tests.rs covers all Phase 1/2 requirements and needs no changes*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| XOAUTH2 auth against live Gmail/Outlook | SMTP-04 | Requires valid OAuth2 token | 1. Configure test account with OAuth2, 2. Call testSMTPConnection with token, 3. Verify success:true |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
