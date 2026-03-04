# Phase 7: IMAP Background Sync Worker - Research

**Researched:** 2026-03-04 (updated from 2026-03-02 original; deep-dive rounds already resolved)
**Domain:** async-imap CONDSTORE/UID-range sync, folder role detection, Gmail extensions, OAuth2 token management, body caching, per-operation timeouts, SyncError classification
**Confidence:** HIGH (async-imap Session and Fetch APIs verified via docs.rs and GitHub source; C++ SyncWorker.cpp read directly; RFC 7162/RFC 4549 verified; imap-proto NameAttribute enum verified; Gmail IMAP extension docs checked; all three open questions resolved via source inspection)

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Sync cycle strategy:**
- Priority-based folder ordering: Inbox first, then Sent/Drafts, then remaining folders in round-robin
- Fixed interval with backoff between sync cycles: ~60 seconds base, backing off to ~5 minutes when no changes detected. Re-accelerate immediately when `wake-workers` stdin command arrives
- UIDVALIDITY changes trigger silent re-sync — discard cached UIDs and full re-fetch per RFC 4549 with no user notification (internal consistency operation)
- CONDSTORE modseq-based incremental sync as primary strategy; UID range sync as fallback for servers without CONDSTORE capability

**OAuth2 token management:**
- Credentials read from Account JSON `extra` field on stdin during handshake — no separate credential store
- Sync worker handles token refresh HTTP requests directly (reqwest + rustls) — self-contained, matches C++ XOAuth2TokenManager pattern
- On refresh, emit `ProcessAccountSecretsUpdated` delta so Electron persists the new token
- On refresh failure: retry 2-3 times with exponential backoff, then emit ProcessState with connectionError=true and stop syncing. Resume when `wake-workers` arrives
- Check token expiry within 5-minute buffer before every IMAP authenticate
- Support Gmail + Microsoft OAuth2 (XOAUTH2) from the start — covers ~80% of OAuth users

**Gmail-specific behaviors:**
- Hardcoded folder whitelist: only sync INBOX, [Gmail]/All Mail, [Gmail]/Trash, [Gmail]/Spam, [Gmail]/Drafts, [Gmail]/Sent Mail. All other Gmail virtual folders hidden
- X-GM-LABELS stored as Label records in the database via MessageLabel join table — Electron UI already renders labels from Label model
- X-GM-THRID used as primary Thread record ID for Gmail accounts — gives exact Gmail threading behavior. Non-Gmail accounts fall back to References/In-Reply-To header-based threading
- X-GM-MSGID stored on Message.gMsgId field for stable message identity
- Gmail detection via `account.provider == "gmail"` from handshake JSON — set by onboarding providerForEmail()

