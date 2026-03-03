# Phase 1: Scaffolding and Provider Detection - Research

**Researched:** 2026-03-03
**Domain:** napi-rs v3 project scaffolding + provider JSON parsing + regex domain matching
**Confidence:** HIGH

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Switchover and Fallback:**
- Wrapper module routes each function to the correct addon (Rust for providers, C++ for network) — consumer code stays unchanged
- Every commit must leave the app fully functional — no broken-state commits allowed
- All 37 providers are equally important — no priority ordering for validation

**Build Integration:**
- Rust addon build integrates into `npm start` — auto-build so developers don't need a separate command
- Use npm as the package manager (match existing project)
- Watch mode for Rust development — auto-rebuild `.node` when `.rs` files change (cargo-watch or similar)
- **Windows target: `x86_64-pc-windows-gnu` (GNU/MinGW)** — not MSVC. MinGW needs to be installed
- Rust toolchain is not yet installed — setup documentation needed

**API Contract and Types:**
- Stricter TypeScript types than C++ — narrow string unions (e.g., `connectionType: 'tls' | 'starttls' | 'clear'`)
- **Throw on invalid input** — empty string, no '@', malformed emails throw a JS Error instead of returning null
- Return null only for valid emails that don't match any provider
- `registerProviders(jsonPath)` fully implemented with **merge semantics** — file providers override embedded providers on identifier conflict, rather than replacing the entire set
- Include POP server configs in the return value (full compatibility with C++ output shape)

**Testing and Validation:**
- Electron integration test required — must verify the addon loads in a real Electron process without BoringSSL/OpenSSL conflicts

