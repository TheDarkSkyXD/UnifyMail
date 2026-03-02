# UnifyMail

## What This Is

UnifyMail is an Electron-based desktop email client written in TypeScript with React. It uses a plugin architecture where features are implemented as internal packages, a Flux-based state management system, and a C++ sync engine for IMAP/SMTP communication. It supports multiple email accounts with provider auto-detection during onboarding.

## Core Value

Users can set up email accounts quickly and reliably through automatic provider detection and connection validation — the onboarding experience must work seamlessly across all major email providers.

## Current Milestone: v1.0 Rewrite mailcore N-API in Rust

**Goal:** Replace the `app/mailcore/` C++ N-API addon with a minimal Rust implementation using napi-rs, eliminating the vendored mailcore2 dependency while maintaining API compatibility.

**Target features:**
- Rust napi-rs addon with identical 5-function API surface
- Provider detection from providers.json (sync)
- IMAP/SMTP connection testing and account validation (async)
- Cross-platform builds (Windows, macOS, Linux) without node-gyp
- Full cleanup of C++ code and build infrastructure

## Requirements

### Validated

<!-- Shipped and confirmed valuable. Inferred from existing codebase. -->

- Provider auto-detection during onboarding via `providerForEmail()` (500+ providers)
- Account validation via `validateAccount()` for IMAP/SMTP with password and OAuth2
- IMAP capability detection via `testIMAPConnection()` (idle, condstore, qresync, compress, xoauth2, gmail)
- SMTP connection testing via `testSMTPConnection()` with TLS/STARTTLS
- Provider database loading via `registerProviders()` from bundled JSON
- Async operations run on worker threads (do not block Node.js event loop)
- TypeScript type definitions for all exported functions

### Active

<!-- Current scope. Building toward these. -->

- [ ] Rust napi-rs project scaffolding with cross-compilation targets
- [ ] Provider JSON database parser with serde + domain/MX regex matching
- [ ] IMAP connection + TLS negotiation + capability detection in Rust
- [ ] SMTP connection + TLS negotiation + login test in Rust
- [ ] Full account validation flow (MX resolve -> provider match -> test IMAP -> test SMTP)
- [ ] Integration with existing onboarding-helpers.ts and mailsync-process.ts consumers
- [ ] Removal of all C++ source, node-gyp configs, and vendored mailcore2

### Out of Scope

<!-- Explicit boundaries. Includes reasoning to prevent re-adding. -->

- Full IMAP client implementation — only connection testing needed, mailsync C++ engine handles ongoing sync
- POP3/NNTP protocol support — compiled in old C++ but never used by N-API layer
- Mobile platform builds — desktop only (Windows, macOS, Linux)
- Sync engine rewrite — deferred to v2.0 milestone

## Next Milestone: v2.0 Rewrite mailsync Engine in Rust

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

**Depends on:** v1.0 completion (mailcore N-API rewrite)

## Context

- The current `app/mailcore/` addon wraps the entire mailcore2 C++ library (~100+ source files) but only exposes 5 functions
- The addon is consumed by 2 files: `onboarding-helpers.ts` (providerForEmail) and `mailsync-process.ts` (validateAccount)
- Both consumers have fallback paths if the native addon fails to load
- The `providers.json` file (39KB, 500+ providers) is a pure JSON database that can be reused as-is
- Current build uses node-gyp with platform-specific configs (MSVC+vcpkg on Windows, Xcode on macOS, CMake on Linux)
- napi-rs auto-generates TypeScript type definitions, maintaining API compatibility

## Constraints

- **API compatibility**: The 5 exported functions must have identical signatures and return types
- **Platform targets**: Windows x86_64, macOS arm64+x86_64, Linux x86_64+ARM64
- **Async safety**: validateAccount, testIMAPConnection, testSMTPConnection must not block Node.js event loop
- **OAuth2 support**: Must handle both password and OAuth2/XOAUTH2 authentication methods
- **Module name**: Keep `mailcore-napi` package name or update all consumer imports

## Key Decisions

<!-- Decisions that constrain future work. Add throughout project lifecycle. -->

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Use napi-rs over node-addon-api | Cross-platform builds without node-gyp, auto-generated TS types, Rust safety | -- Pending |
| Use async-imap for IMAP | Lightweight async IMAP client, replaces MCIMAPSession | -- Pending |
| Use lettre for SMTP | Mature Rust SMTP library with TLS and OAuth2 support | -- Pending |
| Use trust-dns-resolver for MX | Pure Rust DNS resolution for provider MX matching | -- Pending |
| Reuse providers.json as-is | JSON database is format-agnostic, no conversion needed | -- Pending |

---
*Last updated: 2026-03-02 after milestone v2.0 definition*
