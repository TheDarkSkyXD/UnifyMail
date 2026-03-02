# Stack Research

**Domain:** Rust napi-rs N-API addon for Electron email client (IMAP/SMTP connection testing, provider detection)
**Researched:** 2026-03-01
**Confidence:** HIGH (core crates verified via docs.rs; cross-compilation tooling verified via official napi-rs docs)

---

## Scope

This file covers ONLY the new Rust crate layer. The existing TypeScript/React/Electron stack is validated and unchanged. The five exported functions being replaced are:

| Function | Sync/Async | What Rust Must Do |
|----------|------------|-------------------|
| `registerProviders(jsonPath)` | Sync | Load + parse providers.json, store in-process |
| `providerForEmail(email)` | Sync | Domain/MX regex match against loaded providers |
| `validateAccount(opts)` | Async (Promise) | MX lookup → provider match → test IMAP → test SMTP |
| `testIMAPConnection(opts)` | Async (Promise) | TCP+TLS connect, send CAPABILITY, parse response |
| `testSMTPConnection(opts)` | Async (Promise) | TCP+TLS connect, send EHLO, optional AUTH test |

---

## Recommended Stack

### Core N-API Framework

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `napi` | **3.8.3** (latest stable, 2026-02-14) | Rust bindings for Node-API | Active development on v3, wasm-compatible, simplified ThreadsafeFunction API vs v2. napi4 default enables async Promises. |
| `napi-derive` | **3.0.0+** (matches napi major) | `#[napi]` proc-macro for auto-generating TS types | Required companion to `napi` — generates the `.d.ts` types that replace the hand-written `types/index.d.ts` |
| `napi-build` | **2.x** | build.rs helper for N-API linking | One-line `build.rs`: `napi_build::setup()`. Required for all napi-rs addons. |

**Minimum Rust version:** 1.88.0 (required by napi 3.8.3; verify with `rustup update stable`).

**Node.js compatibility:** Electron 39 bundles Node.js 22.20.0. napi-rs with `napi4` feature supports Node 10.6.0+, so Node 22 is fully covered. N-API is ABI-stable — the compiled `.node` file works across Node versions without recompile.

**Required Cargo.toml features for this project:**

```toml
[dependencies]
napi = { version = "3", features = ["napi4", "async", "tokio_rt"] }
napi-derive = "3"

[build-dependencies]
napi-build = "2"
```

- `napi4` — Enables N-API 4 (threadsafe functions, Promises). Default feature but explicit is clearer.
- `async` — Enables `async fn` → JavaScript Promise conversion. Internally enables `tokio_rt`.
- `tokio_rt` — Spins up the shared tokio runtime that napi-rs manages. Required when any exported function is `async`.

Do NOT enable `full` — it pulls in `napi9`, `serde-json` (unnecessary bloat), `latin1`, `chrono_date`, `experimental`. Only enable what you need.

### Async Runtime

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `tokio` | **1.x** (latest: ~1.44) | Async runtime for IMAP/SMTP/DNS I/O | napi-rs v3 manages a shared tokio runtime automatically when `tokio_rt` feature is enabled. Do NOT create your own `#[tokio::main]` — let napi-rs own the runtime. |

**Required Cargo.toml features:**

```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "io-util", "macros"] }
```

- `rt-multi-thread` — Multi-threaded executor for concurrent connection tests.
- `net` — `TcpStream` for raw socket connections.
- `time` — Timeouts via `tokio::time::timeout()`.
- `io-util` — `AsyncReadExt`/`AsyncWriteExt` for IMAP line reading.
- `macros` — `tokio::select!`, `tokio::join!` for concurrent IMAP+SMTP testing in `validateAccount`.

### TLS

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `tokio-rustls` | **0.26.4** | Async TLS over tokio TCP streams | Pure Rust, no OpenSSL dependency, works on all three target platforms without system library requirements. Uses rustls 0.23.x. |
| `rustls-platform-verifier` | **0.6.2** | OS-native certificate validation | Validates TLS certs using Windows CertStore, macOS Security.framework, Linux system CAs. Critical for enterprise environments. Preferred over `rustls-native-certs` for revocation (OCSP/CRL) support. Used by Signal, 1Password, Bitwarden. |
| `rustls` | **0.23.x** (transitive via tokio-rustls) | TLS protocol implementation | Do not add directly; pulled in by tokio-rustls. |

