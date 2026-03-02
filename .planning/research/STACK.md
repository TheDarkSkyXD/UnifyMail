# Stack Research

**Domain:** Rust standalone binary sync engine replacing C++ mailsync (~16,200 LOC) for IMAP/SMTP/CalDAV/CardDAV email synchronization
**Milestone:** v2.0 — Rewrite mailsync Engine in Rust
**Researched:** 2026-03-02
**Confidence:** HIGH (core crates verified against docs.rs and official documentation; CONDSTORE/QRESYNC support gap in async-imap confirmed via source inspection)

---

## Scope

This file covers ONLY new Rust crates needed for the v2.0 sync engine binary. The existing TypeScript/React/Electron stack is unchanged. The v1.0 napi-rs addon (mailcore-napi) is a separate concern covered in the v1.0 STACK.md.

The sync engine is a standalone Rust binary (not a Node.js addon) that:
- Receives JSON commands via stdin (task queue, wake-workers, need-bodies)
- Emits newline-delimited JSON deltas via stdout (persist/unpersist model changes)
- Runs one process per email account
- Manages three concurrent threads: background folder sync, foreground IDLE, CalDAV/CardDAV sync
- Handles its own async runtime lifecycle independently of Node.js

### C++ Dependency Mapping

The C++ engine used these vendored libraries that the Rust engine must replace:

| C++ Vendor Library | What It Did | Rust Replacement |
|--------------------|-------------|------------------|
| `mailcore2` (IMAP) | IMAPSession, folder sync, CONDSTORE, IDLE | `async-imap` (IDLE, CONDSTORE partial) |
| `libetpan` (IMAP/SMTP) | Low-level protocol transport | `async-imap` + `lettre` |
| `SQLiteCpp` | SQLite C++ wrapper | `rusqlite` |
| `nlohmann-json` | JSON parsing | `serde_json` |
| `icalendarlib` | iCalendar parsing | `calcard` |
| `spdlog` | Logging | `tracing` + `tracing-subscriber` |
| vcpkg: OpenSSL, curl, libxml2 | TLS, HTTP, DAV XML | `rustls`, `reqwest`, `quick-xml` |
| vcpkg: tidy-html5 | HTML sanitization | `ammonia` |
| vCard parsing (inline) | Contact data | `calcard` |

---

## Recommended Stack

### Core Runtime

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `tokio` | **1.x** (~1.44 latest) | Async runtime for all I/O | Industry standard for async Rust; work-stealing executor handles the three concurrent sync threads. Use `rt-multi-thread` for the background folder iterator + foreground IDLE + CalDAV threads. No napi-rs runtime constraint here — this binary owns its own runtime via `#[tokio::main]`. |
| `clap` | **4.5.60** | CLI argument parsing (`--identity`, `--account`, `--mode`) | The C++ engine takes `--identity`, `--account`, `--mode` args. clap derive API maps cleanly to a typed struct, gives auto-generated `--help`, and is the de facto standard. |

**Required Cargo.toml features:**
```toml
[dependencies]
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "io-util", "macros", "sync", "fs"] }
clap = { version = "4", features = ["derive"] }
```

- `rt-multi-thread` — Multi-threaded executor for concurrent sync/IDLE/DAV threads
- `sync` — `tokio::sync::mpsc` for cross-thread task dispatch, `RwLock` for shared state
- `fs` — Async file I/O for reading config files
- `macros` — `tokio::select!` for concurrent IDLE monitoring

Note: Unlike the v1.0 napi-rs addon where napi-rs managed the tokio runtime, this binary creates its own runtime via `#[tokio::main]`. No tokio ownership conflicts exist.

### IPC Protocol (stdin/stdout JSON)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `serde` | **1.x** (~1.219) | Serialization framework | Required by serde_json and all model derivations. |
| `serde_json` | **1.x** (~1.0.149) | JSON encode/decode for stdin commands and stdout deltas | The only choice for JSON in Rust. The existing protocol uses newline-delimited JSON — `serde_json::from_str()` per line for commands, `serde_json::to_string()` + `\n` for delta emission. |

The IPC protocol pattern is straightforward: stdin is read line-by-line via `tokio::io::BufReader::new(tokio::io::stdin())` with `.lines()` AsyncBufReadExt, each line parsed as JSON. Stdout writes are emitted via `tokio::io::stdout()` with `AsyncWriteExt::write_all`. No external IPC crate is needed.

