# Project Research Summary

**Project:** UnifyMail — Rust napi-rs N-API addon (mailcore-napi rewrite)
**Domain:** Native Node.js addon in Rust replacing a C++ mailcore2 addon for IMAP/SMTP connection testing and email provider detection
**Researched:** 2026-03-01
**Confidence:** HIGH

## Executive Summary

This project is a feature-parity rewrite of five exported functions from a C++ node-addon-api (mailcore2-backed) native addon into a pure-Rust napi-rs addon. The functions — `registerProviders`, `providerForEmail`, `testIMAPConnection`, `testSMTPConnection`, and `validateAccount` — are the boundary between the Electron UI and account validation logic during email onboarding. The existing TypeScript/React/Electron application stack remains entirely unchanged; only `app/mailcore/` is replaced. The driving motivations are eliminating the node-gyp/mailcore2 C++ build chain and gaining ABI-stability across Electron versions.

The recommended approach is to use napi-rs 3.8.3 with `napi4 + async + tokio_rt` features, async-imap 0.11.2 for IMAP capability testing, lettre 0.11.19 for SMTP, hickory-resolver 0.25.2 for MX lookups, and rustls (via tokio-rustls 0.26.4 and rustls-platform-verifier 0.6.2) for TLS throughout. The flat four-file Rust source layout (`provider.rs`, `imap.rs`, `smtp.rs`, `validator.rs`) maps cleanly to the five exported functions and mirrors the C++ source structure. The `napi build --platform` CLI tool replaces node-gyp and generates TypeScript declarations automatically from `#[napi]` proc-macros, eliminating the hand-maintained `types/index.d.ts`.

