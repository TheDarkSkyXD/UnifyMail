// store/task_store.rs — Task-specific database helpers.
//
// Provides targeted SQL helpers for task processing that don't fit the generic
// MailModel CRUD API in MailStore:
//   - save_task_status: UPDATE tasks SET status = ? WHERE id = ?
//   - find_local_tasks:  SELECT tasks WHERE status = 'local' AND accountId = ?
//
// These are used by the task execution loop (Plan 03) and the recovery module.

#![allow(dead_code)]

use crate::error::SyncError;
use crate::models::task_model::Task;
use crate::store::mail_store::{MailStore, SqlParam};

// ============================================================================
// save_task_status
// ============================================================================

/// Updates the `status` column for a given task ID.
///
/// Also updates the `data` JSON blob's `status` field so the two stay in sync.
/// This is the targeted update helper used after local/remote phase transitions
/// and by recovery.rs for bulk status resets.
pub async fn save_task_status(
    store: &MailStore,
    task_id: &str,
    status: &str,
) -> Result<(), SyncError> {
    let task_id = task_id.to_string();
    let status = status.to_string();

    store
        .writer
        .call(move |conn| -> Result<(), rusqlite::Error> {
            // Update both the column and the data JSON blob to keep them in sync.
            conn.execute(
                "UPDATE Task SET status = ?1, data = json_set(data, '$.status', ?2) WHERE id = ?3",
                rusqlite::params![status, status, task_id],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| SyncError::Database(e.to_string()))
}

// ============================================================================
// find_local_tasks
// ============================================================================

/// Returns all tasks with status = 'local' for the given account, ordered by rowid ASC.
///
/// Rowid ordering ensures tasks are processed in insertion order (FIFO).
/// Used by the IDLE loop in Plan 03 to drain the task queue on each cycle.
pub async fn find_local_tasks(
    store: &MailStore,
    account_id: &str,
) -> Result<Vec<Task>, SyncError> {
    let account_id = account_id.to_string();

    store
        .writer
        .call(move |conn| -> Result<Vec<Task>, rusqlite::Error> {
            let mut stmt = conn.prepare(
                "SELECT id, data, accountId, version, status \
                 FROM Task \
                 WHERE status = 'local' AND accountId = ?1 \
                 ORDER BY rowid ASC",
            )?;

            let tasks = stmt
                .query_map([&account_id as &str], |row| {
                    let data_json: String = row.get(1)?;
                    // Parse from the data JSON blob (canonical source of truth)
                    let mut task: Task = serde_json::from_str(&data_json).map_err(|e| {
                        rusqlite::Error::FromSqlConversionFailure(
                            1,
                            rusqlite::types::Type::Text,
                            Box::new(e),
                        )
                    })?;
                    // Ensure the status column value is used (may differ from data blob
                    // in edge cases where they get out of sync)
                    let status: String = row.get(4)?;
                    task.status = status;
                    Ok(task)
                })?
                .collect::<Result<Vec<_>, _>>()?;

            Ok(tasks)
        })
        .await
        .map_err(|e| SyncError::Database(e.to_string()))
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

    fn sample_task(id: &str, status: &str) -> Task {
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

    #[tokio::test]
    async fn save_task_status_updates_status_column() {
        let store = make_store().await;

        // Insert a task via normal save
        let mut task = sample_task("task-001", "local");
        store.save(&mut task).await.unwrap();

        // Update status
        save_task_status(&store, "task-001", "remote").await.unwrap();

        // Verify by reading back
        let tasks = find_local_tasks(&store, "acc1").await.unwrap();
        // Should be empty since we moved to "remote"
        assert!(
            tasks.iter().all(|t| t.id != "task-001"),
            "task-001 should no longer be in local tasks after status update to remote"
        );
    }

    #[tokio::test]
    async fn find_local_tasks_returns_local_tasks_ordered() {
        let store = make_store().await;

        // Insert tasks in order
        let mut t1 = sample_task("task-100", "local");
        let mut t2 = sample_task("task-101", "local");
        let mut t3 = sample_task("task-102", "remote"); // should be excluded
        let mut t4 = sample_task("task-103", "complete"); // should be excluded

        store.save(&mut t1).await.unwrap();
        store.save(&mut t2).await.unwrap();
        store.save(&mut t3).await.unwrap();
        store.save(&mut t4).await.unwrap();

        let local = find_local_tasks(&store, "acc1").await.unwrap();
        assert_eq!(local.len(), 2, "Should return only 2 local tasks");
        assert_eq!(local[0].id, "task-100");
        assert_eq!(local[1].id, "task-101");
    }

    #[tokio::test]
    async fn find_local_tasks_filters_by_account_id() {
        let store = make_store().await;

        // acc1 task
        let mut t1 = sample_task("task-200", "local");
        // acc2 task
        let mut t2 = Task {
            id: "task-201".to_string(),
            account_id: "acc2".to_string(),
            version: 0,
            class_name: "ChangeStarredTask".to_string(),
            status: "local".to_string(),
            should_cancel: None,
            error: None,
        };

        store.save(&mut t1).await.unwrap();
        store.save(&mut t2).await.unwrap();

        let acc1_tasks = find_local_tasks(&store, "acc1").await.unwrap();
        let acc2_tasks = find_local_tasks(&store, "acc2").await.unwrap();

        assert_eq!(acc1_tasks.len(), 1);
        assert_eq!(acc1_tasks[0].id, "task-200");
        assert_eq!(acc2_tasks.len(), 1);
        assert_eq!(acc2_tasks[0].id, "task-201");
    }
}