**Required Cargo.toml features:**
```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

### IMAP Client

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `async-imap` | **0.11.2** | IMAP folder sync, IDLE, CONDSTORE, capability detection | The only maintained async IMAP client for Rust. Maintained by chatmail/deltachat team. Supports: IDLE (RFC 2177), QUOTA (RFC 2087), ID (RFC 2971), and **CONDSTORE via `select_condstore()`** (RFC 7162 SELECT parameter). Used in production by Delta Chat. |

**Critical finding:** `async-imap` has `select_condstore()` (verified via source inspection of `client.rs`). QRESYNC full extension support (RFC 7162 Section 3) is **not implemented** — there is no `enable_qresync()` method. The C++ engine used CONDSTORE but its QRESYNC usage is limited to SELECT modifiers. The Rust engine should use CONDSTORE for incremental sync and implement QRESYNC parameter passing via raw command extension if needed.

**Extensions supported by async-imap 0.11.2:**
- `extensions::idle` — IDLE command (RFC 2177), key for foreground IDLE thread
- `extensions::compress` — DEFLATE compression (feature-gated with `"compress"` flag)
- `extensions::quota` — GETQUOTA/GETQUOTAROOT (RFC 2087)
- `extensions::id` — ID command (RFC 2971)
- `select_condstore()` — SELECT with CONDSTORE parameter for modseq-based incremental sync

**Required Cargo.toml features:**
```toml
[dependencies]
async-imap = { version = "0.11", features = ["runtime-tokio"] }
```

- `runtime-tokio` — REQUIRED. Without this, async-imap defaults to async-std runtime, which conflicts with this binary's tokio runtime. This is the same requirement as the v1.0 addon.

**QRESYNC gap and mitigation:** For QRESYNC-based resync (passing `QRESYNC (uidvalidity modseq)` in SELECT), async-imap does not provide a typed API. Options: (1) use `session.run_command_and_read_response("SELECT INBOX (CONDSTORE QRESYNC (12345 67890))")` for raw command execution, or (2) implement QRESYNC as a phase-2 enhancement when async-imap adds typed support. Start with CONDSTORE for MVP — it covers 95% of incremental sync use cases.

### SMTP Client

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `lettre` | **0.11.19** | SMTP message sending (SendDraftTask) | Mature, async tokio support, connection pooling, XOAUTH2 built-in, MIME message building. The sync engine uses lettre both for connection testing (as in v1.0) AND for actual message delivery (SendDraftTask). lettre can build MIME emails via `lettre::Message::builder()`, handling attachments and multipart. |

**Required Cargo.toml features:**
```toml
[dependencies]
lettre = { version = "0.11", default-features = false, features = [
    "tokio1",          # async runtime
    "rustls-tls",      # rustls TLS backend (no OpenSSL)
    "builder",         # Message builder for composing MIME emails
    "smtp-transport",  # AsyncSmtpTransport
] }
```

- `builder` — Enables `lettre::Message::builder()` for composing outbound email with headers, body, and attachments. This is needed for `performRemoteSendDraft`.
- Do NOT enable `file-transport` or `sendmail-transport` (not needed).
- Do NOT enable `native-tls` (introduces OpenSSL — avoid for cleanliness, though the binary doesn't have Electron's BoringSSL constraint; rustls is still preferred for cross-platform build simplicity).

### TLS

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `tokio-rustls` | **0.26.4** | Async TLS for IMAP streams | Same as v1.0. Pure Rust, no system TLS dependencies. |
| `rustls-platform-verifier` | **0.6.2** | OS-native certificate validation | Same as v1.0. Uses OS trust store (Windows CertStore, macOS Security.framework, Linux CA bundle). Required for enterprise environments. |

**Required Cargo.toml:**
```toml
[dependencies]
tokio-rustls = "0.26"
rustls-platform-verifier = "0.6"
```

Note: lettre uses rustls internally via its `rustls-tls` feature. Do not add `rustls` crate directly — let tokio-rustls and lettre's TLS feature select the version transitively.

### HTTP Client (CalDAV/CardDAV, metadata sync)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `reqwest` | **0.13.x** | HTTP for CalDAV/CardDAV WebDAV requests and metadata API calls | The C++ engine used libcurl via vcpkg. reqwest is the Rust equivalent: ergonomic, async tokio, rustls TLS, supports custom headers (WebDAV methods REPORT/PROPFIND), and is the most widely used HTTP client in the Rust ecosystem. |

**Required Cargo.toml features:**
```toml
[dependencies]
reqwest = { version = "0.13", default-features = false, features = [
    "rustls-tls",       # rustls TLS (no OpenSSL)
    "json",             # serde_json integration for metadata API
    "http2",            # HTTP/2 support (CalDAV servers often prefer it)
] }
```

- `rustls-tls` — Consistent TLS stack across the engine. No OpenSSL.
- `json` — Auto-serialization with serde_json for `performRemoteMetadata*` tasks.
- Do NOT use `native-tls` feature.

**CalDAV/CardDAV WebDAV method support:** reqwest supports arbitrary HTTP methods via `client.request(Method::from_bytes(b"PROPFIND")?, url)`. The C++ engine made REPORT, PROPFIND, PROPPATCH, MKCALENDAR, PUT, DELETE requests to DAV endpoints — all achievable via reqwest with custom method names and XML body strings.

### CalDAV and CardDAV Client

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `libdav` | **0.10.2** (released 2026-02-10) | CalDAV/CardDAV client with service discovery | Implements CalDAV (RFC 4791) and CardDAV (RFC 6352) client protocols with service discovery bootstrapping. Provides `CalDavClient` and `CardDavClient` with `bootstrap_via_service_discovery()`. Uses hyper internally but wraps it in a DAV-aware HTTP layer. Actively maintained. |

**Alternative for DAV:** Build WebDAV operations directly on reqwest (custom PROPFIND/REPORT XML bodies). This is viable for a thin client that replicates the C++ DAVWorker's existing request patterns. **libdav is recommended** to avoid reimplementing WebDAV discovery, PROPFIND parsing, and sync-collection support from scratch.

**TLS configuration for libdav:** libdav uses hyper as its HTTP client with `hyper-rustls` for TLS. The library has no feature flags — TLS backend is determined by how the underlying connector is configured at construction time. Pass a `hyper_rustls::HttpsConnector` built with `rustls-platform-verifier` roots to `CalDavClient::new()`.

```toml
[dependencies]
libdav = "0.10"
hyper-rustls = { version = "0.27", features = ["http2", "native-roots"] }
```

### iCalendar and vCard Parsing

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `calcard` | **0.3.2** | iCalendar (.ics) and vCard (.vcf) parsing and building | Maintained by Stalwart Labs (same team as `mail-parser`). Supports full iCalendar RFC 5545 and vCard RFC 6350, plus JSCalendar/JSContact conversion. Used in production by Stalwart Mail Server. Replaces C++ vendored `icalendarlib` and custom vCard parsing in `DAVWorker`. |

**Required Cargo.toml:**
```toml
[dependencies]
calcard = "0.3"
```

No feature flags needed — the crate provides iCalendar and vCard in the default build.

**What it replaces:** `Vendor/icalendarlib/` (iCalendar parsing), inline vCard parsing in `DAVWorker.cpp::ingestAddressDataNode()`.

**Alternative considered:** `ical` crate (Peltoche/ical-rs) — less comprehensive, lower maintenance activity. `calcard` is preferred because it handles both formats in one crate and is battle-tested in a production mail server.

### Email (MIME) Parsing

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `mail-parser` | **0.11.2** | RFC 5322/MIME email parsing for `MailProcessor` | Maintained by Stalwart Labs. Zero-copy via `Cow<str>`, 100% safe Rust, no external dependencies, conforms to RFC 5322 + RFC 2045-2049 (MIME), handles 41 character sets including UTF-7. The C++ engine used mailcore2's message parsing to extract headers, bodies, and attachments — `mail-parser` is the direct replacement for `MailProcessor.cpp`. Battle-tested with millions of real-world emails in Stalwart Mail Server. |

**Required Cargo.toml:**
```toml
[dependencies]
mail-parser = "0.11"
```

**What it replaces:** `MailProcessor.cpp` / mailcore2 message parsing (MCAbstractMessage, MCMessagePart, MCAttachment).

**Why not `mailparse`:** `mailparse` is simpler but less comprehensive in character set support and header parsing correctness. `mail-parser` is better tested against real-world email edge cases.

### HTML Sanitization

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `ammonia` | **4.1.2** | HTML sanitization for email message bodies | Whitelist-based HTML sanitizer built on html5ever. Strips XSS vectors, onclick handlers, script tags, and malicious attributes. Parses HTML the same way browsers do — resilient to obfuscation. The C++ engine used vcpkg-managed tidy-html5 for sanitization. ammonia is more security-focused (whitelist vs. tidy's repair approach) and requires no system libraries. Version 4.1.2 applies fixes for RUSTSEC-2025-0071. |

**Required Cargo.toml:**
```toml
[dependencies]
ammonia = "4"
```

**What it replaces:** tidy-html5 (vcpkg) used in `MailUtils.cpp` for HTML cleaning before storing message bodies.

**Why not tidy-html5 via bindings:** system library dependency, C linkage, not pure Rust.

### SQLite Database Layer

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `rusqlite` | **0.38.0** | SQLite database for all models (Thread, Message, Folder, Contact, Event, etc.) | The C++ engine used SQLiteCpp with a "fat row" pattern: indexed columns + a `data` JSON blob. rusqlite is the standard Rust SQLite binding. Use `bundled` feature to embed SQLite 3.51.1 and eliminate the system SQLite dependency — critical for consistent behavior across macOS (old system SQLite) and Linux distros. |

**Required Cargo.toml features:**
```toml
[dependencies]
rusqlite = { version = "0.38", features = ["bundled", "serde_json"] }
```

- `bundled` — Compiles and statically links SQLite 3.51.1. Eliminates system SQLite dependency. Required for Windows (no system SQLite) and macOS (system SQLite is often outdated). This matches the C++ approach of vendoring SQLite source.
- `serde_json` — Enables rusqlite's JSON column support, needed for the `data` JSONB column pattern.

**WAL mode:** Enable at connection open time — not a Cargo feature:
```rust
conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
```

WAL mode allows the Electron TypeScript layer to read the database concurrently while the sync engine writes. This is how the C++ engine achieved read concurrency with the UI.

**The "fat row" schema pattern:** The C++ engine stored most model data in a `data JSON` column with a few indexed columns for queries (id, account_id, thread_id, etc.). The Rust engine should replicate this exact schema to maintain compatibility with any existing databases migrated from C++.

**Why not sqlx:** sqlx requires async compile-time query checking against a running database. rusqlite's synchronous API is appropriate here — SQLite writes are inherently single-threaded and blocking; wrapping them in `tokio::task::spawn_blocking` is the standard pattern for async Rust + SQLite.

### OAuth2 Token Management

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `oauth2` | **5.0.0** | OAuth2 token refresh for Gmail, Outlook, and other OAuth providers | The de facto Rust OAuth2 crate by ramosbugs. Supports PKCE, refresh token flow, and multiple HTTP backends (reqwest by default). The sync engine needs to refresh access tokens before they expire and re-authenticate IMAP/SMTP sessions. Version 5.0.0 is the latest major release. |

**Required Cargo.toml features:**
```toml
[dependencies]
oauth2 = { version = "5", features = ["reqwest"] }
```

- `reqwest` — Uses the reqwest HTTP client for token endpoint requests. This reuses the reqwest dependency already in the stack.

**Token refresh pattern:** On each IMAP session establishment, check token expiry. If expired or within 5 minutes of expiry, call `client.exchange_refresh_token(&refresh_token).request_async().await`. Store refreshed tokens to the database and emit a delta so the Electron UI can update stored credentials.

**What it replaces:** OAuth2 token refresh logic embedded in `MailUtils.cpp::getAuthorizationHeader()` and the C++ account configuration.

### iCalendar/vCard Data Encoding (Base64)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `base64` | **0.22.1** | Base64 encoding for MIME parts, IMAP AUTHENTICATE, and vCard photo/binary data | The standard base64 crate with 783M total downloads. The engine API requires `base64::engine::general_purpose::STANDARD` for MIME and `base64::engine::general_purpose::STANDARD_NO_PAD` for XOAUTH2 SASL. Version 0.22 uses the Engine API (vs old deprecated `encode()`/`decode()` functions). |

**Required Cargo.toml:**
```toml
[dependencies]
base64 = "0.22"
```

### Structured Logging

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `tracing` | **0.1.x** | Structured, async-aware instrumentation and logging | The Tokio project's logging framework. Spans track context across async await points — critical for correlating log entries with specific accounts/folders when multiple sync threads are running. Replaces C++ spdlog. |
| `tracing-subscriber` | **0.3.x** | Log output formatting (JSON or human-readable) | Provides `FmtSubscriber` for formatted output. The sync engine should write logs to stderr (not stdout, which is reserved for delta JSON). Use `EnvFilter` to control verbosity via `RUST_LOG` env var. |

**Required Cargo.toml:**
```toml
[dependencies]
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
```

- `env-filter` — `RUST_LOG=debug` / `RUST_LOG=unifymail_sync=trace` control at runtime
- `json` — Optional: JSON-structured log output for production log aggregation

**stderr routing:** The C++ engine used spdlog to write to files. The Rust engine should write logs to stderr by default (keeping stdout clean for delta JSON). Add file appender if needed via `tracing-appender` crate.

### XML Parsing (CalDAV/CardDAV responses)

| Technology | Version | Purpose | Why Recommended |
|------------|---------|---------|-----------------|
| `quick-xml` | **0.37.x** | Parse PROPFIND/REPORT XML responses from CalDAV/CardDAV servers | The C++ engine used libxml2 (vcpkg) via `DavXML.cpp`. quick-xml is a zero-copy, streaming XML parser/writer in pure Rust. Needed for parsing WebDAV responses (multistatus, prop, href nodes). No system library dependency. |

**Required Cargo.toml:**
```toml
[dependencies]
quick-xml = { version = "0.37", features = ["serialize"] }
```

- `serialize` — Enables serde integration for deserializing XML into Rust structs (eliminates manual DOM traversal).

**Alternative:** `roxmltree` (read-only DOM) — simpler API for parsing but doesn't serialize. If libdav handles all WebDAV XML parsing internally, quick-xml may only be needed for edge cases. Evaluate at implementation time: if libdav covers all CalDAV/CardDAV protocol needs, skip quick-xml.

---

## Supporting Libraries

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `uuid` | **1.x** | Generate stable message/thread IDs (replaces `MailProcessor` ID generation) | For generating IDs from message headers (Message-ID + threading) |
| `sha2` | **0.10.x** | SHA-256 for content hashing (ID generation, ETag comparison) | Computing deterministic IDs from email headers (same as C++ `MCMessage::stableMessageId`) |
| `hex` | **0.4.x** | Hex encoding for binary hashes | Converting SHA-256 hashes to hex string IDs |
| `chrono` | **0.4.x** | Date/time handling for email headers and iCalendar | Parsing RFC 2822 dates, RRULE date math; `chrono` has good timezone support |
| `regex` | **1.x** | Pattern matching for email classification and folder role detection | Folder role detection (e.g., matching "Sent" in multiple languages as in `MailUtils.cpp`) |
| `hickory-resolver` | **0.25.2** | DNS MX lookups | Only needed if the sync engine validates provider MX records; may be inherited from v1.0 addon |
| `tokio-util` | **0.7.x** | `LinesCodec` for framed line-delimited stdin reading | Cleaner alternative to manual `BufReader.lines()` — `FramedRead<Stdin, LinesCodec>` provides a Stream of lines |

**Required Cargo.toml additions for supporting libraries:**
```toml
[dependencies]
uuid = { version = "1", features = ["v5"] }   # v5 = SHA-1 namespace UUIDs (deterministic from message headers)
sha2 = "0.10"
hex = "0.4"
chrono = { version = "0.4", features = ["serde"] }
regex = "1"
tokio-util = { version = "0.7", features = ["codec"] }
```

---

## Development Tools

| Tool | Purpose | Notes |
|------|---------|-------|
| `cargo-xwin` | Cross-compile Windows MSVC target from Linux/macOS CI | `cargo install cargo-xwin`. Same as v1.0. Windows native builds use MSVC on `windows-latest` runners. |
| `cargo-zigbuild` | Cross-compile Linux aarch64 from x86_64 | `cargo install cargo-zigbuild`. Same as v1.0. |
| `cargo-bloat` | Binary size analysis | `cargo install cargo-bloat`. The sync engine should target <20 MB stripped on Linux x64. |
| `cargo-audit` | Security vulnerability scanning | `cargo install cargo-audit`. Run `cargo audit` before each release; catch crates with published CVEs (e.g., RUSTSEC-2025-0071 in ammonia, addressed in 4.1.2). |

---

## Cross-Compilation Targets

Same targets as v1.0 N-API addon — the binary ships alongside the `.node` addon in the Electron package:

| Target Triple | Platform | Build Method | CI Runner |
|---------------|----------|--------------|-----------|
| `x86_64-pc-windows-msvc` | Windows x64 | Native MSVC | `windows-latest` |
| `x86_64-apple-darwin` | macOS Intel | Native Clang | `macos-latest` |
| `aarch64-apple-darwin` | macOS Apple Silicon | Native Clang | `macos-latest` |
| `x86_64-unknown-linux-gnu` | Linux x64 | Native GCC | `ubuntu-latest` |
| `aarch64-unknown-linux-gnu` | Linux ARM64 | cargo-zigbuild | `ubuntu-latest` (cross) |

**Binary distribution:** The sync engine binary is distributed as a platform-specific binary alongside the Electron app (same as the current C++ `mailsync` binary). It is launched as a child process by `mailsync-process.ts` and is NOT a Node.js module — no napi-rs involved.

---

## Complete Cargo.toml Template

```toml
[package]
name = "unifymail-sync"
version = "2.0.0"
edition = "2021"

