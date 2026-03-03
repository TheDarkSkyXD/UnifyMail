//! Phase 2: IMAP connection testing.
//!
//! Implements `testIMAPConnection` as an async napi-rs export that handles:
//! - Three TLS connection paths: direct TLS (port 993), STARTTLS upgrade, clear/unencrypted
//! - Two authentication methods: password (LOGIN) and XOAUTH2 SASL
//! - Detection of 7 IMAP capabilities post-login
//! - A 15-second timeout wrapping the entire connect+auth+capability flow
//! - Categorized errors with an `errorType` field (never rejects the Promise)

use async_imap::{Authenticator, Client};
use napi_derive::napi;
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;
use std::fmt;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

// Internal error type alias for non-napi functions.
// napi functions use napi::Result which requires AsRef<str>, so we keep BoxError
// as the internal error type and only convert at the napi boundary.
type BoxError = Box<dyn std::error::Error + Send + Sync>;
type InternalResult<T> = std::result::Result<T, BoxError>;

// ---------------------------------------------------------------------------
// napi-exported types — must match (and extend) app/mailcore/types/index.d.ts
// ---------------------------------------------------------------------------

/// Options for testIMAPConnection.
///
/// Field names use snake_case in Rust; napi-rs auto-converts to camelCase for
/// JavaScript (connection_type → connectionType, oauth2_token → oauth2Token).
#[napi(object)]
pub struct IMAPConnectionOptions {
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

/// Result of testIMAPConnection.
///
/// Extends the C++ IMAPConnectionResult with an optional `errorType` field.
/// The Promise ALWAYS resolves — connection/auth failures use { success: false }.
#[napi(object)]
pub struct IMAPConnectionResult {
    pub success: bool,
    pub error: Option<String>,
    /// Categorized error type: "connection_refused" | "timeout" | "tls_error" |
    /// "auth_failed" | "unknown". Present only when success is false.
    pub error_type: Option<String>,
    /// Detected IMAP capabilities: subset of "idle", "condstore", "qresync",
    /// "compress", "namespace", "xoauth2", "gmail". Present only on success.
    pub capabilities: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// XOAUTH2 SASL Authenticator
// ---------------------------------------------------------------------------

/// Implements the XOAUTH2 SASL mechanism for async-imap.
///
/// Token format: `user=<email>\x01auth=Bearer <token>\x01\x01`
/// Note: \x01 (SOH, ASCII 1) separators are required — NOT \x00 (null).
struct XOAuth2 {
    user: String,
    access_token: String,
}

impl fmt::Debug for XOAuth2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("XOAuth2")
            .field("user", &self.user)
            .field("access_token", &"[redacted]")
            .finish()
    }
}

impl Authenticator for XOAuth2 {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
    }
}

// ---------------------------------------------------------------------------
// TLS configuration
// ---------------------------------------------------------------------------

/// Build a rustls ClientConfig using the OS platform certificate verifier.
///
/// Uses `rustls-platform-verifier` to validate against the OS trust store
/// (Windows Certificate Store, macOS Keychain, Linux ca-certificates).
/// This is required for enterprise environments with custom internal CAs.
fn make_tls_config() -> InternalResult<ClientConfig> {
    let config = ClientConfig::with_platform_verifier()?;
    Ok(config)
}

/// Build a rustls ServerName for the given host string.
///
/// Handles both IP addresses (ServerName::IpAddress) and DNS hostnames
/// (ServerName::DnsName). IP addresses would fail ServerName::try_from
/// which expects DNS names only.
fn make_server_name(host: &str) -> InternalResult<ServerName<'static>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        Ok(ServerName::IpAddress(ip.into()))
    } else {
        let name = ServerName::try_from(host.to_string())
            .map_err(|e| format!("Invalid hostname '{host}': {e}"))?;
        Ok(name)
    }
}

// ---------------------------------------------------------------------------
// Capability detection
// ---------------------------------------------------------------------------

/// Map IMAP server capabilities to the 7 lowercase names used by this API.
///
/// Capabilities are checked AFTER login, as some servers only advertise full
/// capability sets post-authentication (RFC 3501 Section 6.1.1).
fn extract_capabilities(caps: &async_imap::types::Capabilities) -> Vec<String> {
    let mut result = Vec::new();

    if caps.has_str("IDLE") {
        result.push("idle".to_string());
    }
    if caps.has_str("CONDSTORE") {
        result.push("condstore".to_string());
    }
    if caps.has_str("QRESYNC") {
        result.push("qresync".to_string());
    }
    if caps.has_str("COMPRESS=DEFLATE") {
        result.push("compress".to_string());
    }
    if caps.has_str("NAMESPACE") {
        result.push("namespace".to_string());
    }
    if caps.has_str("AUTH=XOAUTH2") {
        result.push("xoauth2".to_string());
    }
    if caps.has_str("X-GM-EXT-1") {
        result.push("gmail".to_string());
    }

    result
}

