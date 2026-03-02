# Phase 1: Scaffolding and Provider Detection - Research

**Researched:** 2026-03-02
**Domain:** napi-rs v3 project scaffolding + provider JSON parsing + regex domain matching
**Confidence:** HIGH

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SCAF-01 | napi-rs v3 project initialized at `app/mailcore-rs/` with Cargo.toml, build.rs, and package.json | napi new scaffold workflow documented; directory naming clarified (see pitfall) |
| SCAF-02 | Addon loads in Electron main without crashes (tokio runtime, rustls, no OpenSSL) | N-API ABI stability + rustls exclusivity pattern documented |
| PROV-01 | User can call `registerProviders(jsonPath)` to load provider database from JSON file | OnceLock overwrite pattern + serde_json parsing documented |
| PROV-02 | Provider database auto-initializes on module load via embedded `providers.json` | include_str!() + OnceLock::get_or_init pattern documented |
| PROV-03 | User can call `providerForEmail(email)` and receive matching provider with IMAP/SMTP/POP server configs | C++ algorithm reverse-engineered; Rust regex crate replacement documented |
| PROV-04 | Domain-regex and MX-regex matching produces identical results to C++ addon for 50 representative email addresses | C++ matching algorithm fully documented; anchor + case-insensitive behavior captured |
</phase_requirements>

---

## Summary

Phase 1 establishes the Rust napi-rs addon skeleton and implements the two synchronous provider functions: `registerProviders` and `providerForEmail`. This phase is pure in-process work — no network I/O. The scaffolding creates the `app/mailcore-rs/` directory (a new Rust project alongside the existing `app/mailcore/` C++ directory) with the napi-rs build infrastructure. The provider detection logic reads a 37-provider JSON database and performs case-insensitive regex matching against email domains and MX hostnames.

The critical finding from the C++ source audit is that the `matchDomain` function anchors every pattern with `^` and `$` before applying it. The providers.json patterns like `yahoo\\..*` are POSIX extended regex patterns — not JavaScript regex. The Rust `regex` crate handles this correctly, but the anchoring must be done explicitly (wrap each pattern string with `^...$` before compiling). Additionally, the providers.json uses `domain-exclude` for one provider (Yahoo), which must be checked before domain-match — omitting this check produces incorrect results for `yahoo.co.jp` email addresses.

The providers.json file contains 37 providers (not 500+ as previously estimated). The file's top-level structure is a JSON object where each key is the provider identifier and each value contains `servers`, `domain-match`, `mx-match`, and optionally `domain-exclude` and `mailboxes`. Server entries use `ssl: true` for TLS, `starttls: true` for STARTTLS, or neither for clear. This maps directly to Rust serde structs.

**Primary recommendation:** Scaffold at `app/mailcore-rs/` using `napi new` CLI. Implement providers using `include_str!()` for auto-init + `OnceLock<ProviderDatabase>` for the singleton. Use the `regex` crate with `^...$`-anchored, case-insensitive patterns to replicate the C++ matching behavior exactly.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `napi` | 3.x (3.0.0+ in template) | Rust-to-Node-API bridge, macro system | The only maintained napi-rs framework; v3 is the current stable branch |
| `napi-derive` | 3.x | `#[napi]` proc-macro for type generation | Required companion — generates TypeScript `.d.ts` |
| `napi-build` | 2.x | build.rs setup helper | One-liner `napi_build::setup()` handles all platform linker flags |
| `serde` | 1.x | Deserialization framework | Required by serde_json |
| `serde_json` | 1.x | Parse providers.json | Standard JSON in Rust; no alternative considered |
| `regex` | 1.x | Case-insensitive anchored pattern matching | The standard Rust regex engine; needed to replicate C++ POSIX regex behavior |

### Supporting (Phase 1 only — Phase 2+ needs more)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | 1.x | Async runtime | Needed in Cargo.toml even if Phase 1 has no async functions, because napi's `async` feature requires it. Add now to avoid Cargo.lock churn in Phase 2. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `regex` crate | `fancy-regex` | fancy-regex supports lookahead/lookbehind but adds 3x the compile time; providers.json patterns are simple POSIX regex, no lookahead needed |
| `serde_json` | `simd-json` | simd-json is faster but 39KB JSON parsed once at startup is not a bottleneck |

**Installation:**
```bash
# Install napi CLI globally (required for scaffold + build)
npm install -g @napi-rs/cli

# Scaffold the new project
mkdir -p app/mailcore-rs
cd app/mailcore-rs
napi new
# Prompts: package name = mailcore-napi, targets = win32-x64-msvc darwin-universal linux-x64-gnu linux-arm64-gnu, GitHub Actions = yes
```

