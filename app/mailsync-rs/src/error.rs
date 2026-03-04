// SyncError — error type for the mailsync-rs binary.
//
// Error key strings MUST match the C++ error keys verbatim because
// LocalizedErrorStrings in mailsync-process.ts maps them to user-facing messages.
// Any mismatch causes the TypeScript side to show "Unknown error" to the user.
//
// JSON error shape on stdout: { "error": "ErrorKey", "error_service": "imap"|"smtp", "log": "..." }
// This shape is parsed by mailsync-process.ts _spawnAndWait() at lines 290-293.

use thiserror::Error;

/// Sync process errors. Each variant maps to an exact C++ error key string.
///
/// All ~20 C++ error key variants are defined upfront per 05-CONTEXT.md:
/// "Full error enum defined upfront covering all ~20 C++ error keys as Rust enum variants.
/// Later phases use existing variants rather than adding new ones."
/// Variants not yet used by Phase 5 are marked allow(dead_code).
#[allow(dead_code)]
#[derive(Debug, Error)]
pub enum SyncError {
    // Authentication / credential errors
    #[error("IMAP/SMTP authentication failed")]
    Authentication,

    #[error("Network connection failed")]
    Connection,

    #[error("TLS not available on server")]
    TLSNotAvailable,

    #[error("TLS certificate error")]
    Certificate,

    #[error("Protocol parse error: {0}")]
    Parse(String),

    #[error("Gmail IMAP is not enabled for this account")]
    GmailIMAPNotEnabled,

    #[error("Unexpected error: {0}")]
    Unexpected(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("I/O error: {0}")]
    Io(String),

    #[error("JSON error: {0}")]
    Json(String),

    #[error("Feature not yet implemented: {0}")]
    NotImplemented(String),

    #[error("Request timed out")]
    Timeout,

    #[error("Retryable transient error: {0}")]
    Retryable(String),

    #[error("Invalid credentials")]
    InvalidCredentials,

    #[error("Microsoft Exchange is not enabled or accessible")]
    ExchangeNotEnabled,

    #[error("Identity is missing — cannot proceed")]
    IdentityMissing,

    #[error("Yahoo does not allow sending from this address")]
    YahooSendNotAllowed,

    #[error("No route to host")]
    NoRouteToHost,

    #[error("DNS lookup failed")]
    DnsResolutionFailed,

    #[error("SSL handshake failed")]
    SslHandshakeFailed,
}

impl SyncError {
    /// Returns the exact C++ error key string for this error.
    /// These strings are used as keys in LocalizedErrorStrings in mailsync-process.ts.
    pub fn error_key(&self) -> &str {
        match self {
            SyncError::Authentication => "ErrorAuthentication",
            SyncError::Connection => "ErrorConnection",
            SyncError::TLSNotAvailable => "ErrorTLSNotAvailable",
            SyncError::Certificate => "ErrorCertificate",
            SyncError::Parse(_) => "ErrorParse",
            SyncError::GmailIMAPNotEnabled => "ErrorGmailIMAPNotEnabled",
            SyncError::Unexpected(_) => "ErrorUnexpected",
            SyncError::Protocol(_) => "ErrorParse",
            SyncError::Database(_) => "ErrorConnection",
            SyncError::Io(_) => "ErrorConnection",
            SyncError::Json(_) => "ErrorParse",
            SyncError::NotImplemented(_) => "ErrorNotImplemented",
            SyncError::Timeout => "ErrorTimeout",
            SyncError::Retryable(_) => "ErrorRetryable",
            SyncError::InvalidCredentials => "ErrorInvalidCredentials",
            SyncError::ExchangeNotEnabled => "ErrorExchangeNotEnabled",
            SyncError::IdentityMissing => "ErrorIdentityMissing",
            SyncError::YahooSendNotAllowed => "ErrorYahooSendNotAllowed",
            SyncError::NoRouteToHost => "ErrorNoRouteToHost",
            SyncError::DnsResolutionFailed => "ErrorDNSResolution",
            SyncError::SslHandshakeFailed => "ErrorTLSNotAvailable",
        }
    }

    /// Builds the JSON error object for stdout output.
    /// Shape: { "error": "ErrorKey", "error_service": "imap"|"smtp", "log": "details" }
    /// This is parsed by mailsync-process.ts _spawnAndWait() at lines 290-293.
    pub fn to_json_error(&self, service: &str) -> serde_json::Value {
        serde_json::json!({
            "error": self.error_key(),
            "error_service": service,
            "log": self.to_string(),
        })
    }
}

// ============================================================================
// From conversions — allow using ? operator with common error types
// ============================================================================

impl From<std::io::Error> for SyncError {
    fn from(e: std::io::Error) -> Self {
        SyncError::Io(e.to_string())
    }
}

impl From<serde_json::Error> for SyncError {
    fn from(e: serde_json::Error) -> Self {
        SyncError::Json(e.to_string())
    }
}

impl From<rusqlite::Error> for SyncError {
    fn from(e: rusqlite::Error) -> Self {
        SyncError::Database(e.to_string())
    }
}

impl From<tokio_rusqlite::Error> for SyncError {
    fn from(e: tokio_rusqlite::Error) -> Self {
        SyncError::Database(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_key_authentication() {
        assert_eq!(SyncError::Authentication.error_key(), "ErrorAuthentication");
    }

    #[test]
    fn error_key_connection() {
        assert_eq!(SyncError::Connection.error_key(), "ErrorConnection");
    }

    #[test]
    fn error_key_tls_not_available() {
        assert_eq!(SyncError::TLSNotAvailable.error_key(), "ErrorTLSNotAvailable");
    }

    #[test]
    fn error_key_certificate() {
        assert_eq!(SyncError::Certificate.error_key(), "ErrorCertificate");
    }

    #[test]
    fn error_key_gmail_imap_not_enabled() {
        assert_eq!(
            SyncError::GmailIMAPNotEnabled.error_key(),
            "ErrorGmailIMAPNotEnabled"
        );
    }

    #[test]
    fn error_key_not_implemented() {
        assert_eq!(
            SyncError::NotImplemented("test".into()).error_key(),
            "ErrorNotImplemented"
        );
    }

    #[test]
    fn to_json_error_shape() {
        let err = SyncError::Authentication;
        let json = err.to_json_error("imap");
        assert_eq!(json["error"], "ErrorAuthentication");
        assert_eq!(json["error_service"], "imap");
        assert!(json["log"].is_string());
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let sync_err: SyncError = io_err.into();
        assert!(matches!(sync_err, SyncError::Io(_)));
    }

    #[test]
    fn from_serde_json_error() {
        let json_err = serde_json::from_str::<serde_json::Value>("not json").unwrap_err();
        let sync_err: SyncError = json_err.into();
        assert!(matches!(sync_err, SyncError::Json(_)));
    }
}
