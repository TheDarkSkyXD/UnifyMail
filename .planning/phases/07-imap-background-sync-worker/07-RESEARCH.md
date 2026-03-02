# Phase 7: IMAP Background Sync Worker - Research

**Researched:** 2026-03-02
**Domain:** async-imap CONDSTORE/UID-range sync, folder role detection, Gmail extensions, OAuth2 token management, body caching, per-operation timeouts, SyncError enum
**Confidence:** HIGH (async-imap Session and Fetch APIs verified via docs.rs and GitHub source; C++ SyncWorker.cpp read directly; RFC 7162/RFC 4549 verified; imap-proto NameAttribute enum verified; Gmail IMAP extension docs checked)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| ISYN-01 | IMAP folder enumeration via LIST with role detection (Inbox, Sent, Drafts, Trash, Spam, Archive) | Two-pass role assignment documented: RFC 6154 special-use flags first, name-based fallback second; imap-proto NameAttribute enum variants confirmed |
| ISYN-02 | IMAP incremental sync via CONDSTORE (modseq-based change detection) | `select_condstore()` returns `Mailbox` with `highest_modseq: Option<u64>`; raw `UID FETCH uid_set (FLAGS) (CHANGEDSINCE modseq)` pattern documented |
| ISYN-03 | IMAP incremental sync via UID range (fallback for servers without CONDSTORE) | `uid_fetch(uid_set, query)` pattern documented; Mailbox.uid_next and uid_validity fields confirmed; C++ shallow/deep scan pattern mapped to Rust |
| ISYN-04 | UIDVALIDITY change detection triggers full folder re-sync per RFC 4549 | RFC 4549 algorithm confirmed: compare stored vs server UIDVALIDITY, clear local UIDs, full re-fetch; C++ implementation read directly |
| ISYN-05 | Message header sync (FETCH ENVELOPE + BODYSTRUCTURE) with stable ID generation from headers | Fetch.envelope(), Fetch.bodystructure() methods confirmed; C++ SHA-256 + Base58 stable ID algorithm documented; mail-parser as MIME parser for body |
| ISYN-06 | Message body caching with `need-bodies` priority queue and per-folder age policy (3 months) | C++ maxAgeForBodySync() hardcoded to `24*60*60*30*3` seconds; MessageBody table with placeholder pattern documented; spam/trash excluded from body caching |
| ISYN-07 | Background sync worker iterates folders on 2-10 minute schedule | C++ constants: SHALLOW_SCAN_INTERVAL=120s, DEEP_SCAN_INTERVAL=600s; role-priority folder ordering documented |
| OAUT-01 | OAuth2 token refresh via HTTP token exchange endpoint | C++ XOAuth2TokenManager uses HTTP POST to provider token endpoint; oauth2 crate 5.0 reqwest-backed async refresh documented |
| OAUT-02 | Token expiry check before IMAP authenticate (5-minute buffer window) | C++ checks `expiryDate > time(0) + 60` (60s buffer); requirement spec says 5-minute buffer; expiry field from token response |
| OAUT-03 | Updated token credentials emitted to UI via `ProcessAccountSecretsUpdated` delta | C++ `DeltaStream::sendUpdatedSecrets()` emits JSON; Rust equivalent is emitting `{ type: "persist", modelClass: "ProcessAccountSecretsUpdated", ... }` |
| GMAL-01 | Gmail folder whitelist — only sync INBOX, All Mail, Trash, Spam | C++: only folders with `IMAPFolderFlagAll`, `IMAPFolderFlagSpam`, `IMAPFolderFlagTrash` are synced as folders; others become Labels |
| GMAL-02 | X-GM-LABELS, X-GM-MSGID, X-GM-THRID IMAP extension parsing | `Fetch::gmail_labels()` and `Fetch::gmail_msg_id()` are native async-imap methods; X-GM-THRID must be fetched via raw query string; query must include `X-GM-LABELS X-GM-MSGID X-GM-THRID` |
| GMAL-03 | Gmail contacts via Google People API (not standard CardDAV) | Phase 9 scope — not implemented in Phase 7 |
| GMAL-04 | Gmail skips IMAP APPEND for Sent folder after SMTP send | Gmail auto-saves sent mail; APPEND causes duplicates; detect Gmail capability and skip APPEND in SendDraftTask remote phase |
| IMPR-05 | Per-operation timeouts via `tokio::time::timeout()` for all network operations | `tokio::time::timeout(Duration, future)` pattern; recommended durations documented; wraps every await point on IMAP session calls |
| IMPR-06 | Structured `SyncError` enum distinguishing auth/TLS/network/server error classes | thiserror-derived enum; `is_retryable()` / `is_offline()` / `is_auth()` methods; mapping from async-imap error variants documented |
</phase_requirements>

---

## Summary

Phase 7 implements the background sync worker — the core of the mailsync engine. It translates what the C++ `SyncWorker::syncNow()` and `SyncWorker::syncFoldersAndLabels()` methods do into async Rust using `async-imap` 0.11.2.

The key architectural reality is that `async-imap 0.11.2` already provides first-class support for the three primary operations: `select_condstore()` returns a `Mailbox` struct with `highest_modseq: Option<u64>`, `uid_fetch(uid_set, query)` accepts raw IMAP fetch query strings including `(CHANGEDSINCE modseq)`, and the `Fetch` struct exposes `gmail_labels()` and `gmail_msg_id()` as native typed methods. `X-GM-THRID` requires including the string `X-GM-THRID` in the fetch query and reading it from the raw response via `imap-proto` — there is no dedicated typed method yet.

The role detection algorithm is two-pass: first check RFC 6154 special-use flags from `imap-proto::NameAttribute` (All, Sent, Drafts, Junk, Trash, Archive), then fall back to name-based matching against a lookup table of common folder names in multiple languages. The C++ source confirms this is the production algorithm. The Gmail folder whitelist (ISYN-01, GMAL-01) is implemented by checking the `X-GM-EXT-1` capability: when present, only folders with `\All`, `\Spam`, or `\Trash` special-use attributes are synced as folders; all other Gmail folders become Label objects.

