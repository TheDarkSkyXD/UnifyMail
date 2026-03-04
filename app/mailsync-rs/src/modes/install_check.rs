// --mode install-check handler.
//
// Verifies the binary is operational and all required subsystems are available.
// In the C++ implementation this does HTTP + IMAP + SMTP + tidy checks.
// In Phase 5, all checks pass trivially (network code not yet implemented).
//
// Exit code: 0 (success — all checks passed)
// Stdout: JSON with check results per open question 2 in 05-RESEARCH.md

use std::io::Write;
use crate::error::SyncError;

/// Runs the install-check mode.
/// Prints JSON result to stdout and returns Ok(()) (exits with code 0).
pub async fn run() -> Result<(), SyncError> {
    // In Phase 5, all checks pass trivially — network code not yet implemented.
    // The TypeScript bridge (mailsync-process.ts) calls this mode during onboarding
    // to verify the binary is functional before spawning a full sync process.
    let result = serde_json::json!({
        "http_check": { "success": true },
        "imap_check": { "success": true },
        "smtp_check": { "success": true },
        "tidy_check": { "success": true },
    });

    println!("{result}");
    std::io::stdout().flush().map_err(SyncError::from)?;

    Ok(())
}
