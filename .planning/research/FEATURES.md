# Feature Research

**Domain:** Rust napi-rs mailcore N-API addon rewrite — IMAP/SMTP connection testing and provider detection
**Researched:** 2026-03-01
**Confidence:** HIGH (C++ source examined directly; Rust library docs and official sources verified)

---

## Context: What Is Being Built

This is a feature-parity rewrite, not a new product. The 5 functions below already exist in C++ (mailcore2 + node-addon-api). The goal is identical behavior, different implementation language. Each feature section describes: what the C++ does today, what behavior Rust must replicate, what error scenarios must be handled, and what complexity to expect.

**The 5 functions to rewrite:**

| Function | Type | C++ Source |
|----------|------|-----------|
| `registerProviders(jsonPath)` | sync, void | `napi_provider.cpp` |
| `providerForEmail(email)` | sync, MailProviderInfo or null | `napi_provider.cpp` |
| `validateAccount(opts)` | async Promise | `napi_validator.cpp` |
| `testIMAPConnection(opts)` | async Promise | `napi_imap.cpp` |
| `testSMTPConnection(opts)` | async Promise | `napi_smtp.cpp` |

---

## Feature Landscape

### Table Stakes (Users Expect These)

These behaviors are mandatory for API parity. Missing any = consumers break.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| `registerProviders(jsonPath)` — load providers.json | Called automatically on addon load; consumers assume database is ready | LOW | Parse JSON with serde_json into in-memory HashMap; regex patterns from `domain-match` and `mx-match` arrays need to be compiled with the `regex` crate. The providers.json uses escaped regex patterns like `"pobox\\.com"` — raw regex strings, not glob patterns. |
| `providerForEmail(email)` — sync domain + MX lookup | Called during onboarding before async steps; must be synchronous | MEDIUM | Domain extracted from email; match against `domain-match` regexes first, then `mx-match` regexes. Complexity: MX records are not available synchronously — the C++ mailcore2 implementation uses pre-loaded MX patterns matched against previously-resolved MX records passed in from the JS side (`onboarding-helpers.ts` resolves MX via Node.js `dns.resolveMx` then passes email address; mailcore matches by domain only in the N-API path). In practice: domain-only matching is sufficient for the N-API path; MX matching happens in `onboarding-helpers.ts` fallback. |
| Identical return shape for `MailProviderInfo` | Consumers destructure `{ identifier, servers: { imap, smtp, pop }, domainMatch, mxMatch }` | LOW | Must include `pop` array (even if empty); must use `connectionType: 'tls' \| 'starttls' \| 'clear'` strings. The providers.json uses boolean flags `ssl: true` or `starttls: true` — these must be mapped to the string enum on return. |
| `testIMAPConnection` — connect + capability detection | Called during manual account setup; must return `{ success, error?, capabilities? }` | HIGH | Capabilities expected: `idle`, `condstore`, `qresync`, `compress`, `namespace`, `xoauth2`, `gmail`. In imap-proto, capabilities are `Capability::Atom(str)` — must check `.has_str("IDLE")`, `.has_str("CONDSTORE")`, `.has_str("QRESYNC")`, `.has_str("COMPRESS=DEFLATE")`, `.has_str("NAMESPACE")`, `.has_str("X-GM-EXT-1")` and `Auth("XOAUTH2")` for the xoauth2 capability. |
| `testIMAPConnection` — TLS, STARTTLS, clear connection types | Onboarding presents 3 security modes; all must work | HIGH | Three paths: `tls` (connect on port 993 with immediate TLS), `starttls` (connect plain then STARTTLS upgrade), `clear` (no TLS). async-imap does not abstract this — caller must construct the right TLS stream before passing to `async_imap::connect()`. Requires separate code paths per connection type using `tokio-rustls` or `async-native-tls`. |
| `testSMTPConnection` — connect + login | Tests SMTP credentials before account is saved | MEDIUM | Uses lettre `SmtpTransport`. `test_connection()` method sends NOOP and returns `Result<bool, Error>`. For authentication, must build transport with `Credentials` (username + password). Returns `{ success: bool, error?: string }`. |
| `testSMTPConnection` — TLS, STARTTLS, clear connection types | Same 3 modes as IMAP | MEDIUM | lettre maps cleanly: `SmtpTransport::relay()` for TLS (port 465), `SmtpTransport::starttls_relay()` for STARTTLS (port 587), `SmtpTransport::builder_dangerous()` for clear/plain. |
| `validateAccount` — orchestrated IMAP + SMTP test | Used by `finalizeAndValidateAccount()` in onboarding; the entry point before saving an account | HIGH | C++ `AccountValidator` internally handles provider lookup + tries multiple server configs. Rust must: accept `{ email, password?, oauth2Token?, imapHostname?, imapPort?, smtpHostname?, smtpPort? }`, test IMAP then SMTP, return `{ success, error?, identifier?, imapServer?, smtpServer? }`. The identifier field comes from provider lookup for the email domain. |
| OAuth2/XOAUTH2 for both IMAP and SMTP | Gmail and O365 accounts use OAuth2 access tokens | HIGH | XOAUTH2 SASL format: `base64("user=" + email + "\x01auth=Bearer " + token + "\x01\x01")`. async-imap supports pluggable `Authenticator` trait — must implement a custom XOAUTH2 authenticator struct. lettre supports XOAUTH2 natively via `Credentials::new()` with mechanism override (verified via docs). |
| Async execution on worker thread | Must not block Node.js event loop | MEDIUM | napi-rs `#[napi]` on `async fn` automatically runs in tokio runtime. Requires `features = ["async", "tokio_rt"]` in napi Cargo.toml. The tokio runtime is created in a separate thread by napi-rs. All 3 async functions must be `async fn` — not `AsyncTask` — to use the tokio runtime directly with async-imap and lettre. |
| TypeScript type definitions auto-generated | Consumers import typed functions | LOW | napi-rs `#[napi]` macro auto-generates `.d.ts` files. Must match the interface shapes in the existing `types/index.d.ts` exactly. Return types use `Option<T>` in Rust → `T \| undefined` in TS; `null` in the C++ path becomes `null` from napi-rs when returning `Option<T>` from a sync fn. |

