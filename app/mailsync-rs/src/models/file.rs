// File model — fat-row struct matching C++ File.cpp.
//
// Note: File objects are also embedded in Message._data["files"] as a JSON array.
// Both the File table and Message.files array must be maintained.
//
// C++ table: File
// Supports metadata: NO
// columnsForQuery: {id, data, accountId, version, filename}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// File (email attachment) metadata model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// Parent message ID
    #[serde(rename = "messageId", default)]
    pub message_id: String,

    /// IMAP BODYSTRUCTURE part ID
    #[serde(rename = "partId", default, skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,

    /// CID for inline attachments
    #[serde(rename = "contentId", default, skip_serializing_if = "Option::is_none")]
    pub content_id: Option<String>,

    /// MIME content type
    #[serde(rename = "contentType", default)]
    pub content_type: String,

    /// Display filename
    #[serde(default)]
    pub filename: String,

    /// Byte size
    #[serde(default)]
    pub size: i64,
}

impl MailModel for File {
    fn table_name() -> &'static str {
        "File"
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
        &["id", "data", "accountId", "version", "filename"]
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
            self.filename,   // ?5 filename
        ])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_file() -> File {
        File {
            id: "file1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            message_id: "msg1".to_string(),
            part_id: Some("2".to_string()),
            content_id: Some("cid:image001".to_string()),
            content_type: "application/pdf".to_string(),
            filename: "document.pdf".to_string(),
            size: 102400,
        }
    }

    #[test]
    fn file_serializes_with_correct_json_keys() {
        let file = sample_file();
        let json = serde_json::to_value(&file).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("messageId").is_some(), "missing 'messageId'");
        assert!(json.get("partId").is_some(), "missing 'partId'");
        assert!(json.get("contentId").is_some(), "missing 'contentId'");
        assert!(json.get("contentType").is_some(), "missing 'contentType'");
        assert!(json.get("filename").is_some(), "missing 'filename'");
        assert!(json.get("size").is_some(), "missing 'size'");

        // No snake_case keys
        assert!(json.get("message_id").is_none());
        assert!(json.get("content_type").is_none());
        assert!(json.get("content_id").is_none());
    }

    #[test]
    fn file_to_json_includes_cls() {
        let file = sample_file();
        let json = file.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("File"));
    }

    #[test]
    fn file_json_roundtrip() {
        let original = sample_file();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: File = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.filename, deserialized.filename);
        assert_eq!(original.content_type, deserialized.content_type);
        assert_eq!(original.size, deserialized.size);
    }

    #[test]
    fn file_optional_fields_omitted_when_none() {
        let mut file = sample_file();
        file.part_id = None;
        file.content_id = None;
        let json = serde_json::to_value(&file).unwrap();
        assert!(json.get("partId").is_none());
        assert!(json.get("contentId").is_none());
    }

    #[test]
    fn file_supports_metadata_false() {
        assert!(!File::supports_metadata());
    }

    #[test]
    fn file_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE File (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                filename TEXT
            )"
        ).unwrap();

        let file = sample_file();
        let data_json = serde_json::to_string(&file.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO File (id, data, accountId, version, filename)
             VALUES (?1, ?2, ?3, ?4, ?5)"
        ).unwrap();

        file.bind_to_statement(&mut stmt, &data_json).unwrap();

        let fetched_filename: String = db.query_row(
            "SELECT filename FROM File WHERE id = ?1",
            rusqlite::params![file.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(fetched_filename, "document.pdf");
    }
}
