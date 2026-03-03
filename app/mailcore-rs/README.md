# mailcore-rs — Rust napi-rs Addon

Rust implementation of provider detection for UnifyMail, compiled as a Node.js N-API addon
using [napi-rs](https://napi.rs/) v3. Replaces the C++ addon's `providerForEmail` and
`registerProviders` functions for Phase 1; additional network functions (IMAP, SMTP) are
added in Phases 2–3.

## Overview

This crate compiles as both a `cdylib` (the `.node` native addon) and an `rlib` (for
Rust integration tests in `tests/`). The embedded `resources/providers.json` (37 providers)
is parsed at compile time via `include_str!()`, eliminating runtime path resolution
issues in packaged Electron apps.

The addon is loaded through `app/mailcore-wrapper/` which routes:
- `providerForEmail`, `registerProviders` → this Rust addon
- `validateAccount`, `testIMAPConnection`, `testSMTPConnection` → C++ addon (until Phase 2–3)

## Prerequisites

### Required for all platforms

| Tool | Version | Install |
|------|---------|---------|
| Rust toolchain | stable | `curl https://sh.rustup.rs -sSf \| sh` (or [rustup.rs](https://rustup.rs/)) |
| Node.js | ≥16 | [nodejs.org](https://nodejs.org/) |
| @napi-rs/cli | ^3.0.0 | `npm install` (installed as devDependency in this package) |

### Windows-specific prerequisites

UnifyMail uses the **GNU/MinGW toolchain** (`x86_64-pc-windows-gnu`) rather than MSVC.
This requires:

#### 1. MSYS2 with MinGW-w64

Install MSYS2 from [msys2.org](https://www.msys2.org/), then install MinGW-w64 tools:

```bash
# In MSYS2 MinGW64 terminal:
pacman -S mingw-w64-x86_64-toolchain
```

Ensure `C:\msys64\mingw64\bin` is in your system PATH (needed for `dlltool.exe`).

#### 2. GNU Rust target

```bash
rustup target add x86_64-pc-windows-gnu
```

#### 3. libnode.dll import library

The napi-build crate requires a GNU-format import library for `node.exe`. Standard Node.js
distributions do not include this. Generate it once using MSYS2 tools:

```bash
# In MSYS2 MinGW64 terminal:
# 1. Find your node.exe (typically C:\Program Files\nodejs\node.exe)
NODE_EXE="C:/Program Files/nodejs/node.exe"

# 2. Extract symbol table
gendef "$NODE_EXE"
# This creates node.def in the current directory

# 3. Create the import library
dlltool --dllname node.exe --def node.def --output-lib /tmp/libnode.dll

# 4. Set LIBNODE_PATH for builds (add to your shell profile)
export LIBNODE_PATH=/tmp
```

> **Note:** `/tmp` in MSYS2 maps to `C:\msys64\tmp`. The import library only needs to
> be generated once per Node.js version upgrade.

**Why GNU instead of MSVC?** The standard Windows Node.js distribution is MSVC-built, but
napi-rs uses the stable N-API ABI layer — not the C++ ABI. This means a GNU-compiled `.node`
file loads correctly in a MSVC Node.js process. The MSVC Rust target (`x86_64-pc-windows-msvc`)
requires Visual Studio Build Tools (link.exe), which is not installed in this project's
development environment.

## Building

The Rust addon is built automatically when you run `npm start` from the project root.
For manual builds:

```bash
cd app/mailcore-rs

# Install @napi-rs/cli (first time only)
npm install

# Release build (used by npm start and production)
npm run build
# Equivalent: LIBNODE_PATH=/tmp PATH="$PATH:/c/msys64/mingw64/bin" \
#   napi build --platform --release --target x86_64-pc-windows-gnu

# Debug build (faster compile, unoptimized)
npm run build:debug

# Or from the project root
npm run build:rust
```

The build produces:
- `mailcore-napi-rs.win32-x64-gnu.node` — the native addon binary (gitignored)
- `index.d.ts` — TypeScript declarations (gitignored, auto-generated)

> **Note:** `index.js` is **hand-written** (not generated) and is tracked in git.
> The napi-generated `index.js` uses MSVC/GNU process detection which fails in standard
> Windows Node.js environments. Our custom `index.js` loads the GNU binary directly
> via explicit path, bypassing the detection logic.

## Testing

```bash
cd app/mailcore-rs

# Rust integration tests (16 tests)
cargo test

# JavaScript cross-validation (49 tests — domain matching + server configs + error inputs)
node tests/cross-validate-providers.js

# JavaScript cross-validation with C++ comparison (requires built C++ addon)
CPP_ADDON=1 node tests/cross-validate-providers.js

# Electron main process integration test (7 checks — no BoringSSL conflicts)
npx electron test/electron-integration-test.js
# (from project root)
```

## Development Workflow

### Watch mode (fast iteration)

For rapid development when editing Rust source files:

```bash
# Install cargo-watch (one-time)
cargo install cargo-watch

# Auto-rebuild .node on .rs file changes
cargo watch -x build

# Or with tests on every change
cargo watch -x test
```

### Linting

```bash
cd app/mailcore-rs

# Check formatting (non-destructive)
cargo fmt --check

# Fix formatting in place
cargo fmt

# Run Clippy (warnings as errors)
cargo clippy -- -D warnings

# Shorthand (runs both)
npm run lint
```

Linting is also integrated into `npm run lint` at the project root (via
`grunt eslint` + Rust linting step).

## Architecture

```
app/mailcore-rs/
├── src/
│   ├── lib.rs          — napi module init, include_str!(providers.json), module_exports
│   └── provider.rs     — full provider logic: structs, singleton, parse, init, lookup
├── resources/
│   └── providers.json  — 37 email providers (copy of C++ addon's providers.json)
├── tests/
│   ├── provider_tests.rs         — 16 Rust integration tests (cargo test)
│   └── cross-validate-providers.js — 49 JS tests comparing Rust results vs expected
└── index.js            — hand-written platform-aware binary loader
```

### Key patterns

**LazyLock singleton** — The provider database lives in:
```rust
static PROVIDERS: LazyLock<RwLock<Option<Vec<Provider>>>> = LazyLock::new(|| RwLock::new(None));
```
This pattern supports `registerProviders()` (merge-into-existing semantics) while remaining
thread-safe. `OnceLock` would be simpler but doesn't allow post-init mutations.

**Compile-time resource embedding:**
```rust
static PROVIDERS_JSON: &str = include_str!("../resources/providers.json");
```
Eliminates runtime path resolution across dev/production/packaged Electron environments.

**napi thin-wrapper pattern** — `#[napi]` functions call internal `pub fn` helpers:
```rust
#[napi(js_name = "providerForEmail")]
pub fn provider_for_email(email: String) -> Result<Option<MailProviderInfo>> {
    lookup_provider(&email)  // pub fn — callable from Rust tests without napi context
}
```

**Domain-exclude-first ordering** — For each provider, exclusion patterns are checked
before match patterns. This is critical for Yahoo (yahoo.co.jp excluded from generic yahoo).

**TEST_MUTEX pattern** — Integration tests serialize via a process-global mutex to prevent
races when resetting/re-initializing the singleton between tests.

### Wrapper module

The `app/mailcore-wrapper/` module intercepts all `require('mailcore-napi')` calls
(the C++ addon's package name) and routes them:

```
require('mailcore-napi')
    └── app/mailcore-wrapper/index.js
            ├── providerForEmail()     → app/mailcore-rs/index.js (Rust)
            ├── registerProviders()    → app/mailcore-rs/index.js (Rust)
            ├── validateAccount()      → app/mailcore/build/Release/mailcore_napi.node (C++)
            ├── testIMAPConnection()   → app/mailcore/build/Release/mailcore_napi.node (C++)
            └── testSMTPConnection()  → app/mailcore/build/Release/mailcore_napi.node (C++)
```

Consumer code (`onboarding-helpers.ts`, `mailsync-process.ts`) is unchanged.

## Known Limitations

- **MX-match deferred to Phase 3.** Providers with only `mx-match` entries (no
  `domain-match`) are not matched in Phase 1. `providerForEmail` returns `null` for
  email addresses that would match only via MX lookup. Async DNS resolution is needed.

- **Windows only: GNU target, not MSVC.** The MSVC Rust target requires Visual Studio
  Build Tools. The GNU target works correctly via N-API's stable ABI. Other platforms
  (macOS, Linux) use their native targets without GNU/MSVC concerns.

- **No CI pipeline yet.** Continuous integration is deferred to Phase 4 (packaging).

## Dependency Versions (pinned)

| Crate | Version | Reason |
|-------|---------|--------|
| napi | =3.8.3 | napi-rs v3 stable; exact pin for reproducible builds |
| napi-derive | =3.5.2 | Must match napi version |
| napi-build | =2.3.1 | Build script for linking |
| serde | =1.0.228 | JSON deserialization of providers.json |
| serde_json | =1.0.149 | JSON parser |
| regex | =1.12.3 | Domain pattern matching |
| tokio | =1.50.0 | Async runtime (required by napi async features) |
