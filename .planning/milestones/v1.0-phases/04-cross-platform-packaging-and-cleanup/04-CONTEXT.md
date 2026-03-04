# Phase 4: Cross-Platform Packaging and Cleanup - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

GitHub Actions CI produces .node binaries for all 5 platform targets (win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64), the stripped Linux x64 release binary is under 8MB with LTO, consumer code imports via `require('mailcore-napi')` without modification, the mailcore-wrapper module is removed, and all C++ N-API addon artifacts (app/mailcore/, app/mailcore-wrapper/, node-gyp, node-addon-api) are deleted from the repository. The C++ mailsync engine (app/mailsync/) is NOT touched — it stays until v2.0 Phase 10.

</domain>

<decisions>
## Implementation Decisions

### Binary size enforcement
- **8MB hard fail** in CI on Linux x64 — CI step fails the build if the stripped .node binary exceeds 8MB
- Size check runs on **Linux x64 only** — other platforms may vary and aren't gated
- If 8MB is exceeded after Phase 3 code is complete: **investigate with cargo-bloat** first, identify largest contributors, trim unnecessary features or adjust opt-level before considering relaxing the limit
- Full Cargo release profile: `lto = true`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`

### Wrapper module removal
- **Remove mailcore-wrapper entirely** — delete `app/mailcore-wrapper/` directory and all references
- Point `app/package.json` directly at `"mailcore-napi": "file:mailcore-rs"` in `optionalDependencies`
- **Keep as optionalDependencies** (not regular dependencies) — both consumer files (onboarding-helpers.ts, mailsync-process.ts) have try/catch fallbacks that should continue working
- The require chain after cleanup: `require('mailcore-napi')` → `app/mailcore-rs/index.js` → `mailcore-rs.<platform>.node`

### C++ deletion scope
- **Delete app/mailcore/** — entire C++ N-API addon directory (src/, build-windows/, Externals/, binding.gyp, etc.)
- **Delete app/mailcore-wrapper/** — no longer needed after wrapper removal
- **Leave app/mailsync/ untouched** — C++ sync engine binary stays until v2.0 Phase 10 replaces it
- **Completely remove Windows C++ CI steps** — vcpkg setup, msbuild steps 6-12 in build-windows.yaml. No comments, clean delete. Git history has the old steps
- **Aggressive trim of Linux CI system packages** — remove all C++-only packages: autoconf, automake, clang, cmake, libctemplate-dev, libcurl4-openssl-dev, libicu-dev, libsasl2-dev, libsasl2-modules, libsasl2-modules-gssapi-mit, libssl-dev, libtidy-dev, libtool, libxml2-dev, execstack. Keep Electron-needed packages
- Remove `node-addon-api` and `node-gyp` from root package.json
- Remove any remaining references to `file:mailcore` in package.json files

### CI workflow structure
- **Insert Rust build steps into existing 4 workflows** — no new shared/reusable workflow
- Each workflow gets: Rust toolchain setup (dtolnay/rust-toolchain@stable), cargo cache (actions/cache@v4), napi build step with platform-specific target
- **Add CI verification step** — explicit `npm ci` after cleanup changes to catch broken references
- **Add smoke test** — quick Electron headless test that verifies `require('mailcore-napi')` loads and `providerForEmail('test@gmail.com')` returns a provider object. Catches packaging/loading regressions

### Dev workflow
- **No changes to npm start** — Phase 1's Rust build integration stays as-is. Cargo incremental compilation handles no-op builds efficiently

### Claude's Discretion
- opt-level choice: try `"z"` (size) first, fall back to `"s"` (balanced) if performance issues arise
- Custom index.js vs napi-rs generated loader: check if napi-rs v3 fixed the shlib_suffix detection issue for GNU .node in MSVC Node.js; use standard loader if fixed, keep custom if not
- Windows setup documentation updates after C++ removal
- Exact smoke test implementation (inline in workflow vs separate script)
- Cargo dependency feature flag tuning for minimum binary size

</decisions>

<specifics>
## Specific Ideas

- Research (04-RESEARCH.md) has complete step-by-step maps for all 4 CI workflows — exact steps to keep, modify, add, and remove
- The `*.node` glob in `build/tasks/package-task.js` asar.unpack already catches native binaries — no changes needed for Electron packaging
- Azure Trusted Signing on Windows already includes `.node` extension in its filter — the Rust addon will be automatically signed
- macOS workflow uses a matrix with separate arm64/x64 entries (no universal binary) — add arch-conditional Rust targets
- The macOS workflow has a stale cache key using `yarn.lock` instead of `package-lock.json` — pre-existing issue, not Phase 4 scope

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailcore-rs/`: Complete Rust napi-rs addon with all 5 functions implemented (Phases 1-3)
- `app/mailcore-rs/index.js`: Custom loader that handles GNU .node in MSVC Node.js
- `app/mailcore-rs/Cargo.toml`: Already has correct dependency feature flags; needs release profile additions
- `.github/workflows/build-*.yaml`: 4 existing CI workflows ready for Rust step insertion

### Established Patterns
- CI workflows trigger on `workflow_dispatch` only (manual trigger)
- `actions/setup-node@v4` with `cache: 'npm'` for Node.js caching — extend with separate cargo cache
- `build/tasks/package-task.js` asar.unpack already handles `*.node` files
- Debug logging via `MAILCORE_DEBUG=1` environment variable (Phases 1-3)
- Phase 1 already integrated Rust build into `npm start`

### Integration Points
- `app/package.json` `optionalDependencies`: change `"file:mailcore"` → `"file:mailcore-rs"` (was `file:mailcore-wrapper` after Phase 1)
- `app/internal_packages/onboarding/lib/onboarding-helpers.ts` line 104: `require('mailcore-napi')` — no changes needed
- `app/frontend/mailsync-process.ts` line 439: `require('mailcore-napi')` — no changes needed
- `package.json` (root): remove `node-gyp` and `node-addon-api` dependencies

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-cross-platform-packaging-and-cleanup*
*Context gathered: 2026-03-03*