**Required Cargo.toml:**

```toml
[dependencies]
tokio-rustls = "0.26"
rustls-platform-verifier = "0.6"
```

**Why NOT native-tls:** `native-tls` links against OpenSSL on Linux (requiring the dev package), Secure Transport on macOS, and SChannel on Windows. Eliminates pure-Rust build. rustls avoids all system TLS library dependencies.

**STARTTLS implementation note:** async-imap does not handle STARTTLS internally for all cases — you will need to manually wrap the `TcpStream` with `TlsConnector` after the STARTTLS handshake. This is ~20 lines of manual Rust code but keeps the dependency count low.

### IMAP Client

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `async-imap` | **0.11.2** | Async IMAP client for capability detection | The only maintained async IMAP library for Rust. Handles RFC 3501 command/response parsing. Used by delta.chat/chatmail projects. |

**Required Cargo.toml:**

```toml
[dependencies]
async-imap = { version = "0.11", features = ["runtime-tokio"] }
```

- `runtime-tokio` — Required. Switches async-imap from its default `async-std` runtime to tokio. Without this flag, async-imap uses `async-std` which conflicts with napi-rs's tokio runtime.
- The crate does NOT have built-in rustls/native-tls features — TLS wrapping is done externally by passing a `tokio_rustls::TlsStream<TcpStream>` to `async_imap::Client::new()`.

**What it replaces:** `napi_imap.cpp` / `MCIMAPSession`. You only need `CAPABILITY` after login (no full IMAP client needed). The `async-imap` Session's `capabilities()` method returns the `CAPABILITY` list directly.

**Capabilities to detect** (from existing `types/index.d.ts` and C++ code):
- IDLE, CONDSTORE, QRESYNC, COMPRESS=DEFLATE, XOAUTH2, X-GM-EXT-1 (Gmail)

### SMTP Client

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `lettre` | **0.11.19** (latest stable) | SMTP connection testing | Mature library, supports XOAUTH2 SASL mechanism (confirmed in SMTP transport docs), async tokio, rustls TLS. Only library in Rust ecosystem with XOAUTH2 support built-in. |

**Required Cargo.toml:**

```toml
[dependencies]
lettre = { version = "0.11", default-features = false, features = [
    "tokio1",          # async runtime
    "rustls-tls",      # rustls TLS backend
] }
```

- `tokio1` — Enables `AsyncSmtpTransport<Tokio1Executor>`.
- `rustls-tls` — TLS via rustls (consistent with the rest of the stack). Do NOT enable `native-tls` (OpenSSL dependency on Linux).
- `default-features = false` — Disables the default `file-transport` and other features not needed for connection testing only.

**XOAUTH2 support:** lettre's SMTP transport implements `AUTH PLAIN`, `AUTH LOGIN`, and `AUTH XOAUTH2` mechanisms (RFC 4954). XOAUTH2 is constructed by passing a pre-obtained OAuth2 token — lettre encodes the SASL exchange correctly.

**What it replaces:** `napi_smtp.cpp` / `MCSmtpSession`. For connection testing, use `SmtpTransport::test_connection()` which connects and sends EHLO without sending any message.

### DNS Resolver (MX Lookups)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `hickory-resolver` | **0.25.2** (latest stable, March 2025) | Async MX record lookups for provider matching | Direct rename/successor to `trust-dns-resolver` (rebranded at v0.24). Pure Rust, tokio-native, supports `lookup_mx()`. |

**Required Cargo.toml:**

```toml
[dependencies]
hickory-resolver = { version = "0.25", features = ["tokio-runtime"] }
```

- `tokio-runtime` — Enables `TokioResolver` and `TokioConnectionProvider`. Required for async operation.

**Do NOT use `trust-dns-resolver`** — the crate was renamed to `hickory-resolver` at v0.24 and is no longer updated. Using the old name causes confusion and may pull stale code.

**MX lookup pattern:**

```rust
use hickory_resolver::Resolver;
use hickory_resolver::config::*;

let resolver = Resolver::builder_tokio()
    .unwrap()
    .build();
let mx_lookup = resolver.mx_lookup("example.com.").await?;
for mx in mx_lookup.iter() {
    // mx.exchange() is the MX hostname
    // mx.preference() is the priority
}
```

