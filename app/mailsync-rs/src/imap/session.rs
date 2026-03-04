// imap/session.rs — IMAP connection and session management.
//
// Implements:
//   - Two-pass folder role detection (RFC 6154 attributes first, name fallback)
//   - Gmail folder whitelist (6 folders become Folder objects, rest are Labels)
//   - ImapSession struct with connect (TLS/STARTTLS), authenticate (password/XOAUTH2),
//     list_folders, select_condstore, and uid_fetch methods.
//
// NOTE: connect() and authenticate() are implemented in Plan 03.
// This file contains the role detection helpers and struct definition.

use imap_proto::types::NameAttribute;

use crate::error::SyncError;

// ============================================================================
// Role detection helpers
// ============================================================================

/// Maps RFC 6154 special-use attributes to role strings.
///
/// Only covers the 6 defined special-use flags. There is no Inbox variant
/// in imap-proto's NameAttribute enum — inbox is detected by name only.
/// Returns None for unrecognized attributes (Custom, Extension, NoSelect, etc.).
pub fn role_for_name_attribute(attr: &NameAttribute<'_>) -> Option<&'static str> {
    match attr {
        NameAttribute::All => Some("all"),
        NameAttribute::Sent => Some("sent"),
        NameAttribute::Drafts => Some("drafts"),
        NameAttribute::Junk => Some("spam"),
        NameAttribute::Trash => Some("trash"),
        NameAttribute::Archive => Some("archive"),
        _ => None,
    }
}

/// Name-based fallback role detection.
///
/// Case-insensitive matching against known folder name patterns.
/// The `path_lower` argument must already be lowercased by the caller.
pub fn role_for_folder_via_path(path_lower: &str) -> Option<&'static str> {
    match path_lower {
        "inbox" => Some("inbox"),
        "sent" | "sent mail" | "sent items" | "sent messages" | "[gmail]/sent mail" => {
            Some("sent")
        }
        "drafts" | "draft" | "[gmail]/drafts" => Some("drafts"),
        "trash" | "deleted" | "deleted items" | "deleted messages" | "[gmail]/trash" => {
            Some("trash")
        }
        "spam" | "junk" | "junk mail" | "junk e-mail" | "[gmail]/spam" => Some("spam"),
        "archive" | "all mail" | "[gmail]/all mail" => Some("all"),
        _ => None,
    }
}

