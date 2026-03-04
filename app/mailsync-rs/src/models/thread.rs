// Thread model — fat-row struct matching C++ Thread.cpp/Thread.h.
//
// Note the name mismatch: JSON key "attachmentCount" maps to indexed column "hasAttachments".
// This is a C++ quirk that must be preserved exactly.
//
// C++ table: Thread
// Supports metadata: YES (supportsMetadata() returns true)
// columnsForQuery: {id, data, accountId, version, gThrId, unread, starred, inAllMail,
//                  subject, lastMessageTimestamp, lastMessageReceivedTimestamp,
//                  lastMessageSentTimestamp, firstMessageTimestamp, hasAttachments}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Email thread model.
///
/// JSON key "attachmentCount" maps to SQLite column "hasAttachments" — this is
/// an intentional C++ name mismatch that the Rust code must replicate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    /// Primary key (format: "t:" + msgId)
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v"
    #[serde(rename = "v")]
    pub version: i64,

    /// Thread subject
    #[serde(default)]
    pub subject: String,

    /// Last message timestamp — JSON key "lmt"
    #[serde(rename = "lmt", default)]
    pub last_message_timestamp: i64,

    /// First message timestamp — JSON key "fmt"
    #[serde(rename = "fmt", default)]
    pub first_message_timestamp: i64,

    /// Last message sent timestamp — JSON key "lmst"
    #[serde(rename = "lmst", default)]
    pub last_message_sent_timestamp: i64,

    /// Last message received timestamp — JSON key "lmrt"
    #[serde(rename = "lmrt", default)]
    pub last_message_received_timestamp: i64,

    /// Gmail Thread ID — JSON key "gThrId"
    #[serde(rename = "gThrId", default, skip_serializing_if = "Option::is_none")]
    pub g_thr_id: Option<String>,

    /// Unread message count
    #[serde(default)]
    pub unread: i64,

    /// Starred message count
    #[serde(default)]
    pub starred: i64,

    /// Is in All Mail (Gmail)
    #[serde(rename = "inAllMail", default)]
    pub in_all_mail: bool,

    /// Attachment count — JSON key "attachmentCount", indexed column "hasAttachments"
    /// NOTE: C++ intentional name mismatch — JSON key != SQL column name
    #[serde(rename = "attachmentCount", default)]
    pub attachment_count: i64,

    /// FTS5 search row ID
    #[serde(rename = "searchRowId", default, skip_serializing_if = "Option::is_none")]
    pub search_row_id: Option<i64>,

    /// Folder objects array (each with _refs, _u fields)
    #[serde(default)]
    pub folders: Vec<serde_json::Value>,

    /// Label objects array (each with _refs, _u fields)
    #[serde(default)]
    pub labels: Vec<serde_json::Value>,

    /// Participant contacts array
    #[serde(default)]
    pub participants: Vec<serde_json::Value>,

    /// Plugin metadata array
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Vec<serde_json::Value>>,
}

