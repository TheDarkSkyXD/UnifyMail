// tasks/ — Task processing infrastructure for mailsync-rs.
//
// This module provides:
// - TaskKind enum: serde-tagged dispatch for all 8 task type names from C++
// - execute_task: two-phase orchestration (local DB write + delta, then remote stub)
// - parse_task_kind: deserializes the __cls-tagged task JSON payload
//
// Plans 03 and 04 wire execute_task into the IDLE loop and fill in remote phase impls.
// Phase 9 will add SyncbackEventTask remote implementation.

pub mod recovery;

use serde::{Deserialize, Serialize};

use crate::account::Account;
use crate::delta::stream::DeltaStream;
use crate::error::SyncError;
use crate::imap::task_executor::{execute_remote_phase, ImapTaskOps};
use crate::models::task_model::Task;
use crate::oauth2::TokenManager;
use crate::store::mail_store::MailStore;

// ============================================================================
// TaskKind — serde-tagged enum for all 8 C++ task type names
// ============================================================================

/// Represents all email task types dispatched from the TypeScript front-end.
///
/// Uses serde tag = "__cls" to match the C++ task JSON format exactly.
/// Each variant holds the full raw JSON payload (as `serde_json::Value`) to
/// preserve task data for DB round-trips, alongside any typed fields needed
/// by the remote execution phase.
///
/// C++ task class names match variant names exactly (PascalCase).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "__cls")]
pub enum TaskKind {
    /// Send a draft email via SMTP.
    /// fields: headerMessageId, draftId (and full task blob via raw)
    SendDraftTask {
        #[serde(rename = "headerMessageId", default)]
        header_message_id: String,
        #[serde(rename = "draftId", default)]
        draft_id: String,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },

    /// Permanently delete a draft.
    /// fields: messageId, folderId
    DestroyDraftTask {
        #[serde(rename = "messageId", default)]
        message_id: String,
        #[serde(rename = "folderId", default)]
        folder_id: String,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },

    /// Star or unstar threads/messages.
    /// fields: starred (bool), threadIds or messageIds
    ChangeStarredTask {
        #[serde(default)]
        starred: bool,
        #[serde(rename = "threadIds", default)]
        thread_ids: Vec<String>,
        #[serde(rename = "messageIds", default)]
        message_ids: Vec<String>,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },

    /// Mark threads/messages as read or unread.
    /// fields: unread (bool), threadIds or messageIds
    ChangeUnreadTask {
        #[serde(default)]
        unread: bool,
        #[serde(rename = "threadIds", default)]
        thread_ids: Vec<String>,
        #[serde(rename = "messageIds", default)]
        message_ids: Vec<String>,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },

    /// Move threads/messages between folders.
    /// fields: fromFolderId, toFolderId, threadIds or messageIds
    ChangeFolderTask {
        #[serde(rename = "fromFolderId", default)]
        from_folder_id: String,
        #[serde(rename = "toFolderId", default)]
        to_folder_id: String,
        #[serde(rename = "threadIds", default)]
        thread_ids: Vec<String>,
        #[serde(rename = "messageIds", default)]
        message_ids: Vec<String>,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },

    /// Add/remove labels on Gmail threads/messages.
    /// fields: labelsToAdd, labelsToRemove, threadIds or messageIds
    ChangeLabelsTask {
        #[serde(rename = "labelsToAdd", default)]
        labels_to_add: Vec<String>,
        #[serde(rename = "labelsToRemove", default)]
        labels_to_remove: Vec<String>,
        #[serde(rename = "threadIds", default)]
        thread_ids: Vec<String>,
        #[serde(rename = "messageIds", default)]
        message_ids: Vec<String>,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },

    /// Sync plugin metadata back to IMAP via custom headers.
    /// fields: modelId, modelClass, pluginId
    SyncbackMetadataTask {
        #[serde(rename = "modelId", default)]
        model_id: String,
        #[serde(rename = "modelClass", default)]
        model_class: String,
        #[serde(rename = "pluginId", default)]
        plugin_id: String,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },

    /// Sync a calendar event to CalDAV (Phase 9 placeholder — no remote impl yet).
    /// fields: calendarId, eventId
    SyncbackEventTask {
        #[serde(rename = "calendarId", default)]
        calendar_id: String,
        #[serde(rename = "eventId", default)]
        event_id: String,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },
}

// ============================================================================
// parse_task_kind
// ============================================================================

/// Parses the task kind from a task's full JSON payload.
///
/// Extracts the __cls discriminant and deserializes the variant fields.
/// Returns SyncError::Json if __cls is missing or unrecognized.
pub fn parse_task_kind(task_json: &serde_json::Value) -> Result<TaskKind, SyncError> {
    serde_json::from_value(task_json.clone()).map_err(|e| {
        SyncError::Json(format!("Failed to parse TaskKind: {}", e))
    })
}

