---
phase: 04-cross-platform-packaging-and-cleanup
plan: 02
subsystem: infra
tags: [rust, napi-rs, ci, github-actions, cargo, msys2, linux, macos, windows]

# Dependency graph
requires:
  - phase: 04-cross-platform-packaging-and-cleanup
    plan: 01
    provides: mailcore-napi package name, direct require() resolution, C++ addon deleted, Cargo release profile optimized
provides:
  - build-linux.yaml with Rust toolchain, cargo cache, napi build (x86_64-unknown-linux-gnu), 8MB binary size gate, smoke test, trimmed system deps
  - build-linux-arm64.yaml with Rust toolchain, cargo cache, napi build (aarch64-unknown-linux-gnu), smoke test, trimmed system deps
  - build-macos.yaml with arch-conditional Rust toolchain, cargo cache, napi build (aarch64-apple-darwin or x86_64-apple-darwin), smoke test
  - build-windows.yaml with MSYS2, libnode.dll generation, Rust toolchain (x86_64-pc-windows-gnu), cargo cache, napi build, smoke test; C++ steps 6-12 removed
affects: [CI-pipeline, all-5-platform-targets, electron-builder-packaging, binary-size-gating]

# Tech tracking
tech-stack:
  added:
    - dtolnay/rust-toolchain@stable (GitHub Actions action for Rust toolchain setup)
    - msys2/setup-msys2@v2 (Windows MINGW64 toolchain for dlltool.exe)
  patterns:
    - "Rust napi-rs build in CI: dtolnay/rust-toolchain -> actions/cache (Cargo.lock key) -> npx @napi-rs/cli build --release --target <triple>"
    - "MSYS2 shell for Windows GNU build: gendef + dlltool to produce libnode.dll import library, then napi build in msys2 {0} shell"
    - "Arch-conditional target in macOS matrix: matrix.arch == 'arm64' && 'aarch64-apple-darwin' || 'x86_64-apple-darwin'"
    - "Binary size gate (Linux x64 only): du -k *.node checked against 8192KB limit, fail with cargo-bloat hint"
    - "Smoke test pattern: node -e require('mailcore-napi').providerForEmail('test@gmail.com') validates addon loads end-to-end"

key-files:
  created: []
  modified:
    - .github/workflows/build-linux.yaml
    - .github/workflows/build-linux-arm64.yaml
    - .github/workflows/build-macos.yaml
    - .github/workflows/build-windows.yaml

key-decisions:
  - "Insert Rust build steps AFTER npm ci and BEFORE Lint in all workflows — npm ci must run first to create the node_modules/mailcore-napi symlink; napi build must run before Build step which packages the .node file"
  - "Binary size gate (8MB / 8192KB) on Linux x64 ONLY — per user decision from Phase 4 planning; other platforms use same binary, Linux x64 is the canonical size check"
  - "Smoke test uses default shell (not msys2) on Windows — only needs Node.js which comes from setup-node; msys2 shell used only for GNU toolchain steps"
  - "Remove C++ system packages from Linux workflows — autoconf, automake, clang, cmake, execstack, libctemplate-dev, libcurl4-openssl-dev, libicu-dev, libsasl2-*, libssl-dev, libtidy-dev, libtool, libxml2-dev all removed; mailcore2 C++ addon is gone"

patterns-established:
  - "CI Rust build pattern: rust-toolchain -> cargo cache (Cargo.lock key) -> napi build -> smoke test -> (size gate on Linux x64) all before Lint"
  - "Windows GNU CI pattern: MSYS2 MINGW64 -> gendef+dlltool for libnode.dll -> Rust GNU target -> napi build in msys2 shell -> smoke test in default shell"

requirements-completed: [SCAF-03, INTG-01, INTG-02]

# Metrics
duration: 7min
completed: 2026-03-04
---

# Phase 4 Plan 02: CI Workflow Rust Build Integration Summary

**dtolnay/rust-toolchain + napi-rs build steps inserted into all 4 CI workflows; Windows C++ msbuild/vcpkg/mailcore2 steps fully removed and replaced with MSYS2 GNU toolchain; 8MB binary size gate and smoke tests added for all 5 platform targets**

## Performance

- **Duration:** 7 min
- **Started:** 2026-03-04T01:03:42Z
- **Completed:** 2026-03-04T01:10:42Z
- **Tasks:** 2
- **Files modified:** 4

