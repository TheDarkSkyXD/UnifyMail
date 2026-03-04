//! Integration tests for the SMTP connection testing module.
//!
//! Uses mock SMTP servers (TcpListener on random ports) so no live server is
//! required. Each test creates its own listener with no shared state — tests
//! run in parallel without a mutex.
//!
//! # Error handling
//! `do_test_smtp` returns `Err(BoxError)` for connection/auth failures.
//! The napi export converts these to `Ok(SMTPConnectionResult { success: false, ... })`.
//! Tests verify error classification by inspecting SMTPConnectionResult fields directly,
//! since the napi wrapper handles all classification and always resolves.
//!
//! # TLS note
//! Direct TLS (port 465-style) tests against a plain TCP mock will fail because
//! rustls-platform-verifier rejects untrusted/self-signed certs. We test TLS error
//! classification by checking that the result is a failure. Phase 4 validates against
//! real servers.

use mailcore_napi_rs::smtp::{do_test_smtp, SMTPConnectionOptions};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
use tokio::time::timeout;

// ---------------------------------------------------------------------------
// Mock server infrastructure
// ---------------------------------------------------------------------------

/// Controls how the mock server handles authentication.
#[derive(Clone, Debug)]
enum MockSmtpMode {
    /// Accept all connections and auth attempts.
    AcceptAll,
    /// Reject AUTH commands with 535.
    RejectAuth,
    /// Accept TCP but never sends greeting (simulates timeout).
    NeverRespond,
}

/// Start a mock SMTP server with configurable mode.
///
/// Returns `(port, JoinHandle)`. The handle keeps the server alive; dropping it
/// cancels the server task.
async fn start_mock_smtp_server(mode: MockSmtpMode) -> (u16, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    let handle = tokio::spawn(async move {
        if let Ok((stream, _addr)) = listener.accept().await {
            handle_mock_smtp_connection(stream, mode).await;
        }
    });
    (port, handle)
}

/// Handle one mock SMTP connection.
async fn handle_mock_smtp_connection(stream: tokio::net::TcpStream, mode: MockSmtpMode) {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);

    match mode {
        MockSmtpMode::NeverRespond => {
            // Accept TCP but never send greeting — connection hangs until timeout fires.
            tokio::time::sleep(Duration::from_secs(120)).await;
            return;
        }
        _ => {}
    }

    // Send SMTP greeting.
    let _ = writer
        .write_all(b"220 mock.smtp.test ESMTP ready\r\n")
        .await;

    // Read and respond to commands in a loop.
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }

        // Parse: <COMMAND> [rest]
        let mut parts = trimmed.splitn(2, ' ');
        let cmd = parts.next().unwrap_or("").to_uppercase();
        let _rest = parts.next().unwrap_or("");

        match cmd.as_str() {
            "EHLO" => {
                let _ = writer
                    .write_all(
                        b"250-mock.smtp.test\r\n\
                          250-AUTH LOGIN XOAUTH2\r\n\
                          250-SIZE 52428800\r\n\
                          250 OK\r\n",
                    )
                    .await;
            }
            "HELO" => {
                let _ = writer
                    .write_all(b"250 mock.smtp.test\r\n")
                    .await;
            }
            "AUTH" => {
                let rest = _rest.trim();
                let mechanism = rest.split_whitespace().next().unwrap_or("").to_uppercase();

                match mechanism.as_str() {
                    "LOGIN" => {
                        match &mode {
                            MockSmtpMode::RejectAuth => {
                                let _ = writer
                                    .write_all(b"535 5.7.8 Authentication credentials invalid\r\n")
                                    .await;
                            }
                            _ => {
                                // Multi-step AUTH LOGIN dialog
                                // Step 1: Ask for username
                                let _ = writer
                                    .write_all(b"334 VXNlcm5hbWU6\r\n")
                                    .await;
                                // Read base64-encoded username
                                let mut username_line = String::new();
                                match reader.read_line(&mut username_line).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(_) => {}
                                }
                                // Step 2: Ask for password
                                let _ = writer
                                    .write_all(b"334 UGFzc3dvcmQ6\r\n")
                                    .await;
                                // Read base64-encoded password
                                let mut password_line = String::new();
                                match reader.read_line(&mut password_line).await {
                                    Ok(0) | Err(_) => break,
                                    Ok(_) => {}
                                }
                                // Accept auth
                                let _ = writer
                                    .write_all(b"235 2.7.0 Authentication successful\r\n")
                                    .await;
                            }
                        }
                    }
                    "XOAUTH2" => {
                        match &mode {
                            MockSmtpMode::RejectAuth => {
                                let _ = writer
                                    .write_all(b"535 5.7.8 Authentication credentials invalid\r\n")
                                    .await;
                            }
                            _ => {
                                // XOAUTH2: client may send initial response inline or wait for challenge
                                // If initial response already provided (rest after "XOAUTH2 "), accept
                                // Otherwise send challenge
                                let initial_response = rest.trim_start_matches("XOAUTH2").trim();
                                if !initial_response.is_empty() {
                                    // Inline initial response — accept directly
                                    let _ = writer
                                        .write_all(b"235 2.7.0 Authentication successful\r\n")
                                        .await;
                                } else {
                                    // Send challenge
                                    let _ = writer.write_all(b"334 \r\n").await;
                                    // Read client response
                                    let mut xoauth_line = String::new();
                                    match reader.read_line(&mut xoauth_line).await {
                                        Ok(0) | Err(_) => break,
                                        Ok(_) => {}
                                    }
                                    let _ = writer
                                        .write_all(b"235 2.7.0 Authentication successful\r\n")
                                        .await;
                                }
                            }
                        }
                    }
                    _ => {
                        let _ = writer
                            .write_all(b"504 5.5.4 Unrecognized authentication type\r\n")
                            .await;
                    }
                }
            }
            "NOOP" => {
                let _ = writer.write_all(b"250 OK\r\n").await;
            }
            "QUIT" => {
                let _ = writer.write_all(b"221 2.0.0 Bye\r\n").await;
                break;
            }
            _ => {
                // Respond with OK to unknown commands to keep connection alive
                let _ = writer.write_all(b"250 OK\r\n").await;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

/// SMTPConnectionOptions for a clear (no-TLS) connection with password.
fn clear_opts_with_password(port: u16) -> SMTPConnectionOptions {
    SMTPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("clear".to_string()),
        username: Some("user@example.com".to_string()),
        password: Some("password123".to_string()),
        oauth2_token: None,
    }
}

