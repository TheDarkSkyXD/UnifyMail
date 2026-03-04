// MailStoreTransaction — RAII wrapper for SQL transactions with delta accumulation.
//
// Mirrors C++ MailStoreTransaction: a wrapper that gates delta emission.
// Saves within a transaction accumulate deltas in a Vec instead of emitting them.
// On commit(), the SQL transaction is committed AND all accumulated deltas are emitted.
// On rollback(), the SQL transaction is rolled back AND the deltas are discarded.
//
// RAII safety: if a MailStoreTransaction is dropped without commit() or rollback(),
// it logs a warning. The SQL ROLLBACK should be called explicitly, but the delta vec
// is cleared at minimum to prevent any partial emission on a future transaction.

use std::sync::Arc;
use tokio::sync::Mutex;
use crate::delta::item::DeltaStreamItem;
use crate::delta::stream::DeltaStream;

/// RAII transaction handle.
///
/// Obtain via `MailStore::begin_transaction()`.
/// Use `commit()` to apply changes and emit deltas.
/// Use `rollback()` to discard changes and deltas.
pub struct MailStoreTransaction {
    /// The shared delta accumulator — same Arc as MailStore::transaction_deltas.
    /// When Some(vec), save/remove push deltas here.
    transaction_deltas: Arc<Mutex<Option<Vec<DeltaStreamItem>>>>,
    /// Delta stream for emitting on commit.
    delta_stream: Option<Arc<DeltaStream>>,
    /// Whether commit() or rollback() has been called.
    /// Prevents the Drop impl from attempting a second cleanup.
    committed: bool,
}

impl MailStoreTransaction {
    /// Creates a new transaction handle.
    /// Called only by MailStore::begin_transaction().
    pub(crate) fn new(
        transaction_deltas: Arc<Mutex<Option<Vec<DeltaStreamItem>>>>,
        delta_stream: Option<Arc<DeltaStream>>,
    ) -> Self {
        Self {
            transaction_deltas,
            delta_stream,
            committed: false,
        }
    }

    /// Commits the SQL transaction and emits all accumulated deltas.
    ///
    /// Steps:
    /// 1. Execute COMMIT on the writer connection
    /// 2. Take the accumulated deltas (swap with None to clear transaction mode)
    /// 3. Emit each delta to the delta channel
    pub async fn commit(mut self, store: &crate::store::mail_store::MailStore) -> Result<(), crate::error::SyncError> {
        // Execute SQL COMMIT
        store.execute_commit().await?;

        // Take and emit accumulated deltas
        let deltas = {
            let mut guard = self.transaction_deltas.lock().await;
            guard.take().unwrap_or_default()
        };

        if let Some(ref stream) = self.delta_stream {
            for delta in deltas {
                stream.emit(delta);
            }
        }

        self.committed = true;
        Ok(())
    }

    /// Rolls back the SQL transaction and discards all accumulated deltas.
    ///
    /// Steps:
    /// 1. Execute ROLLBACK on the writer connection
    /// 2. Clear the accumulated deltas (swap with None — deltas are discarded)
    pub async fn rollback(mut self, store: &crate::store::mail_store::MailStore) -> Result<(), crate::error::SyncError> {
        // Execute SQL ROLLBACK
        store.execute_rollback().await?;

        // Discard accumulated deltas
        {
            let mut guard = self.transaction_deltas.lock().await;
            *guard = None;
        }

        self.committed = true;
        Ok(())
    }
}

