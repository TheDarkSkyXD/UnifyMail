// ContactGroup model — fat-row struct matching C++ ContactGroup.cpp.
//
// C++ table: ContactGroup
// Supports metadata: NO
// columnsForQuery: {id, accountId, version, data, name, bookId}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Contact group model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactGroup {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// Group name
    #[serde(default)]
    pub name: String,

    /// Book ID — JSON key "bid"
    #[serde(rename = "bid", default)]
    pub book_id: String,

    /// Google resource name — JSON key "grn"
    #[serde(rename = "grn", default, skip_serializing_if = "Option::is_none")]
    pub google_resource_name: Option<String>,
}

impl MailModel for ContactGroup {
    fn table_name() -> &'static str {
        "ContactGroup"
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
        &["id", "data", "accountId", "version", "name", "bookId"]
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
            self.name,       // ?5 name
            self.book_id,    // ?6 bookId
        ])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contact_group() -> ContactGroup {
        ContactGroup {
            id: "group1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            name: "Work Contacts".to_string(),
            book_id: "book1".to_string(),
            google_resource_name: Some("contactGroups/g123".to_string()),
        }
    }

    #[test]
    fn contact_group_serializes_with_correct_json_keys() {
        let group = sample_contact_group();
        let json = serde_json::to_value(&group).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("name").is_some(), "missing 'name'");
        assert!(json.get("bid").is_some(), "missing 'bid' (book_id)");
        assert!(json.get("grn").is_some(), "missing 'grn' (google_resource_name)");

        // No snake_case keys
        assert!(json.get("book_id").is_none(), "book_id should be renamed to 'bid'");
        assert!(json.get("google_resource_name").is_none(), "should be renamed to 'grn'");
    }

    #[test]
    fn contact_group_to_json_includes_cls() {
        let group = sample_contact_group();
        let json = group.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("ContactGroup"));
    }

    #[test]
    fn contact_group_json_roundtrip() {
        let original = sample_contact_group();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ContactGroup = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.name, deserialized.name);
        assert_eq!(original.book_id, deserialized.book_id);
    }

    #[test]
    fn contact_group_grn_omitted_when_none() {
        let mut group = sample_contact_group();
        group.google_resource_name = None;
        let json = serde_json::to_value(&group).unwrap();
        assert!(json.get("grn").is_none());
    }

    #[test]
    fn contact_group_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE ContactGroup (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                name TEXT,
                bookId TEXT
            )"
        ).unwrap();

        let group = sample_contact_group();
        let data_json = serde_json::to_string(&group.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO ContactGroup (id, data, accountId, version, name, bookId)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        ).unwrap();

        group.bind_to_statement(&mut stmt, &data_json).unwrap();

        let fetched_name: String = db.query_row(
            "SELECT name FROM ContactGroup WHERE id = ?1",
            rusqlite::params![group.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(fetched_name, "Work Contacts");
    }
}