[[bin]]
name = "mailsync"
path = "src/main.rs"

[dependencies]
# --- Async runtime ---
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "io-util", "macros", "sync", "fs"] }
tokio-util = { version = "0.7", features = ["codec"] }

# --- CLI ---
clap = { version = "4", features = ["derive"] }

# --- IPC protocol ---
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# --- IMAP ---
async-imap = { version = "0.11", features = ["runtime-tokio"] }

# --- SMTP (send + connection test) ---
lettre = { version = "0.11", default-features = false, features = [
    "tokio1", "rustls-tls", "builder", "smtp-transport"
] }

# --- TLS ---
tokio-rustls = "0.26"
rustls-platform-verifier = "0.6"

# --- HTTP (CalDAV/CardDAV/metadata API) ---
reqwest = { version = "0.13", default-features = false, features = [
    "rustls-tls", "json", "http2"
] }

# --- CalDAV / CardDAV ---
libdav = "0.10"
hyper-rustls = { version = "0.27", features = ["http2", "native-roots"] }

# --- iCalendar + vCard ---
calcard = "0.3"

# --- Email parsing ---
mail-parser = "0.11"

# --- HTML sanitization ---
ammonia = "4"

# --- SQLite ---
rusqlite = { version = "0.38", features = ["bundled", "serde_json"] }