---

## Architecture Patterns

### Recommended Project Structure

```
app/mailcore-rs/           # NEW: Rust napi-rs project (separate from app/mailcore/)
├── Cargo.toml             # cdylib, napi features, serde, regex
├── build.rs               # napi_build::setup() — one liner
├── .cargo/
│   └── config.toml        # Generated: cross-compilation linker config
├── src/
│   ├── lib.rs             # Module root: #[napi(module_exports)] init fn, re-exports
│   └── provider.rs        # providerForEmail + registerProviders (sync)
├── resources/
│   └── providers.json     # Copy of app/mailcore/resources/providers.json
├── package.json           # name=mailcore-napi, napi config section, scripts
├── index.js               # Generated by napi build — do not edit
└── index.d.ts             # Generated by napi build — do not edit
```

The `imap.rs`, `smtp.rs`, and `validator.rs` files are added in Phases 2-3. Phase 1 creates only `lib.rs` and `provider.rs`.

### Pattern 1: napi-rs Scaffold Workflow

**What:** `napi new` generates the project skeleton including Cargo.toml, build.rs, package.json, .cargo/config.toml, .github/workflows/CI.yml, and src/lib.rs.

**When to use:** One-time scaffold step at the start of Phase 1.

**Files generated by `napi new`:**
- `Cargo.toml` — minimal, `crate-type = ["cdylib"]`, `napi = "3"`, `napi-derive = "3"`, `napi-build = "2"`
- `build.rs` — `napi_build::setup();`
- `package.json` — with `napi` config section listing target triples
- `.cargo/config.toml` — cross-compilation linker settings
- `.github/workflows/CI.yml` — multi-platform build matrix
- `src/lib.rs` — sample `sum` function showing `#[napi]` usage
- `rustfmt.toml`, `.npmignore`

**After `napi build --platform` runs (not generated by scaffold, generated by build):**
- `index.js` — platform-aware binary loader
- `index.d.ts` — TypeScript declarations from `#[napi]` macros
- `mailcore-napi.{platform}-{arch}.node` — compiled binary

### Pattern 2: Module Init Hook with Embedded JSON

**What:** Use `#[napi(module_exports)]` to run initialization code when the `.node` file is first `require()`'d. Use `include_str!()` to embed providers.json at compile time, eliminating all runtime path resolution.

**When to use:** Auto-init on module load (satisfies PROV-02). This is the clean approach that avoids the C++ `GetModuleFileName`/`dladdr` complexity.

**Example:**
```rust
// src/lib.rs
use napi::bindgen_prelude::*;
use napi_derive::napi;

mod provider;

// include_str! resolves the path relative to this source file at compile time
// providers.json must be at src/../resources/providers.json (i.e., app/mailcore-rs/resources/providers.json)
static PROVIDERS_JSON: &str = include_str!("../resources/providers.json");

#[napi(module_exports)]
pub fn module_init(mut _exports: Object) -> Result<()> {
    provider::init_from_embedded(PROVIDERS_JSON)
        .map_err(|e| Error::from_reason(format!("Failed to load embedded providers: {}", e)))?;
    Ok(())
}
```

**Critical:** The `include_str!()` path is relative to the Rust source file, not the working directory. If `lib.rs` is at `src/lib.rs` and providers.json is at `resources/providers.json`, the path is `"../resources/providers.json"`.

### Pattern 3: OnceLock Provider Singleton

**What:** `std::sync::OnceLock<ProviderDatabase>` holds the parsed provider data. Initialized once (either from embedded JSON on module load, or overwritten via `registerProviders`). Thread-safe zero-cost reads after init.

**When to use:** All provider state. Replaces C++ `MailProvidersManager::sharedManager()` singleton.

