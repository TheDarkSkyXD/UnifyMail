// Folder model — fat-row struct matching C++ Folder.cpp/Folder.h.
//
// Label inherits from Folder in C++ and uses the same columns/JSON structure.
// The Rust Label struct re-uses this same struct shape with a different table name.
//
// C++ table: Folder
// Supports metadata: NO
// columnsForQuery: {id, data, accountId, version, path, role}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Mail folder model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// IMAP path (e.g., "INBOX", "Sent")
    #[serde(default)]
    pub path: String,

    /// Folder role (e.g., "inbox", "sent", "drafts")
    #[serde(default)]
    pub role: String,

    /// Local sync status object
    #[serde(rename = "localStatus", default, skip_serializing_if = "Option::is_none")]
    pub local_status: Option<serde_json::Value>,
}

impl MailModel for Folder {
    fn table_name() -> &'static str {
        "Folder"
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn account_id(&self) -> &str {
        &self.account_id
    }

    fn version(&self) -> i64 {
        self.version
    }

    fn increment_version(&mut self) {
        self.version += 1;
    }

    fn columns_for_query() -> &'static [&'static str] {
        &["id", "data", "accountId", "version", "path", "role"]
    }

    fn bind_to_statement(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        data_json: &str,
    ) -> rusqlite::Result<()> {
        stmt.execute(rusqlite::params![
            self.id,         // ?1 id
            data_json,       // ?2 data
            self.account_id, // ?3 accountId
            self.version,    // ?4 version
            self.path,       // ?5 path
            self.role,       // ?6 role
        ])?;
        Ok(())
    }

    fn supports_metadata() -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_folder() -> Folder {
        Folder {
            id: "folder1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            path: "INBOX".to_string(),
            role: "inbox".to_string(),
            local_status: Some(serde_json::json!({"busy": false, "lastSyncedAt": 1700000000})),
        }
    }

    #[test]
    fn folder_serializes_with_correct_json_keys() {
        let folder = sample_folder();
        let json = serde_json::to_value(&folder).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("path").is_some(), "missing 'path'");
        assert!(json.get("role").is_some(), "missing 'role'");
        assert!(json.get("localStatus").is_some(), "missing 'localStatus'");

        // No snake_case keys
        assert!(json.get("account_id").is_none());
    }

    #[test]
    fn folder_to_json_includes_cls() {
        let folder = sample_folder();
        let json = folder.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("Folder"));
    }

    #[test]
    fn folder_json_roundtrip() {
        let original = sample_folder();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Folder = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.path, deserialized.path);
        assert_eq!(original.role, deserialized.role);
    }

    #[test]
    fn folder_local_status_omitted_when_none() {
        let mut folder = sample_folder();
        folder.local_status = None;
        let json = serde_json::to_value(&folder).unwrap();
        assert!(json.get("localStatus").is_none());
    }

    #[test]
    fn folder_supports_metadata_false() {
        assert!(!Folder::supports_metadata());
    }

    #[test]
    fn folder_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Folder (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                path TEXT,
                role TEXT
            )"
        ).unwrap();

        let folder = sample_folder();
        let data_json = serde_json::to_string(&folder.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO Folder (id, data, accountId, version, path, role)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ).unwrap();

        folder.bind_to_statement(&mut stmt, &data_json).unwrap();

        let (fetched_path, fetched_role): (String, String) = db.query_row(
            "SELECT path, role FROM Folder WHERE id = ?1",
            rusqlite::params![folder.id],
            |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        assert_eq!(fetched_path, "INBOX");
        assert_eq!(fetched_role, "inbox");
    }
}