**Body caching and need-bodies:**
- FIFO with dedup for need-bodies request prioritization — process in order received, deduplicate same message IDs
- Background sync pre-fetches bodies for messages from the last 7 days automatically; older messages header-only until need-bodies arrives
- Message bodies stored in SQLite data blob (Message record's `body` and `snippet` fields) — single source of truth, matches C++ behavior
- Body fetch progress reported via ProcessState delta with sync progress field — OnlineStatusStore consumes this in Electron UI
- Per-folder age policy: 3 months for header sync, 7 days for automatic body pre-fetch

### Claude's Discretion

- Initial sync depth strategy (how many months of headers to fetch on first connect)
- Exact CONDSTORE modseq tracking and storage mechanism
- IMAP connection pooling / session reuse approach
- Exact backoff curve values for sync interval and OAuth retry
- Header-based threading algorithm for non-Gmail accounts (References/In-Reply-To matching)
- IMAP per-operation timeout values
- Error classification logic (auth vs TLS vs network vs server)
- mail-parser MIME parsing integration details
- Stable message ID generation algorithm (SHA-256 + Base58 from C++)
- Folder role detection two-pass algorithm (RFC 6154 special-use flags first, name-based fallback)

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope
</user_constraints>

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| ISYN-01 | IMAP folder enumeration via LIST with role detection (Inbox, Sent, Drafts, Trash, Spam, Archive) | Two-pass role assignment documented: RFC 6154 special-use flags first, name-based fallback second; imap-proto NameAttribute enum variants confirmed |
| ISYN-02 | IMAP incremental sync via CONDSTORE (modseq-based change detection) | `select_condstore()` returns `Mailbox` with `highest_modseq: Option<u64>`; raw `UID FETCH uid_set (FLAGS) (CHANGEDSINCE modseq)` pattern documented |
| ISYN-03 | IMAP incremental sync via UID range (fallback for servers without CONDSTORE) | `uid_fetch(uid_set, query)` pattern documented; Mailbox.uid_next and uid_validity fields confirmed; C++ shallow/deep scan pattern mapped to Rust |
| ISYN-04 | UIDVALIDITY change detection triggers full folder re-sync per RFC 4549 | RFC 4549 algorithm confirmed: compare stored vs server UIDVALIDITY, clear local UIDs, full re-fetch; C++ implementation read directly |
| ISYN-05 | Message header sync (FETCH ENVELOPE + BODYSTRUCTURE) with stable ID generation from headers | Fetch.envelope(), Fetch.bodystructure() methods confirmed; C++ SHA-256 + Base58 stable ID algorithm documented; `bs58` crate + `rfc2047-decoder` as correct Rust implementation |
| ISYN-06 | Message body caching with `need-bodies` priority queue and per-folder age policy (3 months header, 7 days body pre-fetch) | C++ maxAgeForBodySync() hardcoded to `24*60*60*30*3` seconds; MessageBody table with placeholder pattern documented; spam/trash excluded from body caching |
| ISYN-07 | Background sync worker iterates folders on 2-10 minute schedule | C++ constants: SHALLOW_SCAN_INTERVAL=120s, DEEP_SCAN_INTERVAL=600s; role-priority folder ordering documented |
| OAUT-01 | OAuth2 token refresh via HTTP token exchange endpoint | C++ XOAuth2TokenManager uses HTTP POST to provider token endpoint; oauth2 crate 5.0 reqwest-backed async refresh documented |
| OAUT-02 | Token expiry check before IMAP authenticate (5-minute buffer window) | C++ checks `expiryDate > time(0) + 60` (60s buffer); requirement spec says 5-minute buffer; expiry field from token response |
| OAUT-03 | Updated token credentials emitted to UI via `ProcessAccountSecretsUpdated` delta | C++ `DeltaStream::sendUpdatedSecrets()` emits JSON; Rust equivalent is emitting `{ type: "persist", modelClass: "ProcessAccountSecretsUpdated", ... }` |
| GMAL-01 | Gmail folder whitelist — only sync INBOX, All Mail, Trash, Spam, Drafts, Sent Mail | C++: only folders with `IMAPFolderFlagAll`, `IMAPFolderFlagSpam`, `IMAPFolderFlagTrash` are synced as folders; others become Labels |
| GMAL-02 | X-GM-LABELS, X-GM-MSGID, X-GM-THRID IMAP extension parsing | `Fetch::gmail_labels()` and `Fetch::gmail_msg_id()` are native async-imap methods; X-GM-THRID uses `AttributeValue::GmailThrId(u64)` from imap-proto — implement `gmail_thread_id()` free function |
| GMAL-03 | Gmail contacts via Google People API (not standard CardDAV) | Phase 9 scope — not implemented in Phase 7 |
| GMAL-04 | Gmail skips IMAP APPEND for Sent folder after SMTP send | Gmail auto-saves sent mail; APPEND causes duplicates; detect Gmail capability and skip APPEND in SendDraftTask remote phase (Phase 8) |
| IMPR-05 | Per-operation timeouts via `tokio::time::timeout()` for all network operations | `tokio::time::timeout(Duration, future)` pattern; recommended durations documented; wraps every await point on IMAP session calls |
| IMPR-06 | Structured `SyncError` enum distinguishing auth/TLS/network/server error classes | Existing `error.rs` has `SyncError` — extend with `is_retryable()`, `is_offline()`, `is_auth()`, `is_fatal()` classification methods |
</phase_requirements>

---

## Summary

Phase 7 implements the background sync worker — the core of the mailsync engine. It translates what the C++ `SyncWorker::syncNow()` and `SyncWorker::syncFoldersAndLabels()` methods do into async Rust using `async-imap` 0.11 (already in mailcore-rs Cargo.toml).

The insertion point is `background_sync_stub` in `app/mailsync-rs/src/modes/sync.rs` (line ~121). The stub already receives `shutdown_rx: broadcast::Receiver<()>` — it becomes a real sync loop that owns an IMAP session, iterates folders per role priority, performs CONDSTORE or UID-range sync, caches bodies, handles OAuth2 token refresh, and applies Gmail-specific behaviors. The `stdin_loop.rs` dispatch function already parses `NeedBodies` and `WakeWorkers` commands — Phase 7 wires these via `tokio::sync::mpsc` channels to the background sync task.

The key architectural reality is that `async-imap 0.11` already provides first-class support for the three primary operations: `select_condstore()` returns a `Mailbox` struct with `highest_modseq: Option<u64>`, `uid_fetch(uid_set, query)` accepts raw IMAP fetch query strings including `(CHANGEDSINCE modseq)`, and the `Fetch` struct exposes `gmail_labels()` and `gmail_msg_id()` as native typed methods. `X-GM-THRID` uses `imap_proto::types::AttributeValue::GmailThrId(u64)` — implement a free function following the same pattern as `gmail_msg_id()`.

Three new crate dependencies are required for Phase 7 that are NOT yet in mailsync-rs's Cargo.toml: `rfc2047-decoder` (RFC 2047 MIME encoded-word decoding for stable ID generation), `bs58` (Base58 encoding matching the C++ bitcoin-alphabet `toBase58()`), and `reqwest`+`oauth2` for OAuth2 HTTP token exchange. The `sha2` and `chrono` crates are also needed.

**Primary recommendation:** Model `imap/sync_worker.rs` directly on the C++ `SyncWorker::syncNow()` flow, replacing mailcore2 calls with async-imap equivalents. The sync loop structure, local-status JSON fields (`uidvalidity`, `highestmodseq`, `uidnext`, `syncedMinUID`, `bodiesPresent`, `bodiesWanted`), and folder role priority ordering must match the C++ implementation exactly.

---

## Standard Stack

### Core (additions to mailsync-rs Cargo.toml — Phase 7 new deps)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `async-imap` | 0.11 (runtime-tokio) | IMAP session, LIST, uid_fetch, select_condstore | Already in mailcore-rs workspace; CONDSTORE and Gmail extensions confirmed in source |
| `imap-proto` | 0.16.x | Low-level IMAP response parsing (NameAttribute for role detection, AttributeValue::GmailThrId) | Underlying parser used by async-imap; GmailThrId variant confirmed in types.rs |
| `tokio-rustls` | 0.26.x | TLS stream for IMAP connections | No OpenSSL; same as mailcore-rs |
| `rustls-platform-verifier` | 0.6.x | OS trust store certificate validation | Same as prior phases |
| `mail-parser` | 0.11.x | Parse MIME message from BODY[] raw bytes | Zero-copy; 41 charsets; body caching and snippet extraction |
| `ammonia` | 4.1.x | HTML sanitization of message body before storage | Whitelist-based; fixes RUSTSEC-2025-0071 |
| `oauth2` | 5.0.x (reqwest) | OAuth2 access token refresh via HTTP POST | Provider-agnostic RFC 6749; async reqwest-backed |
| `reqwest` | 0.13.x (rustls-tls, json) | HTTP client for OAuth2 token endpoint | Already in mailcore-rs; rustls-backed |
| `sha2` | 0.10.x | SHA-256 hash for stable message ID generation | Replicates C++ picosha2 — must match exactly |
| `bs58` | 0.5.x | Base58 encoding with bitcoin alphabet | Matches C++ `toBase58()` byte-for-byte |
| `rfc2047-decoder` | 1.x | RFC 2047 MIME encoded-word decoding for subjects/message-IDs | Required to match C++ mailcore2 decoded strings before hashing |
| `chrono` | 0.4.x | RFC 2822 date parsing for message ID generation | Required for timestamp extraction from Envelope |
| `base64` | 0.22.x | XOAUTH2 SASL token encoding | Building `user=<u>\x01auth=Bearer <t>\x01\x01` payload |

### Supporting (already in mailsync-rs Cargo.toml)

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio` | 1.x (time, sync, io-util) | `tokio::time::timeout()`, `mpsc` channels, async I/O | Foundation runtime — all IMAP awaits wrapped with timeout |
| `thiserror` | 2.x | `SyncError` enum derivation | Already in error.rs; extend with classification methods |
| `serde` + `serde_json` | 1.x | `ProcessAccountSecretsUpdated` delta emission, folder localStatus JSON | Already in stack |
| `tracing` | 0.1.x | Structured async logging per folder/account span | Already in stack |
| `tokio-rusqlite` | 0.7.x | Database writes (folder status, messages, bodies) | Already in stack; all DB access via call() |
| `indexmap` | 2.x | Ordered map for folder sync order preservation | Already in stack |

**Installation (additions only — add to mailsync-rs Cargo.toml):**

```toml
# Phase 7 new dependencies
async-imap = { version = "0.11", features = ["runtime-tokio"], default-features = false }
imap-proto = "0.16"
tokio-rustls = "0.26"
rustls-platform-verifier = "0.6"
mail-parser = "0.11"
ammonia = "4"
oauth2 = { version = "5", features = ["reqwest"] }
reqwest = { version = "0.13", default-features = false, features = ["rustls-tls", "json"] }
sha2 = "0.10"
bs58 = "0.5"
rfc2047-decoder = "1"
chrono = { version = "0.4", default-features = false, features = ["std"] }
base64 = "0.22"
```

**Note:** All crate versions match what mailcore-rs uses for shared dependencies. Verify with `cargo tree` after adding.

---

## Architecture Patterns

### Recommended Project Structure (Phase 7 new files)

```
app/mailsync-rs/src/
├── imap/
│   ├── mod.rs              # pub use; module declarations
│   ├── session.rs          # ImapSession: connect TLS/STARTTLS, XOAUTH2/password auth
│   ├── sync_worker.rs      # background_sync task: folder iteration, CONDSTORE, body fetch loop
│   └── mail_processor.rs   # parse Fetch -> Message + Thread; stable ID generation
├── oauth2.rs               # TokenManager: expiry check, HTTP refresh, secrets delta emission
├── modes/
│   └── sync.rs             # Replace background_sync_stub with background_sync task
├── stdin_loop.rs           # Wire NeedBodies + WakeWorkers to sync_worker channels
└── error.rs                # Extend SyncError with is_retryable/is_offline/is_auth/is_fatal
```

### Pattern 1: Two-Pass Folder Role Detection

**What:** Assign a role string (`"inbox"`, `"sent"`, `"drafts"`, `"trash"`, `"spam"`, `"archive"`, `"all"`) to each folder returned by `session.list()`. First pass checks RFC 6154 special-use attributes from `imap_proto::types::NameAttribute`. Second pass matches against a name lookup table.

**When to use:** During `sync_folders_and_labels()` called at the start of every `background_sync` cycle.

**Why two passes:** RFC 6154 is not universally supported (older IMAP servers omit it). Name-based fallback covers Courier, Dovecot, and Exchange servers.

**Example:**

```rust
// Source: C++ MailUtils::roleForFolderViaFlags() + roleForFolderViaPath()
use imap_proto::types::NameAttribute;

fn role_for_name_attribute(attr: &NameAttribute) -> Option<&'static str> {
    match attr {
        NameAttribute::All => Some("all"),
        NameAttribute::Sent => Some("sent"),
        NameAttribute::Drafts => Some("drafts"),
        NameAttribute::Junk => Some("spam"),
        NameAttribute::Trash => Some("trash"),
        NameAttribute::Archive => Some("archive"),
        // No Inbox variant in imap-proto NameAttribute — Inbox is detected by name only
        _ => None,
    }
}

