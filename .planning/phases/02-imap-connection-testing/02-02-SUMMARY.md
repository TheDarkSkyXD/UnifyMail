---
phase: 02-imap-connection-testing
plan: 02
subsystem: testing
tags: [rust, async-imap, imap, tokio, mock-server, napi-rs, xoauth2, tls]

# Dependency graph
requires:
  - phase: 02-01
    provides: "do_test_imap function, IMAPConnectionOptions/IMAPConnectionResult structs, connect_clear/connect_tls/connect_starttls builders, XOAUTH2 Authenticator"
provides:
  - "Mock IMAP server test suite (12 tests) covering all connection paths, auth methods, capabilities, error scenarios"
  - "testIMAPConnection routed to Rust in wrapper module — Rust IMAP implementation is now live in production"
  - "Bug fix: IMAP greeting consumed after Client::new in connect_clear and connect_tls — prevents XOAUTH2 deadlock"
affects: [03-smtp-connection-testing, phase-3-integration, wrapper-routing]

# Tech tracking
tech-stack:
  added: [base64 (in test file for SASL format validation)]
  patterns:
    - "Mock IMAP server pattern: TcpListener::bind(127.0.0.1:0) for random port, per-test server, MockAuthMode enum for behavior"
    - "classify_error_str helper in tests mirrors imap.rs classify_error for error type assertion"
    - "Tokio timeout wrapper in tests (3s) simulates 15s production timeout for fast test execution"
    - "Greeting consumption: read_response() after Client::new before authenticate/login"

key-files:
  created:
    - app/mailcore-rs/tests/imap_tests.rs
  modified:
    - app/mailcore-rs/src/imap.rs
    - app/mailcore-wrapper/index.js
    - app/mailcore-rs/loader.js

key-decisions:
  - "Read IMAP greeting after Client::new in connect_clear and connect_tls — async-imap docs require explicit greeting consumption before authenticate; do_auth_handshake (XOAUTH2) processes each response in order and would misroute the greeting as a challenge, causing a deadlock"
  - "Tests call do_test_imap (pub internal function) not test_imap_connection (napi) — napi functions require a JS runtime; tests use tokio::time::timeout to simulate the 15s production timeout"
  - "For TLS tests: TLS against a plain TCP mock correctly fails with tls_error — positive TLS path validated against real servers in Phase 3 integration"
  - "classify_error_str helper duplicated in test file — avoids making classify_error pub in imap.rs while keeping tests self-contained"
  - "loader.js updated to export testIMAPConnection — Phase 1 loader only exported Phase 1 functions; each phase adds its exports"

patterns-established:
  - "Mock server per test: each test binds TcpListener::bind(127.0.0.1:0) for a random port — no shared state, no TEST_MUTEX needed (unlike provider_tests)"
  - "MockAuthMode enum: encodes server behavior as enum variant, passed to start_mock_imap_server"
  - "Error classification in tests: local classify_error_str mirrors production logic for assertions"

requirements-completed: [IMAP-01, IMAP-02, IMAP-03, IMAP-04, IMAP-05, IMAP-06]

# Metrics
duration: 14min
completed: 2026-03-03
---

# Phase 2 Plan 02: IMAP Connection Testing Summary

**12 mock IMAP server tests (clear/XOAUTH2/capabilities/errors) + Rust testIMAPConnection live via wrapper switchover + greeting bug fix**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-03T23:04:09Z
- **Completed:** 2026-03-03T23:18:21Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Created comprehensive mock IMAP server test suite (12 tests) with no live server required
- Fixed critical XOAUTH2 deadlock bug: `do_auth_handshake` misrouted the server greeting as the first SASL challenge, hanging indefinitely — fixed by reading the greeting after `Client::new`
- Switched `testIMAPConnection` in the wrapper module from C++ to Rust — implementation is now live

## Task Commits

Each task was committed atomically:

1. **Task 1: Mock IMAP server test suite** - `398a395` (test)
2. **Task 2: Switch wrapper module from C++ to Rust** - `27fe877` (feat)

**Plan metadata:** (docs commit — see final metadata commit)

## Files Created/Modified

- `app/mailcore-rs/tests/imap_tests.rs` — 12 mock IMAP server integration tests covering all paths, auth modes, capabilities, and error scenarios
- `app/mailcore-rs/src/imap.rs` — Made `do_test_imap` pub; read greeting after `Client::new` in `connect_clear` and `connect_tls`
- `app/mailcore-wrapper/index.js` — Routes `testIMAPConnection` to `getRust()` with debug logging; `testSMTPConnection`/`validateAccount` remain on C++
- `app/mailcore-rs/loader.js` — Added `testIMAPConnection` export (Phase 1 loader only had Phase 1 exports)