### Differentiators (Improvements Over C++ Baseline)

Features not in the C++ implementation that could be added in the Rust rewrite with low risk.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| Configurable connection timeout | C++ mailcore2 uses hardcoded timeouts (30s MX lookup noted in onboarding-helpers comment). Rust can expose timeout as parameter or use sensible 10s default | LOW | `tokio::time::timeout()` wraps async calls. Default 10s for IMAP/SMTP connect, 5s for capability exchange. No API change needed — internal improvement only. |
| Structured error codes in addition to message string | C++ returns only a string from `ErrorMessage::messageForError(err)`. Rust can return a machine-readable error category | MEDIUM | Would require adding `errorCode?: string` to the TS interface — a compatible addition. Categories: `auth_failed`, `tls_error`, `connection_refused`, `timeout`, `dns_error`. Callers currently only display the error string, so this is purely additive. |
| Capability detection for IMAP4rev2 | C++ mailcore2 checks for RFC 3501 capabilities only. Rust can cheaply check `IMAP4rev2` via `has_str()` | LOW | Additive to capabilities array; consumers ignore unknown capabilities. Safe to add `"imap4rev2"` to the returned array if detected. |
| Concurrent IMAP + SMTP test in `validateAccount` | C++ tests sequentially. tokio allows concurrent execution | MEDIUM | `tokio::join!()` on both tests simultaneously. Must be careful: if IMAP succeeds but SMTP fails, still want to report SMTP error. Reduces `validateAccount` latency by ~50% for most users. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| Full IMAP client (FETCH, SEARCH, IDLE loop) | Seems natural since we are already connecting | Out of scope per PROJECT.md — the mailsync C++ engine owns ongoing IMAP. Adding FETCH here creates two IMAP stacks, drift in behavior, and maintenance burden | Keep scope to connection test only: connect, authenticate, read capabilities, disconnect |
| Self-signed certificate acceptance by default | Some enterprise servers use self-signed certs; users report connection failures | Silent cert bypass is a security hazard and makes the client complicit in MITM attacks | Expose an explicit `allowInsecureSsl: boolean` option (matching the existing `imap_allow_insecure_ssl` field in account settings) using rustls `DangerousClientConfigBuilder` |
| POP3 support | providers.json includes POP server entries | C++ N-API layer never exposed POP; it is out of scope per PROJECT.md | Return `pop: []` in MailProviderInfo (already done in C++) — no POP connection testing needed |
| DNS MX resolution inside the addon | MX records are needed for provider matching | Adds async DNS dependency to what should be a sync lookup; onboarding-helpers.ts already resolves MX via Node.js dns module and passes email to `providerForEmail()` — the function only needs domain matching | Keep `providerForEmail()` as a pure in-memory domain-regex lookup. MX matching happens in the JS fallback path which already works. |
| Hot-reload of providers.json | Re-registering providers without restart | Adds concurrency complexity (RwLock on global state) with no actual use case — providers.json is bundled and static | `registerProviders()` is call-once at startup; use `OnceLock<ProviderDb>` in Rust for safe single-initialization |

