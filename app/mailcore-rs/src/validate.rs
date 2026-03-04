//! Phase 3 Plan 02: Account validation with concurrent IMAP + SMTP + MX testing.
//!
//! Implements `validateAccount` as an async napi-rs export that:
//! - Runs IMAP test, SMTP test, and MX DNS resolution concurrently via tokio::join!()
//! - Wraps the entire operation in a 15-second timeout
//! - Returns a rich result shape with sub-results for each protocol
//! - Resolves an email domain's MX records to identify the provider
//! - Never fails due to MX resolution errors (fail-silent)
//!
//! The total validation time equals max(IMAP, SMTP, MX) — not their sum.

use hickory_resolver::Resolver;
use napi_derive::napi;
use std::time::Duration;

use crate::imap::{do_test_imap, IMAPConnectionOptions, IMAPConnectionResult};
use crate::smtp::{do_test_smtp, SMTPConnectionOptions, SMTPConnectionResult};

// Internal error type alias for non-napi functions.
type BoxError = Box<dyn std::error::Error + Send + Sync>;
type InternalResult<T> = std::result::Result<T, BoxError>;

// ---------------------------------------------------------------------------
// napi-exported result types
// ---------------------------------------------------------------------------

/// IMAP sub-result within AccountValidationResult.
///
/// Mirrors IMAPConnectionResult but as a separate type for the validate API.
#[napi(object)]
pub struct IMAPSubResult {
    pub success: bool,
    pub error: Option<String>,
    pub error_type: Option<String>,
    pub capabilities: Option<Vec<String>>,
}

/// SMTP sub-result within AccountValidationResult.
#[napi(object)]
pub struct SMTPSubResult {
    pub success: bool,
    pub error: Option<String>,
    pub error_type: Option<String>,
}

/// Server connection info returned in AccountValidationResult.
#[napi(object)]
pub struct ServerInfo {
    pub hostname: String,
    pub port: u32,
}

/// Full result of validateAccount.
///
/// The Promise ALWAYS resolves — failures are encoded in success=false.
#[napi(object)]
pub struct AccountValidationResult {
    /// true only when both IMAP and SMTP succeed
    pub success: bool,
    /// Top-level error: prefixed with "IMAP: " or "SMTP: " depending on which failed.
    /// When both fail, IMAP takes priority.
    pub error: Option<String>,
    /// Top-level errorType: from the failing protocol (IMAP takes priority).
    pub error_type: Option<String>,
    /// Provider identifier from MX record matching, or None if no match.
    pub identifier: Option<String>,
    /// IMAP sub-result (always present, never None).
    pub imap_result: IMAPSubResult,
    /// SMTP sub-result (always present, never None).
    pub smtp_result: SMTPSubResult,
    /// IMAP server info (hostname + port from opts).
    pub imap_server: ServerInfo,
    /// SMTP server info (hostname + port from opts).
    pub smtp_server: ServerInfo,
}

// ---------------------------------------------------------------------------
// Input options
// ---------------------------------------------------------------------------

/// Options for validateAccount.
///
/// Contains both IMAP and SMTP settings plus per-protocol auth credentials.
#[napi(object)]
pub struct ValidateAccountOptions {
    pub email: String,
    // IMAP settings
    pub imap_hostname: String,
    pub imap_port: u32,
    pub imap_connection_type: Option<String>,
    // Per-protocol IMAP auth
    pub imap_username: Option<String>,
    pub imap_password: Option<String>,
    // SMTP settings
    pub smtp_hostname: String,
    pub smtp_port: u32,
    pub smtp_connection_type: Option<String>,
    // Per-protocol SMTP auth
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    // Account-wide OAuth2 token (shared per locked decision)
    pub oauth2_token: Option<String>,
}

// ---------------------------------------------------------------------------
// MX identifier resolution (fail-silent)
// ---------------------------------------------------------------------------

