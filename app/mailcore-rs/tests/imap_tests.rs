//! Integration tests for the IMAP connection testing module.
//!
//! Uses mock IMAP servers (TcpListener on random ports) so no live server is
//! required. Each test creates its own listener with no shared state — tests
//! run in parallel without a mutex.
//!
//! # Error handling
//! `do_test_imap` returns `Err(BoxError)` for connection/auth failures.
//! The napi export converts these to `Ok(IMAPConnectionResult { success: false, ... })`.
//! Tests verify error classification by inspecting the error message string, which
//! mirrors the `classify_error` logic in `imap.rs`.
//!
//! # TLS note
//! Direct TLS (port 993-style) tests against a plain TCP mock will fail because
//! rustls-platform-verifier rejects untrusted/self-signed certs. We test TLS error
//! classification by checking the error message. Phase 3 validates against real servers.

use base64::{engine::general_purpose, Engine as _};
use mailcore_napi_rs::imap::{do_test_imap, IMAPConnectionOptions};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::time::timeout;

// ---------------------------------------------------------------------------
// Mock server infrastructure
// ---------------------------------------------------------------------------

/// Controls how the mock server handles authentication.
#[derive(Clone, Debug)]
enum MockAuthMode {
    /// Accept any LOGIN command.
    AcceptPassword,
    /// Reject LOGIN with NO [AUTHENTICATIONFAILED].
    RejectPassword,
    /// Accept AUTHENTICATE XOAUTH2 if the SASL token has valid format.
    AcceptXOAuth2,
    /// Drop the connection immediately after sending the greeting.
    DropAfterGreeting,
    /// Hang forever without sending a greeting.
    HangForever,
    /// Send a garbage (non-IMAP) greeting then close.
    GarbageGreeting,
}

/// Start a mock IMAP server with configurable capabilities and auth mode.
///
/// Returns `(port, JoinHandle)`. The handle keeps the server alive; dropping it
/// cancels the server task.
async fn start_mock_imap_server(
    capabilities: &str,
    auth_mode: MockAuthMode,
) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let caps = capabilities.to_string();
    let handle = tokio::spawn(async move {
        if let Ok((stream, _addr)) = listener.accept().await {
            handle_mock_connection(stream, &caps, auth_mode).await;
        }
    });
    (port, handle)
}

