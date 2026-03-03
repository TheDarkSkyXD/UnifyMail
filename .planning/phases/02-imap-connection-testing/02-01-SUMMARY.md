---
phase: 02-imap-connection-testing
plan: 01
subsystem: infra
tags: [rust, napi-rs, async-imap, tokio-rustls, rustls-platform-verifier, imap, tls, starttls, xoauth2]

# Dependency graph
requires:
  - phase: 01-scaffolding-and-provider-detection
    provides: napi-rs crate scaffold, lib.rs module root, Cargo.toml base, established napi patterns
provides:
  - testIMAPConnection napi async function in app/mailcore-rs/src/imap.rs
  - IMAPConnectionOptions and IMAPConnectionResult napi(object) structs
  - Three TLS connection paths (direct TLS, STARTTLS upgrade, clear/unencrypted)
  - XOAuth2 Authenticator implementation for XOAUTH2 SASL
  - Post-login detection of 7 IMAP capabilities (idle, condstore, qresync, compress, namespace, xoauth2, gmail)
  - 15-second timeout via tokio::time::timeout
  - Error classification into connection_refused/tls_error/auth_failed/timeout/unknown errorType
  - rustls-platform-verifier OS trust store integration (no OpenSSL)
affects:
  - 02-02 (wrapper switchover: route testIMAPConnection from C++ to Rust)
  - future-phases (SMTP connection testing follows same pattern)

# Tech tracking
tech-stack:
  added:
    - async-imap = "0.11" (runtime-tokio feature, default-features=false — prevents async-std conflict)
    - tokio-rustls = "0.26"
    - rustls = "0.23"
    - rustls-platform-verifier = "0.6"
    - base64 = "0.22"
  patterns:
    - InternalResult<T> = std::result::Result<T, BoxError> for non-napi functions; napi::Result only at napi boundary
    - XOAuth2 Authenticator trait: process() returns SASL string format user=<email>\x01auth=Bearer <token>\x01\x01
    - auth_and_capabilities<S> generic over AsyncRead+AsyncWrite+Unpin+Send+Debug — unified auth for TLS and clear streams
    - classify_error() pattern: check message chain for known error patterns, return categorized string
    - make_server_name() pattern: parse IpAddr first, use ServerName::IpAddress for IPs, ServerName::DnsName for hostnames
    - default-features = false required on async-imap 0.11 (default is runtime-async-std, conflicts with runtime-tokio)
    - with_platform_verifier() returns Result<ClientConfig, rustls::Error> — must propagate with ?

key-files:
  created:
    - app/mailcore-rs/src/imap.rs
  modified:
    - app/mailcore-rs/Cargo.toml
    - app/mailcore-rs/Cargo.lock
    - app/mailcore-rs/src/lib.rs

key-decisions:
  - "async-imap 0.11 requires default-features = false — its default feature is runtime-async-std which conflicts with runtime-tokio; both enabled causes E0252 duplicate imports and compile_error!()"
  - "InternalResult<T> = std::result::Result<T, BoxError> for all internal async fns — napi::Result requires AsRef<str> on error type which BoxError does not implement"
  - "rustls-platform-verifier ConfigVerifierExt::with_platform_verifier() returns Result<ClientConfig, rustls::Error>, not ClientConfig — must ? propagate"
  - "STARTTLS: after upgrade, Client::new on TLS stream is safe — async_imap Client::new does not auto-read greeting; greeting was consumed on the initial plain TCP Client in Step 2"

patterns-established:
  - "InternalResult<T> pattern: use std::result::Result<T, BoxError> for internal functions, napi::Result only at napi export boundary"
  - "Stream-generic auth pattern: auth_and_capabilities<S: AsyncRead+AsyncWrite+Unpin+Send+Debug> unifies all connection types"
  - "classify_error pattern: string-based error message inspection for user-facing errorType classification"
  - "default-features = false on async-imap prevents async-std runtime conflict in napi-rs tokio environment"

requirements-completed: [IMAP-01, IMAP-02, IMAP-03, IMAP-04, IMAP-05, IMAP-06]

# Metrics
duration: 14min
completed: 2026-03-03
---

# Phase 02 Plan 01: IMAP Connection Testing Summary

**testIMAPConnection implemented in Rust with three TLS paths (direct/STARTTLS/clear), XOAuth2 SASL auth, 7-capability post-login detection, 15s timeout, and categorized errorType field — rustls-platform-verifier only, no OpenSSL**

## Performance

- **Duration:** 14 min
- **Started:** 2026-03-03T22:45:47Z
- **Completed:** 2026-03-03T23:00:11Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Rust `imap.rs` module implements `testIMAPConnection` as a napi async export, handling all three IMAP TLS connection modes with a unified generic authentication flow
- XOAUTH2 SASL Authenticator trait correctly formats `user=<email>\x01auth=Bearer <token>\x01\x01` (SOH byte separators, not null bytes)
- Post-login capability detection maps 7 server capabilities (IDLE, CONDSTORE, QRESYNC, COMPRESS=DEFLATE, NAMESPACE, AUTH=XOAUTH2, X-GM-EXT-1) to lowercase API strings
- rustls-platform-verifier provides OS-native certificate validation (Windows Certificate Store, macOS Keychain, Linux ca-certificates) — zero OpenSSL symbols in dependency tree
- All verification passes: cargo check, clippy -D warnings, cargo fmt --check, no OpenSSL in cargo tree

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Phase 2 dependencies and wire imap module** - `13fb8a4` (chore)
2. **Task 2: Implement testIMAPConnection in imap.rs** - `853714d` (feat)

