---
phase: 01-scaffolding-and-provider-detection
plan: 01
subsystem: infra
tags: [rust, napi-rs, regex, serde, tokio, provider-detection]

# Dependency graph
requires: []
provides:
  - Rust napi-rs crate at app/mailcore-rs/ that compiles as cdylib+rlib
  - Provider detection via domain-match/domain-exclude regex matching
  - init_from_embedded() that parses 37 providers from compile-time-embedded JSON
  - lookup_provider() internal API callable from Rust tests
  - merge_providers_from_str() / register_providers() with file-wins merge semantics
  - 16 integration tests covering all provider behaviors
affects:
  - 01-02-scaffolding-and-provider-detection (Node.js runtime load test)
  - future-phases (IMAP, SMTP, account validation)

# Tech tracking
tech-stack:
  added:
    - napi = "=3.8.3" (with napi4, async, tokio_rt features)
    - napi-derive = "=3.5.2"
    - napi-build = "=2.3.1"
    - serde = "=1.0.228" (with derive feature)
    - serde_json = "=1.0.149"
    - regex = "=1.12.3"
    - tokio = "=1.50.0" (rt-multi-thread, net, time, io-util, macros)
  patterns:
    - LazyLock<RwLock<Option<Vec<Provider>>>> singleton for mutable global state
    - Regex pre-compilation at load time (stored as (pattern, Regex) tuples)
    - domain-exclude checked before domain-match in every provider iteration
    - pub fn lookup_provider() as thin non-napi wrapper for Rust test access
    - TEST_MUTEX for serial test execution when sharing process-global singletons
    - include_str!() for compile-time embedding of resource files

key-files:
  created:
    - app/mailcore-rs/Cargo.toml
    - app/mailcore-rs/build.rs
    - app/mailcore-rs/package.json
    - app/mailcore-rs/.gitignore
    - app/mailcore-rs/rustfmt.toml
    - app/mailcore-rs/src/lib.rs
    - app/mailcore-rs/src/provider.rs
    - app/mailcore-rs/resources/providers.json
    - app/mailcore-rs/tests/provider_tests.rs
  modified: []

key-decisions:
  - "Use crate-type = [cdylib, rlib] to enable both Node.js addon and Rust integration tests"
  - "Use #![deny(unsafe_code)] not #![forbid(unsafe_code)] — napi macros expand allow(unsafe_code) which forbid blocks"
  - "Expose lookup_provider(), merge_providers_from_str(), provider_count(), reset_providers() as pub for test access without napi context"
  - "GNU Windows target (x86_64-pc-windows-gnu) requires MSYS2 MinGW dlltool.exe and libnode.dll import library — documented in build prerequisites"
  - "libnode.dll generated from node.exe via gendef+dlltool (MSYS2 mingw64 tools); stored at /tmp/libnode.dll for builds"
  - "TEST_MUTEX pattern for serializing integration tests that share process-global singleton state"

patterns-established:
  - "LazyLock singleton pattern: static PROVIDERS: LazyLock<RwLock<Option<Vec<T>>>> for mutable global with merge semantics"
  - "napi thin-wrapper pattern: #[napi] functions call pub fn internals; internals are independently testable"
  - "domain-exclude-first ordering: iterate exclude list before match list for every provider lookup"
  - "Compile-time resource embedding: static JSON: &str = include_str!(relative path)"

requirements-completed: [SCAF-01, PROV-01, PROV-02, PROV-03]

# Metrics
duration: 11min
completed: 2026-03-03
---

# Phase 01 Plan 01: Scaffolding and Provider Detection Summary

**Rust napi-rs crate scaffolded at app/mailcore-rs/ with embedded 37-provider database, domain-regex matching (anchored, case-insensitive, exclude-first), merge semantics, and 16 passing integration tests**

## Performance

- **Duration:** 11 min
- **Started:** 2026-03-03T21:28:00Z
- **Completed:** 2026-03-03T21:39:00Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Rust crate compiles as both cdylib (Node.js addon) and rlib (Rust tests) with zero warnings
- All 37 embedded providers parse from compile-time-embedded providers.json
- Provider matching: anchored regex (`^...$`), case-insensitive, domain-exclude checked before domain-match
- Yahoo/yahoo.co.jp exclusion works correctly — yahoo.co.jp matches dedicated provider, NOT international yahoo
- Merge semantics: file providers override existing by identifier, new identifiers append
- 16 integration tests all pass, using TEST_MUTEX for serial execution of shared-singleton tests
- Zero OpenSSL in dependency tree (verified via cargo tree)
- Clippy -D warnings clean, cargo fmt --check passes

## Task Commits

Each task was committed atomically:

1. **Task 1: Scaffold Rust napi-rs project and implement provider logic** - `404db9e` (feat)
2. **Task 2: Rust integration tests for provider logic** - `ea9f5c6` (test)

_Note: Both tasks used TDD flow — implementation verified via tests in GREEN phase._

## Files Created/Modified
- `app/mailcore-rs/Cargo.toml` - Pinned deps: napi 3.8.3, serde 1.0.228, regex 1.12.3, tokio 1.50.0; cdylib+rlib crate types
- `app/mailcore-rs/build.rs` - napi_build::setup() for Windows linking
- `app/mailcore-rs/package.json` - napi-rs config with 5 platform targets (x86_64-pc-windows-gnu for Windows)
- `app/mailcore-rs/.gitignore` - Excludes target/, *.node, index.js, index.d.ts
- `app/mailcore-rs/rustfmt.toml` - Default formatting (edition 2021)
- `app/mailcore-rs/src/lib.rs` - Module init, PROVIDERS_JSON embed, module_exports
- `app/mailcore-rs/src/provider.rs` - Full provider logic: serde structs, singleton, compile_pattern, parse, init, register, lookup, conversions
- `app/mailcore-rs/resources/providers.json` - Verbatim copy of C++ providers.json (37 providers)
- `app/mailcore-rs/tests/provider_tests.rs` - 16 integration tests with TEST_MUTEX isolation

