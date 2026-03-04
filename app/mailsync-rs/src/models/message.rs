// Message model — fat-row struct matching C++ Message.cpp/Message.h.
//
// Serializes to the exact JSON keys the TypeScript Electron app expects.
// The `data` column stores the full JSON blob; indexed columns mirror the
// C++ columnsForQuery() binding order.
//
// C++ table: Message
// Supports metadata: YES (supportsMetadata() returns true)
// columnsForQuery: {id, data, accountId, version, headerMessageId, subject,
//                  gMsgId, date, draft, unread, starred, remoteUID,
//                  remoteXGMLabels, remoteFolderId, threadId}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Email message model.
///
/// All serde renames match C++ JSON keys verbatim.
/// Optional fields use skip_serializing_if = "Option::is_none" to omit nulls from JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// Synced at unix timestamp — JSON key "_sa"
    #[serde(rename = "_sa", default, skip_serializing_if = "Option::is_none")]
    pub synced_at: Option<i64>,

    /// Sync unsaved changes flag — JSON key "_suc"
    #[serde(rename = "_suc", default, skip_serializing_if = "Option::is_none")]
    pub sync_unsaved_changes: Option<i64>,

    /// IMAP UID — JSON key "remoteUID"
    #[serde(rename = "remoteUID", default)]
    pub remote_uid: u32,

    /// Header date or receivedDate (unix timestamp)
    #[serde(default)]
    pub date: i64,

    /// Email subject
    #[serde(default)]
    pub subject: String,

    /// Header Message-ID — JSON key "hMsgId"
    #[serde(rename = "hMsgId", default)]
    pub header_message_id: String,

    /// Gmail Message ID — JSON key "gMsgId"
    #[serde(rename = "gMsgId", default, skip_serializing_if = "Option::is_none")]
    pub g_msg_id: Option<String>,

    /// Gmail Thread ID — JSON key "gThrId"
    #[serde(rename = "gThrId", default, skip_serializing_if = "Option::is_none")]
    pub g_thr_id: Option<String>,

    /// Reply-To Header Message-ID — JSON key "rthMsgId"
    #[serde(rename = "rthMsgId", default, skip_serializing_if = "Option::is_none")]
    pub reply_to_header_message_id: Option<String>,

    /// Forwarded Header Message-ID — JSON key "fwdMsgId"
    #[serde(rename = "fwdMsgId", default, skip_serializing_if = "Option::is_none")]
    pub forwarded_header_message_id: Option<String>,

    /// Is unread
    #[serde(default)]
    pub unread: bool,

    /// Is starred
    #[serde(default)]
    pub starred: bool,

    /// Is draft
    #[serde(default)]
    pub draft: bool,

    /// X-GM-LABELS array (Gmail label strings)
    #[serde(default)]
    pub labels: Vec<String>,

    /// Extra IMAP headers object
    #[serde(rename = "extraHeaders", default, skip_serializing_if = "Option::is_none")]
    pub extra_headers: Option<serde_json::Value>,

    /// From contacts array
    #[serde(default)]
    pub from: Vec<serde_json::Value>,

    /// To contacts array
    #[serde(default)]
    pub to: Vec<serde_json::Value>,

    /// Cc contacts array
    #[serde(default)]
    pub cc: Vec<serde_json::Value>,

    /// Bcc contacts array
    #[serde(default)]
    pub bcc: Vec<serde_json::Value>,

    /// Reply-To contacts array
    #[serde(rename = "replyTo", default)]
    pub reply_to: Vec<serde_json::Value>,

    /// Client folder JSON (Folder.toJSON())
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<serde_json::Value>,

    /// Remote folder JSON — remoteFolderId indexed column uses remoteFolder["id"]
    #[serde(rename = "remoteFolder", default, skip_serializing_if = "Option::is_none")]
    pub remote_folder: Option<serde_json::Value>,

    /// Thread ID this message belongs to
    #[serde(rename = "threadId", default)]
    pub thread_id: String,

    /// Short preview snippet
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snippet: Option<String>,

    /// Is plaintext only
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plaintext: Option<bool>,

    /// File attachments array
    #[serde(default)]
    pub files: Vec<serde_json::Value>,

    /// Plugin metadata array (join table entries)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<serde_json::Value>>,
}

