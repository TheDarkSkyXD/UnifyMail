---
phase: 03-smtp-testing-and-account-validation
plan: 02
subsystem: testing
tags: [rust, validate, napi-rs, tokio, hickory-resolver, mx-dns, concurrent, tdd]

# Dependency graph
requires:
  - phase: 03-smtp-testing-and-account-validation
    plan: 01
    provides: "do_test_smtp, SMTPConnectionOptions, SMTPConnectionResult, hickory-resolver dep"
  - phase: 02-imap-connection-testing
    provides: "do_test_imap, IMAPConnectionOptions, IMAPConnectionResult"
provides:
  - "validate.rs with validateAccount napi export (tokio::join!() concurrency, 15s timeout)"
  - "do_validate pub function callable from integration tests without napi runtime"
  - "resolve_mx_identifier: fail-silent MX lookup with hickory-resolver + provider pattern matching"
  - "All 5 mailcore-napi functions routed to Rust (no more C++ routing)"
  - "loader.js exporting testSMTPConnection and validateAccount"
affects:
  - wrapper/index.js (C++ routing removed entirely — getCpp() deleted)

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "do_validate pub fn exposes validate logic for integration tests without napi runtime — same pattern as do_test_imap/do_test_smtp"
    - "tokio::join!() in do_validate runs IMAP + SMTP + MX concurrently; total time = max, not sum"
    - "resolve_mx_identifier fail-silent: all errors return None, never fail the outer validateAccount"
    - "assemble_result converts InternalResult<T> to sub-result objects; IMAP priority on both-fail"
    - "pub(crate) on Provider struct fields and PROVIDERS static enables cross-module access from validate.rs"
    - "IMAPSubResult/SMTPSubResult/ServerInfo are separate napi(object) types despite mirroring existing structs"

key-files:
  created:
    - app/mailcore-rs/src/validate.rs
  modified:
    - app/mailcore-rs/src/lib.rs
    - app/mailcore-rs/src/provider.rs
    - app/mailcore-rs/tests/smtp_tests.rs
    - app/mailcore-rs/loader.js
    - app/mailcore-wrapper/index.js

key-decisions:
  - "pub(crate) on Provider.identifier, Provider.mx_match_patterns, and PROVIDERS static — minimum visibility to enable validate.rs MX matching without exposing internals to consumers"
  - "IMAPSubResult/SMTPSubResult as separate napi(object) types rather than reusing IMAPConnectionResult/SMTPConnectionResult — napi-rs requires unique type names per export; reusing would cause duplicate registration errors"
  - "do_validate exposes validate logic as pub fn without napi annotation — enables integration tests to call it directly without a JS runtime; same pattern established by do_test_imap and do_test_smtp"
  - "smtp_res passed directly to assemble_result without .map_err(|e| e) — do_test_smtp returns InternalResult<SMTPConnectionResult> which is already the correct type (clippy caught identity map)"

requirements-completed: [VALD-01, VALD-02, VALD-03, VALD-04]

# Metrics
duration: 5min
completed: 2026-03-04
---

# Phase 3 Plan 02: validateAccount and Wrapper Switchover Summary

**validate.rs with tokio::join!() concurrent IMAP+SMTP+MX testing, fail-silent MX resolution, and full Rust routing for all 5 mailcore-napi functions**

## Performance

- **Duration:** 5 min
- **Started:** 2026-03-04T00:16:47Z
- **Completed:** 2026-03-04T00:22:28Z
- **Tasks:** 2 (Task 1: TDD validate.rs; Task 2: loader + wrapper switchover)
- **Files modified:** 6

## Accomplishments

- validateAccount napi export with 15-second outer timeout; Promise always resolves
- do_validate pub function for integration tests; runs IMAP + SMTP + MX via tokio::join!()
- resolve_mx_identifier: hickory-resolver with 5s sub-timeout, anchored case-insensitive regex matching against provider.mx_match_patterns, fail-silent (all errors -> None)
- assemble_result: IMAP error takes priority at top level when both fail; "IMAP: "/"SMTP: " prefix at top level only; sub-results have no prefix
- AccountValidationResult with imapResult (includes capabilities), smtpResult, imapServer, smtpServer, identifier
- 7 validation tests in smtp_tests.rs: both-succeed, imap-fails, smtp-fails, both-fail-imap-priority, result-shape, imap-capabilities, concurrent-timing
- loader.js: added testSMTPConnection and validateAccount exports
- wrapper/index.js: both functions routed to Rust; getCpp() removed; all 5 functions now via Rust

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement validateAccount in validate.rs** - `413b6f0` (feat)
2. **Task 2: Switch wrapper module to Rust and update loader exports** - `80aa0b8` (feat)

