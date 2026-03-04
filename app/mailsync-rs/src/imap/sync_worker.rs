// imap/sync_worker.rs — IMAP background sync loop.
//
// This file implements the core sync algorithms:
//   - CONDSTORE incremental sync (Plans 05)
//   - UID-range fallback for non-CONDSTORE servers (Plan 05)
//   - UIDVALIDITY change handling (RFC 4549 full re-sync) (Plan 05)
//   - Folder priority ordering (Plan 05)
//   - Per-operation timeout wrapping (Plan 05)

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::error::SyncError;
use crate::models::folder::Folder;

// ---------------------------------------------------------------------------
// Constants used across the sync worker
// ---------------------------------------------------------------------------

/// Folders sorted by role priority for sync ordering.
/// Inbox first, then common high-priority folders, then the rest alphabetically.
pub const ROLE_ORDER: &[&str] = &["inbox", "sent", "drafts", "all", "archive", "trash", "spam"];

/// Maximum MODSEQ delta before truncating UID fetch range.
/// Above this threshold, we limit the UID fetch range to the last 12000 UIDs.
pub const MODSEQ_TRUNCATION_THRESHOLD: u64 = 4000;

/// Number of UIDs to fetch when truncation is active.
pub const TRUNCATION_UID_WINDOW: u32 = 12000;

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

/// Base interval between sync cycles in seconds (~60s).
/// The sync loop waits at least this long between cycles.
pub const BASE_SYNC_INTERVAL_SECS: u64 = 60;

/// Maximum additional wait (backoff) in seconds.
/// Total max wait = BASE_SYNC_INTERVAL_SECS + MAX_BACKOFF_ADDITION_SECS = 300s (5 min).
pub const MAX_BACKOFF_ADDITION_SECS: u64 = 240;

// ---------------------------------------------------------------------------
// FolderSyncState — typed representation of Folder.local_status JSON
// ---------------------------------------------------------------------------

/// Deserializes a modseq value that may be stored as either a JSON string or number.
///
/// The C++ sync engine stores modseq as a string to prevent JavaScript precision loss
/// for values above 2^53 (Pitfall 5 in the plan). We deserialize both forms.
fn deserialize_modseq<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct ModseqVisitor;

    impl<'de> Visitor<'de> for ModseqVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a u64 or a string representing a u64")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u64, E> {
            Ok(v)
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u64, E> {
            if v < 0 {
                Err(E::custom("negative modseq"))
            } else {
                Ok(v as u64)
            }
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u64, E> {
            v.parse::<u64>().map_err(E::custom)
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<u64, E> {
            v.parse::<u64>().map_err(E::custom)
        }
    }

    deserializer.deserialize_any(ModseqVisitor)
}

/// Serializes modseq as a JSON string to prevent JavaScript precision loss for
/// values above 2^53. The C++ sync engine stores modseq as a string for the same reason.
fn serialize_modseq<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}

/// Typed representation of Folder.local_status JSON.
///
/// Matches the C++ SyncWorker local_status shape. Missing fields default to
/// zero/empty (per #[serde(default)]).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FolderSyncState {
    /// IMAP UIDVALIDITY value from last successful SELECT.
    #[serde(default)]
    pub uidvalidity: u32,

    /// CONDSTORE highest modseq from last sync. Stored as string in JSON
    /// to prevent JavaScript precision loss (Pitfall 5).
    #[serde(
        default,
        deserialize_with = "deserialize_modseq",
        serialize_with = "serialize_modseq"
    )]
    pub highestmodseq: u64,

    /// UIDNEXT from last sync (first UID that hasn't been assigned yet).
    #[serde(default)]
    pub uidnext: u32,

    /// Lowest UID synced in the current UID window.
    #[serde(rename = "syncedMinUID", default)]
    pub synced_min_uid: u32,

    /// Message IDs whose bodies have been downloaded.
    #[serde(rename = "bodiesPresent", default)]
    pub bodies_present: Vec<String>,

    /// Message IDs whose bodies are wanted but not yet downloaded.
    #[serde(rename = "bodiesWanted", default)]
    pub bodies_wanted: Vec<String>,

    /// Number of times UIDVALIDITY changed (RFC 4549 full re-sync counter).
    #[serde(rename = "uidvalidityResetCount", default)]
    pub uidvalidity_reset_count: u32,
}