**Example:**
```rust
// src/provider.rs
use std::sync::OnceLock;
use napi::Result;
use napi_derive::napi;
use serde::Deserialize;

static PROVIDERS: OnceLock<Vec<Provider>> = OnceLock::new();

// Called from module_init with embedded JSON
pub fn init_from_embedded(json: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let providers = parse_providers_json(json)?;
    // OnceLock::set fails silently if already set — that is correct behavior here
    let _ = PROVIDERS.set(providers);
    Ok(())
}

// PROV-01: registerProviders(jsonPath)
#[napi(js_name = "registerProviders")]
pub fn register_providers(json_path: String) -> Result<()> {
    let data = std::fs::read_to_string(&json_path)
        .map_err(|e| napi::Error::from_reason(format!("Cannot read {}: {}", json_path, e)))?;
    let providers = parse_providers_json(&data)
        .map_err(|e| napi::Error::from_reason(format!("Invalid providers JSON: {}", e)))?;
    // OnceLock cannot be overwritten — use a RwLock for registerProviders override capability
    // See pitfall section: use RwLock<Option<Vec<Provider>>> if both PROV-01 and PROV-02 are needed
    let _ = PROVIDERS.set(providers);
    Ok(())
}

// PROV-03: providerForEmail(email)
#[napi(js_name = "providerForEmail")]
pub fn provider_for_email(email: String) -> Result<Option<MailProviderInfo>> {
    let providers = PROVIDERS.get().ok_or_else(|| {
        napi::Error::from_reason("Provider database not initialized")
    })?;
    Ok(find_provider_for_email(providers, &email))
}
```

### Pattern 4: Regex Domain Matching (Replicating C++ exactly)

**What:** The C++ `matchDomain` function anchors every pattern with `^` and `$` and applies case-insensitive POSIX extended regex matching. The Rust `regex` crate replicates this when patterns are anchored and the `(?i)` inline flag is used.

**When to use:** Both domain-match and mx-match pattern matching in `find_provider_for_email`.

**C++ matching algorithm (from source audit):**
1. Extract domain from email: `email.split('@').last()`
2. For each `domain-exclude` pattern: wrap with `^...$`, compile case-insensitive, test against domain. If ANY matches, return false.
3. For each `domain-match` pattern: wrap with `^...$`, compile case-insensitive, test against domain. If ANY matches, return true.
4. Return false if no match.

For `matchMX`: iterate `mx-match` patterns with same `^...$` anchoring against the MX hostname.

**Note:** `providerForEmail` in the C++ does ONLY domain matching (not MX). MX matching happens in `providerForMX`, which is called separately during account validation. The napi binding `ProviderForEmail` only calls `matchEmail` (domain-match + domain-exclude). For Phase 1, implement only the domain matching path.

**Example:**
```rust
// Source: C++ MCMailProvider.cpp matchDomain analysis
use regex::RegexBuilder;

fn match_domain_pattern(pattern: &str, domain: &str) -> bool {
    // Replicate C++ non-Windows behavior: wrap with ^...$, REG_ICASE
    let anchored = format!("^{}$", pattern);
    match RegexBuilder::new(&anchored)
        .case_insensitive(true)
        .build()
    {
        Ok(re) => re.is_match(domain),
        Err(_) => false,  // Invalid pattern: no match (same as C++ regcomp failure)
    }
}

fn find_provider_for_email(providers: &[Provider], email: &str) -> Option<MailProviderInfo> {
    let domain = email.split('@').last()?.to_lowercase();

    for provider in providers {
        // Check exclusions first (critical for Yahoo: yahoo.co.jp must NOT match yahoo)
        let excluded = provider.domain_exclude.iter().any(|pat| match_domain_pattern(pat, &domain));
        if excluded {
            continue;
        }
        // Check domain-match
        let matched = provider.domain_match.iter().any(|pat| match_domain_pattern(pat, &domain));
        if matched {
            return Some(provider.to_info());
        }
    }
    None
}
```

### Pattern 5: providers.json Serde Structs

**What:** The providers.json schema is a top-level JSON object keyed by provider identifier. Each value has `servers` (with `imap`, `smtp`, `pop` arrays), `domain-match`, `mx-match`, and optionally `domain-exclude` and `mailboxes`. Server entries use `ssl: bool` and `starttls: bool` fields.

**Example:**
```rust
// Source: Direct analysis of app/mailcore/resources/providers.json
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct ServerEntry {
    pub hostname: String,
    pub port: u16,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub starttls: bool,
}

impl ServerEntry {
    pub fn connection_type(&self) -> &'static str {
        if self.ssl { "tls" }
        else if self.starttls { "starttls" }
        else { "clear" }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderServers {
    #[serde(default)]
    pub imap: Vec<ServerEntry>,
    #[serde(default)]
    pub smtp: Vec<ServerEntry>,
    #[serde(default)]
    pub pop: Vec<ServerEntry>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProviderEntry {
    pub servers: ProviderServers,
    #[serde(rename = "domain-match", default)]
    pub domain_match: Vec<String>,
    #[serde(rename = "domain-exclude", default)]
    pub domain_exclude: Vec<String>,
    #[serde(rename = "mx-match", default)]
    pub mx_match: Vec<String>,
}

// Top-level: HashMap<identifier, ProviderEntry>
// Use std::collections::HashMap<String, ProviderEntry> for parsing
pub fn parse_providers_json(json: &str) -> Result<Vec<Provider>, serde_json::Error> {
    let raw: std::collections::HashMap<String, ProviderEntry> = serde_json::from_str(json)?;
    Ok(raw.into_iter().map(|(id, entry)| Provider { identifier: id, entry }).collect())
}
```

