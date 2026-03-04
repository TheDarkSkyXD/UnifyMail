---
phase: 3
slug: smtp-testing-and-account-validation
status: complete
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-03
audited: 2026-03-03
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
| 03-01-01 | 01 | 1 | SMTP-01 | unit (mock) | `cargo test smtp_tests::test_tls_against_plain_server` | ✅ | ✅ green |
| 03-01-02 | 01 | 1 | SMTP-02 | unit (mock) | `cargo test smtp_tests::test_starttls_against_plain_server` | ✅ | ✅ green |
| 03-01-03 | 01 | 1 | SMTP-03 | unit (mock) | `cargo test smtp_tests::test_clear_connection_succeeds` | ✅ | ✅ green |
| 03-01-04 | 01 | 1 | SMTP-04 | unit (mock) | `cargo test smtp_tests::test_password_auth` / `cargo test smtp_tests::test_xoauth2_auth` | ✅ | ✅ green |
| 03-01-05 | 01 | 1 | SMTP-05 | unit (mock) | `cargo test smtp_tests::test_timeout` | ✅ | ✅ green |
| 03-02-01 | 02 | 2 | VALD-01 | unit (mock) | `cargo test smtp_tests::test_validate_concurrent_timing` | ✅ | ✅ green |
| 03-02-02 | 02 | 2 | VALD-02 | unit (mock) | `cargo test smtp_tests::test_validate_result_shape` (identifier=None proves MX fail-silent) | ✅ | ✅ green |
| 03-02-03 | 02 | 2 | VALD-03 | unit (mock) | `cargo test smtp_tests::test_validate_result_shape` | ✅ | ✅ green |
| 03-02-04 | 02 | 2 | VALD-04 | type defs | TypeScript type exports in `app/mailcore-rs/index.d.ts` | ✅ | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

**Additional tests (bonus coverage beyond requirement map):**

| Test Name | Coverage |
|-----------|----------|
| `test_clear_no_credentials_connect_only` | Connect-only mode (no auth) |
| `test_auth_failure` | errorType=auth_failed classification |
| `test_connection_refused` | errorType=connection_refused classification |
| `test_validate_both_succeed` | Happy path: both protocols succeed |
| `test_validate_imap_fails_smtp_succeeds` | IMAP error propagation |
| `test_validate_smtp_fails_imap_succeeds` | SMTP error propagation |
| `test_validate_both_fail_imap_priority` | IMAP priority on both-fail |
| `test_validate_imap_capabilities_on_success` | IMAP capabilities field |

---

## Wave 0 Requirements

- [x] `app/mailcore-rs/tests/smtp_tests.rs` — 9 SMTP tests + 7 validation tests with mock SMTP server following `imap_tests.rs` pattern (inline mock handler, random port, `TcpListener::bind("127.0.0.1:0")`)
- [x] Mock SMTP server protocol handles: `220` greeting, `EHLO`, `AUTH LOGIN` (base64 multi-step), `AUTH XOAUTH2` (base64 single-step), `NOOP`, `QUIT`
- [x] TypeScript type definitions: `app/mailcore-rs/index.d.ts` updated with `IMAPConnectionOptions`, `IMAPConnectionResult`, `SMTPConnectionOptions`, `SMTPConnectionResult`, `AccountValidationResult`, `ValidateAccountOptions`, and all sub-types (`IMAPSubResult`, `SMTPSubResult`, `ServerInfo`)

*Existing test infrastructure in provider_tests.rs and imap_tests.rs covers all Phase 1/2 requirements and needs no changes*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| XOAUTH2 auth against live Gmail/Outlook | SMTP-04 | Requires valid OAuth2 token | 1. Configure test account with OAuth2, 2. Call testSMTPConnection with token, 3. Verify success:true |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** complete

---

## Validation Audit 2026-03-03

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 1 |
| Escalated | 0 |

**Gap resolved:** VALD-04 — Added TypeScript type definitions for all IMAP, SMTP, and validation exports to `app/mailcore-rs/index.d.ts`. Includes `IMAPConnectionOptions`, `IMAPConnectionResult`, `SMTPConnectionOptions`, `SMTPConnectionResult`, `ValidateAccountOptions`, `AccountValidationResult`, and sub-types (`IMAPSubResult`, `SMTPSubResult`, `ServerInfo`).

**Note:** Test names in the original VALIDATION.md draft didn't match actual implementations (e.g., `test_tls_on_plain_server_returns_tls_error` vs actual `test_tls_against_plain_server`). Updated to match real test names. All 44 Rust tests confirmed passing per execution SUMMARYs (9 SMTP + 7 validate + 12 IMAP + 16 provider).
