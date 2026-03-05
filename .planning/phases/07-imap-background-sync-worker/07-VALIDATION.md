---
phase: 7
slug: imap-background-sync-worker
status: validated
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-04
validated: 2026-03-04
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness + `cargo test` |
| **Config file** | `app/mailsync-rs/Cargo.toml` `[dev-dependencies]` |
| **Quick run command** | `cd app/mailsync-rs && cargo test --bin mailsync-rs 2>&1` |
| **Full suite command** | `cd app/mailsync-rs && cargo test --test-threads=1 2>&1` |
| **Estimated runtime** | ~1 second (unit tests), ~60s+ (integration tests with IPC) |
| **Total tests** | 255 unit + 9 integration (264 total) |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailsync-rs && cargo test --bin mailsync-rs 2>&1 | tail -5`
- **After every plan wave:** Run `cd app/mailsync-rs && cargo test --test-threads=1 2>&1`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 1 second (unit tests)

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Actual Tests | Status |
|---------|------|------|-------------|-----------|-------------------|--------------|--------|
| 07-01-01 | 01 | 1 | ISYN-01 | unit | `cargo test --bin mailsync-rs imap::session::tests::role_for_name_attribute` | 7 tests (role_for_name_attribute_*) | ✅ green |
| 07-01-02 | 01 | 1 | ISYN-01 | unit | `cargo test --bin mailsync-rs imap::session::tests::role_for_folder_via_path` | 7 tests (role_for_folder_via_path_*) | ✅ green |
| 07-01-03 | 01 | 1 | ISYN-01 | unit | `cargo test --bin mailsync-rs imap::session::tests::is_gmail_sync_folder` | 3 tests (is_gmail_sync_folder_*) | ✅ green |
| 07-02-01 | 02 | 1 | ISYN-02 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::condstore_no_change` | 2 tests (condstore_no_change, condstore_no_change_requires_both_match) | ✅ green |
| 07-02-02 | 02 | 1 | ISYN-02 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::condstore_truncation` | 2 tests (condstore_truncation_*) | ✅ green |
| 07-02-03 | 02 | 1 | ISYN-03 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::uid_range_fallback` | 1 test (uid_range_fallback_when_condstore_unavailable) | ✅ green |
| 07-02-04 | 02 | 1 | ISYN-04 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::uidvalidity` | 4 tests (uidvalidity_*) | ✅ green |
| 07-03-01 | 03 | 1 | ISYN-05 | unit | `cargo test --bin mailsync-rs imap::mail_processor::tests::stable_id_ascii` | 1 test | ✅ green |
| 07-03-02 | 03 | 1 | ISYN-05 | unit | `cargo test --bin mailsync-rs imap::mail_processor::tests::stable_id_rfc2047` | 1 test | ✅ green |
| 07-03-03 | 03 | 1 | ISYN-05 | unit | `cargo test --bin mailsync-rs imap::mail_processor::tests::stable_id_no_date` | 1 test | ✅ green |
| 07-04-01 | 04 | 2 | ISYN-06 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::body_age_policy` | 7 tests (body_age_policy_*) | ✅ green |
| 07-04-02 | 04 | 2 | ISYN-06 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::body_age_policy_skip` | 2 tests (skip_spam, skip_trash) | ✅ green |
| 07-04-03 | 04 | 2 | ISYN-07 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::folder_priority` | 4 tests (folder_priority_*) | ✅ green |
| 07-05-01 | 05 | 2 | GMAL-01 | unit | `cargo test --bin mailsync-rs imap::session::tests::is_gmail_sync_folder_custom_label_false` | 1 test (non-whitelist returns false → becomes label) | ✅ green |
| 07-05-02 | 05 | 2 | GMAL-02 | unit | `cargo test --bin mailsync-rs imap::mail_processor::tests::gmail_labels` | 1 test (gmail_labels_struct_stores_strings) | ✅ green |
| 07-05-03 | 05 | 2 | GMAL-02 | unit | `cargo test --bin mailsync-rs imap::mail_processor::tests::gmail_thrid_extracted` | 1 test | ✅ green |
| 07-05-04 | 05 | 2 | GMAL-04 | unit | `cargo test --bin mailsync-rs imap::mail_processor::tests::gmail_skip_sent_append` | 1 test (gmail_skip_sent_append_constant) | ✅ green |
| 07-06-01 | 06 | 1 | OAUT-01 | manual | — | See Manual-Only (refresh request shape requires HTTP mock) | ⚠️ manual |
| 07-06-02 | 06 | 1 | OAUT-02 | unit | `cargo test --bin mailsync-rs oauth2::tests::expiry_buffer` | 2 tests (expiry_buffer_300s_*, expiry_buffer_constant_*) | ✅ green |
| 07-06-03 | 06 | 1 | OAUT-02 | unit | `cargo test --bin mailsync-rs oauth2::tests::valid_token_cached` | 1 test (valid_token_cached_returns_same_on_second_call) | ✅ green |
| 07-06-04 | 06 | 1 | OAUT-03 | unit | `cargo test --bin mailsync-rs oauth2::tests::secrets_updated_on_rotation` | 1 test | ✅ green |
| 07-07-01 | 07 | 2 | IMPR-05 | unit | `cargo test --bin mailsync-rs imap::sync_worker::tests::timeout_fires_on_hang` | 1 test | ✅ green |
| 07-07-02 | 07 | 2 | IMPR-06 | unit | `cargo test --bin mailsync-rs error::tests::is_retryable_variants` | 1 test | ✅ green |
| 07-07-03 | 07 | 2 | IMPR-06 | unit | `cargo test --bin mailsync-rs error::tests::is_auth_variants` | 1 test | ✅ green |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ manual/flaky*

