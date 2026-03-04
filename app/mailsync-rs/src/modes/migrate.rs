// --mode migrate handler.
//
// Opens (or creates) the database at {config_dir}/edgehill.db and runs
// all schema migrations up to version 9. This is idempotent — safe to run
// multiple times. The Electron UI calls this mode on startup to ensure the
// database schema is current.
//
// Exit code: 0 (success)
// Stdout: "Running Migration" during V3 migration (Electron shows progress dialog)

use crate::error::SyncError;
use crate::store::MailStore;

/// Runs the migrate mode.
/// Opens the database, runs all pending migrations, and returns Ok(()).
pub async fn run(config_dir: &str) -> Result<(), SyncError> {
    let store = MailStore::open(config_dir).await?;
    store.migrate().await?;
    store.close();
    Ok(())
}
