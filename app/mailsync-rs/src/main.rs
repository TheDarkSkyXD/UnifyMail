// mailsync-rs — Rust rewrite of the UnifyMail sync engine binary.
//
// This binary is spawned per-account by the Electron main process via
// mailsync-process.ts. It must be an exact wire-format drop-in for the
// C++ mailsync binary from the TypeScript bridge's perspective.
//
// Wire format contract (from 05-RESEARCH.md):
// - stdin: two-line JSON handshake (account JSON, then identity JSON)
//   triggered by TypeScript after receiving any data on stdout
// - stdout: newline-delimited JSON deltas with fields: type, modelClass, modelJSONs
// - stderr: all logging (tracing crate) — stdout is exclusively for IPC protocol
// - exit codes: 0 (success), 1 (error with JSON on stdout), 141 (stdin EOF orphan)
//
// Mode dispatch order (from main.cpp):
// 1. install-check: no stdin, no CONFIG_DIR_PATH needed — dispatch immediately
// 2. migrate: needs CONFIG_DIR_PATH, no stdin
// 3. reset: needs CONFIG_DIR_PATH + account JSON from --account flag or stdin
// 4. test, sync: needs CONFIG_DIR_PATH + account JSON + identity JSON (handshake)
//
// CRITICAL: stdin must be read through a SINGLE BufReader for the entire process.
// Multiple BufReader instances on the same stdin cause data loss: the first BufReader
// buffers more than one line from the OS pipe, and that buffered data is lost when
// the BufReader is dropped. The single shared reader is passed to stdin_loop so it
// can continue reading commands after the handshake is complete.

mod account;
mod cli;
mod delta;
mod error;
mod imap;
mod modes;
mod models;
mod oauth2;
mod stdin_loop;
mod store;
mod tasks;

use clap::Parser;
use cli::{Args, Mode};
use error::SyncError;
use std::io::Write;
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    let args = Args::parse();

    // Initialize tracing — logs go to stderr ONLY.
    // stdout is reserved exclusively for IPC protocol messages (JSON deltas).
    let log_level = if args.verbose { "debug" } else { "info" };
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(log_level));

    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(env_filter)
        .init();

    // Run the selected mode and handle errors
    if let Err(e) = run_mode(args).await {
        // On error: write JSON error to stdout and exit with code 1
        let json_error = e.to_json_error("");
        println!("{json_error}");
        std::io::stdout().flush().ok();
        std::process::exit(1);
    }
}

async fn run_mode(args: Args) -> Result<(), SyncError> {
    // ================================================================
    // Modes that do NOT need CONFIG_DIR_PATH or account JSON
    // ================================================================
    if args.mode == Mode::InstallCheck {
        return modes::install_check::run().await;
    }

    // All other modes require CONFIG_DIR_PATH
    let config_dir = std::env::var("CONFIG_DIR_PATH").map_err(|_| {
        SyncError::Unexpected("CONFIG_DIR_PATH environment variable is required".to_string())
    })?;

    // ================================================================
    // Modes that do NOT need account JSON
    // ================================================================
    if args.mode == Mode::Migrate {
        return modes::migrate::run(&config_dir).await;
    }

    // ================================================================
    // Create a single shared stdin reader for the entire process lifecycle.
    //
    // CRITICAL: We must create ONE BufReader and share it across all reads.
    // Multiple BufReader instances on the same stdin cause data loss: the OS
    // delivers data in chunks, and the first BufReader buffers more than one
    // line. When that BufReader is dropped, the buffered data is lost.
    //
    // Solution: Create one BufReader<tokio::io::Stdin>, read handshake lines
    // from it, then pass the Lines iterator to stdin_loop for continued reading.
    // ================================================================
    let stdin_reader = BufReader::new(tokio::io::stdin());
    let mut lines = stdin_reader.lines();

    // ================================================================
    // Read account JSON (from --account flag or shared stdin reader)
    // ================================================================
    let account = read_account_json(&args, &mut lines).await?;

    // ================================================================
    // Modes that need account JSON but NOT identity JSON
    // ================================================================
    if args.mode == Mode::Reset {
        return modes::reset::run(&config_dir, &account).await;
    }

    // ================================================================
    // Modes that need both account JSON and identity JSON
    // (sync and test modes use the two-line stdin handshake)
    // ================================================================
    let identity = read_identity_json(&args, &mut lines).await?;

    match args.mode {
        Mode::Test => modes::test_auth::run(&account, &identity).await,
        Mode::Sync => modes::sync::run(&config_dir, account, identity, &args, lines).await,
        _ => unreachable!("All modes handled above"),
    }
}

/// Reads account JSON from either the --account flag or the shared stdin reader.
///
/// For sync/test/reset modes, the TypeScript bridge sends account JSON on stdin
/// after the binary writes any data to stdout (the two-line handshake protocol).
///
/// Per 05-RESEARCH.md Pattern 1: the binary must write to stdout FIRST to trigger
/// the TypeScript side to pipe account JSON. Without this, both sides wait — deadlock.
async fn read_account_json(
    args: &Args,
    lines: &mut Lines<BufReader<tokio::io::Stdin>>,
) -> Result<account::Account, SyncError> {
    if let Some(json) = &args.account {
        // Account provided via --account flag (used in some test scenarios)
        return serde_json::from_str(json).map_err(SyncError::from);
    }

    // Signal readiness to TypeScript — this triggers the stdin pipe.
    // The EXACT string from C++ main.cpp is preserved for compatibility
    // when running the binary manually in a terminal.
    print!("\nWaiting for Account JSON:\n");
    std::io::stdout().flush().map_err(SyncError::from)?;

    // Read account JSON from shared stdin reader (first line of two-line handshake)
    let account_line = lines
        .next_line()
        .await
        .map_err(SyncError::from)?
        .unwrap_or_default();
    let account_line = account_line.trim_end_matches('\r');

    if account_line.is_empty() {
        return Err(SyncError::Protocol("stdin closed before account JSON".to_string()));
    }

    serde_json::from_str(account_line).map_err(SyncError::from)
}

/// Reads identity JSON from either the --identity flag or the shared stdin reader.
/// Called after read_account_json() — uses the same shared reader to read line 2.
async fn read_identity_json(
    args: &Args,
    lines: &mut Lines<BufReader<tokio::io::Stdin>>,
) -> Result<Option<account::Identity>, SyncError> {
    if let Some(json) = &args.identity {
        if json == "null" {
            return Ok(None);
        }
        let identity = serde_json::from_str(json).map_err(SyncError::from)?;
        return Ok(Some(identity));
    }

    // Signal readiness for identity JSON
    print!("\nWaiting for Identity JSON:\n");
    std::io::stdout().flush().map_err(SyncError::from)?;

    // Read identity JSON from shared stdin reader (second line of two-line handshake)
    let identity_line = lines
        .next_line()
        .await
        .map_err(SyncError::from)?
        .unwrap_or_default();
    let identity_line = identity_line.trim_end_matches('\r');

    if identity_line.is_empty() || identity_line == "null" {
        Ok(None)
    } else {
        let identity = serde_json::from_str(identity_line).map_err(SyncError::from)?;
        Ok(Some(identity))
    }
}
