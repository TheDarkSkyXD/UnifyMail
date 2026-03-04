---
phase: 01-scaffolding-and-provider-detection
plan: 02
subsystem: infra
tags: [rust, napi-rs, electron, provider-detection, wrapper, cross-validation]

# Dependency graph
requires:
  - phase: 01-01-scaffolding-and-provider-detection
    provides: Rust crate at app/mailcore-rs/ with compiled .node binary and 16 passing tests
provides:
  - Wrapper module at app/mailcore-wrapper/ routing provider functions to Rust, network to C++
  - Custom platform-aware index.js loader for GNU .node in MSVC Node.js environments
  - Cross-validation test suite (49 tests) confirming Rust provider results
  - Electron integration test (7 checks) confirming addon loads without BoringSSL conflicts
  - Build integration in start-dev.js (Rust addon auto-built on npm start)
  - Developer documentation in app/mailcore-rs/README.md (253 lines)
  - CLAUDE.md updated with Rust addon commands
affects:
  - future-phases (IMAP, SMTP, account validation phases 2-3)
  - app-onboarding (providerForEmail now runs in Rust via wrapper)

# Tech tracking
tech-stack:
  added:
    - app/mailcore-wrapper/ (new module, name mailcore-napi, version 2.0.0)
    - Platform-aware index.js loader pattern (direct .node require bypassing napi ABI detection)
  patterns:
    - Wrapper module pattern: same npm package name intercepts all require('mailcore-napi') calls
    - Lazy-loaded network addon: getCpp() only requires C++ .node when first called (deferred load)
    - Custom index.js loader: replaces napi-generated loader for cross-toolchain compatibility
    - Build integration via spawnSync in start-dev.js (synchronous prebuild before Electron launch)

key-files:
  created:
    - app/mailcore-rs/index.js (hand-written custom platform loader, tracked in git)
    - app/mailcore-wrapper/index.js (routes provider→Rust, network→C++)
    - app/mailcore-wrapper/package.json (name=mailcore-napi for transparent require interception)
    - app/mailcore-rs/tests/cross-validate-providers.js (49 JS tests)
    - test/electron-integration-test.js (7 Electron main process checks)
    - app/mailcore-rs/README.md (253 lines, full developer docs)
  modified:
    - app/package.json (mailcore-napi: file:mailcore -> file:mailcore-wrapper)
    - app/mailcore-rs/.gitignore (removed index.js from exclusion, keep .node and index.d.ts)
    - scripts/start-dev.js (added step 0: Rust addon build before Electron launch)
    - package.json (added build:rust script)
    - CLAUDE.md (added Rust Addon section)

key-decisions:
  - "Custom hand-written index.js replaces napi-generated loader: GNU .node loads in MSVC Node.js via N-API stable ABI, but the napi-generated detector checks process.config.variables.shlib_suffix which is wrong for standard Windows Node.js"
  - "MSVC target fallback skipped: requires Visual Studio link.exe which is not installed; GNU target works correctly via N-API layer"
  - "Network functions are lazy-loaded in wrapper: getCpp() called only on first network function invocation, so C++ not required at require('mailcore-napi') time"
  - "app/mailcore-rs/index.js tracked in git (not gitignored): it is hand-written, not a build artifact"

patterns-established:
  - "Wrapper interception pattern: same package name as C++ addon intercepts require() without consumer code changes"
  - "Deferred C++ loading: lazy getCpp() enables the app to start even when C++ addon is not built"
  - "Platform-aware direct .node loading: bypass napi ABI detection for cross-toolchain scenarios"

requirements-completed: [SCAF-02, PROV-04]

# Metrics
duration: 8min
completed: 2026-03-03
---

# Phase 01 Plan 02: Scaffolding and Provider Detection Summary

**Rust addon wired into Electron via wrapper module — 49/49 cross-validation tests passing, 7/7 Electron integration checks passing, GNU .node loads in MSVC Node.js via N-API stable ABI**