---

## Detailed Behavior Specifications

### Feature 1: `registerProviders(jsonPath: string): void`

**Expected behavior:**
- Parse the JSON file at the given path using `serde_json`
- Build an in-memory map of provider identifier → ProviderRecord
- Compile all `domain-match` and `mx-match` regex patterns using the `regex` crate
- Store in a global `OnceLock<ProviderDb>` (or `Lazy<RwLock<ProviderDb>>` if re-registration is needed)
- Called once automatically when the addon loads — mirrors the C++ `Init()` function which calls `registerProvidersWithFilename()` before exporting functions

**Error scenarios:**
- File not found: throw JavaScript TypeError with path in message
- Invalid JSON: throw JavaScript TypeError with parse error message
- Invalid regex pattern in providers.json: log warning and skip that provider (do not crash — corrupted entry should not block all providers)

**providers.json format nuances:**
- `ssl: true` → connectionType `"tls"`
- `starttls: true` → connectionType `"starttls"`
- Neither flag → connectionType `"clear"`
- `hostname` may contain `{domain}` placeholder — this is expanded in JS before calling the N-API, so the Rust layer does not need to handle it
- `domain-match` patterns are already regex strings (escaped dots: `"pobox\\.com"`) — wrap with `^` and `$` anchors when compiling: `^pobox\.com$`

**TLS negotiation:** None — this is a file parse, fully sync.

**Authentication flows:** None.

---

### Feature 2: `providerForEmail(email: string): MailProviderInfo | null`

**Expected behavior:**
- Extract domain from email: `"user@example.com"` → `"example.com"` (lowercase)
- Iterate providers in load order; for each provider:
  - Test each `domain-match` regex against the domain
  - If any matches, return that provider's data
- If no domain match found, return null (MX matching is not done here — it is done in onboarding-helpers.ts JS fallback)
- Return shape: `{ identifier, servers: { imap, smtp, pop }, domainMatch, mxMatch }`
- Each server entry: `{ hostname, port, connectionType: 'tls' | 'starttls' | 'clear' }`

**Error scenarios:**
- Malformed email (no `@`): return null (do not throw — matches C++ behavior which calls `providerForEmail` on the mailcore MailProvidersManager which gracefully returns null)
- Provider database not loaded: return null with a console warning

**Performance:** Must be synchronous and fast (sub-millisecond) — called in the hot path during onboarding UI typing. The compiled regex set should be cached at `registerProviders()` time, not recompiled per call.

**TLS negotiation:** None — this is an in-memory lookup.

