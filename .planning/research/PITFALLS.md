# Pitfalls Research

**Domain:** Rust napi-rs native addon for Electron (IMAP/SMTP/provider detection)
**Researched:** 2026-03-01
**Confidence:** HIGH (most claims verified against official docs, napi-rs changelog, and GitHub issues)

---

## Critical Pitfalls

### Pitfall 1: Tokio Runtime Lifecycle Mismatch with Electron's Process Model

**What goes wrong:**
napi-rs maintains a global tokio runtime internally. On macOS, a bug (fixed in napi v2.16.16) caused the runtime reference count to not be restored after Electron's renderer process lifecycle events, leading to panics or silent promise hangs when async functions were called after the runtime was implicitly torn down. On Windows, napi-sys v2.2.1 introduced `thread_local!` usage that broke `GetProcAddress` during module initialization in Electron, producing an immediate panic at startup.

The issue is not just historical: Electron spawns multiple processes (main, renderer, utility), and each process that loads the `.node` addon gets its own tokio runtime. If the addon is loaded in a worker thread context that terminates while Rust futures are still pending, the result is SIGABRT rather than graceful promise rejection (documented in napi-rs issue #2460, fixed by restoring THREAD_DESTROYED detection).

**Why it happens:**
Electron is not plain Node.js. Its V8 embedder lifecycle, multi-process architecture, and worker thread model create process teardown scenarios that never occur in a standard Node.js server. napi-rs was initially written for servers; Electron-specific bugs were only addressed after being reported. The tokio runtime in napi-rs is initialized lazily and globally — if the process exits while futures are in-flight, the teardown order is not guaranteed.

**How to avoid:**
- Pin napi-rs to a version >= 2.16.16 for the macOS reference count fix.
- Ensure napi-rs is >= the version that removed thread_local usage (post-PR #1176). The napi-rs package template generates a `Cargo.toml` that pulls current versions — use the template rather than a hand-rolled setup.
- Always wrap async napi-rs functions with `tokio::time::timeout` so they cannot hang indefinitely if the runtime is unexpectedly unavailable.
- In Electron, load the `.node` addon only in the main process or a dedicated utility process, not in renderer processes. The existing `mailsync-process.ts` already does this.
- Add an Electron integration test that loads the addon and calls all five functions — run it as part of CI using `electron-mocha` or similar to catch runtime mismatches before shipping.

**Warning signs:**
- Addon loads in regular Node.js but panics immediately in Electron at startup.
- `GetProcAddress failed` message in Electron's stderr on Windows.
- Promises returned by `validateAccount` or `testIMAPConnection` never resolve or reject when Electron is quitting.
- SIGABRT on macOS during app quit, particularly if async operations were in-flight.

**Phase to address:**
Phase 1 (scaffolding) — verify the addon loads cleanly in Electron before writing a single line of IMAP code. This is a go/no-go gate.

---

### Pitfall 2: TLS Library Choice Conflicts with Electron's BoringSSL

**What goes wrong:**
If the Rust crate graph includes `openssl-sys` (even as a transitive dependency), the addon may fail to compile or produce symbol conflicts at runtime on Linux. On macOS and Windows this often compiles fine — then the Linux CI build fails because there are no OpenSSL headers in the CI environment, or there is an ABI mismatch between the OpenSSL the system provides and what Electron's bundled Node.js expects.

More critically: `native-tls` on Linux links against the system's `libssl.so`. Electron ships its own BoringSSL. Symbol names overlap between OpenSSL and BoringSSL. When both are loaded into the same process, global SSL state initialization can conflict, causing TLS handshakes to fail silently or producing hard-to-debug segfaults in the TLS layer. Electron's own issue tracker (issue #13176) confirmed: "they conflict with Chromium's fork of OpenSSL" and exporting BoringSSL symbols was closed as "causes more problems than it solves."

**Why it happens:**
`lettre` defaults to `native-tls` and `async-imap` requires a TLS connector. Developers reaching for the most natural TLS choice end up with system OpenSSL. On macOS, `native-tls` uses Secure Transport (no conflict). On Windows, it uses SChannel (no conflict). Linux is the dangerous case because it uses OpenSSL which can conflict with BoringSSL in Electron.

**How to avoid:**
Use `rustls` exclusively. `rustls` is a pure-Rust TLS implementation with no system library dependencies, no header requirements, and no link-time conflicts with Electron's BoringSSL. Concrete dependency choices:
- `lettre` with feature flags `rustls-tls` and without `native-tls` (disable default features).
- `async-imap` with `tokio-rustls` as the TLS connector, not `async-native-tls`.
- `trust-dns-resolver` with `dns-over-rustls` if DNS-over-TLS is needed.
- Audit `Cargo.lock` after each dependency addition: `cargo tree | grep openssl` must return nothing. If it does, identify and eliminate the dependency pulling it in.

**Warning signs:**
- `cargo tree | grep openssl` shows any entry.
- Linux-only TLS handshake failures during IMAP STARTTLS upgrade when running inside Electron.
- "SSL_CTX_new: no cipher" or similar errors at runtime on Linux.
- CI builds succeed but runtime fails only when loaded from within Electron (not from plain `node`).

**Phase to address:**
Phase 1 (scaffolding) — establish `rustls`-only from the start. Retrofitting TLS libraries across all crates is painful because `lettre` and `async-imap` have separate feature flags that must be coordinated.

---

### Pitfall 3: Electron V8 Memory Cage Breaks External ArrayBuffer/Buffer

**What goes wrong:**
Starting with Electron 21 (June 2022), V8 sandboxed pointers are enabled. This means `ArrayBuffer` or `Buffer` objects pointing to external (off-heap) memory are no longer permitted. Any napi-rs code that calls `Env::create_external_arraybuffer` or uses `TypedArray::with_external_data` will crash at runtime inside Electron with an assertion failure — the application quits without a useful error message.

For this addon, this is only relevant if Rust-allocated byte buffers (e.g., raw email data, provider JSON bytes) are returned directly to JavaScript as `Buffer`. The C++ predecessor never did this for the 5 exposed functions (all returns are plain objects/strings/booleans), but a naive Rust port could introduce it by returning `Vec<u8>` through napi-rs's zero-copy path.

**Why it happens:**
Electron 21+ enforces the V8 memory cage for security. napi-rs added a compatibility fix (`create external TypedArray in Electron env`, released in v3.0.0-beta.10), but the fix copies the data rather than wrapping it — developers who manually bypass this by using `unsafe` external buffer APIs will hit the crash.

**How to avoid:**
- Never use `Env::create_external_arraybuffer` or `TypedArray::with_external_data` for data returned to JavaScript.
- Return `Vec<u8>` or `String` from napi-rs functions and let napi-rs copy them into V8-managed memory.
- For the 5-function API in this project (all return structured objects), this is naturally avoided — but must be consciously maintained if any function ever needs to return binary data.
- Confirm the version of napi-rs used includes the Electron external TypedArray fix.

**Warning signs:**
- "Assertion failed" crash immediately when a function that returns binary data is called from Electron.
- Works fine from `node` but crashes inside Electron.
- No stack trace — the crash happens inside V8's memory validation layer before Rust code can run a destructor.

**Phase to address:**
Phase 1 (scaffolding) — verify all return types up front. Since all five functions return plain JS objects (not buffers), this is a design constraint to declare explicitly, not a fix to apply.

---

### Pitfall 4: Architecture Mismatch — npm Optional Dependencies vs. Electron Multi-Arch Builds

**What goes wrong:**
The recommended napi-rs distribution pattern is platform-specific npm packages installed via `optionalDependencies`. This works perfectly in a standard Node.js context. It fails in Electron when building for a target architecture that differs from the host machine's architecture. npm installs optional dependencies for the *current* machine's architecture, not the *target* Electron architecture. When electron-builder packages the app for `x64` while running on `arm64` macOS (or vice versa), the wrong `.node` binary ends up in the bundle.

The napi-rs issue tracker (node-rs issue #376) confirms: "npm optional modules are installed for the current nodejs architecture, not for the target Electron arch. None of the Electron tooling appears aware of binaries via optional dependencies."

**Why it happens:**
npm's platform/cpu optional dependency resolution predates Electron's multi-architecture build requirements. Electron tooling (electron-builder, electron-forge) has special-casing for node-gyp, but not for the napi-rs optional dependency pattern.

**How to avoid:**
For an app distributed via electron-builder, use the single-package approach instead of optional dependencies:
- Bundle all platform binaries in one package (using `@napi-rs/triples` for naming).
- Use a `postinstall` script or `napi-postinstall` to copy the correct binary at install time based on `process.platform` and `process.arch`.
- Configure electron-builder's `asarUnpack` to exclude `.node` files: `"asarUnpack": ["**/*.node"]`. Native `.node` files cannot run from inside an `.asar` archive — they must be on disk. This is true regardless of distribution method.
- Test the packaged build (not just `npm start`) on each target platform/architecture combination.

**Warning signs:**
- App works in development (`npm start`) but crashes after electron-builder packaging with "invalid ELF header" or "not a valid Win32 application" errors.
- macOS universal build only works on the architecture of the build machine.
- "Cannot find module" errors for the `.node` file when launched from the packaged `.asar`.

**Phase to address:**
Phase 4 (cross-platform packaging) — but the binary distribution strategy must be decided in Phase 1 scaffolding and cannot be easily changed later.

---

### Pitfall 5: IMAP/SMTP Connection Timeout with No Upper Bound

**What goes wrong:**
The C++ `napi_imap.cpp` calls `session->connectIfNeeded(&err)` — mailcore2 handles its own timeout internally. In the Rust replacement, `async-imap`'s `connect` function has no built-in timeout. If the IMAP server is reachable but slow to respond (firewalled destination, overloaded server, wrong port), the future will pend indefinitely. This hangs the onboarding UI: the "Checking connection..." spinner never stops, and the Promise never resolves or rejects.

This is documented in rust-imap's issue #167 and referenced in the async-imap ecosystem: "Long connection timeouts lead to unreliable behavior, and expiration of timeouts is never reliable."

**Why it happens:**
Rust's async networking (via tokio) follows the composable timeout pattern — you must explicitly wrap futures with `tokio::time::timeout`. Library authors do not add defaults because the appropriate timeout is application-specific. Developers porting from mailcore2 assume the library manages timeouts as mailcore2 did; it does not.

**How to avoid:**
Wrap every network future with an explicit timeout using `tokio::time::timeout`:
```rust
use tokio::time::{timeout, Duration};

let result = timeout(
    Duration::from_secs(15),
    async_imap::connect((hostname, port), tls_connector)
).await;

match result {
    Err(_elapsed) => return Err(ImapError::Timeout),
    Ok(Err(e)) => return Err(e.into()),
    Ok(Ok(client)) => client,
}
```
Apply the same pattern to: TLS handshake, CAPABILITY command, LOGIN/AUTHENTICATE command, LOGOUT. Set timeouts to 15 seconds for initial connect, 10 seconds for protocol commands. These mirror reasonable email client defaults. Expose timeout as a parameter if needed for tests.

**Warning signs:**
- Test that connects to `imap.gmail.com:9999` (unreachable port) hangs instead of returning an error after ~15 seconds.
- Onboarding spinner runs indefinitely when a user enters wrong port.
- CI IMAP tests randomly hang (flaky) — almost always a missing timeout.

**Phase to address:**
Phase 2 (IMAP implementation) — implement timeouts as a first-class requirement, not a polish item.

---

### Pitfall 6: XOAUTH2 SASL Encoding Errors

**What goes wrong:**
The XOAUTH2 initial client response format is:
```
base64("user=" + email + "\x01auth=Bearer " + token + "\x01\x01")
```
The `\x01` bytes are ASCII Control-A (byte value 1). Common mistakes:
1. Using the literal string `^A` instead of the byte `\x01`.
2. Using `base64url` encoding (RFC 4648 §5, URL-safe variant with `-` and `_` characters) instead of standard base64 (RFC 4648 §4, with `+` and `/`). IMAP servers expect standard base64.
3. Embedding whitespace or newlines in the base64 output (must be a single continuous string).
4. Not sending an empty response `\r\n` when the server returns an error challenge after a failed AUTHENTICATE attempt — this violates the IMAP protocol and leaves the connection in an undefined state, causing subsequent operations to fail silently.
5. Using the wrong OAuth scope: for IMAP, Google requires `https://mail.google.com/`; for Gmail IMAP admin delegation it's a different scope. Microsoft Exchange requires `https://outlook.office.com/IMAP.AccessAsUser.All`.

The Microsoft SMTP equivalent (`AUTH XOAUTH2`) has the same format but a different scope and has been the subject of multiple recent authentication failures (documented in Microsoft Q&A threads, 2023-2024).

**Why it happens:**
The XOAUTH2 spec (Google's documentation) shows the format but the byte `^A` looks like a typographical convention. Rust's `base64` crate has both standard and URL-safe variants; the wrong one is easy to pick accidentally. The empty-response requirement on error is buried in a footnote of the Gmail XOAUTH2 documentation.

**How to avoid:**
- Construct the SASL payload as a `Vec<u8>`, using literal `b'\x01'` separators — never string formatting with `^A`.
- Use `base64::engine::general_purpose::STANDARD.encode(...)` from the `base64` crate (not `URL_SAFE` or `URL_SAFE_NO_PAD`).
- After sending AUTHENTICATE XOAUTH2, check if the server response is a challenge (`+` followed by base64 error JSON) and send an empty line `\r\n` before returning an error to the caller.
- Write a unit test that constructs the XOAUTH2 payload for a known email/token pair and asserts exact byte equality against the reference output from Google's documentation.

**Warning signs:**
- OAuth2 IMAP/SMTP tests fail with "535 Authentication Credentials Invalid" or "NO AUTHENTICATE failed" only for OAuth2 paths, while password auth succeeds.
- Connection hangs after a failed XOAUTH2 attempt (missing empty response).
- Token encodes correctly when tested in isolation but fails against a real server (wrong base64 variant).

**Phase to address:**
Phase 2 (IMAP) and Phase 3 (SMTP/validator) — implement and unit-test XOAUTH2 encoding before integrating with live servers.

---

### Pitfall 7: Provider Regex Matching — Silent Mismatch for Edge Cases

**What goes wrong:**
The `providers.json` database uses domain-match patterns and MX-match patterns (e.g., `google.com`, `*.googlemail.com`, `aspmx.l.google.com`). The C++ `MailProvidersManager` implements its own matching logic. The Rust replacement must reproduce that logic exactly. Common failures:

1. **Subdomain matching**: A pattern like `googlemail.com` should match `user@googlemail.com` but not `user@notgooglemail.com`. If matching is done as a simple string `contains()`, false positives occur.
2. **MX record suffix matching**: MX records are matched by suffix against patterns in `mxMatch`. A pattern `google.com` should match `aspmx.l.google.com` but the matching must be anchored — `google.com` must not match `notgoogle.com`.
3. **Case sensitivity**: Email domains and MX hostnames should be compared case-insensitively (RFC 5321 specifies case-insensitive domain comparison).
4. **First-match vs. best-match**: If a providers.json entry has both `domainMatch` and `mxMatch`, the C++ code gives priority to the first match found in iteration order. Changing this order breaks provider detection for ambiguous domains.

**Why it happens:**
The matching rules are not formally specified anywhere — they are implicit in the mailcore2 C++ implementation. Developers writing the Rust replacement will naturally use `str::ends_with` or simple regex, which handles the common cases but fails on anchoring and case sensitivity.

**How to avoid:**
- Treat each entry in `domainMatch` as a suffix-anchored, case-insensitive pattern: domain must end with `.{pattern}` OR equal `{pattern}` exactly (to handle the root domain case).
- For `mxMatch`, apply the same suffix-anchored logic to the MX hostname, not the email domain.
- Write a test fixture covering: exact match, subdomain match, suffix-but-not-substring match (e.g., `google.com` must not match `notgoogle.com`), uppercase input, empty input, input with no `@` sign.
- Cross-validate: run the C++ addon's `providerForEmail` on the same 20 test addresses and compare output with the Rust implementation before removing the C++ code.

**Warning signs:**
- Provider detection works for `@gmail.com` but silently returns null for `@googlemail.com` or `@GMAIL.COM`.
- A provider that should be detected via MX records is not detected (check: is MX resolution actually happening? DNS failures can silently degrade to null).
- Provider detection regression reports from users after the C++ removal.

**Phase to address:**
Phase 1 (provider detection) — this is the first piece of logic to implement, and correctness must be established before IMAP/SMTP work begins.

---

### Pitfall 8: Error Messages Become Opaque After Rust-to-JS Propagation

**What goes wrong:**
The C++ addon uses `ErrorMessage::messageForError(err)` to convert mailcore2 error codes into human-readable strings (e.g., "Authentication failed", "Connection timeout"). These strings are displayed directly in the onboarding UI. In the Rust replacement, errors are propagated as `napi::Error` objects. If Rust errors are returned as `Err(Error::from_reason("Connection refused (os error 111)"))`, the JavaScript consumer sees the raw Rust OS error message — which is technical, inconsistent across platforms, and unfamiliar to users.

Worse: if a `?` operator propagates an IO error through multiple layers, the error reason may be `"tcp connect error: Connection refused (os error 111)"` with nested message structure that TypeScript callers are not expecting.

**Why it happens:**
napi-rs converts `Err(napi::Error)` into a JavaScript rejected Promise, where the `.message` property is whatever string was passed to `Error::from_reason()`. Developers focus on making errors propagate at all, not on making error messages match what the UI expects.

**How to avoid:**
- Define a domain-specific error enum in Rust (`ImapError`, `SmtpError`, `ValidationError`) with variants like `Timeout`, `AuthFailed`, `TlsHandshakeFailed`, `ConnectionRefused`, `ProviderNotFound`.
- Implement `Display` for each variant to produce the same human-readable strings as the C++ `ErrorMessage::messageForError()` output.
- Map low-level IO/TLS errors to domain errors at the boundary where they are caught, not at the napi export boundary.
- In napi-rs export functions, produce `napi::Error::from_reason(format!("{}", domain_error))` for user-facing errors, and preserve the raw error in debug logs.
- Write a test asserting that a connection timeout produces `{ success: false, error: "Connection timeout" }` (matching the C++ string) rather than `"deadline has elapsed"` (tokio's raw timeout message).

**Warning signs:**
- TypeScript consumers pattern-match on error messages (they likely do — check `onboarding-helpers.ts`).
- Onboarding UI shows "tcp connect error: Connection refused (os error 111)" instead of a user-friendly message.
- The `error` field in the returned object contains Rust internal details (file paths, tokio internals).

**Phase to address:**
Phase 2 (IMAP) — establish error mapping convention before any integration test. Harder to retrofit across 3 phases than to design once.

---

### Pitfall 9: Binary Size Bloat from Full Tokio and Unused Crate Features

**What goes wrong:**
A naive `Cargo.toml` that includes full tokio (`tokio = { version = "1", features = ["full"] }`) and does not disable unused features on lettre, trust-dns-resolver, or rustls will produce a `.node` binary of 15–25 MB on Linux. This is the binary that ships in the application bundle. The C++ mailcore2 addon (despite the large C++ codebase) was not significantly smaller because of vendored native dependencies, but the user-visible impact is: slow application startup (binary mapped into memory), large download size, and large application bundle.

Specific sources of bloat that have been measured in napi-rs projects:
- `tokio/full` includes `time`, `net`, `process`, `signal`, `fs`, `sync`, `rt-multi-thread` — the addon only needs `rt-multi-thread`, `net`, `time`, and `sync`.
- `lettre`'s `file-transport` and `sendmail-transport` features pull in path handling code that is never used.
- `trust-dns-resolver`'s `dns-over-https` and `dns-over-tls` features pull in significant TLS stack code beyond what rustls already provides.
- The `regex` crate's Unicode support tables are large; if provider patterns only use ASCII, `regex = { version = "1", default-features = false, features = ["std", "perf"] }` drops ~500KB.

**Why it happens:**
Cargo defaults to enabling all declared features. First-time napi-rs projects reach for `features = ["full"]` for tokio to make things "just work." The binary size is only visible at package time, not during development.

**How to avoid:**
After implementing all five functions, run `cargo bloat --release --crates` (install `cargo-bloat`) to identify the top contributors. Apply these specific `Cargo.toml` settings:
```toml
[profile.release]
opt-level = "z"      # size optimization
lto = true           # cross-crate dead code elimination
codegen-units = 1    # enables full LTO
strip = "symbols"    # remove debug symbols

[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "sync", "macros"] }
```
Disable unused features explicitly on each dependency. Target < 5 MB for the stripped release binary on Linux x64.

**Warning signs:**
- `cargo build --release` produces a `.node` > 15 MB before stripping.
- `cargo bloat` shows `regex-unicode-tables` or `h2` or `hyper` in the top 10 — these indicate features or dependencies that were pulled in transitively and are not needed.
- Application bundle size increases by > 20 MB per platform after switching from C++.

**Phase to address:**
Phase 3 (integration / cleanup) — size optimization is a final-pass task, but the structural decisions (tokio features, no-unicode regex) should be made during Phase 1 scaffolding to avoid lock-in.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| `tokio = { features = ["full"] }` | No missing-feature compile errors | 3–8 MB binary size increase, slower startup | Only during early prototyping; must be trimmed before release |
| `native-tls` instead of `rustls` | Familiar API, no feature flags to configure | Linux TLS conflicts with Electron's BoringSSL; system SSL version dependency | Never — rustls is the correct choice for Electron addons |
| Using `unwrap()` / `expect()` instead of `?` on N-API boundary | Faster initial code | Panics propagate to SIGABRT, not JavaScript exceptions; loses error context | Never — all napi export functions must return `Result<T, napi::Error>` |
| Skipping per-call timeout on IMAP/SMTP | Simpler code | Promises hang indefinitely on bad server configs; hangs onboarding UI | Never — timeouts are mandatory |
| Returning raw OS error strings | Simplest error propagation | UI shows `"tcp connect error: os error 111"`; breaks any TypeScript error message parsing | Never after the provider API is defined |
| Bundling all platform binaries in a single npm package | Avoids optional-dependency arch mismatch | Larger node_modules per developer install | Acceptable for Electron apps where users download a packaged binary, not raw npm |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Electron asar packaging | Including `.node` binary inside the `.asar` archive | Always configure `asarUnpack: ["**/*.node"]` in electron-builder; native modules cannot be dlopen'd from inside an archive |
| napi-rs in Electron main process | Loading addon in renderer process | Load only in main process or a dedicated utility process; the existing `mailsync-process.ts` pattern is correct |
| async-imap + STARTTLS | Connecting on port 143 with TLS connector directly | Port 143 is plain TCP; must connect without TLS then call `STARTTLS` to upgrade; port 993 uses implicit TLS from the start |
| lettre SMTP + OAuth2 | Setting credentials without disabling PLAIN/LOGIN auth fallback | Explicitly configure the SMTP transport to use only XOAUTH2 mechanism when token is provided; mixed auth configuration causes servers to pick wrong mechanism |
| DNS MX resolution | Using `trust-dns-resolver`'s async API with `tokio::spawn` in napi async fn | The tokio runtime in napi-rs is the same runtime being used by trust-dns; do not spawn a new runtime inside an async fn — use `.await` directly |
| providers.json path resolution | Using relative path from `process.cwd()` | Resolve relative to the `.node` file location using `__dirname` equivalent (pass path from JS consumer), not CWD; CWD is unpredictable in Electron |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Compiling `regex::Regex` patterns on each `providerForEmail()` call | CPU spike on every keypress in email input field | Compile all provider regexes once at `registerProviders()` time and store in a `Vec<(Regex, ProviderId)>` | At any call frequency; regex compilation is 10–100x slower than matching |
| DNS MX lookup blocking provider detection | 3–5 second delay before provider auto-detection result appears | Run MX lookup in parallel with domain matching; return domain-match result immediately, update with MX-match result async | Any time DNS is slow (corporate networks, slow resolvers) |
| Spawning a new tokio runtime per function call | Overhead accumulates; tokio runtime creation is expensive | Use napi-rs's built-in tokio feature which provides a shared runtime per process | Measurable degradation after ~100 calls in a session |
| Cloning `providers.json` data per lookup | Memory grows with provider count (500+) | Load once, share via `Arc<ProviderDatabase>` or static initialization; never clone the full database | At 500+ providers, each clone is measurable |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Logging OAuth2 tokens in error messages | Tokens appear in log files and Electron crash reports | Error messages must never include the token value; log `"OAuth2 token present: true"` not the token itself |
| Accepting self-signed certificates silently | MITM attacks during IMAP/SMTP connection testing | In rustls, accept system roots only; do not call `dangerous().disable_certificate_verification()` even for testing |
| Returning raw IMAP server banners in error messages | Server identification information exposed to JavaScript layer (minor) | Strip IMAP greeting and banner from error messages returned to JS; include only the parsed error condition |
| Storing passwords in Rust static variables | Credentials survive beyond request lifetime | Ensure credentials are passed per-call, not stored in global state; napi-rs `#[napi]` fn arguments are stack-allocated and dropped after function returns |

---

## "Looks Done But Isn't" Checklist

- [ ] **IMAP connection test:** Addon connects and gets `{ success: true }` for Gmail with password auth — verify it also works with XOAUTH2 token and returns correct capabilities (`idle`, `condstore`, `xoauth2`, `gmail`).
- [ ] **SMTP connection test:** Addon connects to `smtp.gmail.com:587` with STARTTLS — verify it also tests port 465 with implicit TLS and returns correct `{ success: true/false }`.
- [ ] **Provider detection:** `providerForEmail("user@gmail.com")` returns provider — verify `providerForEmail("user@googlemail.com")` also returns the same provider (MX match), and `providerForEmail("user@nonexistent-domain-xyz.com")` returns null without crashing.
- [ ] **Timeout behavior:** All three async functions (validateAccount, testIMAPConnection, testSMTPConnection) reject with a timeout error within 15 seconds when given `hostname: "192.0.2.1"` (an unreachable IP per RFC 5737).
- [ ] **Electron load:** The `.node` binary loads cleanly when `require()`'d from Electron main process — not just from plain `node`.
- [ ] **asar exclusion:** After electron-builder packaging, the `.node` file exists in `app.asar.unpacked/`, not embedded in `app.asar`.
- [ ] **API compatibility:** TypeScript consumers `onboarding-helpers.ts` and `mailsync-process.ts` compile without type errors against the napi-rs-generated `.d.ts` file.
- [ ] **Error strings match:** A failed authentication attempt returns `{ success: false, error: "Authentication failed" }` with the same string format the TypeScript consumers expect (check for any pattern matching on error strings in the consumers).
- [ ] **Cross-platform binary sizes:** Release binary is < 8 MB stripped on Linux x64, < 10 MB on macOS arm64, < 8 MB on Windows x64.

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| Tokio runtime crash in Electron | MEDIUM | Pin to latest napi-rs; add Electron integration test; add `timeout()` wrappers |
| TLS library conflict (native-tls on Linux) | MEDIUM | Audit `Cargo.lock` for `openssl-sys`; replace `native-tls` feature flags with `rustls` on lettre and async-imap; rebuild and retest |
| Wrong binary in Electron package | LOW | Update electron-builder `asarUnpack` config; switch from optionalDependencies to single-package with copy script; rebuild package |
| XOAUTH2 encoding wrong | LOW | Fix base64 variant; fix `\x01` separator; add empty-response for error challenge; add unit test |
| Provider regex false positives/negatives | MEDIUM | Cross-validate against C++ addon output on 50 representative addresses before removing C++ code; treat regex as a port, not a rewrite |
| Binary size > 15 MB | LOW | Apply `profile.release` optimization flags; audit tokio features; run `cargo bloat` |
| Opaque error messages | LOW | Add error enum with Display impl; map at domain boundary; update unit tests to assert message strings |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| Tokio runtime lifecycle mismatch | Phase 1: Scaffolding | Addon loads in Electron main process without panic; all 5 functions callable |
| TLS library conflict (BoringSSL) | Phase 1: Scaffolding | `cargo tree \| grep openssl` returns nothing; IMAP TLS handshake succeeds inside Electron on Linux |
| V8 Memory Cage / external ArrayBuffer | Phase 1: Scaffolding | No `create_external_arraybuffer` in codebase; code review before merge |
| Architecture mismatch in packaging | Phase 1: Scaffolding (strategy) + Phase 4: Packaging (implementation) | Packaged build opens correctly on x64 and arm64 |
| IMAP/SMTP timeout | Phase 2: IMAP + Phase 3: SMTP | Test with unreachable host rejects within 15 seconds |
| XOAUTH2 encoding | Phase 2: IMAP + Phase 3: SMTP | Unit test matches Google's reference encoding; OAuth2 IMAP login succeeds against Gmail |
| Provider regex edge cases | Phase 1: Provider detection | 50-address cross-validation test against C++ addon output passes |
| Opaque error messages | Phase 2: IMAP | Unit test asserts error message string; TypeScript consumer compilation passes |
| Binary size bloat | Phase 3: Integration/cleanup | Stripped release binary < 8 MB on Linux; `cargo bloat` reviewed |

---

## Sources

- [napi-rs changelog (official)](https://napi.rs/changelog/napi) — tokio runtime fixes, Electron external TypedArray fix, Windows GetProcAddress fix
- [napi-rs issue #1175: thread_local breaks Electron Windows](https://github.com/napi-rs/napi-rs/issues/1175)
- [napi-rs issue #2460: SIGABRT on terminated worker thread](https://github.com/napi-rs/napi-rs/issues/2460)
- [napi-rs issue #945: napi_env is not Send](https://github.com/napi-rs/napi-rs/issues/945)
- [napi-rs issue #1346: Cannot allocate Buffer in Chromium V8](https://github.com/napi-rs/napi-rs/issues/1346)
- [Electron blog: V8 Memory Cage (June 2022)](https://www.electronjs.org/blog/v8-memory-cage) — external ArrayBuffer prohibition from Electron 21+
- [Electron issue #13176: Addons cannot use bundled OpenSSL](https://github.com/electron/electron/issues/13176)
- [napi-rs/node-rs issue #376: arch mismatch with optional dependencies and Electron](https://github.com/napi-rs/node-rs/issues/376)
- [Google XOAUTH2 Protocol specification](https://developers.google.com/gmail/imap/xoauth2-protocol) — exact format, base64 requirements, error challenge handling
- [RFC 7628: SASL Mechanisms for OAuth](https://www.rfc-editor.org/rfc/rfc7628.html)
- [rust-imap issue #167: no built-in connection timeout](https://github.com/jonhoo/rust-imap/issues/167)
- [async-imap GitHub (chatmail fork)](https://github.com/chatmail/async-imap) — current maintained fork of async-imap
- [napi-rs cross-build documentation](https://napi.rs/docs/cross-build)
- [min-sized-rust: binary size optimization techniques](https://github.com/johnthagen/min-sized-rust)
- [Electron native node modules documentation](https://www.electronjs.org/docs/latest/tutorial/using-native-node-modules)

---
*Pitfalls research for: Rust napi-rs native addon replacing C++ mailcore N-API addon in Electron*
*Researched: 2026-03-01*