## Performance

- **Duration:** 8 min (automated tasks 1-2; task 3 is human verification checkpoint)
- **Started:** 2026-03-03T21:43:15Z
- **Completed:** 2026-03-03T21:51:00Z (automated portion)
- **Tasks:** 2 auto + 1 human-verify checkpoint
- **Files modified:** 11

## Accomplishments

- Rust napi-rs addon loads in Electron main process without BoringSSL/OpenSSL conflicts (verified via 7-check integration test)
- Wrapper module at app/mailcore-wrapper/ intercepts all `require('mailcore-napi')` calls, routing provider functions to Rust and network functions to C++ (lazy-loaded)
- 49/49 cross-validation tests pass: all 25 domain-match providers, server config spot-checks (gmail, outlook, yahoo, fastmail, hushmail), and error input validation
- Yahoo/yahoo.co.jp domain-exclude pattern verified correctly in JavaScript tests
- Rust addon build integrated into `npm start` via `scripts/start-dev.js` step 0
- GNU .node (x86_64-pc-windows-gnu) loads correctly in MSVC-built Node.js v24 via N-API's stable ABI layer
- Developer documentation complete: 253-line README.md with prerequisites, build steps, architecture, and known limitations

## Task Commits

Each task was committed atomically:

1. **Task 1: Wrapper module, build integration, and cross-validation** - `48518d5` (feat)
2. **Task 2: Developer documentation and CLAUDE.md update** - `6cfa546` (docs)
3. **Task 3: Human verification checkpoint** - pending (awaiting user verification)

## Files Created/Modified

- `app/mailcore-rs/index.js` - Hand-written platform-aware binary loader (replaces napi-generated; tracked in git)
- `app/mailcore-rs/.gitignore` - Removed index.js from exclusion; .node and index.d.ts remain excluded
- `app/mailcore-rs/Cargo.lock` - Added to git for reproducible builds
- `app/mailcore-rs/tests/cross-validate-providers.js` - 49-test cross-validation script
- `app/mailcore-wrapper/index.js` - Wrapper routing provider→Rust, network→C++ (lazy)
- `app/mailcore-wrapper/package.json` - Package name=mailcore-napi for transparent interception
- `app/package.json` - mailcore-napi dependency now points to file:mailcore-wrapper
- `package.json` - Added build:rust script
- `scripts/start-dev.js` - Added synchronous Rust build step 0 before Electron launch
- `test/electron-integration-test.js` - 7-check Electron main process integration test
- `app/mailcore-rs/README.md` - 253-line developer documentation
- `CLAUDE.md` - Added Rust Addon section

## Decisions Made

- **Custom index.js loader:** The napi-generated `index.js` uses `process.config.variables.shlib_suffix === 'dll.a'` to detect GNU vs MSVC Node.js — but standard Windows Node.js (MSVC-built) doesn't set this flag, causing it to look for the MSVC binary and fail. Solution: hand-written loader that directly requires the GNU `.node` by filename. The N-API stable ABI means the GNU `.node` loads correctly in any Node.js process on the same platform.

- **MSVC target skipped:** Building with `x86_64-pc-windows-msvc` failed because `link.exe` (Visual Studio linker) is not installed. The GNU target via MSYS2 dlltool works correctly and the resulting `.node` loads in both GNU and MSVC Node.js processes.

