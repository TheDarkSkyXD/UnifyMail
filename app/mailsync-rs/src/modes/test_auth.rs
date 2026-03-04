// --mode test handler.
//
// In the C++ implementation, this mode does a full IMAP + SMTP connection test
// (runTestAuth() in main.cpp). Phase 5 does not have IMAP/SMTP code yet.
//
// Per 05-RESEARCH.md Open Question 1 and 05-CONTEXT.md:
// The N-API validateAccount path in mailsync-process.ts handles account validation
// already (v1.0), so test mode is vestigial. Implement as a stub returning
// ErrorNotImplemented with exit code 1.
//
// The error JSON is written to stdout by main.rs (via the Err return),
// so this function just returns Err without printing — avoids double-printing.
//
// Exit code: 1 (error — not yet implemented)

use crate::account::{Account, Identity};
use crate::error::SyncError;

/// Runs the test-auth mode (stub for Phase 5).
/// Returns Err with ErrorNotImplemented — main.rs writes JSON to stdout and exits 1.
pub async fn run(_account: &Account, _identity: &Option<Identity>) -> Result<(), SyncError> {
    Err(SyncError::NotImplemented(
        "test mode not yet implemented in Rust binary — use the N-API validateAccount path".to_string(),
    ))
}