## Files Created/Modified

- `app/mailcore-rs/src/imap.rs` - Full testIMAPConnection implementation (417 lines): connection builders, XOAuth2 Authenticator, capability detection, error classification, 15s timeout wrapper
- `app/mailcore-rs/src/lib.rs` - Added `pub mod imap;` declaration
- `app/mailcore-rs/Cargo.toml` - Added Phase 2 deps: async-imap, tokio-rustls, rustls, rustls-platform-verifier, base64
- `app/mailcore-rs/Cargo.lock` - Updated lock file

## Decisions Made

- `async-imap 0.11` requires `default-features = false` alongside `features = ["runtime-tokio"]`. The crate's default feature is `runtime-async-std`; without `default-features = false`, cargo enables both runtimes simultaneously, causing E0252 (duplicate imports) and a compile_error!(). The plan's research notes warned about runtime conflict but did not document the `default-features = false` fix.
- Internal functions use `InternalResult<T> = std::result::Result<T, BoxError>`. The napi `Result<T>` type requires `AsRef<str>` on the error type, which `Box<dyn Error + Send + Sync>` does not implement. Conversion to napi::Error happens only at the top-level napi export boundary.
- `ClientConfig::with_platform_verifier()` from `rustls-platform-verifier 0.6` returns `Result<ClientConfig, rustls::Error>`, not `ClientConfig` directly. The plan showed it as infallible; `?` propagation is required.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] async-imap default-features conflict**
- **Found during:** Task 1 (dependency addition)
- **Issue:** `async-imap = { version = "0.11", features = ["runtime-tokio"] }` as written in the plan enables both `runtime-async-std` (default) and `runtime-tokio` simultaneously, producing compile_error!() and 9 E0252 errors about duplicate imports (Read, Write, timeout)
- **Fix:** Added `default-features = false` to the async-imap dependency entry
- **Files modified:** app/mailcore-rs/Cargo.toml
- **Verification:** cargo check passed without runtime conflict errors
- **Committed in:** 13fb8a4 (Task 1 commit)

**2. [Rule 1 - Bug] with_platform_verifier returns Result, not ClientConfig**
- **Found during:** Task 2 (make_tls_config implementation)
- **Issue:** Plan showed `ClientConfig::with_platform_verifier()` as returning `ClientConfig` directly; actual API returns `Result<ClientConfig, rustls::Error>` in rustls-platform-verifier 0.6
- **Fix:** Changed `Ok(config)` to `Ok(config?)` to propagate the Result
- **Files modified:** app/mailcore-rs/src/imap.rs
- **Verification:** cargo check passed with no type mismatch errors
- **Committed in:** 853714d (Task 2 commit)

**3. [Rule 1 - Bug] napi::Result type incompatible with BoxError in internal functions**
- **Found during:** Task 2 (initial compile attempt)
- **Issue:** Using `Result<T, BoxError>` as return type while importing `napi::Result` caused confusion — `?` operator tried to use napi::Result which requires `AsRef<str>` on error; BoxError doesn't satisfy this
- **Fix:** Introduced `type InternalResult<T> = std::result::Result<T, BoxError>` for all internal functions; only `test_imap_connection` uses `napi::Result`
- **Files modified:** app/mailcore-rs/src/imap.rs
- **Verification:** cargo check and clippy -D warnings both passed with zero errors/warnings
- **Committed in:** 853714d (Task 2 commit)

---

**Total deviations:** 3 auto-fixed (3 bugs in plan-provided code examples)
**Impact on plan:** All auto-fixes were required for compilation correctness. No scope creep. Core functionality as specified (connection paths, auth methods, capabilities, timeout, error classification) implemented exactly as planned.

## Issues Encountered

- **MSYS2 dlltool.exe not in PATH:** The Claude Code bash environment did not have `C:\msys64\mingw64\bin` in PATH, causing the initial `cargo check` to fail with "error calling dlltool 'dlltool.exe': program not found". Resolved by adding `/c/msys64/mingw64/bin` to PATH for all cargo commands.
- **libnode.dll missing from /tmp:** The libnode.dll import library documented in Phase 1 (stored at `/tmp/libnode.dll`) was absent in this session. Regenerated using `gendef node.exe && dlltool --dllname node.exe --def node.def --output-lib /c/msys64/tmp/libnode.dll`. Required for all cargo commands on Windows GNU toolchain.

## Next Phase Readiness

- `testIMAPConnection` Rust implementation is complete and compiles clean
- Plan 02-02 should route `testIMAPConnection` from C++ to Rust in `app/mailcore-wrapper/index.js`
- The Rust addon's `index.js` and `index.d.ts` may need updating to export `testIMAPConnection` and `IMAPConnectionResult` types

## Self-Check: PASSED

All artifacts verified post-execution:
- FOUND: app/mailcore-rs/src/imap.rs
- FOUND: app/mailcore-rs/Cargo.toml (with async-imap runtime-tokio)
- FOUND: app/mailcore-rs/src/lib.rs (with pub mod imap)
- FOUND: 02-01-SUMMARY.md
- FOUND: commit 13fb8a4 (Task 1)
- FOUND: commit 853714d (Task 2)
- PASS: cargo check
- PASS: cargo clippy -- -D warnings
- PASS: cargo fmt --check
- PASS: No OpenSSL in dependency tree

---
*Phase: 02-imap-connection-testing*
*Completed: 2026-03-03*
