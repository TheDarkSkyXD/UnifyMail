// Label model — same structure as Folder, different table name.
//
// In C++, Label is a subclass of Folder, sharing the same bindToQuery()
// and columnsForQuery(). The Rust implementation mirrors this: Label and Folder
// have identical JSON structures but different table_name() values.
//
// C++ table: Label
// Supports metadata: NO (inherits from Folder)
// columnsForQuery: {id, data, accountId, version, path, role}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Mail label model (same structure as Folder, different table name).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Label {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// IMAP path
    #[serde(default)]
    pub path: String,

    /// Label role
    #[serde(default)]
    pub role: String,

    /// Local sync status object
    #[serde(rename = "localStatus", default, skip_serializing_if = "Option::is_none")]
    pub local_status: Option<serde_json::Value>,
}

impl MailModel for Label {
    fn table_name() -> &'static str {
        "Label"
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

    fn sample_label() -> Label {
        Label {
            id: "label1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            path: "\\Important".to_string(),
            role: "important".to_string(),
            local_status: None,
        }
    }

    #[test]
    fn label_serializes_same_keys_as_folder() {
        let label = sample_label();
        let json = serde_json::to_value(&label).unwrap();

        assert!(json.get("id").is_some());
        assert!(json.get("aid").is_some());
        assert!(json.get("v").is_some());
        assert!(json.get("path").is_some());
        assert!(json.get("role").is_some());
    }

    #[test]
    fn label_table_name_is_label() {
        assert_eq!(Label::table_name(), "Label");
    }

    #[test]
    fn label_to_json_includes_cls_label() {
        let label = sample_label();
        let json = label.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("Label"),
            "Label.__cls must be 'Label', not 'Folder'");
    }

    #[test]
    fn label_json_roundtrip() {
        let original = sample_label();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Label = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.path, deserialized.path);
        assert_eq!(original.role, deserialized.role);
    }

    #[test]
    fn label_supports_metadata_false() {
        assert!(!Label::supports_metadata());
    }

    #[test]
    fn label_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Label (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                path TEXT,
                role TEXT
            )"
        ).unwrap();

        let label = sample_label();
        let data_json = serde_json::to_string(&label.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO Label (id, data, accountId, version, path, role)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ).unwrap();

        label.bind_to_statement(&mut stmt, &data_json).unwrap();

        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM Label WHERE id = ?1",
            rusqlite::params![label.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }
}
