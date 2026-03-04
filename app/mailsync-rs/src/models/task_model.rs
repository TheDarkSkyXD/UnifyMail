// Task model — fat-row struct matching C++ Task.cpp.
//
// Named task_model.rs (not task.rs) to avoid confusion with Rust's async task concepts.
//
// IMPORTANT: Task's __cls is set to the task TYPE name (e.g., "SendDraftTask"),
// NOT "Task". The __cls is pre-set in the data before serialization.
// to_json() must NOT override __cls if it's already present.
//
// C++ table: Task
// Supports metadata: NO
// columnsForQuery: {id, data, accountId, version, status}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Mail task model (persisted operations like SendDraftTask, ChangeLabelsTask, etc.).
///
/// The `class_name` field holds the task type (e.g., "SendDraftTask") and is
/// serialized as "__cls". The to_json() implementation must NOT overwrite this
/// pre-set __cls value — it should only inject __cls if absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// Task type class name (e.g., "SendDraftTask") — JSON key "__cls"
    /// Pre-set to the task type name; to_json() must preserve this value.
    #[serde(rename = "__cls")]
    pub class_name: String,

    /// Task status: "local", "remote", "complete", "cancelled"
    #[serde(default)]
    pub status: String,

    /// Should cancel flag
    #[serde(rename = "should_cancel", default, skip_serializing_if = "Option::is_none")]
    pub should_cancel: Option<bool>,

    /// Error information (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<serde_json::Value>,
}

impl MailModel for Task {
    fn table_name() -> &'static str {
        "Task"
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
        &["id", "data", "accountId", "version", "status"]
    }

    fn to_json(&self) -> serde_json::Value {
        // Task's __cls is the task type name, already present in the struct as class_name.
        // The default to_json() would inject __cls = "Task" (table_name()), which is WRONG.
        // We serialize as-is — the __cls field is already set to the correct task type name.
        serde_json::to_value(self).expect("Task serialization failed")
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
            self.status,     // ?5 status
        ])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_task() -> Task {
        Task {
            id: "task1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            class_name: "SendDraftTask".to_string(),
            status: "local".to_string(),
            should_cancel: None,
            error: None,
        }
    }

    #[test]
    fn task_serializes_with_correct_json_keys() {
        let task = sample_task();
        let json = serde_json::to_value(&task).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("status").is_some(), "missing 'status'");
        assert!(json.get("__cls").is_some(), "missing '__cls'");
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("SendDraftTask"));
    }

    #[test]
    fn task_to_json_preserves_preset_cls() {
        // CRITICAL: Task's to_json() must preserve the pre-set __cls (task type name)
        // and NOT override it with "Task" (the table name)
        let task = sample_task();
        let json = task.to_json();

        assert_eq!(
            json.get("__cls").and_then(|v| v.as_str()),
            Some("SendDraftTask"),
            "Task.to_json() must preserve __cls = SendDraftTask, not override with 'Task'"
        );
    }

    #[test]
    fn task_to_json_different_task_types_preserve_cls() {
        let mut task = sample_task();
        task.class_name = "ChangeLabelsTask".to_string();
        let json = task.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("ChangeLabelsTask"));
    }

    #[test]
    fn task_json_roundtrip() {
        let original = sample_task();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Task = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.class_name, deserialized.class_name);
        assert_eq!(original.status, deserialized.status);
    }

    #[test]
    fn task_optional_fields_omitted_when_none() {
        let task = sample_task();
        let json = serde_json::to_value(&task).unwrap();
        assert!(json.get("should_cancel").is_none());
        assert!(json.get("error").is_none());
    }

    #[test]
    fn task_supports_metadata_false() {
        assert!(!Task::supports_metadata());
    }

    #[test]
    fn task_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Task (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                status TEXT
            )"
        ).unwrap();

        let task = sample_task();
        let data_json = serde_json::to_string(&task.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO Task (id, data, accountId, version, status)
             VALUES (?1, ?2, ?3, ?4, ?5)"
        ).unwrap();

        task.bind_to_statement(&mut stmt, &data_json).unwrap();

        let fetched_status: String = db.query_row(
            "SELECT status FROM Task WHERE id = ?1",
            rusqlite::params![task.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(fetched_status, "local");
    }
}
