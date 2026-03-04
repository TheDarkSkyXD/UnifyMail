// MailStore — SQLite database access for mailsync-rs.
//
// Wraps tokio-rusqlite for async access to the email database.
// The database lives at {CONFIG_DIR_PATH}/edgehill.db — same path as the C++ binary.
//
// Key properties matching C++ MailStore.cpp:
// - WAL mode for concurrent reads during sync
// - page_size=4096, cache_size=10000, synchronous=NORMAL (matching MailStore.cpp lines 96-100)
// - busy_timeout=10s (prevents "database is locked" errors during concurrent access)
// - Schema version tracked via PRAGMA user_version
// - Migrations are idempotent — CREATE TABLE IF NOT EXISTS and version guards

use std::io::Write;
use tokio_rusqlite::Connection;
use crate::error::SyncError;
use crate::store::migrations::{
    ACCOUNT_RESET_QUERIES, V1_SETUP, V2_SETUP, V3_SETUP, V4_SETUP, V6_SETUP, V7_SETUP,
    V8_SETUP, V9_SETUP,
};

/// The email database wrapper.
/// In sync mode, two connections are used (writer + reader) for WAL concurrency.
/// In offline modes (migrate, reset), a single connection is sufficient.
pub struct MailStore {
    /// Primary connection — all writes go through this connection
    conn: Connection,
}

impl MailStore {
    /// Opens the database at `{config_dir}/edgehill.db`.
    /// Creates the file if it doesn't exist.
    /// Applies WAL mode and PRAGMA settings matching C++ MailStore constructor.
    pub async fn open(config_dir: &str) -> Result<Self, SyncError> {
        let db_path = format!("{}/edgehill.db", config_dir);

        let conn = Connection::open(&db_path)
            .await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        // Apply PRAGMAs matching C++ MailStore.cpp lines 96-100
        // tokio-rusqlite::Connection::call requires explicit Result type in closure
        conn.call(|c| -> Result<(), rusqlite::Error> {
            c.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA main.page_size = 4096;
                 PRAGMA main.cache_size = 10000;
                 PRAGMA main.synchronous = NORMAL;",
            )?;
            c.busy_timeout(std::time::Duration::from_secs(10))?;
            Ok(())
        })
        .await
        .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(Self { conn })
    }

    /// Runs schema migrations from the current version up to version 9.
    /// Idempotent — safe to call multiple times.
    /// For V3 (MessageBody.fetchedAt), prints "Running Migration" to stdout
    /// to match C++ behavior (Electron shows a migration progress window).
    pub async fn migrate(&self) -> Result<(), SyncError> {
        self.conn
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

        self.conn
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

    /// Closes the database connection.
    /// Consuming self ensures the connection cannot be used after closing.
    pub fn close(self) {
        drop(self.conn);
    }
}
