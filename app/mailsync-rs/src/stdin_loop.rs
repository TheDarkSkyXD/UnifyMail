// stdin_loop — async task that reads stdin commands and dispatches them.
//
// Per 05-RESEARCH.md Pattern 6 and Pattern 8:
// - Reads newline-delimited JSON commands from the shared stdin Lines iterator
// - Dispatches each command to the appropriate handler (stubs for Phase 5)
// - Detects stdin EOF and signals shutdown via broadcast channel
// - Unknown commands: warn-level log, continue reading
//
// All command handlers at this phase log at debug level and return silently.
// Electron UI does not expect responses for stub handlers (05-CONTEXT.md "No response for stubs").
//
// CRITICAL: The Lines iterator is created ONCE in main.rs and passed here.
// Do NOT create a new BufReader inside this function — the handshake lines were
// already consumed by main.rs using the same reader, and creating a new reader
// would lose the data buffered from the handshake (OS pipe data is consumed).
//
// NOTE: This loop does NOT call process::exit() directly. It returns the exit code
// to the caller (sync mode), which ensures the delta flush task has a chance to
// flush its buffer before the process terminates.

use crate::delta::DeltaStream;
use crate::models::task_model::Task;
use crate::tasks::{parse_task_kind, TaskKind};
use std::sync::Arc;
use tokio::io::{BufReader, Lines};
use tokio::sync::mpsc;

/// All C++ stdin command types from main.cpp runListenOnMainThread().
///
/// Defined upfront per 05-CONTEXT.md "Full command enum upfront" decision.
/// Later phases implement real handlers; Phase 5 stubs log and return.
#[derive(Debug)]
#[allow(dead_code)]
pub enum StdinCommand {
    /// Queue a sync task (e.g., send email, move folder, star thread)
    QueueTask { task_json: serde_json::Value },

    /// Cancel a previously queued task
    CancelTask { task_id: String },

    /// Wake sync workers — retry stalled operations immediately
    WakeWorkers,

    /// Request that message bodies be fetched for given message IDs
    NeedBodies { message_ids: Vec<String> },

    /// Trigger calendar sync
    SyncCalendar,

    /// Detect the email provider for a given email address
    DetectProvider { email: String },

    /// Query connection capabilities (IMAP extensions, etc.)
    QueryCapabilities,

    /// Subscribe to status updates for a specific folder
    SubscribeFolderStatus { folder_id: String },

    /// Test command: crash the process (for testing crash reporting)
    TestCrash,

    /// Test command: trigger a segfault (for testing crash reporting)
    TestSegfault,
}

/// Parses a JSON line into a StdinCommand.
///
/// Returns None for unknown commands (after logging a warning).
/// Returns None for malformed JSON (after logging an error).
fn parse_command(line: &str) -> Option<StdinCommand> {
    let parsed: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("Failed to parse stdin command as JSON: {e}. Line: {line}");
            return None;
        }
    };

    let command = parsed.get("command").and_then(|v| v.as_str()).unwrap_or("");

    match command {
        "queue-task" => {
            let task_json = parsed.get("task").cloned().unwrap_or(serde_json::Value::Null);
            Some(StdinCommand::QueueTask { task_json })
        }
        "cancel-task" => {
            let task_id = parsed
                .get("taskId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(StdinCommand::CancelTask { task_id })
        }
        "wake-workers" => Some(StdinCommand::WakeWorkers),
        "need-bodies" => {
            let message_ids = parsed
                .get("messageIds")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                })
                .unwrap_or_default();
            Some(StdinCommand::NeedBodies { message_ids })
        }
        "sync-calendar" => Some(StdinCommand::SyncCalendar),
        "detect-provider" => {
            let email = parsed
                .get("email")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(StdinCommand::DetectProvider { email })
        }
        "query-capabilities" => Some(StdinCommand::QueryCapabilities),
        "subscribe-folder-status" => {
            let folder_id = parsed
                .get("folderId")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(StdinCommand::SubscribeFolderStatus { folder_id })
        }
        "test-crash" => Some(StdinCommand::TestCrash),
        "test-segfault" => Some(StdinCommand::TestSegfault),
        unknown => {
            tracing::warn!("Unknown stdin command received: '{unknown}' — ignoring (forward-compatible)");
            None
        }
    }
}

