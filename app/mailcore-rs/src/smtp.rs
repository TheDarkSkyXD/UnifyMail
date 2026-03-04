//! SMTP connection testing.
//!
//! Implements `testSMTPConnection` as an async napi-rs export that handles:
//! - Three TLS connection paths: direct TLS (port 465), STARTTLS upgrade, clear/unencrypted
//! - Two authentication methods: password (LOGIN) and XOAUTH2 SASL
//! - A 15-second timeout wrapping the entire connect+auth+NOOP flow
//! - Categorized errors with an `errorType` field (never rejects the Promise)
//!
//! lettre handles the full SMTP handshake internally: greeting consumption, EHLO,
//! AUTH (multi-step LOGIN or XOAUTH2), and NOOP via `transport.test_connection()`.
//! This is simpler than async-imap which required manual greeting consumption.

use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::Error as SmtpError;
use lettre::{AsyncSmtpTransport, Tokio1Executor};
use napi_derive::napi;
use std::time::Duration;

// Internal error type alias for non-napi functions.
// napi functions use napi::Result which requires AsRef<str>, so we keep BoxError
// as the internal error type and only convert at the napi boundary.
type BoxError = Box<dyn std::error::Error + Send + Sync>;
type InternalResult<T> = std::result::Result<T, BoxError>;

// ---------------------------------------------------------------------------
// napi-exported types
// ---------------------------------------------------------------------------

/// Options for testSMTPConnection.
///
/// Field names use snake_case in Rust; napi-rs auto-converts to camelCase for
/// JavaScript (connection_type → connectionType, oauth2_token → oauth2Token).
#[napi(object)]
pub struct SMTPConnectionOptions {
    pub hostname: String,
    /// Port number as u32 (napi-rs maps u32 to JavaScript number safely).
    pub port: u32,
    /// Connection type: "tls" | "starttls" | "clear". Defaults to "tls" if absent.
    pub connection_type: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    /// OAuth2 bearer token for XOAUTH2 SASL authentication.
    pub oauth2_token: Option<String>,
}

/// Result of testSMTPConnection.
///
/// The Promise ALWAYS resolves — connection/auth failures use { success: false }.
#[napi(object)]
pub struct SMTPConnectionResult {
    pub success: bool,
    pub error: Option<String>,
    /// Categorized error type: "connection_refused" | "timeout" | "tls_error" |
    /// "auth_failed" | "unknown". Present only when success is false.
    pub error_type: Option<String>,
}

// ---------------------------------------------------------------------------
// Error classification
// ---------------------------------------------------------------------------

/// Classify a lettre SMTP error into one of the documented errorType strings.
///
/// Checks the error Display and source chain for known patterns. Returns one of:
/// "connection_refused", "tls_error", "auth_failed", "unknown".
/// (The "timeout" errorType is set separately at the top-level timeout handler.)
fn classify_smtp_error(err: &SmtpError) -> String {
    let msg = err.to_string().to_lowercase();
    let source_msg = std::error::Error::source(err)
        .map(|s| s.to_string().to_lowercase())
        .unwrap_or_default();
    let combined = format!("{msg} {source_msg}");

    if combined.contains("connection refused")
        || combined.contains("os error 111")
        || combined.contains("actively refused")
        || combined.contains("connection was forcibly")
        || combined.contains("os error 10061")
    {
        "connection_refused".to_string()
    } else if combined.contains("tls")
        || combined.contains("rustls")
        || combined.contains("certificate")
        || combined.contains("handshake")
        || combined.contains("ssl")
        || combined.contains("invalidcontenttype")
        || combined.contains("invalidmessage")
        || combined.contains("corrupt message")
    {
        "tls_error".to_string()
    } else if combined.contains("535")
        || combined.contains("authentication")
        || combined.contains("auth failed")
        || combined.contains("credentials invalid")
        || combined.contains("auth credentials")
        || combined.contains("invalid username")
        || combined.contains("invalid password")
        || combined.contains("bad credentials")
        || combined.contains("login failed")
        || combined.contains("username and password not accepted")
    {
        "auth_failed".to_string()
    } else {
        "unknown".to_string()
    }
}

// ---------------------------------------------------------------------------
// Core test function (no timeout — wrapped by the napi export)
// ---------------------------------------------------------------------------

/// Internal implementation of the SMTP connection test (no timeout wrapper).
///
/// Called by `test_smtp_connection` which wraps this in a 15-second timeout.
/// Exposed as `pub` for integration tests in tests/smtp_tests.rs.
///
/// This function always returns `Ok(SMTPConnectionResult)` — errors are
/// classified and returned as `{ success: false, errorType: "..." }`.
pub async fn do_test_smtp(opts: &SMTPConnectionOptions) -> InternalResult<SMTPConnectionResult> {
    let host = opts.hostname.as_str();
    let port = opts.port as u16;
    let conn_type = opts.connection_type.as_deref().unwrap_or("tls");
    let username = opts.username.as_deref();
    let password = opts.password.as_deref();
    let oauth2_token = opts.oauth2_token.as_deref();

    if std::env::var("MAILCORE_DEBUG").as_deref() == Ok("1") {
        eprintln!(
            "[mailcore-rs] testSMTPConnection -> Rust: {}:{} ({})",
            host, port, conn_type
        );
    }

    // Build the transport based on connection type.
    // lettre handles: greeting consumption, EHLO, AUTH, NOOP via test_connection().
    let smtp_result = match conn_type {
        "starttls" => build_and_test_starttls(host, port, username, password, oauth2_token).await,
        "clear" => build_and_test_clear(host, port, username, password, oauth2_token).await,
        _ => {
            // Default: "tls" — direct TLS (port 465 style)
            build_and_test_tls(host, port, username, password, oauth2_token).await
        }
    };

    match smtp_result {
        Ok(()) => Ok(SMTPConnectionResult {
            success: true,
            error: None,
            error_type: None,
        }),
        Err(e) => {
            let error_type = classify_smtp_error(&e);
            Ok(SMTPConnectionResult {
                success: false,
                error: Some(format!("SMTP connection to {host}:{port} failed: {e}")),
                error_type: Some(error_type),
            })
        }
    }
}

