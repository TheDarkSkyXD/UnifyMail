# Phase 3: SMTP Testing and Account Validation - Research

**Researched:** 2026-03-03
**Domain:** lettre SMTP transport, hickory-resolver MX DNS, tokio::join!() concurrency, napi-rs async function export
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- validateAccount returns **separate `imapResult` and `smtpResult` sub-objects** — the onboarding UI can show which part failed specifically
- `success: true` requires **both IMAP and SMTP to pass** — no partial success
- When both fail, **IMAP error takes priority** at the top-level `error` and `errorType` fields (IMAP is the gatekeeper)
- On success, **IMAP capabilities are included** in `imapResult` — no need for a separate testIMAPConnection call during onboarding
- Result shape (exact):
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
- MX-regex matching lives **inside validateAccount only** — providerForEmail stays sync (no breaking change)
- MX resolution **fails silently** — if DNS times out or fails, skip MX matching and continue validation. Identifier may be null but tests still run
- MX resolution runs **concurrently with IMAP+SMTP tests** via `tokio::join!()` — total time = max(MX, IMAP, SMTP)
- **Single 15-second timeout** wraps the entire `tokio::join!()` — validateAccount always resolves within 15s
- **Connect + Auth + NOOP** for SMTP — use lettre's natural SmtpTransport flow (EHLO + auth + NOOP). Goes slightly beyond C++ (which stops after auth) but actually verifies session health
- **Connect-only mode** when no credentials provided — just verify server accepts connections and responds to EHLO
- Return shape: `{ success, error?, errorType? }` — no EHLO extensions, no server info
- **Same 15-second timeout** as IMAP (tokio::time::timeout wrapping entire flow)
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

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SMTP-01 | User can test SMTP connection with TLS | `AsyncSmtpTransport::<Tokio1Executor>::relay(hostname)?` uses `Tls::Wrapper` with `TlsParameters::new_rustls()`; port override via `.port(u16)` builder method |
| SMTP-02 | User can test SMTP connection with STARTTLS | `AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(hostname)?` uses `Tls::Required` with `TlsParameters::new_rustls()`; port override via `.port(u16)` |
| SMTP-03 | User can test SMTP connection with clear/unencrypted | `AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(hostname).port(port).build()`; no TLS parameters |
| SMTP-04 | User can authenticate with password or OAuth2 (XOAUTH2 SASL) | `Credentials::new(user, pass)` + `.authentication(vec![Mechanism::Login])` for password; `Credentials::new(user, token)` + `.authentication(vec![Mechanism::Xoauth2])` for OAuth2; lettre handles XOAUTH2 encoding internally |
| SMTP-05 | Connection timeout of 15 seconds prevents indefinite hang | `tokio::time::timeout(Duration::from_secs(15), transport.test_connection())` at napi export boundary; lettre `.timeout()` sets per-command timeout only — outer tokio timeout is mandatory |
| VALD-01 | User can validate account with IMAP and SMTP tested concurrently via `tokio::join!()` | `tokio::join!(do_test_imap(&imap_opts), do_test_smtp(&smtp_opts), resolve_mx(&domain))` inside a 15s outer `tokio::time::timeout`; both inner functions are `pub` and callable from validate.rs |
| VALD-02 | Validation resolves MX records via DNS for provider matching | hickory-resolver 0.25.2 `Resolver::builder_tokio()?.build().mx_lookup(domain).await`; MX exchange hostnames matched against `mx_match_patterns` already stored in provider.rs `Provider` structs |
| VALD-03 | Validation returns success/error with server details matching C++ result shape — extended with sub-results | Extended result struct with `imapResult`, `smtpResult` sub-objects, `imapServer`, `smtpServer`, `identifier`; IMAP capabilities in imapResult on success; error prefix logic |
| VALD-04 | TypeScript types auto-generated by napi-rs compile without errors against existing consumer files | `#[napi(object)]` on all result structs; `#[napi(js_name = "testSMTPConnection")]` and `#[napi(js_name = "validateAccount")]` on exports; loader.js must re-export both new functions |
</phase_requirements>

---

## Summary

Phase 3 implements two napi-rs async functions: `testSMTPConnection` in `smtp.rs` and `validateAccount` in `validate.rs`. Both are new modules added to `src/lib.rs` following the structure established by Phase 2's `imap.rs`. At phase completion, `app/mailcore-wrapper/index.js` routes both functions from C++ to Rust, and `app/mailcore-rs/loader.js` is extended to export them.