/// Two-pass folder role detection.
///
/// Pass 1: Check RFC 6154 NameAttributes via `role_for_name_attribute`.
/// Pass 2 (fallback): Check the folder path via `role_for_folder_via_path`.
///
/// Returns None if no role can be determined.
pub fn detect_folder_role(attrs: &[NameAttribute<'_>], path: &str) -> Option<String> {
    // Pass 1: attribute-based detection
    for attr in attrs {
        if let Some(role) = role_for_name_attribute(attr) {
            return Some(role.to_string());
        }
    }
    // Pass 2: name-based fallback
    let path_lower = path.to_lowercase();
    role_for_folder_via_path(&path_lower).map(|s| s.to_string())
}

/// Returns true if this folder should be synced as a Folder (not Label) for Gmail accounts.
///
/// Per the Gmail folder whitelist decision (GMAL-01), only 6 folders are synced as Folder
/// objects on Gmail accounts:
///   INBOX, [Gmail]/All Mail, [Gmail]/Trash, [Gmail]/Spam, [Gmail]/Drafts, [Gmail]/Sent Mail
///
/// INBOX is detected by path (eq_ignore_ascii_case) because it has no special NameAttribute.
/// The other 5 are detected by NameAttribute (All, Junk, Trash, Drafts, Sent).
/// All other Gmail folders (Archive, custom labels, etc.) become Label objects.
pub fn is_gmail_sync_folder(attrs: &[NameAttribute<'_>], path: &str) -> bool {
    // INBOX has no special NameAttribute — detect by path
    if path.eq_ignore_ascii_case("INBOX") {
        return true;
    }
    // The other 5 whitelisted folders all have specific NameAttributes
    attrs.iter().any(|a| {
        matches!(
            a,
            NameAttribute::All
                | NameAttribute::Junk
                | NameAttribute::Trash
                | NameAttribute::Drafts
                | NameAttribute::Sent
        )
    })
}

/// Returns true if the folder has the \NoSelect attribute, meaning it cannot be selected.
///
/// NoSelect folders are mailbox containers only (e.g., [Gmail] on Gmail) and must be
/// skipped during folder enumeration — they have no messages to sync.
pub fn is_noselect(attrs: &[NameAttribute<'_>]) -> bool {
    attrs.iter().any(|a| matches!(a, NameAttribute::NoSelect))
}

// ============================================================================
// ImapSession
// ============================================================================

use std::time::Duration;

use async_imap::Session;
use base64::Engine;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_stream::StreamExt;
use tokio_rustls::client::TlsStream;

use crate::account::Account;
use crate::models::folder::Folder;
use crate::models::label::Label;

/// Concrete TLS stream type used in IMAP sessions.
///
/// Both SSL/TLS direct and STARTTLS connections produce a TlsStream<TcpStream>
/// after the handshake, so we use this concrete type throughout ImapSession
/// rather than a boxed trait object.
pub type ImapTlsStream = TlsStream<TcpStream>;

/// Pre-authenticated IMAP connection.
///
/// Returned by `ImapSession::connect()`. Call `authenticate()` to obtain an `ImapSession`.
/// Capabilities and Gmail detection are deferred to `authenticate()` — async-imap's
/// `Client` (pre-auth) does not expose `capabilities()`; only `Session` (post-auth) does.
pub struct ImapPreAuth {
    client: async_imap::Client<ImapTlsStream>,
}

/// Internal XOAUTH2 SASL authenticator.
///
/// Constructs: base64("user=<email>\x01auth=Bearer <token>\x01\x01")
struct XOAuth2Auth {
    email: String,
    token: String,
}

impl async_imap::Authenticator for XOAuth2Auth {
    type Response = String;

    fn process(&mut self, _challenge: &[u8]) -> Self::Response {
        build_xoauth2_string(&self.email, &self.token)
    }
}

/// Builds the XOAUTH2 SASL base64 payload for IMAP AUTHENTICATE XOAUTH2.
///
/// Format: base64("user=<email>\x01auth=Bearer <token>\x01\x01")
pub fn build_xoauth2_string(email: &str, token: &str) -> String {
    let raw = format!("user={}\x01auth=Bearer {}\x01\x01", email, token);
    base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
}

impl ImapPreAuth {
    /// Authenticates with the IMAP server and returns an active `ImapSession`.
    ///
    /// - `access_token` = Some(token): XOAUTH2 AUTHENTICATE (OAuth2 bearer token)
    /// - `access_token` = None: password LOGIN via account.extra["settings"]["imap_password"]
    ///
    /// Wrapped with a 30-second timeout.
    pub async fn authenticate(
        self,
        account: &Account,
        access_token: Option<&str>,
    ) -> Result<ImapSession, SyncError> {
        let email = account
            .email_address
            .as_deref()
            .ok_or_else(|| SyncError::Unexpected("account missing emailAddress".to_string()))?
            .to_string();

        let mut session = if let Some(token) = access_token {
            // XOAUTH2 path
            let auth = XOAuth2Auth {
                email,
                token: token.to_string(),
            };
            timeout(
                Duration::from_secs(30),
                self.client.authenticate("XOAUTH2", auth),
            )
            .await
            .map_err(|_| SyncError::Timeout)?
            .map_err(|(err, _client)| SyncError::from(err))?
        } else {
            // Password LOGIN path — extract password before consuming self.client
            let pwd = account
                .extra
                .get("settings")
                .and_then(|s| s.get("imap_password"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    SyncError::Unexpected("settings missing 'imap_password'".to_string())
                })?
                .to_string();

            timeout(
                Duration::from_secs(30),
                self.client.login(email, pwd),
            )
            .await
            .map_err(|_| SyncError::Timeout)?
            .map_err(|(err, _client)| SyncError::from(err))?
        };

        // Read capabilities from the authenticated session (CAPABILITY command)
        // This is where async-imap exposes capabilities() — only on Session, not Client.
        let caps = timeout(Duration::from_secs(15), session.capabilities())
            .await
            .map_err(|_| SyncError::Timeout)?
            .map_err(SyncError::from)?;

        let capabilities: Vec<String> = caps
            .iter()
            .map(|c: &async_imap::types::Capability| format!("{c:?}"))
            .collect();

        let is_gmail = capabilities.iter().any(|c| c.contains("X-GM-EXT-1"));

        Ok(ImapSession {
            session,
            capabilities,
            is_gmail,
        })
    }
}

/// Authenticated IMAP session wrapping an async-imap Session.
///
/// Supports both direct TLS (port 993) and STARTTLS (port 143) connections,
/// and both password LOGIN and XOAUTH2 AUTHENTICATE.
///
/// # Usage
///
/// ```no_run
/// let pre_auth = ImapSession::connect(&account).await?;
/// let mut session = pre_auth.authenticate(&account, None).await?;        // password
/// // or:
/// let mut session = pre_auth.authenticate(&account, Some(token)).await?; // XOAUTH2
/// let (folders, labels) = session.list_folders(&account).await?;
/// ```
pub struct ImapSession {
    session: Session<ImapTlsStream>,
    /// Server capabilities as reported after greeting (or after CAPABILITY command).
    pub capabilities: Vec<String>,
    /// True if server advertised X-GM-EXT-1 capability (Gmail-specific extensions).
    is_gmail: bool,
}

impl ImapSession {
    /// Returns true if the connected server is Gmail (has X-GM-EXT-1 capability).
    pub fn is_gmail(&self) -> bool {
        self.is_gmail
    }

    /// Connects to the IMAP server described by `account.extra["settings"]`.
    ///
    /// Reads: `imap_host`, `imap_port`, `imap_security` ("SSL/TLS" or "STARTTLS").
    ///
    /// - SSL/TLS (port 993): TCP connect -> TLS handshake -> greeting
    /// - STARTTLS (port 143): TCP connect -> greeting -> STARTTLS command -> TLS upgrade
    ///
    /// Capability detection (CAPABILITY command) is deferred to `authenticate()` since
    /// async-imap's `Client` (pre-auth) does not expose `capabilities()` — only `Session`
    /// (post-auth) does.
    ///
    /// All I/O wrapped with a 15-second connection timeout (IMPR-05).
    /// Returns an `ImapPreAuth` handle; call `.authenticate()` to proceed.
    pub async fn connect(account: &Account) -> Result<ImapPreAuth, SyncError> {
        let settings = account
            .extra
            .get("settings")
            .ok_or_else(|| {
                SyncError::Unexpected("account missing 'settings' field".to_string())
            })?;

        let host = settings
            .get("imap_host")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SyncError::Unexpected("settings missing 'imap_host'".to_string()))?
            .to_string();

        let port = settings
            .get("imap_port")
            .and_then(|v| v.as_u64())
            .unwrap_or(993) as u16;

        let security = settings
            .get("imap_security")
            .and_then(|v| v.as_str())
            .unwrap_or("SSL/TLS")
            .to_string();

        let addr = format!("{host}:{port}");

        // Build TLS connector using platform trust store (rustls-platform-verifier 0.6)
        use rustls_platform_verifier::ConfigVerifierExt as _;
        let tls_config = tokio_rustls::rustls::ClientConfig::with_platform_verifier()
            .map_err(|e| SyncError::Unexpected(format!("TLS config error: {e}")))?;
        let connector = tokio_rustls::TlsConnector::from(std::sync::Arc::new(tls_config));
        let server_name =
            tokio_rustls::rustls::pki_types::ServerName::try_from(host.as_str())
                .map_err(|e| SyncError::Unexpected(format!("invalid hostname '{host}': {e}")))?
                .to_owned();

        let client = if security == "STARTTLS" {
            // STARTTLS: plain TCP -> greeting -> STARTTLS command -> TLS upgrade
            let tcp = timeout(Duration::from_secs(15), TcpStream::connect(&addr))
                .await
                .map_err(|_| SyncError::Timeout)?
                .map_err(|_| SyncError::Connection)?;

            let mut plain = async_imap::Client::new(tcp);

            // Read server greeting
            timeout(Duration::from_secs(15), plain.read_response())
                .await
                .map_err(|_| SyncError::Timeout)?
                .map_err(SyncError::from)?;

            // Send STARTTLS command
            timeout(
                Duration::from_secs(15),
                plain.run_command_and_check_ok("STARTTLS", None),
            )
            .await
            .map_err(|_| SyncError::Timeout)?
            .map_err(SyncError::from)?;

            // Extract TCP stream and upgrade to TLS (no greeting after STARTTLS per RFC 2595)
            let tcp = plain.into_inner();
            let tls = timeout(Duration::from_secs(15), connector.connect(server_name, tcp))
                .await
                .map_err(|_| SyncError::Timeout)?
                .map_err(|_| SyncError::SslHandshakeFailed)?;

            async_imap::Client::new(tls)
        } else {
            // SSL/TLS direct: TCP -> TLS handshake -> greeting
            let tcp = timeout(Duration::from_secs(15), TcpStream::connect(&addr))
                .await
                .map_err(|_| SyncError::Timeout)?
                .map_err(|_| SyncError::Connection)?;

            let tls = timeout(Duration::from_secs(15), connector.connect(server_name, tcp))
                .await
                .map_err(|_| SyncError::Timeout)?
                .map_err(|_| SyncError::SslHandshakeFailed)?;

            let mut client = async_imap::Client::new(tls);

            // Read server greeting (SSL/TLS only — no greeting after STARTTLS)
            timeout(Duration::from_secs(15), client.read_response())
                .await
                .map_err(|_| SyncError::Timeout)?
                .map_err(SyncError::from)?;

            client
        };

        // capabilities/is_gmail deferred to authenticate() — async-imap Client
        // (pre-auth) does not expose capabilities(); only Session (post-auth) does.
        Ok(ImapPreAuth { client })
    }

    /// Enumerates all selectable folders on the server.
    ///
    /// Issues LIST "" * with a 30-second timeout. Skips NoSelect folders.
    /// Assigns roles via two-pass detection (RFC 6154 attributes first, name fallback).
    ///
    /// For Gmail accounts (`is_gmail == true`):
    ///   - 6 whitelisted folders (INBOX, All Mail, Trash, Spam, Drafts, Sent Mail) become Folder objects
    ///   - All others become Label objects (GMAL-01)
    ///
    /// For non-Gmail accounts:
    ///   - All selectable folders become Folder objects, no Labels produced
    ///
    /// `Folder.id` is set to `"{account_id}:{folder_path}"` matching the C++ ID scheme.
    pub async fn list_folders(
        &mut self,
        account: &Account,
    ) -> Result<(Vec<Folder>, Vec<Label>), SyncError> {
        let names_stream = timeout(
            Duration::from_secs(30),
            self.session.list(Some(""), Some("*")),
        )
        .await
        .map_err(|_| SyncError::Timeout)?
        .map_err(SyncError::from)?;

        // Collect all folder names before processing
        let mut name_list = Vec::new();
        let mut s = names_stream;
        while let Some(result) = s.next().await {
            let name = result.map_err(SyncError::from)?;
            name_list.push(name);
        }

        let mut folders: Vec<Folder> = Vec::new();
        let mut labels: Vec<Label> = Vec::new();

        for name in name_list {
            let attrs = name.attributes();
            let path = name.name();

            // Skip container-only folders
            if is_noselect(attrs) {
                continue;
            }

            let role = detect_folder_role(attrs, path).unwrap_or_default();
            let id = format!("{}:{}", account.id, path);

            if self.is_gmail {
                if is_gmail_sync_folder(attrs, path) {
                    folders.push(Folder {
                        id,
                        account_id: account.id.clone(),
                        version: 1,
                        path: path.to_string(),
                        role,
                        local_status: Some(serde_json::json!({})),
                    });
                } else {
                    labels.push(Label {
                        id,
                        account_id: account.id.clone(),
                        version: 1,
                        path: path.to_string(),
                        role,
                        local_status: Some(serde_json::json!({})),
                    });
                }
            } else {
                folders.push(Folder {
                    id,
                    account_id: account.id.clone(),
                    version: 1,
                    path: path.to_string(),
                    role,
                    local_status: Some(serde_json::json!({})),
                });
            }
        }

        Ok((folders, labels))
    }

    /// SELECT with CONDSTORE extension.
    ///
    /// Selects the named mailbox and returns the Mailbox struct containing
    /// uid_validity, highest_modseq, exists count, etc.
    /// Wrapped with a 30-second timeout.
    pub async fn select_condstore(
        &mut self,
        path: &str,
    ) -> Result<async_imap::types::Mailbox, SyncError> {
        timeout(
            Duration::from_secs(30),
            self.session.select_condstore(path),
        )
        .await
        .map_err(|_| SyncError::Timeout)?
        .map_err(SyncError::from)
    }

    /// UID FETCH wrapper.
    ///
    /// Initiates UID FETCH with a 120-second timeout for stream creation.
    /// The caller iterates the returned stream with per-item timeouts.
    ///
    /// The stream borrows `self` mutably — the caller must exhaust or drop it before
    /// calling other session methods.
    /// UID FETCH wrapper — returns a pinned boxed stream.
    ///
    /// The stream is boxed to avoid `impl Stream + '_` RPIT lifetime issues across
    /// the `async fn` boundary. Callers use `tokio_stream::StreamExt` to iterate.
    pub async fn uid_fetch(
        &mut self,
        uid_set: &str,
        query: &str,
    ) -> Result<
        std::pin::Pin<
            Box<
                dyn tokio_stream::Stream<
                    Item = Result<async_imap::types::Fetch, async_imap::error::Error>,
                > + Send
                + '_,
            >,
        >,
        SyncError,
    > {
        let stream = timeout(
            Duration::from_secs(120),
            self.session.uid_fetch(uid_set, query),
        )
        .await
        .map_err(|_| SyncError::Timeout)?
        .map_err(SyncError::from)?;

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- role_for_name_attribute tests ----

    #[test]
    fn role_for_name_attribute_all() {
        assert_eq!(
            role_for_name_attribute(&NameAttribute::All),
            Some("all")
        );
    }

    #[test]
    fn role_for_name_attribute_sent() {
        assert_eq!(
            role_for_name_attribute(&NameAttribute::Sent),
            Some("sent")
        );
    }

    #[test]
    fn role_for_name_attribute_drafts() {
        assert_eq!(
            role_for_name_attribute(&NameAttribute::Drafts),
            Some("drafts")
        );
    }

    #[test]
    fn role_for_name_attribute_junk_maps_to_spam() {
        assert_eq!(
            role_for_name_attribute(&NameAttribute::Junk),
            Some("spam")
        );
    }

    #[test]
    fn role_for_name_attribute_trash() {
        assert_eq!(
            role_for_name_attribute(&NameAttribute::Trash),
            Some("trash")
        );
    }

    #[test]
    fn role_for_name_attribute_archive() {
        assert_eq!(
            role_for_name_attribute(&NameAttribute::Archive),
            Some("archive")
        );
    }

    #[test]
    fn role_for_name_attribute_noselect_returns_none() {
        assert_eq!(role_for_name_attribute(&NameAttribute::NoSelect), None);
    }

    // ---- role_for_folder_via_path tests ----

    #[test]
    fn role_for_folder_via_path_inbox() {
        assert_eq!(role_for_folder_via_path("inbox"), Some("inbox"));
    }

    #[test]
    fn role_for_folder_via_path_sent_variants() {
        assert_eq!(role_for_folder_via_path("sent"), Some("sent"));
        assert_eq!(role_for_folder_via_path("sent mail"), Some("sent"));
        assert_eq!(role_for_folder_via_path("[gmail]/sent mail"), Some("sent"));
    }

    #[test]
    fn role_for_folder_via_path_drafts() {
        assert_eq!(role_for_folder_via_path("drafts"), Some("drafts"));
        assert_eq!(role_for_folder_via_path("draft"), Some("drafts"));
    }

    #[test]
    fn role_for_folder_via_path_trash() {
        assert_eq!(role_for_folder_via_path("trash"), Some("trash"));
        assert_eq!(role_for_folder_via_path("deleted"), Some("trash"));
    }

    #[test]
    fn role_for_folder_via_path_spam() {
        assert_eq!(role_for_folder_via_path("spam"), Some("spam"));
        assert_eq!(role_for_folder_via_path("junk"), Some("spam"));
    }

    #[test]
    fn role_for_folder_via_path_archive() {
        assert_eq!(role_for_folder_via_path("archive"), Some("all"));
        assert_eq!(role_for_folder_via_path("[gmail]/all mail"), Some("all"));
    }

    #[test]
    fn role_for_folder_via_path_unknown_returns_none() {
        assert_eq!(role_for_folder_via_path("custom-folder"), None);
        assert_eq!(role_for_folder_via_path(""), None);
    }

    // ---- detect_folder_role tests ----

    #[test]
    fn detect_folder_role_attribute_wins_over_path() {
        // NameAttribute::Sent + "inbox" path — attribute should win
        let attrs = vec![NameAttribute::Sent];
        let role = detect_folder_role(&attrs, "inbox");
        assert_eq!(role.as_deref(), Some("sent"));
    }

    #[test]
    fn detect_folder_role_fallback_to_path() {
        let attrs = vec![NameAttribute::NoSelect]; // No role attribute
        let role = detect_folder_role(&attrs, "INBOX");
        assert_eq!(role.as_deref(), Some("inbox"));
    }

    #[test]
    fn detect_folder_role_no_match_returns_none() {
        let attrs = vec![NameAttribute::NoSelect];
        let role = detect_folder_role(&attrs, "custom-label");
        assert!(role.is_none());
    }

    // ---- is_gmail_sync_folder tests ----

    #[test]
    fn is_gmail_sync_folder_inbox_by_path() {
        let role = is_gmail_sync_folder(&[], "INBOX");
        assert!(role);
        let role_lower = is_gmail_sync_folder(&[], "inbox");
        assert!(role_lower);
    }

    #[test]
    fn is_gmail_sync_folder_all_mail_by_attribute() {
        let attrs = vec![NameAttribute::All];
        assert!(is_gmail_sync_folder(&attrs, "[Gmail]/All Mail"));
    }

    #[test]
    fn is_gmail_sync_folder_custom_label_false() {
        let attrs: Vec<NameAttribute<'_>> = vec![];
        assert!(!is_gmail_sync_folder(&attrs, "Work"));
    }

    // ---- is_noselect tests ----

    #[test]
    fn is_noselect_true_for_noselect_attr() {
        let attrs = vec![NameAttribute::NoSelect];
        assert!(is_noselect(&attrs));
    }

    #[test]
    fn is_noselect_false_without_attr() {
        let attrs = vec![NameAttribute::All];
        assert!(!is_noselect(&attrs));
    }
}