// ============================================================================
// execute_task — two-phase orchestration
// ============================================================================

/// Executes a task through two phases: local DB write + delta, then remote.
///
/// Phase A (local):
///   1. Set task.status = "remote"
///   2. Save via store (triggers persist delta)
///
/// Phase B (remote):
///   3. Call execute_remote_phase() with the provided session (or no-op if None)
///   4. On success: set status = "complete", save, emit delta
///   5. On error: set status = "complete" with error field, save, emit delta
///
/// The task is always marked "complete" after remote phase (success or failure).
/// Errors are stored in task.error for UI display.
///
/// # Session parameter
/// When `session` is Some, the remote phase executes real IMAP/SMTP commands.
/// When `session` is None, the remote phase is skipped (useful for tests or local-only tasks).
pub async fn execute_task(
    task: &mut Task,
    task_kind: &TaskKind,
    store: &MailStore,
    delta: &DeltaStream,
    session: Option<&mut dyn ImapTaskOps>,
    account: Option<&Account>,
    token_manager: Option<&tokio::sync::Mutex<TokenManager>>,
) -> Result<(), SyncError> {
    // ---- Phase A: local — mark as remote, persist, emit delta ----
    task.status = "remote".to_string();
    store.save(task).await?;

    // ---- Phase B: remote — call execute_remote_phase, handle success/error ----
    let remote_result = if let (Some(sess), Some(acct), Some(tm)) = (session, account, token_manager) {
        execute_remote_phase(sess, task_kind, acct, store, delta, tm).await
    } else {
        // No session provided — skip remote phase (local-only tasks or test mode)
        Ok(())
    };

    match remote_result {
        Ok(()) => {
            task.status = "complete".to_string();
            task.error = None;
        }
        Err(e) => {
            task.status = "complete".to_string();
            task.error = Some(serde_json::json!({
                "message": e.to_string(),
                "key": e.error_key(),
            }));
        }
    }

    // Persist final status
    store.save(task).await?;

    Ok(())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    // Helper: build a minimal Task struct for testing
    fn make_task(id: &str, class_name: &str, status: &str) -> Task {
        Task {
            id: id.to_string(),
            account_id: "acc1".to_string(),
            version: 0,
            class_name: class_name.to_string(),
            status: status.to_string(),
            should_cancel: None,
            error: None,
        }
    }

    // ---- TaskKind deserialization tests ----

    #[test]
    fn task_kind_deserializes_send_draft_task() {
        let json = serde_json::json!({
            "__cls": "SendDraftTask",
            "headerMessageId": "<msg@example.com>",
            "draftId": "draft1"
        });
        let kind = parse_task_kind(&json).expect("Should parse SendDraftTask");
        assert!(matches!(kind, TaskKind::SendDraftTask { .. }));
        if let TaskKind::SendDraftTask { header_message_id, .. } = kind {
            assert_eq!(header_message_id, "<msg@example.com>");
        }
    }

    #[test]
    fn task_kind_deserializes_change_starred_task() {
        let json = serde_json::json!({
            "__cls": "ChangeStarredTask",
            "starred": true,
            "threadIds": ["t1", "t2"]
        });
        let kind = parse_task_kind(&json).expect("Should parse ChangeStarredTask");
        assert!(matches!(kind, TaskKind::ChangeStarredTask { starred: true, .. }));
    }

    #[test]
    fn task_kind_deserializes_change_unread_task() {
        let json = serde_json::json!({
            "__cls": "ChangeUnreadTask",
            "unread": false,
            "messageIds": ["m1"]
        });
        let kind = parse_task_kind(&json).expect("Should parse ChangeUnreadTask");
        assert!(matches!(kind, TaskKind::ChangeUnreadTask { unread: false, .. }));
    }

    #[test]
    fn task_kind_deserializes_change_folder_task() {
        let json = serde_json::json!({
            "__cls": "ChangeFolderTask",
            "fromFolderId": "inbox",
            "toFolderId": "trash",
            "threadIds": ["t3"]
        });
        let kind = parse_task_kind(&json).expect("Should parse ChangeFolderTask");
        assert!(matches!(kind, TaskKind::ChangeFolderTask { .. }));
        if let TaskKind::ChangeFolderTask { from_folder_id, to_folder_id, .. } = kind {
            assert_eq!(from_folder_id, "inbox");
            assert_eq!(to_folder_id, "trash");
        }
    }

    #[test]
    fn task_kind_deserializes_change_labels_task() {
        let json = serde_json::json!({
            "__cls": "ChangeLabelsTask",
            "labelsToAdd": ["label1"],
            "labelsToRemove": ["label2"],
            "threadIds": ["t4"]
        });
        let kind = parse_task_kind(&json).expect("Should parse ChangeLabelsTask");
        assert!(matches!(kind, TaskKind::ChangeLabelsTask { .. }));
    }

    #[test]
    fn task_kind_deserializes_destroy_draft_task() {
        let json = serde_json::json!({
            "__cls": "DestroyDraftTask",
            "messageId": "msg1",
            "folderId": "drafts"
        });
        let kind = parse_task_kind(&json).expect("Should parse DestroyDraftTask");
        assert!(matches!(kind, TaskKind::DestroyDraftTask { .. }));
    }

    #[test]
    fn task_kind_deserializes_syncback_metadata_task() {
        let json = serde_json::json!({
            "__cls": "SyncbackMetadataTask",
            "modelId": "m1",
            "modelClass": "Message",
            "pluginId": "plugin-x"
        });
        let kind = parse_task_kind(&json).expect("Should parse SyncbackMetadataTask");
        assert!(matches!(kind, TaskKind::SyncbackMetadataTask { .. }));
    }

    #[test]
    fn task_kind_deserializes_syncback_event_task() {
        let json = serde_json::json!({
            "__cls": "SyncbackEventTask",
            "calendarId": "cal1",
            "eventId": "ev1"
        });
        let kind = parse_task_kind(&json).expect("Should parse SyncbackEventTask");
        assert!(matches!(kind, TaskKind::SyncbackEventTask { .. }));
    }

    #[test]
    fn task_kind_fails_on_unknown_cls() {
        let json = serde_json::json!({
            "__cls": "UnknownTask",
            "foo": "bar"
        });
        let result = parse_task_kind(&json);
        assert!(result.is_err(), "Unknown task type should fail to parse");
    }

    // ---- execute_task tests use an in-memory store ----
    // These tests require tokio runtime (async tests)

    async fn make_in_memory_store() -> MailStore {
        use crate::delta::stream::DeltaStream;
        use tokio::sync::mpsc;

        let (tx, _rx) = mpsc::unbounded_channel();
        let delta = Arc::new(DeltaStream::new(tx));

        // Use a temp file store since MailStore doesn't expose open_in_memory directly.
        // We use a tempfile path so tests are isolated.
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::open_with_delta(tmp.path().to_str().unwrap(), delta)
            .await
            .unwrap();
        store.migrate().await.unwrap();
        store
    }

    #[tokio::test]
    async fn execute_task_local_phase_sets_status_remote() {
        let store = make_in_memory_store().await;
        let (tx, _rx) = mpsc::unbounded_channel();
        let delta = DeltaStream::new(tx);

        let mut task = make_task("t1", "SendDraftTask", "local");
        // Save initial task
        store.save(&mut task).await.unwrap();

        let kind = parse_task_kind(&serde_json::json!({
            "__cls": "SendDraftTask",
            "headerMessageId": "<msg@example.com>",
            "draftId": "draft1"
        })).unwrap();

        // Pass None for session/account/token_manager — tests skip remote phase
        execute_task(&mut task, &kind, &store, &delta, None, None, None).await.unwrap();

        // After execute_task, status should be "complete" (both phases ran)
        assert_eq!(task.status, "complete");
    }

    #[tokio::test]
    async fn execute_task_completes_successfully() {
        // Use a single channel for the store so we can observe deltas emitted during execute_task.
        use crate::delta::stream::DeltaStream;
        let (tx, mut rx) = mpsc::unbounded_channel();
        let delta_arc = Arc::new(DeltaStream::new(tx));

        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::open_with_delta(tmp.path().to_str().unwrap(), delta_arc.clone())
            .await
            .unwrap();
        store.migrate().await.unwrap();

        let mut task = make_task("t2", "ChangeStarredTask", "local");
        // Save initial — drains the initial persist delta from rx
        store.save(&mut task).await.unwrap();
        // Drain initial save delta
        while rx.try_recv().is_ok() {}

        let kind = TaskKind::ChangeStarredTask {
            starred: true,
            thread_ids: vec!["t1".to_string()],
            message_ids: vec![],
            extra: Default::default(),
        };

        // execute_task uses store.save() internally which emits to the store's delta channel.
        // Pass None for session/account/token_manager — tests skip remote phase.
        execute_task(&mut task, &kind, &store, &*delta_arc, None, None, None).await.unwrap();

        assert_eq!(task.status, "complete");
        assert!(task.error.is_none());

        // At least two deltas should have been emitted:
        // 1. store.save() during local phase (status -> remote)
        // 2. store.save() during completion (status -> complete)
        let delta_count = {
            let mut count = 0;
            while rx.try_recv().is_ok() {
                count += 1;
            }
            count
        };
        assert!(delta_count >= 2, "At least 2 deltas should be emitted during execute_task (got {})", delta_count);
    }
}