The central SMTP library is **lettre 0.11.19** with features `smtp-transport`, `tokio1`, `tokio1-rustls`, and `rustls-platform-verifier` (the latter is an optional dep in lettre's Cargo.toml that can be activated as a feature). Lettre provides `AsyncSmtpTransport<Tokio1Executor>` with three builder paths: `relay()` for direct TLS (port 465), `starttls_relay()` for STARTTLS (port 587), and `builder_dangerous()` for clear connections. Crucially, lettre's `test_connection()` method issues an SMTP NOOP command and returns `Result<bool, Error>` — this is the exact test operation needed. Lettre handles XOAUTH2 natively via `Mechanism::Xoauth2`, eliminating the need for a custom SASL Authenticator (unlike async-imap which required one in Phase 2).

For DNS MX resolution, **hickory-resolver 0.25.2** (the successor to trust-dns-resolver, rebranded since v0.24) provides `Resolver::builder_tokio()?.build().mx_lookup(domain).await`. MX hostnames from the resolver are matched against the `mx_match_patterns: Vec<String>` already stored in `provider.rs`'s `Provider` struct — these patterns were parsed in Phase 1 but deferred for matching until this phase. MX resolution runs inside `tokio::join!()` concurrently with the IMAP and SMTP tests and fails silently if it errors.

The result shape for `validateAccount` is an intentional improvement over the C++ output: separate `imapResult` and `smtpResult` sub-objects give the onboarding UI precise failure information. IMAP capabilities are included in `imapResult` on success. The top-level `success` requires both to pass; top-level `error` prefixes with "IMAP: " or "SMTP: " to identify which protocol failed.

**Primary recommendation:** Add lettre 0.11.19 (`default-features = false`, with `tokio1-rustls` feature) and hickory-resolver 0.25.2 to Cargo.toml. Implement `smtp.rs` mirroring the structure of `imap.rs` (same error classification, XOAUTH2 via lettre Mechanism enum, same timeout approach). Implement `validate.rs` calling both inner functions concurrently via `tokio::join!()`. Extend loader.js and wrapper/index.js at plan end.

---

## Standard Stack

### Core (Phase 3 additions to Phase 1+2 Cargo.toml)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `lettre` | 0.11.19 | Async SMTP transport: TLS/STARTTLS/clear, password+XOAUTH2 auth, NOOP test | Only maintained async SMTP library in Rust with XOAUTH2 + rustls support; `test_connection()` issues NOOP without manual message construction; will also be used in Phase 8 for actual sending |
| `hickory-resolver` | 0.25.2 | Async DNS MX record resolution | Successor to trust-dns-resolver (renamed at v0.24); `mx_lookup()` returns typed MX records with priority and exchange hostname; tokio-native via `builder_tokio()` |

### Already in Cargo.toml (used by Phase 3)

| Library | Version | Phase 3 Usage |
|---------|---------|---------------|
| `tokio` | 1.50.0 | `tokio::join!()` for concurrent IMAP+SMTP+MX; `tokio::time::timeout` for 15s outer deadline |
| `rustls-platform-verifier` | 0.6 | SMTP TLS: lettre uses platform verifier when `rustls-platform-verifier` feature is activated in lettre |
| `regex` | 1.12.3 | MX-regex pattern matching: `mx_match_patterns` from provider.rs compiled against MX hostnames in validate.rs |
| `napi` / `napi-derive` | 3.x | `#[napi(object)]` on result structs; `#[napi(js_name = "...")]` on napi exports |
| `base64` | 0.22 | Not needed for SMTP (lettre handles XOAUTH2 encoding); already present from Phase 2 |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| lettre `AsyncSmtpTransport` | Manual tokio TCP + SMTP protocol | Custom SMTP state machine requires handling EHLO, AUTH multi-step, pipelining, multi-line responses; lettre encodes 10+ years of RFC 5321 + 4954 + 2487 compat fixes |
| lettre XOAUTH2 via Mechanism::Xoauth2 | Custom SASL Authenticator (as done in imap.rs for async-imap) | lettre has `Mechanism::Xoauth2` built-in; no custom Authenticator needed — simpler than Phase 2's XOAuth2 struct |
| hickory-resolver | `trust-dns-resolver` | trust-dns-resolver IS the old name of the same codebase; hickory-resolver 0.25 is the current maintained version |
| hickory-resolver | `std::net::lookup_host` / `tokio::net::lookup_host` | These only resolve A/AAAA records; MX requires DNS query type MX |

### Installation

Add to `app/mailcore-rs/Cargo.toml` `[dependencies]`:

```toml
# Phase 3: SMTP connection testing and account validation
lettre = { version = "=0.11.19", default-features = false, features = [
    "smtp-transport",
    "tokio1",
    "tokio1-rustls",
    "rustls-platform-verifier",
] }
hickory-resolver = { version = "=0.25.2", features = ["tokio", "system-config"] }
```

**Critical — `default-features = false` on lettre is mandatory.** lettre's default features include `native-tls`, which introduces OpenSSL symbols that conflict with Electron's BoringSSL. This is the same hard constraint that governs async-imap and tokio-rustls in Phases 1 and 2.

**Feature explanations:**
- `smtp-transport` — enables `AsyncSmtpTransport`; required
- `tokio1` — enables `Tokio1Executor`; required for async
- `tokio1-rustls` — enables TLS via rustls paired with tokio; do NOT use `tokio1-native-tls`
- `rustls-platform-verifier` — activates lettre's built-in rustls-platform-verifier integration (it is an optional dependency in lettre's Cargo.toml, accessible as a feature); enables OS certificate store verification matching the approach in imap.rs
- `system-config` on hickory-resolver — enables `Resolver::builder_tokio()` which reads OS resolver config (`/etc/resolv.conf` on Unix, registry on Windows)

---

## Architecture Patterns

### Recommended Module Layout

```
app/mailcore-rs/src/
├── lib.rs           # existing — add: pub mod smtp; pub mod validate;
├── provider.rs      # Phase 1 — mx_match_patterns: Vec<String> already stored in Provider struct
├── imap.rs          # Phase 2 — do_test_imap() is pub; reused by validate.rs
├── smtp.rs          # Phase 3 NEW — do_test_smtp() (pub), test_smtp_connection napi export
└── validate.rs      # Phase 3 NEW — validate_account napi export, tokio::join!()

app/mailcore-rs/tests/
├── provider_tests.rs  # Phase 1
├── imap_tests.rs      # Phase 2
└── smtp_tests.rs      # Phase 3 NEW — mock SMTP server, mirrors imap_tests.rs pattern
```

### Pattern 1: lettre AsyncSmtpTransport — three connection modes

lettre's `relay()` and `starttls_relay()` factory methods are the simplest path for TLS and STARTTLS. For clear connections, use `builder_dangerous()`. All three can have credentials added via `.credentials()` and `.authentication()`.