# --- OAuth2 ---
oauth2 = { version = "5", features = ["reqwest"] }

# --- XML (CalDAV responses) ---
quick-xml = { version = "0.37", features = ["serialize"] }

# --- Utilities ---
base64 = "0.22"
uuid = { version = "1", features = ["v5"] }
sha2 = "0.10"
hex = "0.4"
chrono = { version = "0.4", features = ["serde"] }
regex = "1"

# --- Logging ---
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }

[profile.release]
lto = true
strip = "symbols"
opt-level = 3
```

---

## Installation

```bash
# Add cross-compilation tools (same as v1.0 if not already installed)
cargo install cargo-xwin
cargo install cargo-zigbuild
cargo install cargo-bloat
cargo install cargo-audit

# Add required rust targets
rustup target add x86_64-pc-windows-msvc
rustup target add aarch64-apple-darwin
rustup target add x86_64-apple-darwin
rustup target add aarch64-unknown-linux-gnu

# Dev build (host platform)
cargo build

# Release build (host platform)
cargo build --release

# Release build (specific target)
cargo build --release --target aarch64-apple-darwin

# Check binary size after release build
cargo bloat --release --crates

# Security audit
cargo audit
```

---

## Alternatives Considered

| Recommended | Alternative | Why Not Alternative |
|-------------|-------------|---------------------|
| `async-imap 0.11` | `imap` (jonhoo, sync) | Synchronous — requires `spawn_blocking` per connection, wastes threads. async-imap integrates with the existing tokio runtime. |
| `async-imap 0.11` | Custom IMAP parser on raw tokio TCP | ~3000 lines of parser code to reimplement. async-imap's imap-proto parser already handles RFC 3501 response parsing correctly including literal strings and nested lists. |
| `rusqlite` | `sqlx` (async) | sqlx requires compile-time query checking against a live database; adds CI complexity. SQLite is inherently single-writer synchronous — wrapping rusqlite in `spawn_blocking` is the standard pattern and avoids sqlx's compile-time setup. |
| `rusqlite` bundled | System SQLite | macOS ships SQLite 3.36 (outdated); Windows has no system SQLite at all. Bundling SQLite 3.51.1 ensures consistent behavior and WAL compatibility. |
| `calcard` | `ical` (Peltoche) | `ical` is a low-level parser without vCard support. `calcard` handles both iCalendar and vCard in one crate and is actively maintained by Stalwart Labs. |
| `mail-parser` | `mailparse` | `mailparse` has narrower character set support and less battle-testing. `mail-parser` is production-validated in Stalwart Mail Server with 41 charsets including UTF-7. |
| `ammonia 4` | `sanitize_html` | ammonia is the established standard with html5ever parser (browser-accurate). sanitize_html is newer and less tested. |
| `libdav 0.10` | Raw reqwest WebDAV | libdav provides service discovery, PROPFIND parsing, and sync-collection support. Reimplementing this on raw reqwest is ~1000 lines of non-trivial XML handling. |
| `oauth2 5` | `yup-oauth2` | yup-oauth2 is Google-specific. `oauth2` is provider-agnostic, works with any RFC 6749 endpoint (Gmail, Outlook, Yahoo, etc.). |
| `reqwest 0.13` | `hyper` directly | hyper requires manual connection management. reqwest provides connection pooling, redirect handling, timeout configuration, and serde_json integration out of the box. |
| `tracing` | `log` + `env_logger` | `log` is not async-aware; span correlation across await points is impossible. `tracing` is the Tokio-maintained standard for async Rust observability. |

---

## What NOT to Use

| Avoid | Why | Use Instead |
|-------|-----|-------------|
| `native-tls` crate | OpenSSL system dependency on Linux; inconsistent behavior across platforms; avoidable | `tokio-rustls` + `rustls-platform-verifier` |
| `trust-dns-resolver` | Renamed to `hickory-resolver` at v0.24; old crate is unmaintained | `hickory-resolver = "0.25"` |
| `async-std` runtime | async-imap defaults to async-std — must override with `runtime-tokio`; mixing runtimes causes panics | tokio exclusively |
| `sqlx` for SQLite | Requires compile-time database for query checking; SQLite is single-writer by nature | `rusqlite` with `spawn_blocking` |
| `libsqlite3-sys` without `bundled` | System SQLite varies wildly; Windows has none; macOS system version is too old for WAL2 | `rusqlite` with `bundled` feature |
| Manual WebDAV XML | Reimplementing DAV discovery, PROPFIND parsing, sync-collection protocol ~1000 LOC | `libdav` |
| `ical` crate alone | No vCard support; low maintenance; less comprehensive than calcard | `calcard` |
| stdout for logs | stdout is reserved for newline-delimited JSON delta protocol; logging to stdout corrupts the IPC stream | stderr via tracing-subscriber |
| `#[tokio::main]` with napi-rs | Not applicable in this binary — this is a standalone binary, not an addon. DO use `#[tokio::main]` here. | `#[tokio::main]` is correct for this binary |