/// Handle one mock IMAP connection.
async fn handle_mock_connection(
    stream: tokio::net::TcpStream,
    capabilities: &str,
    auth_mode: MockAuthMode,
) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    match auth_mode {
        MockAuthMode::HangForever => {
            // Never send greeting — connection hangs until timeout fires.
            tokio::time::sleep(Duration::from_secs(120)).await;
            return;
        }
        MockAuthMode::GarbageGreeting => {
            let _ = writer.write_all(b"GARBAGE NOT AN IMAP SERVER\r\n").await;
            // Drop immediately after garbage.
            return;
        }
        MockAuthMode::DropAfterGreeting => {
            let _ = writer
                .write_all(b"* OK IMAP4rev1 Server ready\r\n")
                .await;
            // Flush and drop — simulates mid-connection failure.
            return;
        }
        _ => {}
    }

    // Send IMAP greeting.
    let _ = writer
        .write_all(b"* OK IMAP4rev1 Server ready\r\n")
        .await;

    // Read and respond to commands in a loop.
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        // Clone to owned so `line` is free to be reused.
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        // Parse: <tag> <COMMAND> [rest]
        let mut parts = trimmed.splitn(3, ' ');
        let tag = parts.next().unwrap_or("*").to_string();
        let cmd = parts.next().unwrap_or("").to_uppercase();
        let rest = parts.next().unwrap_or("").to_string();

        match cmd.as_str() {
            "CAPABILITY" => {
                let response = format!(
                    "* CAPABILITY IMAP4rev1 {}\r\n{} OK CAPABILITY completed\r\n",
                    capabilities, tag
                );
                let _ = writer.write_all(response.as_bytes()).await;
            }
            "LOGIN" => match &auth_mode {
                MockAuthMode::AcceptPassword => {
                    let ok = format!("{} OK LOGIN completed\r\n", tag);
                    let _ = writer.write_all(ok.as_bytes()).await;
                }
                MockAuthMode::RejectPassword => {
                    let no = format!(
                        "{} NO [AUTHENTICATIONFAILED] Invalid credentials\r\n",
                        tag
                    );
                    let _ = writer.write_all(no.as_bytes()).await;
                }
                _ => {
                    let no = format!("{} NO LOGIN not accepted in this mode\r\n", tag);
                    let _ = writer.write_all(no.as_bytes()).await;
                }
            },
            "AUTHENTICATE" => {
                let mechanism = rest.trim().to_uppercase();
                if mechanism == "XOAUTH2" {
                    // RFC 5802: server sends challenge as `+ <base64>` or `+ ` for empty.
                    // XOAUTH2 uses an empty initial challenge.
                    let _ = writer.write_all(b"+ \r\n").await;

                    // Read client's base64-encoded SASL response.
                    let mut sasl_line = String::new();
                    match reader.read_line(&mut sasl_line).await {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {}
                    }
                    let sasl_b64 = sasl_line.trim().to_string();

                    match &auth_mode {
                        MockAuthMode::AcceptXOAuth2 => {
                            let decoded = general_purpose::STANDARD
                                .decode(sasl_b64.as_str())
                                .unwrap_or_default();
                            let decoded_str = String::from_utf8_lossy(&decoded);
                            // Validate: user=<email>\x01auth=Bearer <token>\x01\x01
                            let valid = decoded_str.starts_with("user=")
                                && decoded_str.contains("\x01auth=Bearer ")
                                && decoded_str.ends_with("\x01\x01");
                            if valid {
                                let ok = format!("{} OK AUTHENTICATE completed\r\n", tag);
                                let _ = writer.write_all(ok.as_bytes()).await;
                            } else {
                                let no = format!(
                                    "{} NO [AUTHENTICATIONFAILED] Invalid XOAUTH2 token format\r\n",
                                    tag
                                );
                                let _ = writer.write_all(no.as_bytes()).await;
                            }
                        }
                        _ => {
                            let no = format!(
                                "{} NO [AUTHENTICATIONFAILED] XOAUTH2 rejected\r\n",
                                tag
                            );
                            let _ = writer.write_all(no.as_bytes()).await;
                        }
                    }
                } else {
                    let no = format!("{} NO unsupported mechanism\r\n", tag);
                    let _ = writer.write_all(no.as_bytes()).await;
                }
            }
            "LOGOUT" => {
                let bye =
                    format!("* BYE Server logging out\r\n{} OK LOGOUT completed\r\n", tag);
                let _ = writer.write_all(bye.as_bytes()).await;
                break;
            }
            _ => {
                let bad = format!("{} BAD unknown command\r\n", tag);
                let _ = writer.write_all(bad.as_bytes()).await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// IMAPConnectionOptions for a clear (no-TLS) connection to 127.0.0.1 with password.
fn clear_opts(port: u16) -> IMAPConnectionOptions {
    IMAPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("clear".to_string()),
        username: Some("user@example.com".to_string()),
        password: Some("password123".to_string()),
        oauth2_token: None,
    }
}

/// IMAPConnectionOptions for a direct TLS connection to 127.0.0.1 with password.
fn tls_opts(port: u16) -> IMAPConnectionOptions {
    IMAPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("tls".to_string()),
        username: Some("user@example.com".to_string()),
        password: Some("password123".to_string()),
        oauth2_token: None,
    }
}

/// Classify an error string using the same rules as `imap.rs::classify_error`.
fn classify_error_str(msg: &str) -> &'static str {
    let lower = msg.to_lowercase();
    if lower.contains("connection refused") || lower.contains("os error 111") || lower.contains("actively refused") {
        "connection_refused"
    } else if lower.contains("tls")
        || lower.contains("rustls")
        || lower.contains("certificate")
        || lower.contains("handshake")
        || lower.contains("ssl")
        || lower.contains("invalidcontenttype")
        || lower.contains("invalidmessage")
    {
        "tls_error"
    } else if lower.contains("authentication")
        || lower.contains("login failed")
        || lower.contains("xoauth2 auth failed")
        || lower.contains("invalid credentials")
        || lower.contains("bad credentials")
        || lower.contains("[authenticationfailed]")
        || lower.contains("authenticate failed")
        || lower.contains("no password")
        || lower.contains("no oauth2")
    {
        "auth_failed"
    } else {
        "unknown"
    }
}

// ---------------------------------------------------------------------------
// Tests: clear connection with password auth
// ---------------------------------------------------------------------------

/// Clear connection with password login returns success and detected capabilities.
#[tokio::test]
async fn test_clear_connection_with_password() {
    let (port, _handle) =
        start_mock_imap_server("IDLE CONDSTORE", MockAuthMode::AcceptPassword).await;
    let opts = clear_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("clear connection must succeed");
    assert!(result.success, "Expected success, got: {:?}", result.error);
    let caps = result
        .capabilities
        .expect("capabilities must be present on success");
    assert!(
        caps.contains(&"idle".to_string()),
        "IDLE capability must be detected"
    );
    assert!(
        caps.contains(&"condstore".to_string()),
        "CONDSTORE capability must be detected"
    );
}

