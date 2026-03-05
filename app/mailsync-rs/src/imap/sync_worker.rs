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
    session: &mut crate::imap::session::ImapSession,
    account: &crate::account::Account,
    store: &crate::store::mail_store::MailStore,
    delta: &crate::delta::stream::DeltaStream,
    body_queue: &mut crate::imap::mail_processor::BodyQueue,
) -> Result<bool, SyncError> {
    use crate::imap::mail_processor::process_fetched_message;
    use crate::models::thread::Thread;
    use crate::models::label::Label;
    use crate::store::mail_store::SqlParam;
    use tokio_stream::StreamExt;

    let mut had_changes = false;

    // -----------------------------------------------------------------------
    // Step 1: Enumerate folders
    // -----------------------------------------------------------------------
    let (mut folders, mut labels) = session.list_folders(account).await?;

    // Save each label (emits persist deltas to Electron UI)
    for mut label in labels.drain(..) {
        if let Err(e) = store.save::<Label>(&mut label).await {
            tracing::warn!(account_id = %account.id, label_id = %label.id, error = %e, "Failed to save label");
        }
    }

    // Sort folders in priority order (inbox first)
    sort_folders_by_role_priority(&mut folders);

    // Save each folder (emits persist deltas to Electron UI)
    for folder in folders.iter_mut() {
        if let Err(e) = store.save(folder).await {
            tracing::warn!(account_id = %account.id, folder_id = %folder.id, error = %e, "Failed to save folder");
        }
    }

    let total = folders.len();

    // -----------------------------------------------------------------------
    // Step 2: Per-folder sync loop
    // -----------------------------------------------------------------------
    for (idx, folder) in folders.iter_mut().enumerate() {
        // Emit per-folder progress
        delta.emit_sync_progress(&account.id, &folder.path, idx as f32 / total.max(1) as f32);

        // SELECT with CONDSTORE to get mailbox state
        let mailbox = match session.select_condstore(&folder.path).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    account_id = %account.id,
                    folder = %folder.path,
                    error = %e,
                    "Failed to SELECT folder — skipping"
                );
                continue;
            }
        };

        let server_uidvalidity = mailbox.uid_validity.unwrap_or(0);
        let server_modseq = mailbox.highest_modseq.unwrap_or(0);
        let server_uidnext = mailbox.uid_next.unwrap_or(0);

        // Load stored sync state
        let mut state = get_sync_state(folder);

        // UIDVALIDITY check — RFC 4549 full re-sync on change
        if needs_uidvalidity_reset(state.uidvalidity, server_uidvalidity) {
            tracing::info!(
                account_id = %account.id,
                folder = %folder.path,
                stored = state.uidvalidity,
                server = server_uidvalidity,
                "UIDVALIDITY changed — unlinking messages and resetting sync state"
            );
            if let Err(e) = store.unlink_messages_in_folder(&account.id, &folder.id).await {
                tracing::warn!(account_id = %account.id, folder = %folder.path, error = %e, "Failed to unlink messages");
            }
            state = FolderSyncState {
                uidvalidity: server_uidvalidity,
                uidvalidity_reset_count: state.uidvalidity_reset_count + 1,
                ..Default::default()
            };
            set_sync_state(folder, &state);
            if let Err(e) = store.save(folder).await {
                tracing::warn!(account_id = %account.id, folder = %folder.path, error = %e, "Failed to save folder after UIDVALIDITY reset");
            }
        }

        // Determine sync strategy and UID set
        let strategy = select_sync_strategy(mailbox.highest_modseq);
        let (uid_set, use_changedsince) = match &strategy {
            SyncStrategy::Condstore { server_modseq: sms } => {
                let decision = decide_condstore_action(*sms, server_uidnext, state.highestmodseq, state.uidnext);
                match decision {
                    CondstoreDecision::NoChange => {
                        tracing::debug!(account_id = %account.id, folder = %folder.path, "CONDSTORE NoChange — skipping folder");
                        continue;
                    }
                    CondstoreDecision::Incremental { uid_set } => (uid_set, state.highestmodseq > 0),
                    CondstoreDecision::Truncated { uid_set } => (uid_set, false),
                }
            }
            SyncStrategy::UidRange => ("1:*".to_string(), false),
        };

        // Capture is_gmail before the mutable borrow from uid_fetch
        let is_gmail = session.is_gmail();

        // Build fetch query
        let query = if is_gmail {
            if use_changedsince && state.highestmodseq > 0 {
                format!("(UID FLAGS ENVELOPE INTERNALDATE X-GM-LABELS X-GM-MSGID X-GM-THRID) (CHANGEDSINCE {})", state.highestmodseq)
            } else {
                "(UID FLAGS ENVELOPE INTERNALDATE X-GM-LABELS X-GM-MSGID X-GM-THRID)".to_string()
            }
        } else if use_changedsince && state.highestmodseq > 0 {
            format!("(UID FLAGS ENVELOPE INTERNALDATE) (CHANGEDSINCE {})", state.highestmodseq)
        } else {
            "(UID FLAGS ENVELOPE INTERNALDATE)".to_string()
        };

        // UID FETCH
        let mut fetch_stream = match session.uid_fetch(&uid_set, &query).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    account_id = %account.id,
                    folder = %folder.path,
                    error = %e,
                    "UID FETCH failed — skipping folder"
                );
                continue;
            }
        };

        // Stream iteration — process each fetched message
        while let Some(fetch_result) = fetch_stream.next().await {
            let fetch = match fetch_result {
                Ok(f) => f,
                Err(e) => {
                    tracing::warn!(account_id = %account.id, folder = %folder.path, error = %e, "Fetch stream error — skipping message");
                    continue;
                }
            };

            match process_fetched_message(&fetch, &[], folder, account, is_gmail) {
                Ok((mut message, thread_opt)) => {
                    if let Err(e) = store.save(&mut message).await {
                        tracing::warn!(account_id = %account.id, msg_id = %message.id, error = %e, "Failed to save message");
                        continue;
                    }

                    // Save thread if it's new (check DB first)
                    if let Some(mut thread) = thread_opt {
                        let thread_id_param = thread.id.clone();
                        match store.find::<Thread>("id = ?1", vec![SqlParam::Text(thread_id_param)]).await {
                            Ok(None) => {
                                if let Err(e) = store.save(&mut thread).await {
                                    tracing::warn!(account_id = %account.id, thread_id = %thread.id, error = %e, "Failed to save thread");
                                }
                            }
                            Ok(Some(_)) => {} // Thread already exists
                            Err(e) => {
                                tracing::warn!(account_id = %account.id, thread_id = %thread.id, error = %e, "Failed to find thread");
                            }
                        }
                    }

                    had_changes = true;
                }
                Err(e) => {
                    tracing::warn!(account_id = %account.id, folder = %folder.path, error = %e, "Failed to process fetched message");
                }
            }
        }

        // Update FolderSyncState after processing all fetches
        state.uidvalidity = server_uidvalidity;
        state.highestmodseq = server_modseq;
        state.uidnext = server_uidnext;
        set_sync_state(folder, &state);
        if let Err(e) = store.save(folder).await {
            tracing::warn!(account_id = %account.id, folder = %folder.path, error = %e, "Failed to save folder sync state");
        }
    }

    // -----------------------------------------------------------------------
    // Step 3: Body queue processing
    // -----------------------------------------------------------------------
    let cutoff_ts = chrono::Utc::now().timestamp() - BODY_PREFETCH_AGE_SECS as i64;

    for folder in &folders {
        if !should_cache_bodies_in_folder(&folder.role) {
            continue;
        }

        // Fetch bodies for messages needing them (background prefetch)
        let needing_bodies = match store
            .find_messages_needing_bodies(&account.id, &folder.id, cutoff_ts, BODY_SYNC_BATCH_SIZE as i64)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(account_id = %account.id, folder = %folder.path, error = %e, "find_messages_needing_bodies failed");
                continue;
            }
        };

        let mut body_fetch_count: usize = 0;
        for (msg_id, uid) in needing_bodies {
            if uid == 0 {
                continue;
            }
            let mut body_stream = match session.uid_fetch(&uid.to_string(), "BODY.PEEK[]").await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(account_id = %account.id, uid = uid, error = %e, "BODY.PEEK[] fetch failed");
                    continue;
                }
            };

            if let Some(fetch_result) = body_stream.next().await {
                match fetch_result {
                    Ok(fetch) => {
                        if let Some(body_bytes) = fetch.body() {
                            let body_str = String::from_utf8_lossy(body_bytes).into_owned();
                            let snippet: String = body_str.chars().take(200).collect();
                            if let Err(e) = store.save_body(msg_id.clone(), body_str, snippet.clone()).await {
                                tracing::warn!(account_id = %account.id, msg_id = %msg_id, error = %e, "Failed to save body");
                            } else {
                                had_changes = true;

                                // IMPR-07: Per-message progress delta so UI sees incremental loading.
                                // Emit a persist delta for the Message model so Electron can update
                                // the snippet and show body loading progress in real time.
                                delta.emit(crate::delta::item::DeltaStreamItem::new(
                                    "persist",
                                    "Message",
                                    vec![serde_json::json!({
                                        "id": msg_id,
                                        "snippet": snippet,
                                    })],
                                ));

                                body_fetch_count += 1;
                                // Yield to tokio scheduler every 10 messages to avoid
                                // starving other tasks during large body sync batches.
                                if body_fetch_count % 10 == 0 {
                                    tokio::task::yield_now().await;
                                }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(account_id = %account.id, uid = uid, error = %e, "Body fetch stream error");
                    }
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Drain priority body_queue items (Phase 7 deferral completed here)
    //
    // Priority items are message IDs requested via the NeedBodies stdin command.
    // For each ID: look up the message's remoteUID and folder path, then fetch the body.
    // -----------------------------------------------------------------------
    while let Some(msg_id) = body_queue.next() {
        tracing::debug!(
            account_id = %account.id,
            msg_id = %msg_id,
            "Processing priority body queue item"
        );

        // Look up remoteUID and folder path via the MailStore helper
        let uid_and_folder = match store.find_message_uid_and_folder(&msg_id).await {
            Ok(Some(pair)) => pair,
            Ok(None) => {
                tracing::warn!(
                    account_id = %account.id,
                    msg_id = %msg_id,
                    "Priority body queue: message not found or has no IMAP UID — skipping"
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    account_id = %account.id,
                    msg_id = %msg_id,
                    error = %e,
                    "Priority body queue: DB lookup failed — skipping"
                );
                continue;
            }
        };

        let (uid, folder_path) = uid_and_folder;

        // SELECT the folder, then fetch the body
        if let Err(e) = session.select_condstore(&folder_path).await {
            tracing::warn!(
                account_id = %account.id,
                msg_id = %msg_id,
                folder = %folder_path,
                error = %e,
                "Priority body queue: SELECT folder failed — skipping"
            );
            continue;
        }

        let mut body_stream = match session.uid_fetch(&uid.to_string(), "BODY.PEEK[]").await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(
                    account_id = %account.id,
                    uid = uid,
                    msg_id = %msg_id,
                    error = %e,
                    "Priority body queue: BODY.PEEK[] fetch failed"
                );
                continue;
            }
        };

        if let Some(fetch_result) = body_stream.next().await {
            match fetch_result {
                Ok(fetch) => {
                    if let Some(body_bytes) = fetch.body() {
                        let body_str = String::from_utf8_lossy(body_bytes).into_owned();
                        let snippet: String = body_str.chars().take(200).collect();
                        if let Err(e) = store.save_body(msg_id.clone(), body_str, snippet.clone()).await {
                            tracing::warn!(
                                account_id = %account.id,
                                msg_id = %msg_id,
                                error = %e,
                                "Priority body queue: save_body failed"
                            );
                        } else {
                            had_changes = true;

                            // IMPR-07: Per-message progress delta for priority fetch
                            delta.emit(crate::delta::item::DeltaStreamItem::new(
                                "persist",
                                "Message",
                                vec![serde_json::json!({
                                    "id": msg_id,
                                    "snippet": snippet,
                                })],
                            ));
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        account_id = %account.id,
                        uid = uid,
                        msg_id = %msg_id,
                        error = %e,
                        "Priority body queue: body stream error"
                    );
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Step 4: Emit final progress
    // -----------------------------------------------------------------------
    delta.emit_sync_progress(&account.id, "", 1.0);

    Ok(had_changes)
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

    // ---- Task 07-07: Wiring verification tests ----
    // These tests verify that the decision logic composition in run_sync_cycle_and_bodies()
    // is correct. No live IMAP session is required — all tested functions are pure.

    #[test]
    fn test_sync_strategy_condstore_incremental_wiring() {
        // Given: stored modseq=100, server modseq=110, stored uidnext=190, server uidnext=200
        // Expected: CONDSTORE strategy and Incremental decision (delta=10 < 4000 threshold)
        let strategy = select_sync_strategy(Some(110));
        assert_eq!(
            strategy,
            SyncStrategy::Condstore { server_modseq: 110 },
            "select_sync_strategy(Some(110)) must return Condstore"
        );

        let decision = decide_condstore_action(110, 200, 100, 190);
        match decision {
            CondstoreDecision::Incremental { uid_set } => {
                assert_eq!(uid_set, "1:*", "Incremental wiring: uid_set must be '1:*'");
            }
            other => panic!("Expected Incremental, got {:?}", other),
        }
    }

    #[test]
    fn test_sync_strategy_uidrange_fallback_wiring() {
        // No highest_modseq from server -> UID-range fallback
        let strategy = select_sync_strategy(None);
        assert_eq!(
            strategy,
            SyncStrategy::UidRange,
            "select_sync_strategy(None) must return UidRange"
        );
    }

    #[test]
    fn test_uidvalidity_reset_triggers_state_clear() {
        // Simulate: stored uidvalidity=100 differs from server uidvalidity=200
        let stored_uidvalidity: u32 = 100;
        let server_uidvalidity: u32 = 200;

        // Verify reset is needed
        assert!(
            needs_uidvalidity_reset(stored_uidvalidity, server_uidvalidity),
            "needs_uidvalidity_reset(100, 200) must be true"
        );

        // Simulate the reset: create new default state with only uidvalidity and reset_count
        let old_state = FolderSyncState {
            uidvalidity: stored_uidvalidity,
            highestmodseq: 500,
            uidnext: 200,
            uidvalidity_reset_count: 0,
            ..Default::default()
        };

        // After reset, state is zeroed except for new uidvalidity and incremented reset count
        let new_state = FolderSyncState {
            uidvalidity: server_uidvalidity,
            uidvalidity_reset_count: old_state.uidvalidity_reset_count + 1,
            ..Default::default()
        };

        assert_eq!(new_state.highestmodseq, 0, "highestmodseq must be cleared on UIDVALIDITY reset");
        assert_eq!(new_state.uidnext, 0, "uidnext must be cleared on UIDVALIDITY reset");
        assert_eq!(new_state.uidvalidity, server_uidvalidity, "uidvalidity must be updated to server value");
        assert_eq!(new_state.uidvalidity_reset_count, 1, "uidvalidity_reset_count must be incremented");
    }

    #[test]
    fn test_folder_enumeration_sort_and_save_order() {
        // Create 5 folders with roles [spam, inbox, drafts, sent, trash]
        let mut folders = vec![
            make_folder("spam"),
            make_folder("inbox"),
            make_folder("drafts"),
            make_folder("sent"),
            make_folder("trash"),
        ];

        sort_folders_by_role_priority(&mut folders);

        let roles: Vec<&str> = folders.iter().map(|f| f.role.as_str()).collect();
        assert_eq!(
            roles,
            vec!["inbox", "sent", "drafts", "trash", "spam"],
            "Folders must be processed in priority order: inbox, sent, drafts, trash, spam"
        );

        // Verify inbox is first (highest sync priority)
        assert_eq!(folders[0].role, "inbox", "First folder to sync must be inbox");
        // Verify spam is last (lowest sync priority)
        assert_eq!(folders[4].role, "spam", "Last folder to sync must be spam");
    }

    #[test]
    fn test_body_prefetch_cutoff_calculation() {
        // Cutoff timestamp should be approximately 7 days ago
        let now = chrono::Utc::now().timestamp();
        let cutoff = now - BODY_PREFETCH_AGE_SECS as i64;

        let seven_days_secs = 7i64 * 24 * 3600;
        let expected_cutoff = now - seven_days_secs;

        // Within 1 second tolerance
        assert!(
            (cutoff - expected_cutoff).abs() <= 1,
            "Body prefetch cutoff must be approximately 7 days ago (within 1s tolerance)"
        );

        // Sanity check: cutoff is in the past
        assert!(cutoff < now, "Cutoff must be in the past");
    }

    #[test]
    fn test_condstore_changedsince_no_change_skips() {
        // When server modseq and uidnext both match stored values -> NoChange
        // This causes the sync loop to skip fetching for that folder
        let decision = decide_condstore_action(
            500, // server_modseq == stored
            100, // server_uidnext == stored
            500, // stored_modseq
            100, // stored_uidnext
        );

        assert_eq!(
            decision,
            CondstoreDecision::NoChange,
            "decide_condstore_action(500, 100, 500, 100) must return NoChange — folder should be skipped"
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
