---
phase: 07-imap-background-sync-worker
plan: 01
subsystem: imap
tags: [async-imap, imap-proto, tokio-rustls, rustls-platform-verifier, mail-parser, ammonia, oauth2, reqwest, sha2, bs58, rfc2047-decoder, chrono, base64, tokio-stream, rust, cargo]

# Dependency graph
requires:
  - phase: 06-mail-store-models
    provides: SyncError enum with all 20 variants — classification methods extend this type
  - phase: 05-core-infrastructure-ipc
    provides: main.rs mod declarations pattern, tokio runtime setup
provides:
  - Phase 7 crate dependencies compiled (async-imap, imap-proto, tokio-rustls, rustls-platform-verifier, mail-parser, ammonia, oauth2, reqwest, sha2, bs58, rfc2047-decoder, chrono, base64, tokio-stream)
  - imap/mod.rs, imap/session.rs, imap/sync_worker.rs, imap/mail_processor.rs module scaffold
  - oauth2.rs module stub with TokenManager placeholder
  - SyncError::is_retryable(), is_offline(), is_auth(), is_fatal() classification methods
  - From<async_imap::error::Error> conversion for SyncError
  - ROLE_ORDER, MODSEQ_TRUNCATION_THRESHOLD, BODY_CACHE_AGE_SECS, BODY_PREFETCH_AGE_SECS, BODY_SYNC_BATCH_SIZE constants
affects:
  - 07-02 through 07-05 (all Phase 7 plans depend on this foundation)
  - Any plan using SyncError for error handling decisions

# Tech tracking
tech-stack:
  added:
    - async-imap 0.11 (tokio runtime feature) — IMAP client library
    - imap-proto 0.16 — IMAP protocol parser
    - tokio-rustls 0.26 — async TLS for IMAP connections
    - rustls-platform-verifier 0.6 — platform-native TLS certificate verification
    - mail-parser 0.11 — RFC822/MIME message parsing
    - ammonia 4 — HTML sanitization for email bodies
    - oauth2 5 (reqwest feature) — OAuth2 PKCE flow
    - reqwest 0.13 (rustls-native-certs, json features) — HTTP client for OAuth2 token exchange
    - sha2 0.10 — SHA-256 for message ID derivation
    - bs58 0.5 — base58 encoding for message IDs
    - rfc2047-decoder 1 — MIME encoded-word header decoding
    - chrono 0.4 (std feature) — date/time for message timestamps
    - base64 0.22 — XOAUTH2 SASL string encoding
    - tokio-stream 0.1 — async stream adapters for IMAP fetch responses
  patterns:
    - "Stub modules with empty test blocks: stub structs exist with #[allow(dead_code)], empty #[cfg(test)] mod tests {} blocks, and plan reference comments"
    - "SyncError classification via matches! macro: boolean methods on enum use matches!() for concise pattern matching"
    - "From<ExternalError> conversions on SyncError: semantic mapping (No -> Authentication, Io -> Connection, etc.) rather than string wrapping"
    - "MODSEQ_TRUNCATION_THRESHOLD = 4000: full sync cheaper than delta processing above this UID count"

key-files:
  created:
    - app/mailsync-rs/src/imap/mod.rs
    - app/mailsync-rs/src/imap/session.rs
    - app/mailsync-rs/src/imap/sync_worker.rs
    - app/mailsync-rs/src/imap/mail_processor.rs
    - app/mailsync-rs/src/oauth2.rs
  modified:
    - app/mailsync-rs/Cargo.toml (14 Phase 7 deps added)
    - app/mailsync-rs/src/error.rs (4 classification methods + From<async_imap::error::Error>)
    - app/mailsync-rs/src/main.rs (mod imap + mod oauth2 declarations)

key-decisions:
  - "async-imap::error::Error has no Tls variant — TLS errors surface as Io(IoError); mapped to SyncError::Connection rather than SslHandshakeFailed"
  - "reqwest uses rustls-native-certs feature (not rustls-tls) — uses platform certificate store, consistent with rustls-platform-verifier approach"
  - "From<async_imap::error::Error>::No maps to Authentication — IMAP NO response from LOGIN/AUTHENTICATE commands indicates credential rejection"

patterns-established:
  - "Stub pattern: #[allow(dead_code)] struct/fn with empty test module and plan-reference comments"
  - "Error classification: pub fn is_X(&self) -> bool using matches! macro"

requirements-completed: [IMPR-06]

# Metrics
duration: 15min
completed: 2026-03-04
---

# Phase 7 Plan 01: IMAP Module Scaffold and SyncError Classification Summary

**Phase 7 compilation foundation: async-imap ecosystem deps, IMAP/OAuth2 module stubs, and SyncError classification methods (is_retryable/is_offline/is_auth/is_fatal) enabling all parallel Phase 7 plans**

## Performance

