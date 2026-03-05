---
phase: 08-foreground-idle-and-task-execution
plan: 02
subsystem: smtp
tags: [lettre, smtp, tls, starttls, xoauth2, mime, multipart, email-send, rust]

# Dependency graph
requires:
  - phase: 08-01
    provides: TaskKind enum, execute_task framework, lettre dependency added to Cargo.toml
  - phase: 07-imap-background-sync-worker
    provides: Account struct with extra serde_json::Value for settings, SyncError variants, oauth2 TokenManager
provides:
  - SmtpSender struct with build_transport() (TLS/STARTTLS/clear) and send_message() with 30s timeout
  - build_draft_email() MIME builder for all 5 content variants (plain, HTML+plain, inline CIDs, attachments, full)
  - DraftData/ContactField/FileAttachment structs for draft JSON deserialization
  - get_raw_message() for IMAP APPEND to Sent folder
  - parse_draft_data() for extracting DraftData from task JSON
affects: [08-04-send-draft-task-execution, plan-04-task-execution-wiring]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "SmtpSender is stateless — transport instances built fresh per send via build_transport()"
    - "lettre builder_dangerous() for transport; TLS validation still performed by rustls during handshake"
    - "Outer tokio::time::timeout(30s) wraps transport.send() for SEND-04 compliance"
    - "MIME nesting selected dynamically based on content: plain -> alternative -> related -> mixed"
    - "Inline images identified by content_id: Option<String> on FileAttachment — Some = inline CID, None = regular attachment"
    - "camelCase serde with #[serde(rename_all = camelCase)] on DraftData to match Electron JSON keys"

key-files:
  created:
    - app/mailsync-rs/src/smtp/mod.rs
    - app/mailsync-rs/src/smtp/sender.rs
    - app/mailsync-rs/src/smtp/mime_builder.rs
  modified:
    - app/mailsync-rs/src/main.rs

key-decisions:
  - "SmtpSender is stateless — no persistent transport held; build_transport() creates fresh instance per send to avoid connection state issues"
  - "Tls::Wrapper for SSL (port 465), Tls::Required for STARTTLS (port 587), Tls::None for cleartext — matches lettre 0.11 API exactly as verified in research"
  - "Error detection for auth failure uses string matching on '535' and 'authentication' since lettre SmtpResult doesn't expose structured response codes as enum"
  - "html_to_plain() is a simple tag-stripper — sufficient for generating plain fallback; no external HTML parser dependency needed"
  - "File bytes read via std::fs::read() (synchronous) — files are local temp files from Electron, path is trusted, no async file I/O needed for this use case"

patterns-established:
  - "MIME builder pattern: separate files into inline vs regular before building, then select structure from 5 cases"
  - "ContactField parse_mailbox() helper centralizes lettre Mailbox parsing with name+email formatting"

requirements-completed: [SEND-01, SEND-02, SEND-03, SEND-04]

# Metrics
duration: 25min
completed: 2026-03-04
---

# Phase 08 Plan 02: SMTP Sender and MIME Builder Summary

**lettre SMTP transport with TLS/STARTTLS/clear + XOAUTH2/password auth, 30s send timeout, and 5-variant MIME builder (plain/HTML+plain/inline-CID/attachment/full-nesting) for SendDraftTask**

## Performance

- **Duration:** 25 min
- **Started:** 2026-03-04T16:00:00Z
- **Completed:** 2026-03-04T16:25:00Z
- **Tasks:** 2
- **Files modified:** 4 (3 created, 1 modified)

## Accomplishments

- SmtpSender builds lettre AsyncSmtpTransport for all 3 TLS modes (SSL/Tls::Wrapper, STARTTLS/Tls::Required, cleartext/Tls::None) with both Mechanism::Plain and Mechanism::Xoauth2 auth
- send_message() wraps transport.send() in 30-second tokio::time::timeout outer guard (SEND-04), with error mapping to SyncError::Timeout/Authentication/Connection
- build_draft_email() dynamically selects MIME structure for all 5 content variants including correct multipart nesting: mixed(alternative(plain + related(html + inline CIDs)) + attachments)
- 22 unit tests across sender (15) and mime_builder (7) all passing, covering all TLS modes, both auth mechanisms, all 5 MIME variants, multiple recipients, and reply headers