fn role_for_folder_via_path(path_lower: &str) -> Option<&'static str> {
    match path_lower {
        "inbox" => Some("inbox"),
        "sent" | "sent mail" | "sent items" | "sent messages"
            | "[gmail]/sent mail" => Some("sent"),
        "drafts" | "draft" | "[gmail]/drafts" => Some("drafts"),
        "trash" | "deleted" | "deleted items" | "deleted messages"
            | "[gmail]/trash" => Some("trash"),
        "spam" | "junk" | "junk mail" | "junk e-mail"
            | "[gmail]/spam" => Some("spam"),
        "archive" | "all mail" | "[gmail]/all mail" => Some("all"),
        _ => None,
    }
}

fn detect_folder_role(name: &async_imap::types::Name, namespace_prefix: &str) -> Option<String> {
    // Pass 1: RFC 6154 special-use attributes
    if let Some(role) = name.attributes().iter().find_map(|attr| role_for_name_attribute(attr)) {
        return Some(role.to_string());
    }
    // Pass 2: Name-based lookup
    let path = name.name();
    let stripped = strip_namespace_prefix(path, namespace_prefix).to_lowercase();
    role_for_folder_via_path(&stripped).map(|r| r.to_string())
}
```

**Gmail-specific:** When `X-GM-EXT-1` is in capabilities, only folders with `NameAttribute::All`, `NameAttribute::Junk`, or `NameAttribute::Trash` become sync targets (Folder objects). All other Gmail folders become Label objects.

### Pattern 2: CONDSTORE Incremental Sync

**What:** After selecting a folder with `select_condstore()`, compare stored `highestmodseq` against `Mailbox.highest_modseq`. If they differ, issue a raw `UID FETCH` with `(CHANGEDSINCE modseq)` modifier to retrieve only changed/new messages.

**When to use:** When `Mailbox.highest_modseq.is_some()` after `select_condstore()`.

**Critical:** The `UID FETCH 1:* (FLAGS ENVELOPE) (CHANGEDSINCE modseq)` command string is passed as the `query` parameter to `session.uid_fetch()`. async-imap does not have a typed API for `CHANGEDSINCE` — it must be included in the raw query string.

**Example:**

```rust
// Source: C++ SyncWorker::syncFolderChangesViaCondstore() + async-imap Session docs
use async_imap::types::Mailbox;
use tokio_stream::StreamExt;

async fn sync_folder_condstore(
    session: &mut async_imap::Session<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin>,
    folder: &mut Folder,
    store: &MailStore,
    delta: &DeltaStream,
) -> Result<(), SyncError> {
    let mailbox: Mailbox = tokio::time::timeout(
        Duration::from_secs(30),
        session.select_condstore(&folder.path),
    ).await.map_err(|_| SyncError::Timeout)??;

    // Detect UIDVALIDITY change (RFC 4549)
    let server_uid_validity = mailbox.uid_validity.unwrap_or(0);
    if folder.local_uid_validity != 0 && folder.local_uid_validity != server_uid_validity {
        return handle_uidvalidity_change(session, folder, store, delta, &mailbox).await;
    }

    let server_modseq = match mailbox.highest_modseq {
        Some(m) => m,
        None => {
            // Server advertises CONDSTORE but returns [NOMODSEQ] for this folder
            return sync_folder_uid_range(session, folder, store, delta, &mailbox).await;
        }
    };

    if server_modseq == folder.local_highest_modseq
        && mailbox.uid_next == Some(folder.local_uid_next)
    {
        return Ok(()); // Nothing changed
    }

    // Truncate modseq range if the delta is enormous (C++ MODSEQ_TRUNCATION_THRESHOLD=4000)
    let uid_range = if server_modseq.saturating_sub(folder.local_highest_modseq) > 4000 {
        let bottom = mailbox.uid_next.unwrap_or(1).saturating_sub(12000).max(1);
        format!("{}:*", bottom)
    } else {
        "1:*".to_string()
    };

    // Raw CONDSTORE fetch — CHANGEDSINCE modifier included in query string
    let query = format!(
        "(UID FLAGS ENVELOPE BODYSTRUCTURE) (CHANGEDSINCE {})",
        folder.local_highest_modseq
    );
    let mut fetch_stream = tokio::time::timeout(
        Duration::from_secs(120),
        session.uid_fetch(&uid_range, &query),
    ).await.map_err(|_| SyncError::Timeout)??;

    while let Some(fetch) = tokio::time::timeout(
        Duration::from_secs(30),
        fetch_stream.next(),
    ).await.map_err(|_| SyncError::Timeout)? {
        let fetch = fetch?;
        process_fetched_message(&fetch, folder, store, delta).await?;
    }

    folder.local_highest_modseq = server_modseq;
    folder.local_uid_next = mailbox.uid_next.unwrap_or(folder.local_uid_next);
    store.save(folder).await?;
    Ok(())
}
```

### Pattern 3: UIDVALIDITY Change Handling (RFC 4549)

**What:** When the server's UIDVALIDITY differs from the stored value, all local UIDs for that folder are invalidated. Discard local message UIDs and full re-sync.

**C++ algorithm (SyncWorker.cpp:366-401):**
1. Set all messages' `remoteUID` to the "UNLINKED" sentinel value
2. Run a full `syncFolderUIDRange(folder, RangeMake(1, UINT64_MAX), false)` to refetch all messages
3. Increment `uidvalidityResetCount` in localStatus
4. Update `uidvalidity`, `uidnext`, `highestmodseq`, `syncedMinUID` to new values
5. Skip to next folder iteration

**Example:**

```rust
// Source: C++ SyncWorker.cpp:366-401
async fn handle_uidvalidity_change(
    session: &mut ImapSession,
    folder: &mut Folder,
    store: &MailStore,
    delta: &DeltaStream,
    mailbox: &Mailbox,
) -> Result<(), SyncError> {
    tracing::warn!(folder = %folder.path, "UIDVALIDITY changed — resetting remote UIDs");

    // Unlink all messages in this folder (clear remoteUID)
    store.unlink_messages_in_folder(&folder.id).await?;

    // Full resync
    sync_folder_uid_range(session, folder, store, delta, mailbox).await?;

    // Update localStatus with new values
    folder.local_uid_validity = mailbox.uid_validity.unwrap_or(0);
    folder.local_uid_next = mailbox.uid_next.unwrap_or(1);
    folder.local_highest_modseq = mailbox.highest_modseq.unwrap_or(0);
    folder.local_synced_min_uid = 1;
    folder.uid_validity_reset_count += 1;

    store.save(folder).await?;
    Ok(())
}
```

### Pattern 4: Stable Message ID Generation (SHA-256 + Base58)

**What:** Generate a stable, cross-folder message ID that survives UID changes and folder moves.

**CRITICAL: This algorithm MUST match the C++ exactly.** Existing deployed Electron databases store message IDs generated by the C++ engine. If the Rust engine generates different IDs for the same message, the UI will show duplicate messages and metadata will be orphaned.

**The C++ algorithm (MailUtils.cpp:630-703, Scheme v1):**

```
input = accountId + "-" + unix_timestamp_str + subject + "-" + sorted_recipients + "-" + messageID
hash = sha256(input)
id = base58_encode(hash[0..30])
```

- `unix_timestamp_str`: from `Date:` header as Unix epoch string. If date is 0 or -1, use `folderPath + ":" + uid` as fallback.
- `sorted_recipients`: To + CC + BCC email addresses (not names), sorted lexicographically, concatenated without separator.
- `messageID`: from `Message-ID:` header. Empty string if header is missing.
- `subject`: fully decoded UTF-8 (mailcore2 decoded RFC 2047 encoded-words before hashing).
- Base58 alphabet: bitcoin alphabet `123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz`

**Rust implementation using `bs58` + `rfc2047-decoder`:**

```rust
// Source: C++ MailUtils::idForMessage() — must produce identical output
use sha2::{Sha256, Digest};

fn decode_mime_header(raw: &[u8]) -> String {
    // Decode RFC 2047 encoded-words exactly as mailcore2 does
    // C++ calls String::stringByDecodingMIMEHeaderValue() which decodes at parse time
    rfc2047_decoder::decode(raw)
        .unwrap_or_else(|_| String::from_utf8_lossy(raw).into_owned())
}