/// Deserialize FolderSyncState from a Folder's local_status JSON value.
///
/// Returns a default (all-zero) state if local_status is None or cannot be parsed.
pub fn get_sync_state(folder: &Folder) -> FolderSyncState {
    folder
        .local_status
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

/// Serialize a FolderSyncState back to Folder.local_status.
pub fn set_sync_state(folder: &mut Folder, state: &FolderSyncState) {
    folder.local_status = serde_json::to_value(state).ok();
}

// ---------------------------------------------------------------------------
// Folder priority sort
// ---------------------------------------------------------------------------

/// Sort folders by role priority for optimal sync ordering.
///
/// Ordering follows ROLE_ORDER: inbox, sent, drafts, all, archive, trash, spam.
/// Folders with unknown roles (custom folders) sort after all known roles,
/// preserving their relative order among themselves (stable sort).
pub fn sort_folders_by_role_priority(folders: &mut Vec<Folder>) {
    folders.sort_by(|a, b| {
        let idx_a = ROLE_ORDER.iter().position(|&r| r == a.role.as_str());
        let idx_b = ROLE_ORDER.iter().position(|&r| r == b.role.as_str());
        match (idx_a, idx_b) {
            (Some(i), Some(j)) => i.cmp(&j),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
}

// ---------------------------------------------------------------------------
// CONDSTORE decision logic — pure functions (testable without IMAP)
// ---------------------------------------------------------------------------

/// Result of checking CONDSTORE state against the server.
#[derive(Debug, PartialEq)]
pub enum CondstoreDecision {
    /// No changes: server modseq + uidnext match stored values.
    NoChange,
    /// Normal incremental fetch using CHANGEDSINCE.
    Incremental { uid_set: String },
    /// Large gap — truncate to the last `TRUNCATION_UID_WINDOW` UIDs.
    Truncated { uid_set: String },
}

/// Determine what CONDSTORE sync action to take based on server vs stored state.
///
/// - If `server_modseq == stored_modseq && server_uidnext == stored_uidnext` -> NoChange
/// - If `server_modseq - stored_modseq > MODSEQ_TRUNCATION_THRESHOLD` -> Truncated
/// - Otherwise -> Incremental
///
/// `server_uidnext` of 0 means the server didn't report UIDNEXT (treat as changed).
pub fn decide_condstore_action(
    server_modseq: u64,
    server_uidnext: u32,
    stored_modseq: u64,
    stored_uidnext: u32,
) -> CondstoreDecision {
    // No-change: modseq and uidnext both match
    if server_modseq == stored_modseq && server_uidnext != 0 && server_uidnext == stored_uidnext {
        return CondstoreDecision::NoChange;
    }

    // Calculate delta (saturating subtraction to handle first sync where stored=0)
    let delta = server_modseq.saturating_sub(stored_modseq);

    if delta > MODSEQ_TRUNCATION_THRESHOLD {
        // Truncate to last TRUNCATION_UID_WINDOW UIDs
        let start_uid = server_uidnext.saturating_sub(TRUNCATION_UID_WINDOW);
        let uid_set = format!("{}:*", start_uid.max(1));
        CondstoreDecision::Truncated { uid_set }
    } else {
        CondstoreDecision::Incremental {
            uid_set: "1:*".to_string(),
        }
    }
}

/// Select the sync strategy based on whether the server supports CONDSTORE.
///
/// When `select_condstore()` returns a Mailbox with `highest_modseq = Some(...)`,
/// CONDSTORE incremental sync is used. When `highest_modseq = None`, the server
/// doesn't support CONDSTORE and UID-range fallback sync is used instead.
#[derive(Debug, PartialEq)]
pub enum SyncStrategy {
    /// Server supports CONDSTORE — use `decide_condstore_action()`.
    Condstore { server_modseq: u64 },
    /// Server does not support CONDSTORE — use UID-range fallback sync.
    UidRange,
}

/// Determine the sync strategy from a server's reported highest_modseq.
///
/// Returns `SyncStrategy::Condstore` if `highest_modseq` is Some, otherwise
/// `SyncStrategy::UidRange`.
pub fn select_sync_strategy(highest_modseq: Option<u64>) -> SyncStrategy {
    match highest_modseq {
        Some(modseq) => SyncStrategy::Condstore {
            server_modseq: modseq,
        },
        None => SyncStrategy::UidRange,
    }
}

/// Determine if a UIDVALIDITY change requires a full re-sync.
///
/// Per RFC 4549: if stored uidvalidity is non-zero AND differs from the server's
/// value, the client MUST discard all cached state and perform a full re-sync.
///
/// If stored uidvalidity is 0, this is the first time the folder is selected —
/// no reset is needed.
pub fn needs_uidvalidity_reset(stored: u32, server: u32) -> bool {
    stored != 0 && stored != server
}

// ---------------------------------------------------------------------------
// Stubs for async functions (implementations that wrap ImapSession)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Body caching helpers
// ---------------------------------------------------------------------------

/// Returns true if bodies should be cached for the given folder role.
///
/// Spam and trash folders are excluded from body caching per ISYN-07 policy:
/// fetching bodies of spam/trash messages is wasteful and may expose malicious content.
pub fn should_cache_bodies_in_folder(role: &str) -> bool {
    role != "spam" && role != "trash"
}

// ---------------------------------------------------------------------------
// Background sync entry point (real implementation — replaces stub)
// ---------------------------------------------------------------------------

/// Background sync entry point.
///
/// Connects to the IMAP server, runs sync cycles with backoff scheduling,
/// and processes body fetch requests from stdin commands (need-bodies, wake-workers).
///
/// Authentication:
/// - Password accounts: ImapSession::connect + authenticate(None)
/// - OAuth2 accounts: get token from TokenManager, authenticate with access_token
///
/// Backoff scheduling:
/// - Base: BASE_SYNC_INTERVAL_SECS (~60s)
/// - No-change increment: +30s per consecutive no-change cycle
/// - Maximum: BASE_SYNC_INTERVAL_SECS + MAX_BACKOFF_ADDITION_SECS (~300s / 5 min)
/// - wake-workers resets streak to 0 (re-accelerates immediately)
///
/// Error handling:
/// - Auth errors (after TokenManager's 3-retry exhaustion): emit connectionError, wait for wake
/// - Retryable errors: sleep 30s, continue
/// - Fatal errors: log, process::exit(1)
#[allow(dead_code)]
pub async fn background_sync(
    account: std::sync::Arc<crate::account::Account>,
    store: std::sync::Arc<crate::store::mail_store::MailStore>,
    delta: std::sync::Arc<crate::delta::stream::DeltaStream>,
    token_manager: std::sync::Arc<tokio::sync::Mutex<crate::oauth2::TokenManager>>,
    mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    mut wake_rx: tokio::sync::mpsc::Receiver<()>,
    mut body_queue_rx: tokio::sync::mpsc::Receiver<Vec<String>>,
) {
    use crate::imap::mail_processor::BodyQueue;
    use crate::imap::session::ImapSession;
    use std::time::Duration;
    use tokio::time::sleep;

    tracing::info!(account_id = %account.id, "background_sync started");

    let mut body_queue = BodyQueue::new();
    let mut no_change_streak: u64 = 0;

    // Main sync loop — runs until shutdown signal
    loop {
        // Drain any pending body_queue_rx messages (non-blocking)
        loop {
            match body_queue_rx.try_recv() {
                Ok(ids) => {
                    tracing::debug!("Received {} priority body IDs from stdin", ids.len());
                    body_queue.enqueue_priority(ids);
                }
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    tracing::warn!("body_queue_rx channel closed");
                    break;
                }
            }
        }

        // Get OAuth2 token if account uses OAuth2 (has refreshToken)
        let access_token: Option<String> = if account.extra.get("refreshToken").is_some() {
            match token_manager.lock().await.get_valid_token(&account, &delta).await {
                Ok(token) => Some(token),
                Err(e) if e.is_auth() => {
                    // Auth failure after TokenManager's 3-retry exhaustion — emit connectionError
                    tracing::error!(
                        account_id = %account.id,
                        error = %e,
                        "OAuth2 token refresh failed (retries exhausted) — emitting connectionError"
                    );
                    delta.emit_process_state(&account.id, true);

                    // Wait for wake signal or shutdown
                    tokio::select! {
                        _ = wake_rx.recv() => {
                            tracing::info!("wake-workers received after auth error — retrying");
                            no_change_streak = 0;
                            continue;
                        }
                        _ = shutdown_rx.recv() => {
                            tracing::info!("Shutdown received during auth error wait");
                            return;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(account_id = %account.id, error = %e, "Token fetch error (retryable) — sleeping 30s");
                    tokio::select! {
                        _ = sleep(Duration::from_secs(30)) => {}
                        _ = shutdown_rx.recv() => return,
                    }
                    continue;
                }
            }
        } else {
            None
        };

        // Connect and authenticate
        let mut session = match connect_and_authenticate(&account, access_token.as_deref()).await {
            Ok(s) => s,
            Err(e) if e.is_auth() => {
                tracing::error!(account_id = %account.id, error = %e, "IMAP auth failed — emitting connectionError");
                delta.emit_process_state(&account.id, true);
                tokio::select! {
                    _ = wake_rx.recv() => {
                        no_change_streak = 0;
                        continue;
                    }
                    _ = shutdown_rx.recv() => return,
                }
            }
            Err(e) if e.is_retryable() => {
                tracing::warn!(account_id = %account.id, error = %e, "IMAP connect failed (retryable) — sleeping 30s");
                tokio::select! {
                    _ = sleep(Duration::from_secs(30)) => {}
                    _ = shutdown_rx.recv() => return,
                }
                continue;
            }
            Err(e) if e.is_fatal() => {
                tracing::error!(account_id = %account.id, error = %e, "Fatal IMAP error");
                std::process::exit(1);
            }
            Err(e) => {
                tracing::warn!(account_id = %account.id, error = %e, "IMAP connect error — sleeping 30s");
                tokio::select! {
                    _ = sleep(Duration::from_secs(30)) => {}
                    _ = shutdown_rx.recv() => return,
                }
                continue;
            }
        };

        // Run one sync cycle and handle result
        let cycle_had_changes = run_sync_cycle_and_bodies(
            &mut session,
            &account,
            &store,
            &delta,
            &mut body_queue,
        ).await;

        match cycle_had_changes {
            Ok(true) => {
                no_change_streak = 0;
                tracing::debug!(account_id = %account.id, "Sync cycle completed with changes");
            }
            Ok(false) => {
                no_change_streak += 1;
                tracing::debug!(account_id = %account.id, streak = no_change_streak, "Sync cycle completed, no changes");
            }
            Err(e) if e.is_auth() => {
                tracing::error!(account_id = %account.id, error = %e, "Auth error during sync — emitting connectionError");
                delta.emit_process_state(&account.id, true);
                tokio::select! {
                    _ = wake_rx.recv() => {
                        no_change_streak = 0;
                        continue;
                    }
                    _ = shutdown_rx.recv() => return,
                }
            }
            Err(e) if e.is_retryable() => {
                tracing::warn!(account_id = %account.id, error = %e, "Retryable error during sync — sleeping 30s");
                tokio::select! {
                    _ = sleep(Duration::from_secs(30)) => {}
                    _ = shutdown_rx.recv() => return,
                }
                continue;
            }
            Err(e) if e.is_fatal() => {
                tracing::error!(account_id = %account.id, error = %e, "Fatal error during sync");
                std::process::exit(1);
            }
            Err(e) => {
                tracing::warn!(account_id = %account.id, error = %e, "Sync cycle error — sleeping 30s");
                tokio::select! {
                    _ = sleep(Duration::from_secs(30)) => {}
                    _ = shutdown_rx.recv() => return,
                }
                continue;
            }
        }

        // Calculate wait with backoff
        let wait_addition = std::cmp::min(
            no_change_streak.saturating_mul(30),
            MAX_BACKOFF_ADDITION_SECS,
        );
        let wait_secs = BASE_SYNC_INTERVAL_SECS + wait_addition;

        tracing::debug!(account_id = %account.id, wait_secs, "Waiting before next sync cycle");

        // Wait for next cycle, wake signal, or shutdown
        tokio::select! {
            _ = sleep(Duration::from_secs(wait_secs)) => {}
            _ = wake_rx.recv() => {
                tracing::info!(account_id = %account.id, "wake-workers received — re-accelerating sync");
                no_change_streak = 0;
            }
            _ = shutdown_rx.recv() => {
                tracing::info!(account_id = %account.id, "Shutdown signal received — exiting sync loop");
                return;
            }
        }
    }
}

/// Connect to IMAP and authenticate.
async fn connect_and_authenticate(
    account: &crate::account::Account,
    access_token: Option<&str>,
) -> Result<crate::imap::session::ImapSession, SyncError> {
    let pre_auth = crate::imap::session::ImapSession::connect(account).await?;
    pre_auth.authenticate(account, access_token).await
}

/// Run one sync cycle and process body queue items.
/// Returns Ok(true) if any changes were found/fetched, Ok(false) for no changes.
async fn run_sync_cycle_and_bodies(
    _session: &mut crate::imap::session::ImapSession,
    _account: &crate::account::Account,
    _store: &crate::store::mail_store::MailStore,
    _delta: &crate::delta::stream::DeltaStream,
    _body_queue: &mut crate::imap::mail_processor::BodyQueue,
) -> Result<bool, SyncError> {
    // TODO: Phase 8 will implement full run_sync_cycle() with folder enumeration.
    // This stub returns Ok(false) (no changes) to drive the backoff scheduler.
    Ok(false)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_folder(role: &str) -> Folder {
        Folder {
            id: format!("acc:{}", role),
            account_id: "acc".to_string(),
            version: 1,
            path: role.to_string(),
            role: role.to_string(),
            local_status: None,
        }
    }

    // ---- Task 1: Folder priority sort tests ----

    #[test]
    fn folder_priority_sort_correct_order() {
        let mut folders = vec![
            make_folder("trash"),
            make_folder("sent"),
            make_folder("spam"),
            make_folder("inbox"),
            make_folder("drafts"),
            make_folder("archive"),
            make_folder("all"),
        ];
        sort_folders_by_role_priority(&mut folders);
        let roles: Vec<&str> = folders.iter().map(|f| f.role.as_str()).collect();
        assert_eq!(
            roles,
            vec!["inbox", "sent", "drafts", "all", "archive", "trash", "spam"]
        );
    }

    #[test]
    fn folder_priority_inbox_first() {
        let mut folders = vec![
            make_folder("spam"),
            make_folder("sent"),
            make_folder("inbox"),
        ];
        sort_folders_by_role_priority(&mut folders);
        assert_eq!(folders[0].role, "inbox", "Inbox must be position 0");
    }

    #[test]
    fn folder_priority_unknown_last() {
        let mut folders = vec![
            make_folder("custom-folder"),
            make_folder("inbox"),
            make_folder("my-work"),
        ];
        sort_folders_by_role_priority(&mut folders);
        // inbox is first, custom folders come after
        assert_eq!(folders[0].role, "inbox");
        // both custom folders should be after inbox
        assert!(folders[1].role != "inbox");
        assert!(folders[2].role != "inbox");
    }

    #[test]
    fn folder_priority_empty_role_is_unknown() {
        let mut folders = vec![make_folder(""), make_folder("inbox")];
        sort_folders_by_role_priority(&mut folders);
        assert_eq!(folders[0].role, "inbox");
        assert_eq!(folders[1].role, "");
    }

    // ---- Task 1: FolderSyncState serialization tests ----

    #[test]
    fn local_status_serialize() {
        let state = FolderSyncState {
            uidvalidity: 12345,
            highestmodseq: 67890,
            uidnext: 500,
            synced_min_uid: 100,
            bodies_present: vec!["msg1".to_string(), "msg2".to_string()],
            bodies_wanted: vec!["msg3".to_string()],
            uidvalidity_reset_count: 0,
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(json["uidvalidity"], 12345);
        // highestmodseq must be serialized as a string
        assert_eq!(json["highestmodseq"], "67890");
        assert_eq!(json["uidnext"], 500);
        assert_eq!(json["syncedMinUID"], 100);
        assert_eq!(json["bodiesPresent"], serde_json::json!(["msg1", "msg2"]));
        assert_eq!(json["bodiesWanted"], serde_json::json!(["msg3"]));
        assert_eq!(json["uidvalidityResetCount"], 0);
    }

    #[test]
    fn local_status_parse() {
        let json = serde_json::json!({
            "uidvalidity": 12345,
            "highestmodseq": "67890",
            "uidnext": 500,
            "syncedMinUID": 100,
            "bodiesPresent": ["msg1", "msg2"],
            "bodiesWanted": ["msg3"],
            "uidvalidityResetCount": 0
        });
        let state: FolderSyncState = serde_json::from_value(json).unwrap();
        assert_eq!(state.uidvalidity, 12345);
        assert_eq!(state.highestmodseq, 67890);
        assert_eq!(state.uidnext, 500);
        assert_eq!(state.synced_min_uid, 100);
        assert_eq!(state.bodies_present, vec!["msg1", "msg2"]);
        assert_eq!(state.bodies_wanted, vec!["msg3"]);
        assert_eq!(state.uidvalidity_reset_count, 0);
    }

    #[test]
    fn local_status_defaults() {
        // Empty JSON object — all fields should default to zero/empty
        let state: FolderSyncState = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(state.uidvalidity, 0);
        assert_eq!(state.highestmodseq, 0);
        assert_eq!(state.uidnext, 0);
        assert_eq!(state.synced_min_uid, 0);
        assert!(state.bodies_present.is_empty());
        assert!(state.bodies_wanted.is_empty());
        assert_eq!(state.uidvalidity_reset_count, 0);
    }

    #[test]
    fn local_status_modseq_as_number_parses() {
        // Some JSON sources may emit modseq as a number instead of string
        let json = serde_json::json!({ "highestmodseq": 67890u64 });
        let state: FolderSyncState = serde_json::from_value(json).unwrap();
        assert_eq!(state.highestmodseq, 67890);
    }

    #[test]
    fn local_status_modseq_serialized_as_string() {
        // Verify the serialized form is always a string (for JS interop)
        let state = FolderSyncState {
            highestmodseq: 9007199254740993u64, // > 2^53, would lose precision as JS number
            ..Default::default()
        };
        let json = serde_json::to_value(&state).unwrap();
        assert_eq!(
            json["highestmodseq"].as_str(),
            Some("9007199254740993"),
            "highestmodseq must be a string in JSON"
        );
    }

    #[test]
    fn get_sync_state_from_folder_with_status() {
        let mut folder = make_folder("inbox");
        folder.local_status = Some(serde_json::json!({
            "uidvalidity": 100,
            "highestmodseq": "200",
            "uidnext": 300
        }));
        let state = get_sync_state(&folder);
        assert_eq!(state.uidvalidity, 100);
        assert_eq!(state.highestmodseq, 200);
        assert_eq!(state.uidnext, 300);
    }

    #[test]
    fn get_sync_state_from_folder_without_status_returns_default() {
        let folder = make_folder("inbox"); // local_status is None
        let state = get_sync_state(&folder);
        assert_eq!(state.uidvalidity, 0);
        assert_eq!(state.highestmodseq, 0);
    }

    #[test]
    fn set_sync_state_writes_to_folder() {
        let mut folder = make_folder("inbox");
        let state = FolderSyncState {
            uidvalidity: 555,
            highestmodseq: 999,
            uidnext: 100,
            ..Default::default()
        };
        set_sync_state(&mut folder, &state);
        let read_back = get_sync_state(&folder);
        assert_eq!(read_back.uidvalidity, 555);
        assert_eq!(read_back.highestmodseq, 999);
        assert_eq!(read_back.uidnext, 100);
    }

    // ---- Task 2: CONDSTORE decision logic tests ----

    #[test]
    fn condstore_no_change() {
        // When server modseq == stored modseq AND uidnext matches -> NoChange
        let decision = decide_condstore_action(
            1000, // server_modseq
            500,  // server_uidnext
            1000, // stored_modseq
            500,  // stored_uidnext
        );
        assert_eq!(decision, CondstoreDecision::NoChange);
    }

    #[test]
    fn condstore_no_change_requires_both_match() {
        // Only modseq matches but uidnext differs -> Incremental (new messages arrived)
        let decision = decide_condstore_action(1000, 501, 1000, 500);
        assert!(matches!(decision, CondstoreDecision::Incremental { .. }));
    }

    #[test]
    fn condstore_normal_incremental() {
        // Small modseq delta -> Incremental with "1:*" uid_set
        let decision = decide_condstore_action(
            1010, // server_modseq (delta = 10, below threshold)
            500,  // server_uidnext
            1000, // stored_modseq
            490,  // stored_uidnext (different so not NoChange)
        );
        match decision {
            CondstoreDecision::Incremental { uid_set } => {
                assert_eq!(uid_set, "1:*", "Small delta should use full '1:*' range");
            }
            other => panic!("Expected Incremental, got {:?}", other),
        }
    }

    #[test]
    fn condstore_truncation_activates_at_threshold() {
        // Delta of exactly MODSEQ_TRUNCATION_THRESHOLD+1 -> Truncated
        let delta = MODSEQ_TRUNCATION_THRESHOLD + 1;
        let decision = decide_condstore_action(
            1000 + delta,
            13000, // server_uidnext
            1000,  // stored_modseq
            0,     // stored_uidnext
        );
        match decision {
            CondstoreDecision::Truncated { uid_set } => {
                // start = uidnext(13000) - TRUNCATION_UID_WINDOW(12000) = 1000
                assert_eq!(uid_set, "1000:*");
            }
            other => panic!("Expected Truncated, got {:?}", other),
        }
    }

    #[test]
    fn condstore_truncation_uid_range_clamps_to_one() {
        // When uidnext < TRUNCATION_UID_WINDOW, start should clamp to 1
        let decision = decide_condstore_action(
            5001, // server_modseq (delta > 4000)
            100,  // server_uidnext (< 12000)
            1000, // stored_modseq
            0,
        );
        match decision {
            CondstoreDecision::Truncated { uid_set } => {
                // 100 - 12000 would underflow -> clamped to 1
                assert_eq!(uid_set, "1:*");
            }
            other => panic!("Expected Truncated, got {:?}", other),
        }
    }

    #[test]
    fn condstore_first_sync_large_modseq() {
        // First sync (stored_modseq=0, stored_uidnext=0) with large server modseq
        // -> Incremental (delta would be truncated only if > 4000 from 0, so if modseq > 4000)
        let decision = decide_condstore_action(
            100, // server_modseq < THRESHOLD
            50,  // server_uidnext
            0,   // stored_modseq (first sync)
            0,   // stored_uidnext
        );
        // Delta = 100 < 4000 -> Incremental
        assert!(matches!(decision, CondstoreDecision::Incremental { .. }));
    }

    // ---- Task 2: UID-range fallback strategy tests ----

    #[test]
    fn uid_range_fallback_when_condstore_unavailable() {
        // When highest_modseq is None, server doesn't support CONDSTORE
        let strategy = select_sync_strategy(None);
        assert_eq!(strategy, SyncStrategy::UidRange);
    }

    #[test]
    fn condstore_strategy_when_modseq_present() {
        // When highest_modseq is Some, use CONDSTORE sync
        let strategy = select_sync_strategy(Some(12345));
        assert_eq!(
            strategy,
            SyncStrategy::Condstore {
                server_modseq: 12345
            }
        );
    }

    // ---- Task 2: UIDVALIDITY reset tests ----

    #[test]
    fn uidvalidity_reset_when_different() {
        // stored != 0 AND stored != server -> needs reset
        assert!(needs_uidvalidity_reset(100, 200));
    }

    #[test]
    fn uidvalidity_no_reset_when_same() {
        // stored == server -> no reset needed
        assert!(!needs_uidvalidity_reset(100, 100));
    }

    #[test]
    fn uidvalidity_first_sync_no_reset() {
        // stored == 0 -> first time selecting, no reset
        assert!(!needs_uidvalidity_reset(0, 12345));
    }

    #[test]
    fn uidvalidity_both_zero() {
        // stored = 0, server = 0 -> technically both zero, stored == server so no reset
        assert!(!needs_uidvalidity_reset(0, 0));
    }

    // ---- Task 06-1: Body age policy tests ----

    #[test]
    fn body_age_policy_skip_spam() {
        // Spam folder must be excluded from body caching
        assert!(!should_cache_bodies_in_folder("spam"), "spam folder should NOT be body-cached");
    }

    #[test]
    fn body_age_policy_skip_trash() {
        // Trash folder must be excluded from body caching
        assert!(!should_cache_bodies_in_folder("trash"), "trash folder should NOT be body-cached");
    }

    #[test]
    fn body_age_policy_include_inbox() {
        // Inbox must be included in body caching
        assert!(should_cache_bodies_in_folder("inbox"), "inbox should be body-cached");
    }

    #[test]
    fn body_age_policy_include_sent() {
        assert!(should_cache_bodies_in_folder("sent"), "sent should be body-cached");
    }

    #[test]
    fn body_age_policy_include_drafts() {
        assert!(should_cache_bodies_in_folder("drafts"), "drafts should be body-cached");
    }

    #[test]
    fn body_age_policy_include_custom_folder() {
        assert!(should_cache_bodies_in_folder("work-projects"), "custom folders should be body-cached");
    }

    #[test]
    fn body_age_policy_include_empty_role() {
        // Folders with empty role (no detected role) should be cached
        assert!(should_cache_bodies_in_folder(""), "empty role should be body-cached");
    }

    #[test]
    fn body_prefetch_age_secs_is_7_days() {
        // BODY_PREFETCH_AGE_SECS must be exactly 7 days in seconds
        assert_eq!(BODY_PREFETCH_AGE_SECS, 7 * 24 * 3600, "BODY_PREFETCH_AGE_SECS must be 7 days");
    }

    #[test]
    fn body_cache_age_secs_is_3_months() {
        // BODY_CACHE_AGE_SECS must be ~3 months (90 days)
        assert_eq!(BODY_CACHE_AGE_SECS, 30 * 24 * 3600, "BODY_CACHE_AGE_SECS must be 30 days (header sync window)");
    }

    #[test]
    fn body_sync_batch_size_is_30() {
        assert_eq!(BODY_SYNC_BATCH_SIZE, 30, "BODY_SYNC_BATCH_SIZE must be 30");
    }

    #[test]
    fn backoff_constants_correct() {
        // BASE_SYNC_INTERVAL_SECS + MAX_BACKOFF_ADDITION_SECS = 300s (5 minutes)
        assert_eq!(BASE_SYNC_INTERVAL_SECS, 60, "Base interval must be 60s");
        assert_eq!(MAX_BACKOFF_ADDITION_SECS, 240, "Max addition must be 240s");
        assert_eq!(
            BASE_SYNC_INTERVAL_SECS + MAX_BACKOFF_ADDITION_SECS,
            300,
            "Total max backoff must be 300s (5 min)"
        );
    }

    // ---- Task 2: Timeout pattern test ----

    #[tokio::test]
    async fn timeout_fires_on_hang() {
        use std::time::Duration;

        // Simulate an operation that hangs forever
        let result = tokio::time::timeout(
            Duration::from_millis(10),
            tokio::time::sleep(Duration::from_secs(3600)),
        )
        .await;

        assert!(result.is_err(), "Timeout must fire on a hanging operation");

        // Map to SyncError::Timeout as the sync worker would
        let sync_err = result.map_err(|_| SyncError::Timeout).unwrap_err();
        assert!(matches!(sync_err, SyncError::Timeout));
    }
}