The critical risk area is not Rust implementation complexity but Electron integration correctness. Three Pitfall classes — TLS library choice (rustls vs. native-tls/OpenSSL conflicting with Electron's BoringSSL), Electron V8 memory cage constraints on external ArrayBuffers, and napi-rs tokio runtime lifecycle in Electron's multi-process model — are go/no-go gates that must be verified before any IMAP or SMTP code is written. Getting these wrong requires painful refactoring across all three async function implementations. The strategy is: prove the addon loads correctly in Electron main process first, then implement in strict dependency order (provider sync logic, then IMAP, then SMTP, then validateAccount which composes the others).

## Key Findings

### Recommended Stack

The Rust crate selection is well-established and verified against official documentation. napi-rs v3 (3.8.3) is the current stable branch with simplified ThreadsafeFunction API, auto-generated TypeScript declarations, and native async/tokio integration. The async runtime (tokio 1.x) is owned by napi-rs — no manual `Runtime::new()` is needed or permitted. TLS uses rustls exclusively throughout (tokio-rustls 0.26.4 + rustls-platform-verifier 0.6.2) because native-tls on Linux introduces OpenSSL symbols that conflict with Electron's bundled BoringSSL. There are no viable alternatives to this TLS choice for an Electron addon that must run on Linux.

See `.planning/research/STACK.md` for the complete Cargo.toml template and version compatibility matrix.

**Core technologies:**
- `napi 3.8.3` + `napi-derive 3` + `napi-build 2`: N-API framework — current stable, ABI-stable across Electron versions, auto-generates `.d.ts`
- `tokio 1.x` (rt-multi-thread, net, time, io-util, macros): async runtime — managed by napi-rs; required for all three async function exports
- `tokio-rustls 0.26.4` + `rustls-platform-verifier 0.6.2`: TLS — pure Rust, no OpenSSL, uses OS trust store for certificate validation (critical for enterprise CAs)
- `async-imap 0.11.2` (feature: runtime-tokio): IMAP capability detection — only maintained async IMAP crate; must use runtime-tokio feature or async-std conflict occurs
- `lettre 0.11.19` (features: tokio1, rustls-tls): SMTP connection testing — only Rust SMTP library with built-in XOAUTH2 SASL support
- `hickory-resolver 0.25.2` (feature: tokio-runtime): MX lookups — renamed successor to trust-dns-resolver; required for provider matching in `validateAccount`
- `serde 1.x` + `serde_json 1.x`: providers.json parsing — industry standard, 39KB file parsed once at module load

### Expected Features

The five functions are a closed scope — this is a parity rewrite, not a feature expansion. All behavior must match C++ output exactly. See `.planning/research/FEATURES.md` for full behavioral specifications, capability mapping tables, error scenario lists, and XOAUTH2 encoding requirements.

**Must have (table stakes — v1, required for API parity):**
- `registerProviders(jsonPath)` — JSON parse with serde_json, regex compilation with `regex::RegexSet`, stored in `OnceLock<ProviderDatabase>`
- `providerForEmail(email)` — synchronous domain-regex lookup returning typed MailProviderInfo (must map `ssl/starttls` bool flags to `connectionType` string enum)
- `testIMAPConnection(opts)` — TLS/STARTTLS/clear paths, password + XOAUTH2 authentication, 7 specific capability strings detected (idle, condstore, qresync, compress, namespace, xoauth2, gmail)
- `testSMTPConnection(opts)` — TLS/STARTTLS/clear via lettre SmtpTransport variants, password + XOAUTH2, `test_connection()` NOOP
- `validateAccount(opts)` — orchestrates IMAP + SMTP with TLS, adds provider identifier from domain lookup, returns typed result shape
- TypeScript declarations auto-generated by napi-rs matching existing `types/index.d.ts` shapes exactly (use `#[napi(js_name = "...")]` to preserve uppercase naming like `testIMAPConnection`)
- All three async functions non-blocking on Node.js event loop via napi-rs async tokio integration

**Should have (v1.x — safe additive improvements):**
- Configurable connection timeout parameter (currently hardcoded; C++ comment notes 30s hardcoded timeout)
- Concurrent IMAP + SMTP testing in `validateAccount` via `tokio::join!()` — reduces validation latency ~50%
- `allowInsecureSsl` parameter for enterprise environments with self-signed certificates

**Defer (v2+):**
- Structured error codes (errorCode field) — only if TypeScript consumers need machine-readable errors
- IMAP4rev2 capability detection — additive, low risk, deferred until needed

**Anti-features (explicitly excluded):**
- Full IMAP client (FETCH, SEARCH, IDLE loop) — mailsync C++ engine owns ongoing IMAP
- POP3 connection testing — return `pop: []` in MailProviderInfo, no connection test
- Hot-reload of providers.json — use `OnceLock` single-initialization; providers.json is bundled and static

### Architecture Approach

The architecture is a thin Rust layer directly below the existing TypeScript consumers. The `app/mailcore/` directory is converted from a node-gyp C++ project to a napi-rs Rust project in place; the `"mailcore-napi": "file:mailcore"` reference in `app/package.json` remains unchanged. The `index.js` platform binary loader and `index.d.ts` declaration file are generated by `napi build --platform` and committed to the repo. The flat Rust source structure with four module files keeps the codebase shallow and maintainable.

Key architectural decisions from research:
- Embed `providers.json` at compile time via `include_str!()` to eliminate runtime path resolution across Electron dev/production/packaged environments
- Use `OnceLock<ProviderDatabase>` for global provider state — zero-cost reads, no lock on the hot path, no external crates (standard since Rust 1.70)
- Use `#[napi(module_exports)]` init hook for automatic provider loading on module load, replicating C++ `Init()` behavior
- Never use `napi::Env` inside async fn bodies — collect results in plain Rust types, return them, let napi-rs marshal on main thread

**Major components:**
1. `provider.rs` — sync provider lookup; OnceLock global state; serde_json parse; regex matching
2. `imap.rs` — async IMAP test; async-imap + tokio-rustls; three connection type paths; XOAUTH2 Authenticator trait impl
3. `smtp.rs` — async SMTP test; lettre AsyncSmtpTransport; three transport builders; native XOAUTH2 via lettre Credentials
4. `validator.rs` — async account validation; composes imap + smtp; tokio::join!() for concurrent testing
5. `lib.rs` — module entry, `#[napi(module_exports)]` init, re-exports from all four modules

### Critical Pitfalls

See `.planning/research/PITFALLS.md` for full details including warning signs, recovery strategies, and phase assignments.

1. **TLS library conflict: native-tls vs. Electron's BoringSSL** — Use rustls exclusively (tokio-rustls + rustls-platform-verifier). Run `cargo tree | grep openssl` and it must return nothing. Address in Phase 1 scaffolding — retrofitting TLS libraries later requires coordinated changes across imap.rs, smtp.rs, and Cargo.toml features.

2. **Tokio runtime lifecycle mismatch in Electron** — Pin napi-rs >= 2.16.16 (use template defaults which are v3). Wrap all async functions with `tokio::time::timeout`. Load addon only in Electron main process (the existing mailsync-process.ts pattern is correct). Add Electron integration test as go/no-go before IMAP code.

3. **IMAP/SMTP connections hang with no timeout** — async-imap and tokio provide no default timeout. Wrap every network future with `tokio::time::timeout(Duration::from_secs(15), ...)`. This is mandatory, not optional polish — missing timeouts hang the onboarding UI indefinitely.

4. **XOAUTH2 SASL encoding errors** — Use `Vec<u8>` with literal `b'\x01'` separators. Use `base64::engine::general_purpose::STANDARD` (not URL_SAFE). Send empty response `\r\n` after failed AUTHENTICATE challenge. Unit test against Google's reference encoding before live server testing.

5. **Provider regex matching silent mismatches** — Treat domain-match patterns as suffix-anchored case-insensitive matches. Cross-validate Rust output against C++ addon on 50 representative addresses before removing C++ code. Regex patterns need `^` + `$` anchoring and must not match substrings (`google.com` must not match `notgoogle.com`).

## Implications for Roadmap

Based on combined research, the phase structure is driven by two constraints: (1) Electron integration correctness is a go/no-go gate that must be proven before any IMAP/SMTP code, and (2) the five functions have a clear dependency order — provider lookup is synchronous and foundational; IMAP and SMTP are independent of each other; validateAccount composes all of the above.

### Phase 1: Scaffolding and Provider Detection

**Rationale:** The three critical Electron-specific pitfalls (TLS library, tokio runtime lifecycle, V8 memory cage) must be validated before any network code is written. Provider detection is synchronous, has no external dependencies, and produces an immediately verifiable result — it serves as the first meaningful implementation milestone after scaffolding is proven. Regex matching correctness for providers.json must also be established here since it underpins validateAccount's `identifier` field.

**Delivers:** Working napi-rs scaffold loading in Electron main process; `registerProviders` + `providerForEmail` functions with full type generation; 50-address cross-validation test passing against C++ output; `cargo tree | grep openssl` clean; CI building for all 5 target triples.

**Addresses:** registerProviders, providerForEmail (FEATURES.md table stakes); OnceLock + module_init pattern (ARCHITECTURE.md); include_str! embedding for providers.json path resolution.

**Avoids:** TLS library conflict (BoringSSL) — establish rustls-only in Cargo.toml from the start; V8 memory cage — declare no external ArrayBuffer return types; tokio runtime mismatch — verify Electron load before proceeding.

**Pitfall prevention:** Provider regex edge cases (Pitfall 7) must be resolved here.

### Phase 2: IMAP Connection Testing

**Rationale:** IMAP is the most complex of the three async functions. It has three TLS paths (tls/starttls/clear), requires a custom XOAUTH2 Authenticator trait implementation, and the STARTTLS stream-upgrade pattern is the highest-risk implementation task. Tackling IMAP before SMTP means the harder problem is proven first; SMTP is easier and benefits from patterns established here. Error message format must be established in this phase since it affects all subsequent consumer integration.

**Delivers:** `testIMAPConnection` with all three connection types, password + XOAUTH2 auth, 7 capabilities correctly detected, 15-second timeout on all network operations, error strings matching C++ format.

**Uses:** async-imap 0.11.2 (runtime-tokio), tokio-rustls 0.26.4, rustls-platform-verifier 0.6.2 (STACK.md).

**Implements:** imap.rs module (ARCHITECTURE.md).

**Avoids:** Connection timeout hang (Pitfall 5) — mandatory timeout wrappers; XOAUTH2 encoding errors (Pitfall 6) — unit test before live server; opaque error messages (Pitfall 8) — domain error enum with C++-matching Display strings established here.

### Phase 3: SMTP Connection Testing and Account Validation

**Rationale:** SMTP is structurally simpler than IMAP because lettre provides a higher-level API that maps directly to the three connection type variants and has built-in XOAUTH2 support. `validateAccount` is a composition of the already-proven IMAP and SMTP functions plus the provider lookup from Phase 1 — it should be implemented last when all components are individually validated.

**Delivers:** `testSMTPConnection` with all three lettre transport builders and XOAUTH2; `validateAccount` orchestrating both with `tokio::join!()` for concurrent testing; complete API parity across all 5 functions; TypeScript consumers (`onboarding-helpers.ts`, `mailsync-process.ts`) compile without type errors.

**Uses:** lettre 0.11.19 (tokio1, rustls-tls), hickory-resolver 0.25.2 (STACK.md).

**Implements:** smtp.rs, validator.rs modules (ARCHITECTURE.md).

**Avoids:** SMTP XOAUTH2 SASL encoding (Pitfall 6 SMTP variant); provider identifier correctly populated in validateAccount result.

### Phase 4: Cross-Platform Packaging and CI

**Rationale:** The napi-rs binary distribution strategy (single-package vs. optional dependencies) has different implications for Electron vs. Node.js packaging and must be validated with electron-builder. The asarUnpack configuration, cross-compilation for all 5 target triples, and binary size targets all require a separate focus phase. This phase cannot be done in parallel with implementation because it depends on the finalized Cargo.toml feature flags that determine binary size.

**Delivers:** GitHub Actions CI building all 5 platform binaries; electron-builder asarUnpack configured for `.node` files; stripped release binary < 8 MB on Linux x64; macOS universal binary; verified packaged build on each target platform.

**Avoids:** Architecture mismatch in packaging (Pitfall 4) — electron-builder asarUnpack + single-package distribution strategy; binary size bloat (Pitfall 9) — `cargo bloat` review, explicit tokio feature flags, `profile.release` with lto + strip.

**Research flag:** This phase requires validation of the electron-builder asarUnpack + napi-rs binary distribution interaction — the napi-rs documentation covers npm distribution but Electron-builder specifics require verification against the actual build configuration.

### Phase Ordering Rationale

- Provider detection before IMAP/SMTP: synchronous and verifiable without network; correctness of regex matching underpins validateAccount's identifier field
- Scaffolding as go/no-go gate: three of the nine documented pitfalls are Electron-specific structural issues that would require full refactoring if discovered after IMAP/SMTP implementation
- IMAP before SMTP: STARTTLS stream upgrade and custom XOAUTH2 Authenticator are the hardest implementation tasks; establishing error format conventions in Phase 2 benefits Phase 3
- validateAccount last: pure composition of proven components; `tokio::join!()` concurrent execution is trivial once both IMAP and SMTP are tested independently
- Packaging as final phase: depends on finalized Cargo.toml features; binary size optimization is a final-pass concern once functionality is proven

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 2 (IMAP STARTTLS):** The stream-type transition from plain TcpStream to TlsStream inside async-imap is not abstracted by the library. Existing patterns from deltachat-core-rust or similar projects should be studied before implementation. This is the highest-complexity task in the entire rewrite.
- **Phase 4 (Electron packaging):** The interaction between napi-rs single-package binary distribution and electron-builder's asarUnpack mechanism needs hands-on verification. The napi-rs/node-rs issue #376 documents the problem but the recommended workaround (napi-postinstall + asarUnpack) may need adjustment for this project's specific electron-builder configuration.

Phases with standard patterns (no research-phase needed):
- **Phase 1 (Provider detection):** JSON parsing, regex matching, OnceLock state — all standard Rust patterns with comprehensive documentation.
- **Phase 2 (IMAP TLS + password auth):** Direct TLS connection on port 993 using async-imap + tokio-rustls is well-documented in async-imap examples.
- **Phase 3 (SMTP):** lettre's transport builder pattern is the cleanest API in the crate graph; XOAUTH2 is natively supported and documented.

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crate versions verified against docs.rs and official napi-rs documentation. Cargo.toml feature combinations verified against crate feature lists. One MEDIUM item: Electron 39 Node.js version (22.20.0) sourced from a third-party article — cross-reference against electronjs.org releases recommended. |
| Features | HIGH | C++ source files read directly. TypeScript consumer files read directly. Behavioral specifications derived from actual source, not inference. XOAUTH2 format verified against Google's protocol spec. |
| Architecture | HIGH | napi-rs official documentation is comprehensive. Component boundaries directly mirror C++ source structure. One known gap: `get_current_dll_path()` platform-specific implementation — the `include_str!()` embedding approach (documented in ARCHITECTURE.md) eliminates this entirely and is preferred. |
| Pitfalls | HIGH | Most pitfalls sourced from official napi-rs issue tracker, Electron blog posts, and RFC specifications. All 9 pitfalls have documented recovery strategies. IMAP STARTTLS implementation complexity is the one area where the mitigation advice ("look at deltachat-core-rust") is directional rather than prescriptive. |

**Overall confidence:** HIGH

### Gaps to Address

- **IMAP STARTTLS stream upgrade:** Research recommends looking at deltachat-core-rust patterns before implementing — this should happen at the start of Phase 2, not as a research-phase but as a direct code reading exercise on the reference implementation.
- **Electron 39 Node.js version:** Verify `22.20.0` vs `22.21.1` against the official Electron releases page (ARCHITECTURE.md and STACK.md have slightly different version numbers for the same Electron release — does not affect any implementation decision since napi4 supports all Node 22.x versions).
- **providers.json regex matching semantics:** The C++ mailcore2 matching logic is implicit in source code, not formally specified. The cross-validation test (50 addresses against C++ output) in Phase 1 is the validation mechanism — it must be run before the C++ code is deleted.
- **Binary distribution strategy for this specific electron-builder setup:** The project's existing `app/package.json` uses `"file:mailcore"` reference. Whether this maps well to the napi-rs single-package distribution pattern with electron-builder needs validation in Phase 4.

## Sources

### Primary (HIGH confidence)
- [docs.rs/crate/napi/latest](https://docs.rs/crate/napi/latest) — napi 3.8.3 features and version
- [napi.rs/docs/concepts/async-fn](https://napi.rs/docs/concepts/async-fn) — async + tokio_rt integration
- [napi.rs/docs/cross-build](https://napi.rs/docs/cross-build) — cross-compilation tooling
- [napi.rs/docs/deep-dive/release](https://napi.rs/docs/deep-dive/release) — binary distribution strategy
- [napi.rs/changelog/napi](https://napi.rs/changelog/napi) — version history, Electron fixes
- [docs.rs/async-imap](https://docs.rs/async-imap/latest/async_imap/) — 0.11.2, runtime-tokio feature
- [docs.rs/lettre/latest](https://docs.rs/lettre/latest/lettre/) — 0.11.19, XOAUTH2, TLS
- [docs.rs/hickory-resolver](https://docs.rs/hickory-resolver/latest/hickory_resolver/) — 0.25.2
- [docs.rs/crate/rustls-platform-verifier](https://docs.rs/crate/rustls-platform-verifier/latest) — 0.6.2
- [docs.rs/tokio-rustls](https://docs.rs/tokio-rustls/latest/tokio_rustls/) — 0.26.4
- [developers.google.com/gmail/imap/xoauth2-protocol](https://developers.google.com/workspace/gmail/imap/xoauth2-protocol) — XOAUTH2 spec
- [electronjs.org/blog/v8-memory-cage](https://www.electronjs.org/blog/v8-memory-cage) — external ArrayBuffer prohibition
- C++ source files read directly: `app/mailcore/src/napi/napi_imap.cpp`, `napi_smtp.cpp`, `napi_provider.cpp`, `napi_validator.cpp`, `addon.cpp`
- TypeScript interface read directly: `app/mailcore/types/index.d.ts`
- Consumer behavior read directly: `app/internal_packages/onboarding/lib/onboarding-helpers.ts`

### Secondary (MEDIUM confidence)
- [napi-rs/napi-rs issue #1175](https://github.com/napi-rs/napi-rs/issues/1175) — Windows thread_local GetProcAddress fix
- [napi-rs/napi-rs issue #2460](https://github.com/napi-rs/napi-rs/issues/2460) — SIGABRT on terminated worker thread
- [napi-rs/node-rs issue #376](https://github.com/napi-rs/node-rs/issues/376) — arch mismatch with optional dependencies in Electron
- [electron/electron issue #13176](https://github.com/electron/electron/issues/13176) — BoringSSL/OpenSSL conflict confirmation
- [rust-imap issue #167](https://github.com/jonhoo/rust-imap/issues/167) — no built-in connection timeout

### Tertiary (MEDIUM-LOW confidence)
- [Electron 39 release notes (third-party)](https://xiuerold.medium.com/electron-39-a-quiet-evolution-of-the-modern-runtime-11a079fd8517) — Node.js 22.20.0 version claim; cross-reference against official releases recommended

---
*Research completed: 2026-03-01*
*Ready for roadmap: yes*