impl MailModel for Message {
    fn table_name() -> &'static str {
        "Message"
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
            "id",
            "data",
            "accountId",
            "version",
            "headerMessageId",
            "subject",
            "gMsgId",
            "date",
            "draft",
            "unread",
            "starred",
            "remoteUID",
            "remoteXGMLabels",
            "remoteFolderId",
            "threadId",
        ]
    }

    fn bind_to_statement(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        data_json: &str,
    ) -> rusqlite::Result<()> {
        // Serialize labels as JSON array for remoteXGMLabels column
        let labels_json = serde_json::to_string(&self.labels)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;

        // Extract remoteFolderId from remoteFolder["id"] if present
        let remote_folder_id: Option<String> = self
            .remote_folder
            .as_ref()
            .and_then(|f| f.get("id"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        stmt.execute(rusqlite::params![
            self.id,                // ?1  id
            data_json,              // ?2  data
            self.account_id,        // ?3  accountId
            self.version,           // ?4  version
            self.header_message_id, // ?5  headerMessageId
            self.subject,           // ?6  subject
            self.g_msg_id,          // ?7  gMsgId
            self.date,              // ?8  date
            self.draft as i32,      // ?9  draft
            self.unread as i32,     // ?10 unread
            self.starred as i32,    // ?11 starred
            self.remote_uid,        // ?12 remoteUID
            labels_json,            // ?13 remoteXGMLabels
            remote_folder_id,       // ?14 remoteFolderId
            self.thread_id,         // ?15 threadId
        ])?;
        Ok(())
    }

    fn supports_metadata() -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_message() -> Message {
        Message {
            id: "msg_abc123".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            synced_at: Some(1700000000),
            sync_unsaved_changes: Some(0),
            remote_uid: 42,
            date: 1700000000,
            subject: "Test Subject".to_string(),
            header_message_id: "<test@example.com>".to_string(),
            g_msg_id: Some("gmsg123".to_string()),
            g_thr_id: Some("gthr123".to_string()),
            reply_to_header_message_id: Some("<replyto@example.com>".to_string()),
            forwarded_header_message_id: Some("<fwd@example.com>".to_string()),
            unread: true,
            starred: false,
            draft: false,
            labels: vec!["\\Inbox".to_string()],
            extra_headers: Some(serde_json::json!({"X-Custom": "value"})),
            from: vec![serde_json::json!({"email": "sender@example.com"})],
            to: vec![serde_json::json!({"email": "recipient@example.com"})],
            cc: vec![],
            bcc: vec![],
            reply_to: vec![],
            folder: Some(serde_json::json!({"id": "folder1", "name": "INBOX"})),
            remote_folder: Some(serde_json::json!({"id": "rfolder1", "name": "INBOX"})),
            thread_id: "t:abc123".to_string(),
            snippet: Some("Preview of the message...".to_string()),
            plaintext: Some(false),
            files: vec![],
            metadata: None,
        }
    }

    #[test]
    fn message_serializes_with_correct_json_keys() {
        let msg = sample_message();
        let json = serde_json::to_value(&msg).unwrap();

        // Verify exact C++ JSON key names
        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("_sa").is_some(), "missing '_sa'");
        assert!(json.get("_suc").is_some(), "missing '_suc'");
        assert!(json.get("remoteUID").is_some(), "missing 'remoteUID'");
        assert!(json.get("date").is_some(), "missing 'date'");
        assert!(json.get("subject").is_some(), "missing 'subject'");
        assert!(json.get("hMsgId").is_some(), "missing 'hMsgId'");
        assert!(json.get("gMsgId").is_some(), "missing 'gMsgId'");
        assert!(json.get("gThrId").is_some(), "missing 'gThrId'");
        assert!(json.get("rthMsgId").is_some(), "missing 'rthMsgId'");
        assert!(json.get("fwdMsgId").is_some(), "missing 'fwdMsgId'");
        assert!(json.get("unread").is_some(), "missing 'unread'");
        assert!(json.get("starred").is_some(), "missing 'starred'");
        assert!(json.get("draft").is_some(), "missing 'draft'");
        assert!(json.get("labels").is_some(), "missing 'labels'");
        assert!(json.get("extraHeaders").is_some(), "missing 'extraHeaders'");
        assert!(json.get("from").is_some(), "missing 'from'");
        assert!(json.get("to").is_some(), "missing 'to'");
        assert!(json.get("cc").is_some(), "missing 'cc'");
        assert!(json.get("bcc").is_some(), "missing 'bcc'");
        assert!(json.get("replyTo").is_some(), "missing 'replyTo'");
        assert!(json.get("folder").is_some(), "missing 'folder'");
        assert!(json.get("remoteFolder").is_some(), "missing 'remoteFolder'");
        assert!(json.get("threadId").is_some(), "missing 'threadId'");
        assert!(json.get("snippet").is_some(), "missing 'snippet'");
        assert!(json.get("plaintext").is_some(), "missing 'plaintext'");
        assert!(json.get("files").is_some(), "missing 'files'");

        // Verify no snake_case keys leaked
        assert!(json.get("account_id").is_none(), "found 'account_id' — should be 'aid'");
        assert!(json.get("header_message_id").is_none(), "found 'header_message_id' — should be 'hMsgId'");
        assert!(json.get("g_msg_id").is_none(), "found 'g_msg_id' — should be 'gMsgId'");
        assert!(json.get("reply_to_header_message_id").is_none(), "found snake_case rthMsgId");
    }

    #[test]
    fn message_to_json_includes_cls() {
        let msg = sample_message();
        let json = msg.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("Message"),
            "to_json() must inject __cls: Message");
    }

    #[test]
    fn message_json_roundtrip() {
        let original = sample_message();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Message = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.account_id, deserialized.account_id);
        assert_eq!(original.version, deserialized.version);
        assert_eq!(original.header_message_id, deserialized.header_message_id);
        assert_eq!(original.subject, deserialized.subject);
        assert_eq!(original.g_msg_id, deserialized.g_msg_id);
        assert_eq!(original.thread_id, deserialized.thread_id);
        assert_eq!(original.unread, deserialized.unread);
        assert_eq!(original.starred, deserialized.starred);
    }

    #[test]
    fn message_optional_fields_omitted_when_none() {
        let msg = Message {
            id: "m1".to_string(),
            account_id: "a1".to_string(),
            version: 1,
            synced_at: None,
            sync_unsaved_changes: None,
            remote_uid: 0,
            date: 0,
            subject: String::new(),
            header_message_id: String::new(),
            g_msg_id: None,
            g_thr_id: None,
            reply_to_header_message_id: None,
            forwarded_header_message_id: None,
            unread: false,
            starred: false,
            draft: false,
            labels: vec![],
            extra_headers: None,
            from: vec![],
            to: vec![],
            cc: vec![],
            bcc: vec![],
            reply_to: vec![],
            folder: None,
            remote_folder: None,
            thread_id: String::new(),
            snippet: None,
            plaintext: None,
            files: vec![],
            metadata: None,
        };
        let json = serde_json::to_value(&msg).unwrap();

        // Optional fields should be absent when None
        assert!(json.get("_sa").is_none(), "_sa should be absent when None");
        assert!(json.get("_suc").is_none(), "_suc should be absent when None");
        assert!(json.get("gMsgId").is_none(), "gMsgId should be absent when None");
        assert!(json.get("gThrId").is_none(), "gThrId should be absent when None");
        assert!(json.get("rthMsgId").is_none(), "rthMsgId should be absent when None");
        assert!(json.get("fwdMsgId").is_none(), "fwdMsgId should be absent when None");
        assert!(json.get("extraHeaders").is_none(), "extraHeaders should be absent when None");
        assert!(json.get("snippet").is_none(), "snippet should be absent when None");
        assert!(json.get("plaintext").is_none(), "plaintext should be absent when None");
        assert!(json.get("metadata").is_none(), "metadata should be absent when None");
    }

    #[test]
    fn message_supports_metadata_true() {
        assert!(Message::supports_metadata(), "Message must support metadata");
    }

    #[test]
    fn message_table_name() {
        assert_eq!(Message::table_name(), "Message");
    }

    #[test]
    fn message_bind_to_statement_against_real_sqlite() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Message (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                headerMessageId TEXT,
                subject TEXT,
                gMsgId TEXT,
                date INTEGER,
                draft INTEGER,
                unread INTEGER,
                starred INTEGER,
                remoteUID INTEGER,
                remoteXGMLabels TEXT,
                remoteFolderId TEXT,
                threadId TEXT
            )"
        ).unwrap();

        let msg = sample_message();
        let data_json = serde_json::to_string(&msg.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO Message (id, data, accountId, version, headerMessageId, subject,
             gMsgId, date, draft, unread, starred, remoteUID, remoteXGMLabels,
             remoteFolderId, threadId)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"
        ).unwrap();

        msg.bind_to_statement(&mut stmt, &data_json).unwrap();

        let count: i64 = db.query_row(
            "SELECT COUNT(*) FROM Message WHERE id = ?1",
            rusqlite::params![msg.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(count, 1, "Message should be inserted");

        // Verify indexed columns were bound correctly
        let (fetched_unread, fetched_thread_id): (i32, String) = db.query_row(
            "SELECT unread, threadId FROM Message WHERE id = ?1",
            rusqlite::params![msg.id],
            |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        assert_eq!(fetched_unread, 1);
        assert_eq!(fetched_thread_id, "t:abc123");
    }
}
