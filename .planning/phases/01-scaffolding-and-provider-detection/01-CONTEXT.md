# Phase 1: Scaffolding and Provider Detection - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Prove Electron integration is sound by scaffolding a Rust napi-rs addon at `app/mailcore-rs/` and implementing the two provider functions (`registerProviders`, `providerForEmail`) with domain-regex matching and cross-validation against the C++ addon. MX-regex matching is deferred (requires async DNS resolution). The Rust addon becomes integrated via a wrapper module that routes provider functions to Rust and network functions to the C++ addon, ensuring the app remains fully functional at every commit.

</domain>

<decisions>
## Implementation Decisions

### Switchover & fallback
- Wrapper module routes each function to the correct addon (Rust for providers, C++ for network) — consumer code (`onboarding-helpers.ts`, `mailsync-process.ts`) stays unchanged
- **Every commit must leave the app fully functional** — no broken-state commits allowed
- All 37 providers are equally important — no priority ordering for validation
- Claude's discretion: switchover timing (when Rust becomes primary), C++ cleanup timing, fallback behavior (leaning fail-loudly since the goal is C++ elimination), wrapper module location, whether unimplemented network functions route to C++ or throw

### Build integration
- Rust addon build integrates into `npm start` — auto-build so developers don't need a separate command
- Use npm as the package manager (match existing project)
- Watch mode for Rust development — auto-rebuild `.node` when `.rs` files change (cargo-watch or similar)
- **Windows target: `x86_64-pc-windows-gnu` (GNU/MinGW)** — not MSVC. MinGW needs to be installed
- Rust toolchain is not yet installed — setup documentation needed
- No CI pipeline exists yet — Claude decides whether basic CI goes in Phase 1 or defers to Phase 4
- Claude's discretion: debug vs release builds for dev, rebuild strategy (always vs conditional), exact script location, Grunt integration approach, MSRV pinning, watch mode restart behavior (rebuild-only vs rebuild+restart)

### API contract & types
- Stricter TypeScript types than C++ — narrow string unions (e.g., `connectionType: 'tls' | 'starttls' | 'clear'` instead of `string`)
- **Throw on invalid input** — empty string, no '@', malformed emails throw a JS Error instead of returning null. This is an intentional behavioral improvement over C++ (which returns null for everything). Consumers already wrap in try-catch
- Return null only for valid emails that don't match any provider
- `registerProviders(jsonPath)` fully implemented with **merge semantics** — file providers override embedded providers on identifier conflict, rather than replacing the entire set
- Include POP server configs in the return value (full compatibility with C++ output shape)
- Claude's discretion: whether `domainMatch[]` and `mxMatch[]` arrays appear in the return value, TypeScript .d.ts approach (auto-generated only vs hand-written wrapper)

### Testing & validation
- Electron integration test required — must verify the addon loads in a real Electron process without BoringSSL/OpenSSL conflicts
- Claude's discretion: cross-validation scope (50 representative vs full 500+), comparison depth (full object vs identifier), test location (standalone script vs npm test), output format, test data generation approach, Jasmine integration vs standalone script

### Error handling
- Claude's discretion: error surfacing pattern (napi throw vs result object), panic policy (catch_unwind vs crash), error detail level, bad-provider-regex handling (fail vs skip), error crate choice (thiserror/anyhow/napi::Error), embedded JSON parse failure behavior

### Project structure
- Module-per-function layout in `app/mailcore-rs/src/` — separate files for provider logic, types, etc., ready for imap.rs and smtp.rs in Phases 2-3
- Rust tests in a separate `tests/` directory (integration-style), not inline `#[cfg(test)]`
- Copy `providers.json` to `app/mailcore-rs/resources/` — self-contained, ready for C++ deletion
- **Use a different package name during development** (e.g., `mailcore-napi-rs`) — rename to `mailcore-napi` at switchover to avoid confusion while both addons exist
- Single Cargo crate (no workspace)
- Claude's discretion: cross-validation script location, .gitignore for build artifacts

### MX matching scope
- **Domain-match only in Phase 1** — `domain-match` and `domain-exclude` regex patterns implemented
- MX-match deferred — requires async DNS resolution, will be addressed in Phase 3 (validateAccount)
- Claude's discretion: whether MX-only providers are skipped or included-but-not-matched, where MX matching eventually lives (providerForEmail vs validateAccount)