fn id_for_message(
    account_id: &str,
    folder_path: &str,
    uid: u32,
    envelope: &imap_proto::types::Envelope<'_>,
) -> String {
    // Subject: MUST decode RFC 2047 to match C++ decoded value
    let subject = envelope.subject.as_ref()
        .map(|s| decode_mime_header(s))
        .unwrap_or_default();

    // Message-ID: typically ASCII; be defensive and attempt decode
    let message_id = envelope.message_id.as_ref()
        .map(|m| decode_mime_header(m))
        .unwrap_or_default();

    // Recipients: mailbox@host — do NOT RFC 2047 decode address parts
    let mut recipients: Vec<String> = vec![];
    for addr_list in [&envelope.to, &envelope.cc, &envelope.bcc] {
        if let Some(addrs) = addr_list {
            for addr in addrs {
                let mailbox = addr.mailbox.as_ref()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .unwrap_or_default();
                let host = addr.host.as_ref()
                    .map(|h| String::from_utf8_lossy(h).into_owned())
                    .unwrap_or_default();
                if !mailbox.is_empty() && !host.is_empty() {
                    recipients.push(format!("{}@{}", mailbox, host));
                }
            }
        }
    }
    recipients.sort();
    let participants = recipients.join("");

    // Date: parse RFC 2822 to Unix timestamp string
    let timestamp_str = envelope.date.as_ref()
        .and_then(|d| std::str::from_utf8(d).ok())
        .and_then(|s| chrono::DateTime::parse_from_rfc2822(s).ok())
        .map(|dt| dt.timestamp().to_string())
        .unwrap_or_else(|| format!("{}:{}", folder_path, uid));

    // Build src string exactly as C++ MailUtils.cpp:670-699 Scheme 1
    let src = format!(
        "{}-{}{}-{}-{}",
        account_id, timestamp_str, subject, participants, message_id
    );

    let mut hasher = Sha256::new();
    hasher.update(src.as_bytes());
    let hash = hasher.finalize();

    // Encode first 30 bytes as Bitcoin Base58 (matches C++ `toBase58(hash.data(), 30)`)
    bs58::encode(&hash[..30])
        .with_alphabet(bs58::Alphabet::BITCOIN)
        .into_string()
}
```

### Pattern 5: Gmail Extension Attributes in FETCH

**What:** Include Gmail extension attributes in `uid_fetch` query string. Access them via `Fetch` methods.

**Confirmed API (from async-imap source, docs.rs, imap-proto types.rs):**
- `Fetch::gmail_labels()` — `Option<&Vec<Cow<'_, str>>>` — typed method for X-GM-LABELS
- `Fetch::gmail_msg_id()` — `Option<&u64>` — typed method for X-GM-MSGID
- `X-GM-THRID` — `imap_proto::types::AttributeValue::GmailThrId(u64)` — typed variant exists; implement a free function (async-imap 0.11 does not expose it as a method on Fetch)

**X-GM-THRID extractor (RESOLVED — HIGH confidence from imap-proto source):**

```rust
// Source: imap-proto AttributeValue::GmailThrId(u64) confirmed in types.rs
// Pattern mirrors async-imap gmail_msg_id() source exactly
use imap_proto::types::AttributeValue;
use async_imap::imap_proto::Response;

/// Extract X-GM-THRID from an async-imap Fetch response.
/// Requires X-GM-THRID to be included in the uid_fetch query string.
pub fn gmail_thread_id(fetch: &async_imap::types::Fetch) -> Option<u64> {
    // NOTE: fetch.response may be private in async-imap 0.11.
    // If so, add a PR to async-imap adding gmail_thread_id() using this exact pattern.
    // Fallback: parse raw response bytes if internal field is inaccessible.
    if let Response::Fetch(_, attrs) = fetch.response.parsed() {
        attrs.iter()
            .filter_map(|av| match av {
                AttributeValue::GmailThrId(id) => Some(*id),
                _ => None,
            })
            .next()
    } else {
        None
    }
}

// Gmail fetch query — include all three extensions
const GMAIL_FETCH_QUERY: &str =
    "(UID FLAGS ENVELOPE BODYSTRUCTURE X-GM-LABELS X-GM-MSGID X-GM-THRID)";
```

**Gmail folder whitelist enforcement (GMAL-01):**

```rust
// Source: C++ SyncWorker.cpp:659-661
// When IMAP capability string contains "X-GM-EXT-1":
fn is_gmail_sync_folder(name: &async_imap::types::Name) -> bool {
    name.attributes().iter().any(|attr| matches!(
        attr,
        NameAttribute::All | NameAttribute::Junk | NameAttribute::Trash
    ))
}
// All other Gmail folders (Inbox, Sent, Drafts, etc.) become Label objects, not Folder.
// The "All Mail" folder gets role "all" and is the primary sync target.
```

### Pattern 6: OAuth2 Token Refresh Before IMAP Authenticate

**What:** Before every IMAP `AUTHENTICATE XOAUTH2`, check if the access token is expired or within the 5-minute buffer window. If so, perform an HTTP token refresh. Emit `ProcessAccountSecretsUpdated` if the refresh token changes.

**Note:** The `TokenManager` must use `Arc<tokio::sync::Mutex<TokenManager>>` to prevent race conditions between background sync and Phase 8's foreground IDLE sessions.

```rust
// Source: C++ XOAuth2TokenManager.cpp + OAUT-01/OAUT-02 requirements
pub struct TokenManager {
    // Per-account cached: (access_token, expiry_unix_timestamp)
    cache: std::collections::HashMap<String, (String, i64)>,
}

impl TokenManager {
    pub async fn get_valid_token(
        &mut self,
        account: &Account,
        delta: &DeltaStream,
    ) -> Result<String, SyncError> {
        let key = account.id.clone();
        let buffer_secs = 300i64; // 5-minute buffer per OAUT-02

        if let Some((token, expiry)) = self.cache.get(&key) {
            if *expiry > chrono::Utc::now().timestamp() + buffer_secs {
                return Ok(token.clone());
            }
        }

        // Token expired or within buffer — refresh
        let new_token_response = self.refresh_token(account).await?;
        let access_token = new_token_response.access_token().secret().to_string();
        let expiry = chrono::Utc::now().timestamp()
            + new_token_response.expires_in()
                .map(|d| d.as_secs() as i64)
                .unwrap_or(3600);

        // If refresh token rotated, emit ProcessAccountSecretsUpdated (OAUT-03)
        if let Some(new_refresh) = new_token_response.refresh_token() {
            if new_refresh.secret() != account.extra["refreshToken"].as_str().unwrap_or("") {
                delta.emit(DeltaStreamItem::account_secrets_updated(account, new_refresh.secret()));
            }
        }

        self.cache.insert(key, (access_token.clone(), expiry));
        Ok(access_token)
    }
}

// XOAUTH2 SASL payload format
fn build_xoauth2_string(username: &str, access_token: &str) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    let raw = format!("user={}\x01auth=Bearer {}\x01\x01", username, access_token);
    STANDARD.encode(raw.as_bytes())
}
```

### Pattern 7: Per-Operation Timeouts

**What:** Wrap every IMAP network await with `tokio::time::timeout()`. This prevents silent hangs when the server stops responding.

**Recommended timeouts:**

| Operation | Timeout | Rationale |
|-----------|---------|-----------|
| TCP connect | 15s | Same as Phase 2 IMAP testing |
| TLS handshake | 15s | Same as Phase 2 |
| IMAP login / AUTHENTICATE | 30s | Server may be slow under load |
| SELECT / select_condstore | 30s | Normally instant |
| uid_fetch (headers, batch) | 120s | Batch fetch can be large |
| uid_fetch (single body BODY.PEEK[]) | 120s | Large attachments |
| IMAP STATUS | 15s | Should be instant |
| LIST | 30s | Large folder lists can be slow |
| fetch stream item | 30s | Per-item timeout within a stream |

