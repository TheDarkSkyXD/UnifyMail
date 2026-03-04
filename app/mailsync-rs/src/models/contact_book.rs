// ContactBook model — fat-row struct matching C++ ContactBook.cpp.
//
// C++ table: ContactBook
// Supports metadata: NO
// columnsForQuery: {id, accountId, version, data} (no extra indexed columns)

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Contact book (CardDAV address book) model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactBook {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// CardDAV URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    /// Source type
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,

    /// CardDAV ctag
    #[serde(rename = "ctag", default, skip_serializing_if = "Option::is_none")]
    pub ctag: Option<String>,

    /// CardDAV sync-token
    #[serde(rename = "syncToken", default, skip_serializing_if = "Option::is_none")]
    pub sync_token: Option<String>,
}

impl MailModel for ContactBook {
    fn table_name() -> &'static str {
        "ContactBook"
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
        // ContactBook.bindToQuery() calls only MailModel::bindToQuery() — no extra columns
        &["id", "data", "accountId", "version"]
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
        ])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contact_book() -> ContactBook {
        ContactBook {
            id: "book1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            url: Some("https://carddav.example.com/user/contacts/".to_string()),
            source: Some("carddav".to_string()),
            ctag: Some("ctag_abc".to_string()),
            sync_token: Some("token_xyz".to_string()),
        }
    }

    #[test]
    fn contact_book_serializes_with_correct_json_keys() {
        let book = sample_contact_book();
        let json = serde_json::to_value(&book).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("url").is_some(), "missing 'url'");
        assert!(json.get("source").is_some(), "missing 'source'");
        assert!(json.get("ctag").is_some(), "missing 'ctag'");
        assert!(json.get("syncToken").is_some(), "missing 'syncToken'");
    }

    #[test]
    fn contact_book_to_json_includes_cls() {
        let book = sample_contact_book();
        let json = book.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("ContactBook"));
    }

    #[test]
    fn contact_book_json_roundtrip() {
        let original = sample_contact_book();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: ContactBook = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.url, deserialized.url);
        assert_eq!(original.ctag, deserialized.ctag);
    }

    #[test]
    fn contact_book_optional_fields_omitted_when_none() {
        let book = ContactBook {
            id: "b1".to_string(),
            account_id: "a1".to_string(),
            version: 1,
            url: None,
            source: None,
            ctag: None,
            sync_token: None,
        };
        let json = serde_json::to_value(&book).unwrap();
        assert!(json.get("url").is_none());
        assert!(json.get("source").is_none());
        assert!(json.get("ctag").is_none());
        assert!(json.get("syncToken").is_none());
    }

    #[test]
    fn contact_book_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE ContactBook (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER
            )"
        ).unwrap();

        let book = sample_contact_book();
        let data_json = serde_json::to_string(&book.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO ContactBook (id, data, accountId, version)
             VALUES (?1, ?2, ?3, ?4)"
        ).unwrap();

        book.bind_to_statement(&mut stmt, &data_json).unwrap();

        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM ContactBook WHERE id = ?1",
            rusqlite::params![book.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }
}
