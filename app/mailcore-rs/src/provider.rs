use napi::bindgen_prelude::*;
use napi_derive::napi;
use regex::{Regex, RegexBuilder};
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

// ---------------------------------------------------------------------------
// Serde deserialization structs — match providers.json schema exactly
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone)]
struct RawServerEntry {
    #[serde(default)]
    pub hostname: Option<String>,
    #[serde(default)]
    pub port: Option<u32>,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default)]
    pub starttls: bool,
    #[serde(default)]
    pub tls: bool,
}

#[derive(Debug, Deserialize, Clone, Default)]
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

// ---------------------------------------------------------------------------
// napi-exported types — must match app/mailcore/types/index.d.ts interface
// ---------------------------------------------------------------------------

/// Connection settings for a single IMAP/SMTP/POP server.
///
/// Field names use snake_case in Rust; napi-rs auto-converts to camelCase for JavaScript
/// (connection_type → connectionType).
#[napi(object)]
#[derive(Clone)]
pub struct NetServiceInfo {
    pub hostname: String,
    /// napi-rs maps u32 to JavaScript number safely. Do NOT use u16.
    pub port: u32,
    pub connection_type: String,
}

/// Grouped server lists for a mail provider.
#[napi(object)]
#[derive(Clone)]
pub struct ProviderServers {
    pub imap: Vec<NetServiceInfo>,
    pub smtp: Vec<NetServiceInfo>,
    pub pop: Vec<NetServiceInfo>,
}

/// Full provider info returned by providerForEmail.
///
/// Matches the MailProviderInfo TypeScript interface.
#[napi(object)]
#[derive(Clone)]
pub struct MailProviderInfo {
    pub identifier: String,
    pub servers: ProviderServers,
    pub domain_match: Vec<String>,
    pub mx_match: Vec<String>,
}

// ---------------------------------------------------------------------------
// Internal provider representation with pre-compiled regexes
// ---------------------------------------------------------------------------

pub(crate) struct Provider {
    pub(crate) identifier: String,
    servers: RawServers,
    /// (original_pattern, compiled_regex) pairs for domain matching
    domain_match_compiled: Vec<(String, Regex)>,
    /// (original_pattern, compiled_regex) pairs for domain exclusion
    domain_exclude_compiled: Vec<(String, Regex)>,
    /// Raw MX patterns used in Phase 3 for provider identifier resolution via DNS MX lookup
    pub(crate) mx_match_patterns: Vec<String>,
}

// ---------------------------------------------------------------------------
// Singleton provider database — LazyLock<RwLock<...>> (NOT OnceLock)
// so that register_providers can merge into the existing set.
// ---------------------------------------------------------------------------

pub(crate) static PROVIDERS: LazyLock<RwLock<Option<Vec<Provider>>>> =
    LazyLock::new(|| RwLock::new(None));

// ---------------------------------------------------------------------------
// Pattern compilation helpers
// ---------------------------------------------------------------------------

/// Wrap a raw regex pattern with `^...$` anchors and compile case-insensitively.
///
/// Returns None if the pattern is invalid (skip rather than crash).
fn compile_pattern(pattern: &str) -> Option<Regex> {
    let anchored = format!("^{}$", pattern);
    RegexBuilder::new(&anchored)
        .case_insensitive(true)
        .build()
        .ok()
}

// ---------------------------------------------------------------------------
// Provider parsing
// ---------------------------------------------------------------------------