- **Network functions lazy-loaded:** `getCpp()` only requires the C++ addon on first invocation. This allows `require('mailcore-napi')` to succeed even when the C++ addon hasn't been built, which is the current state of the development environment.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] napi-generated index.js fails to load GNU binary in MSVC Node.js**
- **Found during:** Task 1 (first `node -e "require('./index.js')"` test)
- **Issue:** The auto-generated `index.js` uses `process.config.variables.shlib_suffix === 'dll.a'` to determine if the running Node.js is GNU-built. Standard Windows Node.js distributions are MSVC-built and don't have this flag, causing the loader to look for `mailcore-napi-rs.win32-x64-msvc.node` which doesn't exist.
- **Fix:** Replaced the napi-generated `index.js` with a hand-written custom loader that directly requires the GNU `.node` file by filename, bypassing ABI detection entirely.
- **Files modified:** `app/mailcore-rs/index.js`, `app/mailcore-rs/.gitignore`
- **Verification:** `node -e "require('./index.js')"` loads successfully; `providerForEmail('test@gmail.com')` returns `{identifier: 'gmail', ...}`
- **Committed in:** `48518d5` (Task 1 commit)

**2. [Rule 3 - Blocking] MSVC Rust target build failed (no Visual Studio link.exe)**
- **Found during:** Task 1 (fallback attempt after GNU loader issue identified)
- **Issue:** The `x86_64-pc-windows-msvc` target requires Microsoft's `link.exe` linker. The system has MinGW `link.exe` (from MSYS2) which is incompatible with MSVC target linking. Error: `link: missing operand after ' ■'`.
- **Fix:** Confirmed GNU target is the correct choice (per user preference documented in CONTEXT.md). The GNU `.node` loads in MSVC Node.js via N-API's stable ABI, so no fallback to MSVC is needed.
- **Files modified:** None (build environment, not code)
- **Verification:** GNU target builds successfully; Electron integration test passes
- **Committed in:** N/A (environment issue, resolved by confirming GNU works)

---

**Total deviations:** 2 auto-fixed (1 loader bug, 1 blocking build issue)
**Impact on plan:** Both issues stem from the GNU/MSVC toolchain boundary, as anticipated by the plan's "Windows GNU target runtime validation" section. The N-API ABI layer makes the GNU binary universally compatible. No scope creep.

## Issues Encountered

- **dlltool.exe not in PATH:** The first `napi build` attempt failed because `dlltool.exe` (required for GNU .node linking) was not in the shell's PATH. Fixed by prepending `/c/msys64/mingw64/bin` to PATH in the build command. This is now documented in `scripts/start-dev.js` and `app/mailcore-rs/README.md`.

## User Setup Required

None for running the app (Rust addon builds automatically on `npm start`).

**Developer workstation setup** (first time only) requires:
1. MSYS2 with MinGW-w64 (`C:\msys64\mingw64\bin` in PATH)
2. Rust GNU toolchain (`rustup target add x86_64-pc-windows-gnu`)
3. libnode.dll import library (generated from node.exe via gendef+dlltool, stored at `/tmp/libnode.dll`)

Full instructions: `app/mailcore-rs/README.md`

## Next Phase Readiness

- Wrapper module is live — `require('mailcore-napi')` routes provider calls to Rust
- All 37 providers correctly detected in JavaScript (49/49 tests)
- Electron integration confirmed (7/7 checks) — no BoringSSL conflicts
- Phase 2 (IMAP connection) can add `imap.rs` to `app/mailcore-rs/src/` and export via wrapper
- C++ addon remains available for network functions until Phase 2-3 replaces them

**Phase 3 risk noted:** electron-builder asarUnpack interaction with `.node` binary distribution still unverified (logged in STATE.md).

## Self-Check: PASSED

All files exist, commits verified, dependency link confirmed:
- app/mailcore-rs/index.js: FOUND
- app/mailcore-rs/tests/cross-validate-providers.js: FOUND
- app/mailcore-wrapper/index.js: FOUND
- app/mailcore-wrapper/package.json: FOUND
- test/electron-integration-test.js: FOUND
- app/mailcore-rs/README.md: FOUND (253 lines)
- Commit 48518d5: FOUND
- Commit 6cfa546: FOUND
- app/package.json mailcore-napi -> file:mailcore-wrapper: FOUND

---
*Phase: 01-scaffolding-and-provider-detection*
*Completed: 2026-03-03*