/// Dispatches a parsed command to the appropriate handler.
///
/// WakeWorkers and NeedBodies are forwarded to the background sync worker via mpsc channels.
/// QueueTask is parsed and forwarded to the foreground worker via task_tx.
fn dispatch_command(
    command: StdinCommand,
    _delta: &Arc<DeltaStream>,
    wake_tx: &mpsc::Sender<()>,
    body_queue_tx: &mpsc::Sender<Vec<String>>,
    task_tx: &mpsc::Sender<(Task, TaskKind)>,
) {
    match command {
        StdinCommand::QueueTask { task_json } => {
            // Parse the task JSON into a Task model and TaskKind enum
            match parse_task_kind(&task_json) {
                Ok(task_kind) => {
                    // Build Task model from the JSON fields for DB persistence
                    let task = Task {
                        id: task_json
                            .get("id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        account_id: task_json
                            .get("aid")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        version: task_json
                            .get("v")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0),
                        class_name: task_json
                            .get("__cls")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        status: "local".to_string(),
                        should_cancel: None,
                        error: None,
                    };
                    // Non-blocking send: log a warning if the channel is full or closed.
                    // With capacity 32, this should not drop tasks under normal load.
                    if task_tx.try_send((task, task_kind)).is_err() {
                        tracing::warn!("QueueTask: task channel full or closed — task dropped");
                    }
                }
                Err(e) => {
                    tracing::error!("QueueTask: failed to parse task JSON: {e}. JSON: {task_json}");
                }
            }
        }
        StdinCommand::CancelTask { task_id } => {
            tracing::debug!("CancelTask received (stub): taskId={task_id}");
        }
        StdinCommand::WakeWorkers => {
            tracing::debug!("WakeWorkers received — sending wake signal to sync worker");
            // Non-blocking send: if channel is full, the sync worker already has a pending wake
            if wake_tx.try_send(()).is_err() {
                tracing::debug!("WakeWorkers: wake channel full or closed, signal dropped");
            }
        }
        StdinCommand::NeedBodies { message_ids } => {
            tracing::debug!("NeedBodies received: {} message ids — forwarding to body queue", message_ids.len());
            if body_queue_tx.try_send(message_ids).is_err() {
                tracing::warn!("NeedBodies: body_queue channel full or closed, request dropped");
            }
        }
        StdinCommand::SyncCalendar => {
            tracing::debug!("SyncCalendar received (stub)");
        }
        StdinCommand::DetectProvider { email } => {
            tracing::debug!("DetectProvider received (stub): email={email}");
        }
        StdinCommand::QueryCapabilities => {
            tracing::debug!("QueryCapabilities received (stub)");
        }
        StdinCommand::SubscribeFolderStatus { folder_id } => {
            tracing::debug!("SubscribeFolderStatus received (stub): folderId={folder_id}");
        }
        StdinCommand::TestCrash => {
            tracing::warn!("TestCrash command received — ignoring in Rust implementation");
        }
        StdinCommand::TestSegfault => {
            tracing::warn!("TestSegfault command received — ignoring in Rust implementation");
        }
    }
}

/// The stdin reader tokio task.
///
/// Per 05-RESEARCH.md Pattern 6:
/// - Accepts the shared Lines iterator (created in main.rs after handshake reads)
/// - Loops on next_line() until EOF or error
/// - On EOF (Ok(None)): signal shutdown via broadcast channel, return
///   (sync mode handles flush + exit(141))
/// - On error: log, continue
/// - Orphan mode: log EOF but don't signal shutdown
///
/// `wake_tx`, `body_queue_tx`, and `task_tx` are forwarded to `dispatch_command` for
/// WakeWorkers, NeedBodies, and QueueTask commands respectively.
///
/// Per the CRITICAL note above: do NOT create a new BufReader inside this fn.
/// The Lines iterator already has the correct read position after the handshake.
pub async fn stdin_loop(
    shutdown_tx: tokio::sync::broadcast::Sender<()>,
    delta: Arc<DeltaStream>,
    orphan: bool,
    mut lines: Lines<BufReader<tokio::io::Stdin>>,
    wake_tx: mpsc::Sender<()>,
    body_queue_tx: mpsc::Sender<Vec<String>>,
    task_tx: mpsc::Sender<(Task, TaskKind)>,
) {
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let line = line.trim().to_string();
                if line.is_empty() {
                    // Ignore empty lines (blank lines between commands, etc.)
                    continue;
                }

                // Parse and dispatch the command
                if let Some(command) = parse_command(&line) {
                    dispatch_command(command, &delta, &wake_tx, &body_queue_tx, &task_tx);
                }
                // If parse_command returns None: already logged, continue
            }

            Ok(None) => {
                // stdin EOF — parent process closed the pipe
                if orphan {
                    tracing::info!("stdin EOF detected in orphan mode — not signaling shutdown");
                    return; // Exit the task but not the process in orphan mode
                }

                tracing::info!("stdin EOF detected — signaling shutdown");
                // Signal shutdown to sync mode — it will handle flush + exit(141)
                let _ = shutdown_tx.send(());
                return;
            }

            Err(e) => {
                // Read error — log and continue
                tracing::error!("stdin read error: {e}");
                // Continue the loop — may recover on next read
            }
        }
    }
}
