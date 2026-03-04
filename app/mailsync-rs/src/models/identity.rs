// Identity — plain struct, NOT implementing MailModel.
//
// Identity is never stored in SQLite. In C++, Identity::tableName() and
// Identity::columnsForQuery() both call assert(false) — this model is purely
// an in-memory singleton for process-level identity (the user's identity from
// the stdin handshake).
//
// This struct is separate from account::Identity which handles stdin deserialization.
// This version has the full field set from the C++ Identity model.

use serde::{Deserialize, Serialize};

/// Process-level identity — the user's account identity from the stdin handshake.
///
/// NOT implementing MailModel — Identity is never stored in SQLite.
/// Fields match the C++ Identity model JSON keys.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    /// Identity UUID
    pub id: String,

    /// Primary email address
    #[serde(rename = "emailAddress", default, skip_serializing_if = "Option::is_none")]
    pub email_address: Option<String>,

    /// First name
    #[serde(rename = "firstName", default, skip_serializing_if = "Option::is_none")]
    pub first_name: Option<String>,

    /// Last name
    #[serde(rename = "lastName", default, skip_serializing_if = "Option::is_none")]
    pub last_name: Option<String>,

    /// Auth token
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,

    /// Creation timestamp
    #[serde(rename = "createdAt", default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_is_a_plain_struct() {
        // Identity should NOT implement MailModel.
        // This test verifies it can be constructed and serialized without MailModel.
        let identity = Identity {
            id: "identity1".to_string(),
            email_address: Some("user@example.com".to_string()),
            first_name: Some("Jane".to_string()),
            last_name: Some("Doe".to_string()),
            token: Some("tok_abc".to_string()),
            created_at: Some(1700000000),
        };

        let json = serde_json::to_value(&identity).unwrap();
        assert_eq!(json.get("id").and_then(|v| v.as_str()), Some("identity1"));
        assert_eq!(json.get("emailAddress").and_then(|v| v.as_str()), Some("user@example.com"));
        assert_eq!(json.get("firstName").and_then(|v| v.as_str()), Some("Jane"));
        assert_eq!(json.get("lastName").and_then(|v| v.as_str()), Some("Doe"));

        // No snake_case keys
        assert!(json.get("email_address").is_none());
        assert!(json.get("first_name").is_none());
        assert!(json.get("last_name").is_none());

        // No __cls — Identity is not a MailModel
        assert!(json.get("__cls").is_none());
    }

    #[test]
    fn identity_json_roundtrip() {
        let original = Identity {
            id: "id1".to_string(),
            email_address: Some("test@example.com".to_string()),
            first_name: Some("Test".to_string()),
            last_name: None,
            token: None,
            created_at: None,
        };
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Identity = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.email_address, deserialized.email_address);
    }

    #[test]
    fn identity_optional_fields_omitted_when_none() {
        let identity = Identity {
            id: "id1".to_string(),
            email_address: None,
            first_name: None,
            last_name: None,
            token: None,
            created_at: None,
        };
        let json = serde_json::to_value(&identity).unwrap();
        assert!(json.get("emailAddress").is_none());
        assert!(json.get("firstName").is_none());
        assert!(json.get("lastName").is_none());
        assert!(json.get("token").is_none());
        assert!(json.get("createdAt").is_none());
    }
}
