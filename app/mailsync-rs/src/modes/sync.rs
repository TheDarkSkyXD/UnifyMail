// Sync mode — the main operating mode for mailsync-rs.
//
// Per 05-RESEARCH.md Pattern 8:
// - Three independent tokio tasks: stdin_loop, delta_flush_task, background_sync_stub
// - Handshake is done in main.rs before calling run()
// - After run() starts: emit ProcessState delta to tell Electron the account is online
// - Sync mode loops until stdin closes (exit 141)
//
// Phase 5: All IMAP/SMTP work is stubbed. The account appears "online" to Electron
// but no actual mail syncing occurs. Commands are accepted and logged at debug level.
//
// EXIT PROTOCOL:
// 1. stdin_loop detects EOF -> sends shutdown broadcast signal -> returns
// 2. sync::run() awaits stdin_handle completion
// 3. Drop DeltaStream (closes the mpsc channel sender)
// 4. Await delta_flush_task to complete (flushes remaining buffer to stdout)
// 5. Call process::exit(141)
//
// This ensures all pending deltas (including ProcessState) are flushed before exit.

use crate::account::{Account, Identity};
use crate::cli::Args;
use crate::delta::{delta_flush_task, DeltaStream, DeltaStreamItem};
use crate::error::SyncError;
use crate::stdin_loop::stdin_loop;
use crate::store::mail_store::MailStore;
use std::sync::Arc;
use tokio::io::{BufReader, Lines};
use tokio::sync::{broadcast, mpsc};

/// Entry point for --mode sync.
///
/// Called from main.rs after the two-line stdin handshake is complete.
/// Receives the parsed Account, optional Identity, and the shared stdin Lines
/// iterator (positioned after the handshake lines).
pub async fn run(
    config_dir: &str,
    account: Account,
    _identity: Option<Identity>,
    args: &Args,
    lines: Lines<BufReader<tokio::io::Stdin>>,
) -> Result<(), SyncError> {
    tracing::info!(
        account_id = %account.id,
        "Starting sync mode for account"
    );

    // Create the broadcast shutdown channel for coordinating task shutdown
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Create the delta mpsc channel — connects DeltaStream sender to flush task receiver
    let (delta_tx, delta_rx) = mpsc::unbounded_channel::<DeltaStreamItem>();

    // Open the MailStore (SQLite database) for this account
    let _store = Arc::new(MailStore::open(config_dir).await?);

    // Create the shared DeltaStream (Arc so it can be shared across tokio tasks)
    let delta = Arc::new(DeltaStream::new(delta_tx));

    // Emit startup ProcessState delta.
    // This tells Electron: "account is online, connectionError=false"
    // OnlineStatusStore.onSyncProcessStateReceived() processes this in TypeScript.
    // The delta goes into the mpsc channel and will be flushed to stdout
    // by the delta_flush_task on its 500ms tick or on channel close.
    delta.emit_process_state(&account.id, false);

    tracing::info!(
        account_id = %account.id,
        "ProcessState delta queued — account will appear online to Electron after flush"
    );

    // Task 1: stdin_loop — reads commands from the shared stdin reader, signals shutdown on EOF
    // The Lines iterator is passed directly so stdin_loop continues reading from where
    // the handshake left off (no data lost from multiple BufReader instances).
    let stdin_shutdown_tx = shutdown_tx.clone();
    let stdin_delta = Arc::clone(&delta);
    let orphan = args.orphan;
    let stdin_handle = tokio::spawn(async move {
        stdin_loop(stdin_shutdown_tx, stdin_delta, orphan, lines).await;
    });

    // Task 2: delta_flush_task — owns stdout, flushes every 500ms.
    // We keep the JoinHandle so we can await it after the delta channel closes.
    let flush_handle = tokio::spawn(delta_flush_task(delta_rx));

    // Task 3: background sync stub — waits for shutdown signal, does nothing
    // Phase 7+ will replace this with real IMAP sync work
    let sync_shutdown_rx = shutdown_tx.subscribe();
    let sync_handle = tokio::spawn(background_sync_stub(sync_shutdown_rx));

    // Wait for stdin_loop to complete (it returns after signaling shutdown on EOF)
    stdin_handle.await.ok();

    tracing::info!("stdin_loop completed — initiating graceful shutdown");

    // Cancel background sync stub (it's just waiting on shutdown signal)
    sync_handle.abort();

    // Drop the DeltaStream Arc to close the mpsc channel sender.
    // When the last sender is dropped, the flush task's rx.recv() returns None,
    // which triggers the channel-close branch that flushes the remaining buffer.
    drop(delta);

    // Await the flush task — this ensures all pending deltas (including ProcessState)
    // are written to stdout before we call process::exit().
    flush_handle.await.ok();

    tracing::info!("delta_flush_task completed — all deltas flushed, exiting with code 141");

    // Exit with code 141 — the standard orphan detection exit code
    // Matches C++ main.cpp behavior exactly (per 05-RESEARCH.md IPC-05)
    std::process::exit(141);
}

/// Placeholder for Phase 7+ IMAP sync workers.
///
/// In Phase 5, this task simply waits for the shutdown broadcast signal.
/// The real IMAP sync loop will be implemented in Phase 7 (IMAP Engine).
async fn background_sync_stub(mut shutdown_rx: broadcast::Receiver<()>) {
    tracing::debug!("background_sync_stub started — waiting for shutdown");
    let _ = shutdown_rx.recv().await;
    tracing::debug!("background_sync_stub received shutdown signal");
}