```rust
// Source: https://docs.rs/lettre/0.11.19/lettre/transport/smtp/struct.AsyncSmtpTransport.html
use lettre::{
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, Tokio1Executor,
};

// TLS (SMTPS, port 465): relay() defaults to port 465 with Tls::Wrapper
let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(hostname)?
    .port(port as u16)   // override default port when user specifies a non-465 TLS port
    .credentials(creds)
    .authentication(vec![mechanism])
    .build();

// STARTTLS (port 587): starttls_relay() defaults to port 587 with Tls::Required
let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(hostname)?
    .port(port as u16)
    .credentials(creds)
    .authentication(vec![mechanism])
    .build();

// Clear (no TLS): builder_dangerous() has no defaults except port 25
let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(hostname)
    .port(port as u16)
    // Note: only add .credentials()/.authentication() when credentials are provided
    .build();

// Issue NOOP: test_connection() does EHLO -> AUTH (if creds set) -> NOOP -> returns Ok(true)
let connected = transport.test_connection().await?;
```

### Pattern 2: lettre XOAUTH2 and password credentials

```rust
// Source: https://docs.rs/lettre/latest/lettre/transport/smtp/index.html
use lettre::transport::smtp::authentication::{Credentials, Mechanism};

// XOAUTH2: pass the oauth2_token as the "password" argument of Credentials::new
// lettre formats internally: user={username}\x01auth=Bearer {token}\x01\x01
// Explicitly set Mechanism::Xoauth2 — without this lettre tries Plain/Login first
let creds = Credentials::new(username.to_owned(), oauth2_token.to_owned());
let mechanism = Mechanism::Xoauth2;

// Password (LOGIN mechanism — widely supported, including Office 365)
let creds = Credentials::new(username.to_owned(), password.to_owned());
let mechanism = Mechanism::Login;  // or Mechanism::Plain
```