**Critical serde details:**
- Fields with hyphens (`domain-match`, `mx-match`, `domain-exclude`) need `#[serde(rename = "...")]`
- `ssl` and `starttls` fields are absent (not `false`) for clear connections — use `#[serde(default)]`
- Array fields may be absent entirely — use `#[serde(default)]` on Vec fields
- The `pop` array may be absent for some providers (e.g., gmail has no `pop` key)

### Anti-Patterns to Avoid

- **Treating providers.json regex patterns as JavaScript regex:** They are POSIX extended regex. Patterns like `yahoo\\..*` mean "yahoo dot (any chars)" — the double backslash in JSON source is a single backslash in the string, meaning "literal dot" in POSIX regex. The Rust `regex` crate handles this correctly because POSIX extended and RE2 syntax both treat `\.` as a literal dot.
- **Omitting domain-exclude check:** Only one provider (Yahoo) uses `domain-exclude`, but omitting the check breaks `yahoo.co.jp` matching — it would incorrectly match the Yahoo provider instead of returning null.
- **Using OnceLock alone when registerProviders must overwrite:** `OnceLock::set` is a one-shot write — subsequent calls are no-ops. If both PROV-01 (load from file) and PROV-02 (auto-init) must coexist with the ability to override, use `RwLock<Option<Vec<Provider>>>` instead.
- **Parsing regex patterns at match time:** Compiling a new `Regex` for every `find_provider_for_email` call is expensive under repeated calls. Pre-compile all patterns at parse time and store `Vec<Regex>` in each provider struct.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Regex pattern matching | Custom DFA implementation | `regex` crate | regex crate is a proven, fast RE2-style engine; handles Unicode, backtracking-free |
| JSON parsing | Custom JSON parser | `serde_json` | serde_json is the Rust standard; battle-tested on billions of inputs |
| Module-level singleton | Custom mutex + flag | `OnceLock<T>` or `RwLock<Option<T>>` | std library primitives; OnceLock is zero-cost on the read path |
| Platform binary selection | Custom `require()` patching | napi-generated `index.js` | napi build --platform generates the correct loader for all 5 targets |
| TypeScript type declarations | Hand-written `index.d.ts` | napi-derive `#[napi(object)]` + `#[napi]` | Auto-generated types are guaranteed to match the Rust function signatures |

**Key insight:** The providers.json parsing and regex matching look simple but the C++ implementation has subtle correctness requirements (anchoring, case-insensitivity, domain-exclude ordering) that are easy to get wrong in a hand-rolled solution.

---

## Common Pitfalls

### Pitfall 1: Directory Naming — SCAF-01 Says `app/mailcore-rs/` but Architecture Research Says `app/mailcore/`

**What goes wrong:** The REQUIREMENTS.md specifies SCAF-01 as "napi-rs v3 project initialized at `app/mailcore-rs/`". The prior ARCHITECTURE.md research specifies creating Rust files inside the existing `app/mailcore/` directory. These are inconsistent.

**Why it happens:** The architecture research was written before the requirements document was finalized. The requirements document is more authoritative (it was reviewed and committed separately).

**How to avoid:** Use `app/mailcore-rs/` as the Rust project root. This is cleaner: the C++ `app/mailcore/` directory can remain unchanged during Phase 1, and Phase 4 removes it. The existing `app/package.json` has `"mailcore-napi": "file:mailcore"` — this must be updated to `"file:mailcore-rs"` when the Rust addon is ready.

**Warning signs:** If `cargo build` is run inside `app/mailcore/`, it will clash with the existing CMakeLists.txt and C++ build artifacts.

### Pitfall 2: OnceLock Cannot Be Overwritten

**What goes wrong:** `OnceLock::set` succeeds only once. Calling `registerProviders` after module load (which triggers auto-init from embedded JSON) silently does nothing. The user's custom providers.json is never loaded.