// ---------------------------------------------------------------------------
// Error classification
// ---------------------------------------------------------------------------

/// Classify an internal error into one of the documented errorType strings.
///
/// Checks the error message chain for known patterns. Returns one of:
/// "connection_refused", "tls_error", "auth_failed", "unknown".
/// (The "timeout" errorType is set separately at the top-level timeout handler.)
fn classify_error(err: &dyn std::error::Error) -> String {
    let msg = err.to_string().to_lowercase();
    let source_msg = err
        .source()
        .map(|s| s.to_string().to_lowercase())
        .unwrap_or_default();
    let combined = format!("{msg} {source_msg}");

    if combined.contains("connection refused") || combined.contains("os error 111") {
        "connection_refused".to_string()
    } else if combined.contains("tls")
        || combined.contains("rustls")
        || combined.contains("certificate")
        || combined.contains("handshake")
        || combined.contains("ssl")
    {
        "tls_error".to_string()
    } else if combined.contains("authentication")
        || combined.contains("login failed")
        || combined.contains("xoauth2 auth failed")
        || combined.contains("invalid credentials")
        || combined.contains("bad credentials")
        || combined.contains("[authenticationfailed]")
        || combined.contains("authenticate failed")
        || combined.contains("no password")
        || combined.contains("no oauth2")
        || combined.contains("no password or oauth2")
    {
        "auth_failed".to_string()
    } else {
        "unknown".to_string()
    }
}

// ---------------------------------------------------------------------------
// Connection builders
// ---------------------------------------------------------------------------

/// Connect via direct TLS (port 993 style): TCP → TLS handshake → async-imap Client.
async fn connect_tls(
    host: &str,
    port: u16,
) -> InternalResult<Client<tokio_rustls::client::TlsStream<TcpStream>>> {
    let tcp_stream = TcpStream::connect((host, port)).await?;

    let tls_config = make_tls_config()?;
    let connector = TlsConnector::from(Arc::new(tls_config));
    let server_name = make_server_name(host)?;
    let tls_stream = connector.connect(server_name, tcp_stream).await?;

    let client = Client::new(tls_stream);
    Ok(client)
}

/// Connect via STARTTLS: TCP → Client (reads greeting) → STARTTLS command →
/// extract stream → TLS upgrade → new Client (no greeting after STARTTLS).
///
/// Pattern from deltachat-core-rust src/imap/client.rs.
/// After STARTTLS upgrade, the server does NOT send a new greeting.
/// The 15-second outer timeout ensures we don't hang if behavior deviates.
async fn connect_starttls(
    host: &str,
    port: u16,
) -> InternalResult<Client<tokio_rustls::client::TlsStream<TcpStream>>> {
    // Step 1: Plain TCP connect
    let tcp_stream = TcpStream::connect((host, port)).await?;

    // Step 2: Wrap in async-imap Client (reads server greeting from plain TCP)
    let mut client = Client::new(tcp_stream);

    // Step 3: Send STARTTLS command and verify OK response
    client
        .run_command_and_check_ok("STARTTLS", None)
        .await
        .map_err(|e| -> BoxError { format!("STARTTLS command failed: {e}").into() })?;

    // Step 4: Extract the raw TCP stream from the client
    let tcp_stream = client.into_inner();

    // Step 5: Perform TLS handshake on the plain TCP stream
    let tls_config = make_tls_config()?;
    let connector = TlsConnector::from(Arc::new(tls_config));
    let server_name = make_server_name(host)?;
    let tls_stream = connector.connect(server_name, tcp_stream).await?;

    // Step 6: Wrap TLS stream in a new async-imap Client.
    // No server greeting is sent after STARTTLS (RFC compliance).
    // async_imap::Client::new wraps the stream in a Connection — no automatic
    // greeting read at this point. Safe to proceed directly to authentication.
    let client = Client::new(tls_stream);

    Ok(client)
}

/// Connect via clear/unencrypted TCP (no TLS).
async fn connect_clear(host: &str, port: u16) -> InternalResult<Client<TcpStream>> {
    let tcp_stream = TcpStream::connect((host, port)).await?;
    let client = Client::new(tcp_stream);
    Ok(client)
}

// ---------------------------------------------------------------------------
// Authentication and capability retrieval (generic over stream type)
// ---------------------------------------------------------------------------

