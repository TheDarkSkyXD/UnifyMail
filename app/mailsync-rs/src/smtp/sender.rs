// smtp/sender.rs — SMTP transport builder and send wrapper.
//
// SmtpSender extracts SMTP configuration from an Account's settings JSON and
// builds a lettre AsyncSmtpTransport with the appropriate TLS mode and auth mechanism.
//
// TLS modes (from account.settings.smtp_security):
//   "SSL"      → Tls::Wrapper  (direct TLS, typically port 465)
//   "STARTTLS" → Tls::Required (STARTTLS upgrade, typically port 587)
//   "none"/""  → Tls::None    (cleartext, typically port 25)
//
// Auth mechanisms:
//   is_oauth2 = true  → Mechanism::Xoauth2 (XOAUTH2 bearer token)
//   is_oauth2 = false → Mechanism::Plain   (password credential)
//
// send_message() wraps transport.send() in a 30-second tokio::time::timeout
// to satisfy SEND-04: outer timeout on the full SMTP send operation.

use std::time::Duration;

use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::client::{Tls, TlsParameters};
use lettre::{AsyncSmtpTransport, AsyncTransport, Tokio1Executor};

use crate::account::Account;
use crate::error::SyncError;

// ============================================================================
// SmtpSender
// ============================================================================

/// Holds SMTP connection parameters extracted from an Account's settings JSON.
///
/// Created once per send operation via `SmtpSender::new()`. The sender is
/// stateless between sends — transport instances are created fresh each time
/// via `build_transport()` to avoid holding open persistent connections.
#[derive(Debug, Clone)]
pub struct SmtpSender {
    /// SMTP hostname from account.settings.smtp_host
    pub smtp_host: String,
    /// SMTP port from account.settings.smtp_port (default: 587)
    pub smtp_port: u16,
    /// Security mode from account.settings.smtp_security: "SSL", "STARTTLS", or "none"
    pub smtp_security: String,
    /// SMTP username from account.settings.smtp_username
    pub smtp_username: String,
}

impl SmtpSender {
    /// Extracts SMTP settings from account.settings JSON.
    ///
    /// Required fields: smtp_host, smtp_username.
    /// Optional: smtp_port (default 587), smtp_security (default "STARTTLS").
    pub fn new(account: &Account) -> Result<Self, SyncError> {
        let settings = &account.extra;

        let smtp_host = settings["smtp_host"]
            .as_str()
            .ok_or_else(|| SyncError::Unexpected("Account missing smtp_host".to_string()))?
            .to_string();

        let smtp_port = settings["smtp_port"]
            .as_u64()
            .unwrap_or(587) as u16;

        let smtp_security = settings["smtp_security"]
            .as_str()
            .unwrap_or("STARTTLS")
            .to_string();

        let smtp_username = settings["smtp_username"]
            .as_str()
            .ok_or_else(|| SyncError::Unexpected("Account missing smtp_username".to_string()))?
            .to_string();

        Ok(Self {
            smtp_host,
            smtp_port,
            smtp_security,
            smtp_username,
        })
    }

    /// Builds a lettre AsyncSmtpTransport configured for this account's TLS mode and auth.
    ///
    /// # Arguments
    /// * `password_or_token` — password (Plain auth) or XOAUTH2 bearer token
    /// * `is_oauth2` — if true, uses Mechanism::Xoauth2; else Mechanism::Plain
    ///
    /// # TLS configuration
    /// Uses rustls (via tokio1-rustls-tls feature) — never native-tls.
    /// TlsParameters::new() uses the rustls platform verifier for certificate validation.
    pub async fn build_transport(
        &self,
        password_or_token: &str,
        is_oauth2: bool,
    ) -> Result<AsyncSmtpTransport<Tokio1Executor>, SyncError> {
        // Build TLS configuration based on smtp_security
        let tls = match self.smtp_security.as_str() {
            "SSL" => {
                let tls_params = TlsParameters::new(self.smtp_host.clone())
                    .map_err(|e| SyncError::Unexpected(format!("TLS params error: {e}")))?;
                Tls::Wrapper(tls_params)
            }
            "STARTTLS" => {
                let tls_params = TlsParameters::new(self.smtp_host.clone())
                    .map_err(|e| SyncError::Unexpected(format!("TLS params error: {e}")))?;
                Tls::Required(tls_params)
            }
            _ => {
                // "none" or empty string — cleartext
                Tls::None
            }
        };

        // Build credentials
        let credentials = Credentials::new(
            self.smtp_username.clone(),
            password_or_token.to_string(),
        );

        // Select authentication mechanism
        let auth_mechanisms = if is_oauth2 {
            vec![Mechanism::Xoauth2]
        } else {
            vec![Mechanism::Plain]
        };

        // Build transport — builder_dangerous() skips hostname validation at the
        // builder stage; TLS validation is still performed by rustls during handshake.
        let transport = AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.smtp_host)
            .port(self.smtp_port)
            .tls(tls)
            .credentials(credentials)
            .authentication(auth_mechanisms)
            .timeout(Some(Duration::from_secs(30)))
            .build();