/// SMTPConnectionOptions for a clear (no-TLS) connection with no credentials.
fn clear_opts_no_credentials(port: u16) -> SMTPConnectionOptions {
    SMTPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("clear".to_string()),
        username: None,
        password: None,
        oauth2_token: None,
    }
}

/// SMTPConnectionOptions for a TLS connection with password.
fn tls_opts_with_password(port: u16) -> SMTPConnectionOptions {
    SMTPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("tls".to_string()),
        username: Some("user@example.com".to_string()),
        password: Some("password123".to_string()),
        oauth2_token: None,
    }
}

/// SMTPConnectionOptions for a STARTTLS connection with password.
fn starttls_opts_with_password(port: u16) -> SMTPConnectionOptions {
    SMTPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("starttls".to_string()),
        username: Some("user@example.com".to_string()),
        password: Some("password123".to_string()),
        oauth2_token: None,
    }
}

// ---------------------------------------------------------------------------
// Tests: clear connection
// ---------------------------------------------------------------------------

/// Clear connection with no credentials (connect-only) returns success.
#[tokio::test]
async fn test_clear_no_credentials_connect_only() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::AcceptAll).await;
    let opts = clear_opts_no_credentials(port);
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError for connect-only");
    assert!(
        result.success,
        "Connect-only clear must succeed, got: {:?}",
        result.error
    );
    assert!(result.error.is_none(), "No error on success");
    assert!(result.error_type.is_none(), "No error_type on success");
}

/// Clear connection with password auth returns success.
#[tokio::test]
async fn test_clear_connection_succeeds() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::AcceptAll).await;
    let opts = clear_opts_with_password(port);
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError");
    assert!(
        result.success,
        "Clear connection with password must succeed, got: {:?}",
        result.error
    );
}

// ---------------------------------------------------------------------------
// Tests: password authentication
// ---------------------------------------------------------------------------

/// Password auth against accepting mock server returns success.
#[tokio::test]
async fn test_password_auth() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::AcceptAll).await;
    let opts = clear_opts_with_password(port);
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError");
    assert!(
        result.success,
        "Password auth must succeed against AcceptAll mock, got: {:?}",
        result.error
    );
}

