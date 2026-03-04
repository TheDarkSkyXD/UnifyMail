// --mode reset handler.
//
// Deletes all data for the specified account from the database.
// Data for other accounts is preserved. Runs migrate() first to ensure
// the schema exists (matching C++ behavior in MailStore.cpp::resetForAccount).
//
// The account JSON is provided either via the --account flag or as a single
// line from stdin (C++ reads a single getline after mode dispatch).
//
// Exit code: 0 (success)

use crate::account::Account;
use crate::error::SyncError;
use crate::store::MailStore;

/// Runs the reset mode for the given account.
/// Ensures the schema exists (migrate first), then deletes all account data.
pub async fn run(config_dir: &str, account: &Account) -> Result<(), SyncError> {
    let store = MailStore::open(config_dir).await?;

    // Run migrate first to ensure tables exist for the DELETE queries.
    // C++ does this in MailStore.cpp::resetForAccount to handle fresh installs.
    store.migrate().await?;

    // Delete all data for this account
    store.reset_for_account(&account.id).await?;

    store.close();
    Ok(())
}