impl Drop for MailStoreTransaction {
    /// RAII safety net: if dropped without commit/rollback, log a warning and
    /// clear the delta accumulator to prevent partial emission.
    ///
    /// Note: Drop cannot be async. We cannot issue a SQL ROLLBACK here.
    /// The caller must explicitly call rollback() or commit() before dropping.
    fn drop(&mut self) {
        if !self.committed {
            tracing::warn!(
                "MailStoreTransaction dropped without commit or rollback — \
                 delta accumulator cleared; SQL transaction may need manual rollback"
            );
            // Clear delta accumulator synchronously using try_lock.
            // In normal operation this should always succeed since no other
            // code holds the mutex when the transaction is being dropped.
            if let Ok(mut guard) = self.transaction_deltas.try_lock() {
                *guard = None;
            }
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::mpsc;
    use crate::delta::stream::DeltaStream;
    use crate::models::Folder;
    use crate::store::mail_store::{MailStore, SqlParam};

    async fn setup_test_store() -> (MailStore, mpsc::UnboundedReceiver<DeltaStreamItem>) {
        let (tx, rx) = mpsc::unbounded_channel::<DeltaStreamItem>();
        let delta_stream = Arc::new(DeltaStream::new(tx));

        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().to_str().unwrap().to_string();

        let store = MailStore::open_with_delta(&config_dir, delta_stream)
            .await
            .expect("open_with_delta failed");

        store.migrate().await.expect("migrate failed");

        // Keep dir alive for the test
        std::mem::forget(dir);

        (store, rx)
    }

    fn sample_folder(id: &str, account_id: &str) -> Folder {
        Folder {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            path: "INBOX".to_string(),
            role: "inbox".to_string(),
            local_status: None,
        }
    }

    // -----------------------------------------------------------------------
    // Transaction tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_transaction_save_does_not_emit_until_commit() {
        let (store, mut rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");

        let txn = store.begin_transaction().await.expect("begin_transaction failed");
        store.save(&mut folder).await.expect("save failed");

        // No delta should be emitted yet
        assert!(
            rx.try_recv().is_err(),
            "delta should NOT be emitted before commit"
        );

        txn.commit(&store).await.expect("commit failed");

        // Now the delta should be emitted
        let delta = rx.try_recv().expect("delta should be emitted after commit");
        assert_eq!(delta.delta_type, "persist");
        assert_eq!(delta.model_class, "Folder");
    }

    #[tokio::test]
    async fn test_transaction_commit_emits_all_accumulated_deltas() {
        let (store, mut rx) = setup_test_store().await;
        let mut f1 = sample_folder("f1", "acc1");
        let mut f2 = sample_folder("f2", "acc1");
        let mut f3 = sample_folder("f3", "acc1");

        let txn = store.begin_transaction().await.expect("begin_transaction failed");
        store.save(&mut f1).await.expect("save f1 failed");
        store.save(&mut f2).await.expect("save f2 failed");
        store.save(&mut f3).await.expect("save f3 failed");

        // Nothing emitted yet
        assert!(rx.try_recv().is_err(), "no deltas before commit");

        txn.commit(&store).await.expect("commit failed");

        // All 3 deltas emitted
        let d1 = rx.try_recv().expect("delta 1");
        let d2 = rx.try_recv().expect("delta 2");
        let d3 = rx.try_recv().expect("delta 3");
        assert_eq!(d1.delta_type, "persist");
        assert_eq!(d2.delta_type, "persist");
        assert_eq!(d3.delta_type, "persist");
        assert!(rx.try_recv().is_err(), "no more deltas");
    }

    #[tokio::test]
    async fn test_transaction_rollback_discards_all_deltas() {
        let (store, mut rx) = setup_test_store().await;
        let mut f1 = sample_folder("f1", "acc1");
        let mut f2 = sample_folder("f2", "acc1");

        let txn = store.begin_transaction().await.expect("begin_transaction failed");
        store.save(&mut f1).await.expect("save f1 failed");
        store.save(&mut f2).await.expect("save f2 failed");

        txn.rollback(&store).await.expect("rollback failed");

        // No deltas emitted
        assert!(rx.try_recv().is_err(), "no deltas after rollback");
    }

    #[tokio::test]
    async fn test_transaction_rollback_discards_sql_changes() {
        let (store, _rx) = setup_test_store().await;
        let mut f1 = sample_folder("f1", "acc1");

        let txn = store.begin_transaction().await.expect("begin_transaction failed");
        store.save(&mut f1).await.expect("save f1 failed");

        txn.rollback(&store).await.expect("rollback failed");

        // Row should NOT exist after rollback
        let count = store
            .count::<Folder>("id = ?1", vec![SqlParam::Text("f1".to_string())])
            .await
            .expect("count failed");
        assert_eq!(count, 0, "row should not exist after rollback");
    }

    #[tokio::test]
    async fn test_transaction_commit_persists_sql_changes() {
        let (store, _rx) = setup_test_store().await;
        let mut f1 = sample_folder("f1", "acc1");

        let txn = store.begin_transaction().await.expect("begin_transaction failed");
        store.save(&mut f1).await.expect("save f1 failed");
        txn.commit(&store).await.expect("commit failed");

        // Row should exist after commit
        let count = store
            .count::<Folder>("id = ?1", vec![SqlParam::Text("f1".to_string())])
            .await
            .expect("count failed");
        assert_eq!(count, 1, "row should exist after commit");
    }

    #[tokio::test]
    async fn test_transaction_multiple_model_types_in_one_transaction() {
        use crate::models::{Message, Thread};

        let (store, mut rx) = setup_test_store().await;

        let mut folder = Folder {
            id: "f1".to_string(),
            account_id: "acc1".to_string(),
            version: 0,
            path: "INBOX".to_string(),
            role: "inbox".to_string(),
            local_status: None,
        };
        let mut thread = Thread {
            id: "t:thr1".to_string(),
            account_id: "acc1".to_string(),
            version: 0,
            subject: "Hello".to_string(),
            last_message_timestamp: 0,
            first_message_timestamp: 0,
            last_message_sent_timestamp: 0,
            last_message_received_timestamp: 0,
            g_thr_id: None,
            unread: 0,
            starred: 0,
            in_all_mail: false,
            attachment_count: 0,
            search_row_id: None,
            folders: vec![],
            labels: vec![],
            participants: vec![],
            metadata: None,
        };
        let mut msg = Message {
            id: "msg1".to_string(),
            account_id: "acc1".to_string(),
            version: 0,
            synced_at: None,
            sync_unsaved_changes: None,
            remote_uid: 1,
            date: 1700000000,
            subject: "Test".to_string(),
            header_message_id: "<msg1@test.com>".to_string(),
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
            thread_id: "t:thr1".to_string(),
            snippet: None,
            plaintext: None,
            files: vec![],
            metadata: None,
        };

        let txn = store.begin_transaction().await.expect("begin_transaction failed");
        store.save(&mut folder).await.expect("save folder");
        store.save(&mut thread).await.expect("save thread");
        store.save(&mut msg).await.expect("save msg");

        // No deltas before commit
        assert!(rx.try_recv().is_err(), "no deltas before commit");

        txn.commit(&store).await.expect("commit failed");

        // 3 deltas emitted after commit
        let d1 = rx.try_recv().expect("delta 1");
        let d2 = rx.try_recv().expect("delta 2");
        let d3 = rx.try_recv().expect("delta 3");
        assert!(rx.try_recv().is_err(), "no more than 3 deltas");

        let classes: Vec<&str> = [&d1, &d2, &d3].iter().map(|d| d.model_class.as_str()).collect();
        assert!(classes.contains(&"Folder"), "Folder delta emitted");
        assert!(classes.contains(&"Thread"), "Thread delta emitted");
        assert!(classes.contains(&"Message"), "Message delta emitted");
    }

    #[tokio::test]
    async fn test_transaction_drop_without_commit_clears_delta_vec() {
        let (store, mut rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");

        {
            let txn = store.begin_transaction().await.expect("begin_transaction failed");
            store.save(&mut folder).await.expect("save failed");
            // Drop txn here without commit/rollback — triggers RAII cleanup
            drop(txn);
        }

        // No deltas should be emitted
        assert!(rx.try_recv().is_err(), "no deltas after implicit rollback");

        // Verify delta accumulator is cleared for next transaction
        let guard = store.transaction_deltas.lock().await;
        assert!(guard.is_none(), "transaction_deltas should be None after drop");
    }
}