## Files Created/Modified

- `app/mailcore-rs/src/validate.rs` - Full validateAccount implementation: IMAPSubResult, SMTPSubResult, ServerInfo, AccountValidationResult, ValidateAccountOptions, resolve_mx_identifier, build_imap_opts, build_smtp_opts, assemble_result, do_validate (pub), validate_account (napi)
- `app/mailcore-rs/src/lib.rs` - Added `pub mod validate;`
- `app/mailcore-rs/src/provider.rs` - Made Provider struct and fields pub(crate); PROVIDERS static pub(crate)
- `app/mailcore-rs/tests/smtp_tests.rs` - 7 validation tests with mock IMAP + mock SMTP servers
- `app/mailcore-rs/loader.js` - Added testSMTPConnection and validateAccount exports
- `app/mailcore-wrapper/index.js` - Full Rust routing for all 5 functions; getCpp() removed

## Decisions Made

- **pub(crate) visibility on Provider**: The minimum visibility needed to read identifier and mx_match_patterns from validate.rs without exposing internals through the public API.
- **IMAPSubResult and SMTPSubResult as separate types**: napi-rs registers types by name; reusing IMAPConnectionResult would cause duplicate registration conflicts. Separate types also have semantic clarity (sub-result vs. standalone result).
- **do_validate pub fn pattern**: Integration tests cannot call napi async functions without a JS runtime. Exposing the core logic as a plain async fn (same pattern as do_test_imap/do_test_smtp) enables direct test coverage.
- **smtp_res passed directly (clippy map_identity fix)**: do_test_smtp returns `InternalResult<SMTPConnectionResult>` which is exactly what assemble_result expects; the original `.map_err(|e| e)` was an identity no-op caught by clippy.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Duplicate imports in smtp_tests.rs validation section**
- **Found during:** Task 1 (cargo compile error E0252)
- **Issue:** The validation section added `use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader}` and `use tokio::net::TcpListener` which were already imported at the top of the file
- **Fix:** Removed duplicate imports from the validation section (already available via top-level imports)
- **Files modified:** app/mailcore-rs/tests/smtp_tests.rs
- **Verification:** cargo test compiles and all 44 tests pass

**2. [Rule 1 - Bug] Clippy: `Iterator::last` on DoubleEndedIterator**
- **Found during:** Task 1 (cargo clippy -D warnings)
- **Issue:** `opts.email.split('@').last()` triggers `double_ended_iterator_last` lint (needlessly iterates entire iterator)
- **Fix:** Changed to `opts.email.split('@').next_back()` as suggested by clippy
- **Files modified:** app/mailcore-rs/src/validate.rs
- **Verification:** cargo clippy clean

**3. [Rule 1 - Bug] Clippy: unnecessary identity map_err**
- **Found during:** Task 1 (cargo clippy -D warnings)
- **Issue:** `smtp_res.map_err(|e| e)` is an identity map (no-op); clippy `map_identity` lint
- **Fix:** Removed `.map_err(|e| e)` — smtp_res is already InternalResult<SMTPConnectionResult>
- **Files modified:** app/mailcore-rs/src/validate.rs
- **Verification:** cargo clippy clean

---

**Total deviations:** 3 auto-fixed (1 compile error — duplicate imports; 2 clippy lints)
**Impact on plan:** All fixes necessary for correctness and clean linting. No scope creep.

## Issues Encountered

None beyond the deviations documented above.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- All 5 mailcore-napi functions route to Rust: providerForEmail, registerProviders, testIMAPConnection, testSMTPConnection, validateAccount
- The C++ addon (mailcore/) is no longer used by the wrapper module
- Phase 4 (packaging and integration testing) can now verify the Rust addon end-to-end via the standard npm build pipeline

## Self-Check: PASSED

- FOUND: app/mailcore-rs/src/validate.rs
- FOUND: app/mailcore-rs/loader.js (contains testSMTPConnection and validateAccount)
- FOUND: app/mailcore-wrapper/index.js (routes to Rust, no getCpp)
- FOUND: 413b6f0 (feat: validate.rs)
- FOUND: 80aa0b8 (feat: loader + wrapper)
- All 44 tests pass (12 IMAP + 16 provider + 16 SMTP/validate)
- cargo clippy -D warnings: clean
- cargo fmt --check: clean
- No OpenSSL in cargo tree
- wrapper exports: providerForEmail, registerProviders, testIMAPConnection, testSMTPConnection, validateAccount

---
*Phase: 03-smtp-testing-and-account-validation*
*Completed: 2026-03-04*