/// Resolve MX records for a domain and match against provider patterns.
///
/// Returns `Some(identifier)` if an MX hostname matches a provider's mx_match_patterns.
/// Returns `None` if: DNS lookup fails, no MX records, no provider matches, or timeout.
///
/// This function NEVER returns an error — all failures become `None`.
/// Per the locked CONTEXT.md decision: MX resolution failure is silent.
async fn resolve_mx_identifier(domain: &str) -> Option<String> {
    // Use hickory_resolver with a 5-second sub-timeout (within the 15s outer timeout).
    let resolver = match Resolver::builder_tokio() {
        Ok(builder) => builder.build(),
        Err(_) => return None,
    };

    // Wrap the MX lookup in a 5-second timeout.
    let mx_result = tokio::time::timeout(Duration::from_secs(5), resolver.mx_lookup(domain)).await;

    let mx_lookup = match mx_result {
        Ok(Ok(lookup)) => lookup,
        _ => return None,
    };

    // Collect all MX hostnames (strip trailing dots, lowercase).
    let mx_hosts: Vec<String> = mx_lookup
        .iter()
        .map(|mx| mx.exchange().to_utf8().trim_end_matches('.').to_lowercase())
        .collect();

    if mx_hosts.is_empty() {
        return None;
    }

    // Read the PROVIDERS global and match MX hostnames against provider patterns.
    let lock = match crate::provider::PROVIDERS.read() {
        Ok(l) => l,
        Err(_) => return None,
    };

    let providers = match lock.as_ref() {
        Some(p) => p,
        None => return None,
    };

    for provider in providers {
        for pattern in &provider.mx_match_patterns {
            let anchored = format!("^{pattern}$");
            if let Ok(re) = regex::RegexBuilder::new(&anchored)
                .case_insensitive(true)
                .build()
            {
                for mx_host in &mx_hosts {
                    if re.is_match(mx_host) {
                        return Some(provider.identifier.clone());
                    }
                }
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Builder helpers
// ---------------------------------------------------------------------------

/// Build IMAPConnectionOptions from ValidateAccountOptions.
fn build_imap_opts(opts: &ValidateAccountOptions) -> IMAPConnectionOptions {
    IMAPConnectionOptions {
        hostname: opts.imap_hostname.clone(),
        port: opts.imap_port,
        connection_type: opts.imap_connection_type.clone(),
        username: opts.imap_username.clone(),
        password: opts.imap_password.clone(),
        oauth2_token: opts.oauth2_token.clone(),
    }
}

/// Build SMTPConnectionOptions from ValidateAccountOptions.
fn build_smtp_opts(opts: &ValidateAccountOptions) -> SMTPConnectionOptions {
    SMTPConnectionOptions {
        hostname: opts.smtp_hostname.clone(),
        port: opts.smtp_port,
        connection_type: opts.smtp_connection_type.clone(),
        username: opts.smtp_username.clone(),
        password: opts.smtp_password.clone(),
        oauth2_token: opts.oauth2_token.clone(),
    }
}

// ---------------------------------------------------------------------------
// Result assembly
// ---------------------------------------------------------------------------

/// Assemble the final AccountValidationResult from concurrent sub-results.
///
/// Priority rules (per CONTEXT.md locked decisions):
/// - success = imap_success AND smtp_success
/// - When both fail, IMAP error takes priority at top level
/// - Top-level error/errorType carry the "IMAP: " or "SMTP: " prefix
/// - Sub-results do NOT include the prefix (sub-object already indicates protocol)
fn assemble_result(
    imap_res: InternalResult<IMAPConnectionResult>,
    smtp_res: InternalResult<SMTPConnectionResult>,
    identifier: Option<String>,
    opts: &ValidateAccountOptions,
) -> AccountValidationResult {
    // Convert InternalResult into sub-result objects.
    let (imap_sub, imap_success) = match imap_res {
        Ok(r) => {
            let s = r.success;
            let sub = IMAPSubResult {
                success: r.success,
                error: r.error,
                error_type: r.error_type,
                capabilities: r.capabilities,
            };
            (sub, s)
        }
        Err(e) => {
            let msg = e.to_string();
            let sub = IMAPSubResult {
                success: false,
                error: Some(msg),
                error_type: Some("unknown".to_string()),
                capabilities: None,
            };
            (sub, false)
        }
    };

    let (smtp_sub, smtp_success) = match smtp_res {
        Ok(r) => {
            let s = r.success;
            let sub = SMTPSubResult {
                success: r.success,
                error: r.error,
                error_type: r.error_type,
            };
            (sub, s)
        }
        Err(e) => {
            let msg = e.to_string();
            let sub = SMTPSubResult {
                success: false,
                error: Some(msg),
                error_type: Some("unknown".to_string()),
            };
            (sub, false)
        }
    };

    let overall_success = imap_success && smtp_success;

    // Top-level error/errorType: IMAP takes priority when both fail.
    let (top_error, top_error_type) = if overall_success {
        (None, None)
    } else if !imap_success {
        // IMAP failed (takes priority even if SMTP also failed)
        let err = imap_sub
            .error
            .as_deref()
            .map(|e| format!("IMAP: {e}"))
            .or_else(|| Some("IMAP: connection failed".to_string()));
        let err_type = imap_sub.error_type.clone();
        (err, err_type)
    } else {
        // Only SMTP failed
        let err = smtp_sub
            .error
            .as_deref()
            .map(|e| format!("SMTP: {e}"))
            .or_else(|| Some("SMTP: connection failed".to_string()));
        let err_type = smtp_sub.error_type.clone();
        (err, err_type)
    };

    AccountValidationResult {
        success: overall_success,
        error: top_error,
        error_type: top_error_type,
        identifier,
        imap_result: imap_sub,
        smtp_result: smtp_sub,
        imap_server: ServerInfo {
            hostname: opts.imap_hostname.clone(),
            port: opts.imap_port,
        },
        smtp_server: ServerInfo {
            hostname: opts.smtp_hostname.clone(),
            port: opts.smtp_port,
        },
    }
}

// ---------------------------------------------------------------------------
// Internal implementation (callable from tests without napi runtime)
// ---------------------------------------------------------------------------

/// Internal implementation of validateAccount — no timeout wrapper, no napi.
///
/// Exposed as `pub` for integration tests in tests/smtp_tests.rs.
/// Tests call this directly to avoid needing a napi runtime.
pub async fn do_validate(opts: ValidateAccountOptions) -> AccountValidationResult {
    let domain = opts.email.split('@').next_back().unwrap_or("").to_string();

    let imap_opts = build_imap_opts(&opts);
    let smtp_opts = build_smtp_opts(&opts);

    let (imap_res, smtp_res, identifier) = tokio::join!(
        do_test_imap(&imap_opts),
        do_test_smtp(&smtp_opts),
        resolve_mx_identifier(&domain),
    );

    assemble_result(imap_res, smtp_res, identifier, &opts)
}

// ---------------------------------------------------------------------------
// napi-exported async function
// ---------------------------------------------------------------------------

/// Validate an email account by testing IMAP and SMTP connections concurrently.
///
/// - Runs IMAP test, SMTP test, and MX DNS lookup in parallel via tokio::join!()
/// - Total time equals max(IMAP, SMTP, MX) — not their sum
/// - Wraps the entire operation in a 15-second timeout
/// - Always resolves the Promise — never rejects
///
/// Result shape:
/// - success: true only when both IMAP and SMTP pass
/// - error/errorType: from the failing protocol (IMAP takes priority when both fail)
/// - identifier: provider identifier from MX match, or null
/// - imapResult/smtpResult: sub-results with their own success/error/errorType
/// - imapServer/smtpServer: the hostname+port used for testing
#[napi(js_name = "validateAccount")]
pub async fn validate_account(
    opts: ValidateAccountOptions,
) -> napi::Result<AccountValidationResult> {
    let imap_host = opts.imap_hostname.clone();
    let smtp_host = opts.smtp_hostname.clone();
    let imap_port = opts.imap_port;
    let smtp_port = opts.smtp_port;
    let imap_conn_type = opts.imap_connection_type.clone().unwrap_or_default();
    let smtp_conn_type = opts.smtp_connection_type.clone().unwrap_or_default();

    match tokio::time::timeout(Duration::from_secs(15), do_validate(opts)).await {
        Ok(result) => Ok(result),
        Err(_elapsed) => {
            // Timeout: return a synthetic failure result.
            // Sub-results represent unknown state (we don't know which timed out).
            Ok(AccountValidationResult {
                success: false,
                error: Some("Account validation timed out after 15 seconds".to_string()),
                error_type: Some("timeout".to_string()),
                identifier: None,
                imap_result: IMAPSubResult {
                    success: false,
                    error: Some(format!(
                        "Connection to {imap_host}:{imap_port} ({imap_conn_type}) timed out"
                    )),
                    error_type: Some("timeout".to_string()),
                    capabilities: None,
                },
                smtp_result: SMTPSubResult {
                    success: false,
                    error: Some(format!(
                        "Connection to {smtp_host}:{smtp_port} ({smtp_conn_type}) timed out"
                    )),
                    error_type: Some("timeout".to_string()),
                },
                imap_server: ServerInfo {
                    hostname: imap_host,
                    port: imap_port,
                },
                smtp_server: ServerInfo {
                    hostname: smtp_host,
                    port: smtp_port,
                },
            })
        }
    }
}
