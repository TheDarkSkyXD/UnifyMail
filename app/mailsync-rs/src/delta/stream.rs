// DeltaStream — wrapper around the mpsc sender for delta emission.
//
// DeltaStream is shared (Arc) across multiple tokio tasks. Each task calls
// emit() to send delta items to the dedicated delta_flush_task which owns
// stdout exclusively.
//
// This separation ensures stdout is written from exactly one task, preventing
// interleaved writes and satisfying the IPC contract (IPC-06).

use super::item::DeltaStreamItem;
use tokio::sync::mpsc::UnboundedSender;

/// Shared handle for emitting delta messages.
///
/// Wrap in Arc to share across tokio tasks:
/// `let delta = Arc::new(DeltaStream::new(tx));`
pub struct DeltaStream {
    tx: UnboundedSender<DeltaStreamItem>,
}

impl DeltaStream {
    /// Creates a new DeltaStream backed by the given mpsc sender.
    pub fn new(tx: UnboundedSender<DeltaStreamItem>) -> Self {
        Self { tx }
    }

    /// Emits a delta item to the flush task.
    ///
    /// Logs a warning if the channel is closed (flush task has exited).
    pub fn emit(&self, item: DeltaStreamItem) {
        if self.tx.send(item).is_err() {
            tracing::warn!("Delta channel closed — flush task may have exited");
        }
    }

    /// Convenience method for emitting a ProcessState delta.
    ///
    /// Called immediately after sync mode starts to signal Electron that the
    /// account is "online" (connectionError=false).
    pub fn emit_process_state(&self, account_id: &str, connection_error: bool) {
        self.emit(DeltaStreamItem::process_state(account_id, connection_error));
    }

    /// Emits a ProcessState delta with sync progress information.
    ///
    /// Used by the background sync worker to report per-folder sync progress to
    /// Electron's OnlineStatusStore. Progress is a value from 0.0 to 1.0.
    pub fn emit_sync_progress(&self, account_id: &str, folder_path: &str, progress: f32) {
        let item = DeltaStreamItem::new(
            "persist",
            "ProcessState",
            vec![serde_json::json!({
                "id": account_id,
                "accountId": account_id,
                "syncProgress": {
                    "folderPath": folder_path,
                    "progress": progress,
                },
            })],
        );
        self.emit(item);
    }
}
