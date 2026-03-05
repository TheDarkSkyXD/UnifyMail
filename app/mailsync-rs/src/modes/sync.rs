// Sync mode — the main operating mode for mailsync-rs.
//
// Per 05-RESEARCH.md Pattern 8:
// - Four independent tokio tasks: stdin_loop, delta_flush_task, background_sync, foreground_worker
// - Handshake is done in main.rs before calling run()
// - After run() starts: emit ProcessState delta to tell Electron the account is online
// - Sync mode loops until stdin closes (exit 141)
//
// Phase 7: background_sync_stub replaced with real background_sync function.
// Stdin commands WakeWorkers and NeedBodies are forwarded via mpsc channels.
//
// Phase 8: foreground_worker added for IDLE-based real-time inbox monitoring.
// queue-task stdin commands are forwarded via task_tx/task_rx to foreground worker.
// foreground_worker signals background_sync via fg_wake_tx (clone of wake_tx sender).
//
// EXIT PROTOCOL:
// 1. stdin_loop detects EOF -> sends shutdown broadcast signal -> returns
// 2. sync::run() awaits stdin_handle completion
// 3. Abort background_sync and foreground_worker handles
// 4. Drop DeltaStream (closes the mpsc channel sender)
// 5. Await delta_flush_task to complete (flushes remaining buffer to stdout)
// 6. Call process::exit(141)
//
// This ensures all pending deltas (including ProcessState) are flushed before exit.

use crate::account::{Account, Identity};
use crate::cli::Args;
use crate::delta::{delta_flush_task, DeltaStream, DeltaStreamItem};
use crate::error::SyncError;
use crate::imap::foreground_worker::run_foreground_worker;
use crate::imap::sync_worker::background_sync;
use crate::models::task_model::Task;
use crate::oauth2::TokenManager;
use crate::stdin_loop::stdin_loop;
use crate::store::mail_store::MailStore;
use crate::tasks::TaskKind;
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

    // Create the shared DeltaStream (Arc so it can be shared across tokio tasks and MailStore)
    let delta = Arc::new(DeltaStream::new(delta_tx));

    // Open the MailStore with reader connection and delta stream (sync mode).
    // open_with_delta() creates writer + reader connections for WAL concurrency (DATA-01).
    // The store holds an Arc<DeltaStream> clone so save/remove can emit deltas.
    let _store = Arc::new(MailStore::open_with_delta(config_dir, Arc::clone(&delta)).await?);

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

    // Create mpsc channels for background_sync communication from stdin commands:
    // - wake_tx/wake_rx: WakeWorkers command resets backoff and re-accelerates sync
    // - body_queue_tx/body_queue_rx: NeedBodies command inserts IDs at front of BodyQueue
    //
    // wake_tx is cloned before being passed to stdin_loop so the foreground worker can
    // also signal background_sync when IDLE detects new mail (multiple senders, one receiver).
    let (wake_tx, wake_rx) = mpsc::channel::<()>(32);
    let (body_queue_tx, body_queue_rx) = mpsc::channel::<Vec<String>>(32);

    // Clone wake_tx for the foreground worker before moving the original into stdin_loop.
    // mpsc::Sender is cheaply clonable — both senders deliver to the same wake_rx.
    let fg_wake_tx = wake_tx.clone();

    // Create task channel for foreground worker (queue-task stdin commands).
    // Bounded capacity 32 provides backpressure — prevents stdin from getting too far ahead.
    let (task_tx, task_rx) = mpsc::channel::<(Task, TaskKind)>(32);

    // Create shared TokenManager for OAuth2 token caching and refresh.
    // Wrapped in Arc<Mutex<>> so background_sync and foreground_worker (IDLE session)
    // don't race on concurrent token refresh requests (Phase 8 IDLE session).
    let token_manager = Arc::new(tokio::sync::Mutex::new(TokenManager::new()));

    // Task 1: stdin_loop — reads commands from the shared stdin reader, signals shutdown on EOF
    // The Lines iterator is passed directly so stdin_loop continues reading from where
    // the handshake left off (no data lost from multiple BufReader instances).
    // task_tx is passed so queue-task commands route to the foreground IDLE worker.
    let stdin_shutdown_tx = shutdown_tx.clone();
    let stdin_delta = Arc::clone(&delta);
    let orphan = args.orphan;
    let stdin_handle = tokio::spawn(async move {
        stdin_loop(stdin_shutdown_tx, stdin_delta, orphan, lines, wake_tx, body_queue_tx, task_tx)
            .await;
    });

    // Task 2: delta_flush_task — owns stdout, flushes every 500ms.
    // We keep the JoinHandle so we can await it after the delta channel closes.
    let flush_handle = tokio::spawn(delta_flush_task(delta_rx));

    // Wrap account in Arc for sharing between background_sync and foreground_worker.
    let sync_account = Arc::new(account);

    // Task 3: background_sync — real IMAP sync worker (replaces background_sync_stub).
    // Connects to IMAP, syncs all folders with backoff scheduling, and processes
    // body fetch requests from stdin commands (need-bodies, wake-workers).
    let sync_shutdown_rx = shutdown_tx.subscribe();
    let sync_store = Arc::clone(&_store);
    let sync_delta = Arc::clone(&delta);
    let sync_handle = tokio::spawn(background_sync(
        Arc::clone(&sync_account),
        sync_store,
        sync_delta,
        Arc::clone(&token_manager),
        sync_shutdown_rx,
        wake_rx,
        body_queue_rx,
    ));

    // Task 4: foreground_worker — monitors INBOX via IDLE for real-time new mail detection.
    // Uses a SEPARATE IMAP session from background_sync (IDLE-03).
    // Receives queue-task commands from stdin_loop via the bounded task_tx/task_rx channel.
    // Signals background_sync on new mail via fg_wake_tx (clone of the same wake channel).
    let fg_shutdown_rx = shutdown_tx.subscribe();
    let fg_account = Arc::clone(&sync_account);
    let fg_store = Arc::clone(&_store);
    let fg_delta = Arc::clone(&delta);
    let fg_token = Arc::clone(&token_manager);
    let fg_handle = tokio::spawn(run_foreground_worker(
        fg_account,
        fg_store,
        fg_delta,
        fg_token,
        task_rx,
        fg_shutdown_rx,
        fg_wake_tx,
    ));

    // Wait for stdin_loop to complete (it returns after signaling shutdown on EOF)
    stdin_handle.await.ok();

    tracing::info!("stdin_loop completed — initiating graceful shutdown");

    // Abort background sync and foreground workers (shutdown broadcast was already sent)
    sync_handle.abort();
    fg_handle.abort();

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
