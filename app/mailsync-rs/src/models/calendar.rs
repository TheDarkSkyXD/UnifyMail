// Calendar model — fat-row struct matching C++ Calendar.cpp.
//
// CRITICAL: Calendar's bindToQuery() does NOT call MailModel::bindToQuery().
// It binds only :id, :data, :accountId — NO version column.
// The table has no version column.
//
// C++ table: Calendar
// Supports metadata: NO
// columnsForQuery: {id, data, accountId} (NOTE: no version column!)

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Calendar model.
///
/// CRITICAL: bind_to_statement() does NOT bind version. The Calendar table
/// has no version column — this is a C++ design decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Calendar {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v" (in JSON but NOT in SQLite columns)
    #[serde(rename = "v")]
    pub version: i64,

    /// CalDAV path
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,

    /// Calendar display name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// CalDAV ctag (sync indicator)
    #[serde(rename = "ctag", default, skip_serializing_if = "Option::is_none")]
    pub ctag: Option<String>,

    /// CalDAV sync-token
    #[serde(rename = "syncToken", default, skip_serializing_if = "Option::is_none")]
    pub sync_token: Option<String>,

    /// Display color (hex string)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,

    /// Description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Read-only flag
    #[serde(rename = "read_only", default)]
    pub read_only: bool,

    /// Display order
    #[serde(default)]
    pub order: i64,
}

impl MailModel for Calendar {
    fn table_name() -> &'static str {
        "Calendar"
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
        // CRITICAL: No version column for Calendar
        &["id", "data", "accountId"]
    }

    fn bind_to_statement(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        data_json: &str,
    ) -> rusqlite::Result<()> {
        // CRITICAL: Calendar does NOT bind version — only id, data, accountId
        stmt.execute(rusqlite::params![
            self.id,         // ?1 id
            data_json,       // ?2 data
            self.account_id, // ?3 accountId
        ])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_calendar() -> Calendar {
        Calendar {
            id: "cal1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            path: Some("/caldav/user/calendar/".to_string()),
            name: Some("Personal".to_string()),
            ctag: Some("ctag_cal".to_string()),
            sync_token: Some("synctoken_cal".to_string()),
            color: Some("#FF5733".to_string()),
            description: Some("My personal calendar".to_string()),
            read_only: false,
            order: 0,
        }
    }

    #[test]
    fn calendar_serializes_with_correct_json_keys() {
        let cal = sample_calendar();
        let json = serde_json::to_value(&cal).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("path").is_some(), "missing 'path'");
        assert!(json.get("name").is_some(), "missing 'name'");
        assert!(json.get("ctag").is_some(), "missing 'ctag'");
        assert!(json.get("syncToken").is_some(), "missing 'syncToken'");
        assert!(json.get("color").is_some(), "missing 'color'");
        assert!(json.get("description").is_some(), "missing 'description'");
        assert!(json.get("read_only").is_some(), "missing 'read_only'");
        assert!(json.get("order").is_some(), "missing 'order'");
    }

    #[test]
    fn calendar_to_json_includes_cls() {
        let cal = sample_calendar();
        let json = cal.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("Calendar"));
    }

    #[test]
    fn calendar_json_roundtrip() {
        let original = sample_calendar();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Calendar = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.ctag, deserialized.ctag);
    }

    #[test]
    fn calendar_bind_to_statement_does_not_bind_version() {
        // Calendar table has no version column — bind_to_statement must NOT bind version
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Calendar (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT
                -- NOTE: no version column
            )"
        ).unwrap();

        let cal = sample_calendar();
        let data_json = serde_json::to_string(&cal.to_json()).unwrap();

        // The INSERT SQL has only 3 params — if version were bound, this would fail
        let mut stmt = db.prepare(
            "INSERT INTO Calendar (id, data, accountId) VALUES (?1, ?2, ?3)"
        ).unwrap();

        // This must succeed without binding version
        cal.bind_to_statement(&mut stmt, &data_json).unwrap();

        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM Calendar WHERE id = ?1",
            rusqlite::params![cal.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 1, "Calendar should be inserted without version");
    }

    #[test]
    fn calendar_optional_fields_omitted_when_none() {
        let cal = Calendar {
            id: "c1".to_string(),
            account_id: "a1".to_string(),
            version: 1,
            path: None,
            name: None,
            ctag: None,
            sync_token: None,
            color: None,
            description: None,
            read_only: false,
            order: 0,
        };
        let json = serde_json::to_value(&cal).unwrap();
        assert!(json.get("path").is_none());
        assert!(json.get("name").is_none());
        assert!(json.get("ctag").is_none());
        assert!(json.get("syncToken").is_none());
        assert!(json.get("color").is_none());
        assert!(json.get("description").is_none());
    }
}