/// Parse the providers.json content into a Vec of internal Provider structs,
/// pre-compiling all regex patterns.
fn parse_providers_json(json: &str) -> Result<Vec<Provider>> {
    let raw: HashMap<String, RawProviderEntry> =
        serde_json::from_str(json).map_err(|e| Error::from_reason(e.to_string()))?;

    let mut providers = Vec::with_capacity(raw.len());

    for (identifier, entry) in raw {
        let domain_match_compiled: Vec<(String, Regex)> = entry
            .domain_match
            .iter()
            .filter_map(|p| compile_pattern(p).map(|r| (p.clone(), r)))
            .collect();

        let domain_exclude_compiled: Vec<(String, Regex)> = entry
            .domain_exclude
            .iter()
            .filter_map(|p| compile_pattern(p).map(|r| (p.clone(), r)))
            .collect();

        providers.push(Provider {
            identifier,
            servers: entry.servers,
            domain_match_compiled,
            domain_exclude_compiled,
            mx_match_patterns: entry.mx_match,
        });
    }

    Ok(providers)
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

/// Parse and store the embedded providers JSON.
/// Called automatically from lib.rs module_init.
/// Always logs provider count to stderr.
pub fn init_from_embedded(json: &str) -> Result<()> {
    let providers = parse_providers_json(json)?;
    let count = providers.len();

    let mut lock = PROVIDERS
        .write()
        .map_err(|e| Error::from_reason(format!("PROVIDERS write lock poisoned: {e}")))?;
    *lock = Some(providers);

    eprintln!("[mailcore-rs] Loaded {count} providers from embedded JSON");
    Ok(())
}

// ---------------------------------------------------------------------------
// Test helpers (public for integration tests in tests/)
// ---------------------------------------------------------------------------

/// Returns the number of providers currently loaded in the singleton.
///
/// Used by integration tests to verify parse count without exposing Provider internals.
pub fn provider_count() -> usize {
    PROVIDERS
        .read()
        .ok()
        .and_then(|lock| lock.as_ref().map(|p| p.len()))
        .unwrap_or(0)
}

/// Reset the providers singleton (used by integration tests to isolate test state).
///
/// Integration tests run in the same process and share the singleton — calling
/// this at the start of each test function ensures isolation.
pub fn reset_providers() {
    if let Ok(mut lock) = PROVIDERS.write() {
        *lock = None;
    }
}

/// Merge additional providers from a JSON string (used by integration tests).
///
/// Identical merge semantics as register_providers but accepts a JSON string
/// directly rather than a file path (avoids needing a temp file in tests).
pub fn merge_providers_from_str(json: &str) -> Result<()> {
    let new_providers = parse_providers_json(json)?;

    let mut lock = PROVIDERS
        .write()
        .map_err(|e| Error::from_reason(format!("PROVIDERS write lock poisoned: {e}")))?;

    let providers = lock.get_or_insert_with(Vec::new);

    for incoming in new_providers {
        if let Some(existing) = providers
            .iter_mut()
            .find(|p| p.identifier == incoming.identifier)
        {
            *existing = incoming;
        } else {
            providers.push(incoming);
        }
    }

    let total = providers.len();
    eprintln!("[mailcore-rs] merge_providers_from_str: database now has {total} entries");
    Ok(())
}

// ---------------------------------------------------------------------------
// Public API — registered with napi
// ---------------------------------------------------------------------------

/// Load additional providers from a JSON file and merge them into the existing set.
///
/// File providers win on identifier conflict (file overrides embedded).
/// Newly-seen identifiers are appended.
#[napi(js_name = "registerProviders")]
pub fn register_providers(json_path: String) -> Result<()> {
    let content = std::fs::read_to_string(&json_path)
        .map_err(|e| Error::from_reason(format!("Cannot read {json_path}: {e}")))?;

    let new_providers = parse_providers_json(&content)?;

    let mut lock = PROVIDERS
        .write()
        .map_err(|e| Error::from_reason(format!("PROVIDERS write lock poisoned: {e}")))?;

    let providers = lock.get_or_insert_with(Vec::new);

    for incoming in new_providers {
        if let Some(existing) = providers
            .iter_mut()
            .find(|p| p.identifier == incoming.identifier)
        {
            // File wins on conflict — overwrite in-place
            *existing = incoming;
        } else {
            providers.push(incoming);
        }
    }

    let total = providers.len();
    eprintln!("[mailcore-rs] registerProviders: merged providers database now has {total} entries");
    Ok(())
}

// ---------------------------------------------------------------------------
// Internal matching logic (also usable from tests without napi context)
// ---------------------------------------------------------------------------

/// Core domain matching logic. Separated from the napi export so tests can call it directly.
///
/// Returns Ok(Some(info)) on match, Ok(None) on no match, Err on invalid input.
pub fn lookup_provider(email: &str) -> Result<Option<MailProviderInfo>> {
    if email.is_empty() {
        return Err(Error::from_reason(
            "providerForEmail: email must not be empty".to_string(),
        ));
    }

    let at_pos = email.rfind('@').ok_or_else(|| {
        Error::from_reason("providerForEmail: email must contain '@'".to_string())
    })?;

    let domain = email[at_pos + 1..].to_lowercase();

    if domain.is_empty() {
        return Err(Error::from_reason(
            "providerForEmail: domain part must not be empty".to_string(),
        ));
    }

    let debug_enabled = std::env::var("MAILCORE_DEBUG").is_ok();

    let lock = PROVIDERS
        .read()
        .map_err(|e| Error::from_reason(format!("PROVIDERS read lock poisoned: {e}")))?;

    let providers = match lock.as_ref() {
        Some(p) => p,
        None => return Ok(None),
    };

    for provider in providers {
        // Step 1: check domain-exclude FIRST (critical for Yahoo pattern)
        let excluded = provider
            .domain_exclude_compiled
            .iter()
            .any(|(_, re)| re.is_match(&domain));

        if excluded {
            if debug_enabled {
                eprintln!(
                    "[mailcore-rs] providerForEmail('{email}'): domain '{domain}' excluded from provider '{}'",
                    provider.identifier
                );
            }
            continue;
        }

        // Step 2: check domain-match
        let matched = provider
            .domain_match_compiled
            .iter()
            .any(|(_, re)| re.is_match(&domain));

        if matched {
            if debug_enabled {
                eprintln!(
                    "[mailcore-rs] providerForEmail('{email}'): matched provider '{}'",
                    provider.identifier
                );
            }
            return Ok(Some(provider_to_info(provider)));
        }
    }

    if debug_enabled {
        eprintln!(
            "[mailcore-rs] providerForEmail('{email}'): no provider matched domain '{domain}'"
        );
    }

    Ok(None)
}

/// Look up the mail provider for a given email address.
///
/// - Returns a provider object when a domain-match is found.
/// - Returns null when the email is valid but no provider matches.
/// - Throws a JS Error when email is empty or missing '@'.
#[napi(js_name = "providerForEmail")]
pub fn provider_for_email(email: String) -> Result<Option<MailProviderInfo>> {
    lookup_provider(&email)
}

// ---------------------------------------------------------------------------
// Conversion helpers
// ---------------------------------------------------------------------------

fn connection_type_for(entry: &RawServerEntry) -> String {
    if entry.ssl || entry.tls {
        "tls".to_string()
    } else if entry.starttls {
        "starttls".to_string()
    } else {
        "clear".to_string()
    }
}

fn server_to_net_service(entry: &RawServerEntry) -> NetServiceInfo {
    NetServiceInfo {
        hostname: entry.hostname.clone().unwrap_or_default(),
        port: entry.port.unwrap_or(0),
        connection_type: connection_type_for(entry),
    }
}

fn provider_to_info(provider: &Provider) -> MailProviderInfo {
    MailProviderInfo {
        identifier: provider.identifier.clone(),
        servers: ProviderServers {
            imap: provider
                .servers
                .imap
                .iter()
                .map(server_to_net_service)
                .collect(),
            smtp: provider
                .servers
                .smtp
                .iter()
                .map(server_to_net_service)
                .collect(),
            pop: provider
                .servers
                .pop
                .iter()
                .map(server_to_net_service)
                .collect(),
        },
        domain_match: provider
            .domain_match_compiled
            .iter()
            .map(|(p, _)| p.clone())
            .collect(),
        mx_match: provider.mx_match_patterns.clone(),
    }
}