---

## Stack Patterns by Variant

**If email provider uses CONDSTORE (most modern IMAP servers):**
- Call `session.select_condstore("INBOX").await?` to get HIGHESTMODSEQ
- On subsequent syncs, fetch only messages with MODSEQ > last known value via raw `FETCH 1:* (FLAGS) (CHANGEDSINCE modseq)` command
- Store per-folder `highestmodseq` in the local database

**If email provider uses IDLE (most providers):**
- Use `async-imap`'s `extensions::idle` module
- `session.idle().wait_with_timeout(Duration::from_secs(1740)).await` (29 minutes, RFC 2177 recommends < 30 minutes)
- On idle notification, interrupt the background sync thread via `tokio::sync::mpsc` channel

**If CalDAV server supports sync-collection (RFC 6578):**
- Use `libdav`'s `runForCalendarWithSyncToken` equivalent via `CalDavClient::sync_collection()`
- Store sync-token per calendar, pass on subsequent calls for incremental sync (replicates C++ `runForCalendarWithSyncToken`)

**If OAuth2 token is expired:**
- Exchange refresh token via `oauth2::Client::exchange_refresh_token().request_async().await`
- Update stored tokens in SQLite, emit a Task delta to Electron so UI can update credential display
- Retry IMAP AUTHENTICATE with new token immediately