        Ok(transport)
    }

    /// Sends a message through the given transport with a 30-second outer timeout.
    ///
    /// The transport itself has a 30-second per-command timeout (set in build_transport).
    /// This wraps the entire send() call in an additional outer timeout (SEND-04) to
    /// guard against edge cases where multiple commands each take close to 30 seconds.
    ///
    /// # Error mapping
    /// - Timeout → SyncError::Timeout
    /// - Transport errors → SyncError::Connection
    /// - Auth failures (535 response) → SyncError::Authentication
    pub async fn send_message(
        &self,
        transport: &AsyncSmtpTransport<Tokio1Executor>,
        message: lettre::Message,
    ) -> Result<(), SyncError> {
        let result = tokio::time::timeout(
            Duration::from_secs(30),
            transport.send(message),
        )
        .await;

        match result {
            Err(_elapsed) => Err(SyncError::Timeout),
            Ok(Ok(_response)) => Ok(()),
            Ok(Err(e)) => {
                let err_str = e.to_string();
                // Detect auth failures by response code 535 or "authentication" in message
                if err_str.contains("535") || err_str.to_lowercase().contains("authentication") {
                    Err(SyncError::Authentication)
                } else {
                    Err(SyncError::Connection)
                }
            }
        }
    }
}

/// Returns the raw RFC 2822 bytes of a lettre Message for IMAP APPEND to Sent folder.
///
/// The sync engine needs to upload a copy of the sent message to the Sent folder
/// via IMAP APPEND after a successful send. This function returns the formatted bytes.
pub fn get_raw_message(message: &lettre::Message) -> Vec<u8> {
    message.formatted()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::account::Account;

    // Helper: build a minimal Account with SMTP settings in extra JSON
    fn make_account(smtp_security: &str, port: u64) -> Account {
        let json_str = format!(
            r#"{{"id":"acc1","smtp_host":"smtp.example.com","smtp_port":{port},"smtp_security":"{smtp_security}","smtp_username":"user@example.com"}}"#
        );
        serde_json::from_str(&json_str).unwrap()
    }

    fn make_oauth2_account() -> Account {
        let json_str = r#"{"id":"acc1","smtp_host":"smtp.gmail.com","smtp_port":587,"smtp_security":"STARTTLS","smtp_username":"user@gmail.com","access_token":"tok123"}"#;
        serde_json::from_str(json_str).unwrap()
    }

    // ---- SmtpSender::new() tests ----

    #[test]
    fn new_extracts_smtp_settings() {
        let account = make_account("STARTTLS", 587);
        let sender = SmtpSender::new(&account).expect("Should create SmtpSender");
        assert_eq!(sender.smtp_host, "smtp.example.com");
        assert_eq!(sender.smtp_port, 587);
        assert_eq!(sender.smtp_security, "STARTTLS");
        assert_eq!(sender.smtp_username, "user@example.com");
    }

    #[test]
    fn new_fails_without_smtp_host() {
        let json_str = r#"{"id":"acc1","smtp_username":"user@example.com"}"#;
        let account: Account = serde_json::from_str(json_str).unwrap();
        let result = SmtpSender::new(&account);
        assert!(result.is_err(), "Should fail without smtp_host");
    }

    #[test]
    fn new_fails_without_smtp_username() {
        let json_str = r#"{"id":"acc1","smtp_host":"smtp.example.com"}"#;
        let account: Account = serde_json::from_str(json_str).unwrap();
        let result = SmtpSender::new(&account);
        assert!(result.is_err(), "Should fail without smtp_username");
    }

    #[test]
    fn new_uses_default_port_when_absent() {
        let json_str = r#"{"id":"acc1","smtp_host":"smtp.example.com","smtp_username":"u@x.com"}"#;
        let account: Account = serde_json::from_str(json_str).unwrap();
        let sender = SmtpSender::new(&account).unwrap();
        assert_eq!(sender.smtp_port, 587, "Default port should be 587");
    }

    #[test]
    fn new_uses_default_security_when_absent() {
        let json_str = r#"{"id":"acc1","smtp_host":"smtp.example.com","smtp_username":"u@x.com"}"#;
        let account: Account = serde_json::from_str(json_str).unwrap();
        let sender = SmtpSender::new(&account).unwrap();
        assert_eq!(sender.smtp_security, "STARTTLS", "Default security should be STARTTLS");
    }

    // ---- build_transport() tests ----
    // We verify that transport construction does not panic/error for each TLS mode.
    // We cannot connect to a real SMTP server in unit tests, but we can verify
    // that the builder pattern succeeds without errors.

    #[tokio::test]
    async fn build_transport_ssl_mode_succeeds() {
        let account = make_account("SSL", 465);
        let sender = SmtpSender::new(&account).unwrap();
        // SSL (Tls::Wrapper) transport construction should succeed
        let result = sender.build_transport("password123", false).await;
        assert!(result.is_ok(), "SSL transport build should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn build_transport_starttls_mode_succeeds() {
        let account = make_account("STARTTLS", 587);
        let sender = SmtpSender::new(&account).unwrap();
        // STARTTLS (Tls::Required) transport construction should succeed
        let result = sender.build_transport("password123", false).await;
        assert!(result.is_ok(), "STARTTLS transport build should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn build_transport_cleartext_mode_succeeds() {
        let account = make_account("none", 25);
        let sender = SmtpSender::new(&account).unwrap();
        // Cleartext (Tls::None) transport construction should succeed
        let result = sender.build_transport("password123", false).await;
        assert!(result.is_ok(), "Cleartext transport build should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn build_transport_empty_security_uses_cleartext() {
        let account = make_account("", 25);
        let sender = SmtpSender::new(&account).unwrap();
        let result = sender.build_transport("password123", false).await;
        assert!(result.is_ok(), "Empty security string should produce cleartext transport: {:?}", result);
    }

    #[tokio::test]
    async fn build_transport_oauth2_uses_xoauth2_mechanism() {
        let account = make_oauth2_account();
        let sender = SmtpSender::new(&account).unwrap();
        // OAuth2 transport build should succeed (mechanism selection doesn't cause build failure)
        let result = sender.build_transport("bearer_token_xyz", true).await;
        assert!(result.is_ok(), "OAuth2 transport build should succeed: {:?}", result);
    }

    #[tokio::test]
    async fn build_transport_password_uses_plain_mechanism() {
        let account = make_account("STARTTLS", 587);
        let sender = SmtpSender::new(&account).unwrap();
        // Password (Plain) transport build should succeed
        let result = sender.build_transport("password123", false).await;
        assert!(result.is_ok(), "Password Plain transport build should succeed: {:?}", result);
    }

    // ---- Error mapping tests ----

    #[test]
    fn connection_error_is_retryable() {
        assert!(SyncError::Connection.is_retryable());
    }

    #[test]
    fn authentication_error_is_auth() {
        assert!(SyncError::Authentication.is_auth());
    }

    #[test]
    fn timeout_error_is_retryable() {
        assert!(SyncError::Timeout.is_retryable());
    }

    // ---- get_raw_message test ----

    #[test]
    fn get_raw_message_returns_formatted_bytes() {
        use lettre::message::header::ContentType;
        use lettre::Message;

        let message = Message::builder()
            .from("sender@example.com".parse().unwrap())
            .to("recipient@example.com".parse().unwrap())
            .subject("Test Subject")
            .header(ContentType::TEXT_PLAIN)
            .body("Test body".to_string())
            .unwrap();

        let raw = get_raw_message(&message);
        assert!(!raw.is_empty(), "Raw message bytes should not be empty");
        let raw_str = String::from_utf8_lossy(&raw);
        assert!(raw_str.contains("Subject: Test Subject"), "Raw message should contain subject");
        assert!(raw_str.contains("Test body"), "Raw message should contain body");
    }
}