**What it replaces:** The MX-based provider matching in `napi_validator.cpp` that uses mailcore2's `MailProvidersManager`. In the Rust rewrite, MX lookups feed into the same domain-matching logic reimplemented with regex/string matching against the loaded providers.json.

### JSON Parsing (Provider Database)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `serde` | **1.x** (latest: ~1.219) | Serialization framework | Industry standard. Required by serde_json. |
| `serde_json` | **1.0.149** (released 2026-01-06) | Parse providers.json at startup | Sole option for JSON in Rust. 614M total downloads. The `providers.json` file (39KB, 500+ entries) fits comfortably in memory and is parsed once on `registerProviders()` call. |

**Required Cargo.toml:**

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- `derive` feature — Required for `#[derive(Deserialize)]` on provider structs.
- No special `serde_json` features needed — the default JSON parsing is sufficient for the providers.json structure.

**What it replaces:** mailcore2's custom provider XML/JSON loading in `MCMailProvidersManager`.

---

## Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `napi` CLI (`@napi-rs/cli`) | Scaffold project, build, and generate TypeScript bindings | `npm install -g @napi-rs/cli` then `napi new` to scaffold. Use `napi build --release` instead of `cargo build` — it handles the `.node` output renaming and TS type generation. |
| `cargo-xwin` | Cross-compile Windows MSVC targets from Linux/macOS CI | `cargo install cargo-xwin`. Needed only in CI — Windows builds done natively on `windows-latest` runners. |
| `cargo-zigbuild` | Cross-compile Linux aarch64 targets using Zig linker | `cargo install cargo-zigbuild`. Useful for Linux ARM64 (aarch64-unknown-linux-gnu) cross-compilation from x86_64. |
| `rustup target add` | Add cross-compilation targets | See targets table below. |

---

## Cross-Compilation Targets

| Target Triple | Platform | Build Method | CI Runner |
|---------------|----------|--------------|-----------|
| `x86_64-pc-windows-msvc` | Windows x64 | Native MSVC | `windows-latest` |
| `x86_64-apple-darwin` | macOS Intel | Native Clang | `macos-latest` |
| `aarch64-apple-darwin` | macOS Apple Silicon | Native Clang (universal) | `macos-latest` |
| `x86_64-unknown-linux-gnu` | Linux x64 | Native GCC | `ubuntu-latest` |
| `aarch64-unknown-linux-gnu` | Linux ARM64 | cargo-zigbuild | `ubuntu-latest` (cross) |

**macOS universal binary:** napi-rs supports building a universal `arm64+x86_64` macOS binary with `--target universal2-apple-darwin` in the napi CLI. Preferred over shipping two separate macOS packages.

**CI workflow:** `napi new` generates a `.github/workflows/CI.yml` that handles all targets out of the box. The key sections per platform:

```yaml
# macOS universal
- run: napi build --platform --release --target universal2-apple-darwin

# Linux aarch64 (via zigbuild)
- run: cargo install cargo-zigbuild && napi build --platform --release --target aarch64-unknown-linux-gnu --use-napi-cross

# Windows x64 (native)
- run: napi build --platform --release
```

**Binary distribution:** napi-rs publishes per-platform npm packages (e.g., `mailcore-napi-win32-x64-msvc`, `mailcore-napi-darwin-universal`) as `optionalDependencies` of the main `mailcore-napi` package. The generated `index.js` loader selects the correct binary at runtime. Use `napi-postinstall` as the `postinstall` script to handle legacy npm version edge cases.

---

## Complete Cargo.toml Template

```toml
[package]
name = "mailcore-napi"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
# N-API framework
napi = { version = "3", features = ["napi4", "async", "tokio_rt"] }
napi-derive = "3"

# Async runtime
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "io-util", "macros"] }

# TLS
tokio-rustls = "0.26"
rustls-platform-verifier = "0.6"

# IMAP
async-imap = { version = "0.11", features = ["runtime-tokio"] }

# SMTP
lettre = { version = "0.11", default-features = false, features = ["tokio1", "rustls-tls"] }

# DNS
hickory-resolver = { version = "0.25", features = ["tokio-runtime"] }

# JSON / providers.json
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[build-dependencies]
napi-build = "2"

[profile.release]
lto = true
strip = "symbols"
```

---

## Installation