**Authentication flows:** None.

---

### Feature 3: `testIMAPConnection(opts): Promise<IMAPConnectionResult>`

**Input:**
```typescript
{
  hostname: string,
  port: number,
  connectionType?: 'tls' | 'starttls' | 'clear',  // default: 'tls'
  username?: string,
  password?: string,
  oauth2Token?: string
}
```

**Expected behavior:**
1. Establish TCP connection to `hostname:port`
2. Apply TLS based on `connectionType`:
   - `'tls'`: wrap TCP stream with TLS immediately (IMAPS, port 993)
   - `'starttls'`: connect plain, send STARTTLS command, upgrade to TLS
   - `'clear'`: use plain TCP (no TLS)
3. Read server greeting
4. Authenticate:
   - If `oauth2Token` is set: use XOAUTH2 SASL mechanism
   - Else if `username` + `password`: use LOGIN command
   - If neither: skip authentication (capability detection still works post-greeting pre-auth for many servers)
5. Send CAPABILITY command, parse response
6. Check for: IDLE, CONDSTORE, QRESYNC, COMPRESS=DEFLATE, NAMESPACE, AUTH=XOAUTH2, X-GM-EXT-1 (Gmail)
7. Disconnect (LOGOUT)
8. Return `{ success: true, capabilities: [...] }` or `{ success: false, error: "message" }`

**Capability mapping (C++ to Rust string labels):**
| C++ IMAPCapability flag | Rust check | Return label |
|------------------------|------------|-------------|
| `IMAPCapabilityIdle` | `caps.has_str("IDLE")` | `"idle"` |
| `IMAPCapabilityCondstore` | `caps.has_str("CONDSTORE")` | `"condstore"` |
| `IMAPCapabilityQResync` | `caps.has_str("QRESYNC")` | `"qresync"` |
| `IMAPCapabilityCompressDeflate` | `caps.has_str("COMPRESS=DEFLATE")` | `"compress"` |
| `IMAPCapabilityNamespace` | `caps.has_str("NAMESPACE")` | `"namespace"` |
| `IMAPCapabilityXOAuth2` | `caps.has(Capability::Auth("XOAUTH2"))` | `"xoauth2"` |
| `IMAPCapabilityGmail` | `caps.has_str("X-GM-EXT-1")` | `"gmail"` |

**XOAUTH2 SASL format (RFC compliant):**
```
base64( "user=" + email + "\x01auth=Bearer " + access_token + "\x01\x01" )
```
Implement as a struct implementing `async_imap::Authenticator` trait.

**Error scenarios:**
- TCP connection refused: `{ success: false, error: "Connection refused to hostname:port" }`
- TLS handshake failure (bad cert): `{ success: false, error: "TLS error: ..." }`
- Auth failure (wrong password): `{ success: false, error: "Authentication failed" }`
- Timeout (server unresponsive): `{ success: false, error: "Connection timed out" }`
- STARTTLS not supported by server: `{ success: false, error: "Server does not support STARTTLS" }`
- Self-signed cert (when strict TLS): `{ success: false, error: "Certificate verification failed" }`

**TLS negotiation:**
- Use `tokio-rustls` with `rustls-native-certs` for OS certificate store integration (avoids WebPKI-only which misses enterprise CAs)
- For STARTTLS: send `STARTTLS` command, check OK response, then upgrade stream with `TlsConnector`
- `allowInsecureSsl` (if exposed): use `rustls::ClientConfig` with `DangerousClientConfigBuilder::with_custom_certificate_verifier(Arc::new(NoCertVerifier))`

**Authentication flows:**
- Password: `session.login(username, password).await` after connect
- XOAUTH2: `session.authenticate("XOAUTH2", &XOAuth2Authenticator { user, token }).await`
- No credentials: attempt capability fetch after greeting without auth (some servers advertise caps pre-auth)

---

### Feature 4: `testSMTPConnection(opts): Promise<SMTPConnectionResult>`

