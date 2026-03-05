// imap/foreground_worker.rs — Foreground IDLE worker for real-time inbox monitoring.
//
// Implements the foreground IDLE loop described in 08-RESEARCH.md Pattern 1:
//   - Connects a dedicated IMAP session (separate from background sync) (IDLE-03)
//   - Enters IDLE on INBOX and waits for server push notifications
//   - Re-issues IDLE every 25 minutes to prevent 29-minute server timeout (IDLE-01)
//   - Interrupts IDLE immediately when a task arrives via mpsc channel (IDLE-02)
//   - Calls idle.done().await unconditionally after every IDLE exit (Pitfall 1)
//   - Executes tasks between IDLE cycles using real ImapSession (via from_inner)
//   - Drains any queued tasks before re-entering IDLE
//   - Signals background sync when new mail is detected via wake_tx
//   - Reconnects on connection errors with exponential backoff (max 3 retries then 30s sleep)
//
// EXIT: When shutdown_rx fires or task_rx is closed, the loop exits cleanly.

use std::sync::Arc;
use std::time::Duration;

use async_imap::extensions::idle::IdleResponse;
use tokio::sync::{broadcast, mpsc, Mutex};

use crate::account::Account;
use crate::delta::stream::DeltaStream;
use crate::error::SyncError;
use crate::imap::session::{ImapSession, ImapTlsStream};
use crate::models::task_model::Task;
use crate::oauth2::TokenManager;
use crate::store::mail_store::MailStore;
use crate::tasks::recovery::{expire_completed_tasks, reset_stuck_tasks, TASK_RETENTION_SECS};
use crate::tasks::{execute_task, TaskKind};

/// Connects and authenticates a fresh IMAP session for the foreground worker.
///
/// Returns the full `ImapSession` (with capabilities and is_gmail) so the caller
/// can preserve metadata when converting to raw session for IDLE and back via from_inner().
async fn connect_session(
    account: &Account,
    token_manager: &Arc<Mutex<TokenManager>>,
    delta: &Arc<DeltaStream>,
) -> Result<ImapSession, SyncError> {
    let pre_auth = ImapSession::connect(account).await?;

    // Get OAuth2 token if this account uses OAuth2
    let is_oauth2 = account
        .extra
        .get("settings")
        .and_then(|s| s.get("imap_security_type"))
        .and_then(|v| v.as_str())
        .map(|s| s == "oauth2")
        .unwrap_or(false);

    let session = if is_oauth2 {
        let token = token_manager.lock().await.get_valid_token(account, delta).await?;
        pre_auth.authenticate(account, Some(&token)).await?
    } else {
        pre_auth.authenticate(account, None).await?
    };

    Ok(session)
}

