# UnifyMail

## What This Is

UnifyMail is an Electron-based desktop email client written in TypeScript with React. It uses a plugin architecture where features are implemented as internal packages, a Flux-based state management system, and a C++ sync engine for IMAP/SMTP communication. The mailcore N-API addon has been rewritten in Rust (napi-rs), providing provider auto-detection, IMAP/SMTP connection testing, and account validation with cross-platform builds.

## Core Value

Users can set up email accounts quickly and reliably through automatic provider detection and connection validation — the onboarding experience must work seamlessly across all major email providers.

## Current Milestone: v2.0 Rewrite mailsync Engine in Rust

**Goal:** Replace the `app/mailsync/` C++ sync engine (~16,200 LOC) with a Rust implementation, eliminating all vendored C++ dependencies (libetpan, mailcore2) while maintaining the stdin/stdout JSON protocol and multi-threaded sync architecture.

**Target features:**
- Rust sync engine binary with identical stdin/stdout JSON protocol
- IMAP sync (background folder iteration + foreground IDLE) using async Rust
- SMTP send (via lettre or equivalent)
- CalDAV calendar sync and CardDAV contact sync
- SQLite database layer with delta emission
- Task processor (send, move, label, star, delete, metadata sync)
- OAuth2 token management
- Cross-platform builds (Windows, macOS, Linux)
- Full cleanup of C++ mailsync source and vendored dependencies

**Depends on:** v1.0 completion (mailcore N-API rewrite) — DONE

## Requirements

### Validated

- Provider auto-detection during onboarding via `providerForEmail()` (500+ providers) — v1.0
- Account validation via `validateAccount()` for IMAP/SMTP with password and OAuth2 — v1.0
- IMAP capability detection via `testIMAPConnection()` (idle, condstore, qresync, compress, xoauth2, gmail) — v1.0
- SMTP connection testing via `testSMTPConnection()` with TLS/STARTTLS — v1.0
- Provider database loading via `registerProviders()` from bundled JSON — v1.0
- Async operations run on worker threads (do not block Node.js event loop) — v1.0
- TypeScript type definitions for all exported functions — v1.0
- Rust napi-rs project scaffolding with cross-compilation targets — v1.0
- Provider JSON database parser with serde + domain/MX regex matching — v1.0
- IMAP connection + TLS negotiation + capability detection in Rust — v1.0
- SMTP connection + TLS negotiation + login test in Rust — v1.0
- Full account validation flow (MX resolve -> provider match -> test IMAP -> test SMTP) — v1.0
- Integration with existing onboarding-helpers.ts and mailsync-process.ts consumers — v1.0
- Removal of all C++ source, node-gyp configs, and vendored mailcore2 — v1.0
- CI builds for 5 targets with shared smoke tests — v1.0

### Active

- [ ] Rust mailsync binary with stdin/stdout JSON IPC protocol
- [ ] SQLite database layer with WAL mode, all 13 data models, delta emission
- [ ] IMAP background sync with CONDSTORE incremental sync and UID range fallback
- [ ] IMAP IDLE foreground monitoring with task interruption
- [ ] SMTP send via lettre with MIME construction
- [ ] Task processor for all 13+ task types with crash recovery
- [ ] OAuth2 token management with auto-refresh
- [ ] CalDAV/CardDAV calendar and contact sync
- [ ] Gmail-specific behaviors (folder whitelist, X-GM extensions, People API contacts)
- [ ] Cross-platform packaging with asar unpacking and C++ deletion

### Out of Scope

- Full IMAP client library — sync engine only needs sync operations, not a general-purpose IMAP library
- POP3/NNTP protocol support — compiled in old C++ but never used by N-API layer
- Mobile platform builds — desktop only (Windows, macOS, Linux)
- Exchange ActiveSync (EAS) — complex proprietary protocol with licensing requirements
- Per-folder IDLE connections — one IDLE on primary folder is sufficient
- Full body pre-download — lazy body caching with age policy is the correct approach
- IMAP NOTIFY extension (RFC 5465) — only ~30% of servers support it
- QRESYNC typed API — deferred until CONDSTORE parity is validated

## Context

Shipped v1.0 with 1,558 LOC Rust source + 1,699 LOC Rust tests.
Tech stack: Electron, TypeScript, React, Rust (napi-rs for N-API addon, upcoming standalone binary for sync engine).
The mailcore N-API addon (`app/mailcore-rs/`) is now pure Rust, loaded via `require('mailcore-napi')` npm symlink.
C++ mailcore2 source tree (~1,500 files) has been fully removed.
CI builds for all 5 platform targets (win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64).
The C++ mailsync engine (`app/mailsync/`) remains and is the target of v2.0.

## Constraints

- **API compatibility**: The 5 exported N-API functions have identical signatures and return types (v1.0 validated)
- **IPC compatibility**: Rust mailsync must use identical stdin/stdout JSON protocol as C++ engine
- **Platform targets**: Windows x86_64, macOS arm64+x86_64, Linux x86_64+ARM64
- **Async safety**: All network operations must use tokio async runtime, not block threads
- **OAuth2 support**: Must handle both password and OAuth2/XOAUTH2 authentication methods
- **TLS**: rustls exclusively — no OpenSSL symbols to conflict with Electron's BoringSSL
- **SQLite**: WAL mode with tokio-rusqlite single-writer pattern (no synchronous rusqlite on async threads)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Use napi-rs over node-addon-api | Cross-platform builds without node-gyp, auto-generated TS types, Rust safety | Good |
| Use async-imap for IMAP | Lightweight async IMAP client, replaces MCIMAPSession | Good |
| Use lettre for SMTP | Mature Rust SMTP library with TLS and OAuth2 support | Good |
| Use trust-dns-resolver for MX | Pure Rust DNS resolution for provider MX matching | Good |
| Reuse providers.json as-is | JSON database is format-agnostic, no conversion needed | Good |
| Use rustls exclusively (no OpenSSL) | Prevents BoringSSL symbol conflicts in Electron | Good — hard constraint validated |
| Embed providers.json via include_str!() | Eliminates runtime path resolution across environments | Good |
| Custom loader.js for GNU .node in MSVC Node.js | N-API stable ABI bypasses flawed shlib_suffix detection | Good — critical for Windows |
| Wrapper module same package name as C++ addon | require('mailcore-napi') routes transparently to Rust | Good — zero consumer changes |
| Per-protocol credential split in ValidateAccountOptions | Separate IMAP/SMTP username/password fields close field mapping gap | Good |
| Cargo release profile with 5 size flags | lto, strip, codegen-units=1, panic=abort, opt-level=z for sub-8MB binary | Good |
| Rust mailsync as standalone binary (not N-API addon) | Owns tokio runtime, no BoringSSL conflict, stdin/stdout IPC | -- Pending |
| Use CONDSTORE-only for incremental sync (no QRESYNC) | async-imap 0.11.2 lacks typed QRESYNC API | -- Pending |
| Use tokio-rusqlite for all database access | Prevents tokio thread starvation from synchronous rusqlite | -- Pending |
| Use libdav 0.10.2 for CalDAV/CardDAV | Replaces ~1,000 lines of manual WebDAV discovery/PROPFIND parsing | -- Pending |
| Dedicated stdout flush task with exclusive ownership | Prevents block-buffering silent drop of deltas in pipe mode | -- Pending |

---
*Last updated: 2026-03-04 after v1.0 milestone*