**If sending email with attachments:**
- Use `lettre::Message::builder()` with `.multipart()` for MIME construction
- Pass raw `mail-parser` part data (bytes) as attachment content

**If SQLite is locked (concurrent read from Electron):**
- Set `PRAGMA busy_timeout=5000;` on connection open — automatically retries for up to 5 seconds before returning SQLITE_BUSY
- WAL mode reduces lock contention: readers never block writers and vice versa

---

## Version Compatibility

| Package | Version | Compatible With | Notes |
|---------|---------|-----------------|-------|
| `tokio` | 1.x | `async-imap` (runtime-tokio), `lettre` (tokio1), `reqwest` 0.13, `oauth2` 5 (reqwest) | All major async crates in this stack share tokio 1.x — no runtime conflicts |
| `async-imap` | 0.11.2 | tokio 1.x (with `runtime-tokio` feature) | Must use `runtime-tokio` feature |
| `lettre` | 0.11.19 | tokio 1.x (with `tokio1` feature), rustls 0.23 (via `rustls-tls`) | 0.11.x stable branch |
| `tokio-rustls` | 0.26.4 | rustls 0.23.x, tokio 1.x | Do not pin rustls directly |
| `rustls-platform-verifier` | 0.6.2 | rustls 0.23.x | Run `cargo tree` to verify rustls version consistency |
| `reqwest` | 0.13.x | tokio 1.x, rustls 0.23 (via `rustls-tls`) | 0.13 is the current major; 0.12 is previous |
| `libdav` | 0.10.2 | hyper 1.x, hyper-rustls 0.27.x | libdav pins to hyper 1.x; verify `hyper-rustls` version with `cargo tree` |
| `rusqlite` | 0.38.0 | Bundles SQLite 3.51.1 | `bundled` feature avoids system SQLite version conflicts |
| `oauth2` | 5.0.0 | reqwest (via `reqwest` feature), tokio 1.x | Version 5.0 is a major rewrite from 4.x; do not mix v4 and v5 APIs |
| `calcard` | 0.3.2 | No runtime dependencies of concern | Pure Rust, no external C libraries |
| `mail-parser` | 0.11.2 | No runtime dependencies of concern | 100% safe Rust, no external deps |
| `ammonia` | 4.1.2 | html5ever (transitive) | 4.1.2 fixes RUSTSEC-2025-0071; do not use 4.1.1 or earlier |