```rust
// Source: IMPR-05 requirement + tokio::time docs
// Pattern for any IMAP operation:
let mailbox = tokio::time::timeout(
    Duration::from_secs(30),
    session.select_condstore(&folder.path),
)
.await
.map_err(|_| SyncError::Timeout)??;

// Pattern for fetch stream (timeout per item, not entire stream):
let mut stream = tokio::time::timeout(
    Duration::from_secs(120),
    session.uid_fetch(&uid_set, FETCH_QUERY),
).await.map_err(|_| SyncError::Timeout)??;

while let Some(item) = tokio::time::timeout(Duration::from_secs(30), stream.next())
    .await
    .map_err(|_| SyncError::Timeout)?
{
    // process item
}
```

### Pattern 8: Body Caching with Age Policy and MessageBody Table

**What:** Lazy body fetch — store message bodies in the existing Message record's `body` field. Only fetch bodies for messages newer than `maxAgeForBodySync()`. Exclude spam and trash folders from body caching. Insert a NULL placeholder (or empty body) before fetching to prevent double-fetch if interrupted.

**C++ constants (SyncWorker.cpp:987):**

```cpp
time_t SyncWorker::maxAgeForBodySync(Folder & folder) {
    return 24 * 60 * 60 * 30 * 3; // three months
}
```

**Body sync SQL query (from C++ SyncWorker.cpp:1028):**

```sql
SELECT Message.id, Message.remoteUID
FROM Message
LEFT JOIN MessageBody ON MessageBody.id = Message.id
WHERE Message.accountId = ?
  AND Message.remoteFolderId = ?
  AND (Message.date > ? OR Message.draft = 1)
  AND Message.remoteUID > 0
  AND MessageBody.id IS NULL
ORDER BY Message.date DESC
LIMIT 30
```

**Rust pattern:**

```rust
// Source: C++ SyncWorker::syncMessageBodies()
const BODY_CACHE_AGE_SECS: i64 = 24 * 60 * 60 * 30 * 3; // 3 months header sync
const BODY_PREFETCH_AGE_SECS: i64 = 24 * 60 * 60 * 7;   // 7 days auto body pre-fetch
const BODY_SYNC_BATCH_SIZE: i64 = 30;

fn should_cache_bodies_in_folder(role: &str) -> bool {
    role != "spam" && role != "trash"
}

async fn sync_message_bodies(
    session: &mut ImapSession,
    folder: &Folder,
    store: &MailStore,
) -> Result<bool, SyncError> {
    if !should_cache_bodies_in_folder(&folder.role) {
        return Ok(false);
    }

    let cutoff = chrono::Utc::now().timestamp() - BODY_PREFETCH_AGE_SECS;
    let ids = store.find_messages_needing_bodies(&folder.id, cutoff, BODY_SYNC_BATCH_SIZE).await?;

    if ids.is_empty() {
        return Ok(false);
    }

    for (msg_id, uid, _folder_path) in &ids {
        let body_stream = tokio::time::timeout(
            Duration::from_secs(120),
            session.uid_fetch(&uid.to_string(), "BODY.PEEK[]"),
        ).await.map_err(|_| SyncError::Timeout)??;

        // parse with mail-parser, sanitize with ammonia, save body snippet to Message
    }

    Ok(true)
}
```

### Pattern 9: need-bodies Priority Queue

**What:** The stdin loop receives `need-bodies` commands with a list of message IDs. These IDs are inserted at the front of the body fetch queue, bypassing age-policy ordering.

**Implementation:**

```rust
// Source: C++ SyncWorker::idleQueueBodiesToSync() + idleCycleIteration()
use std::collections::VecDeque;

pub struct BodyQueue {
    queue: VecDeque<String>,
}

impl BodyQueue {
    pub fn enqueue_priority(&mut self, ids: Vec<String>) {
        for id in ids.into_iter().rev() {
            if !self.queue.contains(&id) {
                self.queue.push_front(id);
            }
        }
    }

    pub fn enqueue_background(&mut self, id: String) {
        if !self.queue.contains(&id) {
            self.queue.push_back(id);
        }
    }

    pub fn next(&mut self) -> Option<String> {
        self.queue.pop_front()
    }
}
```

### Pattern 10: Sync Loop Structure and WakeWorkers

**What:** The `background_sync` tokio task owns one IMAP session and loops indefinitely. It awaits either a sleep timer (2-minute base interval, backing off to 5 minutes on no-change) or a `wake_rx: mpsc::Receiver<()>` channel that re-accelerates when `wake-workers` arrives from stdin.

```rust
// Source: C++ SyncWorker::syncNow() — role-priority folder ordering
// Matches C++ roleOrder: {"inbox", "sent", "drafts", "all", "archive", "trash", "spam"}
fn sort_folders_by_role_priority(folders: &mut Vec<Folder>) {
    const ROLE_ORDER: &[&str] = &["inbox", "sent", "drafts", "all", "archive", "trash", "spam"];
    folders.sort_by_key(|f| {
        ROLE_ORDER.iter().position(|&r| r == f.role.as_str()).unwrap_or(usize::MAX)
    });
}

pub async fn background_sync(
    account: Arc<Account>,
    store: Arc<MailStore>,
    delta: Arc<DeltaStream>,
    mut shutdown_rx: broadcast::Receiver<()>,
    mut wake_rx: mpsc::Receiver<()>,
    mut body_queue_rx: mpsc::Receiver<Vec<String>>,
) {
    let mut no_change_streak = 0u32;

    loop {
        match run_sync_cycle(&account, &store, &delta).await {
            Ok(had_changes) => {
                if had_changes {
                    no_change_streak = 0;
                } else {
                    no_change_streak = no_change_streak.saturating_add(1);
                }

                // Backoff: 120s base, up to 300s when no changes
                let wait_secs = (120u64).min(300).max(
                    120 + (no_change_streak as u64 * 30).min(180)
                );

                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_secs(wait_secs)) => {},
                    _ = wake_rx.recv() => {
                        no_change_streak = 0; // Re-accelerate on wake
                    },
                    _ = shutdown_rx.recv() => return,
                }
            }
            Err(e) if e.is_auth() => {
                tracing::error!("Auth failure — stopping sync: {}", e);
                delta.emit_process_state(&account.id, true); // connectionError=true
                return;
            }
            Err(e) if e.is_retryable() => {
                tracing::warn!("Retryable sync error: {}", e);
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            Err(e) => {
                tracing::error!("Fatal sync error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
```

### Anti-Patterns to Avoid

- **Sharing one IMAP session with Phase 8's IDLE:** IDLE in Phase 8 uses a separate session per RFC. The background sync session is separate.
- **Fetching `BODY[]` instead of `BODY.PEEK[]`:** Using `BODY[]` marks messages as `\Seen`. Always use `BODY.PEEK[]`.
- **Calling `session.uid_fetch("1:*", "...")` without a timeout:** A slow server can return millions of messages.
- **Not handling `highest_modseq: None` after `select_condstore()`:** Some servers advertise CONDSTORE but return `[NOMODSEQ]` for specific folders — fall back to UID range sync silently.
- **Storing modseq as u32:** RFC 7162 defines modseq as 64-bit. Store as `i64` in SQLite (JSON string for safety with JS parsing).
- **Not handling the CONDSTORE truncation case:** If `server_modseq - stored_modseq > 4000` (MODSEQ_TRUNCATION_THRESHOLD), cap to last 12,000 UIDs.
- **Using raw `String::from_utf8_lossy()` on Envelope subjects:** RFC 2047 encoded-words must be decoded before hashing for stable IDs.
- **Creating a new BufReader for stdin in the sync task:** stdin is shared via the existing `Lines` iterator from Phase 5 — see `stdin_loop.rs` CRITICAL comment.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| IMAP protocol parsing | Custom IMAP response parser | `async-imap` + `imap-proto` | RFC 3501 response grammar has nested literals, binary content, edge cases |
| IMAP RFC 6154 attribute parsing | Manual string comparison on LIST responses | `imap_proto::NameAttribute` enum | imap-proto already parses and normalizes all RFC 6154 attributes |
| MIME email parsing | Custom MIME parser | `mail-parser` 0.11.x | RFC 2045-2049 has 41 character sets, multi-part recursion, encoding edge cases |
| HTML sanitization | Regex-based tag stripping | `ammonia` 4.x | Browser-accurate HTML5 parser; regex stripping misses obfuscated XSS vectors |
| OAuth2 token exchange | Manual HTTP POST + JSON parsing | `oauth2` 5.0.x | RFC 6749 edge cases (token rotation, expiry calculation, error responses) |
| Base58 encoding | Custom implementation | `bs58` 0.5.x with bitcoin alphabet | Must byte-for-byte match C++ `toBase58()` |
| SHA-256 hashing | Custom hash | `sha2` crate | Cryptographic correctness; `sha2` is audited |
| RFC 2047 MIME word decoding | Manual encoded-word parser | `rfc2047-decoder` 1.x | Handles all charsets, edge cases in encoded-word boundary detection |
| Per-operation timeouts | Manual `tokio::select!` with sleep | `tokio::time::timeout()` | `timeout()` correctly cancels the future |

