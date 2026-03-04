---
phase: 07-imap-background-sync-worker
plan: 02
subsystem: oauth2
tags: [oauth2, token-manager, xoauth2, imap-auth, delta-stream, rust, reqwest]

# Dependency graph
requires:
  - phase: 07-01
    provides: oauth2.rs stub, TokenManager placeholder, DeltaStreamItem::new() factory
  - phase: 06-mail-store-models
    provides: DeltaStream/DeltaStreamItem infrastructure, SyncError enum
provides:
  - TokenManager with get_valid_token, refresh_token_with_retry, build_xoauth2_string
  - DeltaStreamItem::account_secrets_updated() factory method
  - ProcessAccountSecretsUpdated delta emission on refresh token rotation
affects:
  - 07-03 (ImapSession.authenticate() consumes TokenManager.get_valid_token())
  - 07-04 (background_sync loop will call TokenManager before IMAP authenticate)
  - Electron frontend (ProcessAccountSecretsUpdated delta updates stored credentials)

# Tech tracking
tech-stack:
  added:
    - reqwest 0.13 form feature for .form() method on HTTP POST requests
    - Windows MSYS2 MinGW toolchain workarounds in .cargo/config.toml
  patterns:
    - "Arc<tokio::sync::Mutex<TokenManager>> for safe concurrent refresh (documented in struct doc)"
    - "Token cache key = account.id (not email) — accounts can change email"
    - "Exponential backoff: REFRESH_BACKOFF_BASE_SECS * 3^attempt (5s, 15s, 45s)"
    - "DeltaStreamItem factory methods: process_state(), account_secrets_updated()"
    - "Rotation detection: compare response.refresh_token vs account.extra[refreshToken]"

key-files:
  created: []
  modified:
    - app/mailsync-rs/src/oauth2.rs (TokenManager full implementation + 18 tests)
    - app/mailsync-rs/src/delta/item.rs (account_secrets_updated factory method)
    - app/mailsync-rs/Cargo.toml (reqwest form feature added)
    - app/Cargo.lock (updated dependency lockfile)
    - app/mailsync-rs/src/imap/mail_processor.rs (fix imap_proto types, Rule 3 auto-fix)
    - app/.cargo/config.toml (Windows MSYS2 toolchain config, nanosleep64 stub, gcc paths)

key-decisions:
  - "reqwest form feature added explicitly — .form() method requires the 'form' cargo feature in reqwest 0.13"
  - "emit_secrets_updated() delegates to DeltaStreamItem::account_secrets_updated() — single source of truth for delta shape"
  - "Token endpoint priority: settings.imap_oauth_token_url > provider default (gmail/outlook) > extra.tokenEndpoint"
  - "Windows GNU toolchain: use powershell with PATH=C:\\msys64\\mingw64\\bin prepended to run cargo test (dlltool/gcc needed)"

patterns-established:
  - "Test DeltaStream via mpsc::unbounded_channel: make_test_delta_stream() helper returns (DeltaStream, Receiver)"
  - "make_account() helper: serde_json merge of base fields + extra_json for test accounts"
  - "make_manager_with_cache() helper: pre-seeded CachedToken for expiry buffer tests"

requirements-completed: [OAUT-01, OAUT-02, OAUT-03]

# Metrics
duration: 45min
completed: 2026-03-04
---

# Phase 7 Plan 02: OAuth2 TokenManager Summary

**OAuth2 TokenManager with 300s expiry buffer, 3-retry exponential backoff (5s/15s/45s), XOAUTH2 SASL builder, and ProcessAccountSecretsUpdated delta emission on token rotation — 18 tests pass**

## Performance

- **Duration:** ~45 min
- **Started:** 2026-03-04T18:45:00Z
- **Completed:** 2026-03-04T19:30:00Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Full `TokenManager` implementation in `oauth2.rs` (was a 19-line stub):
  - `get_valid_token()`: checks 300s expiry buffer, returns cached or refreshes
  - `refresh_token_with_retry()`: 3 attempts with 5s/15s/45s backoff
  - `refresh_token()`: HTTP POST with grant_type=refresh_token, parses error field
  - `build_xoauth2_string()`: Base64("user=\x01auth=Bearer \x01\x01")
  - `emit_secrets_updated()`: delegates to `DeltaStreamItem::account_secrets_updated()`
- `DeltaStreamItem::account_secrets_updated()` factory method in `delta/item.rs`
- 18 unit tests covering: expiry buffer (5 tests), XOAUTH2 encoding (2), endpoint resolution (3), retry constants (2), token caching (2), token response parsing (2), secrets rotation (3)
- Windows MSYS2 toolchain workarounds (Rule 3): `.cargo/config.toml` with gcc/ar paths, nanosleep64 stub linkage, getrandom windows_legacy backend

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement TokenManager with expiry check, cache, retry-with-backoff refresh, and XOAUTH2 SASL builder** - `643a296` (feat)
2. **Task 2: Add account_secrets_updated delta factory and secrets rotation tests** - `006ff84` (feat)

## Files Created/Modified