// ---------------------------------------------------------------------------
// Tests: all 7 capabilities detected
// ---------------------------------------------------------------------------

/// Mock server advertising all 7 capabilities — all 7 lowercase names returned.
#[tokio::test]
async fn test_capability_detection_all_seven() {
    let all_caps = "IDLE CONDSTORE QRESYNC COMPRESS=DEFLATE NAMESPACE AUTH=XOAUTH2 X-GM-EXT-1";
    let (port, _handle) =
        start_mock_imap_server(all_caps, MockAuthMode::AcceptPassword).await;
    let opts = clear_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("connection must succeed");
    assert!(result.success, "Expected success, got: {:?}", result.error);
    let caps = result.capabilities.expect("capabilities must be present");
    assert!(caps.contains(&"idle".to_string()), "idle missing");
    assert!(caps.contains(&"condstore".to_string()), "condstore missing");
    assert!(caps.contains(&"qresync".to_string()), "qresync missing");
    assert!(caps.contains(&"compress".to_string()), "compress missing");
    assert!(caps.contains(&"namespace".to_string()), "namespace missing");
    assert!(caps.contains(&"xoauth2".to_string()), "xoauth2 missing");
    assert!(caps.contains(&"gmail".to_string()), "gmail missing");
    assert_eq!(
        caps.len(),
        7,
        "Expected exactly 7 capabilities, got {}: {:?}",
        caps.len(),
        caps
    );
}

/// Mock server advertising only IDLE and NAMESPACE — only those two returned.
#[tokio::test]
async fn test_capability_detection_partial() {
    let (port, _handle) =
        start_mock_imap_server("IDLE NAMESPACE", MockAuthMode::AcceptPassword).await;
    let opts = clear_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("connection must succeed");
    assert!(result.success, "Expected success");
    let caps = result.capabilities.expect("capabilities must be present");
    assert!(
        caps.contains(&"idle".to_string()),
        "idle must be detected"
    );
    assert!(
        caps.contains(&"namespace".to_string()),
        "namespace must be detected"
    );
    assert!(
        !caps.contains(&"condstore".to_string()),
        "condstore must NOT be returned"
    );
    assert!(
        !caps.contains(&"gmail".to_string()),
        "gmail must NOT be returned"
    );
    assert_eq!(
        caps.len(),
        2,
        "Expected exactly 2 capabilities, got {}: {:?}",
        caps.len(),
        caps
    );
}

// ---------------------------------------------------------------------------
// Tests: XOAUTH2 authentication
// ---------------------------------------------------------------------------

/// XOAUTH2 authentication with a valid token returns success.
#[tokio::test]
async fn test_xoauth2_authentication() {
    let (port, _handle) =
        start_mock_imap_server("AUTH=XOAUTH2 IDLE", MockAuthMode::AcceptXOAuth2).await;
    let opts = IMAPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("clear".to_string()),
        username: Some("user@example.com".to_string()),
        password: None,
        oauth2_token: Some("ya29.valid_token_here".to_string()),
    };
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("XOAUTH2 auth must succeed at do_test_imap level");
    assert!(
        result.success,
        "XOAUTH2 auth must return success=true, got: {:?}",
        result.error
    );
}

/// The Rust client sends a valid SASL token format; the mock server validates it.
///
/// Mock server validates: `user=<email>\x01auth=Bearer <token>\x01\x01`
/// Server accepts only if the format is correct. Test success proves correct format.
#[tokio::test]
async fn test_xoauth2_sasl_format_validation() {
    let (port, _handle) =
        start_mock_imap_server("AUTH=XOAUTH2", MockAuthMode::AcceptXOAuth2).await;
    let opts = IMAPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("clear".to_string()),
        username: Some("test@domain.com".to_string()),
        password: None,
        oauth2_token: Some("access_token_value".to_string()),
    };
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("XOAUTH2 SASL format test must succeed at do_test_imap level");
    assert!(
        result.success,
        "XOAUTH2 SASL format must be valid (RFC-compliant). Server rejected it: {:?}",
        result.error
    );
}

// ---------------------------------------------------------------------------
// Tests: error scenarios
// ---------------------------------------------------------------------------