---

## CONDSTORE/QRESYNC Gap — Decision Required

This is the most significant capability gap between the C++ engine and the current Rust crate ecosystem.

**C++ engine used (via mailcore2):** `selectWithParameters` passing both CONDSTORE and QRESYNC parameters, enabling modseq-based incremental sync AND rapid mailbox resynchronization after reconnect.

**async-imap 0.11.2 provides:** `select_condstore()` — CONDSTORE parameter only. No typed QRESYNC API.

**Options for v2.0:**

1. **CONDSTORE only (recommended for MVP):** Use `select_condstore()` for modseq-based incremental sync. Skip QRESYNC resync. This covers 95% of the sync performance benefit. Reconnect sync uses UID FETCH instead of QRESYNC. Add QRESYNC in a later patch when async-imap adds typed support.

2. **QRESYNC via raw command:** Use `session.run_command_and_read_response("ENABLE QRESYNC")` followed by `session.run_command_and_read_response("SELECT INBOX (QRESYNC (uidvalidity modseq))")` and manually parse the response. High implementation complexity — response parsing is not assisted by async-imap for QRESYNC-specific data.

3. **Implement QRESYNC support in async-imap:** Contribute upstream to chatmail/async-imap. High effort but correct long-term solution.

**Recommendation:** Start with Option 1 (CONDSTORE only). The C++ engine's QRESYNC usage is an optimization — the engine falls back gracefully to full sync when QRESYNC fails. Defer QRESYNC to a v2.1 patch after CONDSTORE-based sync is validated.