- `app/mailsync-rs/src/oauth2.rs` — TokenManager full implementation, 18 unit tests
- `app/mailsync-rs/src/delta/item.rs` — account_secrets_updated() factory method
- `app/mailsync-rs/Cargo.toml` — reqwest form feature added
- `app/Cargo.lock` — updated lockfile
- `app/mailsync-rs/src/imap/mail_processor.rs` — fixed imap_proto type imports (Rule 3)
- `app/.cargo/config.toml` — Windows MSYS2 MinGW toolchain configuration

## Decisions Made

- `reqwest 0.13` requires the `form` cargo feature for `.form()` on `RequestBuilder`. Added explicitly since the plan didn't include it but the implementation requires it.
- `emit_secrets_updated()` uses `DeltaStreamItem::account_secrets_updated()` factory method (Task 2) rather than inline `serde_json::json!()` — this ensures the delta shape is defined in one place.
- Token endpoint priority order: `settings.imap_oauth_token_url` (explicit override) > provider-based default (gmail/outlook) > `extra.tokenEndpoint` (fallback).
- Windows MSYS2 toolchain: `cargo test` requires `C:\msys64\mingw64\bin` on PATH for `dlltool.exe`, `gcc.exe`, `ar.exe` (needed by aws-lc-sys, ring, stacker build scripts). Documented in `.cargo/config.toml`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed mail_processor.rs compilation errors blocking test builds**
- **Found during:** Task 1 (attempting cargo test)
- **Issue:** `imap/mail_processor.rs` used `async_imap::types::Envelope` and `async_imap::types::Address` which don't exist in async-imap 0.11. Also `fetch.parsed()` doesn't exist; gmail_msg_id type inference issues.
- **Fix:** Changed to `imap_proto::Envelope`, `imap_proto::Address`. The linter (cargo fmt + clippy hook) auto-fixed most issues during the session.
- **Files modified:** `app/mailsync-rs/src/imap/mail_processor.rs`
- **Commit:** `643a296` (included in Task 1 commit)

**2. [Rule 3 - Blocking] Added reqwest form feature missing from plan**
- **Found during:** Task 1 (`.form()` method not found on RequestBuilder)
- **Issue:** Plan specified using reqwest for HTTP token refresh but didn't include the `form` Cargo feature required for `.form()` method in reqwest 0.13.
- **Fix:** Added `"form"` to reqwest features in Cargo.toml.
- **Files modified:** `app/mailsync-rs/Cargo.toml`
- **Commit:** `643a296` (Task 1 commit)

**3. [Rule 3 - Environment] Added Windows MSYS2 MinGW toolchain workarounds**
- **Found during:** Task 1 (cargo test link failures)
- **Issue:** `aws-lc-sys` (via rustls-platform-verifier) has a `nanosleep64` undefined reference on Windows MSYS2 MinGW. `getrandom 0.3` uses `raw-dylib` requiring `dlltool.exe`. Build scripts need `gcc.exe` from MSYS2 toolchain.
- **Fix:** Created `.cargo/config.toml` with: explicit gcc/ar paths to MSYS2, nanosleep64 stub archive linkage, getrandom windows_legacy backend cfg flag. Tests run via PowerShell with `C:\msys64\mingw64\bin` prepended to PATH.
- **Files modified:** `app/.cargo/config.toml`
- **Commit:** `643a296` (Task 1 commit)

### Out-of-scope (logged, not fixed)

The linter continuously modified files during the session (session.rs underwent multiple revisions from the linter). The linter's session.rs changes are tracked in 07-03-SUMMARY.md (Plan 03's scope).

## Issues Encountered

**Windows MSYS2 toolchain environment:** The `cargo check` passed at session start (cached build artifacts from previous session). After `cargo test` invalidated build artifacts, `cargo check` began failing because build scripts for `cmake`, `ring`, `stacker` couldn't find `gcc.exe` and `dlltool.exe` (not in the Git-for-Windows bash PATH, only in the MSYS2 path at `C:\msys64\mingw64\bin`). Resolution: run `cargo test` via PowerShell with PATH prepended.

## User Setup Required

To run `cargo test` on Windows with this toolchain:
```powershell
$env:PATH = "C:\msys64\mingw64\bin;" + $env:PATH
cd app\mailsync-rs
cargo test "oauth2::tests"
```

## Next Phase Readiness

- `TokenManager` is ready for consumption by `ImapSession.authenticate()` in Plan 03
- `DeltaStreamItem::account_secrets_updated()` is available for any future plan needing to emit secrets updates
- All 18 oauth2 tests pass; all 6 delta::item tests pass

## Self-Check: PASSED

- FOUND: app/mailsync-rs/src/oauth2.rs
- FOUND: app/mailsync-rs/src/delta/item.rs (account_secrets_updated method)
- FOUND: 643a296 (Task 1 commit)
- FOUND: 006ff84 (Task 2 commit)
- cargo test oauth2::tests: 18 passed, 0 failed
- cargo test delta::item::tests: 6 passed, 0 failed
- cargo check: Finished dev profile with 0 errors (34 warnings, all dead_code/unused)

---
*Phase: 07-imap-background-sync-worker*
*Completed: 2026-03-04*
