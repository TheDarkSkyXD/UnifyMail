# Phase 2: IMAP Connection Testing - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Implement `testIMAPConnection` in Rust using async-imap + tokio-rustls. Handles all three TLS paths (direct TLS on port 993, STARTTLS upgrade, clear/unencrypted), both authentication methods (password and XOAUTH2 SASL), detects 7 IMAP capabilities (idle, condstore, qresync, compress, namespace, xoauth2, gmail), and enforces a 15-second connection timeout. The wrapper module switches from C++ to Rust for this function at the end of the phase.

</domain>

<decisions>
## Implementation Decisions

### Error message style
- Categorized errors: return `errorType` field alongside `error` string — values: `connection_refused`, `timeout`, `tls_error`, `auth_failed`, `unknown`
- Error messages should include server hostname/port for context (e.g., "Connection to imap.gmail.com:993 timed out")
- Distinguish password vs OAuth2 auth failures — enables smarter onboarding UI recovery prompts
- Capabilities returned on success only — match C++ behavior, no partial results on auth failure

### TLS certificate handling
- **Strict certificate validation only** — no `allowInsecureSsl` parameter in Phase 2 (deferred to IMPR-02)
- Use `rustls-platform-verifier` for OS certificate store integration (Windows Certificate Store, macOS Keychain, Linux ca-certificates) — already decided in Phase 1
- **Hostname verification enabled** — rustls default, verify server hostname matches cert SANs. More secure than C++ OpenSSL behavior
- Include specific TLS error details when available (e.g., "certificate expired", "self-signed", "hostname mismatch") — helps enterprise sysadmins diagnose without Wireshark

### Wrapper switchover timing
- **Switch testIMAPConnection from C++ to Rust at end of Phase 2** — as soon as Rust implementation passes tests
- **No fallback to C++** — fail loudly if Rust addon fails. Matches Phase 1 decision: goal is C++ elimination, not coexistence
- Debug mode routing logs (`MAILCORE_DEBUG=1` logs "testIMAPConnection -> Rust") — consistent with Phase 1 debug logging pattern
- **Stricter TypeScript types** — follow Phase 1 approach: narrow unions where possible (connectionType: 'tls' | 'starttls' | 'clear'), maintain runtime API compatibility

### Testing approach
- **Mock IMAP server in Rust tests** — tokio TcpListener responding to IMAP protocol. Tests all 3 TLS paths, auth methods, capabilities. Runs offline, fast, deterministic
- **Full failure simulation** — mock server simulates: auth rejection, TLS handshake failure, timeout (delayed response), mid-connection drop, invalid greeting
- **Parallel tests** — each test creates its own mock server on a random port with its own connection. No shared state, no TEST_MUTEX needed for IMAP tests
- **Validate XOAUTH2 SASL format** — mock server parses the SASL token and verifies format matches RFC ('user=...\x01auth=Bearer ...\x01\x01'). Catches encoding bugs in Authenticator implementation

### Claude's Discretion
- Exact mock IMAP server implementation approach (inline vs extracted helper)
- Whether to extend Electron integration test (`test/electron-integration-test.js`) for testIMAPConnection
- Internal code organization within `imap.rs` (connection builders, auth strategies, capability parser)
- Debug log verbosity (TLS version, cipher suite, server greeting in debug mode)
- Error message wording for each failure type
- New Cargo.toml dependency versions (async-imap, tokio-rustls, rustls-platform-verifier)

</decisions>

<specifics>
## Specific Ideas

- STARTTLS stream upgrade is the known risk from STATE.md — deltachat-core-rust patterns should be consulted (already in research)
- Capabilities detection should map the same 7 flags as C++: idle, condstore, qresync, compress, namespace, xoauth2, gmail
- The 15-second timeout wraps the entire connection+auth+capability flow via `tokio::time::timeout`, not individual steps
- Research (02-RESEARCH.md) has detailed patterns for all three TLS paths, XOAUTH2 Authenticator trait, and capability mapping

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailcore-rs/src/lib.rs`: Module entry point — add `pub mod imap;` for the new module
- `app/mailcore-rs/Cargo.toml`: Already has tokio with `net`/`time`/`io-util` features; needs async-imap, tokio-rustls, rustls, rustls-platform-verifier additions
- `app/mailcore/types/index.d.ts`: Defines `IMAPConnectionResult { success, error?, capabilities? }` — target return shape (will extend with `errorType?`)
- `app/mailcore/src/napi/napi_imap.cpp`: C++ reference implementation — exact connection flow, capability parsing, error handling

### Established Patterns
- Module-per-function layout: `provider.rs` exists, `imap.rs` follows the same pattern
- napi async functions: Phase 1 established the pattern with `#[napi]` async fn returning `Result<T>`
- Debug logging via `MAILCORE_DEBUG=1` environment variable
- `#![deny(unsafe_code)]` — no unsafe Rust, napi macros handle the FFI boundary
- Exact dependency pinning in Cargo.toml (e.g., `"=1.0.228"`)
- Integration tests in `tests/` directory (not inline `#[cfg(test)]`)

### Integration Points
- `app/mailcore-wrapper/index.js` line 35: Currently routes `testIMAPConnection` to C++ — will change to route to Rust
- `app/mailcore-rs/index.js`: Rust addon entry point — must export `testIMAPConnection`
- `app/internal_packages/onboarding/lib/onboarding-helpers.ts`: Ultimate consumer via the wrapper module

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 02-imap-connection-testing*
*Context gathered: 2026-03-03*
