// tasks/recovery.rs — Crash recovery and completed task expiry.
//
// On startup after a crash, tasks stuck in "remote" status are reset to "local"
// so they can be re-queued for execution. Tasks that were mid-flight (remote phase
// started but never completed) will be re-executed.
//
// Completed tasks older than TASK_RETENTION_SECS (15 minutes) are deleted on a
// periodic timer to keep the Task table from growing unboundedly.
//
// Both functions operate directly on the MailStore writer connection via
// tokio-rusqlite call() for async access to SQLite.
//
// Retention period matches C++ TaskProcessor.cpp: 15 minutes = 900 seconds.

#![allow(dead_code)]

use crate::error::SyncError;
use crate::store::mail_store::MailStore;

/// Retention period for completed tasks (15 minutes, matching C++ TaskProcessor.cpp).
pub const TASK_RETENTION_SECS: i64 = 900;

// ============================================================================
// reset_stuck_tasks
// ============================================================================

/// Resets all tasks stuck in "remote" status back to "local" on startup.
///
/// Tasks in "remote" status mean the remote phase was started but not completed
/// (e.g., due to a crash). Resetting them to "local" ensures they are re-queued
/// on the next IDLE loop cycle.
///
/// Returns the number of tasks that were reset.
/// Logs a warning if any tasks were found stuck.
pub async fn reset_stuck_tasks(store: &MailStore) -> Result<usize, SyncError> {
    let count = store
        .writer
        .call(|conn| -> Result<usize, rusqlite::Error> {
            let n = conn.execute(
                "UPDATE Task \
                 SET status = 'local', data = json_set(data, '$.status', 'local') \
                 WHERE status = 'remote'",
                [],
            )?;
            Ok(n)
        })
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

    if count > 0 {
        tracing::warn!(
            "Crash recovery: reset {} task(s) from 'remote' to 'local' for re-execution",
            count
        );
    }

    Ok(count)
}

// ============================================================================
// expire_completed_tasks
// ============================================================================