```bash
# Install napi CLI globally
npm install -g @napi-rs/cli

# Scaffold the new Rust project inside app/mailcore-rust/
napi new

# When prompted:
#   Package name: mailcore-napi
#   Dir name: . (or mailcore-rust)
#   Targets: win32-x64-msvc, darwin-universal, linux-x64-gnu, linux-arm64-gnu
#   Enable GitHub Actions: yes

# Add cross-compilation tools
cargo install cargo-xwin       # Windows cross-compile from Linux CI
cargo install cargo-zigbuild   # Linux aarch64 cross-compile

# Add required rust targets
rustup target add x86_64-pc-windows-msvc
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
rustup target add aarch64-unknown-linux-gnu

# Dev build (debug, runs on host)
napi build

# Release build (host platform)
napi build --release

# Release build (specific target)
napi build --release --target aarch64-apple-darwin
```

---

## Alternatives Considered

| Recommended | Alternative | Why Not Alternative |
|-------------|-------------|---------------------|
| `napi` v3 (3.8.3) | `napi` v2 (2.16.x) | v2 has more complex ThreadsafeFunction API; v3 is the current branch receiving fixes. The napi-rs package template now defaults to v3. |
| `rustls` + `tokio-rustls` | `native-tls` | native-tls requires OpenSSL dev headers on Linux (breaks clean Rust builds), system TLS varies by distro. rustls is fully self-contained. |
| `rustls-platform-verifier` | `rustls-native-certs` | platform-verifier uses OS trust decisions including OCSP/CRL revocation; native-certs only reads root CAs without revocation. For an email client handling user credentials, revocation matters. |
| `hickory-resolver` | `trust-dns-resolver` | trust-dns-resolver was renamed to hickory-resolver at v0.24; old crate is unmaintained. |
| `hickory-resolver` | `system resolver (getaddrinfo)` | System resolver doesn't expose MX record queries. MX records are needed for provider matching in `validateAccount`. |
| `async-imap` | `imap` (sync crate) | `imap` (jonhoo/rust-imap) is synchronous — running it on a blocking thread in napi-rs works but wastes a thread per connection. async-imap integrates with the same tokio runtime napi-rs already manages. |
| `lettre` | Manual SMTP implementation | lettre's built-in XOAUTH2 SASL mechanism saves ~200 lines of SASL encoding code. Only library in Rust with XOAUTH2 built-in. |
| `serde_json` | `simd-json`, `json5` | serde_json is the de-facto standard. providers.json is standard JSON (not JSON5), and 39KB is not a performance-sensitive parse. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `node-gyp` | Being eliminated — the entire point of this rewrite | napi-rs with `napi build` |
| `node-addon-api` (C++) | C++ addon being replaced | napi-rs Rust |
| `native-tls` crate | OpenSSL system dependency on Linux breaks clean Rust cross-compilation | `tokio-rustls` + `rustls-platform-verifier` |
| `trust-dns-resolver` crate | Unmaintained; rebranded to `hickory-resolver` at v0.24 | `hickory-resolver = "0.25"` |
| `async-std` runtime | Conflicts with napi-rs's tokio runtime. async-imap defaults to async-std — must set `runtime-tokio` feature | tokio |
| `napi::full` feature | Pulls in `napi9`, `serde-json`, `latin1`, `chrono_date`, `experimental` — none needed here | Only enable `napi4`, `async`, `tokio_rt` |
| `tokio::main` macro | napi-rs manages its own tokio runtime. Creating a second runtime panics or leaks | Let napi-rs own the runtime; use `async fn` with `#[napi]` |
| Custom TypeScript `.d.ts` | The existing `types/index.d.ts` will be replaced by napi-derive auto-generation | `napi-derive` `#[napi]` macro generates correct typings |
| `mailcore2` (vendored C++) | The entire mailcore2 dependency tree (~100 C++ source files) is eliminated | Purpose-built Rust crates |

---

## Stack Patterns by Variant

**If testing IMAP with TLS on port 993:**
- Connect `TcpStream`, wrap with `tokio-rustls` `TlsConnector`, pass to `async_imap::Client::new()`
- Call `client.login(username, password)` then `session.capabilities()`

**If testing IMAP with STARTTLS on port 143:**
- Connect plain `TcpStream`, pass to `async_imap::Client::new()`
- Call `client.starttls(&tls_connector)` to upgrade
- Then proceed with login and CAPABILITY

