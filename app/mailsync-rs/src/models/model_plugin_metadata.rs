// ModelPluginMetadata — join table struct, NOT a fat-row model.
//
// This table maps model IDs to plugin IDs with optional expiration.
// It is maintained by MailStore when saving models where supports_metadata() is true.
//
// NOT implementing MailModel — this is a join table, not a fat-row model.
// There is NO data blob column on this table.
//
// Schema (from constants.h V1):
//   id VARCHAR(40)       — the parent model's id (Thread id or Message id)
//   accountId VARCHAR(8) — account id
//   objectType VARCHAR(15) — "Thread" or "Message"
//   value TEXT           — pluginId (e.g., "snooze-plugin")
//   expiration DATETIME  — unix timestamp or NULL

use serde::{Deserialize, Serialize};

/// Plugin metadata join table entry.
///
/// NOT implementing MailModel — this is a join/lookup table only.
/// The actual metadata values are embedded in the parent model's `data` JSON
/// under the `metadata` array.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelPluginMetadata {
    /// The parent model's ID (e.g., a Thread id or Message id)
    pub id: String,

    /// Account ID
    #[serde(rename = "accountId")]
    pub account_id: String,

    /// Object type ("Thread" or "Message")
    #[serde(rename = "objectType")]
    pub object_type: String,

    /// Plugin ID (e.g., "snooze-plugin") — stored in the `value` column
    pub value: String,

    /// Expiration unix timestamp (None if no expiration)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expiration: Option<i64>,
}

impl ModelPluginMetadata {
    /// Create a new ModelPluginMetadata entry.
    pub fn new(
        model_id: impl Into<String>,
        account_id: impl Into<String>,
        object_type: impl Into<String>,
        plugin_id: impl Into<String>,
        expiration: Option<i64>,
    ) -> Self {
        Self {
            id: model_id.into(),
            account_id: account_id.into(),
            object_type: object_type.into(),
            value: plugin_id.into(),
            expiration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_plugin_metadata_has_correct_fields() {
        let meta = ModelPluginMetadata::new(
            "thread1",
            "acc1",
            "Thread",
            "snooze-plugin",
            Some(1800000000),
        );

        assert_eq!(meta.id, "thread1");
        assert_eq!(meta.account_id, "acc1");
        assert_eq!(meta.object_type, "Thread");
        assert_eq!(meta.value, "snooze-plugin");
        assert_eq!(meta.expiration, Some(1800000000));
    }

    #[test]
    fn model_plugin_metadata_serializes_with_correct_keys() {
        let meta = ModelPluginMetadata::new("msg1", "acc1", "Message", "read-receipt", None);
        let json = serde_json::to_value(&meta).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("accountId").is_some(), "missing 'accountId'");
        assert!(json.get("objectType").is_some(), "missing 'objectType'");
        assert!(json.get("value").is_some(), "missing 'value'");

        // expiration is None — should be absent
        assert!(json.get("expiration").is_none(), "expiration should be absent when None");
    }

    #[test]
    fn model_plugin_metadata_json_roundtrip() {
        let original = ModelPluginMetadata::new("t:abc", "acc2", "Thread", "snooze", Some(1900000000));
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ModelPluginMetadata = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.value, deserialized.value);
        assert_eq!(original.expiration, deserialized.expiration);
    }

    #[test]
    fn model_plugin_metadata_no_data_column() {
        // ModelPluginMetadata is NOT a fat-row model — there is no data blob column.
        // Verify the struct has no such field by checking JSON keys.
        let meta = ModelPluginMetadata::new("m1", "a1", "Message", "plugin1", None);
        let json = serde_json::to_value(&meta).unwrap();

        // Should NOT have a 'data' field
        assert!(json.get("data").is_none(), "ModelPluginMetadata must NOT have a data column");
        // Should NOT have a 'v' version field
        assert!(json.get("v").is_none(), "ModelPluginMetadata must NOT have a version field");
    }

    #[test]
    fn model_plugin_metadata_inserts_into_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE ModelPluginMetadata (
                id TEXT,
                accountId TEXT,
                objectType TEXT,
                value TEXT,
                expiration INTEGER,
                PRIMARY KEY (value, id)
            )"
        ).unwrap();

        let meta = ModelPluginMetadata::new("thread1", "acc1", "Thread", "snooze-plugin", Some(1800000000));

        db.execute(
            "INSERT INTO ModelPluginMetadata (id, accountId, objectType, value, expiration)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![meta.id, meta.account_id, meta.object_type, meta.value, meta.expiration],
        ).unwrap();

        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM ModelPluginMetadata WHERE id = ?1 AND value = ?2",
            rusqlite::params![meta.id, meta.value],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }
}