**Error Handling:**
- (Claude's discretion — see below)

**Project Structure:**
- Module-per-function layout in `app/mailcore-rs/src/`
- Rust tests in a separate `tests/` directory (integration-style), not inline `#[cfg(test)]`
- Copy `providers.json` to `app/mailcore-rs/resources/`
- **Use a different package name during development** (e.g., `mailcore-napi-rs`) — rename to `mailcore-napi` at switchover
- Single Cargo crate (no workspace)

**MX Matching Scope:**
- **Domain-match only in Phase 1** — `domain-match` and `domain-exclude` regex patterns implemented
- MX-match deferred to Phase 3

**Logging and Debug:**
- Debug-only logging enabled via environment variable (e.g., `MAILCORE_DEBUG=1` or `RUST_LOG=debug`)
- **Always log provider count on initialization** (sanity check, runs even without debug mode)
- In debug mode, log which provider matched for each `providerForEmail` call

**Dependency Choices:**
- Use the `regex` crate (standard Rust regex) — not `fancy-regex`
- Pre-compile all regex patterns at provider load time — cached for fast lookups
- **Pin exact dependency versions** in Cargo.toml (e.g., `regex = "=1.10.3"`)

**Code Style:**
- Clippy with default warnings enforced
- `#![forbid(unsafe_code)]` — no unsafe Rust
- Integrate Rust linting into `npm run lint` — `cargo fmt --check` and `cargo clippy`

**Documentation:**
- Full README.md in `app/mailcore-rs/` with prerequisites (Rust, MinGW), build steps, testing, and architecture
- Update main project CLAUDE.md with Rust addon build/test commands

### Claude's Discretion

- Switchover timing, fallback behavior, wrapper location
- Debug vs release builds for dev, rebuild strategy, script locations, Grunt integration
- domainMatch/mxMatch in return value, .d.ts generation approach
- Cross-validation scope and infrastructure
- All error handling patterns (error crate, throw vs result object, panic policy, bad-regex handling)
- MX-only provider handling, future MX matching location
- Log output destination (stderr vs Electron console), logging crate choice
- JSON parsing approach (typed structs vs dynamic Value)
- rustfmt config, naming conventions, Cargo.lock policy
- Whether CI goes in Phase 1 or defers to Phase 4

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.

</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SCAF-01 | napi-rs v3 project initialized at `app/mailcore-rs/` with Cargo.toml, build.rs, and package.json | `napi new` scaffold workflow documented; exact files generated listed in Architecture section |
| SCAF-02 | Addon loads in Electron main without crashes (tokio runtime, rustls, no OpenSSL) | N-API ABI stability documented; GNU target DLL risk documented as critical pitfall; rustls exclusivity pattern documented |
| PROV-01 | User can call `registerProviders(jsonPath)` to load provider database from JSON file | RwLock+LazyLock merge pattern documented; serde_json parsing structs fully specified |
| PROV-02 | Provider database auto-initializes on module load via embedded `providers.json` | include_str!() + module_exports init hook pattern documented |
| PROV-03 | User can call `providerForEmail(email)` and receive matching provider with IMAP/SMTP/POP server configs | C++ algorithm fully reverse-engineered; Rust regex replacement documented; full provider.rs example provided |
| PROV-04 | Domain-regex and MX-regex matching produces identical results to C++ addon for 50 representative addresses | C++ matching algorithm captured with anchoring behavior; cross-validation test pattern documented |

</phase_requirements>

---

## Summary

Phase 1 establishes the Rust napi-rs addon skeleton at `app/mailcore-rs/` and implements two synchronous provider functions: `registerProviders` and `providerForEmail`. This is pure in-process work — no network I/O. The scaffolding creates a new Rust project alongside the existing `app/mailcore/` C++ directory, with napi-rs build infrastructure, a wrapper JavaScript module that routes to either the Rust or C++ addon, and a comprehensive README.

The most critical technical finding is the **Windows GNU toolchain runtime risk**: while napi-rs merged support for `x86_64-pc-windows-gnu` in PR #2026 (June 2024), actual runtime loading fails with "Load Node-API [napi_get_last_error_info] from host runtime failed: GetProcAddress failed". The root cause is that the GNU toolchain requires `libnode.dll` to be explicitly loaded before Node-API bindings are accessible, but the current napi-rs implementation uses `GetModuleHandleExW(0, NULL, _)` which fails without the explicit DLL reference. This means the developer's preferred Windows target (`x86_64-pc-windows-gnu`) may not work at runtime and the planner should include a fallback investigation task.

The second critical finding is the **C++ matching algorithm**: the `matchDomain` function anchors every pattern with `^` and `$` before applying it. Providers.json patterns like `yahoo\\..*` are POSIX extended regex — not JavaScript regex. The Rust `regex` crate handles this, but anchoring must be done explicitly. Additionally, domain-exclude must be checked before domain-match (Yahoo uses this), and the current CONTEXT.md specifies merge semantics for `registerProviders` rather than replace-all, which requires `RwLock<Option<Vec<Provider>>>` rather than `OnceLock`.

The providers.json file has 37 providers (not 500+ as initially estimated). The merge semantics requirement (file providers override embedded on identifier conflict) differs from the C++ behavior (which replaces the entire set) — this is a deliberate behavioral improvement.

**Primary recommendation:** Scaffold at `app/mailcore-rs/` using `napi new`. Use `x86_64-pc-windows-msvc` as primary target for development and investigate GNU runtime at a discrete task boundary. Implement providers using `include_str!()` for auto-init + `RwLock<Option<Vec<Provider>>>` for the singleton (required for merge semantics). Use the `regex` crate with `^...$`-anchored, case-insensitive patterns to replicate C++ matching exactly.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `napi` | `=3.3.0` (pin exact) | Rust-to-Node-API bridge, macro system | Only maintained napi-rs framework; v3 is current stable branch |
| `napi-derive` | `=3.3.0` | `#[napi]` proc-macro for TypeScript type generation | Required companion — generates `.d.ts` automatically |
| `napi-build` | `=2.1.3` | build.rs setup helper | One-liner `napi_build::setup()` handles all platform linker flags |
| `serde` | `=1.0.219` | Deserialization framework | Required by serde_json |
| `serde_json` | `=1.0.140` | Parse providers.json | Standard JSON in Rust; battle-tested |
| `regex` | `=1.11.1` | Case-insensitive anchored pattern matching | Standard RE2-style engine; replicates C++ POSIX regex behavior |

### Supporting (Phase 1 — required for Phase 2+ readiness)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | `=1.44.1` | Async runtime | Needed in Cargo.toml even if Phase 1 has no async functions — napi's `async` feature requires it at module initialization. Add now to avoid Cargo.lock churn in Phase 2. |

### Logging (Claude's Discretion — recommendation)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `log` | `=0.4.22` | Logging facade | Lightweight; use `eprintln!` for simplest approach OR add `log` + conditional `env_logger = "=0.11.6"` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `regex` crate | `fancy-regex` | fancy-regex supports lookahead/lookbehind but adds ~3x compile time; providers.json patterns are simple POSIX regex, no lookahead needed. Decision locked. |
| `serde_json` typed structs | `serde_json::Value` dynamic | Dynamic parsing is more flexible but loses compile-time type safety; typed structs produce better errors. Recommendation: typed structs. |
| `log` + `env_logger` | `eprintln!` macros | eprintln! is simpler; `log` crate allows future RUST_LOG integration. Either acceptable for Phase 1. |

**Installation:**
```bash
# Install napi CLI globally (required for scaffold + build)
npm install -g @napi-rs/cli

# Scaffold the new project
mkdir -p app/mailcore-rs
cd app/mailcore-rs
napi new
# Answer prompts: package name = mailcore-napi-rs, targets include win32-x64-msvc (primary), GitHub Actions = defer to Phase 4
```

---

## Architecture Patterns

### Recommended Project Structure

```
app/mailcore-rs/           # NEW: Rust napi-rs project (separate from app/mailcore/)
├── Cargo.toml             # cdylib, napi features, serde, regex — pinned versions
├── Cargo.lock             # Committed (binary artifact, locked builds)
├── build.rs               # napi_build::setup() — one liner
├── .cargo/
│   └── config.toml        # Generated: cross-compilation linker config
├── src/
│   ├── lib.rs             # Module root: #[napi(module_exports)] init fn, re-exports, #![forbid(unsafe_code)]
│   └── provider.rs        # providerForEmail + registerProviders (sync)
├── tests/
│   └── provider_tests.rs  # Integration-style tests (cargo test --test provider_tests)
├── resources/
│   └── providers.json     # Copy of app/mailcore/resources/providers.json
├── package.json           # name=mailcore-napi-rs, napi config section, scripts
├── README.md              # Prerequisites (Rust, MinGW or MSVC), build steps, testing
├── index.js               # Generated by napi build — do not edit
└── index.d.ts             # Generated by napi build — do not edit
```

There is also a **wrapper module** that the consumer requires instead of the Rust or C++ addon directly:
```
app/
└── mailcore-wrapper/
    ├── index.js           # Routes providerForEmail/registerProviders to Rust; validateAccount etc to C++
    └── package.json       # name=mailcore-napi (the import path consumers use)
```

The `imap.rs`, `smtp.rs`, and `validator.rs` files are added in Phases 2-3. Phase 1 creates only `lib.rs` and `provider.rs`.

### Pattern 1: napi-rs Scaffold Workflow

**What:** `napi new` generates the project skeleton including Cargo.toml, build.rs, package.json, .cargo/config.toml, optional .github/workflows/CI.yml, and src/lib.rs.

**When to use:** One-time scaffold step at the start of Phase 1.

**Files generated by `napi new`:**
- `Cargo.toml` — minimal, `crate-type = ["cdylib"]`, `napi = "3"`, `napi-derive = "3"`, `napi-build = "2"`
- `build.rs` — `napi_build::setup();`
- `package.json` — with `napi` config section listing target triples
- `.cargo/config.toml` — cross-compilation linker settings
- Optional `.github/workflows/CI.yml` — multi-platform build matrix (defer to Phase 4)
- `src/lib.rs` — sample `sum` function showing `#[napi]` usage
- `rustfmt.toml`, `.npmignore`

**After `napi build --platform` runs:**
- `index.js` — platform-aware binary loader
- `index.d.ts` — TypeScript declarations from `#[napi]` macros
- `mailcore-napi-rs.{platform}-{arch}.node` — compiled binary

### Pattern 2: Module Init Hook with Embedded JSON

**What:** Use `#[napi(module_exports)]` to run initialization code when the `.node` file is first `require()`'d. Use `include_str!()` to embed providers.json at compile time, eliminating all runtime path resolution.

**When to use:** Auto-init on module load (satisfies PROV-02).

**Example:**
```rust
// src/lib.rs
#![forbid(unsafe_code)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

mod provider;

// include_str! resolves at compile time relative to this source file
// providers.json must be at app/mailcore-rs/resources/providers.json
static PROVIDERS_JSON: &str = include_str!("../resources/providers.json");

/// Called synchronously when require('mailcore-napi-rs') is first executed.
/// Initializes the provider database from embedded providers.json.
#[napi(module_exports)]
pub fn module_init(mut _exports: Object) -> Result<()> {
    provider::init_from_embedded(PROVIDERS_JSON)
        .map_err(|e| Error::from_reason(format!("mailcore-napi-rs: failed to load providers: {}", e)))?;
    Ok(())
}
```

**Critical:** The `include_str!()` path is relative to the Rust source file at compile time. If `lib.rs` is at `src/lib.rs` and providers.json is at `resources/providers.json`, the path is `"../resources/providers.json"`.

### Pattern 3: RwLock Provider Singleton with Merge Semantics

**What:** `std::sync::LazyLock<RwLock<Option<Vec<Provider>>>>` holds the parsed provider data. Initialized from embedded JSON on module load. `registerProviders` merges file providers (file wins on identifier conflict). Thread-safe: multiple readers or one writer.

**Why not OnceLock:** `OnceLock::set` is a one-shot write — subsequent calls are no-ops. The merge semantics requirement means `registerProviders` must overwrite existing entries. `RwLock<Option<Vec<Provider>>>` supports this. (See Pitfall 2.)

**Example:**
```rust
// src/provider.rs
use std::sync::{RwLock, LazyLock};

static PROVIDERS: LazyLock<RwLock<Option<Vec<Provider>>>> =
    LazyLock::new(|| RwLock::new(None));

// Called from module_init with embedded JSON (PROV-02)
pub fn init_from_embedded(json: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let providers = parse_providers_json(json)?;
    eprintln!("mailcore-napi-rs: loaded {} providers from embedded JSON", providers.len());
    *PROVIDERS.write().unwrap() = Some(providers);
    Ok(())
}

// PROV-01: registerProviders(jsonPath) — merge semantics
// File providers override embedded providers on identifier conflict
#[napi(js_name = "registerProviders")]
pub fn register_providers(json_path: String) -> Result<()> {
    let data = std::fs::read_to_string(&json_path)
        .map_err(|e| napi::Error::from_reason(format!("Cannot read {}: {}", json_path, e)))?;
    let new_providers = parse_providers_json(&data)
        .map_err(|e| napi::Error::from_reason(format!("Invalid providers JSON: {}", e)))?;

    let mut guard = PROVIDERS.write().unwrap();
    let existing = guard.get_or_insert_with(Vec::new);
    // Merge: file providers override on identifier conflict
    for new_p in new_providers {
        if let Some(pos) = existing.iter().position(|p| p.identifier == new_p.identifier) {
            existing[pos] = new_p;
        } else {
            existing.push(new_p);
        }
    }
    eprintln!("mailcore-napi-rs: merged providers, total: {}", existing.len());
    Ok(())
}
```

### Pattern 4: Regex Domain Matching (Replicating C++ Exactly)

**What:** The C++ `matchDomain` function anchors every pattern with `^` and `$` and applies case-insensitive POSIX extended regex matching. The Rust `regex` crate replicates this when patterns are anchored and case-insensitive mode is enabled.

**C++ matching algorithm (from MCMailProvider.cpp source audit):**
1. Extract domain from email: `email.split('@').last()`
2. For each `domain-exclude` pattern: wrap with `^...$`, compile case-insensitive, test domain. If ANY matches, return false.
3. For each `domain-match` pattern: wrap with `^...$`, compile case-insensitive, test domain. If ANY matches, return true.
4. Return false if no match.

**Important:** `providerForEmail` in the C++ does ONLY domain matching (not MX). MX matching is a separate function called during account validation. For Phase 1, implement domain matching only.

**Input validation (CONTEXT.md locked decision):** Throw on invalid input — empty string, no '@', malformed email. Return null for valid emails with no provider match.

**Example:**
```rust
// Source: C++ MCMailProvider.cpp matchDomain analysis
use regex::RegexBuilder;

fn match_domain_pattern(pattern: &str, domain: &str) -> bool {
    // Replicate C++ non-Windows behavior: wrap with ^...$, REG_ICASE
    let anchored = format!("^{}$", pattern);
    RegexBuilder::new(&anchored)
        .case_insensitive(true)
        .build()
        .map(|re| re.is_match(domain))
        .unwrap_or(false)  // invalid pattern: no match (same as C++ regcomp failure)
}

#[napi(js_name = "providerForEmail")]
pub fn provider_for_email(email: String) -> Result<Option<MailProviderInfo>> {
    // Throw on invalid input (CONTEXT.md locked decision)
    if email.is_empty() {
        return Err(napi::Error::from_reason("providerForEmail: email must not be empty"));
    }
    if !email.contains('@') {
        return Err(napi::Error::from_reason("providerForEmail: email must contain '@'"));
    }

    let guard = PROVIDERS.read().unwrap();
    let providers = guard.as_ref().ok_or_else(|| {
        napi::Error::from_reason("Provider database not initialized")
    })?;

    let domain = email.split('@').last()
        .ok_or_else(|| napi::Error::from_reason("providerForEmail: cannot extract domain"))?
        .to_lowercase();

    for provider in providers {
        // domain-exclude check MUST come before domain-match (critical for Yahoo)
        let excluded = provider.domain_exclude_patterns.iter()
            .any(|pat| match_domain_pattern(pat, &domain));
        if excluded { continue; }

        let matched = provider.domain_match_patterns.iter()
            .any(|pat| match_domain_pattern(pat, &domain));
        if matched {
            // Debug logging (CONTEXT.md: log matched provider in debug mode)
            if std::env::var("MAILCORE_DEBUG").is_ok() {
                eprintln!("mailcore-napi-rs: {} matched provider {}", email, provider.identifier);
            }
            return Ok(Some(provider_to_info(provider)));
        }
    }
    Ok(None)  // valid email, no provider match
}
```

### Pattern 5: providers.json Serde Structs

**What:** The providers.json schema is a top-level JSON object keyed by provider identifier. Each value has `servers` (with `imap`, `smtp`, `pop` arrays), `domain-match`, `mx-match`, and optionally `domain-exclude`. Server entries use `ssl: bool` and `starttls: bool` boolean fields.

**Example:**
```rust
// Source: Direct analysis of app/mailcore/resources/providers.json
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize, Clone)]
struct RawServerEntry {
    pub hostname: String,
    pub port: u16,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub starttls: bool,
}

impl RawServerEntry {
    pub fn connection_type(&self) -> &'static str {
        if self.ssl { "tls" }
        else if self.starttls { "starttls" }
        else { "clear" }
    }
}

#[derive(Debug, Deserialize, Clone)]
struct RawServers {
    #[serde(default)]
    pub imap: Vec<RawServerEntry>,
    #[serde(default)]
    pub smtp: Vec<RawServerEntry>,
    #[serde(default)]
    pub pop: Vec<RawServerEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawProviderEntry {
    pub servers: RawServers,
    #[serde(rename = "domain-match", default)]
    pub domain_match: Vec<String>,
    #[serde(rename = "domain-exclude", default)]
    pub domain_exclude: Vec<String>,
    #[serde(rename = "mx-match", default)]
    pub mx_match: Vec<String>,
    // mailboxes field exists in JSON but is not needed in Phase 1
}

pub fn parse_providers_json(json: &str) -> std::result::Result<Vec<Provider>, serde_json::Error> {
    let raw: HashMap<String, RawProviderEntry> = serde_json::from_str(json)?;
    Ok(raw.into_iter().map(|(id, entry)| Provider {
        identifier: id,
        servers: entry.servers,
        domain_match_patterns: entry.domain_match,
        domain_exclude_patterns: entry.domain_exclude,
        mx_match_patterns: entry.mx_match,
    }).collect())
}
```

**Critical serde details:**
- Fields with hyphens (`domain-match`, `mx-match`, `domain-exclude`) require `#[serde(rename = "...")]`
- `ssl` and `starttls` fields are absent (not `false`) for clear connections — use `#[serde(default)]`
- Array fields may be absent entirely — use `#[serde(default)]` on Vec fields
- The `pop` array may be absent for some providers — `#[serde(default)]` handles this

### Pattern 6: napi-rs Exported Object Types

**What:** Use `#[napi(object)]` to define types that cross the Rust-JS boundary. All fields must be `pub`. Nested `#[napi(object)]` structs are supported. `Vec<T>` maps to JavaScript arrays. `String` fields map to JavaScript strings.

**Example:**
```rust
// napi(object) types auto-generate TypeScript interfaces in index.d.ts
#[napi(object)]
pub struct NetServiceInfo {
    pub hostname: String,
    pub port: u32,
    pub connection_type: String,  // "tls" | "starttls" | "clear"
}

#[napi(object)]
pub struct ProviderServers {
    pub imap: Vec<NetServiceInfo>,
    pub smtp: Vec<NetServiceInfo>,
    pub pop: Vec<NetServiceInfo>,
}

#[napi(object)]
pub struct MailProviderInfo {
    pub identifier: String,
    pub servers: ProviderServers,
    pub domain_match: Vec<String>,
    pub mx_match: Vec<String>,
}
```

**Note on `connectionType` naming:** napi-rs auto-converts snake_case to camelCase, so `connection_type` becomes `connectionType` in the generated TypeScript interface. This matches the existing `index.d.ts` type. Verify this conversion when building.

### Pattern 7: Wrapper Module for Incremental Migration

**What:** A JavaScript wrapper module that `require('mailcore-napi')` calls (the existing import path). Routes provider functions to the Rust addon, network functions (validateAccount, testIMAPConnection, testSMTPConnection) to the C++ addon. Consumer code remains unchanged.

**When to use:** From day one of Phase 1. Every commit must leave the app functional.

**Example:**
```javascript
// app/mailcore-wrapper/index.js
// Routes each function to the correct addon
// Provider functions: Rust addon (Phase 1+)
// Network functions: C++ addon until Phases 2-3 replace them

let rustAddon = null;
let cppAddon = null;

function getRust() {
    if (!rustAddon) {
        rustAddon = require('../mailcore-rs/index.js');
    }
    return rustAddon;
}

function getCpp() {
    if (!cppAddon) {
        cppAddon = require('../mailcore/build/Release/mailcore_napi.node');
    }
    return cppAddon;
}

// Phase 1: Route to Rust
exports.providerForEmail = (email) => getRust().providerForEmail(email);
exports.registerProviders = (jsonPath) => getRust().registerProviders(jsonPath);

// Phase 2-3: Route to C++ until replaced
exports.validateAccount = (opts) => getCpp().validateAccount(opts);
exports.testIMAPConnection = (opts) => getCpp().testIMAPConnection(opts);
exports.testSMTPConnection = (opts) => getCpp().testSMTPConnection(opts);
```

### Anti-Patterns to Avoid

- **GNU target as first priority:** The `x86_64-pc-windows-gnu` target has documented runtime loading failures with `GetProcAddress`. Start with MSVC target for development; investigate GNU separately (see Pitfall 9).
- **Using OnceLock for registerable singleton:** `OnceLock::set` is one-shot. Use `LazyLock<RwLock<Option<Vec<Provider>>>>` instead to support merge semantics.
- **Omitting domain-exclude check:** Only Yahoo uses `domain-exclude`, but omitting the check breaks `yahoo.co.jp` matching.
- **Parsing regex at match time:** Compiling a new `Regex` for every `providerForEmail` call is expensive. Pre-compile at parse time.
- **Treating providers.json regex as JavaScript regex:** They are POSIX extended regex — `yahoo\\..*` means "yahoo dot (any chars)". The Rust `regex` crate handles POSIX regex correctly, but the `^...$` anchoring must be added explicitly.
- **electron-rebuild for Rust addons:** N-API is ABI-stable. No rebuilding needed when upgrading Electron. `electron-rebuild` is for C++ node-addon-api addons only.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Regex pattern matching | Custom DFA implementation | `regex` crate | Proven, fast RE2-style engine; handles Unicode, backtracking-free |
| JSON parsing | Custom JSON parser | `serde_json` | Standard in Rust; battle-tested |
| Module-level singleton | Custom mutex + flag | `LazyLock<RwLock<Option<T>>>` | std library primitives; correct memory ordering guaranteed |
| Platform binary selection | Custom `require()` patching | napi-generated `index.js` | `napi build --platform` generates the correct platform loader |
| TypeScript type declarations | Hand-written `index.d.ts` | napi-derive `#[napi(object)]` + `#[napi]` | Auto-generated types are guaranteed to match Rust function signatures |
| Wrapper routing logic | Full re-implementation | Simple `require()` wrapper in JS | C++ addon is already working; wrapper is 20 lines, not a build system |

**Key insight:** The providers.json matching logic has subtle correctness requirements (anchoring, case-insensitivity, domain-exclude ordering, merge semantics) that are easy to get wrong. Use the C++ source as specification and the regex crate as the implementation.

---

## Common Pitfalls

### Pitfall 1: Directory Naming

**What goes wrong:** REQUIREMENTS.md specifies SCAF-01 as `app/mailcore-rs/`. Running `cargo build` inside `app/mailcore/` clashes with the existing CMakeLists.txt and C++ build artifacts.

**How to avoid:** Create `app/mailcore-rs/` as the Rust project root. The C++ `app/mailcore/` remains unchanged during Phase 1.

**Warning signs:** CMake errors or missing `binding.gyp` errors during Rust build.

### Pitfall 2: OnceLock Cannot Be Overwritten (Merge Semantics Requires RwLock)

**What goes wrong:** `OnceLock::set` succeeds only once. Calling `registerProviders` after module load (which triggers auto-init from embedded JSON) silently does nothing. The user's custom providers.json is never merged.

**Root cause:** OnceLock is designed for exactly-once initialization. PROV-01 requires overwrite/merge capability. Additionally, the CONTEXT.md specifies merge semantics specifically (file providers override on identifier conflict), not replace-all.

**How to avoid:**
```rust
use std::sync::{RwLock, LazyLock};
static PROVIDERS: LazyLock<RwLock<Option<Vec<Provider>>>> =
    LazyLock::new(|| RwLock::new(None));
```

**Warning signs:** Tests of `registerProviders` succeeding but `providerForEmail` still returning results from the embedded database only.

### Pitfall 3: include_str!() Path Resolution

**What goes wrong:** `include_str!("../resources/providers.json")` fails at compile time because `resources/providers.json` does not exist in `app/mailcore-rs/`.

**How to avoid:** Copy `app/mailcore/resources/providers.json` to `app/mailcore-rs/resources/providers.json` as part of the scaffold step. The Rust crate must be self-contained.

**Warning signs:** `error: couldn't read src/../resources/providers.json: No such file or directory` during `cargo build`.

### Pitfall 4: Regex Pattern Anchoring Must Be Explicit

**What goes wrong:** Applying pattern `yahoo\\..*` directly without anchoring matches anywhere in the string. The domain `notyahoo.com` would incorrectly match.

**Root cause:** Rust `regex` crate does substring matching by default. C++ POSIX `regexec` with the pattern wrapped in `^...$` does full-string matching.

**How to avoid:** Always construct `format!("^{}$", pattern)` before compiling. Do this at parse time, not at match time.

**Warning signs:** `match_domain_pattern("yahoo\\..*", "notyahoo.com")` returns `true` (should return `false`).

### Pitfall 5: Tokio Dependency Required Even in Phase 1

**What goes wrong:** Phase 1 has no async functions, so the developer omits `tokio` and napi `async` features. Phase 2 adds async functions, and `cargo build` fails because napi's async support was not initialized.

**How to avoid:** Add napi `async` and `tokio_rt` features plus `tokio` dependency in Phase 1's Cargo.toml. The tokio runtime is lazy — not started until the first async call.

**Warning signs:** `thread 'main' panicked at 'no tokio runtime'` when Phase 2 addon loads.

### Pitfall 6: js_name Required for camelCase Exports

**What goes wrong:** napi-rs auto-converts snake_case to camelCase. `fn test_imap_connection` becomes `testImapConnection` (lowercase imap), which DOES NOT match the C++ export name `testIMAPConnection`.

**How to avoid:** Use `#[napi(js_name = "registerProviders")]` and `#[napi(js_name = "providerForEmail")]` explicitly for all exported functions. Start this habit in Phase 1 to prevent Phase 2-3 breakage.

**Warning signs:** TypeScript compilation errors when `onboarding-helpers.ts` imports `testIMAPConnection` but the generated `.d.ts` exports `testImapConnection`.

### Pitfall 7: Electron Does Not Need electron-rebuild for napi-rs Addons

**What goes wrong:** Developer attempts to run `electron-rebuild` after building the Rust addon.

**Root cause:** `electron-rebuild` is for node-addon-api (C++) addons. napi-rs addons use N-API which is ABI-stable across Node.js versions — the compiled `.node` file works directly in Electron without rebuilding.

**How to avoid:** Do not use `electron-rebuild`. The napi-rs `.node` file built with `napi build --platform` loads directly into Electron 39.

**Warning signs:** Documentation suggesting `electron-rebuild` for a Rust/napi-rs addon.

### Pitfall 8: The providers.json Has 37 Providers, Not 500+

**What goes wrong:** Cross-validation tests designed to cover "500+ providers" will only have 37 actual providers.

**Root cause:** The initial project documentation estimated "500+" based on the mailcore2 upstream repository. This project's bundled `app/mailcore/resources/providers.json` has 37 providers.

**How to avoid:** Use the actual file as ground truth. The 50-address cross-validation test (PROV-04) can cover all 37 providers plus non-matching addresses.

### Pitfall 9: CRITICAL — Windows GNU Target Has Runtime DLL Loading Failure

**What goes wrong:** The `x86_64-pc-windows-gnu` target produces a `.node` file that fails to load with: "Load Node-API [napi_get_last_error_info] from host runtime failed: GetProcAddress failed".

**Root cause (HIGH confidence from napi-rs issue #2001):** The GNU toolchain requires explicitly loading `libnode.dll` before accessing Node-API bindings. The current napi-rs implementation uses `GetModuleHandleExW(0, NULL, _)` which fails without the explicit DLL reference. Node.js does not officially release binaries for the windows-gnu target, compounding the problem.

**Historical context:** A separate issue (#1175, napi-sys 2.2.1) broke Windows+Electron due to thread-local storage — this was fixed in PR #1176. The GNU-specific GetProcAddress issue is a different, unresolved problem.

**How to handle:** For Phase 1 development, use the `x86_64-pc-windows-msvc` target. Investigate whether GNU target works in practice in a dedicated task. If GNU fails at runtime, document as a known limitation — MSVC produces valid Electron addons and avoids the DLL loading problem. If MinGW is specifically required by the developer, a Windows MSVC toolchain installation task may need to precede the Rust scaffold task.

**Warning signs:** Loading the `.node` file in Electron throws `GetProcAddress failed`. This happens at `require()` time, not at build time.

### Pitfall 10: cargo-watch Not Directly Supported for npm Integration on Windows

**What goes wrong:** The CONTEXT.md requires watch mode for Rust development (auto-rebuild `.node` when `.rs` files change). `cargo-watch` is a separate binary that runs `cargo build` on file change but does not automatically copy the output `.node` file or restart Electron.

**How to handle:** Implement watch mode as a custom npm script using `cargo-watch` + file copy. The exact integration approach is Claude's discretion. A simple approach: `cargo watch -x build -s "npm run copy-node"` in the `app/mailcore-rs/` directory. For auto-restart: the `concurrently` package (already in root devDependencies) can run electron and cargo-watch simultaneously.

**Warning signs:** Developer runs `cargo watch` manually and the updated `.node` file is not picked up by Electron because the old `.node` is still cached in `require()`.

---

## Code Examples

### Minimal Cargo.toml for Phase 1 (Pinned Versions)

```toml
# Source: napi-rs/package-template + Phase 1 requirements analysis
[package]
name = "mailcore-napi-rs"
version = "2.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

# MSRV: Rust 1.80 (required for LazyLock stable)
# rust-version = "1.80"

[dependencies]
napi = { version = "=3.3.0", features = ["napi4", "async", "tokio_rt"] }
napi-derive = "=3.3.0"
serde = { version = "=1.0.219", features = ["derive"] }
serde_json = "=1.0.140"
regex = "=1.11.1"
# tokio added now even though Phase 1 has no async fns (see Pitfall 5)
tokio = { version = "=1.44.1", features = ["rt-multi-thread", "net", "time", "io-util", "macros"] }

[build-dependencies]
napi-build = "=2.1.3"

[profile.release]
lto = true
strip = "symbols"
```

**Note on pinning:** CONTEXT.md requires exact version pinning (`"=1.10.3"` syntax in Cargo.toml). The versions above are current as of 2026-03-03. Verify exact latest patch versions via `cargo search` before committing Cargo.toml.

### Minimal build.rs

```rust
// Source: napi.rs/docs/introduction/simple-package
extern crate napi_build;

fn main() {
    napi_build::setup();
}
```

### Complete lib.rs

```rust
// src/lib.rs
#![deny(clippy::all)]
#![forbid(unsafe_code)]

use napi::bindgen_prelude::*;
use napi_derive::napi;

mod provider;

// Embedded at compile time — path relative to this source file (src/lib.rs)
// File must exist at app/mailcore-rs/resources/providers.json
static PROVIDERS_JSON: &str = include_str!("../resources/providers.json");

/// Called synchronously when require('mailcore-napi-rs') is first executed.
/// Initializes the provider database from the embedded providers.json.
/// Always logs provider count; logs match details when MAILCORE_DEBUG=1.
#[napi(module_exports)]
pub fn module_init(mut _exports: Object) -> Result<()> {
    provider::init_from_embedded(PROVIDERS_JSON)
        .map_err(|e| Error::from_reason(format!("mailcore-napi-rs: failed to load providers: {}", e)))?;
    Ok(())
}
```

### Complete provider.rs Structure

```rust
// src/provider.rs
#![allow(dead_code)]  // Some fields used in future phases

use std::sync::{RwLock, LazyLock};
use napi::Result;
use napi_derive::napi;
use regex::RegexBuilder;
use serde::Deserialize;
use std::collections::HashMap;

// --- Serde structs matching providers.json schema ---

#[derive(Debug, Deserialize, Clone)]
struct RawServerEntry {
    pub hostname: String,
    pub port: u16,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub starttls: bool,
}

#[derive(Debug, Deserialize, Clone)]
struct RawServers {
    #[serde(default)]
    pub imap: Vec<RawServerEntry>,
    #[serde(default)]
    pub smtp: Vec<RawServerEntry>,
    #[serde(default)]
    pub pop: Vec<RawServerEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct RawProviderEntry {
    pub servers: RawServers,
    #[serde(rename = "domain-match", default)]
    pub domain_match: Vec<String>,
    #[serde(rename = "domain-exclude", default)]
    pub domain_exclude: Vec<String>,
    #[serde(rename = "mx-match", default)]
    pub mx_match: Vec<String>,
}

// --- napi-rs exported types (auto-generate TypeScript interfaces) ---

#[napi(object)]
pub struct NetServiceInfo {
    pub hostname: String,
    pub port: u32,
    pub connection_type: String,  // "tls" | "starttls" | "clear"
}

#[napi(object)]
pub struct ProviderServers {
    pub imap: Vec<NetServiceInfo>,
    pub smtp: Vec<NetServiceInfo>,
    pub pop: Vec<NetServiceInfo>,
}

#[napi(object)]
pub struct MailProviderInfo {
    pub identifier: String,
    pub servers: ProviderServers,
    pub domain_match: Vec<String>,
    pub mx_match: Vec<String>,
}

// --- Internal provider struct with pre-compiled regex patterns ---

struct Provider {
    identifier: String,
    servers: RawServers,
    // Pre-compiled regex at parse time (pattern, compiled_regex)
    domain_match_compiled: Vec<(String, regex::Regex)>,
    domain_exclude_compiled: Vec<(String, regex::Regex)>,
    mx_match_patterns: Vec<String>,  // raw strings for export; MX matching deferred to Phase 3
}

// --- Module-level singleton ---

static PROVIDERS: LazyLock<RwLock<Option<Vec<Provider>>>> =
    LazyLock::new(|| RwLock::new(None));

// --- Internal helpers ---

fn compile_pattern(pattern: &str) -> Option<regex::Regex> {
    // Replicate C++ matchDomain: wrap with ^...$, REG_ICASE
    let anchored = format!("^{}$", pattern);
    RegexBuilder::new(&anchored)
        .case_insensitive(true)
        .build()
        .ok()
}

fn parse_providers_json(json: &str) -> std::result::Result<Vec<Provider>, serde_json::Error> {
    let raw: HashMap<String, RawProviderEntry> = serde_json::from_str(json)?;
    Ok(raw.into_iter().map(|(id, entry)| {
        // Pre-compile all regex patterns at parse time
        let domain_match_compiled = entry.domain_match.iter()
            .filter_map(|pat| compile_pattern(pat).map(|re| (pat.clone(), re)))
            .collect();
        let domain_exclude_compiled = entry.domain_exclude.iter()
            .filter_map(|pat| compile_pattern(pat).map(|re| (pat.clone(), re)))
            .collect();

        Provider {
            identifier: id,
            servers: entry.servers,
            domain_match_compiled,
            domain_exclude_compiled,
            mx_match_patterns: entry.mx_match,
        }
    }).collect())
}

fn server_to_net_service(entry: &RawServerEntry) -> NetServiceInfo {
    NetServiceInfo {
        hostname: entry.hostname.clone(),
        port: entry.port as u32,
        connection_type: if entry.ssl { "tls".into() }
                         else if entry.starttls { "starttls".into() }
                         else { "clear".into() },
    }
}

fn provider_to_info(p: &Provider) -> MailProviderInfo {
    MailProviderInfo {
        identifier: p.identifier.clone(),
        servers: ProviderServers {
            imap: p.servers.imap.iter().map(server_to_net_service).collect(),
            smtp: p.servers.smtp.iter().map(server_to_net_service).collect(),
            pop: p.servers.pop.iter().map(server_to_net_service).collect(),
        },
        domain_match: p.domain_match_compiled.iter().map(|(s, _)| s.clone()).collect(),
        mx_match: p.mx_match_patterns.clone(),
    }
}

// --- Public init (called from lib.rs module_init) ---

pub fn init_from_embedded(json: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let providers = parse_providers_json(json)?;
    eprintln!("mailcore-napi-rs: loaded {} providers from embedded JSON", providers.len());
    *PROVIDERS.write().unwrap() = Some(providers);
    Ok(())
}

// --- napi exports ---

/// Load providers from a custom JSON file (merge semantics: file overrides on identifier conflict).
#[napi(js_name = "registerProviders")]
pub fn register_providers(json_path: String) -> Result<()> {
    let data = std::fs::read_to_string(&json_path)
        .map_err(|e| napi::Error::from_reason(format!("Cannot read {}: {}", json_path, e)))?;
    let new_providers = parse_providers_json(&data)
        .map_err(|e| napi::Error::from_reason(format!("Invalid providers JSON in {}: {}", json_path, e)))?;

    let mut guard = PROVIDERS.write().unwrap();
    let existing = guard.get_or_insert_with(Vec::new);
    for new_p in new_providers {
        if let Some(pos) = existing.iter().position(|p| p.identifier == new_p.identifier) {
            existing[pos] = new_p;
        } else {
            existing.push(new_p);
        }
    }
    eprintln!("mailcore-napi-rs: merged providers, total: {}", existing.len());
    Ok(())
}

/// Look up a mail provider by email address (synchronous, in-memory).
/// Throws for invalid input (empty, no '@'). Returns null for unrecognized domains.
#[napi(js_name = "providerForEmail")]
pub fn provider_for_email(email: String) -> Result<Option<MailProviderInfo>> {
    if email.is_empty() {
        return Err(napi::Error::from_reason("providerForEmail: email must not be empty"));
    }
    if !email.contains('@') {
        return Err(napi::Error::from_reason("providerForEmail: email must contain '@'"));
    }

    let guard = PROVIDERS.read().unwrap();
    let providers = guard.as_ref()
        .ok_or_else(|| napi::Error::from_reason("mailcore-napi-rs: provider database not initialized"))?;

    let domain = email.split('@').last()
        .unwrap()  // safe: we verified '@' exists above
        .to_lowercase();

    for provider in providers {
        // domain-exclude MUST be checked before domain-match (C++ algorithm)
        let excluded = provider.domain_exclude_compiled.iter()
            .any(|(_, re)| re.is_match(&domain));
        if excluded { continue; }

        let matched = provider.domain_match_compiled.iter()
            .any(|(_, re)| re.is_match(&domain));
        if matched {
            if std::env::var("MAILCORE_DEBUG").is_ok() {
                eprintln!("mailcore-napi-rs: {} matched provider {}", email, provider.identifier);
            }
            return Ok(Some(provider_to_info(provider)));
        }
    }
    Ok(None)
}
```

### Cross-Validation Test Pattern (PROV-04)

```typescript
// app/mailcore-rs/tests/cross-validate-providers.ts (or .js)
// Run with: node app/mailcore-rs/tests/cross-validate-providers.js
// Requires both C++ and Rust addons to be built

const cppAddon = require('../../mailcore/build/Release/mailcore_napi.node');
const rustAddon = require('../index.js');

const testEmails = [
    'user@gmail.com',               // gmail provider
    'user@yahoo.com',               // yahoo provider (domain-match)
    'user@yahoo.co.jp',             // EXCLUDED from yahoo — must return null
    'user@outlook.com',             // microsoft/outlook provider
    'user@hotmail.com',             // hotmail
    'user@protonmail.com',          // protonmail
    'user@icloud.com',              // apple
    'user@unknown-xyz-domain.com',  // no match — must return null
    // ... more covering all 37 providers + non-matching addresses
];

let failures = 0;
for (const email of testEmails) {
    const cpp = cppAddon.providerForEmail(email);
    const rust = rustAddon.providerForEmail(email);

    const cppId = cpp ? cpp.identifier : null;
    const rustId = rust ? rust.identifier : null;

    if (cppId !== rustId) {
        console.error(`FAIL ${email}: C++ says ${cppId}, Rust says ${rustId}`);
        failures++;
    } else {
        console.log(`PASS ${email}: ${rustId}`);
    }
}
process.exit(failures > 0 ? 1 : 0);
```

### Verifying No OpenSSL in the Binary (SCAF-02)

```bash
# After building: app/mailcore-rs/
cd app/mailcore-rs

# Verify no OpenSSL symbols appear in the dependency tree
cargo tree | grep -i openssl
# Expected: no output (Phase 1 has no network code, so no TLS deps)

# On Linux, also verify with nm
nm -D mailcore-napi-rs.linux-x64-gnu.node | grep -i ssl
# Expected: no OpenSSL symbols; only N-API (napi_*) symbols
```

### Electron Integration Test Pattern (SCAF-02)

```javascript
// test/electron-integration-test.js
// Run with: electron test/electron-integration-test.js
const { app } = require('electron');

app.whenReady().then(() => {
    try {
        const addon = require('./app/mailcore-rs/index.js');
        const result = addon.providerForEmail('test@gmail.com');
        if (result && result.identifier === 'gmail') {
            console.log('PASS: Addon loaded in Electron, provider lookup works');
            process.exit(0);
        } else {
            console.error('FAIL: Provider lookup returned unexpected result', result);
            process.exit(1);
        }
    } catch (e) {
        console.error('FAIL: Addon failed to load in Electron', e.message);
        process.exit(1);
    }
});
```

### package.json for app/mailcore-rs/

```json
{
  "name": "mailcore-napi-rs",
  "version": "2.0.0",
  "description": "Rust napi-rs addon for provider detection",
  "main": "index.js",
  "types": "index.d.ts",
  "private": true,
  "napi": {
    "binaryName": "mailcore-napi-rs",
    "targets": [
      "x86_64-pc-windows-msvc",
      "aarch64-apple-darwin",
      "x86_64-apple-darwin",
      "x86_64-unknown-linux-gnu",
      "aarch64-unknown-linux-gnu"
    ]
  },
  "scripts": {
    "build": "napi build --platform --release",
    "build:debug": "napi build --platform",
    "watch": "cargo watch -x build",
    "lint": "cargo fmt --check && cargo clippy -- -D warnings"
  }
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `lazy_static!` / `once_cell::Lazy` | `std::sync::OnceLock` / `std::sync::LazyLock` | Rust 1.70 (OnceLock), Rust 1.80 (LazyLock) | No external crate needed; same performance |
| `node-gyp` + `node-addon-api` (C++) | `napi-rs` + `napi build` CLI | napi-rs v1 (2019), v3 stable (2024) | No MSVC/Xcode/CMake setup required |
| Per-version electron-rebuild required | N-API ABI-stable: compile once | Node.js 6+ (N-API), Electron 3+ | Zero rebuild overhead when upgrading Electron |
| `trust-dns-resolver` | `hickory-resolver` | v0.24 rename (2023) | Old crate name is stale (Phase 3 concern) |
| `napi new` scaffolds v2 by default | `napi new` scaffolds v3 by default | napi-rs v3 release (2024) | Use `napi = "3"`, `napi-derive = "3"` |

**Deprecated/outdated:**
- `lazy_static` crate: superseded by `std::sync::LazyLock` (stable Rust 1.80). Do not add as a dependency.
- `once_cell` crate: superseded by `OnceLock`/`LazyLock` in std. Do not add as a dependency.
- napi-rs v2: still maintained but v3 is the current default.

---

## Open Questions

1. **Does `x86_64-pc-windows-gnu` target actually load at runtime in Electron 39?**
   - What we know: napi-rs PR #2026 (June 2024) added compile support; runtime loading fails with GetProcAddress for the generic case
   - What's unclear: Whether there is a MinGW-specific fix that resolves the DLL loading issue; whether Electron 39's specific Node.js ABI changes anything
   - Recommendation: Attempt with MSVC first; include a discrete investigation task for GNU target runtime loading

2. **Does `napi new` need to run inside `app/mailcore-rs/` or with a path argument?**
   - What we know: `napi new` is interactive and creates files in the current directory
   - What's unclear: Whether `--dir app/mailcore-rs` works non-interactively
   - Recommendation: Run `mkdir -p app/mailcore-rs && cd app/mailcore-rs && napi new`; document exact prompt answers

3. **Watch mode restart behavior: rebuild-only vs rebuild+restart Electron?**
   - Claude's discretion per CONTEXT.md
   - Recommendation: rebuild-only (cargo-watch rebuilds the `.node` file; developer manually restarts Electron via `CTRL+R`). Full restart loop requires `concurrently` + complex process management and is low value for Phase 1.

4. **Should regex patterns be pre-compiled at parse time?**
   - What we know: `regex::Regex` is `Send + Sync` — safe to store in `RwLock<Vec<Provider>>`. Pre-compilation eliminates regex compilation on every `providerForEmail` call (37 providers × N patterns = significant overhead at high call rates).
   - Recommendation: YES — pre-compile at parse time and store the compiled `Regex` objects in the `Provider` struct alongside the original pattern strings.

5. **Should `domainMatch[]` and `mxMatch[]` appear in the return value?**
   - Claude's discretion per CONTEXT.md
   - Current C++ behavior: YES (returns `domainMatch` and `mxMatch` arrays)
   - The existing `onboarding-helpers.ts` uses `napiProvider.domainMatch` (line 126 in the source)
   - Recommendation: YES — include both `domain_match` and `mx_match` in `MailProviderInfo` for full C++ compatibility. The `onboarding-helpers.ts` consumer uses them.

---

## Validation Architecture

> `workflow.nyquist_validation` is absent from `.planning/config.json` — treating as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test runner (`cargo test`) + standalone Node.js scripts |
| Config file | None — `cargo test` runs `tests/*.rs` by default |
| Quick run command | `cd app/mailcore-rs && cargo test` |
| Full suite command | `cd app/mailcore-rs && cargo test && node tests/cross-validate-providers.js` |

### Phase Requirements to Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SCAF-01 | Cargo.toml, build.rs, package.json exist at `app/mailcore-rs/` | manual | `ls app/mailcore-rs/{Cargo.toml,build.rs,package.json}` | Wave 0 |
| SCAF-02 | Addon loads in Electron without crashes | integration | `electron test/electron-integration-test.js` | Wave 0 |
| PROV-01 | `registerProviders(jsonPath)` loads and merges provider database | integration | `cargo test --test provider_tests -- test_register_providers` | Wave 0 |
| PROV-02 | Auto-init from embedded JSON on module load | integration | `cargo test --test provider_tests -- test_auto_init` | Wave 0 |
| PROV-03 | `providerForEmail(email)` returns correct provider with IMAP/SMTP/POP configs | unit | `cargo test --test provider_tests -- test_provider_for_email` | Wave 0 |
| PROV-04 | Domain-regex matching identical to C++ for 50 addresses | integration | `node app/mailcore-rs/tests/cross-validate-providers.js` | Wave 0 |

### Sampling Rate

- **Per task commit:** `cd app/mailcore-rs && cargo test`
- **Per wave merge:** `cd app/mailcore-rs && cargo test && node tests/cross-validate-providers.js`
- **Phase gate:** Full suite green before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `app/mailcore-rs/tests/provider_tests.rs` — covers PROV-01, PROV-02, PROV-03
- [ ] `app/mailcore-rs/tests/cross-validate-providers.js` — covers PROV-04 (requires both addons built)
- [ ] `test/electron-integration-test.js` — covers SCAF-02 (requires Electron)
- [ ] `app/mailcore-rs/resources/providers.json` — required for SCAF-01 build
- [ ] Framework install: `cargo install cargo-watch` — for watch mode development

*(The Rust test runner is built into cargo; no additional test framework install needed for `cargo test`.)*

---

## Sources

### Primary (HIGH confidence)
- Direct code analysis: `app/mailcore/src/core/provider/MCMailProvider.cpp` — C++ matching algorithm, anchoring behavior, domain-exclude ordering
- Direct code analysis: `app/mailcore/src/core/provider/MCMailProvidersManager.cpp` — provider lookup loop, JSON parsing, singleton pattern
- Direct code analysis: `app/mailcore/src/napi/napi_provider.cpp` — exported function signatures and return shape
- Direct code analysis: `app/mailcore/resources/providers.json` — actual schema with 37 providers, ssl/starttls fields, domain-exclude presence
- Direct code analysis: `app/mailcore/types/index.d.ts` — TypeScript interface that Rust must match
- Direct code analysis: `app/internal_packages/onboarding/lib/onboarding-helpers.ts` — `require('mailcore-napi')` usage, `domainMatch` field consumption
- Direct code analysis: `app/frontend/mailsync-process.ts` — `validateAccount` usage (routes through C++ for Phase 1)
- Direct code analysis: `app/package.json` — `"mailcore-napi": "file:mailcore"` import path, Electron 39.2.7
- [napi-rs Issue #2001](https://github.com/napi-rs/napi-rs/issues/2001) — x86_64-pc-windows-gnu support status: compile supported (PR #2026 merged), runtime loading fails with GetProcAddress
- [regex crate docs](https://docs.rs/regex) — version 1.12.3; anchored patterns, case-insensitive via RegexBuilder, LazyLock recommendation
- [napi::Error docs](https://docs.rs/napi/latest/napi/struct.Error.html) — Error::from_reason, Result pattern, From conversions

### Secondary (MEDIUM confidence)
- [napi.rs/docs/introduction/simple-package](https://napi.rs/docs/introduction/simple-package) — files generated by `napi new`
- [napi.rs/docs/concepts/values](https://napi.rs/docs/concepts/values) — Rust-to-JS type mapping, Option<T> → null
- [napi.rs/docs/concepts/object](https://napi.rs/docs/concepts/object) — #[napi(object)] struct requirements
- [napi-rs Issue #1175](https://github.com/napi-rs/napi-rs/issues/1175) — Windows+Electron GetProcAddress bug (napi-sys 2.2.1) — RESOLVED in PR #1176. Separate from GNU-specific issue.
- [napi-rs Issue #125](https://github.com/napi-rs/napi-rs/issues/125) — Electron support history — RESOLVED via PR #270
- [napi.rs/docs/cli/build](https://napi.rs/docs/cli/build) — `napi build` flags: --platform, --target, --release, --dts
- [napi.rs/docs/cli/napi-config](https://napi.rs/docs/cli/napi-config) — napi config schema in package.json: binaryName, targets

### Tertiary (LOW confidence — verify before use)
- The 37-provider count from `providers.json` was confirmed by direct file inspection. If the file is updated before implementation, recount.
- Exact pinned version numbers in Cargo.toml (napi 3.3.0, serde_json 1.0.140, etc.) — verify via `cargo search` at implementation time as patch versions may advance.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — verified from napi-rs docs, docs.rs, direct file analysis
- C++ algorithm: HIGH — read directly from source code, no inference
- providers.json schema: HIGH — read directly from the file and verified field names
- Architecture patterns: HIGH — verified against napi.rs official docs
- Regex anchoring behavior: HIGH — confirmed by cross-referencing C++ source with `regex` crate docs
- OnceLock vs RwLock pitfall: HIGH — OnceLock limitation documented in std library; merge semantics requirement from CONTEXT.md
- Windows GNU target risk: HIGH — documented in napi-rs GitHub issue with root cause analysis; LOW confidence on resolution status (may have been patched after research)
- Validation architecture: MEDIUM — test files don't exist yet (Wave 0 gaps); framework works once scaffold is created

**Research date:** 2026-03-03
**Valid until:** 2026-09-03 (stable crates; napi-rs v3 API is stable; GNU target status should be re-verified at implementation time)
