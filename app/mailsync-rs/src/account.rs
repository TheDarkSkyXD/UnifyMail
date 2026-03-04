// Account and Identity deserialization structs.
//
// These are deserialized from JSON sent by the TypeScript mailsync-process.ts
// on stdin during the two-line startup handshake.
//
// Using #[serde(flatten)] for forward-compatibility: if new fields are added
// to the TypeScript Account/Identity objects in future versions, they are
// captured in `extra` rather than causing a deserialization error.

use serde::Deserialize;

/// Represents a mail account. Deserialized from account JSON sent on stdin.
/// The `id` field is the account UUID; all other fields are optional to allow
/// partial JSON during test scenarios.
///
/// Fields are defined for forward-compatibility — later phases (IMAP, SMTP)
/// will use email_address, provider, and the extra fields.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Account {
    /// Account UUID — unique identifier used as a primary key in the database
    pub id: String,

    /// The account's primary email address
    #[serde(rename = "emailAddress")]
    pub email_address: Option<String>,

    /// Email provider identifier (e.g., "gmail", "outlook", "icloud", "imap")
    pub provider: Option<String>,

    /// All other account fields (credentials, settings, etc.)
    /// Using flatten to avoid failing when TypeScript adds new fields
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

/// Represents the user's identity (display name, signature, etc.).
/// Deserialized from identity JSON sent on stdin after account JSON.
///
/// Fields are defined for forward-compatibility — later phases will
/// use the identity id and extra fields for account configuration.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct Identity {
    /// Identity UUID
    pub id: String,

    /// All other identity fields captured for forward-compatibility
    #[serde(flatten)]
    pub extra: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_deserializes_from_minimal_json() {
        let json = r#"{"id":"x","emailAddress":"e@x.com","provider":"gmail"}"#;
        let account: Account = serde_json::from_str(json).expect("Failed to deserialize Account");
        assert_eq!(account.id, "x");
        assert_eq!(account.email_address.as_deref(), Some("e@x.com"));
        assert_eq!(account.provider.as_deref(), Some("gmail"));
    }

    #[test]
    fn account_deserializes_with_extra_fields() {
        let json = r#"{"id":"acc123","emailAddress":"user@example.com","provider":"outlook","refreshToken":"tok123","customField":"value"}"#;
        let account: Account = serde_json::from_str(json).expect("Failed to deserialize Account with extra fields");
        assert_eq!(account.id, "acc123");
        assert_eq!(account.email_address.as_deref(), Some("user@example.com"));
    }

    #[test]
    fn account_deserializes_with_only_id() {
        let json = r#"{"id":"acct1"}"#;
        let account: Account = serde_json::from_str(json).expect("Failed to deserialize Account with only id");
        assert_eq!(account.id, "acct1");
        assert!(account.email_address.is_none());
        assert!(account.provider.is_none());
    }

    #[test]
    fn identity_deserializes_from_minimal_json() {
        let json = r#"{"id":"identity1"}"#;
        let identity: Identity = serde_json::from_str(json).expect("Failed to deserialize Identity");
        assert_eq!(identity.id, "identity1");
    }

    #[test]
    fn identity_deserializes_with_extra_fields() {
        let json = r#"{"id":"id123","name":"John Doe","email":"john@example.com"}"#;
        let identity: Identity = serde_json::from_str(json).expect("Failed to deserialize Identity with extra fields");
        assert_eq!(identity.id, "id123");
    }
}