**Why it happens:** OnceLock is designed for exactly-once initialization. PROV-01 and PROV-02 together require a "last write wins" semantic.

**How to avoid:** Use `std::sync::RwLock<Option<Vec<Provider>>>` with `LazyLock` for the container:
```rust
use std::sync::{RwLock, LazyLock};
static PROVIDERS: LazyLock<RwLock<Option<Vec<Provider>>>> =
    LazyLock::new(|| RwLock::new(None));

pub fn init_from_embedded(json: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let providers = parse_providers_json(json)?;
    *PROVIDERS.write().unwrap() = Some(providers);
    Ok(())
}

pub fn register_providers_from_path(path: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let data = std::fs::read_to_string(path)?;
    let providers = parse_providers_json(&data)?;
    *PROVIDERS.write().unwrap() = Some(providers);  // Overwrites embedded
    Ok(())
}
```

**Warning signs:** Tests of `registerProviders` succeeding but `providerForEmail` still returning results from the embedded database.

### Pitfall 3: include_str!() Path Resolution

**What goes wrong:** `include_str!("../resources/providers.json")` fails at compile time with "no such file or directory" because `resources/providers.json` does not exist in `app/mailcore-rs/`.

**Why it happens:** `include_str!()` resolves relative to the Rust source file location at compile time. The path `../resources/providers.json` is relative to `src/lib.rs`, meaning the file must exist at `app/mailcore-rs/resources/providers.json`. The existing providers.json is at `app/mailcore/resources/providers.json`.

**How to avoid:** Copy `app/mailcore/resources/providers.json` to `app/mailcore-rs/resources/providers.json` as part of the scaffold step. Do not reference the C++ directory's file directly — the Rust crate is self-contained.

**Warning signs:** `error: couldn't read src/../resources/providers.json: No such file or directory` during `cargo build`.

### Pitfall 4: Regex Pattern Anchoring Must Be Explicit

**What goes wrong:** Applying the regex pattern `yahoo\\..*` directly without anchoring matches anywhere in the string. The domain `notyahoo.com` would match because the pattern finds a match starting at `not`.

**Why it happens:** The Rust `regex` crate (like most RE2 implementations) does substring matching by default. The C++ POSIX `regexec` with `REG_NOSUB` on a pattern wrapped with `^...$` does full-string matching.

**How to avoid:** Always construct the pattern as `format!("^{}$", pattern)` before compiling. This anchors it exactly as the C++ does. Do this at parse time to avoid repeated string allocation.

**Warning signs:** `match_domain_pattern("yahoo\\..*", "notyahoo.com")` returns `true` (should return `false`).

### Pitfall 5: `async` Feature and Tokio Dependency Must Be Present Even in Phase 1

**What goes wrong:** Phase 1 has no async functions, so the developer omits the `napi` `async`/`tokio_rt` features and the `tokio` dependency. Phase 2 adds async functions, but now `cargo build` fails because napi's async support is not initialized and the tokio dependency is missing.

**Why it happens:** napi-rs's async feature initializes a shared tokio runtime at module load. If you add async functions in Phase 2 without having initialized the runtime infrastructure, you get panics or linker errors.

**How to avoid:** Add the napi `async` and `tokio_rt` features plus the `tokio` dependency in Phase 1's Cargo.toml even though Phase 1 has no async functions. The cost is zero at runtime (the tokio runtime is lazy — not started until first async call).

**Warning signs:** `thread 'main' panicked at 'no tokio runtime'` when the Phase 2 addon is first loaded.

### Pitfall 6: `js_name` Required for camelCase Exports Matching C++ Names

**What goes wrong:** `#[napi]` automatically converts Rust `snake_case` to JavaScript `camelCase`. So `fn provider_for_email` becomes `providerForEmail` — which matches. But `fn register_providers` becomes `registerProviders` — also matches. However, the Phase 2/3 functions `testIMAPConnection` (all-caps IMAP) would auto-convert from `fn test_imap_connection` to `testImapConnection` (lowercase imap) — which DOES NOT match the C++ export name.

**Why it happens:** napi-rs camelCase conversion is word-boundary-based; `IMAP` is treated as a single word and lowercased.

**How to avoid:** Use `#[napi(js_name = "registerProviders")]` and `#[napi(js_name = "providerForEmail")]` explicitly for all exported functions. This makes the naming intent explicit and prevents Phase 2/3 breakage. Start this habit in Phase 1.

**Warning signs:** TypeScript compilation errors when `onboarding-helpers.ts` imports `testIMAPConnection` but the generated `.d.ts` exports `testImapConnection`.