/// Foreground IDLE worker — runs concurrently with background_sync.
///
/// Parameters:
/// - `account`: Account credentials for connect/authenticate
/// - `store`: MailStore for task DB operations (crash recovery, task persistence)
/// - `delta`: DeltaStream for emitting deltas to Electron
/// - `token_manager`: Shared OAuth2 token cache (prevents concurrent refresh races)
/// - `task_rx`: Receives (Task, TaskKind) pairs from stdin_loop when queue-task arrives
/// - `shutdown_rx`: Broadcast receiver — fires when stdin closes (EOF)
/// - `wake_tx`: Signals background sync worker to run a quick cycle on new mail
pub async fn run_foreground_worker(
    account: Arc<Account>,
    store: Arc<MailStore>,
    delta: Arc<DeltaStream>,
    token_manager: Arc<Mutex<TokenManager>>,
    mut task_rx: mpsc::Receiver<(Task, TaskKind)>,
    mut shutdown_rx: broadcast::Receiver<()>,
    wake_tx: mpsc::Sender<()>,
) {
    // ---- Crash recovery: reset any tasks stuck in "remote" from a previous crash ----
    match reset_stuck_tasks(&store).await {
        Ok(n) if n > 0 => tracing::info!(
            account_id = %account.id,
            "Foreground worker: reset {} stuck task(s) from previous crash",
            n
        ),
        Err(e) => tracing::warn!(
            account_id = %account.id,
            "Foreground worker: crash recovery failed: {e}"
        ),
        _ => {}
    }

    // ---- Spawn task expiry on 60-second timer ----
    {
        let store_exp = Arc::clone(&store);
        let account_id = account.id.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                if let Err(e) = expire_completed_tasks(&store_exp, TASK_RETENTION_SECS).await {
                    tracing::warn!(
                        account_id = %account_id,
                        "Task expiry error: {e}"
                    );
                }
            }
        });
    }

    // ---- Connect the foreground IMAP session ----
    // Preserve capabilities and is_gmail for use with from_inner() after IDLE cycles.
    let imap_session = match connect_session(&account, &token_manager, &delta).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(
                account_id = %account.id,
                "Foreground worker: initial connect failed: {e} — exiting"
            );
            delta.emit_process_state(&account.id, true);
            return;
        }
    };

    // Extract session metadata before converting to raw for IDLE.
    // These are updated on each reconnect so the from_inner() calls below
    // always use the capabilities of the currently active connection.
    let mut capabilities = imap_session.capabilities.clone();
    let mut is_gmail = imap_session.is_gmail();
    let mut raw_session = imap_session.into_inner();

    // SELECT INBOX for the foreground session
    if let Err(e) = raw_session.select("INBOX").await {
        tracing::error!(
            account_id = %account.id,
            "Foreground worker: failed to SELECT INBOX: {e} — exiting"
        );
        return;
    }

    tracing::info!(
        account_id = %account.id,
        "Foreground worker started — entering IDLE on INBOX"
    );

    // ---- Main IDLE loop ----
    loop {
        // Check for shutdown before entering IDLE
        if shutdown_rx.try_recv().is_ok() {
            tracing::info!(
                account_id = %account.id,
                "Foreground worker: shutdown received — exiting IDLE loop"
            );
            break;
        }

        // Enter IDLE — idle() consumes the session; done() returns it
        let mut idle_handle = raw_session.idle();

        // Send IDLE command to server
        if let Err(e) = idle_handle.init().await {
            tracing::error!(
                account_id = %account.id,
                "Foreground worker: IDLE init failed: {e} — attempting reconnect"
            );
            // Recover session from handle before reconnecting
            match idle_handle.done().await {
                Ok(s) => raw_session = s,
                Err(_) => {
                    // Session unrecoverable — reconnect from scratch
                    match reconnect(&account, &token_manager, &delta).await {
                        Some((s, caps, gmail)) => {
                            raw_session = s;
                            capabilities = caps;
                            is_gmail = gmail;
                        }
                        None => break,
                    };
                }
            }
            continue;
        }

        // 25-minute timeout (IDLE-01) — re-issue IDLE to prevent 29-minute server timeout
        let (idle_future, interrupt) = idle_handle.wait_with_timeout(Duration::from_secs(25 * 60));

        // Relay task: watches for incoming tasks and drops StopSource to interrupt IDLE (IDLE-02)
        // The relay takes ownership of task_rx and interrupt, returning both after IDLE exits.
        let relay = tokio::spawn(async move {
            // Wait for a task or channel close
            let maybe_task = task_rx.recv().await;
            // Drop interrupt — this triggers ManualInterrupt in idle_future
            drop(interrupt);
            (task_rx, maybe_task)
        });

        // Wait for IDLE to complete (timeout, new data, or interrupt)
        let idle_result = idle_future.await;

        // ALWAYS call done() to send DONE to server and reclaim the session (Pitfall 1)
        // done() consumes the idle_handle and returns the inner Session
        let session_result = idle_handle.done().await;

        // Recover relay task regardless of done() result
        let (recovered_rx, maybe_task) = match relay.await {
            Ok(pair) => pair,
            Err(e) => {
                tracing::error!(
                    account_id = %account.id,
                    "Foreground worker: relay task panicked: {e}"
                );
                break;
            }
        };
        task_rx = recovered_rx;

        // Recover raw session from done() result
        match session_result {
            Ok(s) => raw_session = s,
            Err(e) => {
                tracing::error!(
                    account_id = %account.id,
                    "Foreground worker: idle.done() failed: {e} — reconnecting"
                );
                match reconnect(&account, &token_manager, &delta).await {
                    Some((s, caps, gmail)) => {
                        raw_session = s;
                        capabilities = caps;
                        is_gmail = gmail;
                        // Re-SELECT INBOX after reconnect
                        if let Err(e) = raw_session.select("INBOX").await {
                            tracing::error!(
                                account_id = %account.id,
                                "Foreground worker: failed to SELECT INBOX after reconnect: {e}"
                            );
                            break;
                        }
                    }
                    None => break,
                };
                continue;
            }
        }

        // Handle IDLE result
        match idle_result {
            Ok(IdleResponse::NewData(_)) => {
                tracing::debug!(
                    account_id = %account.id,
                    "IDLE: new mail detected — signaling background sync"
                );
                // Signal background sync to do a quick cycle
                if wake_tx.try_send(()).is_err() {
                    tracing::debug!(
                        account_id = %account.id,
                        "IDLE: wake channel full or closed — background sync already awake"
                    );
                }
            }
            Ok(IdleResponse::Timeout) => {
                tracing::debug!(
                    account_id = %account.id,
                    "IDLE: 25-minute timeout — re-entering IDLE"
                );
            }
            Ok(IdleResponse::ManualInterrupt) => {
                tracing::debug!(
                    account_id = %account.id,
                    "IDLE: manual interrupt — processing task"
                );
            }
            Err(e) => {
                tracing::error!(
                    account_id = %account.id,
                    "IDLE error: {e} — reconnecting"
                );
                match reconnect(&account, &token_manager, &delta).await {
                    Some((s, caps, gmail)) => {
                        raw_session = s;
                        capabilities = caps;
                        is_gmail = gmail;
                        // Re-SELECT INBOX after reconnect
                        if let Err(e) = raw_session.select("INBOX").await {
                            tracing::error!(
                                account_id = %account.id,
                                "Foreground worker: failed to SELECT INBOX after reconnect: {e}"
                            );
                            break;
                        }
                    }
                    None => break,
                }
                continue;
            }
        }

        // Execute the task received during IDLE (if any) and drain all queued tasks.
        //
        // Strategy: wrap raw_session into ImapSession via from_inner() so task handlers
        // can use the full ImapTaskOps implementation. After all tasks are done,
        // unwrap back to raw session via into_inner() for the next IDLE cycle.
        //
        // Both the initial task from the relay and any queued tasks are executed within
        // the same wrapped-session scope to avoid repeated wrap/unwrap overhead.
        if maybe_task.is_some() {
            let (mut task, task_kind) = maybe_task.unwrap();
            tracing::debug!(
                account_id = %account.id,
                task_id = %task.id,
                class_name = %task.class_name,
                "Foreground worker: executing task"
            );

            // Wrap raw session into ImapSession for task execution
            let mut imap_session =
                ImapSession::from_inner(raw_session, capabilities.clone(), is_gmail);

            execute_task(
                &mut task,
                &task_kind,
                &store,
                &delta,
                Some(&mut imap_session),
                Some(&account),
                Some(&token_manager),
            )
            .await
            .unwrap_or_else(|e| {
                tracing::error!(
                    account_id = %account.id,
                    task_id = %task.id,
                    "Task execution failed: {e}"
                );
            });

            // Drain any additional queued tasks in the same IMAP session
            while let Ok((mut task, task_kind)) = task_rx.try_recv() {
                tracing::debug!(
                    account_id = %account.id,
                    task_id = %task.id,
                    class_name = %task.class_name,
                    "Foreground worker: draining queued task"
                );
                execute_task(
                    &mut task,
                    &task_kind,
                    &store,
                    &delta,
                    Some(&mut imap_session),
                    Some(&account),
                    Some(&token_manager),
                )
                .await
                .unwrap_or_else(|e| {
                    tracing::error!(
                        account_id = %account.id,
                        task_id = %task.id,
                        "Queued task execution failed: {e}"
                    );
                });
            }

            // Unwrap back to raw session for the next IDLE cycle
            raw_session = imap_session.into_inner();
        } else {
            // No initial task from relay — drain any tasks queued during the IDLE cycle
            // (e.g., queued while a Timeout or NewData response was being handled)
            if let Ok((mut task, task_kind)) = task_rx.try_recv() {
                let mut imap_session =
                    ImapSession::from_inner(raw_session, capabilities.clone(), is_gmail);

                tracing::debug!(
                    account_id = %account.id,
                    task_id = %task.id,
                    class_name = %task.class_name,
                    "Foreground worker: draining queued task (no initial task from relay)"
                );
                execute_task(
                    &mut task,
                    &task_kind,
                    &store,
                    &delta,
                    Some(&mut imap_session),
                    Some(&account),
                    Some(&token_manager),
                )
                .await
                .unwrap_or_else(|e| {
                    tracing::error!(
                        account_id = %account.id,
                        task_id = %task.id,
                        "Queued task execution failed: {e}"
                    );
                });

                while let Ok((mut task, task_kind)) = task_rx.try_recv() {
                    tracing::debug!(
                        account_id = %account.id,
                        task_id = %task.id,
                        class_name = %task.class_name,
                        "Foreground worker: draining queued task"
                    );
                    execute_task(
                        &mut task,
                        &task_kind,
                        &store,
                        &delta,
                        Some(&mut imap_session),
                        Some(&account),
                        Some(&token_manager),
                    )
                    .await
                    .unwrap_or_else(|e| {
                        tracing::error!(
                            account_id = %account.id,
                            task_id = %task.id,
                            "Queued task execution failed: {e}"
                        );
                    });
                }

                raw_session = imap_session.into_inner();
            }
        }

        // Re-SELECT INBOX before re-entering IDLE
        // (tasks may have selected other folders during execution)
        if let Err(e) = raw_session.select("INBOX").await {
            tracing::error!(
                account_id = %account.id,
                "Foreground worker: failed to re-SELECT INBOX: {e} — reconnecting"
            );
            match reconnect(&account, &token_manager, &delta).await {
                Some((s, caps, gmail)) => {
                    raw_session = s;
                    capabilities = caps;
                    is_gmail = gmail;
                    // Try selecting INBOX once more after reconnect
                    if let Err(e) = raw_session.select("INBOX").await {
                        tracing::error!(
                            account_id = %account.id,
                            "Foreground worker: SELECT INBOX after reconnect failed: {e} — exiting"
                        );
                        break;
                    }
                }
                None => break,
            }
        }
    }

    tracing::info!(
        account_id = %account.id,
        "Foreground worker exited"
    );
}