**Input:**
```typescript
{
  hostname: string,
  port: number,
  connectionType?: 'tls' | 'starttls' | 'clear',  // default: 'tls'
  username?: string,
  password?: string,
  oauth2Token?: string
}
```

**Expected behavior:**
1. Build lettre transport based on `connectionType`:
   - `'tls'`: `SmtpTransport::relay(hostname)` on port 465
   - `'starttls'`: `SmtpTransport::starttls_relay(hostname)` on port 587
   - `'clear'`: `SmtpTransport::builder_dangerous(hostname).port(port).build()`
2. Add credentials if `username` is present:
   - Password auth: `Credentials::new(username, password)` with mechanism PLAIN or LOGIN
   - XOAUTH2: `Credentials::new(username, oauth2Token)` with mechanism XOAUTH2
3. Call `transport.test_connection()` — sends SMTP NOOP command
4. Return `{ success: true }` or `{ success: false, error: "message" }`

**Note on lettre async:** lettre's `AsyncSmtpTransport` requires tokio feature. Since napi-rs async functions run on tokio, use `AsyncSmtpTransport<Tokio1Executor>` for the async functions.

**Error scenarios:**
- Connection refused: `{ success: false, error: "Connection refused" }`
- TLS failure: `{ success: false, error: "TLS error: ..." }`
- Auth failure (wrong password): `{ success: false, error: "Authentication failed (535)" }`
- STARTTLS downgrade (server does not advertise STARTTLS): `{ success: false, error: "STARTTLS not available" }`
- Timeout: `{ success: false, error: "Connection timed out" }`

**TLS negotiation:**
- lettre supports `native-tls` and `rustls-tls` feature flags — use `rustls-tls` for pure Rust (no OpenSSL dependency on Linux)
- For Windows, `rustls-native-roots` enables Windows certificate store integration
- C++ behavior: SMTP connection type is also hardcoded to TLS in `napi_validator.cpp` for the validate path (not passed through); standalone `testSMTPConnection` respects the provided `connectionType`

**Authentication flows:**
- No credentials: `test_connection()` only (validates server reachability + greeting)
- Password: credentials added to transport builder before `test_connection()`
- XOAUTH2: lettre's `Credentials` with `Mechanism::Xoauth2` (lettre has native XOAUTH2 support per docs)

---

### Feature 5: `validateAccount(opts): Promise<AccountValidationResult>`

**Input:**
```typescript
{
  email: string,
  password?: string,
  oauth2Token?: string,
  imapHostname?: string,
  imapPort?: number,      // default: 993
  smtpHostname?: string,
  smtpPort?: number       // default: 587
}
```

**Expected behavior:**
1. If `imapHostname` provided: test IMAP with given hostname/port
2. If `smtpHostname` provided: test SMTP with given hostname/port
3. Connection type for both is TLS (`ConnectionTypeTLS`) — this matches the C++ `napi_validator.cpp` which hardcodes `ConnectionTypeTLS`
4. Look up provider identifier from email domain via `providerForEmail()`
5. Return:
```typescript
{
  success: boolean,
  error?: string,
  identifier?: string,         // provider identifier like "gmail" if matched
  imapServer?: { hostname: string, port: number },
  smtpServer?: { hostname: string, port: number }
}
```

**Error scenarios:**
- IMAP fails: `{ success: false, error: "IMAP: <imap error message>" }`
- SMTP fails: `{ success: false, error: "SMTP: <smtp error message>" }`
- Both fail: report first failure (IMAP checked first in C++)

**Note on concurrency:** C++ validates sequentially. The Rust implementation can use `tokio::join!()` to test IMAP and SMTP concurrently for speed, but must merge errors carefully (prefer IMAP error if both fail, since that is what the C++ reports first).

**TLS negotiation:** Always TLS (port 993 IMAP, no STARTTLS). This is a deliberate simplification in the C++ validator — it assumes well-configured servers use IMAPS.