### Logging & debug
- Debug-only logging enabled via environment variable (e.g., `MAILCORE_DEBUG=1` or `RUST_LOG=debug`)
- **Always log provider count on initialization** (e.g., "Loaded 37 providers from embedded JSON") — sanity check that runs even without debug mode
- In debug mode, log which provider matched for each `providerForEmail` call
- Claude's discretion: log output destination (stderr vs Electron console), logging crate choice (log+env_logger vs eprintln!)

### Dependency choices
- Use the `regex` crate (standard Rust regex) — not `fancy-regex`
- Pre-compile all regex patterns at provider load time — cached for fast lookups
- **Pin exact dependency versions** in Cargo.toml (e.g., `regex = "=1.10.3"`)
- Claude's discretion: JSON parsing approach (typed structs vs dynamic Value)

### Documentation
- Full README.md in `app/mailcore-rs/` with prerequisites (Rust, MinGW), build steps, testing instructions, and architecture overview
- Update main project CLAUDE.md with Rust addon build/test commands and location
- Claude's discretion: doc comments depth, architecture diagrams

### Code style
- Clippy with default warnings enforced
- `#![forbid(unsafe_code)]` — no unsafe Rust in the addon
- Integrate Rust linting into `npm run lint` — `cargo fmt --check` and `cargo clippy` alongside TypeScript/JS linting
- Claude's discretion: rustfmt config (default vs custom), naming conventions (standard Rust vs mirror C++), Cargo.lock commit policy

### Claude's Discretion (aggregate)
- Switchover timing, fallback behavior, wrapper location
- Debug vs release builds, rebuild strategy, script locations, Grunt integration
- domainMatch/mxMatch in return value, .d.ts generation approach
- Cross-validation scope and infrastructure
- All error handling patterns
- MX-only provider handling, future MX matching location
- Log destination and crate
- JSON parsing approach, doc comments depth, architecture diagrams
- rustfmt config, naming conventions, Cargo.lock

</decisions>

<specifics>
## Specific Ideas

- The goal is to eliminate C++ — all decisions lean toward making C++ removal easier, not making the transition longer
- Every commit must be functional — wrapper module must be designed for incremental function migration
- Watch mode for Rust development — developer wants fast iteration when changing Rust code
- GNU/MinGW toolchain on Windows — developer prefers this over MSVC despite potential napi-rs ABI compatibility challenges (research needed)

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailcore/resources/providers.json`: 37-provider database with domain-match, mx-match, domain-exclude patterns and IMAP/SMTP/POP server configs — copy to `app/mailcore-rs/resources/`
- `app/mailcore/types/index.d.ts`: TypeScript interface contract (NetServiceInfo, MailProviderInfo, function signatures) — Rust addon's napi-generated types must be compatible
- `app/mailcore/src/core/provider/MCMailProvider.cpp`: C++ matching algorithm — domain extraction, `^...$` regex anchoring, case-insensitive matching, domain-exclude before domain-match ordering
- `app/mailcore/src/napi/napi_provider.cpp`: NAPI wrapper showing exact return object shape and null semantics

### Established Patterns
- Native addons loaded via `require('mailcore-napi')` — wrapper module must preserve this import path
- `app/package.json` uses `"file:mailcore"` for local native addon resolution — will point to wrapper module location
- Module initialization auto-loads providers via singleton pattern — C++ uses `MailProvidersManager::sharedManager()`, Rust equivalent is `OnceLock<ProviderDatabase>` or `RwLock` (for merge-capable registerProviders)
- Build tooling uses Grunt (`build/Gruntfile.js`) — Rust build may bypass or integrate

### Integration Points
- `app/internal_packages/onboarding/lib/onboarding-helpers.ts` (lines 101-133): Primary consumer of `providerForEmail` with try-catch fallback to static JSON lookup
- `app/frontend/mailsync-process.ts`: Consumer of `registerProviders` (to be verified during research)
- `app/package.json` (dependency line): Switchover point from `"file:mailcore"` to wrapper/Rust location

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-scaffolding-and-provider-detection*
*Context gathered: 2026-03-03*