## Accomplishments

- Inserted Rust toolchain, cargo cache, napi-rs build (x86_64-unknown-linux-gnu), 8MB binary size gate, and smoke test into build-linux.yaml; trimmed C++-only system packages from apt-get install list
- Inserted Rust toolchain, cargo cache, napi-rs build (aarch64-unknown-linux-gnu), and smoke test into build-linux-arm64.yaml; same C++ package trim as Linux x64
- Inserted arch-conditional Rust toolchain (aarch64-apple-darwin / x86_64-apple-darwin based on matrix.arch), cargo cache, napi-rs build, and smoke test into build-macos.yaml
- Removed entire C++ Native Build section from build-windows.yaml (7 steps: vcpkg setup, vcpkg install, mailcore2 headers, libetpan build, mailcore2 build, mailsync build, binary copy); replaced with MSYS2 MINGW64 setup, libnode.dll generation via gendef+dlltool, Rust GNU toolchain, cargo cache, napi-rs build, and smoke test

## Task Commits

Each task was committed atomically:

1. **Task 1: Add Rust build steps to Linux and macOS CI workflows** - `92e93d0` (chore)
2. **Task 2: Replace C++ build steps with Rust build in Windows CI workflow** - `34eb37b` (chore)

**Plan metadata:** (docs commit — see below)

## Files Created/Modified

- `.github/workflows/build-linux.yaml` - Trimmed C++ system deps; added Rust toolchain + cargo cache + napi build (x86_64-unknown-linux-gnu) + 8MB size gate + smoke test between Install Dependencies and Lint
- `.github/workflows/build-linux-arm64.yaml` - Trimmed C++ system deps; added Rust toolchain + cargo cache + napi build (aarch64-unknown-linux-gnu) + smoke test between Install Dependencies and Lint
- `.github/workflows/build-macos.yaml` - Added arch-conditional Rust toolchain + cargo cache + napi build + smoke test between Setup Codesigning and Lint
- `.github/workflows/build-windows.yaml` - Removed 7 C++ build steps; added MSYS2 setup + libnode.dll generation + Rust GNU toolchain + cargo cache + napi build (msys2 shell) + smoke test (default shell) before Lint

## Decisions Made

- Insert Rust steps after `npm ci` and before `Lint` in all workflows — `npm ci` must run first to create the `node_modules/mailcore-napi` symlink so the napi build can locate the package; the napi build must run before `Build` (electron-packager) which bundles the `.node` binary
- 8MB binary size gate applied to Linux x64 only (per user decision from Phase 4 planning) — the same `.node` binary ships on all platforms so the check needs to happen only once; Linux x64 is the canonical CI runner for this gate
- Windows smoke test uses default shell (pwsh/cmd), not msys2 — the smoke test only invokes `node -e`, which uses the `actions/setup-node` Node.js; no MinGW64 tools are needed for the smoke test step
- MSYS2 shell (`shell: msys2 {0}`) used only for gendef, dlltool, and the napi build steps on Windows — these require MinGW64 tools in PATH; all other Windows steps (Lint, Build, signing) use the default shell unchanged

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered

None.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- All 4 CI workflows now produce `.node` binaries for their respective platform targets via Rust napi-rs
- Linux x64 CI enforces the 8MB binary size limit with a `cargo-bloat` hint on failure
- All workflows verify addon correctness via smoke test (`require('mailcore-napi').providerForEmail`)
- Windows builds use the established GNU toolchain pattern (MSYS2 + libnode.dll) consistent with Phase 1 local build instructions
- Azure Trusted Signing step on Windows unchanged — `files-folder-filter: exe,dll,node` already covers `.node` files
- Phase 4 is now complete — both plans executed

## Self-Check: PASSED

- .github/workflows/build-linux.yaml: FOUND
- .github/workflows/build-linux-arm64.yaml: FOUND
- .github/workflows/build-macos.yaml: FOUND
- .github/workflows/build-windows.yaml: FOUND
- .planning/phases/04-cross-platform-packaging-and-cleanup/04-02-SUMMARY.md: FOUND
- Commit 92e93d0: FOUND
- Commit 34eb37b: FOUND

---
*Phase: 04-cross-platform-packaging-and-cleanup*
*Completed: 2026-03-04*