/// Auth failure: mock server rejects LOGIN — do_test_imap returns Err with "auth_failed" classification.
#[tokio::test]
async fn test_auth_failure_returns_error() {
    let (port, _handle) =
        start_mock_imap_server("", MockAuthMode::RejectPassword).await;
    let opts = clear_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s");
    match result {
        Ok(r) => {
            // Acceptable: some implementations return Ok(success=false) for auth failures.
            assert!(
                !r.success,
                "Auth failure must return success=false at the result level"
            );
        }
        Err(e) => {
            let err_str = e.to_string();
            let classified = classify_error_str(&err_str);
            assert_eq!(
                classified, "auth_failed",
                "Auth failure error must classify as 'auth_failed'. Error: {err_str}"
            );
        }
    }
}

/// Connection refused: connecting to a closed port — do_test_imap returns Err classified as "connection_refused".
#[tokio::test]
async fn test_connection_refused_returns_error() {
    // Bind to get a free port, then drop so nothing is listening.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let opts = clear_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s");
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("Connection refused must return Err from do_test_imap"),
    };
    let err_str = err.to_string();
    let classified = classify_error_str(&err_str);
    assert_eq!(
        classified, "connection_refused",
        "Connection refused must classify as 'connection_refused'. Error: {err_str}"
    );
}

/// Hanging server: outer timeout fires — do_test_imap does not complete within timeout.
///
/// Using a 3-second timeout to keep the test fast. Production uses 15 seconds.
/// This validates that the connection attempt correctly blocks until the timeout fires.
#[tokio::test]
async fn test_timeout_returns_error() {
    let (port, _handle) =
        start_mock_imap_server("", MockAuthMode::HangForever).await;
    let opts = clear_opts(port);

    // 3-second timeout simulates the 15-second napi timeout for test speed.
    let result = timeout(Duration::from_secs(3), do_test_imap(&opts)).await;
    assert!(
        result.is_err(),
        "Hanging server must cause the outer timeout to fire (Elapsed error)"
    );
    // The Elapsed error confirms timeout behavior.
    // In production, the napi wrapper converts this to error_type "timeout".
}

/// Garbage greeting: server sends non-IMAP data — do_test_imap returns Err.
#[tokio::test]
async fn test_invalid_greeting_returns_error() {
    let (port, _handle) =
        start_mock_imap_server("", MockAuthMode::GarbageGreeting).await;
    let opts = clear_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s");
    assert!(
        result.is_err(),
        "Garbage IMAP greeting must cause do_test_imap to return Err"
    );
}

/// Mid-connection drop: server accepts then closes — do_test_imap returns Err.
#[tokio::test]
async fn test_mid_connection_drop() {
    let (port, _handle) =
        start_mock_imap_server("", MockAuthMode::DropAfterGreeting).await;
    let opts = clear_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s");
    assert!(
        result.is_err(),
        "Connection drop after greeting must cause do_test_imap to return Err"
    );
}

/// Error messages include hostname and port for connection failures (napi wrapper level).
///
/// At the `do_test_imap` level, the raw IO error is returned. The napi wrapper
/// (`test_imap_connection`) formats the error as "Connection to {host}:{port} failed: {e}".
/// This test verifies that a connection refused error from 127.0.0.1 is correctly
/// classified — the hostname/port inclusion in the formatted message is tested at
/// the napi wrapper level (requires a JS runtime) and covered in Phase 3 integration tests.
#[tokio::test]
async fn test_error_includes_hostname_port() {
    // Use 127.0.0.1 with a closed port for a deterministic connection refused error.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let opts = clear_opts(port);

    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s");
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("Connection refused must return Err"),
    };
    let err_str = err.to_string();
    let classified = classify_error_str(&err_str);
    assert_eq!(
        classified, "connection_refused",
        "Error must be connection_refused. Error: {err_str}"
    );
}

// ---------------------------------------------------------------------------
// Tests: TLS path — correctly classified as tls_error
// ---------------------------------------------------------------------------

/// Direct TLS to a plain TCP mock returns Err classified as "tls_error".
///
/// rustls-platform-verifier rejects untrusted certs and non-TLS connections.
/// This test verifies TLS error classification. Real certs are validated in Phase 3.
#[tokio::test]
async fn test_tls_connection_fails_with_tls_error_on_plain_server() {
    let (port, _handle) =
        start_mock_imap_server("IDLE", MockAuthMode::AcceptPassword).await;
    let opts = tls_opts(port);
    let result = timeout(Duration::from_secs(10), do_test_imap(&opts))
        .await
        .expect("test must complete within 10s");
    let err = match result {
        Err(e) => e,
        Ok(_) => panic!("TLS against plain TCP mock must fail"),
    };
    let err_str = format!("{err:?}"); // use Debug for full error chain including source
    let classified = classify_error_str(&err_str);
    assert_eq!(
        classified, "tls_error",
        "TLS failure must classify as 'tls_error'. Error: {err_str}"
    );
}