**Key insight:** The stable ID generation algorithm cannot use any library's built-in ID scheme (UUID, nanoid, etc.) because it must produce identical output to the C++ `picosha2` + `toBase58()` combination. The `sha2` crate (SHA-256) plus `bs58` with the bitcoin alphabet is the only compatible approach.

---

## Common Pitfalls

### Pitfall 1: Envelope Bytes Are Not Decoded Strings

**What goes wrong:** `imap_proto::types::Envelope` fields (subject, message-id, addresses) are `Cow<'_, [u8]>` (raw bytes). Calling `String::from_utf8_lossy()` on a field encoded with RFC 2047 MIME encoded-words (e.g., `=?utf-8?B?SGVsbG8=?=`) produces the literal encoded string, not the decoded text — causing ID mismatches.

**Why it happens:** The IMAP protocol delivers header fields as raw octets; decoding is the client's responsibility. mailcore2 decoded at parse time; imap-proto does not.

**How to avoid:** Use `rfc2047_decoder::decode()` on `subject` and `message_id` fields before hashing. Do NOT decode address mailbox/host parts — mailcore2 treated those as raw bytes.

**Warning signs:** Messages appear as duplicates in the Electron UI after first sync of an account that has non-ASCII subjects.

### Pitfall 2: select_condstore() Returns highest_modseq: None Even When Server Advertises CONDSTORE

**What goes wrong:** Some IMAP servers advertise CONDSTORE capability but return `[NOMODSEQ]` for specific folders (shared mailboxes, public folders). Treating `None` as an error causes a sync abort.

**Why it happens:** RFC 7162 §3.1.2.2 allows servers to indicate `[NOMODSEQ]` for folders that do not support mod-sequences.

**How to avoid:** Always check `mailbox.highest_modseq.is_some()`. If `None`, fall back to UID range sync silently.

**Warning signs:** Specific folders never sync while others work fine on the same account.

### Pitfall 3: X-GM-THRID Requires Custom Extractor (Not a Typed Fetch Method)

**What goes wrong:** There is no `fetch.gmail_thread_id()` method in async-imap 0.11. Trying to call it fails to compile. However, `imap-proto` DOES have `AttributeValue::GmailThrId(u64)` — the value is available, just not surfaced.

**Why it happens:** async-imap 0.11 only implemented `gmail_labels()` and `gmail_msg_id()`. X-GM-THRID was not added.

**How to avoid:** Implement the `gmail_thread_id()` free function (Pattern 5 above). If `fetch.response` is private, submit a PR to async-imap adding the method (one-line change following the `gmail_msg_id()` pattern), or access via the underlying `parsed()` method if it is public.

**Warning signs:** Gmail messages appear without thread associations, or compile errors.

### Pitfall 4: Body Fetch Uses BODY[] Instead of BODY.PEEK[]

**What goes wrong:** Every body fetch marks the message as `\Seen`. Users' unread messages are marked as read by the sync engine.

**Why it happens:** `BODY[]` implicitly sets `\Seen` per RFC 3501 §6.4.5. `BODY.PEEK[]` does not.

**How to avoid:** Always use `BODY.PEEK[]` in all body fetch calls. The only exception is an explicit mark-read STORE command.

**Warning signs:** Users report emails being marked read without opening them.

### Pitfall 5: CONDSTORE modseq Stored as Wrong Integer Width

**What goes wrong:** Storing `highest_modseq` as a 32-bit integer in the Folder `localStatus` JSON causes silent truncation for servers with high modseq values.

**Why it happens:** RFC 7162 defines modseq as unsigned 64-bit. JSON numbers lose precision above 2^53 in JavaScript.

**How to avoid:** Store `highest_modseq` as a JSON string in localStatus (e.g., `"12345678901234"`) to avoid JavaScript precision loss. Parse it as `u64` in Rust.

**Warning signs:** Excessive full-syncs on accounts that have been active for years.

### Pitfall 6: Gmail Label-to-Folder Matching Requires Case-Insensitive Prefix Stripping

**What goes wrong:** Gmail X-GM-LABELS values look like `\Inbox \Sent Important "Work Projects"`. The `\Inbox` label needs to match the folder with path `INBOX`. Direct string equality fails.

**How to avoid:** Replicate the C++ `labelForXGMLabelName()` algorithm:
1. Try exact path match first.
2. For labels starting with `\`, strip the backslash, lowercase, and check if any folder's lowercased path (with `[gmail]/` prefix stripped) matches.
3. Also check if the folder's `role` matches the lowercased label name.

**Warning signs:** Gmail messages show no labels in the Electron UI.

### Pitfall 7: OAuth2 Token Refresh Race Condition

**What goes wrong:** Background sync and Phase 8's foreground IDLE both detect an expired token and both fire HTTP refresh requests simultaneously. A refresh token is consumed twice, the second request fails with `invalid_grant`.

**How to avoid:** Wrap `TokenManager` in `Arc<tokio::sync::Mutex<TokenManager>>`. The mutex is held for the duration of the HTTP request.

**Warning signs:** HTTP 400 `invalid_grant` errors on token refresh.

### Pitfall 8: SyncError Variants vs. Existing error.rs

**What goes wrong:** The existing `error.rs` SyncError enum in mailsync-rs was designed for C++ error key compatibility (with `error_key()` method). It lacks `is_retryable()`, `is_offline()`, `is_auth()`, and `is_fatal()` classification methods that the sync loop needs.

**How to avoid:** Extend the existing `SyncError` enum with these methods rather than creating a second error type. Map existing variants: `SyncError::Authentication` and `SyncError::InvalidCredentials` are auth errors; `SyncError::Connection`, `SyncError::NoRouteToHost`, `SyncError::DnsResolutionFailed` are offline; `SyncError::Timeout` and `SyncError::Retryable(_)` are retryable.

---

## Code Examples

### LIST with Role Detection

```rust
// Source: async-imap Session docs (docs.rs) + imap-proto NameAttribute variants
let mut list_stream = timeout(
    Duration::from_secs(30),
    session.list(Some(""), Some("*")),
).await.map_err(|_| SyncError::Timeout)??;

while let Some(name) = list_stream.next().await {
    let name = name?;

    // Skip non-selectable folders
    if name.attributes().iter().any(|a| matches!(a, NameAttribute::NoSelect)) {
        continue;
    }

    let role = detect_folder_role(&name, &namespace_prefix);

    // On Gmail: check if this should be a Label instead of a Folder
    let is_label = is_gmail
        && !name.attributes().iter().any(|a| matches!(
            a, NameAttribute::All | NameAttribute::Junk | NameAttribute::Trash
        ));

    // Save folder or label to store
    // ...
}
```

### CONDSTORE UID FETCH with CHANGEDSINCE

```rust
// Source: RFC 7162 §3.1.4 CHANGEDSINCE modifier + async-imap uid_fetch API
// Note: async-imap passes query string directly to IMAP — no typed CHANGEDSINCE API
let uid_set = "1:*";
let query = format!("(UID FLAGS ENVELOPE BODYSTRUCTURE) (CHANGEDSINCE {})", stored_modseq);

let mut stream = timeout(
    Duration::from_secs(120),
    session.uid_fetch(uid_set, &query),
).await.map_err(|_| SyncError::Timeout)??;

