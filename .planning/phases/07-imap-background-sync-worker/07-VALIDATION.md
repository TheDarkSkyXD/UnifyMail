---
phase: 7
slug: imap-background-sync-worker
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-04
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in test harness + `cargo test` |
| **Config file** | `app/mailsync-rs/Cargo.toml` `[dev-dependencies]` |
| **Quick run command** | `cd app/mailsync-rs && cargo test --lib 2>&1` |
| **Full suite command** | `cd app/mailsync-rs && cargo test --test-threads=1 2>&1` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd app/mailsync-rs && cargo test --lib 2>&1 | tail -5`
- **After every plan wave:** Run `cd app/mailsync-rs && cargo test --test-threads=1 2>&1`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 07-01-01 | 01 | 1 | ISYN-01 | unit | `cargo test --lib imap::session::tests::role_detection -x` | ❌ W0 | ⬜ pending |
| 07-01-02 | 01 | 1 | ISYN-01 | unit | `cargo test --lib imap::session::tests::role_via_path -x` | ❌ W0 | ⬜ pending |
| 07-01-03 | 01 | 1 | ISYN-01 | unit | `cargo test --lib imap::sync_worker::tests::gmail_folder_whitelist -x` | ❌ W0 | ⬜ pending |
| 07-02-01 | 02 | 1 | ISYN-02 | unit | `cargo test --lib imap::sync_worker::tests::condstore_no_change -x` | ❌ W0 | ⬜ pending |
| 07-02-02 | 02 | 1 | ISYN-02 | unit | `cargo test --lib imap::sync_worker::tests::condstore_truncation -x` | ❌ W0 | ⬜ pending |
| 07-02-03 | 02 | 1 | ISYN-03 | unit | `cargo test --lib imap::sync_worker::tests::uid_range_fallback -x` | ❌ W0 | ⬜ pending |
| 07-02-04 | 02 | 1 | ISYN-04 | unit | `cargo test --lib imap::sync_worker::tests::uidvalidity_reset -x` | ❌ W0 | ⬜ pending |
| 07-03-01 | 03 | 1 | ISYN-05 | unit | `cargo test --lib imap::mail_processor::tests::stable_id_ascii -x` | ❌ W0 | ⬜ pending |
| 07-03-02 | 03 | 1 | ISYN-05 | unit | `cargo test --lib imap::mail_processor::tests::stable_id_rfc2047 -x` | ❌ W0 | ⬜ pending |
| 07-03-03 | 03 | 1 | ISYN-05 | unit | `cargo test --lib imap::mail_processor::tests::stable_id_no_date -x` | ❌ W0 | ⬜ pending |
| 07-04-01 | 04 | 2 | ISYN-06 | unit | `cargo test --lib imap::sync_worker::tests::body_age_policy -x` | ❌ W0 | ⬜ pending |
| 07-04-02 | 04 | 2 | ISYN-06 | unit | `cargo test --lib imap::sync_worker::tests::body_skip_spam_trash -x` | ❌ W0 | ⬜ pending |
| 07-04-03 | 04 | 2 | ISYN-07 | unit | `cargo test --lib imap::sync_worker::tests::folder_priority_sort -x` | ❌ W0 | ⬜ pending |
| 07-05-01 | 05 | 2 | GMAL-01 | unit | `cargo test --lib imap::sync_worker::tests::gmail_non_whitelist_becomes_label -x` | ❌ W0 | ⬜ pending |
| 07-05-02 | 05 | 2 | GMAL-02 | unit | `cargo test --lib imap::mail_processor::tests::gmail_labels_parsed -x` | ❌ W0 | ⬜ pending |
| 07-05-03 | 05 | 2 | GMAL-02 | unit | `cargo test --lib imap::mail_processor::tests::gmail_thrid_extracted -x` | ❌ W0 | ⬜ pending |
| 07-05-04 | 05 | 2 | GMAL-04 | unit | `cargo test --lib imap::sync_worker::tests::gmail_skip_append -x` | ❌ W0 | ⬜ pending |
| 07-06-01 | 06 | 1 | OAUT-01 | unit | `cargo test --lib oauth2::tests::refresh_request_shape -x` | ❌ W0 | ⬜ pending |
| 07-06-02 | 06 | 1 | OAUT-02 | unit | `cargo test --lib oauth2::tests::expiry_buffer_300s -x` | ❌ W0 | ⬜ pending |
| 07-06-03 | 06 | 1 | OAUT-02 | unit | `cargo test --lib oauth2::tests::valid_token_cached -x` | ❌ W0 | ⬜ pending |
| 07-06-04 | 06 | 1 | OAUT-03 | unit | `cargo test --lib oauth2::tests::secrets_updated_on_rotation -x` | ❌ W0 | ⬜ pending |
| 07-07-01 | 07 | 2 | IMPR-05 | unit | `cargo test --lib imap::sync_worker::tests::timeout_fires_on_hang -x` | ❌ W0 | ⬜ pending |
| 07-07-02 | 07 | 2 | IMPR-06 | unit | `cargo test --lib error::tests::is_retryable_variants -x` | ❌ W0 | ⬜ pending |
| 07-07-03 | 07 | 2 | IMPR-06 | unit | `cargo test --lib error::tests::is_auth_variants -x` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `app/mailsync-rs/src/imap/mod.rs` — module declarations for session, sync_worker, mail_processor
- [ ] `app/mailsync-rs/src/imap/session.rs` — ImapSession struct with TLS connect + auth stubs and role detection tests
- [ ] `app/mailsync-rs/src/imap/sync_worker.rs` — background_sync with all unit test stubs
- [ ] `app/mailsync-rs/src/imap/mail_processor.rs` — Fetch → Message conversion + stable ID with test stubs
- [ ] `app/mailsync-rs/src/oauth2.rs` — TokenManager with unit test stubs
- [ ] Phase 7 Cargo.toml additions: `async-imap`, `imap-proto`, `mail-parser`, `ammonia`, `oauth2`, `reqwest`, `sha2`, `bs58`, `rfc2047-decoder`, `chrono`, `base64`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Full E2E sync against live IMAP server | ISYN-01..07 | Requires real IMAP credentials | Connect test account, verify folders enumerate and messages appear in UI |
| OAuth2 token refresh with real Google API | OAUT-01..03 | Requires real OAuth credentials | Wait for token expiry, verify transparent refresh |
| Gmail-specific folder display in Electron sidebar | GMAL-01..04 | Requires real Gmail account | Connect Gmail, verify only INBOX/All Mail/Trash/Spam shown |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