**Authentication flows:** Same as `testIMAPConnection` and `testSMTPConnection` — passes through password or oauth2Token.

---

## Feature Dependencies

```
registerProviders()
    └──required by──> providerForEmail()
                          └──used by──> validateAccount() (for identifier field)

testIMAPConnection()
    └──required by──> validateAccount() (IMAP half)

testSMTPConnection()
    └──required by──> validateAccount() (SMTP half)

tokio runtime (napi-rs feature)
    └──required by──> testIMAPConnection(), testSMTPConnection(), validateAccount()

async-imap crate
    └──required by──> testIMAPConnection()
    └──XOAUTH2 Authenticator impl──required by──> testIMAPConnection() OAuth2 path

lettre crate
    └──required by──> testSMTPConnection()
    └──XOAUTH2 Credentials──required by──> testSMTPConnection() OAuth2 path

TLS stack (rustls + tokio-rustls + rustls-native-certs)
    └──required by──> testIMAPConnection() (TLS + STARTTLS paths)
    └──required by──> testSMTPConnection() (via lettre rustls-tls feature)
```

### Dependency Notes

- **registerProviders() must run before providerForEmail():** The addon auto-calls registerProviders() on module load (same as C++ Init()). If not loaded, providerForEmail() returns null.
- **XOAUTH2 requires custom implementation for IMAP:** async-imap does not provide a built-in XOAUTH2 authenticator — must implement the `Authenticator` trait manually. lettre does have native XOAUTH2 support.
- **TLS stack choice affects all 3 async functions:** Using rustls throughout is preferred over native-tls for cross-platform consistency and no OpenSSL build dependency.

---

## MVP Definition

### Launch With (v1 — API Parity)

These are required for the milestone. All 5 functions, all behaviors matching C++ exactly.

- [ ] `registerProviders()` — JSON parse + regex compile + global store
- [ ] `providerForEmail()` — domain regex lookup, correct return shape with `connectionType` string enum
- [ ] `testIMAPConnection()` — TLS/STARTTLS/clear, password + XOAUTH2 auth, 7 capabilities detected
- [ ] `testSMTPConnection()` — TLS/STARTTLS/clear, password + XOAUTH2 auth, test_connection() NOOP
- [ ] `validateAccount()` — IMAP + SMTP with TLS, identifier from provider lookup, correct result shape
- [ ] TypeScript types auto-generated by napi-rs matching existing `types/index.d.ts` shapes
- [ ] Non-blocking: all 3 async functions run on tokio worker threads via napi-rs async feature

### Add After Validation (v1.x)

Safe additive improvements once core is working.

- [ ] Configurable timeout parameter — when connection hangs on misconfigured servers
- [ ] Concurrent IMAP + SMTP in `validateAccount()` — reduces validation time from ~4s to ~2s
- [ ] `allowInsecureSsl` parameter — enterprise servers with self-signed certs currently fail silently

### Future Consideration (v2+)

- [ ] Structured error codes (errorCode field) — only if consumers need machine-readable errors
- [ ] IMAP4rev2 capability detection — when IMAP4rev2 servers become widespread

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| registerProviders + providerForEmail | HIGH | LOW | P1 |
| testIMAPConnection password auth + TLS | HIGH | MEDIUM | P1 |
| testSMTPConnection password auth + TLS | HIGH | MEDIUM | P1 |
| validateAccount basic flow | HIGH | MEDIUM | P1 |
| STARTTLS for IMAP | HIGH | HIGH | P1 |
| STARTTLS for SMTP | HIGH | LOW | P1 |
| XOAUTH2 for IMAP | HIGH | HIGH | P1 |
| XOAUTH2 for SMTP | HIGH | LOW | P1 |
| 7-capability detection (idle, condstore, etc.) | MEDIUM | LOW | P1 |
| Concurrent IMAP+SMTP in validateAccount | MEDIUM | LOW | P2 |
| Configurable timeout | MEDIUM | LOW | P2 |
| allowInsecureSsl option | LOW | MEDIUM | P2 |
| Structured error codes | LOW | MEDIUM | P3 |