while let Some(fetch) = stream.next().await {
    let fetch = fetch?;
    let uid = fetch.uid.expect("UID requested but not returned");
    let modseq = fetch.modseq; // Option<u64>
    // process...
}
```

### Mailbox Fields After select_condstore

```rust
// Source: async-imap types/mailbox.rs (Mailbox struct fields verified)
let mailbox: async_imap::types::Mailbox = timeout(
    Duration::from_secs(30),
    session.select_condstore("INBOX"),
).await.map_err(|_| SyncError::Timeout)??;

let uid_validity: u32 = mailbox.uid_validity.unwrap_or(0);
let uid_next: u32 = mailbox.uid_next.unwrap_or(0);
let highest_modseq: u64 = mailbox.highest_modseq.unwrap_or(0);
let exists: u32 = mailbox.exists;
```

### Gmail Extension Fetch

```rust
// Source: async-imap Fetch impl (github.com/chatmail/async-imap src/types/fetch.rs)
// Include X-GM-LABELS, X-GM-MSGID, X-GM-THRID in query
let query = "(UID FLAGS ENVELOPE BODYSTRUCTURE X-GM-LABELS X-GM-MSGID X-GM-THRID)";
let mut stream = timeout(
    Duration::from_secs(120),
    session.uid_fetch(&uid_set, query),
).await.map_err(|_| SyncError::Timeout)??;

while let Some(fetch) = stream.next().await {
    let fetch = fetch?;
    let labels: Vec<String> = fetch.gmail_labels()
        .map(|v| v.iter().map(|l| l.to_string()).collect())
        .unwrap_or_default();
    let gmail_msg_id: Option<u64> = fetch.gmail_msg_id().copied();
    let gmail_thr_id: Option<u64> = gmail_thread_id(&fetch); // Free function (Pattern 5)
}
```

### SyncError Extension

```rust
// Source: IMPR-06 requirement — extend existing error.rs
impl SyncError {
    /// Retry after backoff — connection-level failures that may resolve
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::Connection | Self::Timeout | Self::Retryable(_)
                | Self::NoRouteToHost | Self::DnsResolutionFailed | Self::SslHandshakeFailed
        )
    }

    /// Backoff aggressively — likely offline
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Connection | Self::NoRouteToHost | Self::DnsResolutionFailed)
    }

    /// Stop retrying — credentials are invalid
    pub fn is_auth(&self) -> bool {
        matches!(self, Self::Authentication | Self::InvalidCredentials | Self::GmailIMAPNotEnabled)
    }

    /// Never retry — database corruption or fatal state
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Database(_))
    }
}
```

---

## Validation Architecture

### Test Framework

| Property | Value |
|----------|-------|
| Framework | Rust built-in test harness + `cargo test` |
| Config file | None — uses `Cargo.toml` `[dev-dependencies]` |
| Quick run command | `cd app/mailsync-rs && cargo test --lib 2>&1` |
| Full suite command | `cd app/mailsync-rs && cargo test --test-threads=1 2>&1` |

The existing test infrastructure in `app/mailsync-rs/` uses:
- `#[cfg(test)] mod tests` inline unit tests in each source file (145+ tests from Phase 6)
- `tests/ipc_contract.rs` — spawns compiled binary, tests IPC protocol
- `tests/mode_tests.rs` — integration tests for startup modes
- `tests/delta_coalesce.rs` — delta coalescing tests

Phase 7 adds a new integration test file: `tests/sync_worker.rs` (unit tests for sync algorithms without a live IMAP server).

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| ISYN-01 | Folder role detection from RFC 6154 flags | unit | `cargo test --lib imap::session::tests::role_detection -x` | Wave 0 |
| ISYN-01 | Folder role detection from name fallback table | unit | `cargo test --lib imap::session::tests::role_via_path -x` | Wave 0 |
| ISYN-01 | Gmail folder whitelist filter | unit | `cargo test --lib imap::sync_worker::tests::gmail_folder_whitelist -x` | Wave 0 |
| ISYN-02 | CONDSTORE modseq change detection logic | unit | `cargo test --lib imap::sync_worker::tests::condstore_no_change -x` | Wave 0 |
| ISYN-02 | CONDSTORE truncation at MODSEQ_TRUNCATION_THRESHOLD | unit | `cargo test --lib imap::sync_worker::tests::condstore_truncation -x` | Wave 0 |
| ISYN-03 | UID range fallback when CONDSTORE unavailable | unit | `cargo test --lib imap::sync_worker::tests::uid_range_fallback -x` | Wave 0 |
| ISYN-04 | UIDVALIDITY change triggers full resync | unit | `cargo test --lib imap::sync_worker::tests::uidvalidity_reset -x` | Wave 0 |
| ISYN-05 | Stable message ID matches C++ output for ASCII headers | unit | `cargo test --lib imap::mail_processor::tests::stable_id_ascii -x` | Wave 0 |
| ISYN-05 | Stable message ID matches C++ output for RFC 2047 encoded subjects | unit | `cargo test --lib imap::mail_processor::tests::stable_id_rfc2047 -x` | Wave 0 |
| ISYN-05 | Stable ID fallback to folder:uid when no date | unit | `cargo test --lib imap::mail_processor::tests::stable_id_no_date -x` | Wave 0 |
| ISYN-06 | Body age policy excludes messages older than 7 days | unit | `cargo test --lib imap::sync_worker::tests::body_age_policy -x` | Wave 0 |
| ISYN-06 | Body caching skips spam and trash folders | unit | `cargo test --lib imap::sync_worker::tests::body_skip_spam_trash -x` | Wave 0 |
| ISYN-07 | Folder priority ordering: inbox first, then sent/drafts | unit | `cargo test --lib imap::sync_worker::tests::folder_priority_sort -x` | Wave 0 |
| OAUT-01 | Token refresh HTTP request constructed correctly | unit | `cargo test --lib oauth2::tests::refresh_request_shape -x` | Wave 0 |
| OAUT-02 | Token expiry check with 5-minute buffer | unit | `cargo test --lib oauth2::tests::expiry_buffer_300s -x` | Wave 0 |
| OAUT-02 | Valid token reused within buffer window | unit | `cargo test --lib oauth2::tests::valid_token_cached -x` | Wave 0 |
| OAUT-03 | ProcessAccountSecretsUpdated delta emitted on token rotation | unit | `cargo test --lib oauth2::tests::secrets_updated_on_rotation -x` | Wave 0 |
| GMAL-01 | Gmail folder whitelist: non-whitelisted folders become Labels | unit | `cargo test --lib imap::sync_worker::tests::gmail_non_whitelist_becomes_label -x` | Wave 0 |
| GMAL-02 | X-GM-LABELS parsed from fetch response | unit | `cargo test --lib imap::mail_processor::tests::gmail_labels_parsed -x` | Wave 0 |
| GMAL-02 | X-GM-THRID extracted via AttributeValue::GmailThrId | unit | `cargo test --lib imap::mail_processor::tests::gmail_thrid_extracted -x` | Wave 0 |
| GMAL-04 | Gmail skips APPEND for Sent folder | unit | `cargo test --lib imap::sync_worker::tests::gmail_skip_append -x` | Wave 0 (Phase 8 executes, but flag documented here) |
| IMPR-05 | Per-operation timeout fires on simulated hang | unit | `cargo test --lib imap::sync_worker::tests::timeout_fires_on_hang -x` | Wave 0 |
| IMPR-06 | SyncError::is_retryable() classifies correctly | unit | `cargo test --lib error::tests::is_retryable_variants -x` | Wave 0 |
| IMPR-06 | SyncError::is_auth() classifies auth failures | unit | `cargo test --lib error::tests::is_auth_variants -x` | Wave 0 |

**Note:** All sync algorithm tests use mock IMAP sessions (no live server required). Integration tests with a real IMAP server are smoke-test only and manual.

### Sampling Rate

- **Per task commit:** `cd app/mailsync-rs && cargo test --lib 2>&1 | tail -5`
- **Per wave merge:** `cd app/mailsync-rs && cargo test --test-threads=1 2>&1`
- **Phase gate:** Full suite green (including IPC contract tests) before `/gsd:verify-work`

