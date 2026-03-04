// imap/sync_worker.rs — IMAP background sync loop stub.
//
// Full implementation in Phase 7 Plans 02 (folder discovery, sync loop skeleton)
// and 03 (UID sync, MODSEQ-based incremental sync).

use crate::error::SyncError;

// ---------------------------------------------------------------------------
// Constants used across the sync worker (Plans 02/03/04)
// ---------------------------------------------------------------------------

/// Folders sorted by role priority for sync ordering.
/// Inbox first, then common high-priority folders, then the rest alphabetically.
#[allow(dead_code)]
pub const ROLE_ORDER: &[&str] = &[
    "inbox",
    "sent",
    "drafts",
    "trash",
    "archive",
    "spam",
    "junk",
];

/// Maximum MODSEQ delta fetch before falling back to full UID sync.
/// Above this threshold, a full sync is cheaper than processing thousands of changes.
#[allow(dead_code)]
pub const MODSEQ_TRUNCATION_THRESHOLD: u32 = 4000;

/// Body cache TTL in seconds: bodies older than this are considered stale.
/// Computed as 30 days.
#[allow(dead_code)]
pub const BODY_CACHE_AGE_SECS: u64 = 30 * 24 * 3600;

/// Pre-fetch bodies for messages younger than this age (in seconds).
/// 7 days — only recent messages are pre-fetched eagerly.
#[allow(dead_code)]
pub const BODY_PREFETCH_AGE_SECS: u64 = 7 * 24 * 3600;

/// Number of message bodies to fetch per IMAP FETCH batch.
#[allow(dead_code)]
pub const BODY_SYNC_BATCH_SIZE: usize = 30;

// ---------------------------------------------------------------------------
// Stub functions — implemented in Plans 02/03
// ---------------------------------------------------------------------------

/// Background sync entry point (stub).
///
/// Full implementation in Plan 02: spawns folder sync tasks, manages the
/// IMAP connection lifecycle, and loops on the IMAP IDLE command.
#[allow(dead_code)]
pub async fn background_sync() -> Result<(), SyncError> {
    Err(SyncError::NotImplemented("background_sync".into()))
}

/// Sort folders by role priority for optimal sync ordering (stub).
///
/// Full implementation in Plan 02: uses ROLE_ORDER to move high-priority
/// folders (Inbox, Sent, Drafts) ahead of user-created folders.
#[allow(dead_code)]
pub fn sort_folders_by_role_priority(_folders: &mut Vec<String>) {
    // Implemented in Plan 02
}

#[cfg(test)]
mod tests {}
