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

use async_imap::Session;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::account::Account;

/// Trait alias for the boxed async I/O stream type used inside ImapSession.
///
/// Both TLS-direct and STARTTLS connections are boxed into this trait object,
/// allowing ImapSession to hold a single Session<Box<dyn AsyncReadWrite>> field.
pub trait AsyncReadWrite: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send + std::fmt::Debug> AsyncReadWrite for T {}

/// Authenticated IMAP session wrapping an async-imap Session.
///
/// Supports both direct TLS (port 993) and STARTTLS (port 143) connections,
/// and both password LOGIN and XOAUTH2 AUTHENTICATE.
///
/// Plan 03 implements: connect(), authenticate(), list_folders(),
/// select_condstore(), uid_fetch().
pub struct ImapSession {
    pub(crate) session: Session<Box<dyn AsyncReadWrite>>,
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

    /// Connects to the IMAP server and authenticates.
    ///
    /// Full implementation in Plan 03.
    /// Reads account.extra["settings"]: imap_host, imap_port, imap_security.
    #[allow(dead_code)]
    pub async fn connect(_account: &Account) -> Result<Self, SyncError> {
        Err(SyncError::NotImplemented(
            "ImapSession::connect implemented in Plan 03".to_string(),
        ))
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