### Wave 0 Gaps

- [ ] `app/mailsync-rs/src/imap/mod.rs` — module declarations
- [ ] `app/mailsync-rs/src/imap/session.rs` — ImapSession struct (TLS connect + auth)
- [ ] `app/mailsync-rs/src/imap/sync_worker.rs` — background_sync with all unit tests
- [ ] `app/mailsync-rs/src/imap/mail_processor.rs` — Fetch → Message conversion + stable ID
- [ ] `app/mailsync-rs/src/oauth2.rs` — TokenManager with unit tests
- [ ] Phase 7 Cargo.toml additions: `async-imap`, `imap-proto`, `mail-parser`, `ammonia`, `oauth2`, `reqwest`, `sha2`, `bs58`, `rfc2047-decoder`, `chrono`, `base64`

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| QRESYNC for deleted message detection | CONDSTORE-only (no QRESYNC) | v2.0 decision | Deleted messages detected on next UID range scan; acceptable tradeoff |
| OAuth2 server-side refresh via Mailspring identity server | Direct client-side OAuth2 refresh via `oauth2` crate | v2.0 | Eliminates dependency on identity server for auth |
| Per-account OS threads (C++ std::thread) | Per-account tokio tasks | v2.0 | Fewer OS threads; same logical concurrency |
| C++ mailcore2 automatic header decoding | Explicit RFC 2047 decoding via `rfc2047-decoder` | v2.0 | Must handle encoding explicitly |
| 60-second OAuth2 buffer (C++) | 5-minute buffer (OAUT-02) | Phase 7 | More headroom for slow token exchanges |
| C++ picosha2 + toBase58() | sha2 crate + bs58 with bitcoin alphabet | Phase 7 | Identical output required |

**Not yet in async-imap 0.11 (as of research date):**
- Typed QRESYNC API (deferred to v2.x per DFRD-04)
- `gmail_thread_id()` method on Fetch (implement via free function using `AttributeValue::GmailThrId`)
- `CHANGEDSINCE` typed modifier (must be included in raw query string)

---

## Open Questions

All three original open questions from 2026-03-02 have been RESOLVED:

1. **X-GM-THRID raw extraction — RESOLVED (HIGH confidence)**
   - `imap-proto` has `AttributeValue::GmailThrId(u64)` confirmed in types.rs source
   - Implement `gmail_thread_id()` free function matching the `gmail_msg_id()` pattern
   - If `fetch.response` is private in async-imap 0.11, submit a PR (one-line change)

2. **RFC 2047 MIME encoding in stable ID generation — RESOLVED (HIGH confidence)**
   - C++ `setSubject()` calls `String::stringByDecodingMIMEHeaderValue()` at parse time
   - Rust MUST decode RFC 2047 before hashing: use `rfc2047-decoder` crate
   - Address mailbox/host parts: do NOT decode (C++ treated as raw bytes)

3. **Base58 encoding byte-for-byte compatibility — RESOLVED (HIGH confidence)**
   - Use `bs58` 0.5.x with `bs58::Alphabet::BITCOIN`
   - `bs58::encode(&hash[..30]).with_alphabet(bs58::Alphabet::BITCOIN).into_string()`
   - Handles zero-byte leading '1' prefix correctly — matches bitcoin reference implementation

**Remaining validation recommendation:** Write a test that generates message IDs using the Rust algorithm on a set of known messages from a deployed database, then verify they match the C++ output before Phase 7 merge.

---

## Sources

### Primary (HIGH confidence)

- `app/mailsync/MailSync/SyncWorker.cpp` — Read directly: `syncNow()`, `syncFoldersAndLabels()`, `syncFolderUIDRange()`, `syncFolderChangesViaCondstore()`, `syncMessageBodies()`, `maxAgeForBodySync()`, interval constants (SHALLOW_SCAN_INTERVAL=120s, DEEP_SCAN_INTERVAL=600s, MODSEQ_TRUNCATION_THRESHOLD=4000)
- `app/mailsync/MailSync/MailUtils.cpp` — Read directly: `idForMessage()` (SHA-256+Base58 stable ID algorithm), `roleForFolderViaFlags()`, `roleForFolderViaPath()`, `labelForXGMLabelName()`, `idForFolder()`
- `app/mailsync/MailSync/XOAuth2TokenManager.cpp` — Read directly: token cache, 60-second expiry buffer, `sendUpdatedSecrets()` delta emission
- `app/mailsync-rs/src/` — Read directly: sync.rs (insertion point), stdin_loop.rs (command dispatch), error.rs (SyncError enum), account.rs (extra field), delta/stream.rs, models/folder.rs, models/message.rs, models/thread.rs, store/mail_store.rs
- [async-imap Session struct](https://docs.rs/async-imap/latest/async_imap/struct.Session.html) — All method signatures: uid_fetch, select_condstore, list, run_command
- [async-imap Fetch struct](https://docs.rs/async-imap/latest/async_imap/types/struct.Fetch.html) — Fields: uid, modseq; methods: gmail_labels(), gmail_msg_id() — no gmail_thread_id()
- [async-imap Mailbox struct](https://docs.rs/async-imap/latest/async_imap/types/struct.Mailbox.html) — Fields: uid_next, uid_validity, highest_modseq, exists
- [imap-proto NameAttribute enum](https://docs.rs/imap-proto/latest/imap_proto/types/enum.NameAttribute.html) — Variants: All, Archive, Drafts, Flagged, Junk, Sent, Trash, NoSelect
- [imap-proto AttributeValue variants](https://github.com/djc/tokio-imap/blob/main/imap-proto/src/types.rs) — `AttributeValue::GmailThrId(u64)` confirmed in source
- [RFC 7162 CONDSTORE+QRESYNC](https://datatracker.ietf.org/doc/html/rfc7162) — CHANGEDSINCE modifier syntax, modseq as u64
- [RFC 4549 Disconnected IMAP Clients](https://datatracker.ietf.org/doc/html/rfc4549) — UIDVALIDITY change: MUST empty local cache
- [Gmail IMAP Extensions](https://developers.google.com/workspace/gmail/imap/imap-extensions) — X-GM-MSGID, X-GM-THRID, X-GM-LABELS format
- [bs58 crate docs](https://docs.rs/bs58/latest/bs58/) — `bs58::Alphabet::BITCOIN`, encode/decode API
- [rfc2047-decoder crate](https://docs.rs/rfc2047-decoder/latest/rfc2047_decoder/) — `decode(bytes: &[u8]) -> Result<String>`
- mailcore2 `MCMessageHeader.cpp` source — `setSubject()` calls `stringByDecodingMIMEHeaderValue()` at parse time (decoded before hashing confirmed)

### Secondary (MEDIUM confidence)

- [Delta Chat CONDSTORE implementation issue #2941](https://github.com/deltachat/deltachat-core-rust/issues/2941) — Production reference: select with (CONDSTORE), store HIGHESTMODSEQ, issue UID FETCH CHANGEDSINCE on subsequent syncs
- [Gmail support: sent messages auto-saved](https://support.google.com/mail/answer/78892) — Confirms Gmail auto-saves sent messages; APPEND causes duplicates

### Tertiary (LOW confidence — flagged for validation)

- Backoff curve values — recommended in research, not sourced from C++ (C++ uses fixed intervals; backoff is Rust improvement)
- `bs58` zero-byte edge case behavior — should be tested with known inputs from deployed database before merge

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all crate APIs verified via docs.rs and GitHub source inspection
- Architecture: HIGH — C++ source read directly; sync loop structure fully documented
- Pitfalls: HIGH — all pitfalls verified against C++ source or official RFC specs; X-GM-THRID and RFC 2047 issues confirmed via source inspection
- Stable ID algorithm: HIGH — C++ source read directly; `bs58` + `rfc2047-decoder` approach confirmed; hands-on test recommended before merge
- New Cargo.toml dependencies: HIGH — all crates verified on crates.io and docs.rs

**Research date:** 2026-03-04
**Valid until:** 2026-06-04 (async-imap updates could add `gmail_thread_id()`; check before Phase 7 coding)