- **Duration:** ~15 min
- **Started:** 2026-03-04T18:10:00Z
- **Completed:** 2026-03-04T18:25:00Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- All 14 Phase 7 Cargo.toml dependencies compile cleanly (async-imap, imap-proto, tokio-rustls, rustls-platform-verifier, mail-parser, ammonia, oauth2, reqwest, sha2, bs58, rfc2047-decoder, chrono, base64, tokio-stream)
- imap/ module scaffold with session.rs (ImapSession stub), sync_worker.rs (background_sync/sort_folders stubs + 5 constants), and mail_processor.rs (id_for_message/process_fetched_message stubs)
- oauth2.rs stub with TokenManager placeholder and empty test module
- SyncError extended with 4 classification methods used by every sync algorithm in Phases 7-9
- From<async_imap::error::Error> conversion maps semantic IMAP responses to SyncError variants
- 149 total tests pass (13 error classification tests + 136 pre-existing)

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Phase 7 Cargo.toml dependencies and create imap/oauth2 module scaffold** - `c11716f` (feat)
2. **Task 2: Extend SyncError with classification methods and From conversions for IMAP errors** - `b488a9a` (feat)

## Files Created/Modified

- `app/mailsync-rs/Cargo.toml` - Added 14 Phase 7 dependencies under `# Phase 7: IMAP sync worker` comment
- `app/mailsync-rs/src/main.rs` - Added `mod imap;` and `mod oauth2;` declarations
- `app/mailsync-rs/src/imap/mod.rs` - Module declarations: session, sync_worker, mail_processor
- `app/mailsync-rs/src/imap/session.rs` - ImapSession stub struct (Plans 03/05 placeholder)
- `app/mailsync-rs/src/imap/sync_worker.rs` - background_sync/sort_folders stubs + ROLE_ORDER, MODSEQ_TRUNCATION_THRESHOLD, BODY_CACHE_AGE_SECS, BODY_PREFETCH_AGE_SECS, BODY_SYNC_BATCH_SIZE constants
- `app/mailsync-rs/src/imap/mail_processor.rs` - id_for_message/process_fetched_message stubs
- `app/mailsync-rs/src/oauth2.rs` - TokenManager stub struct (Plan 05 placeholder)
- `app/mailsync-rs/src/error.rs` - is_retryable(), is_offline(), is_auth(), is_fatal() methods + From<async_imap::error::Error>

## Decisions Made

- async-imap::error::Error has no `Tls` variant. TLS errors surface via `Io(IoError)` wrapping the underlying rustls error. Mapped to `SyncError::Connection` since from the sync worker's perspective, a TLS failure is a connection failure (connection established but couldn't negotiate security). The plan suggested `SslHandshakeFailed` but the enum variant doesn't exist in async-imap 0.11.
- reqwest already uses `rustls-native-certs` (platform certificate store) rather than the plan's `rustls-tls` — this is consistent with `rustls-platform-verifier` for TLS verification. No change made.
- `From<async_imap::error::Error>::No` maps to `Authentication` because IMAP NO responses to LOGIN/AUTHENTICATE commands are the server rejecting credentials.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed async-imap::error::Error::Tls variant not found**
- **Found during:** Task 2 (From<async_imap::error::Error> implementation)
- **Issue:** Plan specified `async_imap::error::Error::Tls(_) => SyncError::SslHandshakeFailed` but async-imap 0.11 has no `Tls` variant — TLS errors are wrapped inside `Io(IoError)`.
- **Fix:** Replaced `Tls(_)` match arm with correct variants: `Io(_) => Connection`, `ConnectionLost => Connection`. TLS errors surface through `Io` variant.
- **Files modified:** app/mailsync-rs/src/error.rs
- **Verification:** cargo test passes; all 13 error tests pass
- **Committed in:** b488a9a (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug — incorrect variant name from plan)
**Impact on plan:** Fix required for compilation. Semantic mapping preserved: TLS/connection errors still classify as connection failures. No scope creep.

## Issues Encountered

The scaffold files (imap/, oauth2.rs, mod declarations in main.rs) were pre-created and untracked in the working directory before execution. Verified each file against plan specification — all matched. Committed as Task 1.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All Phase 7 plans (07-02 through 07-05) can now build without Cargo.toml conflicts
- imap/ module scaffold provides type stubs for Plan 02 (folder discovery), Plan 03 (IMAP connection), Plan 04 (mail processing), Plan 05 (OAuth2 SASL)
- SyncError classification methods available for retry/backoff logic in all sync algorithm plans
- `cargo check` and `cargo test` both pass with 149 tests

## Self-Check: PASSED

- FOUND: app/mailsync-rs/src/imap/mod.rs
- FOUND: app/mailsync-rs/src/imap/session.rs
- FOUND: app/mailsync-rs/src/imap/sync_worker.rs
- FOUND: app/mailsync-rs/src/imap/mail_processor.rs
- FOUND: app/mailsync-rs/src/oauth2.rs
- FOUND: c11716f (Task 1 commit)
- FOUND: b488a9a (Task 2 commit)
- cargo test: 149 passed, 0 failed

---
*Phase: 07-imap-background-sync-worker*
*Completed: 2026-03-04*