/// Build a direct TLS transport and test the connection.
async fn build_and_test_tls(
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
    oauth2_token: Option<&str>,
) -> Result<(), SmtpError> {
    let builder = AsyncSmtpTransport::<Tokio1Executor>::relay(host)?.port(port);
    let transport = apply_credentials_tls(builder, username, password, oauth2_token);
    transport.test_connection().await?;
    Ok(())
}

/// Build a STARTTLS transport and test the connection.
async fn build_and_test_starttls(
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
    oauth2_token: Option<&str>,
) -> Result<(), SmtpError> {
    let builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)?.port(port);
    let transport = apply_credentials_tls(builder, username, password, oauth2_token);
    transport.test_connection().await?;
    Ok(())
}

/// Build a clear (unencrypted) transport and test the connection.
async fn build_and_test_clear(
    host: &str,
    port: u16,
    username: Option<&str>,
    password: Option<&str>,
    oauth2_token: Option<&str>,
) -> Result<(), SmtpError> {
    let builder = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host).port(port);
    let transport = apply_credentials_clear(builder, username, password, oauth2_token);
    transport.test_connection().await?;
    Ok(())
}

/// Apply credentials to a TLS/STARTTLS SMTP transport builder, then build.
///
/// - Password auth: `Credentials::new(user, pass)` + `Mechanism::Login`
/// - XOAUTH2 auth: `Credentials::new(user, token)` + `Mechanism::Xoauth2`
///   lettre handles XOAUTH2 SASL encoding internally; no custom Authenticator needed.
/// - No credentials: connect-only (EHLO + NOOP, no AUTH)
fn apply_credentials_tls(
    builder: lettre::transport::smtp::AsyncSmtpTransportBuilder,
    username: Option<&str>,
    password: Option<&str>,
    oauth2_token: Option<&str>,
) -> AsyncSmtpTransport<Tokio1Executor> {
    if let Some(user) = username {
        if let Some(token) = oauth2_token {
            let creds = Credentials::new(user.to_string(), token.to_string());
            builder
                .credentials(creds)
                .authentication(vec![Mechanism::Xoauth2])
                .build()
        } else if let Some(pass) = password {
            let creds = Credentials::new(user.to_string(), pass.to_string());
            builder
                .credentials(creds)
                .authentication(vec![Mechanism::Login])
                .build()
        } else {
            builder.build()
        }
    } else {
        builder.build()
    }
}

/// Apply credentials to a clear SMTP transport builder, then build.
///
/// Separate function needed because `builder_dangerous()` returns a different
/// builder type than `relay()` / `starttls_relay()`.
fn apply_credentials_clear(
    builder: lettre::transport::smtp::AsyncSmtpTransportBuilder,
    username: Option<&str>,
    password: Option<&str>,
    oauth2_token: Option<&str>,
) -> AsyncSmtpTransport<Tokio1Executor> {
    apply_credentials_tls(builder, username, password, oauth2_token)
}

// ---------------------------------------------------------------------------
// napi-exported async function
// ---------------------------------------------------------------------------

/// Test an SMTP connection with the provided settings.
///
/// - Handles three connection modes: TLS (default), STARTTLS, and clear.
/// - Authenticates with either password (LOGIN) or OAuth2 (XOAUTH2 SASL).
/// - Wraps the entire operation in a 15-second timeout.
/// - Always resolves the Promise — never rejects for connection/auth failures.
///   Failures are returned as `{ success: false, error: "...", errorType: "..." }`.
///
/// CRITICAL: `js_name` must be "testSMTPConnection" (all-caps SMTP) to match
/// the expected export name. Without js_name, napi-rs auto-converts
/// `test_smtp_connection` to `testSmtpConnection` (lowercase 'mtp').
#[napi(js_name = "testSMTPConnection")]
pub async fn test_smtp_connection(
    opts: SMTPConnectionOptions,
) -> napi::Result<SMTPConnectionResult> {
    let host = opts.hostname.clone();
    let port = opts.port;

    match tokio::time::timeout(Duration::from_secs(15), do_test_smtp(&opts)).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => {
            // BoxError from transport build (programming error) — classify as unknown
            Ok(SMTPConnectionResult {
                success: false,
                error: Some(format!("SMTP connection to {host}:{port} failed: {e}")),
                error_type: Some("unknown".to_string()),
            })
        }
        Err(_elapsed) => Ok(SMTPConnectionResult {
            success: false,
            error: Some(format!(
                "SMTP connection to {host}:{port} timed out after 15 seconds"
            )),
            error_type: Some("timeout".to_string()),
        }),
    }
}