**Priority key:**
- P1: Must have for launch (parity with C++)
- P2: Should have, add when possible
- P3: Nice to have, future consideration

---

## Complexity Hotspots

The following areas have higher implementation complexity than they appear:

### IMAP STARTTLS (Highest Risk)

STARTTLS for IMAP requires the caller to manage two different stream types before and after upgrade. async-imap does not abstract this — you must:
1. Establish plain TCP with `tokio::net::TcpStream`
2. Create an unencrypted `async_imap::Client` using `async_imap::connect()`
3. Send the STARTTLS capability exchange manually
4. Use the returned pre-auth client to drive the STARTTLS upgrade
5. Wrap the upgraded `TlsStream<TcpStream>` as the new transport
6. Re-greet and authenticate on the encrypted connection

This is distinct from IMAPS (port 993) where TLS wraps the TCP connection before the IMAP protocol starts.

**Mitigation:** Look at deltachat-core-rust or mail-parser-rs for existing STARTTLS patterns with async-imap before implementing from scratch.

### XOAUTH2 for IMAP (Medium Risk)

async-imap's `Authenticator` trait is straightforward but requires encoding the XOAUTH2 challenge response correctly. The base64 format is:
```
base64("user=" + email + "\x01auth=Bearer " + token + "\x01\x01")
```
The server may respond with an error JSON payload if the token is expired — this must be parsed and surfaced as a meaningful error, not a raw protocol error.

### Global Provider State Thread Safety

The provider database (loaded by `registerProviders()`) must be accessed from multiple async tasks concurrently. Use `once_cell::Lazy<RwLock<ProviderDb>>` or `std::sync::OnceLock` (stable in Rust 1.70+) to ensure single initialization and shared read access. Never use `Mutex` with `.lock().unwrap()` on the hot path — deadlock risk if a panic occurs during lock hold.

### providers.json Regex Patterns

The patterns are partial regex strings like `"pobox\\.com"`. They need anchoring (`^` + `$`) to prevent substring matches. The `mx-match` patterns are applied against MX hostname strings (already lowercased). Must use the `regex` crate's `RegexSet` for O(n) multi-pattern matching rather than sequential iteration.

---

## Sources

- C++ source (directly read): `app/mailcore/src/napi/napi_imap.cpp`, `napi_smtp.cpp`, `napi_provider.cpp`, `napi_validator.cpp`, `addon.cpp`
- TypeScript interface (directly read): `app/mailcore/types/index.d.ts`
- Consumer behavior (directly read): `app/internal_packages/onboarding/lib/onboarding-helpers.ts`
- [async-imap docs — Session::capabilities()](https://docs.rs/async-imap/latest/async_imap/struct.Session.html)
- [lettre SmtpTransport::test_connection()](https://docs.rs/lettre/latest/lettre/transport/smtp/struct.SmtpTransport.html)
- [lettre SMTP transport docs — TLS, STARTTLS, XOAUTH2](https://docs.rs/lettre/latest/lettre/transport/smtp/index.html)
- [imap-proto Capability enum — Atom/Auth variants](https://docs.rs/imap-proto/latest/imap_proto/types/enum.Capability.html)
- [napi-rs async fn docs](https://napi.rs/docs/concepts/async-fn)
- [napi-rs cross-compilation targets](https://napi.rs/docs/cross-build)
- [hickory-resolver MxLookup](https://docs.rs/hickory-resolver/latest/hickory_resolver/lookup/struct.MxLookup.html)
- [Gmail XOAUTH2 protocol spec](https://developers.google.com/workspace/gmail/imap/xoauth2-protocol)
- [Common IMAP server CAPABILITY responses (reference)](https://gist.github.com/emersion/2c769bc1ed60a7b7945910d35b606801)

---

*Feature research for: Rust napi-rs mailcore N-API addon rewrite*
*Researched: 2026-03-01*