/// Attempts to reconnect with exponential backoff (3 retries, then 30s sleep).
///
/// Returns `Some((raw_session, capabilities, is_gmail))` on success, so callers
/// can update their local copies and use `ImapSession::from_inner()` for task execution.
/// Returns `None` if all retries are exhausted.
/// Emits a connectionError ProcessState delta on persistent auth failure.
async fn reconnect(
    account: &Arc<Account>,
    token_manager: &Arc<Mutex<TokenManager>>,
    delta: &Arc<DeltaStream>,
) -> Option<(async_imap::Session<ImapTlsStream>, Vec<String>, bool)> {
    const MAX_RETRIES: u32 = 3;

    for attempt in 0..MAX_RETRIES {
        let delay = Duration::from_secs(5 * 3u64.pow(attempt)); // 5s, 15s, 45s
        tracing::info!(
            account_id = %account.id,
            attempt = attempt + 1,
            "Foreground worker: reconnect attempt {} (delay {}s)",
            attempt + 1,
            delay.as_secs()
        );
        tokio::time::sleep(delay).await;

        match connect_session(account, token_manager, delta).await {
            Ok(imap_session) => {
                tracing::info!(
                    account_id = %account.id,
                    "Foreground worker: reconnected successfully"
                );
                let capabilities = imap_session.capabilities.clone();
                let is_gmail = imap_session.is_gmail();
                let raw_session = imap_session.into_inner();
                return Some((raw_session, capabilities, is_gmail));
            }
            Err(e) => {
                tracing::warn!(
                    account_id = %account.id,
                    "Foreground worker: reconnect attempt {} failed: {e}",
                    attempt + 1
                );
            }
        }
    }

    // All retries exhausted — emit connection error and wait 30s before giving up
    tracing::error!(
        account_id = %account.id,
        "Foreground worker: all reconnect attempts failed — emitting connectionError"
    );
    delta.emit_process_state(&account.id, true);

    tokio::time::sleep(Duration::from_secs(30)).await;
    None
}
