// CLI argument parsing for mailsync-rs.
// Replicates the --mode, --verbose, --orphan, --info, --account, --identity flags
// from the C++ mailsync binary so the TypeScript bridge (mailsync-process.ts)
// works identically with both binaries.

use clap::{Parser, ValueEnum};

/// UnifyMail sync process — manages email sync for a single account.
/// Spawned by the Electron main process via mailsync-process.ts.
#[derive(Parser, Debug)]
#[command(name = "mailsync-rs")]
pub struct Args {
    /// Process mode — determines the binary's behavior
    #[arg(short = 'm', long = "mode", value_enum)]
    pub mode: Mode,

    /// Allow process to run without a parent bound to stdin (orphan mode).
    /// Without this flag, the binary exits with code 141 when stdin closes.
    #[arg(short = 'o', long = "orphan")]
    pub orphan: bool,

    /// Enable verbose (DEBUG) logging. Also set RUST_LOG for fine-grained control.
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Email address (cosmetic — used for log file naming only)
    #[arg(long = "info")]
    pub info: Option<String>,

    /// Account JSON string (optional — if not provided, read from stdin for sync/test/reset modes)
    #[arg(short = 'a', long = "account")]
    pub account: Option<String>,

    /// Identity JSON string (optional — if not provided, read from stdin for sync/test modes)
    #[arg(short = 'i', long = "identity")]
    pub identity: Option<String>,
}

/// Process mode — controls the binary's operating behavior.
/// Matches the C++ --mode values exactly for compatibility with mailsync-process.ts.
#[derive(ValueEnum, Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Long-running sync mode — connects to IMAP/SMTP and syncs continuously.
    /// Reads account + identity JSON from stdin, emits JSON deltas to stdout.
    Sync,

    /// Account authentication test mode — verifies IMAP + SMTP credentials.
    /// Reads account + identity JSON from stdin.
    Test,

    /// Database schema migration mode — creates or migrates edgehill.db to version 9.
    /// Does not read from stdin. Exits 0 on success.
    Migrate,

    /// Account reset mode — deletes all data for the specified account.
    /// Reads account JSON from stdin (or --account flag).
    Reset,

    /// Installation health check mode — verifies the binary is operational.
    /// Exits 0 with JSON result if all checks pass.
    #[value(name = "install-check")]
    InstallCheck,
}