**Key insight:** For XOAUTH2, lettre treats the oauth2_token as the "password" in `Credentials::new`. No custom Authenticator struct is needed (unlike Phase 2's async-imap XOAuth2 struct). The `Mechanism::Xoauth2` enum variant handles encoding internally.

### Pattern 3: lettre test_connection() for NOOP

```rust
// Source: https://docs.rs/lettre/0.11.19/lettre/transport/smtp/struct.AsyncSmtpTransport.html
// test_connection() "tests the connection by using the SMTP NOOP command"
// Returns Result<bool, Error>
// Internal sequence: TCP connect -> TLS handshake (if configured) -> EHLO -> AUTH -> NOOP

let transport = build_transport(...);
let result = transport.test_connection().await;
// Ok(true) = connected and all steps succeeded
// Err(e) = failure at any step (connection, TLS, or auth)
```

**Critical:** `test_connection()` is a method on `AsyncSmtpTransport<Tokio1Executor>` directly, NOT on the `AsyncTransport` trait (which provides `send()`). No message construction is required.

### Pattern 4: tokio::join!() in validate.rs with outer 15s timeout

```rust
// Source: tokio 1.x documentation — tokio::join! macro
use tokio::time::{timeout, Duration};

#[napi(js_name = "validateAccount")]
pub async fn validate_account(opts: ValidateAccountOptions) -> napi::Result<AccountValidationResult> {
    let domain = opts.email.split('@').last().unwrap_or("").to_owned();

    // Build inner options from outer opts
    let imap_opts = build_imap_opts_from_validate(&opts);
    let smtp_opts = build_smtp_opts_from_validate(&opts);

    // Single 15-second timeout wraps the entire concurrent operation
    match timeout(Duration::from_secs(15), async {
        tokio::join!(
            crate::imap::do_test_imap(&imap_opts),   // returns InternalResult<IMAPConnectionResult>
            do_test_smtp(&smtp_opts),                  // returns InternalResult<SMTPConnectionResult>
            resolve_mx_identifier(&domain),            // returns Option<String>; never errors
        )
    }).await {
        Ok((imap_res, smtp_res, mx_id)) => {
            Ok(assemble_validation_result(imap_res, smtp_res, mx_id, &opts))
        }
        Err(_elapsed) => Ok(AccountValidationResult {
            success: false,
            error: Some("Account validation timed out after 15 seconds".to_string()),
            error_type: Some("timeout".to_string()),
            // fill remaining fields with defaults
            ..Default::default()
        }),
    }
}
```

**tokio::join!() semantics:** All three futures run concurrently on the same tokio task (not separate OS threads or spawned tasks). Total wall time = max(IMAP, SMTP, MX_DNS), not their sum. Do NOT use `tokio::spawn()` — it requires `'static` lifetimes which complicates ownership.

### Pattern 5: MX resolution (fail-silent) in validate.rs

```rust
// Source: https://docs.rs/hickory-resolver/0.25.2/hickory_resolver/struct.Resolver.html
use hickory_resolver::Resolver;

/// Resolve MX records for a domain and match against providers.json mx-match patterns.
/// Returns None if DNS fails, no records found, or no provider matches.
async fn resolve_mx_identifier(domain: &str) -> Option<String> {
    let resolver = Resolver::builder_tokio().ok()?.build();

    // Append trailing dot for fully-qualified name (cheaper — avoids search domain appending)
    let fqdn = format!("{domain}.");

    // Optional sub-timeout within the 15s outer timeout (5s recommended)
    use tokio::time::{timeout, Duration};
    let mx_lookup = timeout(
        Duration::from_secs(5),
        resolver.mx_lookup(fqdn.as_str()),
    ).await.ok()?.ok()?;

    // Build list of MX hostnames with trailing dots stripped
    let mx_hosts: Vec<String> = mx_lookup
        .iter()
        .map(|mx| mx.exchange().to_utf8())
        .map(|s| s.trim_end_matches('.').to_lowercase())  // "aspmx.l.google.com." -> "aspmx.l.google.com"
        .collect();

    // Match against providers' mx_match_patterns (already stored in provider.rs PROVIDERS)
    let guard = crate::provider::PROVIDERS.read().ok()?;
    let providers = guard.as_ref()?;

    for provider in providers {
        for pattern in &provider.mx_match_patterns {
            let anchored = format!("^{pattern}$");
            if let Ok(re) = regex::RegexBuilder::new(&anchored)
                .case_insensitive(true).build()
            {
                if mx_hosts.iter().any(|h| re.is_match(h)) {
                    return Some(provider.identifier.clone());
                }
            }
        }
    }
    None
}
```

**DNS trailing dot:** `mx_lookup` returns FQDNs with trailing dots (e.g., `"aspmx.l.google.com."`). Always strip with `trim_end_matches('.')` before regex matching.

### Pattern 6: Extended AccountValidationResult struct (VALD-03)

The Rust struct must produce the exact TypeScript shape specified in CONTEXT.md. napi-rs maps snake_case fields to camelCase in the generated `.d.ts`.

```rust
// Source: CONTEXT.md locked decision + napi-rs #[napi(object)] pattern from Phases 1+2
use napi_derive::napi;

#[napi(object)]
pub struct IMAPSubResult {
    pub success: bool,
    pub error: Option<String>,
    pub error_type: Option<String>,          // -> errorType in TypeScript
    pub capabilities: Option<Vec<String>>,   // present on imapResult.success
}

#[napi(object)]
pub struct SMTPSubResult {
    pub success: bool,
    pub error: Option<String>,
    pub error_type: Option<String>,          // -> errorType in TypeScript
}

#[napi(object)]
pub struct ServerInfo {
    pub hostname: String,
    pub port: u32,
}

#[napi(object)]
pub struct AccountValidationResult {
    pub success: bool,
    pub error: Option<String>,              // "IMAP: <msg>" or "SMTP: <msg>" prefix
    pub error_type: Option<String>,         // propagated from failing sub-test (IMAP priority)
    pub identifier: Option<String>,         // from MX matching or null
    pub imap_result: IMAPSubResult,         // -> imapResult in TypeScript (always present)
    pub smtp_result: SMTPSubResult,         // -> smtpResult in TypeScript (always present)
    pub imap_server: ServerInfo,            // -> imapServer in TypeScript (always present)
    pub smtp_server: ServerInfo,            // -> smtpServer in TypeScript (always present)
}
```

**Note:** `imapResult` and `smtpResult` are always present (not `Option<>`). The C++ result had these as optional objects; the Rust implementation always fills them. This is an improvement over C++.

### Pattern 7: Error prefix logic for top-level error field

```rust
// IMAP takes priority when both fail
let (top_error, top_error_type) = match (imap_res.as_ref(), smtp_res.as_ref()) {
    (Err(e), _) => (
        Some(format!("IMAP: {e}")),
        Some(classify_error(e.as_ref())),
    ),
    (_, Err(e)) => (
        Some(format!("SMTP: {e}")),
        Some(classify_error(e.as_ref())),
    ),
    _ if !imap_success => (
        imap_result.error.as_ref().map(|e| format!("IMAP: {e}")),
        imap_result.error_type.clone(),
    ),
    _ => (None, None),
};
```

### Anti-Patterns to Avoid

- **lettre with default features:** `lettre = "0.11"` activates `native-tls` by default — OpenSSL symbols crash Electron. Always `default-features = false`.
- **`tokio1-rustls-tls` feature name (wrong):** The correct feature is `tokio1-rustls`. The old beta naming `tokio1-rustls-tls` appeared in pre-release discussions but is not the released feature name.
- **Not setting `Mechanism::Xoauth2` explicitly:** Without `.authentication(vec![Mechanism::Xoauth2])`, lettre tries Plain/Login first. Servers supporting only XOAUTH2 reject with auth failures.
- **Using `Tls::Opportunistic` for STARTTLS:** Opportunistic falls back silently to plaintext if STARTTLS is unavailable. Use `Tls::Required` — this fails explicitly if the server does not support STARTTLS, matching C++ behavior.
- **Sequential IMAP + SMTP in validateAccount:** Running tests sequentially takes up to 30s. `tokio::join!()` caps worst-case at 15s.
- **Not stripping trailing dot from MX hostnames:** `mx_lookup` returns FQDNs with trailing dots (`"aspmx.l.google.com."`). Regex patterns in providers.json do not account for trailing dots — strip before matching.
- **Using `trust-dns-resolver` crate name:** Unmaintained; use `hickory-resolver` 0.25.
- **`js_name` casing on napi exports:** Must be `#[napi(js_name = "testSMTPConnection")]` (all-caps SMTP) and `#[napi(js_name = "validateAccount")]`. napi-rs auto-converts `test_smtp_connection` to `testSmtpConnection` (lowercase imap/smtp — wrong).
- **Not updating loader.js:** The Phase 2 loader.js only exports Phase 1+2 functions. wrapper/index.js getRust() calls fail silently if loader.js doesn't re-export `testSMTPConnection` and `validateAccount`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| SMTP EHLO/AUTH/NOOP state machine | Custom tokio TCP write/read loop | `lettre::AsyncSmtpTransport.test_connection()` | SMTP protocol has multi-step state machine; lettre handles RFC 5321 + AUTH RFC 4954 + STARTTLS RFC 2487 including pipelining and multi-line responses |
| XOAUTH2 SASL for SMTP | Custom base64 format string (as done in imap.rs) | `Mechanism::Xoauth2` in lettre | lettre implements XOAUTH2 internally; the imap.rs XOAuth2 struct approach is not needed here |
| DNS MX record queries | `std::net::lookup_host` or raw UDP | `hickory_resolver::Resolver::mx_lookup()` | `lookup_host` only resolves A/AAAA. MX requires query type MX via a proper DNS resolver |
| SMTP TLS certificate validation | Custom rustls ClientConfig wiring | lettre `rustls-platform-verifier` feature | When enabled, lettre uses OS certificate store via rustls-platform-verifier; matches imap.rs approach |

**Key insight:** lettre's `test_connection()` does exactly what SMTP-01 through SMTP-05 require — connect, optionally authenticate, issue NOOP, return result — in a single async method call. There is no reason to decompose the SMTP conversation manually.

---

## Common Pitfalls

### Pitfall 1: lettre default features activate native-tls
**What goes wrong:** `cargo tree | grep openssl` shows openssl-sys. Electron crashes at addon load time with symbol conflicts.
**Why it happens:** lettre's default features include `native-tls` for general use cases. This project has a hard constraint against OpenSSL (established Phase 1).
**How to avoid:** `lettre = { version = "=0.11.19", default-features = false, features = ["smtp-transport", "tokio1", "tokio1-rustls", "rustls-platform-verifier"] }`
**Warning signs:** `cargo tree -i lettre -e features` shows `native-tls` in the feature tree.

### Pitfall 2: lettre timeout() sets per-command timeout, not total operation timeout
**What goes wrong:** Setting `.timeout(Some(Duration::from_secs(15)))` on the builder and assuming the total connect+auth+NOOP sequence completes within 15s. A slow server can accumulate 15s per step (TCP connect, TLS handshake, EHLO, AUTH, NOOP), resulting in 75s total.
**Why it happens:** lettre applies the timeout per individual SMTP command, not per connection lifecycle.
**How to avoid:** Wrap the entire `do_test_smtp()` call in `tokio::time::timeout(Duration::from_secs(15), ...)` at the napi export boundary. This is the same pattern used in Phase 2 for async-imap.
**Warning signs:** Tests hang longer than 15 seconds against a nonexistent SMTP host.

### Pitfall 3: XOAUTH2 Mechanism must be explicitly specified
**What goes wrong:** `Credentials::new(user, token)` without `.authentication(vec![Mechanism::Xoauth2])`. lettre's default mechanism order is `[Plain, Login]`. Server receives PLAIN auth with the OAuth2 token as the "password" — rejected with auth failure.
**Why it happens:** lettre treats XOAUTH2 as opt-in; Credentials alone don't specify mechanism.
**How to avoid:** Always pair XOAUTH2 credentials with `.authentication(vec![Mechanism::Xoauth2])`.
**Warning signs:** `535 5.7.8 Username and Password not accepted` when token is correct.

### Pitfall 4: DNS MX hostnames have trailing dots
**What goes wrong:** `mx_lookup("gmail.com").await` returns `MxRecord` where `exchange()` is `"aspmx.l.google.com."` (trailing dot). Regex pattern `aspmx\\.l\\.google\\.com` does not match because of the dot at the end.
**Why it happens:** DNS FQDNs include trailing dots by convention. hickory-resolver returns raw DNS names.
**How to avoid:** `let host = mx.exchange().to_utf8(); let host = host.trim_end_matches('.');`
**Warning signs:** `identifier` is always `None` even for Gmail/Outlook accounts during integration testing.

### Pitfall 5: hickory-resolver builder_tokio() feature requirements
**What goes wrong:** `Resolver::builder_tokio()` is not found or panics at runtime with "cannot block the current thread from within a runtime."
**Why it happens:** `builder_tokio()` requires the `system-config` feature (for reading OS resolver config) AND the `tokio` feature. With only `tokio` enabled, `builder_tokio()` may not be available or may not read OS config correctly.
**How to avoid:** Use `features = ["tokio", "system-config"]` on hickory-resolver.
**Warning signs:** Compile error "method not found in `ResolverBuilder`" or runtime panic on resolver construction.

### Pitfall 6: SMTP greeting handling contrast with IMAP Phase 2 fix
**What goes wrong:** Expecting to manually read the SMTP 220 greeting after TCP connect when using lettre (by analogy with Phase 2's async-imap greeting bug fix).
**Why it happens:** Phase 2 had a critical bug where async-imap required explicit `read_response()` after `Client::new`. Developers apply the same pattern to SMTP.
**How to avoid:** lettre's `AsyncSmtpTransport` handles the entire SMTP handshake including the 220 greeting internally. Do NOT add manual greeting reads. lettre is a higher-level abstraction than async-imap.
**Warning signs:** Deadlock or "unexpected response" errors when connecting to a live SMTP server.

### Pitfall 7: Not updating loader.js after adding new napi exports
**What goes wrong:** `wrapper/index.js` calls `getRust().testSMTPConnection(opts)` successfully, but the call throws `TypeError: testSMTPConnection is not a function`.
**Why it happens:** `loader.js` only exports functions that are explicitly listed. Phase 2's lesson (documented in STATE.md): "loader.js must export each new function per phase — Phase 1 loader only had Phase 1 exports."
**How to avoid:** Add to `app/mailcore-rs/loader.js`:
```js
module.exports.testSMTPConnection = nativeBinding.testSMTPConnection;
module.exports.validateAccount = nativeBinding.validateAccount;
```
**Warning signs:** `TypeError: getRust().testSMTPConnection is not a function` in console.

---

## Code Examples

Verified patterns from official sources:

### lettre Cargo.toml feature configuration

```toml
# Source: https://docs.rs/crate/lettre/latest/features
# "tokio1-rustls" pairs tokio1 runtime with rustls TLS backend
# "rustls-platform-verifier" activates lettre's optional dep for OS cert store
lettre = { version = "=0.11.19", default-features = false, features = [
    "smtp-transport",          # AsyncSmtpTransport struct
    "tokio1",                  # Tokio1Executor
    "tokio1-rustls",           # TLS via rustls + tokio (NOT tokio1-native-tls)
    "rustls-platform-verifier", # OS certificate store verification
] }
hickory-resolver = { version = "=0.25.2", features = ["tokio", "system-config"] }
```

### AsyncSmtpTransport TLS with XOAUTH2

```rust
// Source: https://docs.rs/lettre/latest/lettre/transport/smtp/index.html
use lettre::{
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, Tokio1Executor,
};

let creds = Credentials::new(
    "user@gmail.com".to_owned(),
    "ya29.oauth_token_here".to_owned(),  // token as "password"
);

let transport = AsyncSmtpTransport::<Tokio1Executor>::relay("smtp.gmail.com")?
    .port(465)
    .credentials(creds)
    .authentication(vec![Mechanism::Xoauth2])
    .build();

// EHLO -> AUTH XOAUTH2 -> NOOP
let ok: bool = transport.test_connection().await?;
```

### AsyncSmtpTransport STARTTLS with password auth

```rust
// Source: https://docs.rs/lettre/0.11.19/lettre/transport/smtp/struct.AsyncSmtpTransport.html
use lettre::{
    transport::smtp::authentication::{Credentials, Mechanism},
    AsyncSmtpTransport, Tokio1Executor,
};

let creds = Credentials::new("user@example.com".to_owned(), "password".to_owned());

let transport = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay("smtp.example.com")?
    .port(587)
    .credentials(creds)
    .authentication(vec![Mechanism::Login])
    .build();

let ok = transport.test_connection().await?;
```

### AsyncSmtpTransport clear connection (no credentials = connect-only)

```rust
// Source: https://docs.rs/lettre/0.11.19/lettre/transport/smtp/struct.AsyncSmtpTransportBuilder.html
use lettre::{AsyncSmtpTransport, Tokio1Executor};

// builder_dangerous: no TLS, no auth by default
let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous("mail.example.com")
    .port(25)
    // No .credentials() or .authentication() = connect-only (EHLO + NOOP)
    .build();

let ok = transport.test_connection().await?;
```

### hickory-resolver MX lookup (fail-silent, trailing dot stripped)

```rust
// Source: https://docs.rs/hickory-resolver/0.25.2/hickory_resolver/struct.Resolver.html
// https://docs.rs/hickory-resolver/0.25.2/hickory_resolver/lookup/struct.MxLookup.html
use hickory_resolver::Resolver;

async fn get_mx_hosts(domain: &str) -> Vec<String> {
    let Ok(resolver) = Resolver::builder_tokio() else { return vec![]; };
    let resolver = resolver.build();

    // Fully-qualified lookup (trailing dot): cheaper, no search-domain appending
    let fqdn = format!("{domain}.");
    let Ok(mx_lookup) = resolver.mx_lookup(fqdn.as_str()).await else { return vec![]; };

    mx_lookup
        .iter()
        .map(|mx| mx.exchange().to_utf8())
        // Strip trailing DNS dot: "aspmx.l.google.com." -> "aspmx.l.google.com"
        .map(|s| s.trim_end_matches('.').to_lowercase())
        .collect()
}
```

### tokio::join!() concurrent validation with 15s outer timeout

```rust
// Source: tokio 1.x — tokio::join! macro + tokio::time::timeout
use tokio::time::{timeout, Duration};

// Inside the napi async fn:
match timeout(Duration::from_secs(15), async {
    tokio::join!(
        crate::imap::do_test_imap(&imap_opts),   // InternalResult<IMAPConnectionResult>
        do_test_smtp(&smtp_opts),                  // InternalResult<SMTPConnectionResult>
        resolve_mx_identifier(&domain),            // Option<String> (always Ok)
    )
}).await {
    Ok((imap_res, smtp_res, identifier)) => {
        // Assemble AccountValidationResult from sub-results
        let imap_success = imap_res.as_ref().map(|r| r.success).unwrap_or(false);
        let smtp_success = smtp_res.as_ref().map(|r| r.success).unwrap_or(false);
        let success = imap_success && smtp_success;
        // ... build result
    }
    Err(_elapsed) => {
        // Entire join timed out — return timeout error
    }
}
```

### napi export pattern for testSMTPConnection (mirrors imap.rs)

```rust
// Source: established Phase 2 pattern in imap.rs — mirrors test_imap_connection
#[napi(js_name = "testSMTPConnection")]  // CRITICAL: all-caps SMTP to match C++ export name
pub async fn test_smtp_connection(
    opts: SMTPConnectionOptions,
) -> napi::Result<SMTPConnectionResult> {
    let host = opts.hostname.clone();
    let port = opts.port;

    match tokio::time::timeout(Duration::from_secs(15), do_test_smtp(&opts)).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => {
            let error_type = classify_smtp_error(e.as_ref());
            Ok(SMTPConnectionResult {
                success: false,
                error: Some(format!("Connection to {host}:{port} failed: {e}")),
                error_type: Some(error_type),
            })
        }
        Err(_elapsed) => Ok(SMTPConnectionResult {
            success: false,
            error: Some(format!("Connection to {host}:{port} timed out after 15 seconds")),
            error_type: Some("timeout".to_string()),
        }),
    }
}
```

### loader.js additions for Phase 3

```js
// Source: established Phase 2 pattern — lesson from STATE.md:
// "loader.js must export each new function per phase"
// Add to app/mailcore-rs/loader.js:
module.exports.testSMTPConnection = nativeBinding.testSMTPConnection;
module.exports.validateAccount = nativeBinding.validateAccount;
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `trust-dns-resolver` crate | `hickory-resolver` crate | v0.24 (2023) | Same codebase, new name. Use `hickory-resolver`; trust-dns-resolver crate still exists but is unmaintained |
| C++ busy-wait polling loop (`sleep_for(50ms)` in napi_validator.cpp) | Rust `tokio::join!()` concurrent async | This phase | True async concurrency; no CPU waste; 15s cap vs unlimited C++ wait |
| C++ hardcoded `ConnectionTypeTLS` for both IMAP and SMTP in validator | Configurable connection type per server in Rust | This phase | Rust validateAccount supports mixed TLS/STARTTLS (e.g., IMAP TLS + SMTP STARTTLS = standard Gmail/Outlook config) |
| lettre 0.9 sync blocking | lettre 0.10+ `AsyncSmtpTransport` | 2021 | Blocking API removed in 0.10; async-only in 0.10+ |

**Deprecated/outdated:**
- `trust-dns-resolver` crate name: use `hickory-resolver` 0.25
- lettre 0.9 and earlier: blocking only, no async. All pre-0.10 examples are invalid for 0.11+
- `async-smtp` crate: archived, unmaintained, no rustls support. Do not use
- `Resolver::new()` from hickory-resolver: creates internal runtime, may panic inside existing tokio context; use `Resolver::builder_tokio()` instead

---

## Open Questions

1. **lettre rustls version unification**
   - What we know: lettre 0.11.19 pins `rustls = "0.23.18"`. Project already uses `rustls = "0.23"` (from Phase 2). Cargo semver allows `0.23.x` to unify.
   - What's unclear: Whether exact pinned sub-version (0.23.18 in lettre vs unspecified 0.23.x in project) causes a Cargo resolution conflict.
   - Recommendation: After adding lettre to Cargo.toml, run `cargo build` and check `cargo tree -i rustls`. If two rustls versions appear, pin project's rustls to `"=0.23.18"` to match lettre.

2. **hickory-resolver `builder_tokio()` in napi-rs tokio context**
   - What we know: `Resolver::builder_tokio()` is documented to use the ambient tokio runtime. The napi-rs managed runtime is a standard multi-thread tokio runtime. GitHub issue history showed "Cannot start runtime from within runtime" panics for older versions when using `Resolver::new()`.
   - What's unclear: Whether 0.25.2 fully resolved this for `builder_tokio()`.
   - Recommendation: Use `Resolver::builder_tokio()` (not `Resolver::new()`). If panic occurs in early testing, consider creating the resolver outside the napi-rs context or caching it in a `LazyLock`. Flag for early validation during Wave 1 of Phase 3.

3. **lettre `rustls-platform-verifier` feature vs separate `rustls-platform-verifier` dep**
   - What we know: lettre has `rustls-platform-verifier` as an optional dependency in its Cargo.toml. Optional deps in Cargo 2024 edition create a feature of the same name. The project already has `rustls-platform-verifier = "0.6"` directly.
   - What's unclear: Whether activating both lettre's `rustls-platform-verifier` feature AND having the project's direct `rustls-platform-verifier` dep causes version conflicts or duplicate initialization.
   - Recommendation: If a conflict occurs, remove the lettre `rustls-platform-verifier` feature and instead use `TlsParameters::builder(hostname).build_rustls()` (which uses webpki-roots as fallback) or investigate whether lettre 0.11 supports injecting a custom `ClientConfig`.

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness (`#[tokio::test]` attribute) |
| Config file | None — tokio test attribute handles async setup |
| Quick run command | `cd app/mailcore-rs && cargo test smtp_tests 2>/dev/null` |
| Full suite command | `cd app/mailcore-rs && cargo test 2>/dev/null` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SMTP-01 | TLS connection: direct TLS to plain TCP mock returns `tls_error` (consistent with IMAP TLS test pattern) | unit (mock) | `cargo test smtp_tests::test_tls_on_plain_server_returns_tls_error` | ❌ Wave 0 |
| SMTP-02 | STARTTLS connection: mock server sends plain greeting, upgrade attempt fails on plain server | unit (mock) | `cargo test smtp_tests::test_starttls_connection` | ❌ Wave 0 |
| SMTP-03 | Clear connection: mock SMTP server accepts EHLO + NOOP, returns success | unit (mock) | `cargo test smtp_tests::test_clear_connection_succeeds` | ❌ Wave 0 |
| SMTP-04 | Password auth: mock accepts AUTH LOGIN; XOAUTH2: mock accepts AUTH XOAUTH2 with correct format | unit (mock) | `cargo test smtp_tests::test_password_auth` and `cargo test smtp_tests::test_xoauth2_auth` | ❌ Wave 0 |
| SMTP-05 | Timeout: hanging mock server fires 15s timeout (use 3s in test) | unit (mock) | `cargo test smtp_tests::test_timeout_fires` | ❌ Wave 0 |
| VALD-01 | Concurrent execution: both IMAP and SMTP mocks run in parallel (timing assertion) | unit (mock) | `cargo test smtp_tests::test_validate_concurrent_timing` | ❌ Wave 0 |
| VALD-02 | MX fail-silent: `identifier` is `None` when DNS fails; validateAccount still succeeds | unit | `cargo test smtp_tests::test_validate_mx_fail_silent` | ❌ Wave 0 |
| VALD-03 | Result shape: IMAP sub-result has capabilities on success; error prefix "IMAP: " / "SMTP: " | unit | `cargo test smtp_tests::test_validate_result_shape` | ❌ Wave 0 |
| VALD-04 | TypeScript types compile against consumer files | integration | `cd app && npx tsc --noEmit` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `cd app/mailcore-rs && cargo test smtp_tests 2>/dev/null`
- **Per wave merge:** `cd app/mailcore-rs && cargo test 2>/dev/null`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `app/mailcore-rs/tests/smtp_tests.rs` — all SMTP-01 through VALD-03 tests; mock SMTP server following `imap_tests.rs` pattern (inline mock handler, random port, `TcpListener::bind("127.0.0.1:0")`)
- [ ] Mock SMTP server protocol in smtp_tests.rs must handle: `220` greeting, `EHLO`, `AUTH LOGIN` (base64 multi-step), `AUTH XOAUTH2` (base64 single-step), `NOOP`, `QUIT`; optional `STARTTLS` for STARTTLS tests
- [ ] TypeScript type update: `app/mailcore/types/index.d.ts` must be updated with extended `AccountValidationResult` (adding `imapResult`, `smtpResult`, `errorType` fields) and extended `IMAPConnectionResult` / `SMTPConnectionResult`; VALD-04 verified by `cd app && npx tsc --noEmit`

*(Existing test infrastructure in provider_tests.rs and imap_tests.rs covers all Phase 1/2 requirements and needs no changes)*

---

## Sources

### Primary (HIGH confidence)
- [docs.rs/lettre/0.11.19/lettre/transport/smtp/struct.AsyncSmtpTransport](https://docs.rs/lettre/latest/lettre/transport/smtp/struct.AsyncSmtpTransport.html) — relay(), starttls_relay(), builder_dangerous(), test_connection(), Tokio1Executor
- [docs.rs/lettre/0.11.19/lettre/transport/smtp/struct.AsyncSmtpTransportBuilder](https://docs.rs/lettre/latest/lettre/transport/smtp/struct.AsyncSmtpTransportBuilder.html) — credentials(), authentication(), port(), tls(), timeout(), build()
- [docs.rs/lettre/0.11.19/lettre/transport/smtp/authentication/enum.Mechanism](https://docs.rs/lettre/latest/lettre/transport/smtp/authentication/enum.Mechanism.html) — Plain, Login, Xoauth2 variants confirmed
- [docs.rs/lettre/0.11.19/lettre/transport/smtp/client/enum.Tls](https://docs.rs/lettre/latest/lettre/transport/smtp/client/enum.Tls.html) — None, Opportunistic, Required, Wrapper variants
- [docs.rs/lettre/0.11.19 features page](https://docs.rs/crate/lettre/latest/features) — `rustls-platform-verifier` optional dep confirmed; `tokio1-rustls` feature name confirmed
- [docs.rs/hickory-resolver/0.25.2/struct.Resolver](https://docs.rs/hickory-resolver/0.25.2/hickory_resolver/struct.Resolver.html) — builder_tokio(), mx_lookup() API
- [docs.rs/hickory-resolver/0.25.2/lookup/struct.MxLookup](https://docs.rs/hickory-resolver/0.25.2/hickory_resolver/lookup/struct.MxLookup.html) — iter(), exchange().to_utf8(), preference()
- `app/mailcore/src/napi/napi_smtp.cpp` — C++ SMTP test: connect + loginIfNeeded flow (Rust goes beyond with NOOP)
- `app/mailcore/src/napi/napi_validator.cpp` — C++ validateAccount: busy-wait polling, hardcoded TLS, result shape (imapServer, smtpServer, identifier)
- `app/mailcore/types/index.d.ts` — TypeScript interface contracts (AccountValidationResult, SMTPConnectionResult, IMAPConnectionResult)
- `app/mailcore-rs/src/imap.rs` — Phase 2 patterns: error classification, timeout wrapping, napi export structure, XOAuth2 SASL format
- `app/mailcore-rs/loader.js` — Phase 2 lesson: functions must be explicitly re-exported
- `.planning/STATE.md` — confirmed decisions: rustls-only (no OpenSSL), exact dependency pinning, loader.js re-export requirement

### Secondary (MEDIUM confidence)
- [lettre/src/transport/smtp/async_transport.rs](https://github.com/lettre/lettre/blob/master/src/transport/smtp/async_transport.rs) — relay(), starttls_relay(), builder_dangerous() implementation; test_connection() NOOP behavior
- [lettre/src/transport/smtp/client/tls.rs](https://github.com/lettre/lettre/blob/master/src/transport/smtp/client/tls.rs) — TlsParameters build_rustls() with rustls-platform-verifier integration

### Tertiary (LOW confidence — verify before use)
- hickory-resolver 0.25.2 `builder_tokio()` behavior inside napi-rs tokio runtime: documented as ambient-runtime-safe but runtime-within-runtime panic fix status not explicitly confirmed in 0.25.2 changelog; validate early in Phase 3
- lettre `rustls-platform-verifier` feature activation with co-existing direct project dep: needs `cargo build` + `cargo tree` verification

---

## Metadata

**Confidence breakdown:**
- Standard stack (lettre 0.11.19, hickory-resolver 0.25.2): HIGH — verified from docs.rs
- lettre feature flags (`tokio1-rustls`, `rustls-platform-verifier`): HIGH — verified from lettre features page
- lettre API (relay, starttls_relay, builder_dangerous, test_connection, Mechanism::Xoauth2): HIGH — verified from docs.rs
- AccountValidationResult shape: HIGH — directly specified in CONTEXT.md locked decisions
- hickory-resolver MX API: HIGH — verified from docs.rs
- tokio::join!() in napi async fn: HIGH — established pattern from Phase 2 and tokio docs
- hickory-resolver runtime conflict: MEDIUM — known issue, fix status in 0.25.2 not confirmed
- lettre rustls version unification: MEDIUM — needs cargo tree verification

**Research date:** 2026-03-03
**Valid until:** 2026-04-03 (lettre and hickory-resolver are stable; 30-day window appropriate for active development)
