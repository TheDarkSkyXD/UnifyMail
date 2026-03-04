// MailStore — SQLite database access for mailsync-rs.
//
// Wraps tokio-rusqlite for async access to the email database.
// The database lives at {CONFIG_DIR_PATH}/edgehill.db — same path as the C++ binary.
//
// Key properties matching C++ MailStore.cpp:
// - WAL mode for concurrent reads during sync
// - page_size=4096, cache_size=10000, synchronous=NORMAL (matching MailStore.cpp lines 96-100)
// - busy_timeout=5000ms (prevents "database is locked" errors during concurrent access)
// - Schema version tracked via PRAGMA user_version
// - Migrations are idempotent — CREATE TABLE IF NOT EXISTS and version guards
//
// In sync mode, two connections are used (writer + reader) for WAL concurrency.
// Writes go through writer; reads (find, find_all, count) go through reader.
// This matches the C++ MailStore split-connection design (DATA-01).

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_rusqlite::Connection;
use crate::delta::item::DeltaStreamItem;
use crate::delta::stream::DeltaStream;
use crate::error::SyncError;
use crate::models::mail_model::MailModel;
use crate::store::migrations::{
    ACCOUNT_RESET_QUERIES, V1_SETUP, V2_SETUP, V3_SETUP, V4_SETUP, V6_SETUP, V7_SETUP,
    V8_SETUP, V9_SETUP,
};

// ============================================================================
// SqlParam — typed parameter for queries, safe to move into tokio-rusqlite closures
// ============================================================================

/// Typed SQL query parameter.
///
/// tokio-rusqlite closures must be `Send + 'static`, so we cannot pass
/// `&dyn ToSql` references. Instead, callers pass owned `SqlParam` values.
#[derive(Clone, Debug)]
pub enum SqlParam {
    /// Text / VARCHAR value
    Text(String),
    /// Integer value
    Int(i64),
    /// Real / floating-point value
    Real(f64),
    /// SQL NULL
    Null,
}

impl rusqlite::types::ToSql for SqlParam {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, Value};
        match self {
            SqlParam::Text(s) => Ok(ToSqlOutput::Owned(Value::Text(s.clone()))),
            SqlParam::Int(i) => Ok(ToSqlOutput::Owned(Value::Integer(*i))),
            SqlParam::Real(f) => Ok(ToSqlOutput::Owned(Value::Real(*f))),
            SqlParam::Null => Ok(ToSqlOutput::Owned(Value::Null)),
        }
    }
}

// ============================================================================
// MailStore
// ============================================================================

/// The email database wrapper.
/// In sync mode, two connections are used (writer + reader) for WAL concurrency.
/// In offline modes (migrate, reset), a single connection is sufficient.
pub struct MailStore {
    /// Primary connection — all writes go through this connection
    pub(crate) writer: Connection,
    /// Reader connection — queries go through this connection (WAL concurrency)
    /// None in offline modes (migrate, reset) where concurrent reads are not needed.
    reader: Option<Connection>,
    /// Delta emitter — sends change notifications to the flush task.
    /// None in offline modes where delta emission is not needed.
    delta_tx: Option<Arc<DeltaStream>>,
    /// Transaction delta accumulator.
    /// When Some(vec), save/remove push deltas here instead of emitting directly.
    /// On commit, the vec is drained and all deltas are emitted.
    /// On rollback, the vec is dropped without emitting.
    pub(crate) transaction_deltas: Arc<Mutex<Option<Vec<DeltaStreamItem>>>>,
    /// Global labels version counter — incremented when any Label is saved.
    /// Phase 7 (IMAP sync) reads this to invalidate label caches after sync.
    pub labels_version: Arc<AtomicU64>,
}

impl MailStore {
    // ========================================================================
    // Constructors
    // ========================================================================

