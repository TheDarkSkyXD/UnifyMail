// Contact model — fat-row struct matching C++ Contact.cpp/Contact.h.
//
// Note abbreviated JSON keys: "s" for source, "h" for hidden, "gis" for contact groups,
// "grn" for Google resource name, "bid" for book id.
//
// C++ table: Contact
// Supports metadata: NO
// columnsForQuery: {id, data, accountId, version, refs, email, hidden, source, etag, bookId}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Contact model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contact {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// Email address
    #[serde(default)]
    pub email: String,

    /// Source type (mail, carddav, gpeople) — JSON key "s"
    #[serde(rename = "s", default)]
    pub source: String,

    /// Reference count (number of messages this contact appears in)
    #[serde(default)]
    pub refs: i64,

    /// Contact group IDs — JSON key "gis"
    #[serde(rename = "gis", default)]
    pub contact_groups: Vec<String>,

    /// Contact info object (vcf or Google contact info)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub info: Option<serde_json::Value>,

    /// Display name
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Google resource name — JSON key "grn"
    #[serde(rename = "grn", default, skip_serializing_if = "Option::is_none")]
    pub google_resource_name: Option<String>,

    /// CardDAV etag
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,

    /// Book ID — JSON key "bid"
    #[serde(rename = "bid", default, skip_serializing_if = "Option::is_none")]
    pub book_id: Option<String>,

    /// Hidden flag — JSON key "h"
    #[serde(rename = "h", default)]
    pub hidden: bool,
}

impl MailModel for Contact {
    fn table_name() -> &'static str {
        "Contact"
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
        &[
            "id", "data", "accountId", "version", "refs", "email", "hidden", "source",
            "etag", "bookId",
        ]
    }

    fn bind_to_statement(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        data_json: &str,
    ) -> rusqlite::Result<()> {
        stmt.execute(rusqlite::params![
            self.id,              // ?1  id
            data_json,            // ?2  data
            self.account_id,      // ?3  accountId
            self.version,         // ?4  version
            self.refs,            // ?5  refs
            self.email,           // ?6  email
            self.hidden as i32,   // ?7  hidden
            self.source,          // ?8  source
            self.etag,            // ?9  etag
            self.book_id,         // ?10 bookId
        ])?;
        Ok(())
    }

    fn supports_metadata() -> bool {
        false
    }

    /// Contact::after_save — maintains the ContactSearch FTS5 index.
    ///
    /// - version == 1 (new contact): INSERT INTO ContactSearch
    /// - version > 1 AND source != "mail": UPDATE ContactSearch
    ///   (mail-sourced contacts are ephemeral; only addressbook contacts get updated)
    ///
    /// The search content matches C++ Contact::searchContent():
    /// "{name} {email}" (space-separated name and email address)
    fn after_save(&self, conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        let content = format!(
            "{} {}",
            self.name.as_deref().unwrap_or(""),
            self.email
        );

        if self.version == 1 {
            conn.execute(
                "INSERT INTO ContactSearch (content_id, content) VALUES (?1, ?2)",
                rusqlite::params![self.id, content],
            )?;
        } else if self.source != "mail" {
            conn.execute(
                "UPDATE ContactSearch SET content = ?1 WHERE content_id = ?2",
                rusqlite::params![content, self.id],
            )?;
        }

        Ok(())
    }

    /// Contact::after_remove — removes the ContactSearch FTS5 row.
    fn after_remove(&self, conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "DELETE FROM ContactSearch WHERE content_id = ?1",
            rusqlite::params![self.id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_contact() -> Contact {
        Contact {
            id: "contact1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            email: "alice@example.com".to_string(),
            source: "mail".to_string(),
            refs: 5,
            contact_groups: vec!["group1".to_string()],
            info: Some(serde_json::json!({"fn": "Alice Smith"})),
            name: Some("Alice Smith".to_string()),
            google_resource_name: Some("people/c1234".to_string()),
            etag: Some("etag123".to_string()),
            book_id: Some("book1".to_string()),
            hidden: false,
        }
    }

    #[test]
    fn contact_serializes_with_correct_json_keys() {
        let contact = sample_contact();
        let json = serde_json::to_value(&contact).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("email").is_some(), "missing 'email'");
        assert!(json.get("s").is_some(), "missing 's' (source)");
        assert!(json.get("refs").is_some(), "missing 'refs'");
        assert!(json.get("gis").is_some(), "missing 'gis' (contact_groups)");
        assert!(json.get("info").is_some(), "missing 'info'");
        assert!(json.get("name").is_some(), "missing 'name'");
        assert!(json.get("grn").is_some(), "missing 'grn' (google_resource_name)");
        assert!(json.get("etag").is_some(), "missing 'etag'");
        assert!(json.get("bid").is_some(), "missing 'bid' (book_id)");
        assert!(json.get("h").is_some(), "missing 'h' (hidden)");

        // No snake_case keys
        assert!(json.get("account_id").is_none());
        assert!(json.get("source").is_none(), "source should be renamed to 's'");
        assert!(json.get("hidden").is_none(), "hidden should be renamed to 'h'");
        assert!(json.get("contact_groups").is_none(), "contact_groups should be renamed to 'gis'");
        assert!(json.get("book_id").is_none(), "book_id should be renamed to 'bid'");
        assert!(json.get("google_resource_name").is_none(), "google_resource_name should be renamed to 'grn'");
    }

    #[test]
    fn contact_to_json_includes_cls() {
        let contact = sample_contact();
        let json = contact.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("Contact"));
    }

    #[test]
    fn contact_json_roundtrip() {
        let original = sample_contact();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Contact = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.email, deserialized.email);
        assert_eq!(original.source, deserialized.source);
        assert_eq!(original.hidden, deserialized.hidden);
    }

    #[test]
    fn contact_optional_fields_omitted_when_none() {
        let mut contact = sample_contact();
        contact.info = None;
        contact.name = None;
        contact.google_resource_name = None;
        contact.etag = None;
        contact.book_id = None;
        let json = serde_json::to_value(&contact).unwrap();
        assert!(json.get("info").is_none());
        assert!(json.get("name").is_none());
        assert!(json.get("grn").is_none());
        assert!(json.get("etag").is_none());
        assert!(json.get("bid").is_none());
    }

    #[test]
    fn contact_supports_metadata_false() {
        assert!(!Contact::supports_metadata());
    }

    #[test]
    fn contact_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Contact (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                refs INTEGER,
                email TEXT,
                hidden INTEGER,
                source TEXT,
                etag TEXT,
                bookId TEXT
            )"
        ).unwrap();

        let contact = sample_contact();
        let data_json = serde_json::to_string(&contact.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO Contact (id, data, accountId, version, refs, email, hidden, source, etag, bookId)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"
        ).unwrap();

        contact.bind_to_statement(&mut stmt, &data_json).unwrap();

        let (fetched_email, fetched_hidden): (String, i32) = db.query_row(
            "SELECT email, hidden FROM Contact WHERE id = ?1",
            rusqlite::params![contact.id],
            |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        assert_eq!(fetched_email, "alice@example.com");
        assert_eq!(fetched_hidden, 0);
    }
}