## Task Commits

Each task was committed atomically:

1. **Task 1: SMTP sender with TLS/STARTTLS/clear and password/XOAUTH2** - `dedbfe2` (feat)
2. **Task 2: MIME message builder from draft JSON** - `614f96a` (feat)

## Files Created/Modified

- `app/mailsync-rs/src/smtp/mod.rs` - Module declarations for sender and mime_builder submodules
- `app/mailsync-rs/src/smtp/sender.rs` - SmtpSender struct, build_transport() for TLS modes, send_message() with 30s timeout, get_raw_message() for IMAP APPEND, 15 unit tests
- `app/mailsync-rs/src/smtp/mime_builder.rs` - DraftData/ContactField/FileAttachment structs, build_draft_email() with 5-variant MIME selection, parse_draft_data(), 7 unit tests
- `app/mailsync-rs/src/main.rs` - Added `pub mod smtp` to module declarations

## Decisions Made

- **SmtpSender is stateless**: Transport instances created fresh per send via `build_transport()`. No persistent connection held between sends, avoids connection state and idle timeout issues.
- **Auth error detection via string matching**: lettre's SmtpResult doesn't expose response codes as enum variants in 0.11. Auth failures detected by checking for "535" or "authentication" in error string — sufficient for UI error categorization.
- **html_to_plain() simple tag-stripper**: Plain fallback for HTML body uses a minimal tag-stripping function rather than adding an HTML parser dependency. Adequate for generating RFC-compliant alternative text.
- **Synchronous file I/O for attachments**: std::fs::read() used for file attachments — files are local temp paths created by Electron, access is trusted and fast (local disk), no async overhead needed.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed `SyncError::Connection(format!(...))` — Connection variant has no payload**
- **Found during:** Task 1 compilation
- **Issue:** Plan template used `SyncError::Connection(...)` with a string payload, but the existing Connection variant in error.rs takes no arguments (unit variant)
- **Fix:** Changed to `SyncError::Unexpected(format!("TLS params error: {e}"))` for TLS parameter construction failures
- **Files modified:** app/mailsync-rs/src/smtp/sender.rs
- **Verification:** cargo check passes
- **Committed in:** dedbfe2 (Task 1 commit)

**2. [Rule 1 - Bug] Fixed temporary value lifetime in test assertions**
- **Found during:** Task 2 test compilation
- **Issue:** `let raw = String::from_utf8_lossy(&msg.formatted())` — `msg.formatted()` creates a temporary Vec<u8> that is freed before the borrow in `raw` is used
- **Fix:** Bound formatted bytes to named variable first: `let raw_bytes = msg.formatted(); let raw = String::from_utf8_lossy(&raw_bytes);`
- **Files modified:** app/mailsync-rs/src/smtp/mime_builder.rs (7 test locations)
- **Verification:** cargo test smtp::mime_builder passes
- **Committed in:** 614f96a (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (2 Rule 1 bugs)
**Impact on plan:** Both auto-fixes were compile errors caught immediately. No scope creep.

## Issues Encountered

None beyond the two auto-fixed compilation errors documented above.

## Next Phase Readiness

- smtp/sender.rs and smtp/mime_builder.rs are ready for Plan 04 to wire into `execute_remote()` for the SendDraftTask variant
- get_raw_message() provides the raw RFC 2822 bytes needed for Plan 04's IMAP APPEND to Sent folder
- All 22 SMTP tests pass; 298 total unit tests pass

---
*Phase: 08-foreground-idle-and-task-execution*
*Completed: 2026-03-04*