/// Authenticate and retrieve capabilities.
///
/// Generic over the stream type `S` so it works with both TLS and plain TCP
/// connections (avoids duplicating auth logic for each connection type).
///
/// Performs login (password) or authenticate (XOAUTH2), then fetches the
/// post-login capability list, then logs out cleanly.
async fn auth_and_capabilities<S>(
    client: Client<S>,
    username: &str,
    password: Option<&str>,
    oauth2_token: Option<&str>,
) -> InternalResult<IMAPConnectionResult>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + fmt::Debug,
{
    let mut session = if let Some(token) = oauth2_token {
        let auth = XOAuth2 {
            user: username.to_string(),
            access_token: token.to_string(),
        };
        // authenticate() returns Result<Session, (Error, Client)> on failure —
        // we discard the returned client and convert to a plain error.
        client
            .authenticate("XOAUTH2", auth)
            .await
            .map_err(|(err, _client)| -> BoxError {
                format!("xoauth2 auth failed: {err}").into()
            })?
    } else if let Some(pass) = password {
        // login() also returns Result<Session, (Error, Client)> on failure.
        client
            .login(username, pass)
            .await
            .map_err(|(err, _client)| -> BoxError { format!("login failed: {err}").into() })?
    } else {
        return Err("No password or oauth2 token provided".into());
    };

    // Fetch capabilities AFTER login — servers may advertise more capabilities
    // post-authentication than pre-authentication.
    let caps = session.capabilities().await?;
    let capabilities = extract_capabilities(&caps);

    // Disconnect cleanly — ignore errors (server may have already closed)
    let _ = session.logout().await;

    Ok(IMAPConnectionResult {
        success: true,
        error: None,
        error_type: None,
        capabilities: Some(capabilities),
    })
}

// ---------------------------------------------------------------------------
// Core test function (no timeout — wrapped by the napi export)
// ---------------------------------------------------------------------------

/// Internal implementation of the IMAP connection test (no timeout wrapper).
///
/// Called by `test_imap_connection` which wraps this in a 15-second timeout.
async fn do_test_imap(opts: &IMAPConnectionOptions) -> InternalResult<IMAPConnectionResult> {
    let host = opts.hostname.as_str();
    // Port is stored as u32 for napi compatibility; safe to cast to u16 for connect.
    let port = opts.port as u16;
    let conn_type = opts.connection_type.as_deref().unwrap_or("tls");
    let username = opts.username.as_deref().unwrap_or("");
    let password = opts.password.as_deref();
    let oauth2_token = opts.oauth2_token.as_deref();

    if std::env::var("MAILCORE_DEBUG").as_deref() == Ok("1") {
        eprintln!(
            "[mailcore-rs] testIMAPConnection -> Rust: {}:{} ({})",
            host, port, conn_type
        );
    }

    match conn_type {
        "starttls" => {
            let client = connect_starttls(host, port).await?;
            auth_and_capabilities(client, username, password, oauth2_token).await
        }
        "clear" => {
            let client = connect_clear(host, port).await?;
            auth_and_capabilities(client, username, password, oauth2_token).await
        }
        _ => {
            // Default: "tls"
            let client = connect_tls(host, port).await?;
            auth_and_capabilities(client, username, password, oauth2_token).await
        }
    }
}

// ---------------------------------------------------------------------------
// napi-exported async function
// ---------------------------------------------------------------------------

/// Test an IMAP connection with the provided settings.
///
/// - Handles three connection modes: TLS (default), STARTTLS, and clear.
/// - Authenticates with either password (LOGIN) or OAuth2 (XOAUTH2 SASL).
/// - Detects 7 IMAP server capabilities post-login.
/// - Wraps the entire operation in a 15-second timeout.
/// - Always resolves the Promise — never rejects for connection/auth failures.
///   Failures are returned as `{ success: false, error: "...", errorType: "..." }`.
///
/// CRITICAL: `js_name` must be "testIMAPConnection" (all-caps IMAP) to match
/// the C++ export name that TypeScript callers use. Without js_name, napi-rs
/// auto-converts `test_imap_connection` to `testImapConnection` (lowercase).
#[napi(js_name = "testIMAPConnection")]
pub async fn test_imap_connection(
    opts: IMAPConnectionOptions,
) -> napi::Result<IMAPConnectionResult> {
    let host = opts.hostname.clone();
    let port = opts.port;

    match tokio::time::timeout(Duration::from_secs(15), do_test_imap(&opts)).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(e)) => {
            let error_type = classify_error(e.as_ref());
            Ok(IMAPConnectionResult {
                success: false,
                error: Some(format!("Connection to {host}:{port} failed: {e}")),
                error_type: Some(error_type),
                capabilities: None,
            })
        }
        Err(_elapsed) => Ok(IMAPConnectionResult {
            success: false,
            error: Some(format!(
                "Connection to {host}:{port} timed out after 15 seconds"
            )),
            error_type: Some("timeout".to_string()),
            capabilities: None,
        }),
    }
}