/// XOAUTH2 auth against accepting mock server returns success.
#[tokio::test]
async fn test_xoauth2_auth() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::AcceptAll).await;
    let opts = SMTPConnectionOptions {
        hostname: "127.0.0.1".to_string(),
        port: port as u32,
        connection_type: Some("clear".to_string()),
        username: Some("user@example.com".to_string()),
        password: None,
        oauth2_token: Some("ya29.valid_oauth2_token".to_string()),
    };
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError");
    assert!(
        result.success,
        "XOAUTH2 auth must succeed against AcceptAll mock, got: {:?}",
        result.error
    );
}

// ---------------------------------------------------------------------------
// Tests: error scenarios
// ---------------------------------------------------------------------------

/// Auth failure: mock rejects auth — result has success=false, errorType=auth_failed.
#[tokio::test]
async fn test_auth_failure() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::RejectAuth).await;
    let opts = clear_opts_with_password(port);
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError (errors are returned as SMTPConnectionResult)");
    assert!(
        !result.success,
        "Auth failure must return success=false"
    );
    let error_type = result
        .error_type
        .as_deref()
        .expect("error_type must be present on failure");
    assert_eq!(
        error_type, "auth_failed",
        "Auth failure must have errorType=auth_failed, got: {error_type}"
    );
}

/// Connection refused: connecting to a closed port returns success=false, errorType=connection_refused.
#[tokio::test]
async fn test_connection_refused() {
    // Bind to get a free port, then drop so nothing is listening.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let opts = clear_opts_with_password(port);
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError");
    assert!(
        !result.success,
        "Connection refused must return success=false"
    );
    let error_type = result
        .error_type
        .as_deref()
        .expect("error_type must be present on failure");
    assert_eq!(
        error_type, "connection_refused",
        "Connection refused must have errorType=connection_refused, got: {error_type}"
    );
}

/// Timeout: mock accepts TCP but never responds — result has success=false, errorType=timeout.
///
/// Using 3-second timeout to keep tests fast. Production uses 15 seconds.
#[tokio::test]
async fn test_timeout() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::NeverRespond).await;
    let opts = clear_opts_with_password(port);

    // Use 3-second timeout for fast test execution (same pattern as imap_tests.rs).
    let result = timeout(Duration::from_secs(3), do_test_smtp(&opts))
        .await
        .expect("outer test timeout must fire")
        .expect("do_test_smtp must not return BoxError");
    assert!(
        !result.success,
        "Timeout must return success=false"
    );
    let error_type = result
        .error_type
        .as_deref()
        .expect("error_type must be present on timeout");
    assert_eq!(
        error_type, "timeout",
        "Timeout must have errorType=timeout, got: {error_type}"
    );
}

/// TLS against plain TCP mock: connection fails with errorType=tls_error.
///
/// rustls-platform-verifier rejects non-TLS connections; error classification
/// categorizes TLS handshake failures as "tls_error".
#[tokio::test]
async fn test_tls_against_plain_server() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::AcceptAll).await;
    let opts = tls_opts_with_password(port);
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError");
    assert!(
        !result.success,
        "TLS against plain TCP mock must fail"
    );
    let error_type = result
        .error_type
        .as_deref()
        .expect("error_type must be present on TLS failure");
    assert_eq!(
        error_type, "tls_error",
        "TLS failure must have errorType=tls_error, got: {error_type}"
    );
}

/// STARTTLS against plain TCP mock: connection fails with an error (tls_error or connection failure).
///
/// lettre's starttls_relay() will attempt TLS upgrade; plain TCP mock will not respond
/// correctly to TLS handshake, resulting in an error.
#[tokio::test]
async fn test_starttls_against_plain_server() {
    let (port, _handle) = start_mock_smtp_server(MockSmtpMode::AcceptAll).await;
    let opts = starttls_opts_with_password(port);
    let result = timeout(Duration::from_secs(10), do_test_smtp(&opts))
        .await
        .expect("test must complete within 10s")
        .expect("do_test_smtp must not return BoxError");
    assert!(
        !result.success,
        "STARTTLS against plain TCP mock must fail"
    );
    let error_type = result
        .error_type
        .as_deref()
        .expect("error_type must be present on STARTTLS failure");
    // STARTTLS failure can be tls_error (TLS handshake fails) or unknown (SMTP protocol error)
    assert!(
        error_type == "tls_error" || error_type == "unknown",
        "STARTTLS failure must have errorType tls_error or unknown, got: {error_type}"
    );
}