Stable message ID generation replicates the C++ SHA-256 + Base58 scheme: hash `accountId + "-" + timestamp + subject + recipients + messageID`, encode first 30 bytes as Base58. The timestamp is the Unix epoch from the email's Date header (scheme v1) or a folder+UID fallback when no date header exists. This algorithm is non-negotiable — changing it would orphan all existing metadata in deployed Electron databases.

**Primary recommendation:** Model `imap/sync_worker.rs` on the C++ `SyncWorker::syncNow()` flow directly, replacing mailcore2 calls with async-imap equivalents. The sync loop structure, local-status JSON fields (`uidvalidity`, `highestmodseq`, `uidnext`, `syncedMinUID`, `bodiesPresent`, `bodiesWanted`), and folder role priority ordering must match the C++ implementation exactly.

---

## Standard Stack

### Core (Phase 7 only — all in Cargo.toml from Phase 5)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `async-imap` | 0.11.2 (runtime-tokio) | IMAP session, LIST, uid_fetch, select_condstore, idle | Only maintained async IMAP crate; CONDSTORE and Gmail extensions confirmed in source |
| `imap-proto` | 0.16.x | Low-level IMAP response parsing (NameAttribute for role detection, raw X-GM-THRID parsing) | Underlying parser used by async-imap; NameAttribute enum includes all RFC 6154 special-use attributes |
| `tokio-rustls` | 0.26.4 | TLS stream for IMAP connections | No OpenSSL; same as prior phases |
| `rustls-platform-verifier` | 0.6.2 | OS trust store certificate validation | Same as prior phases |
| `mail-parser` | 0.11.2 | Parse MIME message from BODY[] raw bytes | Zero-copy; 41 charsets; used for body caching and snippet extraction |
| `ammonia` | 4.1.2 | HTML sanitization of message body before storage | Whitelist-based; fixes RUSTSEC-2025-0071 |
| `oauth2` | 5.0.0 (reqwest) | OAuth2 access token refresh via HTTP POST | Provider-agnostic RFC 6749; async reqwest-backed |
| `reqwest` | 0.13.x (rustls-tls, json) | HTTP client for OAuth2 token endpoint | Already in stack; rustls-backed |
| `sha2` | 0.10.x | SHA-256 hash for stable message ID generation | Replicates C++ picosha2 — must match exactly |
| `tokio` | 1.x (time, sync, io-util) | `tokio::time::timeout()`, `mpsc` channels, async I/O | Foundation runtime — all IMAP awaits wrapped with timeout |
| `thiserror` | 2.x | `SyncError` enum derivation | Structured error classification with `is_retryable()` |
| `serde` + `serde_json` | 1.x | `ProcessAccountSecretsUpdated` delta emission, folder localStatus JSON | Foundation serialization |
| `tracing` | 0.1.x | Structured async logging per folder/account span | Async-aware; stderr only |
| `chrono` | 0.4.x | RFC 2822 date parsing for message ID generation | Required for timestamp extraction |
| `regex` | 1.x | Folder name matching in role detection fallback table | Case-insensitive name lookup |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `base64` | 0.22.x | XOAUTH2 SASL token encoding for IMAP AUTHENTICATE | Building `user=<u>\x01auth=Bearer <t>\x01\x01` payload |
| `uuid` | 1.x (v5) | Alternative ID generation for messages with no headers | Fallback only; primary is SHA-256 + Base58 |
| `indexmap` | 2.x | Ordered map for folder sync order preservation | Role-priority ordering of folders |
| `tokio-rusqlite` | 0.6.x | Database writes (folder status, messages, bodies) | All DB access goes through this — no raw rusqlite on async threads |

**Installation (additions only — all already in Cargo.toml from prior phases):**

```bash
# All dependencies are already declared in Cargo.toml from Phases 5-6.
# Phase 7 introduces no new crate dependencies.
# Verify versions with: cargo tree | grep -E "async-imap|imap-proto|mail-parser|ammonia|oauth2"
```

---

## Architecture Patterns

### Recommended Project Structure (Phase 7 files)

```
app/mailsync-rust/src/
├── imap/
│   ├── mod.rs
│   ├── session.rs          # ImapSession: connect, TLS/STARTTLS, authenticate (password + XOAUTH2)
│   ├── sync_worker.rs      # background_sync task: folder iteration, CONDSTORE, body fetch loop
│   └── mail_processor.rs   # parse Fetch -> Message + Thread, stable ID generation
├── oauth2.rs               # TokenManager: expiry check, HTTP refresh, secrets delta emission
└── error.rs                # SyncError enum: auth/TLS/network/server variants + is_retryable()
```

### Pattern 1: Two-Pass Folder Role Detection

**What:** Assign a role string (`"inbox"`, `"sent"`, `"drafts"`, `"trash"`, `"spam"`, `"archive"`, `"all"`) to each folder returned by `session.list()`. First pass checks RFC 6154 special-use attributes from `imap-proto::NameAttribute`. Second pass matches against a name lookup table.

**When to use:** During `syncFoldersAndLabels()` called at the start of every `background_sync` loop.

**Why two passes:** RFC 6154 is not universally supported (older IMAP servers omit it). Name-based fallback covers Courier, Dovecot, and Exchange servers that use predictable folder names.

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
        // Note: no Inbox variant in imap-proto NameAttribute — Inbox is detected by name
        _ => None,
    }
}

fn role_for_folder_via_flags(name: &async_imap::types::Name) -> Option<&'static str> {
    name.attributes().iter().find_map(|attr| role_for_name_attribute(attr))
}