### Pitfall 7: Electron Does Not Need electron-rebuild for napi-rs Addons

**What goes wrong:** Developer attempts to run `electron-rebuild` after building the Rust addon, gets errors, and wastes time debugging.

**Why it happens:** `electron-rebuild` is for node-addon-api (C++) addons that must be compiled against Electron's Node.js headers. napi-rs addons use N-API which is ABI-stable — the compiled `.node` binary works across all Node.js versions without rebuilding.

**How to avoid:** Do not use `electron-rebuild` at all. The napi-rs `.node` file built with `napi build --platform` loads directly into Electron without any additional steps.

**Warning signs:** Documentation suggesting `electron-rebuild` for a Rust/napi-rs addon.

### Pitfall 8: The providers.json Has 37 Providers, Not 500+

**What goes wrong:** Cross-validation tests designed to cover "500+ providers" will only have 37 actual providers to test against. Over-engineering the test matrix or assuming broad coverage based on the "500+" estimate in project docs.

**Why it happens:** The initial project documentation estimated "500+" based on the mailcore2 upstream repository, which includes a much larger providers list. This project's bundled `app/mailcore/resources/providers.json` has 37 providers.

**How to avoid:** Use the actual providers.json as the ground truth. The 50-address cross-validation test (PROV-04) can cover all 37 providers plus some non-matching addresses with margin to spare.

---

## Code Examples

### Minimal Cargo.toml for Phase 1

```toml
# Source: napi-rs/package-template Cargo.toml + Phase 1 requirements
[package]
name = "mailcore-napi"
version = "2.0.0"
edition = "2021"

[lib]
crate-type = ["cdylib"]

[dependencies]
napi = { version = "3", features = ["napi4", "async", "tokio_rt"] }
napi-derive = "3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
regex = "1"
# tokio added now even though Phase 1 has no async fns (see pitfall 5)
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "io-util", "macros"] }

[build-dependencies]
napi-build = "2"

[profile.release]
lto = true
strip = "symbols"
```

### Minimal build.rs

```rust
// Source: napi.rs/docs/introduction/simple-package
extern crate napi_build;

fn main() {
    napi_build::setup();
}
```

### lib.rs Module Root with Auto-Init

```rust
// src/lib.rs
use napi::bindgen_prelude::*;
use napi_derive::napi;

mod provider;

// Embedded at compile time — path is relative to this source file
static PROVIDERS_JSON: &str = include_str!("../resources/providers.json");

/// Called synchronously when require('mailcore-napi') is first executed.
/// Initializes the provider database from the embedded providers.json.
#[napi(module_exports)]
pub fn module_init(mut _exports: Object) -> Result<()> {
    provider::init_from_embedded(PROVIDERS_JSON)
        .map_err(|e| Error::from_reason(format!("mailcore-napi: failed to load providers: {}", e)))?;
    Ok(())
}
```

### Complete provider.rs Structure