---

## Sources

- [docs.rs/async-imap/latest](https://docs.rs/async-imap/latest/async_imap/) — Version 0.11.2, extensions module, IDLE/QUOTA/ID/CONDSTORE support (HIGH confidence)
- [github.com/chatmail/async-imap/blob/main/src/client.rs](https://github.com/chatmail/async-imap/blob/main/src/client.rs) — `select_condstore()` method confirmed present, no `select_qresync()` (HIGH confidence — direct source inspection)
- [github.com/chatmail/async-imap/blob/main/src/extensions/mod.rs](https://github.com/chatmail/async-imap/blob/main/src/extensions/mod.rs) — Extensions: idle, quota, id, compress (HIGH confidence — direct source inspection)
- [docs.rs/lettre/latest](https://docs.rs/lettre/latest/lettre/) — Version 0.11.19, tokio1/rustls-tls/builder features (HIGH confidence)
- [docs.rs/rusqlite/latest](https://docs.rs/rusqlite/latest/rusqlite/) — Version 0.38.0, bundled SQLite 3.51.1 (HIGH confidence)
- [docs.rs/libdav/latest](https://docs.rs/libdav/latest/libdav/) — Version 0.10.2 (released 2026-02-10), CalDAV+CardDAV, hyper-based (HIGH confidence)
- [docs.rs/calcard/latest](https://docs.rs/calcard/latest/calcard/) — Version 0.3.2, iCalendar + vCard, Stalwart Labs (HIGH confidence)
- [docs.rs/mail-parser/latest](https://docs.rs/mail-parser/latest/mail_parser/) — Version 0.11.2, RFC 5322/MIME, zero-copy (HIGH confidence)
- [docs.rs/ammonia/latest](https://docs.rs/ammonia/latest/ammonia/) — Version 4.1.2, RUSTSEC-2025-0071 fix applied (HIGH confidence)
- [docs.rs/oauth2/latest](https://docs.rs/oauth2/latest/oauth2/) — Version 5.0.0, reqwest backend, PKCE, async refresh (HIGH confidence)
- [docs.rs/clap/latest](https://docs.rs/clap/latest/clap/) — Version 4.5.60, derive API (HIGH confidence)
- [crates.io/crates/reqwest](https://crates.io/crates/reqwest) — Version 0.13.x, rustls-tls feature (HIGH confidence)
- [crates.io/crates/base64](https://crates.io/crates/base64) — Version 0.22.1 (HIGH confidence)
- [docs.rs/imap-proto/latest](https://docs.rs/crate/imap-proto/latest) — Version 0.16.6, nom-based IMAP parser underlying async-imap (HIGH confidence)
- [C++ mailsync source files](../../../app/mailsync/MailSync/) — SyncWorker.hpp, DAVWorker.hpp, TaskProcessor.hpp, MailStore.hpp read directly to understand replacement scope (HIGH confidence)
- [app/mailsync/CLAUDE.md](../../../app/mailsync/CLAUDE.md) — Threading model, vendor library list, build system overview (HIGH confidence)
- WebSearch: CONDSTORE/QRESYNC in async-imap ecosystem — confirmed CONDSTORE via select_condstore(), QRESYNC gap confirmed via multiple source searches (MEDIUM confidence — source inspection is HIGH)

---

*Stack research for: Rust mailsync engine binary (v2.0 milestone)*
*Researched: 2026-03-02*
