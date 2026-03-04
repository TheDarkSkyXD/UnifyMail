# Phase 3: SMTP Testing and Account Validation - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Implement `testSMTPConnection` and `validateAccount` in Rust, completing the 5-function API surface with full parity to the C++ addon. `testSMTPConnection` handles TLS/STARTTLS/clear connections with password and XOAUTH2 auth. `validateAccount` runs IMAP test, SMTP test, and MX DNS resolution concurrently via `tokio::join!()`, returning separate sub-results for each protocol. MX-regex provider matching (deferred from Phase 1) is implemented here inside validateAccount. The wrapper module switches both functions from C++ to Rust at the end of the phase.

</domain>

<decisions>
## Implementation Decisions

### Partial failure reporting
- validateAccount returns **separate `imapResult` and `smtpResult` sub-objects** — the onboarding UI can show which part failed specifically
- `success: true` requires **both IMAP and SMTP to pass** — no partial success
- When both fail, **IMAP error takes priority** at the top-level `error` and `errorType` fields (IMAP is the gatekeeper — if it fails, SMTP failure is usually secondary)
- On success, **IMAP capabilities are included** in `imapResult` — no need for a separate testIMAPConnection call during onboarding
- Result shape:
  ```js
  {
    success: boolean,
    error?: string,           // prefixed with "IMAP: " or "SMTP: "
    errorType?: string,       // propagated from failing sub-test
    identifier?: string,      // from MX matching or null
    imapResult: {
      success: boolean,
      error?: string,
      errorType?: string,
      capabilities?: string[]  // present on imapResult.success
    },
    smtpResult: {
      success: boolean,
      error?: string,
      errorType?: string
    },
    imapServer: { hostname: string, port: number },
    smtpServer: { hostname: string, port: number }
  }
  ```

### MX matching location
- MX-regex matching lives **inside validateAccount only** — providerForEmail stays sync (no breaking change)
- MX resolution **fails silently** — if DNS times out or fails, skip MX matching and continue validation. Identifier may be null but tests still run
- MX resolution runs **concurrently with IMAP+SMTP tests** via `tokio::join!()` — total time = max(MX, IMAP, SMTP)
- **Single 15-second timeout** wraps the entire `tokio::join!()` — validateAccount always resolves within 15s

### SMTP test depth
- **Connect + Auth + NOOP** — use lettre's natural SmtpTransport flow (EHLO + auth + NOOP). Goes slightly beyond C++ (which stops after auth) but actually verifies session health
- **Connect-only mode** when no credentials provided — just verify server accepts connections and responds to EHLO. Matches C++ `loginIfNeeded` skip behavior
- Return shape: `{ success, error?, errorType? }` — no EHLO extensions, no server info
- **Same 15-second timeout** as IMAP (tokio::time::timeout wrapping entire flow)

### Error type extension
- SMTP uses the **same errorType set as IMAP**: `connection_refused`, `timeout`, `tls_error`, `auth_failed`, `unknown`
- validateAccount's top-level errorType is **propagated from the failing sub-test** (IMAP priority when both fail)
- Error messages **include protocol prefix** at top level: "IMAP: Connection to imap.gmail.com:993 timed out" / "SMTP: Authentication failed for smtp.gmail.com:587"
- Sub-result error messages omit the prefix (the sub-object already indicates protocol)
- **No `dns_error` type** — MX resolution fails silently, never causes validateAccount to fail

### Claude's Discretion
- lettre version and exact SmtpTransport configuration
- DNS resolver choice (trust-dns-resolver vs hickory-resolver vs std::net)
- Mock SMTP server implementation for tests (inline vs extracted helper)
- Whether to extend Electron integration test for testSMTPConnection/validateAccount
- Internal code organization within `smtp.rs` and `validate.rs`
- Debug log verbosity for SMTP and validation flows
- Error message wording for each SMTP failure type
- XOAUTH2 SASL implementation details for lettre
- MX resolution timeout (sub-timeout within the 15s overall)

</decisions>

<specifics>
## Specific Ideas

- validateAccount result shape is an intentional improvement over C++ — separate sub-results give the onboarding UI better error context than a single success/error
- NOOP after auth is "free" with lettre and proves the session is actually healthy — a worthwhile improvement over C++
- MX matching is the last piece of provider detection deferred from Phase 1 — completing it here means providerForEmail + validateAccount together cover 100% of providers.json patterns
- The 15-second timeout wrapping tokio::join!() means the worst case is 15s regardless of how many concurrent operations are running

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailcore-rs/src/imap.rs`: Phase 2 IMAP implementation — `testIMAPConnection` is called internally by validateAccount. Reuse the connection/auth/capability logic directly
- `app/mailcore-rs/src/provider.rs`: Provider database with domain-match patterns. MX-match patterns are already parsed from providers.json but not yet used — ready for MX matching in validateAccount
- `app/mailcore/src/napi/napi_smtp.cpp`: C++ reference for SMTP test flow — connect, auth, disconnect. Rust implementation goes beyond with NOOP
- `app/mailcore/src/napi/napi_validator.cpp`: C++ reference for validateAccount — busy-wait polling loop, hardcoded TLS. Rust improves with tokio::join!() concurrency and configurable connection types
- `app/mailcore/types/index.d.ts`: TypeScript interfaces — AccountValidationResult and SMTPConnectionResult will be extended with errorType and sub-results

### Established Patterns
- Module-per-function layout: `provider.rs`, `imap.rs` exist — add `smtp.rs` and `validate.rs`
- napi async functions: `#[napi]` async fn returning `Result<T>` (Phase 1+2 pattern)
- Categorized errors: `errorType` field alongside `error` string (Phase 2 pattern)
- Debug logging: `MAILCORE_DEBUG=1` environment variable (Phase 1 pattern)
- Exact dependency pinning in Cargo.toml (Phase 1 pattern)
- Mock server testing with parallel random-port tests (Phase 2 pattern)
- Strict TLS via rustls-platform-verifier (Phase 1+2 pattern)

### Integration Points
- `app/mailcore-wrapper/index.js`: Currently routes `testSMTPConnection` and `validateAccount` to C++ — will switch to Rust at end of phase
- `app/mailcore-rs/index.js`: Rust addon entry point — must export `testSMTPConnection` and `validateAccount`
- `app/frontend/mailsync-process.ts` line 436: `test()` method calls `napi.validateAccount()` — primary consumer
- `app/internal_packages/onboarding/lib/onboarding-helpers.ts`: `finalizeAndValidateAccount()` triggers validation via MailsyncProcess.test()

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 03-smtp-testing-and-account-validation*
*Context gathered: 2026-03-03*