// Lookup table for common names (lowercase, after stripping namespace prefix)
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
    if let Some(role) = role_for_folder_via_flags(name) {
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

**When to use:** When `Mailbox.highest_modseq.is_some()` after `select_condstore()`, meaning the server supports CONDSTORE.

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
    ).await.map_err(|_| SyncError::Timeout("select_condstore".into()))??;

    // Detect UIDVALIDITY change (RFC 4549)
    let server_uid_validity = mailbox.uid_validity.unwrap_or(0);
    if folder.local_uid_validity != 0 && folder.local_uid_validity != server_uid_validity {
        return handle_uidvalidity_change(session, folder, store, delta, &mailbox).await;
    }

    let server_modseq = match mailbox.highest_modseq {
        Some(m) => m,
        None => {
            // Server doesn't support CONDSTORE on this folder — fall back to UID range sync
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

    // Raw CONDSTORE fetch: UID FETCH uid_range (UID FLAGS ENVELOPE) (CHANGEDSINCE modseq)
    let query = format!("(UID FLAGS ENVELOPE BODYSTRUCTURE) (CHANGEDSINCE {})", folder.local_highest_modseq);
    let mut fetch_stream = tokio::time::timeout(
        Duration::from_secs(120),
        session.uid_fetch(&uid_range, &query),
    ).await.map_err(|_| SyncError::Timeout("uid_fetch condstore".into()))??;

    while let Some(fetch) = tokio::time::timeout(
        Duration::from_secs(30),
        fetch_stream.next(),
    ).await.map_err(|_| SyncError::Timeout("fetch item".into()))? {
        let fetch = fetch?;
        process_fetched_message(&fetch, folder, store, delta).await?;
    }

    // Update stored modseq and uidnext
    folder.local_highest_modseq = server_modseq;
    folder.local_uid_next = mailbox.uid_next.unwrap_or(folder.local_uid_next);
    store.save_folder_status(folder).await?;
    Ok(())
}
```

### Pattern 3: UIDVALIDITY Change Handling (RFC 4549)

**What:** When the server's UIDVALIDITY differs from the stored value, all local UIDs for that folder are invalidated. The client must discard all local message UIDs, reset localStatus, and perform a full re-sync.

**When to use:** At the start of every folder sync, before any CONDSTORE or UID range logic.

**What the C++ does (from SyncWorker.cpp:366-401):**
1. Set all messages' `remoteUID` to the "UNLINKED" sentinel value
2. Run a full `syncFolderUIDRange(folder, RangeMake(1, UINT64_MAX), false)` to refetch all messages
3. Increment `uidvalidityResetCount` in localStatus
4. Update `uidvalidity`, `uidnext`, `highestmodseq`, `syncedMinUID` to new values
5. Skip to next folder iteration (don't continue with body sync)

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

    // Step 1: Unlink all messages in this folder (clear remoteUID, mark for cleanup)
    store.unlink_messages_in_folder(&folder.id).await?;

    // Step 2: Full resync (fetch headers for all UIDs 1:*)
    sync_folder_uid_range(session, folder, store, delta, mailbox).await?;

    // Step 3: Update localStatus with new values
    folder.local_uid_validity = mailbox.uid_validity.unwrap_or(0);
    folder.local_uid_next = mailbox.uid_next.unwrap_or(1);
    folder.local_highest_modseq = mailbox.highest_modseq.unwrap_or(0);
    folder.local_synced_min_uid = 1;
    folder.uid_validity_reset_count += 1;

    store.save_folder_status(folder).await?;
    Ok(())
}
```

### Pattern 4: Stable Message ID Generation (SHA-256 + Base58)

**What:** Generate a stable, cross-folder message ID that survives UID changes and folder moves. The ID is deterministic from the message headers, not from the IMAP UID.

**Critical: This algorithm MUST match the C++ exactly.** Existing deployed Electron databases store message IDs generated by the C++ engine. If the Rust engine generates different IDs for the same message, the UI will show duplicate messages and metadata will be orphaned.

**The C++ algorithm (MailUtils.cpp:630-703, Scheme v1):**

```
input = accountId + "-" + unix_timestamp_str + subject + "-" + sorted_recipients + "-" + messageID
hash = sha256(input)
id = base58_encode(hash[0..30])
```

- `unix_timestamp_str`: from `Date:` header as Unix epoch string. If date is 0 or -1, use `folderPath + ":" + uid` as fallback.
- `sorted_recipients`: To + CC + BCC email addresses (not names), sorted lexicographically, concatenated without separator.
- `messageID`: from `Message-ID:` header. Empty string if header is missing or auto-generated.
- Base58 alphabet: `123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz` (no 0, I, O, l)

**Rust implementation:**

```rust
// Source: C++ MailUtils::idForMessage() — must produce identical output
use sha2::{Sha256, Digest};

fn base58_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz";
    // Standard base58 encoding — replicate C++ toBase58() exactly
    // (See bitcoin base58 reference implementation)
    todo!("implement base58 from bitcoin reference")
}

fn id_for_message(account_id: &str, folder_path: &str, uid: u32, envelope: &imap_proto::types::Envelope) -> String {
    // Extract and sort recipient emails (To + CC + BCC)
    let mut recipients: Vec<String> = vec![];
    for addr_list in [&envelope.to, &envelope.cc, &envelope.bcc] {
        if let Some(addrs) = addr_list {
            for addr in addrs {
                if let Some(mailbox) = &addr.mailbox {
                    if let Some(host) = &addr.host {
                        recipients.push(format!("{}@{}", mailbox, host));
                    }
                }
            }
        }
    }
    recipients.sort();
    let participants = recipients.join("");

    // Extract timestamp (Unix epoch from Date header)
    let timestamp_str = if let Some(date) = &envelope.date {
        // Parse RFC 2822 date string to Unix timestamp
        chrono::DateTime::parse_from_rfc2822(date)
            .map(|dt| dt.timestamp().to_string())
            .unwrap_or_else(|_| format!("{}:{}", folder_path, uid)) // fallback: folder:uid
    } else {
        format!("{}:{}", folder_path, uid)
    };

    // Extract message-id (empty string if missing/auto-generated)
    let message_id = envelope.message_id.as_deref().unwrap_or("");

    // Extract subject
    let subject = envelope.subject.as_deref().unwrap_or("");

    // Build hash input exactly as C++ does
    let src = format!("{}-{}{}-{}-{}", account_id, timestamp_str, subject, participants, message_id);

    let mut hasher = Sha256::new();
    hasher.update(src.as_bytes());
    let hash = hasher.finalize();

    // Encode first 30 bytes as Base58 (matches C++ `toBase58(hash.data(), 30)`)
    base58_encode(&hash[..30])
}
```

**Warning:** The Envelope struct in `imap-proto` uses `Cow<'_, [u8]>` for field values (raw bytes, not decoded strings). Decode with `std::str::from_utf8()` or `String::from_utf8_lossy()` to get strings. Subject and message-ID may be encoded in RFC 2047 MIME words — use `mail-parser`'s decoder or a charset-aware decoder. The C++ engine's mailcore2 decoded these automatically; Rust must replicate that.

### Pattern 5: Gmail Extension Attributes in FETCH

**What:** Include Gmail extension attributes in `uid_fetch` query string. Access them via `Fetch` methods.

**Confirmed API (from async-imap source, docs.rs):**
- `Fetch::gmail_labels()` → `Option<&Vec<Cow<'_, str>>>` — extracts `X-GM-LABELS`
- `Fetch::gmail_msg_id()` → `Option<&u64>` — extracts `X-GM-MSGID`
- `X-GM-THRID` — NO typed method exists in async-imap 0.11.2. Must read from raw response.

**X-GM-THRID workaround:** Include `X-GM-THRID` in the fetch query string. The value will appear in the raw `imap-proto` response attributes. Access via the underlying `imap-proto::types::AttributeValue` structures from `Fetch`'s internal response data, or use `session.uid_search("X-GM-THRID <id>")` as a secondary lookup. Alternatively, extract via run_command pattern:

```rust
// Source: Gmail IMAP Extensions docs + async-imap Fetch API
// Query string for Gmail message fetch
const GMAIL_FETCH_QUERY: &str =
    "(UID FLAGS ENVELOPE BODYSTRUCTURE X-GM-LABELS X-GM-MSGID X-GM-THRID)";

// Access the available typed methods:
if let Some(labels) = fetch.gmail_labels() {
    let label_strings: Vec<String> = labels.iter().map(|l| l.to_string()).collect();
    message.gmail_labels = label_strings;
}
if let Some(msg_id) = fetch.gmail_msg_id() {
    message.gmail_msg_id = Some(*msg_id);
}
// X-GM-THRID: no typed method — must parse from raw response
// Option A: Don't fetch in initial sync; fetch on-demand when needed
// Option B: Parse from imap-proto AttributeValue::Atom in Fetch's internal response
// Recommended: Parse via raw response inspection until async-imap adds typed method
```

**Gmail folder whitelist enforcement (GMAL-01):**

```rust
// Source: C++ SyncWorker.cpp:659-661, mailsync/CLAUDE.md
// When IMAP capability contains "X-GM-EXT-1":
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

**What:** Before every IMAP `AUTHENTICATE XOAUTH2`, check if the access token is expired or within the buffer window. If so, perform an HTTP token refresh. Emit `ProcessAccountSecretsUpdated` if the refresh token changes.

**Buffer window:** The requirement says 5 minutes. The C++ uses 60 seconds. Use 5 minutes (300 seconds) as specified.

**Example:**

```rust
// Source: C++ XOAuth2TokenManager.cpp + OAUT-01/OAUT-02 requirements
use oauth2::{TokenResponse, StandardTokenResponse};

pub struct TokenManager {
    // Cached: (access_token, expiry_unix_timestamp)
    cache: HashMap<String, (String, i64)>,
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

        // Token expired or within buffer window — refresh
        let new_token_response = self.refresh_token(account).await?;
        let access_token = new_token_response.access_token().secret().to_string();
        let expiry = chrono::Utc::now().timestamp()
            + new_token_response.expires_in()
                .map(|d| d.as_secs() as i64)
                .unwrap_or(3600);

        // If refresh token rotated, emit ProcessAccountSecretsUpdated delta (OAUT-03)
        if let Some(new_refresh) = new_token_response.refresh_token() {
            if new_refresh.secret() != &account.refresh_token {
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

**What:** Wrap every IMAP network await with `tokio::time::timeout()`. This prevents silent hangs when the server stops responding without closing the TCP connection.

**Recommended timeouts:**

| Operation | Timeout | Rationale |
|-----------|---------|-----------|
| TCP connect | 15s | Same as Phase 2 IMAP testing |
| TLS handshake | 15s | Same as Phase 2 |
| IMAP login / AUTHENTICATE | 30s | Server may be slow under load |
| SELECT / select_condstore | 30s | Normally instant |
| uid_fetch (headers, ≤100 UIDs) | 60s | Batch fetch can be large |
| uid_fetch (single body BODY[]) | 120s | Large attachments |
| IMAP STATUS | 15s | Should be instant |
| LIST | 30s | Large folder lists can be slow |
| IDLE wait | 25 * 60s | RFC 2177 recommends < 30 minutes |

**Example:**

```rust
// Source: IMPR-05 requirement + tokio::time docs
use tokio::time::{timeout, Duration};

// Pattern for any IMAP operation:
let mailbox = timeout(
    Duration::from_secs(30),
    session.select_condstore(&folder.path),
)
.await
.map_err(|_| SyncError::Timeout(format!("select_condstore on {}", folder.path)))??;

// Pattern for fetch stream (timeout per item, not for entire stream):
let mut stream = timeout(
    Duration::from_secs(60),
    session.uid_fetch(&uid_set, FETCH_QUERY),
).await.map_err(|_| SyncError::Timeout("uid_fetch".into()))??;

while let Some(item) = timeout(Duration::from_secs(30), stream.next())
    .await
    .map_err(|_| SyncError::Timeout("fetch stream item".into()))?
{
    // process item
}
```

### Pattern 8: Body Caching with Age Policy and MessageBody Table

**What:** Lazy body fetch — store message bodies in a separate `MessageBody` table. Only fetch bodies for messages newer than `maxAgeForBodySync()` (3 months = `24*60*60*30*3` seconds). Exclude spam and trash folders from body caching entirely. Insert a NULL placeholder into `MessageBody` before fetching to prevent double-fetch if the process is interrupted.

**When to use:** After header sync completes for each folder.

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

The `LIMIT 30` ensures we do at most 30 body fetches per sync iteration, keeping each sync cycle bounded in time.

**Rust pattern:**

```rust
// Source: C++ SyncWorker::syncMessageBodies()
const BODY_CACHE_AGE_SECS: i64 = 24 * 60 * 60 * 30 * 3; // 3 months
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

    let cutoff = chrono::Utc::now().timestamp() - BODY_CACHE_AGE_SECS;

    // Find messages needing bodies (not yet in MessageBody table)
    let ids = store.find_messages_needing_bodies(&folder.id, cutoff, BODY_SYNC_BATCH_SIZE).await?;
    if ids.is_empty() {
        return Ok(false);
    }

    // Insert NULL placeholders to mark as "in progress"
    store.insert_body_placeholders(&ids).await?;

    // Fetch bodies one at a time (BODY.PEEK[] to avoid marking as read)
    for (msg_id, uid, folder_path) in &ids {
        let body = timeout(
            Duration::from_secs(120),
            session.uid_fetch(&uid.to_string(), "BODY.PEEK[]"),
        ).await.map_err(|_| SyncError::Timeout("body fetch".into()))??;
        // parse and store body...
    }

    Ok(!ids.is_empty())
}
```

### Pattern 9: need-bodies Priority Queue

**What:** The stdin loop receives `need-bodies` commands with a list of message IDs. These IDs are inserted at the front of the body fetch queue (highest priority), bypassing the age-policy ordering.

**When to use:** When the Electron UI explicitly requests a message body (e.g., user opens a message).

**Implementation:**

```rust
// Source: C++ SyncWorker::idleQueueBodiesToSync() + idleCycleIteration()
// The priority queue is a VecDeque<String> of message IDs.
// need-bodies IDs are pushed to the FRONT (highest priority).
// Background body sync pushes to the BACK.
pub struct BodyQueue {
    queue: VecDeque<String>,
}

impl BodyQueue {
    pub fn enqueue_priority(&mut self, ids: Vec<String>) {
        for id in ids.into_iter().rev() {
            self.queue.push_front(id);
        }
    }

    pub fn enqueue_background(&mut self, id: String) {
        self.queue.push_back(id);
    }

    pub fn next(&mut self) -> Option<String> {
        self.queue.pop_front()
    }
}
```

### Anti-Patterns to Avoid

- **Sharing one IMAP session between background sync and body fetch:** Background sync selects different folders repeatedly; body fetch also needs to select folders. Use the same session but ensure only one operation happens at a time (they run in the same `background_sync` task, so this is naturally sequential).
- **Fetching `BODY[]` instead of `BODY.PEEK[]`:** Using `BODY[]` marks messages as `\Seen`. Always use `BODY.PEEK[]` for body caching.
- **Calling `session.uid_fetch("1:*", "...")` without a timeout:** A malicious or slow server can return millions of messages. Always timeout the fetch and limit UID ranges.
- **Not using CONDSTORE when available:** Always check `mailbox.highest_modseq.is_some()` after `select_condstore()`. Some servers advertise CONDSTORE capability but return `[NOMODSEQ]` for specific folders — handle gracefully by falling back to UID range sync.
- **Storing modseq as u32:** RFC 7162 defines modseq as a 64-bit unsigned integer. The Mailbox struct correctly uses `Option<u64>`. Store in SQLite as `INTEGER` (SQLite's 64-bit signed integer can hold u64 values up to 2^63-1, sufficient in practice).
- **Not handling the CONDSTORE truncation case:** If `server_modseq - stored_modseq > 4000` (C++ MODSEQ_TRUNCATION_THRESHOLD), the full CHANGEDSINCE range could return 100k+ messages. Cap to the last 12,000 UIDs as the C++ does; the deep scan will catch the rest.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| IMAP protocol parsing | Custom IMAP response parser | `async-imap` + `imap-proto` | RFC 3501 response grammar has nested literals, binary content, edge cases; ~3,000 lines to replicate correctly |
| IMAP RFC 6154 attribute parsing | Manual string comparison on LIST responses | `imap-proto::NameAttribute` enum | imap-proto already parses and normalizes all RFC 6154 attributes |
| MIME email parsing | Custom MIME parser | `mail-parser` 0.11.2 | RFC 2045-2049 has 41 character sets, multi-part recursion, and encoding edge cases; validated against millions of real emails |
| HTML sanitization | Regex-based tag stripping | `ammonia` 4.1.2 | Browser-accurate HTML5 parser; regex stripping misses obfuscated XSS vectors |
| OAuth2 token exchange | Manual HTTP POST + JSON parsing | `oauth2` 5.0.0 | RFC 6749 edge cases (token rotation, expiry calculation, error responses, provider-specific quirks) |
| Base58 encoding | Custom implementation | Replicate bitcoin reference exactly | Must byte-for-byte match C++ `toBase58()` or message IDs will differ and metadata will be orphaned |
| SHA-256 hashing | Custom hash | `sha2` crate | Cryptographic correctness; `sha2` is audited |
| Per-operation timeouts | Manual `tokio::select!` with sleep | `tokio::time::timeout()` | `timeout()` correctly cancels the future; manual `select!` with sleep requires careful waker handling |

**Key insight:** The stable ID generation algorithm cannot use any library's built-in ID scheme (UUID, nanoid, etc.) because it must produce identical output to the C++ `picosha2` + `toBase58()` combination. The sha2 crate (SHA-256) plus a hand-written Base58 encoder using the bitcoin alphabet is the only compatible approach.

---

## Common Pitfalls

### Pitfall 1: Envelope Bytes Are Not Decoded Strings

**What goes wrong:** `imap-proto::types::Envelope` fields (subject, message-id, addresses) are `Cow<'_, [u8]>` (raw bytes), not UTF-8 strings. Calling `String::from_utf8_lossy()` on a field encoded with RFC 2047 MIME encoded-words (e.g., `=?utf-8?B?SGVsbG8=?=`) produces garbled output in the stable ID, causing ID mismatches.

**Why it happens:** The IMAP protocol delivers header fields as raw octets; decoding is the client's responsibility.

**How to avoid:** For the stable ID calculation: use the raw bytes as-is (same as C++ mailcore2 which compared decoded strings, but whose behavior can be replicated by using raw encoded bytes consistently). For display purposes, use `mail-parser`'s decoded header accessors. Be consistent: if C++ decoded before hashing, Rust must decode the same way. Inspect what mailcore2 returned for `header()->messageID()` — likely decoded RFC 2047.

**Warning signs:** Messages appear as duplicates in the Electron UI after first sync of an account.

### Pitfall 2: select_condstore() Returns highest_modseq: None Even When Server Advertises CONDSTORE

**What goes wrong:** Some IMAP servers advertise CONDSTORE capability but return `[NOMODSEQ]` for specific folders (e.g., shared mailboxes, public folders). `Mailbox.highest_modseq` will be `None`. Treating this as an error causes a sync abort.

**Why it happens:** RFC 7162 §3.1.2.2 allows servers to indicate `[NOMODSEQ]` for folders that do not support mod-sequences.

**How to avoid:** Always check `mailbox.highest_modseq.is_some()`. If `None`, fall back to UID range sync for that folder without logging an error. This is per-folder, not per-server.

**Warning signs:** Specific folders never sync while others work fine on the same account.

### Pitfall 3: X-GM-THRID Has No Typed Method in async-imap 0.11.2

**What goes wrong:** Including `X-GM-THRID` in the uid_fetch query string and then calling `fetch.gmail_thread_id()` fails to compile — the method doesn't exist. The data is in the response but not exposed via a typed accessor.

**Why it happens:** async-imap 0.11.2 only implemented `gmail_labels()` and `gmail_msg_id()`. `X-GM-THRID` was not added.

**How to avoid:** For Phase 7, include `X-GM-THRID` in the query string (the server will send the data), but implement a raw response parser to extract it from `imap-proto`'s `AttributeValue` structures. Alternatively, omit `X-GM-THRID` from Phase 7 and add it via a follow-up SEARCH operation, or accept that thread grouping for Gmail uses `X-GM-LABELS` for now. The planner should flag this as a task to implement the raw extractor.

**Warning signs:** Gmail messages appear without thread associations, or compile errors on `gmail_thread_id()`.

### Pitfall 4: Body Fetch Uses BODY[] Instead of BODY.PEEK[]

**What goes wrong:** Every body fetch marks the message as `\Seen`. Users' unread messages are marked as read by the sync engine.

**Why it happens:** `BODY[]` implicitly sets `\Seen` per RFC 3501 §6.4.5. `BODY.PEEK[]` does not.

**How to avoid:** Always use `BODY.PEEK[]` in all `uid_fetch` calls that retrieve message bodies. The only exception is if the Electron app explicitly requests a message to be marked read (which would be a separate STORE command, not a side effect of FETCH).

**Warning signs:** Users report emails being marked read without opening them.

### Pitfall 5: CONDSTORE modseq Stored as Wrong Integer Width

**What goes wrong:** Storing `highest_modseq` as a 32-bit integer in the Folder `localStatus` JSON causes silent truncation for servers with high modseq values (> 2^32 ≈ 4 billion). The stored modseq wraps around, causing the engine to think there are no changes when there are.

**Why it happens:** RFC 7162 defines modseq as unsigned 64-bit. JSON numbers lose precision above 2^53 in JavaScript. The C++ engine stores this as a `uint64_t` in JSON.

**How to avoid:** Store `highest_modseq` as a JSON string (e.g., `"12345678901234"`) in localStatus to avoid JavaScript precision loss when the Electron app reads it back. Parse it as `u64` in Rust. Alternatively, keep it as a JSON number but document that JavaScript `JSON.parse()` may lose precision above 2^53.

**Warning signs:** Excessive full-syncs on accounts that have been active for years; `highestmodseq` in localStatus appears to be truncated.

### Pitfall 6: Gmail Label-to-Folder Matching Requires Case-Insensitive Prefix Stripping

**What goes wrong:** Gmail X-GM-LABELS values look like `\Inbox \Sent Important "Work Projects"`. The `\Inbox` label needs to match the folder with path `INBOX`, and `\Sent` needs to match `[Gmail]/Sent Mail`. Direct string equality fails.

**Why it happens:** Gmail's X-GM-LABELS uses backslash-prefixed system labels (e.g., `\Inbox`, `\Sent`) that don't match the actual IMAP folder paths. Custom labels appear as plain strings.

**How to avoid:** Replicate the C++ `labelForXGMLabelName()` algorithm:
1. Try exact path match first.
2. For labels starting with `\`, strip the backslash, lowercase, and check if any folder's lowercased path (with `[gmail]/` prefix stripped) matches.
3. Also check if the folder's `role` matches the lowercased label name (handles `\Sent` → role `"sent"`).

**Warning signs:** Gmail messages show no labels in the Electron UI.

### Pitfall 7: OAuth2 Token Refresh Race Condition

**What goes wrong:** Two concurrent tokio tasks (background sync and foreground IDLE) both detect an expired token and both fire HTTP refresh requests simultaneously. Both succeed but with different results; one overwrites the other's cached token, or the refresh token is consumed twice.

**Why it happens:** The C++ uses a mutex to serialize token refresh (XOAuth2TokenManager has a `lock_guard<mutex>`). Without equivalent protection in Rust, two async tasks can race.

**How to avoid:** The `TokenManager` must use `Arc<tokio::sync::Mutex<TokenManager>>` so only one task can refresh at a time. The mutex is held for the duration of the HTTP request. Alternative: Use `tokio::sync::OnceCell` for per-account token state with a waker pattern.

**Warning signs:** HTTP 400 errors on token refresh; refresh token invalid errors; "invalid_grant" from the OAuth2 endpoint.

---

## Code Examples

Verified patterns from official sources and C++ source inspection:

### LIST with Role Detection

```rust
// Source: async-imap Session docs (docs.rs) + imap-proto NameAttribute variants
let mut list_stream = session
    .list(Some(""), Some("*"))
    .await?;

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

    let folder_id = sha256_base58(&format!("{}:{}", account_id, name.name()));
    // save folder or label to store...
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
).await??;

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
let mailbox: async_imap::types::Mailbox = session.select_condstore("INBOX").await?;
let uid_validity: u32 = mailbox.uid_validity.unwrap_or(0);
let uid_next: u32 = mailbox.uid_next.unwrap_or(0);
let highest_modseq: u64 = mailbox.highest_modseq.unwrap_or(0);
let exists: u32 = mailbox.exists; // message count
```

### Gmail Extension Fetch

```rust
// Source: async-imap Fetch impl (github.com/chatmail/async-imap src/types/fetch.rs)
// Include X-GM-LABELS, X-GM-MSGID in query; X-GM-THRID included but no typed method
let query = "(UID FLAGS ENVELOPE BODYSTRUCTURE X-GM-LABELS X-GM-MSGID X-GM-THRID)";
let mut stream = session.uid_fetch(&uid_set, query).await?;

while let Some(fetch) = stream.next().await {
    let fetch = fetch?;
    // Typed Gmail accessors (async-imap 0.11.2):
    let labels: Vec<String> = fetch.gmail_labels()
        .map(|v| v.iter().map(|l| l.to_string()).collect())
        .unwrap_or_default();
    let gmail_msg_id: Option<u64> = fetch.gmail_msg_id().copied();
    // X-GM-THRID: no typed method — parse from raw imap-proto response data
    // gmail_thread_id: implement custom extraction from AttributeValue::Number
}
```

### SyncError Enum

```rust
// Source: ARCHITECTURE.md Pattern 7 + IMPR-06 requirement
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("IMAP error: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("Auth error: {0}")]
    Auth(String),

    #[error("TLS error: {0}")]
    Tls(String),

    #[error("Network offline: {0}")]
    Offline(String),

    #[error("Server error: {0}")]
    Server(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Protocol error: {0}")]
    Protocol(String),
}

impl SyncError {
    /// Retry after backoff — connection-level failures that may resolve
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Imap(_) | Self::Offline(_) | Self::Timeout(_) | Self::Protocol(_))
    }

    /// Backoff aggressively — likely offline
    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline(_))
    }

    /// Stop retrying — credentials are invalid
    pub fn is_auth(&self) -> bool {
        matches!(self, Self::Auth(_))
    }

    /// Never retry — database corruption or fatal state
    pub fn is_fatal(&self) -> bool {
        matches!(self, Self::Database(_))
    }
}
```

### Per-Folder Sync Loop Structure

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
) {
    loop {
        match run_sync_cycle(&account, &store, &delta).await {
            Ok(sync_again_immediately) => {
                if !sync_again_immediately {
                    // Sleep 2 minutes (shallow scan interval) before next cycle
                    tokio::select! {
                        _ = tokio::time::sleep(Duration::from_secs(120)) => {},
                        _ = shutdown_rx.recv() => return,
                    }
                }
            }
            Err(e) if e.is_retryable() => {
                tracing::warn!("Background sync error (retrying): {}", e);
                tokio::time::sleep(Duration::from_secs(30)).await;
            }
            Err(e) if e.is_auth() => {
                tracing::error!("Authentication failed (stopping sync): {}", e);
                delta.emit(DeltaStreamItem::auth_error(&account.id));
                return; // Don't retry auth failures
            }
            Err(e) => {
                tracing::error!("Fatal sync error: {}", e);
                std::process::exit(1);
            }
        }
    }
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| QRESYNC for deleted message detection | CONDSTORE-only (no QRESYNC) | v2.0 decision | Deleted messages detected on next UID range scan, not immediately; acceptable tradeoff |
| OAuth2 server-side refresh via Mailspring identity server | Direct client-side OAuth2 refresh via `oauth2` crate | v2.0 | Eliminates dependency on identity server for auth; more robust |
| Per-account OS threads (C++ std::thread) | Per-account tokio tasks | v2.0 | Fewer OS threads; same logical concurrency |
| C++ mailcore2 automatic header decoding | Explicit RFC 2047 decoding via mail-parser | v2.0 | Must handle encoding explicitly; same correctness |
| 60-second OAuth2 buffer | 5-minute buffer (OAUT-02) | Phase 7 | More headroom for slow token exchanges |

**Not yet in async-imap (as of 0.11.2):**
- Typed QRESYNC API (deferred to v2.x per DFRD-04)
- `gmail_thread_id()` method on Fetch (X-GM-THRID must be extracted from raw response)
- `CHANGEDSINCE` typed modifier (must be included in raw query string)

---

## Open Questions

1. **X-GM-THRID raw extraction**
   - What we know: `X-GM-THRID` appears in the IMAP response when requested; `imap-proto` parses it into `AttributeValue` structures; `async-imap` doesn't expose it via a typed method
   - What's unclear: The exact `AttributeValue` variant and path to extract the u64 value from the `Fetch` struct's internal `ResponseData`
   - Recommendation: The planner should include a task to inspect the imap-proto source for `AttributeValue::Number` or `AttributeValue::Atom` patterns when `X-GM-THRID` is requested, and implement a custom extractor. This is a one-time investigation taking 1-2 hours.

2. **Envelope field encoding (RFC 2047 MIME words)**
   - What we know: The C++ engine's mailcore2 decoded RFC 2047 MIME encoded-words before building stable IDs; Rust must do the same or IDs will differ
   - What's unclear: Whether the C++ `messageID()` accessor on the Envelope returned the raw RFC 2047 encoded value or the decoded value
   - Recommendation: Write a test that generates a message ID using the C++ algorithm on a known message with an encoded header, then verify the Rust output matches. If they differ, adjust the decoding step.

3. **Base58 encoding byte-for-byte compatibility**
   - What we know: The C++ `toBase58()` function uses the bitcoin Base58 alphabet and processes hash bytes in big-endian order
   - What's unclear: Edge cases — what happens when the first bytes are zero? (Bitcoin Base58 adds leading '1' characters for zero bytes)
   - Recommendation: Test with known C++ inputs/outputs from the deployed database before Phase 7 code review.

---

## Sources

### Primary (HIGH confidence)

- `app/mailsync/MailSync/SyncWorker.cpp` — Read directly: `syncNow()`, `syncFoldersAndLabels()`, `syncFolderUIDRange()`, `syncFolderChangesViaCondstore()`, `syncMessageBodies()`, `maxAgeForBodySync()`, interval constants (SHALLOW_SCAN_INTERVAL=120s, DEEP_SCAN_INTERVAL=600s, MODSEQ_TRUNCATION_THRESHOLD=4000)
- `app/mailsync/MailSync/SyncWorker.hpp` — Read directly: method signatures, localStatus key constants
- `app/mailsync/MailSync/MailUtils.cpp` — Read directly: `idForMessage()` (stable ID algorithm with sha256+base58), `roleForFolderViaFlags()`, `roleForFolderViaPath()`, `labelForXGMLabelName()`, `idForFolder()`
- `app/mailsync/MailSync/XOAuth2TokenManager.cpp` — Read directly: token cache, 60-second expiry buffer, `sendUpdatedSecrets()` delta emission
- `app/mailsync/CLAUDE.md` — Gmail-specific behavior (X-GM-LABELS, folder whitelist), threading model
- [async-imap Session struct](https://docs.rs/async-imap/latest/async_imap/struct.Session.html) — All method signatures: uid_fetch, select_condstore, list, uid_search, run_command (HIGH confidence — docs.rs)
- [async-imap Fetch struct](https://docs.rs/async-imap/latest/async_imap/types/struct.Fetch.html) — Fields: message, uid, size, modseq; methods: flags(), envelope(), bodystructure(), gmail_labels(), gmail_msg_id() — X-GM-THRID NOT present (HIGH confidence — docs.rs + GitHub source inspection)
- [async-imap Mailbox struct](https://docs.rs/async-imap/latest/async_imap/types/struct.Mailbox.html) — Fields: flags, exists, recent, unseen, permanent_flags, uid_next, uid_validity, highest_modseq (HIGH confidence — docs.rs + GitHub source)
- [imap-proto NameAttribute enum](https://docs.rs/imap-proto/latest/imap_proto/types/enum.NameAttribute.html) — Variants: NoInferiors, NoSelect, Marked, Unmarked, All, Archive, Drafts, Flagged, Junk, Sent, Trash, Extension (HIGH confidence — docs.rs)
- [RFC 7162 CONDSTORE+QRESYNC](https://datatracker.ietf.org/doc/html/rfc7162) — CHANGEDSINCE modifier syntax, modseq as u64, HIGHESTMODSEQ untagged response, NOMODSEQ case
- [RFC 4549 Disconnected IMAP Clients](https://datatracker.ietf.org/doc/html/rfc4549) — UIDVALIDITY change: MUST empty local cache, MUST NOT cancel uploads
- [Gmail IMAP Extensions](https://developers.google.com/workspace/gmail/imap/imap-extensions) — X-GM-MSGID, X-GM-THRID, X-GM-LABELS format: 64-bit unsigned int / ASTRING list
- [async-imap fetch.rs source](https://github.com/chatmail/async-imap/blob/main/src/types/fetch.rs) — gmail_labels() and gmail_msg_id() confirmed; no gmail_thread_id()
- [async-imap client.rs source](https://github.com/chatmail/async-imap/blob/main/src/client.rs) — select_condstore() confirmed; run_command() present; run_command_and_read_response() absent
- [tokio::time::timeout](https://docs.rs/tokio/latest/tokio/time/fn.timeout.html) — per-operation timeout wrapping pattern

### Secondary (MEDIUM confidence)

- [Delta Chat CONDSTORE implementation issue #2941](https://github.com/deltachat/deltachat-core-rust/issues/2941) — Production reference: select with (CONDSTORE) parameter, store HIGHESTMODSEQ, issue UID FETCH CHANGEDSINCE on subsequent syncs
- [Gmail support: sent messages auto-saved](https://support.google.com/mail/answer/78892) — Confirms Gmail auto-saves sent messages to Sent folder when using SMTP; APPEND causes duplicates

### Tertiary (LOW confidence — for validation only)

- WebSearch results on Base58 encoding compatibility — flagged for hands-on test before Phase 7 merge
- WebSearch results on RFC 2047 MIME word decoding in Rust mail-parser — requires test against known C++ output

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all crate APIs verified via docs.rs and GitHub source inspection
- Architecture: HIGH — C++ source read directly; sync loop structure fully documented
- Pitfalls: HIGH — all pitfalls verified against C++ source or official RFC specs; two confirmed via async-imap source inspection (X-GM-THRID, no typed method)
- Stable ID algorithm: HIGH — C++ source read directly; Base58 byte-for-byte compatibility flagged as requiring test

**Research date:** 2026-03-02
**Valid until:** 2026-06-01 (async-imap updates could add X-GM-THRID typed method; check before Phase 7 coding)