**If testing SMTP with XOAUTH2:**
- Use `lettre::AsyncSmtpTransport::relay(host)` with `.credentials(Credentials::new_xoauth2(user, token))`

**If running `validateAccount` concurrently:**
- Use `tokio::join!(test_imap_future, test_smtp_future)` to test both in parallel rather than sequentially (halves round-trip time)

**If providers.json changes at runtime:**
- Use `std::sync::RwLock<Option<Vec<Provider>>>` (wrapped in `lazy_static` or `once_cell::Lazy`) to hold parsed providers
- `registerProviders` takes a write lock; `providerForEmail` takes a read lock (zero contention in practice since registerProviders is called once at startup)

---

## Version Compatibility

| Package | Version | Compatible With | Notes |
|---------|---------|-----------------|-------|
| `napi` | 3.8.3 | Node.js 10.6.0+ (napi4), Electron 39 (Node 22.20.0) | ABI-stable — compiled `.node` binary works across Node versions |
| `napi-derive` | 3.x | Must match `napi` major version | Breaking API changes between major versions |
| `napi-build` | 2.x | napi 2.x and 3.x | Major version 2 of napi-build works with both napi v2 and v3 |
| `tokio-rustls` | 0.26.4 | rustls 0.23.x (transitive) | Do not pin rustls version directly; let tokio-rustls select it |
| `rustls-platform-verifier` | 0.6.2 | rustls 0.23.x, tokio-rustls 0.26.x | Verify compatible rustls version with `cargo tree` if conflicts arise |
| `async-imap` | 0.11.2 | tokio 1.x (with `runtime-tokio` feature) | Must use `runtime-tokio` feature or async-std conflict occurs |
| `lettre` | 0.11.19 | tokio 1.x (with `tokio1` feature) | 0.11.x is the stable branch; 0.10.x-alpha was a pre-release |
| `hickory-resolver` | 0.25.2 | tokio 1.x (with `tokio-runtime` feature) | 0.26.0-alpha.1 available but avoid alphas in production |

---

## Sources

- [docs.rs/crate/napi/latest](https://docs.rs/crate/napi/latest) — napi 3.8.3 features list verified (HIGH confidence)
- [napi.rs changelog](https://napi.rs/changelog/napi) — Version 3.8.3 confirmed as latest stable (HIGH confidence)
- [napi.rs/docs/concepts/async-fn](https://napi.rs/docs/concepts/async-fn) — async feature + tokio_rt integration (HIGH confidence)
- [napi-rs/package-template Cargo.toml](https://github.com/napi-rs/package-template/blob/main/Cargo.toml) — Template defaults napi 3.0.0 (HIGH confidence)
- [napi.rs/docs/cross-build](https://napi.rs/docs/cross-build) — Cross-compilation with cargo-xwin and cargo-zigbuild (HIGH confidence)
- [docs.rs/async-imap](https://docs.rs/async-imap/latest/async_imap/) — Version 0.11.2, runtime-tokio feature (HIGH confidence)
- [lib.rs/crates/async-imap/features](https://lib.rs/crates/async-imap/features) — Feature flags: default=runtime-async-std, runtime-tokio (HIGH confidence)
- [docs.rs/lettre/latest](https://docs.rs/lettre/latest/lettre/) — Version 0.11.19, tokio1 + rustls + XOAUTH2 (HIGH confidence)
- [docs.rs/hickory-resolver](https://docs.rs/hickory-resolver/latest/hickory_resolver/) — Version 0.25.2, tokio-runtime feature (HIGH confidence)
- [docs.rs/crate/rustls-platform-verifier](https://docs.rs/crate/rustls-platform-verifier/latest) — Version 0.6.2, platform support (HIGH confidence)
- [docs.rs/tokio-rustls](https://docs.rs/tokio-rustls/latest/tokio_rustls/) — Version 0.26.4, rustls 0.23.x (HIGH confidence)
- [docs.rs/serde_json](https://docs.rs/serde_json) — Version 1.0.149 (HIGH confidence)
- [Electron 39 release notes](https://xiuerold.medium.com/electron-39-a-quiet-evolution-of-the-modern-runtime-11a079fd8517) — Bundles Node.js 22.20.0 (MEDIUM confidence; cross-reference electronjs.org)

---

*Stack research for: Rust napi-rs mailcore N-API addon rewrite*
*Researched: 2026-03-01*