## Decisions Made

- Used `crate-type = ["cdylib", "rlib"]` instead of just `cdylib` — required to allow `tests/` integration tests to import the crate as a library. The rlib is not shipped but enables the test binary to link against provider logic.

- Changed `#![forbid(unsafe_code)]` to `#![deny(unsafe_code)]` — napi-rs macro expansions internally emit `#[allow(unsafe_code)]` which conflicts with `forbid`. The `deny` level still prevents any unsafe code in our own source files while allowing macro expansions to work.

- Exposed internal functions as `pub` (not `pub(crate)`) for integration test access: `init_from_embedded`, `lookup_provider`, `merge_providers_from_str`, `provider_count`, `reset_providers`. The napi-decorated functions are thin wrappers; the internal functions contain all logic and are testable without a Node.js runtime.

- Added `TEST_MUTEX: Mutex<()>` for serializing integration tests that share the PROVIDERS singleton. Without it, tests that call `reset_providers` + `init_from_embedded` race with tests performing lookups, causing flaky failures.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Changed #![forbid(unsafe_code)] to #![deny(unsafe_code)]**
- **Found during:** Task 1 (first cargo build)
- **Issue:** napi-rs v3 macros emit `allow(unsafe_code)` internally, which E0453 rejects when `forbid` is active at crate level
- **Fix:** Changed to `#![deny(unsafe_code)]` — equivalent protection for our own code, allows macro expansions
- **Files modified:** app/mailcore-rs/src/lib.rs
- **Verification:** cargo build succeeds with zero warnings
- **Committed in:** 404db9e (Task 1 commit)

**2. [Rule 1 - Bug] Added crate-type rlib for integration test support**
- **Found during:** Task 2 (first cargo test)
- **Issue:** Rust `cdylib`-only crates cannot be imported by integration tests in `tests/` — the linker has no rlib to link against
- **Fix:** Added `"rlib"` to `crate-type` array in Cargo.toml
- **Files modified:** app/mailcore-rs/Cargo.toml
- **Verification:** cargo test compiles and runs all 16 tests
- **Committed in:** 404db9e (Task 1 commit, included in scaffold)

**3. [Rule 1 - Bug] Added TEST_MUTEX to serialize integration tests**
- **Found during:** Task 2 (tests run in parallel)
- **Issue:** Integration tests share the PROVIDERS global singleton. Parallel execution caused `register_providers_should_override_gmail` to overwrite gmail's IMAP hostname just before `provider_for_email_should_return_gmail_imap_server_config` read it, causing a flaky failure
- **Fix:** Added `static TEST_MUTEX: Mutex<()>` + `init_locked()` helper that acquires the mutex before each test. Tests run serially, each fully isolated
- **Files modified:** app/mailcore-rs/tests/provider_tests.rs
- **Verification:** cargo test passes 16/16 consistently
- **Committed in:** ea9f5c6 (Task 2 commit)

**4. [Rule 3 - Blocking] Generated libnode.dll import library for Windows GNU build**
- **Found during:** Task 1 (first cargo build)
- **Issue:** napi-build v2.3.1 on Windows GNU target requires `libnode.dll` in search path. Stock Node.js installation doesn't include this file
- **Fix:** Used MSYS2 gendef + dlltool to extract symbol table from node.exe and create a GNU import library at /tmp/libnode.dll. Set LIBNODE_PATH=/tmp for builds
- **Files modified:** None (build environment setup)
- **Verification:** cargo build proceeds past napi-build and compiles successfully
- **Note:** This build prerequisite must be documented in app/mailcore-rs/README.md (Plan 01-02 task)

---

**Total deviations:** 4 auto-fixed (2 bugs, 1 bug/test isolation, 1 blocking build prerequisite)
**Impact on plan:** All fixes necessary for correctness and build success. No scope creep.

## Issues Encountered

- **MinGW dlltool.exe required for GNU Windows builds:** The napi-build crate's Windows GNU support requires `libnode.dll` (an import library for Node.js symbols). This is NOT shipped with standard Node.js. Required generating it manually using MSYS2's gendef + dlltool from node.exe. This needs to be documented as a developer setup prerequisite in the README. MSYS2 is already installed at C:\msys64\ on this machine.

## User Setup Required

None — no external service configuration required. However, developer workstation setup for building the Rust crate on Windows requires:
1. MSYS2 with MinGW-w64 (`C:\msys64\mingw64\bin` in PATH)
2. `libnode.dll` generated from node.exe and LIBNODE_PATH set
3. Rust GNU toolchain (`rustup default stable-x86_64-pc-windows-gnu`)

This will be documented in app/mailcore-rs/README.md in Plan 01-02.

## Next Phase Readiness

- Rust crate compiles, tests pass, and all 37 providers are correctly parsed and matched
- Plan 01-02 can proceed: test that the compiled `.node` file loads in a real Electron/Node.js process without BoringSSL conflicts
- If GNU `.node` fails to load in Electron (ABI mismatch), Plan 01-02 documents the finding and switches to x86_64-pc-windows-msvc per the documented fallback path in CONTEXT.md

---
*Phase: 01-scaffolding-and-provider-detection*
*Completed: 2026-03-03*