    /// Opens the database at `{config_dir}/edgehill.db` for offline use.
    /// Creates the file if it doesn't exist.
    /// Applies WAL mode and PRAGMA settings matching C++ MailStore constructor.
    /// No reader connection or delta stream — suitable for migrate/reset modes.
    pub async fn open(config_dir: &str) -> Result<Self, SyncError> {
        let db_path = format!("{}/edgehill.db", config_dir);
        let writer = Self::open_connection(&db_path).await?;
        Ok(Self {
            writer,
            reader: None,
            delta_tx: None,
            transaction_deltas: Arc::new(Mutex::new(None)),
            labels_version: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Opens the database with a separate reader connection and delta stream.
    ///
    /// This is the sync mode constructor. Two connections to the same database
    /// file are opened: one for writes (writer) and one for reads (reader).
    /// WAL mode allows concurrent reads while the writer is active.
    ///
    /// The `delta_stream` is stored and used by save/remove to emit change notifications.
    pub async fn open_with_delta(
        config_dir: &str,
        delta_stream: Arc<DeltaStream>,
    ) -> Result<Self, SyncError> {
        let db_path = format!("{}/edgehill.db", config_dir);
        let writer = Self::open_connection(&db_path).await?;
        let reader = Self::open_connection(&db_path).await?;
        Ok(Self {
            writer,
            reader: Some(reader),
            delta_tx: Some(delta_stream),
            transaction_deltas: Arc::new(Mutex::new(None)),
            labels_version: Arc::new(AtomicU64::new(0)),
        })
    }

    /// Opens a single SQLite connection with standard PRAGMA settings.
    ///
    /// Shared logic for both writer and reader connections.
    /// busy_timeout=5000ms matches C++ MailStore behavior (DATA-01).
    async fn open_connection(db_path: &str) -> Result<Connection, SyncError> {
        let conn = Connection::open(db_path)
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        // Apply PRAGMAs matching C++ MailStore.cpp lines 96-100.
        // busy_timeout=5000ms per DATA-01 spec (C++ uses 5s, not 10s).
        conn.call(|c| -> Result<(), rusqlite::Error> {
            c.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA main.page_size = 4096;
                 PRAGMA main.cache_size = 10000;
                 PRAGMA main.synchronous = NORMAL;",
            )?;
            c.busy_timeout(std::time::Duration::from_millis(5000))?;
            Ok(())
        })
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(conn)
    }

    /// Returns the current labels version counter value.
    /// Used by Phase 7 IMAP sync to detect when label caches need invalidation.
    pub fn labels_version(&self) -> u64 {
        self.labels_version.load(Ordering::Relaxed)
    }

    // ========================================================================
    // Schema migrations
    // ========================================================================

    /// Runs schema migrations from the current version up to version 9.
    /// Idempotent — safe to call multiple times.
    /// For V3 (MessageBody.fetchedAt), prints "Running Migration" to stdout
    /// to match C++ behavior (Electron shows a migration progress window).
    pub async fn migrate(&self) -> Result<(), SyncError> {
        self.writer
            .call(|conn| -> Result<(), rusqlite::Error> {
                // Read current schema version
                let version: i32 =
                    conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;

                // Apply each migration version only if below the target version
                if version < 1 {
                    for sql in V1_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                if version < 2 {
                    for sql in V2_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                if version < 3 {
                    // V3 is time-consuming for large databases — signal progress to Electron.
                    // The Electron bridge shows a "Running Migration" dialog when it sees this text.
                    print!("\nRunning Migration");
                    std::io::stdout().flush().ok();
                    for sql in V3_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                if version < 4 {
                    for sql in V4_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                // NOTE: V5 does not exist in C++ — version numbers skip from 4 to 6

                if version < 6 {
                    for sql in V6_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                if version < 7 {
                    for sql in V7_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                if version < 8 {
                    for sql in V8_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                if version < 9 {
                    for sql in V9_SETUP {
                        conn.execute_batch(sql)?;
                    }
                }

                // Set user_version to 9 if any migrations were applied
                if version < 9 {
                    conn.execute_batch("PRAGMA user_version = 9")?;
                }

                Ok(())
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))
    }

    /// Deletes all data for the specified account from all tables.
    /// Data for other accounts is preserved.
    ///
    /// After deletes: cleans _State entries and VACUUMs to reclaim space.
    pub async fn reset_for_account(&self, account_id: &str) -> Result<(), SyncError> {
        let account_id = account_id.to_string();

        self.writer
            .call(move |conn| -> Result<(), rusqlite::Error> {
                // Execute all account-specific DELETE statements in dependency order
                for sql in ACCOUNT_RESET_QUERIES {
                    conn.execute(sql, [&account_id as &str])?;
                }

                // Clean up _State entries for this account (cursor position, etc.)
                // C++ resets cursor state via a LIKE pattern match on account ID
                let state_pattern = format!("%{}%", account_id);
                conn.execute(
                    "DELETE FROM `_State` WHERE id LIKE ?",
                    [&state_pattern as &str],
                )?;

                // VACUUM to reclaim disk space after large deletes
                conn.execute_batch("VACUUM")?;

                Ok(())
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))
    }

    // ========================================================================
    // Generic CRUD operations
    // ========================================================================

    /// Persists a model to its table. Increments version before saving.
    ///
    /// If version == 1 after increment (was 0): INSERT (new model).
    /// If version > 1 after increment: UPDATE (existing model).
    ///
    /// After a successful write, runs lifecycle hooks:
    /// 1. If T::supports_metadata(): DELETE + re-INSERT ModelPluginMetadata rows
    /// 2. model.after_save(db) — model-specific side effects (ThreadCategory, FTS5, etc.)
    ///
    /// All hooks run INSIDE the writer.call() closure for atomicity.
    ///
    /// After a successful write+hooks, emits a "persist" delta.
    /// If a transaction is active, the delta is accumulated instead of emitted.
    ///
    /// Uses prepare_cached for repeated saves of the same model type.
    pub async fn save<T: MailModel>(&self, model: &mut T) -> Result<(), SyncError> {
        model.increment_version();
        let version = model.version();
        let table = T::table_name();
        let columns = T::columns_for_query();
        let model_json = model.to_json();
        let data_json = serde_json::to_string(&model_json)
            .map_err(|e| SyncError::Json(e.to_string()))?;

        // Build SQL based on INSERT vs UPDATE
        // version == 1 means newly created (was 0 before increment_version)
        let sql = if version == 1 {
            // INSERT: list all columns and bind placeholders
            let col_list = columns.join(", ");
            let placeholders: Vec<String> = (1..=columns.len())
                .map(|i| format!("?{}", i))
                .collect();
            let placeholder_list = placeholders.join(", ");
            format!(
                "INSERT INTO `{}` ({}) VALUES ({})",
                table, col_list, placeholder_list
            )
        } else {
            // UPDATE: set each non-id column, identify by id
            // columns[0] is "id" — skip it in the SET clause
            let set_clauses: Vec<String> = columns
                .iter()
                .enumerate()
                .skip(1) // skip "id"
                .map(|(i, col)| format!("`{}` = ?{}", col, i + 1))
                .collect();
            let set_list = set_clauses.join(", ");
            format!(
                "UPDATE `{}` SET {} WHERE id = ?1",
                table, set_list
            )
        };

        // Collect metadata entries for the closure (must be owned, Send + 'static)
        let supports_metadata = T::supports_metadata();
        let metadata_entries: Vec<(String, String, String, Option<i64>)> = if supports_metadata {
            // Extract from model JSON metadata array
            // Each entry: {"pluginId": "...", "expiration": ..., "value": "..."}
            model_json
                .get("metadata")
                .and_then(|m| m.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|entry| {
                            let plugin_id = entry.get("pluginId")?.as_str()?.to_string();
                            let value = entry
                                .get("value")
                                .and_then(|v| {
                                    if v.is_string() {
                                        v.as_str().map(|s| s.to_string())
                                    } else {
                                        Some(v.to_string())
                                    }
                                })
                                .unwrap_or_default();
                            let expiration = entry.get("expiration").and_then(|v| v.as_i64());
                            Some((plugin_id, value, table.to_string(), expiration))
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        // Clone model and data_json for the closure (must be Send + 'static)
        let data_json_clone = data_json.clone();
        let model_json_dispatch = model.to_json_dispatch();
        let model_clone = model.clone();
        let model_id = model.id().to_string();
        let account_id = model.account_id().to_string();

        self.writer
            .call(move |db| -> Result<(), rusqlite::Error> {
                // Primary INSERT or UPDATE
                let mut stmt = db.prepare_cached(&sql)?;
                model_clone.bind_to_statement(&mut stmt, &data_json_clone)?;

                // Metadata maintenance (ModelPluginMetadata join table)
                if supports_metadata {
                    // Delete all existing metadata rows for this model
                    db.execute(
                        "DELETE FROM ModelPluginMetadata WHERE id = ?1",
                        rusqlite::params![model_id],
                    )?;
                    // Re-insert all current metadata entries
                    for (plugin_id, _value, object_type, expiration) in &metadata_entries {
                        db.execute(
                            "INSERT INTO ModelPluginMetadata \
                             (id, accountId, objectType, value, expiration) \
                             VALUES (?1, ?2, ?3, ?4, ?5)",
                            rusqlite::params![
                                model_id,
                                account_id,
                                object_type,
                                plugin_id,
                                expiration,
                            ],
                        )?;
                    }
                }

                // Model-specific lifecycle hook
                model_clone.after_save(db)?;

                Ok(())
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        // Increment global labels version counter if this model type requires it
        if T::increments_labels_version() {
            self.labels_version.fetch_add(1, Ordering::Relaxed);
        }

        // Emit delta (or accumulate in transaction)
        let delta_item = DeltaStreamItem::new("persist", table, vec![model_json_dispatch]);
        self.emit_or_accumulate(delta_item).await;

        Ok(())
    }

    /// Deletes a model from its table.
    ///
    /// After a successful delete, runs lifecycle hooks:
    /// 1. If T::supports_metadata(): DELETE ModelPluginMetadata rows for this model
    /// 2. model.after_remove(db) — model-specific cleanup (ThreadCategory, FTS5, etc.)
    ///
    /// All hooks run INSIDE the writer.call() closure for atomicity.
    ///
    /// After a successful delete+hooks, emits an "unpersist" delta.
    /// If a transaction is active, the delta is accumulated instead of emitted.
    pub async fn remove<T: MailModel>(&self, model: &T) -> Result<(), SyncError> {
        let table = T::table_name();
        let id = model.id().to_string();
        let id_for_delta = id.clone();
        let supports_metadata = T::supports_metadata();
        let model_clone = model.clone();

        let sql = format!("DELETE FROM `{}` WHERE id = ?1", table);

        self.writer
            .call(move |db| -> Result<(), rusqlite::Error> {
                // Primary DELETE
                db.execute(&sql, rusqlite::params![id])?;

                // Metadata cleanup
                if supports_metadata {
                    db.execute(
                        "DELETE FROM ModelPluginMetadata WHERE id = ?1",
                        rusqlite::params![model_clone.id()],
                    )?;
                }

                // Model-specific lifecycle hook
                model_clone.after_remove(db)?;

                Ok(())
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        // Emit delta (or accumulate in transaction)
        let delta_item = DeltaStreamItem::new(
            "unpersist",
            table,
            vec![serde_json::json!({"id": id_for_delta})],
        );
        self.emit_or_accumulate(delta_item).await;

        Ok(())
    }

    /// Retrieves a single model matching the WHERE clause.
    ///
    /// Returns `Ok(None)` if no rows match.
    /// Reads from the reader connection for WAL concurrency.
    pub async fn find<T: MailModel>(
        &self,
        where_sql: &str,
        params: Vec<SqlParam>,
    ) -> Result<Option<T>, SyncError> {
        let table = T::table_name();
        let sql = format!("SELECT data FROM `{}` WHERE {} LIMIT 1", table, where_sql);

        let reader = self.reader.as_ref().unwrap_or(&self.writer);

        let result = reader
            .call(move |db| -> Result<Option<String>, rusqlite::Error> {
                let mut stmt = db.prepare_cached(&sql)?;
                let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
                let mut rows = stmt.query(params_refs.as_slice())?;
                if let Some(row) = rows.next()? {
                    let data: String = row.get(0)?;
                    Ok(Some(data))
                } else {
                    Ok(None)
                }
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        match result {
            None => Ok(None),
            Some(data_str) => {
                let model: T = serde_json::from_str(&data_str)
                    .map_err(|e| SyncError::Json(e.to_string()))?;
                Ok(Some(model))
            }
        }
    }

    /// Retrieves all models matching the WHERE clause.
    ///
    /// Returns an empty Vec if no rows match.
    /// Reads from the reader connection for WAL concurrency.
    pub async fn find_all<T: MailModel>(
        &self,
        where_sql: &str,
        params: Vec<SqlParam>,
    ) -> Result<Vec<T>, SyncError> {
        let table = T::table_name();
        let sql = format!("SELECT data FROM `{}` WHERE {}", table, where_sql);

        let reader = self.reader.as_ref().unwrap_or(&self.writer);

        let rows_data = reader
            .call(move |db| -> Result<Vec<String>, rusqlite::Error> {
                let mut stmt = db.prepare_cached(&sql)?;
                let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
                let mut rows = stmt.query(params_refs.as_slice())?;
                let mut result = Vec::new();
                while let Some(row) = rows.next()? {
                    let data: String = row.get(0)?;
                    result.push(data);
                }
                Ok(result)
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        let mut models = Vec::with_capacity(rows_data.len());
        for data_str in rows_data {
            let model: T = serde_json::from_str(&data_str)
                .map_err(|e| SyncError::Json(e.to_string()))?;
            models.push(model);
        }
        Ok(models)
    }

    /// Returns the count of rows matching the WHERE clause.
    ///
    /// Reads from the reader connection for WAL concurrency.
    pub async fn count<T: MailModel>(
        &self,
        where_sql: &str,
        params: Vec<SqlParam>,
    ) -> Result<i64, SyncError> {
        let table = T::table_name();
        let sql = format!("SELECT COUNT(*) FROM `{}` WHERE {}", table, where_sql);

        let reader = self.reader.as_ref().unwrap_or(&self.writer);

        let count = reader
            .call(move |db| -> Result<i64, rusqlite::Error> {
                let mut stmt = db.prepare_cached(&sql)?;
                let params_refs: Vec<&dyn rusqlite::types::ToSql> =
                    params.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
                let count: i64 = stmt.query_row(params_refs.as_slice(), |row| row.get(0))?;
                Ok(count)
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(count)
    }

    // ========================================================================
    // Transaction support
    // ========================================================================

    /// Starts a SQL transaction on the writer connection.
    ///
    /// Returns a `MailStoreTransaction` handle. While the transaction is active,
    /// save/remove calls accumulate deltas in the transaction instead of emitting.
    /// Call commit() to flush all accumulated deltas; rollback() to discard them.
    pub async fn begin_transaction(&self) -> Result<crate::store::transaction::MailStoreTransaction, SyncError> {
        // Acquire write lock immediately — matches C++ MailStoreTransaction behavior
        self.writer
            .call(|db| -> Result<(), rusqlite::Error> {
                db.execute_batch("BEGIN IMMEDIATE")?;
                Ok(())
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        // Activate delta accumulation
        {
            let mut guard = self.transaction_deltas.lock().await;
            *guard = Some(Vec::new());
        }

        Ok(crate::store::transaction::MailStoreTransaction::new(
            Arc::clone(&self.transaction_deltas),
            self.delta_tx.clone(),
        ))
    }

    // ========================================================================
    // Internal helpers
    // ========================================================================

    /// Emits a delta to the channel, or accumulates it in the active transaction.
    async fn emit_or_accumulate(&self, item: DeltaStreamItem) {
        let mut guard = self.transaction_deltas.lock().await;
        if let Some(ref mut vec) = *guard {
            // Transaction is active — accumulate
            vec.push(item);
        } else {
            // No transaction — emit directly
            if let Some(ref stream) = self.delta_tx {
                stream.emit(item);
            }
        }
    }

    /// Executes COMMIT on the writer connection.
    /// Called by MailStoreTransaction::commit().
    pub(crate) async fn execute_commit(&self) -> Result<(), SyncError> {
        self.writer
            .call(|db| -> Result<(), rusqlite::Error> {
                db.execute_batch("COMMIT")?;
                Ok(())
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))
    }

    /// Executes ROLLBACK on the writer connection.
    /// Called by MailStoreTransaction::rollback().
    pub(crate) async fn execute_rollback(&self) -> Result<(), SyncError> {
        self.writer
            .call(|db| -> Result<(), rusqlite::Error> {
                db.execute_batch("ROLLBACK")?;
                Ok(())
            })
            .await
            .map_err(|e| SyncError::Database(e.to_string()))
    }

    /// Closes the database connection.
    /// Consuming self ensures the connection cannot be used after closing.
    pub fn close(self) {
        drop(self.writer);
        drop(self.reader);
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Contact, ContactBook, ContactGroup, Event, Folder, Label, Message, Task, Thread, Calendar, File};
    use crate::store::migrations::V1_SETUP;
    use tokio::sync::mpsc;

    // -----------------------------------------------------------------------
    // Test helpers
    // -----------------------------------------------------------------------

    /// Creates an in-memory MailStore with migrations applied, plus a delta receiver.
    async fn setup_test_store() -> (MailStore, mpsc::UnboundedReceiver<DeltaStreamItem>) {
        let (tx, rx) = mpsc::unbounded_channel::<DeltaStreamItem>();
        let delta_stream = Arc::new(DeltaStream::new(tx));

        // Use a temporary file-based DB so we can open two connections (WAL)
        // In-memory databases don't support two separate connections in WAL mode.
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().to_str().unwrap().to_string();

        // We need to keep dir alive for the duration of the test.
        // We'll open the store and run migrations before returning.
        let store = MailStore::open_with_delta(&config_dir, delta_stream)
            .await
            .expect("open_with_delta failed");

        store.migrate().await.expect("migrate failed");

        // Leak the tempdir so it stays alive during the test.
        // This is acceptable in tests.
        std::mem::forget(dir);

        (store, rx)
    }

    fn sample_folder(id: &str, account_id: &str) -> Folder {
        Folder {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0, // will be incremented to 1 on first save
            path: "INBOX".to_string(),
            role: "inbox".to_string(),
            local_status: None,
        }
    }

    fn sample_message(id: &str, account_id: &str, thread_id: &str) -> Message {
        Message {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            synced_at: None,
            sync_unsaved_changes: None,
            remote_uid: 1,
            date: 1700000000,
            subject: "Test Subject".to_string(),
            header_message_id: format!("<{}@test.com>", id),
            g_msg_id: None,
            g_thr_id: None,
            reply_to_header_message_id: None,
            forwarded_header_message_id: None,
            unread: true,
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
            thread_id: thread_id.to_string(),
            snippet: None,
            plaintext: None,
            files: vec![],
            metadata: None,
        }
    }

    fn sample_thread(id: &str, account_id: &str) -> Thread {
        Thread {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            subject: "Test Thread".to_string(),
            last_message_timestamp: 1700001000,
            first_message_timestamp: 1700000000,
            last_message_sent_timestamp: 1700001000,
            last_message_received_timestamp: 1700000500,
            g_thr_id: None,
            unread: 1,
            starred: 0,
            in_all_mail: true,
            attachment_count: 0,
            search_row_id: None,
            folders: vec![],
            labels: vec![],
            participants: vec![],
            metadata: None,
        }
    }

    // -----------------------------------------------------------------------
    // save() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_save_persists_folder_and_find_retrieves_it() {
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("folder1", "acc1");

        store.save(&mut folder).await.expect("save failed");
        assert_eq!(folder.version, 1, "version should be 1 after first save");

        let found: Option<Folder> = store
            .find("id = ?1", vec![SqlParam::Text("folder1".to_string())])
            .await
            .expect("find failed");

        let found = found.expect("should find saved folder");
        assert_eq!(found.id, "folder1");
        assert_eq!(found.account_id, "acc1");
        assert_eq!(found.path, "INBOX");
        assert_eq!(found.role, "inbox");
    }

    #[tokio::test]
    async fn test_save_new_model_uses_insert_version_1() {
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");

        store.save(&mut folder).await.expect("save failed");
        assert_eq!(folder.version, 1);

        let count = store
            .count::<Folder>("id = ?1", vec![SqlParam::Text("f1".to_string())])
            .await
            .expect("count failed");
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_save_existing_model_uses_update_increments_version() {
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");

        // First save — INSERT
        store.save(&mut folder).await.expect("first save failed");
        assert_eq!(folder.version, 1);

        // Second save — UPDATE
        folder.path = "Sent".to_string();
        store.save(&mut folder).await.expect("second save failed");
        assert_eq!(folder.version, 2);

        // Should still be exactly one row
        let count = store
            .count::<Folder>("id = ?1", vec![SqlParam::Text("f1".to_string())])
            .await
            .expect("count failed");
        assert_eq!(count, 1);

        // The path should be updated
        let found: Folder = store
            .find("id = ?1", vec![SqlParam::Text("f1".to_string())])
            .await
            .expect("find failed")
            .expect("should find updated folder");
        assert_eq!(found.path, "Sent");
    }

    #[tokio::test]
    async fn test_save_emits_persist_delta() {
        let (store, mut rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");

        store.save(&mut folder).await.expect("save failed");

        let delta = rx.try_recv().expect("should have received delta");
        assert_eq!(delta.delta_type, "persist");
        assert_eq!(delta.model_class, "Folder");
        assert_eq!(delta.model_jsons.len(), 1);
        assert_eq!(delta.model_jsons[0]["id"], "f1");
    }

    #[tokio::test]
    async fn test_save_message_all_fields_intact() {
        let (store, _rx) = setup_test_store().await;
        let mut msg = sample_message("msg1", "acc1", "t:thread1");
        msg.subject = "Hello World".to_string();
        msg.unread = true;
        msg.starred = false;

        store.save(&mut msg).await.expect("save failed");

        let found: Message = store
            .find("id = ?1", vec![SqlParam::Text("msg1".to_string())])
            .await
            .expect("find failed")
            .expect("should find message");
        assert_eq!(found.id, "msg1");
        assert_eq!(found.subject, "Hello World");
        assert_eq!(found.unread, true);
        assert_eq!(found.thread_id, "t:thread1");
    }

    // -----------------------------------------------------------------------
    // remove() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_remove_deletes_model() {
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");
        store.save(&mut folder).await.expect("save failed");

        store.remove(&folder).await.expect("remove failed");

        let found: Option<Folder> = store
            .find("id = ?1", vec![SqlParam::Text("f1".to_string())])
            .await
            .expect("find failed");
        assert!(found.is_none(), "should be gone after remove");
    }

    #[tokio::test]
    async fn test_remove_emits_unpersist_delta() {
        let (store, mut rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");
        store.save(&mut folder).await.expect("save failed");

        // Drain the persist delta
        let _ = rx.try_recv();

        store.remove(&folder).await.expect("remove failed");

        let delta = rx.try_recv().expect("should have received unpersist delta");
        assert_eq!(delta.delta_type, "unpersist");
        assert_eq!(delta.model_class, "Folder");
        assert_eq!(delta.model_jsons[0]["id"], "f1");
    }

    // -----------------------------------------------------------------------
    // find() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_find_nonexistent_returns_none() {
        let (store, _rx) = setup_test_store().await;

        let found: Option<Folder> = store
            .find("id = ?1", vec![SqlParam::Text("nope".to_string())])
            .await
            .expect("find failed");
        assert!(found.is_none());
    }

    // -----------------------------------------------------------------------
    // find_all() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_find_all_returns_multiple_models() {
        let (store, _rx) = setup_test_store().await;
        let mut f1 = sample_folder("f1", "acc1");
        let mut f2 = sample_folder("f2", "acc1");
        let mut f3 = sample_folder("f3", "acc2"); // different account
        store.save(&mut f1).await.expect("save f1 failed");
        store.save(&mut f2).await.expect("save f2 failed");
        store.save(&mut f3).await.expect("save f3 failed");

        let acc1_folders: Vec<Folder> = store
            .find_all(
                "accountId = ?1",
                vec![SqlParam::Text("acc1".to_string())],
            )
            .await
            .expect("find_all failed");

        assert_eq!(acc1_folders.len(), 2);
        let ids: Vec<&str> = acc1_folders.iter().map(|f| f.id.as_str()).collect();
        assert!(ids.contains(&"f1"));
        assert!(ids.contains(&"f2"));
    }

    #[tokio::test]
    async fn test_find_all_empty_result_returns_empty_vec() {
        let (store, _rx) = setup_test_store().await;

        let results: Vec<Folder> = store
            .find_all("accountId = ?1", vec![SqlParam::Text("nobody".to_string())])
            .await
            .expect("find_all failed");
        assert!(results.is_empty());
    }

    // -----------------------------------------------------------------------
    // count() tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_count_returns_correct_number() {
        let (store, _rx) = setup_test_store().await;
        let mut f1 = sample_folder("f1", "acc1");
        let mut f2 = sample_folder("f2", "acc1");
        let mut f3 = sample_folder("f3", "acc2");
        store.save(&mut f1).await.expect("save failed");
        store.save(&mut f2).await.expect("save failed");
        store.save(&mut f3).await.expect("save failed");

        let count = store
            .count::<Folder>("accountId = ?1", vec![SqlParam::Text("acc1".to_string())])
            .await
            .expect("count failed");
        assert_eq!(count, 2);

        let total = store
            .count::<Folder>("1=1", vec![])
            .await
            .expect("count all failed");
        assert_eq!(total, 3);
    }

    // -----------------------------------------------------------------------
    // WAL concurrency test
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_open_with_delta_creates_writer_and_reader() {
        // Simply verify that open_with_delta succeeds and reader is present
        let (tx, _rx) = mpsc::unbounded_channel::<DeltaStreamItem>();
        let delta_stream = Arc::new(DeltaStream::new(tx));
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().to_str().unwrap().to_string();

        let store = MailStore::open_with_delta(&config_dir, delta_stream)
            .await
            .expect("open_with_delta should succeed");

        assert!(store.reader.is_some(), "reader connection should be present");
    }

    #[tokio::test]
    async fn test_wal_reader_can_read_while_writer_active() {
        // This test verifies WAL mode: reader sees committed data,
        // and busy_timeout prevents SQLITE_BUSY under concurrent access.
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("f1", "acc1");
        store.save(&mut folder).await.expect("save failed");

        // Simultaneous reads via reader connection should work
        let r1 = store.find::<Folder>("id = ?1", vec![SqlParam::Text("f1".to_string())]).await;
        let r2 = store.count::<Folder>("1=1", vec![]).await;

        assert!(r1.expect("r1 should succeed").is_some());
        assert_eq!(r2.expect("r2 should succeed"), 1);
    }

    // -----------------------------------------------------------------------
    // Multi-model type tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_save_and_find_multiple_model_types() {
        let (store, _rx) = setup_test_store().await;

        let mut folder = sample_folder("f1", "acc1");
        let mut thread = sample_thread("t:thr1", "acc1");
        let mut msg = sample_message("msg1", "acc1", "t:thr1");

        store.save(&mut folder).await.expect("save folder");
        store.save(&mut thread).await.expect("save thread");
        store.save(&mut msg).await.expect("save message");

        let found_folder: Folder = store
            .find("id = ?1", vec![SqlParam::Text("f1".to_string())])
            .await.expect("find folder").expect("folder should exist");
        let found_thread: Thread = store
            .find("id = ?1", vec![SqlParam::Text("t:thr1".to_string())])
            .await.expect("find thread").expect("thread should exist");
        let found_msg: Message = store
            .find("id = ?1", vec![SqlParam::Text("msg1".to_string())])
            .await.expect("find msg").expect("msg should exist");

        assert_eq!(found_folder.path, "INBOX");
        assert_eq!(found_thread.subject, "Test Thread");
        assert_eq!(found_msg.subject, "Test Subject");
    }

    // -----------------------------------------------------------------------
    // Sample constructors for additional model types (Task 1 lifecycle tests)
    // -----------------------------------------------------------------------

    fn sample_label(id: &str, account_id: &str) -> Label {
        Label {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            path: "\\Important".to_string(),
            role: "important".to_string(),
            local_status: None,
        }
    }

    fn sample_contact(id: &str, account_id: &str) -> Contact {
        Contact {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            email: format!("{}@test.com", id),
            source: "mail".to_string(),
            refs: 1,
            contact_groups: vec![],
            info: None,
            name: Some(id.to_string()),
            google_resource_name: None,
            etag: None,
            book_id: None,
            hidden: false,
        }
    }

    fn sample_contact_group(id: &str, account_id: &str) -> ContactGroup {
        ContactGroup {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            name: "Test Group".to_string(),
            book_id: "book1".to_string(),
            google_resource_name: None,
        }
    }

    fn sample_event(id: &str, account_id: &str) -> Event {
        Event {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            calendar_id: "cal1".to_string(),
            icsuid: format!("{}@test.com", id),
            ics: None,
            href: None,
            etag: None,
            recurrence_id: String::new(),
            status: None,
            recurrence_start: 0,
            recurrence_end: 0,
            search_title: String::new(),
            search_description: String::new(),
            search_location: String::new(),
            search_participants: String::new(),
        }
    }

    fn sample_thread_with_folder(id: &str, account_id: &str, folder_id: &str) -> Thread {
        Thread {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            subject: "Test Thread".to_string(),
            last_message_timestamp: 1700001000,
            first_message_timestamp: 1700000000,
            last_message_sent_timestamp: 1700001000,
            last_message_received_timestamp: 1700000500,
            g_thr_id: None,
            unread: 1,
            starred: 0,
            in_all_mail: true,
            attachment_count: 0,
            search_row_id: None,
            folders: vec![serde_json::json!({"id": folder_id, "_refs": 1, "_u": 1, "_im": 1})],
            labels: vec![],
            participants: vec![],
            metadata: None,
        }
    }

    fn sample_message_with_metadata(id: &str, account_id: &str) -> Message {
        Message {
            id: id.to_string(),
            account_id: account_id.to_string(),
            version: 0,
            synced_at: None,
            sync_unsaved_changes: None,
            remote_uid: 1,
            date: 1700000000,
            subject: "Test".to_string(),
            header_message_id: format!("<{}@test.com>", id),
            g_msg_id: None,
            g_thr_id: None,
            reply_to_header_message_id: None,
            forwarded_header_message_id: None,
            unread: true,
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
            metadata: Some(vec![
                serde_json::json!({"pluginId": "snooze-plugin", "expiration": 1800000000, "value": "{}"}),
                serde_json::json!({"pluginId": "read-receipt", "expiration": null, "value": "{}"}),
            ]),
        }
    }

    // Helper to count rows in a secondary table
    async fn count_rows_in(store: &MailStore, table: &str, where_clause: &str, id: &str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM `{}` WHERE {} = ?1", table, where_clause);
        let id_owned = id.to_string();
        store.writer.call(move |db| -> Result<i64, rusqlite::Error> {
            db.query_row(&sql, rusqlite::params![id_owned], |row| row.get(0))
        }).await.expect("count_rows_in failed")
    }

    // -----------------------------------------------------------------------
    // Lifecycle hook tests (Task 1)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_lifecycle_message_with_metadata_maintains_join_table() {
        let (store, _rx) = setup_test_store().await;
        let mut msg = sample_message_with_metadata("msg1", "acc1");

        store.save(&mut msg).await.expect("save failed");

        // ModelPluginMetadata rows should be created for each metadata entry
        let count = count_rows_in(&store, "ModelPluginMetadata", "id", "msg1").await;
        assert_eq!(count, 2, "Should have 2 ModelPluginMetadata rows for 2 metadata entries");
    }

    #[tokio::test]
    async fn test_lifecycle_message_empty_metadata_deletes_join_rows() {
        let (store, _rx) = setup_test_store().await;
        let mut msg = sample_message_with_metadata("msg1", "acc1");

        // First save: creates 2 metadata rows
        store.save(&mut msg).await.expect("first save failed");

        // Second save: empty metadata -> should delete all metadata rows
        msg.metadata = Some(vec![]);
        store.save(&mut msg).await.expect("second save failed");

        let count = count_rows_in(&store, "ModelPluginMetadata", "id", "msg1").await;
        assert_eq!(count, 0, "Should have 0 ModelPluginMetadata rows after clearing metadata");
    }

    #[tokio::test]
    async fn test_lifecycle_thread_with_metadata_maintains_join_table() {
        let (store, _rx) = setup_test_store().await;
        let mut thread = sample_thread("t:thr1", "acc1");
        thread.metadata = Some(vec![
            serde_json::json!({"pluginId": "snooze-plugin", "expiration": 1800000000, "value": "{}"}),
        ]);

        store.save(&mut thread).await.expect("save failed");

        let count = count_rows_in(&store, "ModelPluginMetadata", "id", "t:thr1").await;
        assert_eq!(count, 1, "Should have 1 ModelPluginMetadata row for thread");
    }

    #[tokio::test]
    async fn test_lifecycle_folder_v1_inserts_thread_counts() {
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("folder1", "acc1");

        store.save(&mut folder).await.expect("save failed");
        assert_eq!(folder.version, 1);

        let count = count_rows_in(&store, "ThreadCounts", "categoryId", "folder1").await;
        assert_eq!(count, 1, "Saving Folder v1 should create a ThreadCounts row");
    }

    #[tokio::test]
    async fn test_lifecycle_folder_remove_deletes_thread_counts() {
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("folder1", "acc1");

        store.save(&mut folder).await.expect("save failed");
        store.remove(&folder).await.expect("remove failed");

        let count = count_rows_in(&store, "ThreadCounts", "categoryId", "folder1").await;
        assert_eq!(count, 0, "Removing Folder should delete its ThreadCounts row");
    }

    #[tokio::test]
    async fn test_lifecycle_label_v1_inserts_thread_counts() {
        let (store, _rx) = setup_test_store().await;
        let mut label = sample_label("label1", "acc1");

        store.save(&mut label).await.expect("save failed");
        assert_eq!(label.version, 1);

        let count = count_rows_in(&store, "ThreadCounts", "categoryId", "label1").await;
        assert_eq!(count, 1, "Saving Label v1 should create a ThreadCounts row");
    }

    #[tokio::test]
    async fn test_lifecycle_contact_v1_inserts_contact_search() {
        let (store, _rx) = setup_test_store().await;
        let mut contact = sample_contact("contact1", "acc1");

        store.save(&mut contact).await.expect("save failed");
        assert_eq!(contact.version, 1);

        let count = count_rows_in(&store, "ContactSearch", "content_id", "contact1").await;
        assert_eq!(count, 1, "Saving Contact v1 should create a ContactSearch FTS5 row");
    }

    #[tokio::test]
    async fn test_lifecycle_contact_remove_deletes_contact_search() {
        let (store, _rx) = setup_test_store().await;
        let mut contact = sample_contact("contact1", "acc1");

        store.save(&mut contact).await.expect("save failed");
        store.remove(&contact).await.expect("remove failed");

        let count = count_rows_in(&store, "ContactSearch", "content_id", "contact1").await;
        assert_eq!(count, 0, "Removing Contact should delete its ContactSearch row");
    }

    #[tokio::test]
    async fn test_lifecycle_contact_non_mail_source_updates_search() {
        let (store, _rx) = setup_test_store().await;
        let mut contact = sample_contact("contact1", "acc1");
        contact.source = "carddav".to_string();

        // First save (v1): insert into ContactSearch
        store.save(&mut contact).await.expect("first save failed");
        assert_eq!(contact.version, 1);

        // Second save (v2, source != "mail"): update ContactSearch
        contact.name = Some("Updated Name".to_string());
        store.save(&mut contact).await.expect("second save failed");
        assert_eq!(contact.version, 2);

        // Should still have exactly 1 row
        let count = count_rows_in(&store, "ContactSearch", "content_id", "contact1").await;
        assert_eq!(count, 1, "ContactSearch should still have 1 row after update");
    }

    #[tokio::test]
    async fn test_lifecycle_event_with_search_fields_inserts_event_search() {
        let (store, _rx) = setup_test_store().await;
        let mut event = sample_event("event1", "acc1");
        event.search_title = "Team Meeting".to_string();
        event.search_description = "Quarterly review".to_string();

        store.save(&mut event).await.expect("save failed");

        let count = count_rows_in(&store, "EventSearch", "content_id", "event1").await;
        assert_eq!(count, 1, "Saving Event with search fields should create EventSearch row");
    }

    #[tokio::test]
    async fn test_lifecycle_event_remove_deletes_event_search() {
        let (store, _rx) = setup_test_store().await;
        let mut event = sample_event("event1", "acc1");
        event.search_title = "Meeting".to_string();

        store.save(&mut event).await.expect("save failed");
        store.remove(&event).await.expect("remove failed");

        let count = count_rows_in(&store, "EventSearch", "content_id", "event1").await;
        assert_eq!(count, 0, "Removing Event should delete its EventSearch row");
    }

    #[tokio::test]
    async fn test_lifecycle_contact_group_remove_deletes_join_rows() {
        let (store, _rx) = setup_test_store().await;

        // Insert a ContactContactGroup join row manually before removal
        let group_id = "group1";
        let group_id_owned = group_id.to_string();
        store.writer.call(move |db| -> Result<(), rusqlite::Error> {
            db.execute(
                "INSERT INTO ContactContactGroup (id, value) VALUES (?1, ?2)",
                rusqlite::params!["contact1", group_id_owned],
            )?;
            Ok(())
        }).await.expect("manual insert failed");

        let mut group = sample_contact_group(group_id, "acc1");
        group.version = 1; // simulate already saved

        store.remove(&group).await.expect("remove failed");

        let count = count_rows_in(&store, "ContactContactGroup", "value", group_id).await;
        assert_eq!(count, 0, "Removing ContactGroup should delete ContactContactGroup join rows");
    }

    #[tokio::test]
    async fn test_lifecycle_thread_after_save_maintains_thread_category() {
        let (store, _rx) = setup_test_store().await;

        // Create a folder first (so it exists in ThreadCounts)
        let mut folder = sample_folder("folder1", "acc1");
        store.save(&mut folder).await.expect("save folder failed");

        // Create a thread with that folder
        let mut thread = sample_thread_with_folder("t:thr1", "acc1", "folder1");
        store.save(&mut thread).await.expect("save thread failed");

        // ThreadCategory should have a row for this thread+folder combination
        let sql = "SELECT COUNT(*) FROM ThreadCategory WHERE id = ?1 AND value = ?2";
        let count: i64 = store.writer.call(|db| -> Result<i64, rusqlite::Error> {
            db.query_row(sql, rusqlite::params!["t:thr1", "folder1"], |row| row.get(0))
        }).await.expect("ThreadCategory query failed");
        assert_eq!(count, 1, "Thread afterSave should populate ThreadCategory for each folder");
    }

    #[tokio::test]
    async fn test_lifecycle_thread_after_remove_clears_thread_category() {
        let (store, _rx) = setup_test_store().await;
        let mut folder = sample_folder("folder1", "acc1");
        store.save(&mut folder).await.expect("save folder failed");

        let mut thread = sample_thread_with_folder("t:thr1", "acc1", "folder1");
        store.save(&mut thread).await.expect("save thread failed");

        store.remove(&thread).await.expect("remove thread failed");

        let sql = "SELECT COUNT(*) FROM ThreadCategory WHERE id = ?1";
        let count: i64 = store.writer.call(|db| -> Result<i64, rusqlite::Error> {
            db.query_row(sql, rusqlite::params!["t:thr1"], |row| row.get(0))
        }).await.expect("ThreadCategory count failed");
        assert_eq!(count, 0, "Thread afterRemove should clear ThreadCategory rows");
    }
}