## Decisions Made

- **Greeting consumption bug fix:** The `async-imap` documentation explicitly requires reading the server greeting after `Client::new` before calling `login` or `authenticate`. The Plan 01 implementation omitted this. For `login`, it worked accidentally because `check_done_ok` discards non-Done responses. For `authenticate`/XOAUTH2, `do_auth_handshake` processes each response sequentially — it read the greeting as the first "Continue" challenge, then waited for the challenge the mock server never re-sent, causing a deadlock.

- **Tests call `do_test_imap` not `test_imap_connection`:** The `#[napi]` function requires a JS runtime. Tests call the `pub` internal function directly and use `tokio::time::timeout` to simulate the 15-second production timeout.

- **TLS positive test deferred to Phase 3:** A TLS connection to a plain TCP mock server correctly fails with `tls_error` (rustls-platform-verifier rejects untrusted certs). Testing successful TLS connections requires real servers with valid certificates — covered in Phase 3 integration tests.

- **loader.js updated:** The Phase 1 custom loader only exported Phase 1 functions. Each phase must add its functions to the loader exports.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed IMAP greeting not consumed after Client::new**

- **Found during:** Task 1 (Mock IMAP server test suite) — XOAUTH2 tests timed out
- **Issue:** `connect_clear` and `connect_tls` called `Client::new(stream)` without reading the server greeting. `do_auth_handshake` (XOAUTH2) read the greeting as the first response, expected it to be a `Continue` challenge. It was not Continue, so it fell through to `check_done_ok_from` which looped waiting for a tagged Done response. The mock server was simultaneously waiting for the SASL token the client never sent. Both sides hung.
- **Fix:** Added `client.read_response().await?` after `Client::new` in both `connect_clear` and `connect_tls`. STARTTLS is correct as-is — the greeting is consumed by `run_command_and_check_ok("STARTTLS")` and there is no greeting after the TLS upgrade.
- **Files modified:** `app/mailcore-rs/src/imap.rs`
- **Verification:** All 12 XOAUTH2 and other tests pass; 16 provider tests still pass; clippy clean
- **Committed in:** `398a395` (Task 1 commit)

**2. [Rule 2 - Missing Critical] Added testIMAPConnection export to loader.js**

- **Found during:** Task 2 (wrapper switchover verification)
- **Issue:** `app/mailcore-rs/loader.js` only exported `providerForEmail` and `registerProviders` (the Phase 1 functions). Without `testIMAPConnection` in the loader exports, `getRust().testIMAPConnection` would be `undefined`, silently failing at the wrapper level.
- **Fix:** Added `module.exports.testIMAPConnection = nativeBinding.testIMAPConnection;` to `loader.js`.
- **Files modified:** `app/mailcore-rs/loader.js`
- **Verification:** `node -e "typeof require('./app/mailcore-wrapper').testIMAPConnection"` returns `"function"`
- **Committed in:** `27fe877` (Task 2 commit)

---

**Total deviations:** 2 auto-fixed (1 bug, 1 missing critical export)
**Impact on plan:** Both fixes necessary for correct behavior. No scope creep.

## Issues Encountered

- **XOAUTH2 deadlock:** See deviation 1 above. Root cause was undocumented async-imap requirement to read greeting explicitly. Discovered via test timeout analysis and async-imap source inspection.

## User Setup Required

None — no external service configuration required. Tests run offline with mock servers.

## Next Phase Readiness

- Phase 2 complete: `testIMAPConnection` implemented in Rust and live via wrapper
- Phase 3 (SMTP connection testing) can use same wrapper patterns
- TLS positive path needs real server validation in Phase 3 integration tests
- The greeting bug fix applies to all connection types — ensure Phase 3 SMTP implementation reads SMTP greeting (220) before sending commands

## Self-Check: PASSED

All created files exist on disk. All task commits verified in git history.

| Check | Result |
|-------|--------|
| app/mailcore-rs/tests/imap_tests.rs | FOUND |
| app/mailcore-rs/src/imap.rs | FOUND |
| app/mailcore-wrapper/index.js | FOUND |
| app/mailcore-rs/loader.js | FOUND |
| .planning/phases/02-imap-connection-testing/02-02-SUMMARY.md | FOUND |
| Commit 398a395 (Task 1) | VERIFIED |
| Commit 27fe877 (Task 2) | VERIFIED |

---
*Phase: 02-imap-connection-testing*
*Completed: 2026-03-03*