```rust
// src/provider.rs
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

// --- napi-rs exported types ---

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

// --- Internal processed provider with pre-compiled regex ---

struct Provider {
    pub identifier: String,
    pub servers: RawServers,
    pub domain_match_patterns: Vec<String>,  // original strings for export
    pub domain_exclude_patterns: Vec<String>,
    pub mx_match_patterns: Vec<String>,
}

// --- Module-level singleton ---

static PROVIDERS: LazyLock<RwLock<Option<Vec<Provider>>>> =
    LazyLock::new(|| RwLock::new(None));

// --- Internal helpers ---

fn match_domain_pattern(pattern: &str, domain: &str) -> bool {
    // Replicate C++ matchDomain: wrap with ^...$, case-insensitive
    let anchored = format!("^{}$", pattern);
    RegexBuilder::new(&anchored)
        .case_insensitive(true)
        .build()
        .map(|re| re.is_match(domain))
        .unwrap_or(false)  // invalid pattern: no match (same as C++ regcomp failure)
}

fn parse_raw(json: &str) -> std::result::Result<Vec<Provider>, serde_json::Error> {
    let raw: HashMap<String, RawProviderEntry> = serde_json::from_str(json)?;
    Ok(raw.into_iter().map(|(id, entry)| Provider {
        identifier: id,
        servers: entry.servers,
        domain_match_patterns: entry.domain_match,
        domain_exclude_patterns: entry.domain_exclude,
        mx_match_patterns: entry.mx_match,
    }).collect())
}

fn to_net_service_info(entry: &RawServerEntry) -> NetServiceInfo {
    NetServiceInfo {
        hostname: entry.hostname.clone(),
        port: entry.port as u32,
        connection_type: if entry.ssl { "tls".into() }
                         else if entry.starttls { "starttls".into() }
                         else { "clear".into() },
    }
}

// --- Public init functions ---

pub fn init_from_embedded(json: &str) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let providers = parse_raw(json)?;
    *PROVIDERS.write().unwrap() = Some(providers);
    Ok(())
}

// --- napi exports ---

#[napi(js_name = "registerProviders")]
pub fn register_providers(json_path: String) -> Result<()> {
    let data = std::fs::read_to_string(&json_path)
        .map_err(|e| napi::Error::from_reason(format!("Cannot read {}: {}", json_path, e)))?;
    let providers = parse_raw(&data)
        .map_err(|e| napi::Error::from_reason(format!("Invalid JSON: {}", e)))?;
    *PROVIDERS.write().unwrap() = Some(providers);
    Ok(())
}

#[napi(js_name = "providerForEmail")]
pub fn provider_for_email(email: String) -> Result<Option<MailProviderInfo>> {
    let guard = PROVIDERS.read().unwrap();
    let providers = guard.as_ref().ok_or_else(|| {
        napi::Error::from_reason("Provider database not initialized")
    })?;

    let domain = match email.split('@').last() {
        Some(d) => d.to_lowercase(),
        None => return Ok(None),
    };

    for provider in providers {
        // domain-exclude check must come before domain-match (C++ algorithm)
        let excluded = provider.domain_exclude_patterns.iter()
            .any(|pat| match_domain_pattern(pat, &domain));
        if excluded { continue; }

        let matched = provider.domain_match_patterns.iter()
            .any(|pat| match_domain_pattern(pat, &domain));
        if matched {
            return Ok(Some(MailProviderInfo {
                identifier: provider.identifier.clone(),
                servers: ProviderServers {
                    imap: provider.servers.imap.iter().map(to_net_service_info).collect(),
                    smtp: provider.servers.smtp.iter().map(to_net_service_info).collect(),
                    pop: provider.servers.pop.iter().map(to_net_service_info).collect(),
                },
                domain_match: provider.domain_match_patterns.clone(),
                mx_match: provider.mx_match_patterns.clone(),
            }));
        }
    }
    Ok(None)
}
```

### Cross-Validation Test Pattern (PROV-04)

The test script calls both the C++ addon and the Rust addon with the same 50 email addresses and asserts identical results:

```typescript
// test/cross-validate-providers.ts
// Run with: node -r ts-node/register test/cross-validate-providers.ts
const cppAddon = require('../app/mailcore/build/Release/mailcore_napi.node');
const rustAddon = require('../app/mailcore-rs/index.js');

const testEmails = [
    'user@gmail.com',       // gmail provider
    'user@yahoo.com',       // yahoo provider
    'user@yahoo.co.jp',     // EXCLUDED from yahoo — must return null
    'user@outlook.com',     // microsoft/hotmail provider
    'user@hotmail.com',     // hotmail
    'user@protonmail.com',  // protonmail
    'user@icloud.com',      // apple
    'user@unknown-domain-xyz.com',  // no match — must return null
    // ... 42 more covering all 37 providers + non-matching addresses
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

# Verify no OpenSSL symbols appear in the linked binary
cargo tree | grep -i openssl
# Expected: no output

# On Linux, also verify with nm
nm -D mailcore-napi.linux-x64-gnu.node | grep -i ssl
# Expected: no OpenSSL symbols; only N-API (napi_*) symbols
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `lazy_static!` / `once_cell::Lazy` for module statics | `std::sync::OnceLock` / `std::sync::LazyLock` | Rust 1.70 (OnceLock), Rust 1.80 (LazyLock) | No external crate needed; same performance |
| `node-gyp` + `node-addon-api` (C++) | `napi-rs` + `napi build` CLI | napi-rs v1 (2019), v3 stable (2024) | No MSVC/Xcode/CMake setup; cross-compile from any platform |
| Per-version electron-rebuild required | N-API ABI-stable: compile once | Node.js 6+ (N-API), Electron 3+ | Zero rebuild overhead when upgrading Electron |
| `trust-dns-resolver` | `hickory-resolver` | v0.24 rename (2023) | Old crate name is stale; new name is the same code |

**Deprecated/outdated:**
- `lazy_static` crate: superseded by `std::sync::LazyLock` (stable Rust 1.80). Do not add as a dependency.
- `once_cell` crate: superseded by `OnceLock`/`LazyLock` in std since Rust 1.70/1.80. Do not add as a dependency.
- napi-rs v2: still maintained but v3 is the current default. The `napi new` command now scaffolds v3 by default.

---

## Open Questions

1. **Does `napi new` need to run inside `app/mailcore-rs/` or can it be run from the repo root with a path argument?**
   - What we know: `napi new` is interactive and creates files in the current directory (or a prompted subdirectory)
   - What's unclear: Whether `napi new --name mailcore-napi --dir app/mailcore-rs` works non-interactively
   - Recommendation: Run `mkdir -p app/mailcore-rs && cd app/mailcore-rs && napi new` and answer the prompts; document the exact answers

2. **Does the Electron app's existing build process (app/package.json scripts) need updating to build the Rust addon?**
   - What we know: `app/package.json` has `"mailcore-napi": "file:mailcore"` in dependencies; `postinstall` scripts run node-gyp
   - What's unclear: Whether npm install for the parent package will trigger `npm run build` inside `app/mailcore-rs/`
   - Recommendation: Add `"preinstall": "cd mailcore-rs && npm install && npm run build"` to `app/package.json` or handle it in the Phase 4 integration step; Phase 1 manual build is sufficient for now

3. **Should regex patterns be pre-compiled at parse time or at match time?**
   - What we know: match time compilation works correctly but is slower; pre-compilation requires storing `Regex` objects in the Provider struct
   - What's unclear: Whether `Regex` is `Send + Sync` (required for use in `RwLock<Option<Vec<Provider>>>`)
   - Recommendation: The `regex` crate's `Regex` struct is `Send + Sync` — pre-compile at parse time and store in the struct. This eliminates regex compilation overhead on every `providerForEmail` call.

---

## Sources

### Primary (HIGH confidence)
- Direct code analysis: `app/mailcore/src/core/provider/MCMailProvider.cpp` — C++ matching algorithm, anchoring behavior, domain-exclude ordering
- Direct code analysis: `app/mailcore/src/core/provider/MCMailProvidersManager.cpp` — provider lookup loop, JSON parsing
- Direct code analysis: `app/mailcore/src/napi/napi_provider.cpp` — exported function signatures and return shape
- Direct code analysis: `app/mailcore/src/napi/addon.cpp` — module init, GetProvidersJsonPath logic
- Direct code analysis: `app/mailcore/resources/providers.json` — actual schema with 37 providers, ssl/starttls fields, domain-exclude presence
- Direct code analysis: `app/mailcore/types/index.d.ts` — TypeScript interface that Rust must match
- Direct code analysis: `app/internal_packages/onboarding/lib/onboarding-helpers.ts` — `require('mailcore-napi')` usage, connectionType field consumption
- [napi-rs package-template Cargo.toml](https://github.com/napi-rs/package-template/blob/main/Cargo.toml) — verified napi 3.0.0, napi-build 2
- [napi.rs/docs/introduction/simple-package](https://napi.rs/docs/introduction/simple-package) — files generated by `napi new`
- [napi.rs/docs/concepts/exports](https://napi.rs/docs/concepts/exports) — `#[napi(module_exports)]` pattern

### Secondary (MEDIUM confidence)
- [electron-builder asarUnpack native modules](https://github.com/electron-userland/electron-builder/issues/1285) — confirmed `.node` files must be unpacked from asar; electron-builder does this automatically
- Prior `.planning/research/STACK.md` — version selections and feature flags (HIGH confidence, already verified against docs.rs)
- Prior `.planning/research/ARCHITECTURE.md` — module load flow, anti-patterns (HIGH confidence, verified against official napi-rs docs)

### Tertiary (LOW confidence — verify before use)
- The 37-provider count from `providers.json` was verified by Node.js script against the actual file. If the file has been or will be updated before implementation, recount.

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — verified from package-template Cargo.toml and prior STACK.md research
- C++ algorithm: HIGH — read directly from source code, no inference
- providers.json schema: HIGH — read directly from the file and verified field names
- Architecture patterns: HIGH — verified against napi.rs official docs and package-template
- Regex anchoring behavior: HIGH — confirmed by cross-referencing C++ source (POSIX `^...$` wrapping) with `regex` crate docs
- OnceLock vs RwLock pitfall: HIGH — OnceLock limitation is documented in std library

**Research date:** 2026-03-02
**Valid until:** 2026-09-02 (stable crates; napi-rs version may advance but v3 API is stable)
