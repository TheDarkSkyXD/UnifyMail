// delta_flush_task — owns stdout exclusively and flushes the coalesce buffer.
//
// Per 05-RESEARCH.md Pattern 4:
// - Buffer: IndexMap<String, Vec<DeltaStreamItem>> keyed by model_class
// - 500ms tick interval (MissedTickBehavior::Skip)
// - On tick: flush all buffered items to stdout (newline-delimited JSON), clear buffer
// - On channel close: flush remaining buffer, return
//
// CRITICAL: stdout.flush() is called explicitly after EVERY batch write.
// Without this, the OS pipe buffer is not flushed immediately, causing Electron
// to miss ProcessState deltas (IPC-06).
//
// This task is the ONLY writer to stdout. All other code must use tracing/stderr.

use super::item::DeltaStreamItem;
use indexmap::IndexMap;
use std::io::Write;
use tokio::sync::mpsc;
use tokio::time::{interval, Duration, MissedTickBehavior};

/// Dedicated tokio task that owns stdout and flushes delta items in 500ms batches.
///
/// Receives DeltaStreamItems from the mpsc channel, coalesces them by modelClass,
/// and writes newline-delimited JSON to stdout every 500ms (or immediately on
/// channel close).
///
/// The flush task runs until the sender side is dropped (sync mode exits).
#[allow(dead_code)]
pub async fn delta_flush_task(mut rx: mpsc::UnboundedReceiver<DeltaStreamItem>) {
    // IndexMap preserves insertion order for deterministic output
    let mut buffer: IndexMap<String, Vec<DeltaStreamItem>> = IndexMap::new();

    let mut tick = interval(Duration::from_millis(500));
    tick.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            // Incoming delta item — coalesce into buffer
            maybe_item = rx.recv() => {
                match maybe_item {
                    Some(item) => {
                        coalesce_into(&mut buffer, item);
                    }
                    None => {
                        // Channel closed — flush remaining buffer and exit
                        if !buffer.is_empty() {
                            flush_buffer(&buffer);
                        }
                        return;
                    }
                }
            }

            // 500ms tick — flush non-empty buffer to stdout
            _ = tick.tick() => {
                if !buffer.is_empty() {
                    flush_buffer(&buffer);
                    buffer.clear();
                }
            }
        }
    }
}

/// Coalesces an incoming item into the buffer.
///
/// Per 05-RESEARCH.md Pattern 3:
/// - Find existing entry for this item's model_class
/// - Try to concatenate (same delta_type required)
/// - If concatenation fails (different delta_type): push as new entry
#[allow(dead_code)]
fn coalesce_into(buffer: &mut IndexMap<String, Vec<DeltaStreamItem>>, item: DeltaStreamItem) {
    let key = item.model_class.clone();
    let entries = buffer.entry(key).or_default();

    // Try to merge with the last item of the same type
    if let Some(last) = entries.last_mut() {
        if last.concatenate(&item) {
            return; // Merged successfully
        }
    }

    // Could not merge — push as new entry
    entries.push(item);
}

/// Writes all buffered items to stdout as newline-delimited JSON.
///
/// Per 05-RESEARCH.md Pattern 4:
/// - Lock stdout for the entire batch write (prevents interleaving)
/// - Use serde_json::json! macro with string literal keys for guaranteed field names
/// - Call flush() explicitly after all items are written
///
/// The json! macro with literal string keys is a second layer of defense beyond
/// the serde renames — it guarantees exact field names regardless of struct renames.
#[allow(dead_code)]
fn flush_buffer(buffer: &IndexMap<String, Vec<DeltaStreamItem>>) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    for entries in buffer.values() {
        for item in entries {
            // Use json! macro with literal keys to guarantee exact wire format
            let line = serde_json::json!({
                "type": item.delta_type,
                "modelClass": item.model_class,
                "modelJSONs": item.model_jsons
            });

            if let Err(e) = writeln!(out, "{line}") {
                tracing::error!("Failed to write delta to stdout: {e}");
                return;
            }
        }
    }

    // CRITICAL: explicit flush ensures the OS pipe buffer is drained immediately.
    // Without this, Electron may not receive the ProcessState delta for seconds.
    if let Err(e) = out.flush() {
        tracing::error!("Failed to flush stdout after delta batch: {e}");
    }
}
