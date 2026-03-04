---
phase: 02
slug: imap-connection-testing
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-03
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[tokio::test]` (cargo test) |
| **Config file** | `app/mailcore-rs/Cargo.toml` |
| **Quick run command** | `cd app/mailcore-rs && cargo test --test imap_tests` |
| **Full suite command** | `cd app/mailcore-rs && cargo test` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailcore-rs && cargo test --test imap_tests`
- **After every plan wave:** Run `cd app/mailcore-rs && cargo test`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | IMAP-01..06 | infra | `cd app/mailcore-rs && cargo check` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-01 | integration | `cargo test --test imap_tests test_tls_connection_fails_with_tls_error_on_plain_server` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-02 | manual | N/A — STARTTLS requires real TLS server | ❌ manual | ⚠️ manual-only |
| 02-01-02 | 01 | 1 | IMAP-03 | integration | `cargo test --test imap_tests test_clear_connection_with_password` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-04 | integration | `cargo test --test imap_tests test_xoauth2_authentication` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-04 | integration | `cargo test --test imap_tests test_xoauth2_sasl_format_validation` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-04 | integration | `cargo test --test imap_tests test_auth_failure_returns_error` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-05 | integration | `cargo test --test imap_tests test_capability_detection_all_seven` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-05 | integration | `cargo test --test imap_tests test_capability_detection_partial` | ✅ | ✅ green |
| 02-01-02 | 01 | 1 | IMAP-06 | integration | `cargo test --test imap_tests test_timeout_returns_error` | ✅ | ✅ green |
| 02-02-01 | 02 | 2 | IMAP-01..06 | integration | `cargo test --test imap_tests` | ✅ | ✅ green |
| 02-02-01 | 02 | 2 | errors | integration | `cargo test --test imap_tests test_connection_refused_returns_error` | ✅ | ✅ green |
| 02-02-01 | 02 | 2 | errors | integration | `cargo test --test imap_tests test_invalid_greeting_returns_error` | ✅ | ✅ green |
| 02-02-01 | 02 | 2 | errors | integration | `cargo test --test imap_tests test_mid_connection_drop` | ✅ | ✅ green |
| 02-02-01 | 02 | 2 | errors | integration | `cargo test --test imap_tests test_error_includes_hostname_port` | ✅ | ✅ green |
| 02-02-02 | 02 | 2 | wrapper | manual | N/A — requires built .node binary | ❌ manual | ⚠️ manual-only |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

Existing infrastructure covers all phase requirements. No additional test framework installation needed.

- [x] `app/mailcore-rs/tests/imap_tests.rs` — 12 mock IMAP server tests
- [x] Mock server infrastructure (per-test TcpListener, MockAuthMode enum)
- [x] `cargo test` configuration in Cargo.toml

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| TLS positive path (direct TLS connection succeeds with valid cert) | IMAP-01 | rustls-platform-verifier rejects self-signed certs; mock TCP server cannot provide valid TLS. Deferred to Phase 3 integration. | `cd app/mailcore-rs && cargo test --test imap_tests` validates TLS error classification. Positive path: test against a real IMAP server (e.g., `imap.gmail.com:993`) with valid credentials. |
| STARTTLS upgrade path | IMAP-02 | STARTTLS requires real TLS server with valid certificate chain. Mock TCP cannot complete STARTTLS upgrade. Deferred to Phase 3 integration. | Code inspection: `connect_starttls()` at `imap.rs:242` implements TCP→Client→STARTTLS→TLS upgrade→new Client. Test against a real server supporting STARTTLS (e.g., port 143). |
| Wrapper routing to Rust | wrapper | Requires built `.node` native binary on disk. Build environment needs MSYS2 MinGW-w64 and libnode.dll. | Run `npm run build:rust` then `node -e "const m = require('./app/mailcore-wrapper'); console.log(typeof m.testIMAPConnection);"` — should print `function`. Set `MAILCORE_DEBUG=1` and call testIMAPConnection to see "testIMAPConnection -> Rust" in console. |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 15s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-03-03

---

## Validation Audit 2026-03-03

| Metric | Count |
|--------|-------|
| Gaps found | 3 |
| Resolved | 0 |
| Escalated | 3 (manual-only — architectural limitations) |

All 3 gaps are by-design limitations:
- TLS/STARTTLS positive paths require real servers with valid certificates (rustls-platform-verifier)
- Wrapper routing requires built native binary

12 of 12 automated tests cover requirements IMAP-03 through IMAP-06 fully, IMAP-01 negative path, and all error scenarios. Phase 3 integration will close the TLS/STARTTLS positive-path gaps.