**Summary:** 23/24 tasks automated and green. 1 task (07-06-01) marked manual-only.

---

## Wave 0 Requirements

- [x] `app/mailsync-rs/src/imap/mod.rs` — module declarations for session, sync_worker, mail_processor
- [x] `app/mailsync-rs/src/imap/session.rs` — ImapSession struct with TLS connect + auth and role detection tests (19 tests)
- [x] `app/mailsync-rs/src/imap/sync_worker.rs` — background_sync with all unit tests (42 tests)
- [x] `app/mailsync-rs/src/imap/mail_processor.rs` — Fetch → Message conversion + stable ID tests (21 tests)
- [x] `app/mailsync-rs/src/oauth2.rs` — TokenManager with unit tests (18 tests)
- [x] Phase 7 Cargo.toml additions: `async-imap`, `imap-proto`, `mail-parser`, `ammonia`, `oauth2`, `reqwest`, `sha2`, `bs58`, `rfc2047-decoder`, `chrono`, `base64`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Full E2E sync against live IMAP server | ISYN-01..07 | Requires real IMAP credentials | Connect test account, verify folders enumerate and messages appear in UI |
| OAuth2 token refresh with real Google API | OAUT-01..03 | Requires real OAuth credentials | Wait for token expiry, verify transparent refresh |
| Gmail-specific folder display in Electron sidebar | GMAL-01..04 | Requires real Gmail account | Connect Gmail, verify only INBOX/All Mail/Trash/Spam shown |
| OAuth2 refresh request shape (grant_type, client_id, refresh_token params) | OAUT-01 | Requires HTTP mock framework (not yet in dev-dependencies); individual components tested | Verify via `get_token_endpoint_*` (3 tests) + `get_client_id_from_extra` + `xoauth2_sasl_*` (2 tests) |

---

## Validation Sign-Off

- [x] All tasks have automated verify or manual-only justification
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all required modules and dependencies
- [x] No watch-mode flags
- [x] Feedback latency < 1s (unit tests)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** validated

---

## Validation Audit 2026-03-04

| Metric | Count |
|--------|-------|
| Gaps found | 1 |
| Resolved | 0 |
| Escalated (manual-only) | 1 |
| Tasks COVERED | 23 |
| Tasks MANUAL | 1 |
| Total unit tests | 255 |
| All tests passing | yes |

**Note:** Task 07-06-01 (refresh_request_shape) escalated to manual-only — the assembled HTTP request requires an HTTP mock framework (`mockito` or `wiremock`) not currently in dev-dependencies. Individual request components are fully tested: endpoint resolution (3 tests), client_id extraction (1 test), XOAUTH2 encoding (2 tests), response parsing (2 tests).