/// Deletes completed tasks older than `retention_secs` seconds.
///
/// Uses the `completed_at` field stored in the task data JSON blob.
/// Tasks without a `completed_at` timestamp are not deleted (defensive guard).
///
/// Returns the number of tasks deleted.
/// Logs at debug level.
pub async fn expire_completed_tasks(
    store: &MailStore,
    retention_secs: i64,
) -> Result<usize, SyncError> {
    let count = store
        .writer
        .call(move |conn| -> Result<usize, rusqlite::Error> {
            // Use datetime arithmetic to compare completed_at against the retention window.
            // json_extract safely returns NULL if the field is missing — those rows are skipped.
            let n = conn.execute(
                &format!(
                    "DELETE FROM Task \
                     WHERE status = 'complete' \
                     AND json_extract(data, '$.completed_at') IS NOT NULL \
                     AND json_extract(data, '$.completed_at') < datetime('now', '-{} seconds')",
                    retention_secs
                ),
                [],
            )?;
            Ok(n)
        })
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

    tracing::debug!("Task expiry: deleted {} completed task(s) older than {}s", count, retention_secs);

    Ok(count)
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delta::stream::DeltaStream;
    use crate::models::mail_model::MailModel;
    use crate::models::task_model::Task;
    use std::sync::Arc;
    use tokio::sync::mpsc;

    /// Creates an in-memory MailStore with schema applied.
    async fn make_store() -> MailStore {
        let (tx, _rx) = mpsc::unbounded_channel();
        let delta = Arc::new(DeltaStream::new(tx));
        let tmp = tempfile::tempdir().unwrap();
        let store = MailStore::open_with_delta(tmp.path().to_str().unwrap(), delta)
            .await
            .unwrap();
        store.migrate().await.unwrap();
        store
    }

    fn make_task(id: &str, status: &str) -> Task {
        Task {
            id: id.to_string(),
            account_id: "acc1".to_string(),
            version: 0,
            class_name: "SendDraftTask".to_string(),
            status: status.to_string(),
            should_cancel: None,
            error: None,
        }
    }

    /// Inserts a task with a specific completed_at timestamp directly via SQL.
    async fn insert_task_with_completed_at(store: &MailStore, id: &str, status: &str, completed_at: &str) {
        let id = id.to_string();
        let status = status.to_string();
        let completed_at = completed_at.to_string();

        store.writer.call(move |conn| -> Result<(), rusqlite::Error> {
            let data = serde_json::json!({
                "id": id,
                "aid": "acc1",
                "v": 1,
                "__cls": "SendDraftTask",
                "status": status,
                "completed_at": completed_at,
            });
            conn.execute(
                "INSERT OR REPLACE INTO Task (id, data, accountId, version, status) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![id, serde_json::to_string(&data).unwrap(), "acc1", 1i64, status],
            )?;
            Ok(())
        }).await.unwrap();
    }

    // ---- reset_stuck_tasks tests ----

    #[tokio::test]
    async fn reset_stuck_tasks_resets_remote_to_local() {
        let store = make_store().await;

        let mut t_remote = make_task("rt-1", "remote");
        store.save(&mut t_remote).await.unwrap();

        let count = reset_stuck_tasks(&store).await.unwrap();
        assert_eq!(count, 1, "Should reset 1 remote task");

        // Verify status via raw query
        let status: String = store.writer.call(|conn| {
            conn.query_row(
                "SELECT status FROM Task WHERE id = ?1",
                ["rt-1"],
                |row| row.get(0),
            )
        }).await.unwrap();
        assert_eq!(status, "local", "Status should now be 'local'");
    }

    #[tokio::test]
    async fn reset_stuck_tasks_does_not_touch_other_statuses() {
        let store = make_store().await;

        let mut t_local = make_task("st-1", "local");
        let mut t_complete = make_task("st-2", "complete");
        let mut t_cancelled = make_task("st-3", "cancelled");
        store.save(&mut t_local).await.unwrap();
        store.save(&mut t_complete).await.unwrap();
        store.save(&mut t_cancelled).await.unwrap();

        let count = reset_stuck_tasks(&store).await.unwrap();
        assert_eq!(count, 0, "Should not reset any non-remote tasks");

        // Verify all statuses unchanged
        let local_status: String = store.writer.call(|conn| {
            conn.query_row("SELECT status FROM Task WHERE id = ?1", ["st-1"], |row| row.get(0))
        }).await.unwrap();
        assert_eq!(local_status, "local");

        let complete_status: String = store.writer.call(|conn| {
            conn.query_row("SELECT status FROM Task WHERE id = ?1", ["st-2"], |row| row.get(0))
        }).await.unwrap();
        assert_eq!(complete_status, "complete");

        let cancelled_status: String = store.writer.call(|conn| {
            conn.query_row("SELECT status FROM Task WHERE id = ?1", ["st-3"], |row| row.get(0))
        }).await.unwrap();
        assert_eq!(cancelled_status, "cancelled");
    }

    #[tokio::test]
    async fn reset_stuck_tasks_returns_count_of_reset_tasks() {
        let store = make_store().await;

        let mut t1 = make_task("rc-1", "remote");
        let mut t2 = make_task("rc-2", "remote");
        let mut t3 = make_task("rc-3", "local");
        store.save(&mut t1).await.unwrap();
        store.save(&mut t2).await.unwrap();
        store.save(&mut t3).await.unwrap();

        let count = reset_stuck_tasks(&store).await.unwrap();
        assert_eq!(count, 2, "Should reset exactly 2 remote tasks");
    }

    // ---- expire_completed_tasks tests ----

    #[tokio::test]
    async fn expire_completed_tasks_deletes_old_completed_tasks() {
        let store = make_store().await;

        // Insert a task completed 20 minutes ago (older than 15-min retention)
        insert_task_with_completed_at(
            &store,
            "exp-1",
            "complete",
            "2000-01-01 00:00:00", // Far in the past
        ).await;

        let count = expire_completed_tasks(&store, TASK_RETENTION_SECS).await.unwrap();
        assert_eq!(count, 1, "Should delete 1 expired completed task");

        // Verify it's gone
        let remaining: i64 = store.writer.call(|conn| {
            conn.query_row("SELECT COUNT(*) FROM Task WHERE id = ?1", ["exp-1"], |row| row.get(0))
        }).await.unwrap();
        assert_eq!(remaining, 0, "Expired task should be deleted");
    }

    #[tokio::test]
    async fn expire_completed_tasks_does_not_delete_recent_completed_tasks() {
        let store = make_store().await;

        // Insert a task completed just now (within retention window)
        let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
        insert_task_with_completed_at(&store, "exp-2", "complete", &now).await;

        let count = expire_completed_tasks(&store, TASK_RETENTION_SECS).await.unwrap();
        assert_eq!(count, 0, "Should not delete recently completed task");

        let remaining: i64 = store.writer.call(|conn| {
            conn.query_row("SELECT COUNT(*) FROM Task WHERE id = ?1", ["exp-2"], |row| row.get(0))
        }).await.unwrap();
        assert_eq!(remaining, 1, "Recent task should still exist");
    }

    #[tokio::test]
    async fn expire_completed_tasks_does_not_delete_non_complete_tasks() {
        let store = make_store().await;

        // Insert old tasks with non-complete status
        insert_task_with_completed_at(&store, "exp-3", "local", "2000-01-01 00:00:00").await;
        insert_task_with_completed_at(&store, "exp-4", "remote", "2000-01-01 00:00:00").await;

        let count = expire_completed_tasks(&store, TASK_RETENTION_SECS).await.unwrap();
        assert_eq!(count, 0, "Should not delete non-complete tasks even if old");

        let remaining: i64 = store.writer.call(|conn| {
            conn.query_row("SELECT COUNT(*) FROM Task WHERE id IN ('exp-3', 'exp-4')", [], |row| row.get(0))
        }).await.unwrap();
        assert_eq!(remaining, 2, "Non-complete tasks should survive expiry");
    }

    #[tokio::test]
    async fn expire_completed_tasks_ignores_tasks_without_completed_at() {
        let store = make_store().await;

        // Task with no completed_at field (normal case for recently-completed tasks
        // where completed_at may not be set by the stub)
        let mut task = make_task("exp-5", "complete");
        store.save(&mut task).await.unwrap();

        let count = expire_completed_tasks(&store, TASK_RETENTION_SECS).await.unwrap();
        assert_eq!(count, 0, "Tasks without completed_at should not be expired");
    }
}