impl MailModel for Thread {
    fn table_name() -> &'static str {
        "Thread"
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
            "gThrId",
            "unread",
            "starred",
            "inAllMail",
            "subject",
            "lastMessageTimestamp",
            "lastMessageReceivedTimestamp",
            "lastMessageSentTimestamp",
            "firstMessageTimestamp",
            "hasAttachments",
        ]
    }

    fn bind_to_statement(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        data_json: &str,
    ) -> rusqlite::Result<()> {
        stmt.execute(rusqlite::params![
            self.id,                              // ?1  id
            data_json,                            // ?2  data
            self.account_id,                      // ?3  accountId
            self.version,                         // ?4  version
            self.g_thr_id,                        // ?5  gThrId
            self.unread,                          // ?6  unread
            self.starred,                         // ?7  starred
            self.in_all_mail as i32,              // ?8  inAllMail
            self.subject,                         // ?9  subject
            self.last_message_timestamp,          // ?10 lastMessageTimestamp
            self.last_message_received_timestamp, // ?11 lastMessageReceivedTimestamp
            self.last_message_sent_timestamp,     // ?12 lastMessageSentTimestamp
            self.first_message_timestamp,         // ?13 firstMessageTimestamp
            self.attachment_count,                // ?14 hasAttachments (NOTE: col name differs from JSON key)
        ])?;
        Ok(())
    }

    fn supports_metadata() -> bool {
        true
    }

    /// Thread::after_save — maintains ThreadCategory join table and optionally ThreadSearch.
    ///
    /// ThreadCategory is the primary index used by the Electron thread list to show
    /// which threads belong to which folders/labels with their unread counts.
    ///
    /// Algorithm (matches C++ Thread::afterSave):
    /// 1. DELETE all ThreadCategory rows for this thread id
    /// 2. For each folder in `self.folders`: INSERT ThreadCategory row
    /// 3. For each label in `self.labels`: INSERT ThreadCategory row
    /// 4. If search_row_id is set: UPDATE ThreadSearch categories column
    ///
    /// ThreadCounts update (unread/total diff) is deferred to Phase 7 when
    /// the full message-to-thread propagation cycle is implemented.
    /// See: 06-DEEP-DIVE-THREAD-MAINTENANCE.md for the full algorithm.
    /// TODO(Phase 7): Implement ThreadCounts unread/total diff updates via
    ///   applyMessageAttributeChanges snapshot-diff algorithm.
    fn after_save(&self, conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        // Step 1: Clear existing ThreadCategory rows for this thread
        conn.execute(
            "DELETE FROM ThreadCategory WHERE id = ?1",
            rusqlite::params![self.id],
        )?;

        // Step 2+3: Insert ThreadCategory rows for each folder and label
        // folders and labels are JSON values with fields:
        //   "id"   — the category id (folder id or label id)
        //   "_u"   — unread count for this thread in this category
        //   "_im"  — inAllMail flag
        //   "lmrt" / "lmst" — timestamps (may not be in category objects, fall back to thread)
        let insert_sql = "INSERT OR REPLACE INTO ThreadCategory \
            (id, value, inAllMail, unread, lastMessageReceivedTimestamp, lastMessageSentTimestamp) \
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)";

        for category in self.folders.iter().chain(self.labels.iter()) {
            let cat_id = category.get("id").and_then(|v| v.as_str()).unwrap_or("");
            if cat_id.is_empty() {
                continue;
            }
            let unread = category.get("_u").and_then(|v| v.as_i64()).unwrap_or(0);
            let in_all_mail = category.get("_im").and_then(|v| v.as_i64()).unwrap_or(0);

            conn.execute(
                insert_sql,
                rusqlite::params![
                    self.id,                              // id = thread id
                    cat_id,                               // value = folder/label id
                    in_all_mail,                          // inAllMail
                    unread,                               // unread
                    self.last_message_received_timestamp, // lastMessageReceivedTimestamp
                    self.last_message_sent_timestamp,     // lastMessageSentTimestamp
                ],
            )?;
        }

        // Step 4: Update ThreadSearch categories column if search was previously indexed
        if let Some(row_id) = self.search_row_id {
            // Build categories string: space-separated category ids
            let categories: Vec<&str> = self
                .folders
                .iter()
                .chain(self.labels.iter())
                .filter_map(|c| c.get("id").and_then(|v| v.as_str()))
                .collect();
            let categories_str = categories.join(" ");

            conn.execute(
                "UPDATE ThreadSearch SET categories = ?1 WHERE rowid = ?2",
                rusqlite::params![categories_str, row_id],
            )?;
        }

        Ok(())
    }

    /// Thread::after_remove — clears ThreadCategory rows for this thread.
    fn after_remove(&self, conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "DELETE FROM ThreadCategory WHERE id = ?1",
            rusqlite::params![self.id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_thread() -> Thread {
        Thread {
            id: "t:abc123".to_string(),
            account_id: "acc1".to_string(),
            version: 2,
            subject: "Test Thread".to_string(),
            last_message_timestamp: 1700001000,
            first_message_timestamp: 1700000000,
            last_message_sent_timestamp: 1700001000,
            last_message_received_timestamp: 1700000500,
            g_thr_id: Some("gthread123".to_string()),
            unread: 2,
            starred: 1,
            in_all_mail: true,
            attachment_count: 3,
            search_row_id: Some(42),
            folders: vec![serde_json::json!({"id": "folder1", "_refs": 1, "_u": 1})],
            labels: vec![],
            participants: vec![serde_json::json!({"email": "user@example.com"})],
            metadata: None,
        }
    }

    #[test]
    fn thread_serializes_with_correct_json_keys() {
        let thread = sample_thread();
        let json = serde_json::to_value(&thread).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("subject").is_some(), "missing 'subject'");
        assert!(json.get("lmt").is_some(), "missing 'lmt'");
        assert!(json.get("fmt").is_some(), "missing 'fmt'");
        assert!(json.get("lmst").is_some(), "missing 'lmst'");
        assert!(json.get("lmrt").is_some(), "missing 'lmrt'");
        assert!(json.get("gThrId").is_some(), "missing 'gThrId'");
        assert!(json.get("unread").is_some(), "missing 'unread'");
        assert!(json.get("starred").is_some(), "missing 'starred'");
        assert!(json.get("inAllMail").is_some(), "missing 'inAllMail'");
        assert!(json.get("attachmentCount").is_some(), "missing 'attachmentCount'");
        assert!(json.get("searchRowId").is_some(), "missing 'searchRowId'");
        assert!(json.get("folders").is_some(), "missing 'folders'");
        assert!(json.get("labels").is_some(), "missing 'labels'");
        assert!(json.get("participants").is_some(), "missing 'participants'");

        // No snake_case keys
        assert!(json.get("account_id").is_none());
        assert!(json.get("last_message_timestamp").is_none());
        assert!(json.get("attachment_count").is_none(), "must use 'attachmentCount' not 'attachment_count'");
    }

    #[test]
    fn thread_to_json_includes_cls() {
        let thread = sample_thread();
        let json = thread.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("Thread"));
    }

    #[test]
    fn thread_json_roundtrip() {
        let original = sample_thread();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Thread = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.account_id, deserialized.account_id);
        assert_eq!(original.version, deserialized.version);
        assert_eq!(original.last_message_timestamp, deserialized.last_message_timestamp);
        assert_eq!(original.attachment_count, deserialized.attachment_count);
    }

    #[test]
    fn thread_hasattachments_bound_from_attachment_count() {
        // Verify that attachment_count field is bound to the 'hasAttachments' indexed column
        // The JSON key is "attachmentCount" but SQLite column is "hasAttachments"
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Thread (
                id TEXT PRIMARY KEY,
                data TEXT,
                accountId TEXT,
                version INTEGER,
                gThrId TEXT,
                unread INTEGER,
                starred INTEGER,
                inAllMail INTEGER,
                subject TEXT,
                lastMessageTimestamp INTEGER,
                lastMessageReceivedTimestamp INTEGER,
                lastMessageSentTimestamp INTEGER,
                firstMessageTimestamp INTEGER,
                hasAttachments INTEGER
            )"
        ).unwrap();

        let thread = sample_thread();
        let data_json = serde_json::to_string(&thread.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO Thread (id, data, accountId, version, gThrId, unread, starred,
             inAllMail, subject, lastMessageTimestamp, lastMessageReceivedTimestamp,
             lastMessageSentTimestamp, firstMessageTimestamp, hasAttachments)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
        ).unwrap();

        thread.bind_to_statement(&mut stmt, &data_json).unwrap();

        let has_attachments: i64 = db.query_row(
            "SELECT hasAttachments FROM Thread WHERE id = ?1",
            rusqlite::params![thread.id],
            |row| row.get(0)
        ).unwrap();
        assert_eq!(has_attachments, 3, "hasAttachments column should equal attachment_count value");
    }

    #[test]
    fn thread_supports_metadata_true() {
        assert!(Thread::supports_metadata());
    }

    #[test]
    fn thread_optional_fields_omitted_when_none() {
        let mut thread = sample_thread();
        thread.g_thr_id = None;
        thread.search_row_id = None;
        thread.metadata = None;
        let json = serde_json::to_value(&thread).unwrap();
        assert!(json.get("gThrId").is_none(), "gThrId should be absent when None");
        assert!(json.get("searchRowId").is_none(), "searchRowId should be absent when None");
        assert!(json.get("metadata").is_none(), "metadata should be absent when None");
    }
}
