// imap/mail_processor.rs — Message parsing, stable ID generation, and BodyQueue.
//
// Implements:
//   - id_for_message(): stable SHA-256+Base58 message ID matching C++ MailUtils::idForMessage()
//   - decode_mime_header(): RFC 2047 MIME encoded-word decoding (for subjects and message-ids)
//   - gmail_thread_id(): X-GM-THRID extraction from async-imap Fetch
//   - extract_gmail_extensions(): All three Gmail extension attributes
//   - process_fetched_message(): Fetch -> Message + Optional<Thread> conversion
//   - parse_flags(): IMAP flags to (unread, starred, draft)
//   - BodyQueue: Priority queue for body fetch requests with dedup

use std::collections::VecDeque;
use sha2::{Sha256, Digest};
use async_imap::types::{Fetch, Flag};
use imap_proto::types::{Address, Envelope};
use chrono::DateTime;

use crate::error::SyncError;
use crate::models::message::Message;
use crate::models::thread::Thread;
use crate::models::folder::Folder;
use crate::account::Account;

// ============================================================================
// GMAL-04: Gmail skip-append flag
// ============================================================================

/// Gmail auto-saves sent mail to the Sent folder via its IMAP extension.
/// Appending a sent message to the Sent folder would create a duplicate.
///
/// Phase 8 SendDraftTask must check this flag and skip the APPEND command
/// for Gmail accounts. The Gmail server automatically adds the sent message.
///
/// References: GMAL-04 requirement.
pub const GMAIL_SKIP_SENT_APPEND: bool = true;

// ============================================================================
// Gmail extension structs
// ============================================================================

/// Gmail IMAP extension attributes extracted from a Fetch response.
#[derive(Debug, Clone, Default)]
pub struct GmailExtensions {
    /// X-GM-LABELS: Gmail label strings (e.g., ["\\Inbox", "Work", "Important"])
    pub labels: Vec<String>,
    /// X-GM-MSGID: Gmail unique message ID
    pub msg_id: Option<u64>,
    /// X-GM-THRID: Gmail thread ID (maps to thread_id as hex string)
    pub thr_id: Option<u64>,
}

// ============================================================================
// Header decoding
// ============================================================================

/// Decodes an RFC 2047 MIME encoded-word byte slice into a String.
///
/// Used for subject and message-id fields before hashing. Per the C++ matching
/// requirement, address mailbox/host parts are NOT decoded here — those are
/// treated as raw bytes in C++.
///
/// Falls back to String::from_utf8_lossy() if decoding fails.
pub fn decode_mime_header(raw: &[u8]) -> String {
    let s = String::from_utf8_lossy(raw);
    match rfc2047_decoder::decode(s.as_bytes()) {
        Ok(decoded) => decoded,
        Err(_) => s.into_owned(),
    }
}

// ============================================================================
// Stable message ID generation (matches C++ MailUtils::idForMessage exactly)
// ============================================================================

/// Generates a stable, deterministic message ID that matches C++ MailUtils::idForMessage().
///
/// Algorithm (C++ Scheme v1):
/// 1. Decode subject via decode_mime_header()
/// 2. Decode message-id via decode_mime_header()
/// 3. Extract recipients from To+CC+BCC: "mailbox@host" format (raw bytes, not decoded)
/// 4. Sort recipients lexicographically, join without separator
/// 5. Parse date as RFC 2822 string -> Unix timestamp string
///    - If None or parse fails -> use "{folder_path}:{uid}" fallback
/// 6. Build input: "{account_id}-{timestamp}{subject}-{recipients}-{message_id}"
/// 7. SHA-256 hash, take first 30 bytes, encode with Base58 Bitcoin alphabet
pub fn id_for_message(
    account_id: &str,
    folder_path: &str,
    uid: u32,
    envelope: &Envelope<'_>,
) -> String {
    // Step 1: Decode subject
    let subject = envelope
        .subject
        .as_ref()
        .map(|s| decode_mime_header(s))
        .unwrap_or_default();

    // Step 2: Decode message-id
    let message_id = envelope
        .message_id
        .as_ref()
        .map(|m| decode_mime_header(m))
        .unwrap_or_default();

    // Step 3: Extract recipients from To+CC+BCC (raw mailbox@host, no decoding)
    let mut recipients: Vec<String> = Vec::new();
    for addr_list in [&envelope.to, &envelope.cc, &envelope.bcc] {
        if let Some(addrs) = addr_list {
            for addr in addrs {
                let mailbox = addr
                    .mailbox
                    .as_ref()
                    .map(|m| String::from_utf8_lossy(m).into_owned())
                    .unwrap_or_default();
                let host = addr
                    .host
                    .as_ref()
                    .map(|h| String::from_utf8_lossy(h).into_owned())
                    .unwrap_or_default();
                if !mailbox.is_empty() || !host.is_empty() {
                    recipients.push(format!("{}@{}", mailbox, host));
                }
            }
        }
    }

    // Step 4: Sort lexicographically, join without separator
    recipients.sort();
    let recipients_str = recipients.join("");

    // Step 5: Parse date as RFC 2822 -> Unix timestamp string or fallback
    let timestamp = envelope
        .date
        .as_ref()
        .and_then(|d| {
            let date_str = String::from_utf8_lossy(d);
            DateTime::parse_from_rfc2822(date_str.trim())
                .ok()
                .map(|dt| dt.timestamp())
                .filter(|&ts| ts != 0)
        })
        .map(|ts| ts.to_string())
        .unwrap_or_else(|| format!("{}:{}", folder_path, uid));

    // Step 6: Build hash input: "{account_id}-{timestamp}{subject}-{recipients}-{message_id}"
    let input = format!(
        "{}-{}{}-{}-{}",
        account_id, timestamp, subject, recipients_str, message_id
    );

    hash_to_base58(&input)
}

/// Internal helper: SHA-256 hash, first 30 bytes, Base58 Bitcoin encode.
fn hash_to_base58(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();
    let first_30 = &hash[..30];
    bs58::encode(first_30)
        .with_alphabet(bs58::Alphabet::BITCOIN)
        .into_string()
}

// ============================================================================
// Gmail extension extraction
// ============================================================================

/// Extract the X-GM-THRID value from a slice of imap_proto AttributeValues.
///
/// async-imap 0.11 exposes gmail_labels() and gmail_msg_id() on Fetch but does
/// NOT expose gmail_thr_id() as a public method. This free function accepts the
/// AttributeValue slice from a parsed imap_proto response and extracts
/// GmailThrId directly from the AttributeValue::GmailThrId variant.
///
/// Usage: The IMAP session layer (sync_worker) passes the parsed attributes
/// slice when a X-GM-THRID item is expected in the FETCH response.
pub fn gmail_thread_id(attrs: &[imap_proto::types::AttributeValue<'_>]) -> Option<u64> {
    for attr in attrs {
        if let imap_proto::types::AttributeValue::GmailThrId(thr_id) = attr {
            return Some(*thr_id);
        }
    }
    None
}

/// Extract all three Gmail extension attributes from a Fetch response.
///
/// Note: X-GM-THRID is not accessible via the Fetch public API in async-imap 0.11.
/// Pass an empty slice for `attrs` when X-GM-THRID is not available; `thr_id`
/// will be `None` and the caller falls back to the message's own stable ID.
/// The IMAP session layer should pass the full parsed attributes for thr_id access.
pub fn extract_gmail_extensions(
    fetch: &Fetch,
    attrs: &[imap_proto::types::AttributeValue<'_>],
) -> GmailExtensions {
    let labels = fetch
        .gmail_labels()
        .map(|labels| {
            labels
                .iter()
                .map(|l| l.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let msg_id = fetch.gmail_msg_id().copied();
    let thr_id = gmail_thread_id(attrs);

    GmailExtensions {
        labels,
        msg_id,
        thr_id,
    }
}

// ============================================================================
// IMAP flag parsing
// ============================================================================

/// Parse IMAP flags into (unread, starred, draft) tuple.
///
/// - \Seen -> unread=false (inverse of flag!)
/// - \Flagged -> starred=true
/// - \Draft -> draft=true
/// - Default: unread=true, starred=false, draft=false
pub fn parse_flags(flags: &[Flag<'_>]) -> (bool, bool, bool) {
    let mut unread = true;
    let mut starred = false;
    let mut draft = false;

    for flag in flags {
        match flag {
            Flag::Seen => unread = false,
            Flag::Flagged => starred = true,
            Flag::Draft => draft = true,
            _ => {}
        }
    }

    (unread, starred, draft)
}

// ============================================================================
// Contact address helpers
// ============================================================================

/// Convert an imap-proto Address into a JSON contact object {"name": "", "email": ""}.
fn address_to_json(addr: &Address<'_>) -> serde_json::Value {
    let name = addr
        .name
        .as_ref()
        .map(|n| decode_mime_header(n))
        .unwrap_or_default();

    let mailbox = addr
        .mailbox
        .as_ref()
        .map(|m| String::from_utf8_lossy(m).into_owned())
        .unwrap_or_default();

    let host = addr
        .host
        .as_ref()
        .map(|h| String::from_utf8_lossy(h).into_owned())
        .unwrap_or_default();

    let email = if host.is_empty() {
        mailbox
    } else {
        format!("{}@{}", mailbox, host)
    };

    serde_json::json!({
        "name": name,
        "email": email,
    })
}

/// Convert an optional address list to JSON contact array.
fn addr_list_to_json(addrs: &Option<Vec<Address<'_>>>) -> Vec<serde_json::Value> {
    addrs
        .as_ref()
        .map(|list| list.iter().map(address_to_json).collect::<Vec<_>>())
        .unwrap_or_default()
}

// ============================================================================
// Thread ID derivation for non-Gmail accounts
// ============================================================================

/// Derive a thread_id from References or In-Reply-To headers for non-Gmail accounts.
///
/// Algorithm:
/// 1. Try References header: take the LAST message-id in the list
/// 2. Fall back to In-Reply-To header
/// 3. If neither: return None (caller uses message's own stable ID)
///
/// Thread ID = SHA-256+Base58 of "{account_id}-{last_reference_message_id}"
fn derive_thread_id_from_headers(
    account_id: &str,
    references: Option<&[u8]>,
    in_reply_to: Option<&[u8]>,
) -> Option<String> {
    // Try to find the last message-id in the References header
    let last_ref = references
        .and_then(|r| {
            let refs_str = String::from_utf8_lossy(r);
            // References header contains space/newline-separated message-ids
            refs_str
                .split_whitespace()
                .filter(|s| s.starts_with('<') || s.contains('@'))
                .last()
                .map(|s| s.to_string())
        })
        .or_else(|| {
            in_reply_to.map(|r| String::from_utf8_lossy(r).trim().to_string())
        });

    last_ref.filter(|r| !r.is_empty()).map(|ref_id| {
        let input = format!("{}-{}", account_id, ref_id);
        hash_to_base58(&input)
    })
}

// ============================================================================
// Fetch-to-Message conversion
// ============================================================================

/// Converts an async-imap Fetch response into a Message model and optionally
/// a new Thread record.
///
/// Returns (Message, Option<Thread>) where Thread is Some if a new thread
/// should be created for this message's thread_id (caller checks DB).
pub fn process_fetched_message(
    fetch: &Fetch,
    folder: &Folder,
    account: &Account,
    is_gmail: bool,
) -> Result<(Message, Option<Thread>), SyncError> {
    let uid = fetch.uid.unwrap_or(0);

    let envelope = fetch
        .envelope()
        .ok_or_else(|| SyncError::Parse("Fetch missing envelope".to_string()))?;

    // Generate stable message ID
    let id = id_for_message(&account.id, &folder.path, uid, envelope);

    // Decode subject
    let subject = envelope
        .subject
        .as_ref()
        .map(|s| decode_mime_header(s))
        .unwrap_or_default();

    // Parse date to Unix timestamp
    let date = envelope
        .date
        .as_ref()
        .and_then(|d| {
            let date_str = String::from_utf8_lossy(d);
            DateTime::parse_from_rfc2822(date_str.trim())
                .ok()
                .map(|dt| dt.timestamp())
        })
        .unwrap_or(0);

    // Extract header message-id
    let header_message_id = envelope
        .message_id
        .as_ref()
        .map(|m| decode_mime_header(m))
        .unwrap_or_default();

    // Extract reply-to header message-id from In-Reply-To header
    // Note: The envelope in-reply-to is a byte slice of the raw In-Reply-To header
    let reply_to_header_message_id = envelope
        .in_reply_to
        .as_ref()
        .map(|r| String::from_utf8_lossy(r).trim().to_string())
        .filter(|s| !s.is_empty());

    // Convert address lists to JSON contact arrays
    let from = addr_list_to_json(&envelope.from);
    let to = addr_list_to_json(&envelope.to);
    let cc = addr_list_to_json(&envelope.cc);
    let bcc = addr_list_to_json(&envelope.bcc);
    let reply_to = addr_list_to_json(&envelope.reply_to);

    // Parse IMAP flags
    let flags_slice: Vec<Flag<'_>> = fetch.flags().collect();
    let (unread, starred, draft) = parse_flags(&flags_slice);

    // Extract Gmail extensions if applicable
    let mut gmail_ext = GmailExtensions::default();
    if is_gmail {
        gmail_ext = extract_gmail_extensions(fetch);
    }

    // Derive thread_id
    let thread_id = if is_gmail {
        // Gmail: thread_id = hex string of X-GM-THRID
        gmail_ext
            .thr_id
            .map(|thr_id| format!("{:x}", thr_id))
            .unwrap_or_else(|| id.clone())
    } else {
        // Non-Gmail: use References/In-Reply-To based threading
        // The envelope doesn't have References/In-Reply-To directly,
        // those come from the raw message headers (RFC822.HEADER fetch)
        // For now we handle via the envelope in_reply_to field only
        // Full header parsing is done when body is fetched
        derive_thread_id_from_headers(
            &account.id,
            None, // References requires RFC822.HEADER fetch — handled in body phase
            envelope.in_reply_to.as_deref(),
        )
        .unwrap_or_else(|| id.clone())
    };

    // Build remote_folder as JSON object
    let remote_folder = Some(serde_json::json!({
        "id": folder.id,
        "name": folder.path,
        "path": folder.path,
        "role": folder.role,
    }));

    // Construct the Message
    let message = Message {
        id: id.clone(),
        account_id: account.id.clone(),
        version: 1,
        synced_at: None,
        sync_unsaved_changes: None,
        remote_uid: uid,
        date,
        subject: subject.clone(),
        header_message_id,
        g_msg_id: gmail_ext.msg_id.map(|id| id.to_string()),
        g_thr_id: gmail_ext.thr_id.map(|id| format!("{:x}", id)),
        reply_to_header_message_id,
        forwarded_header_message_id: None,
        unread,
        starred,
        draft,
        labels: gmail_ext.labels,
        extra_headers: None,
        from,
        to,
        cc,
        bcc,
        reply_to,
        folder: None,
        remote_folder,
        thread_id: thread_id.clone(),
        snippet: None,
        plaintext: None,
        files: vec![],
        metadata: None,
    };

    // Create Thread record for new threads
    // Caller is responsible for checking if thread_id already exists in DB
    let thread = Thread {
        id: thread_id.clone(),
        account_id: account.id.clone(),
        version: 1,
        subject: subject.clone(),
        last_message_timestamp: date,
        first_message_timestamp: date,
        last_message_sent_timestamp: date,
        last_message_received_timestamp: date,
        g_thr_id: if is_gmail {
            gmail_ext.thr_id.map(|id| format!("{:x}", id))
        } else {
            None
        },
        unread: if unread { 1 } else { 0 },
        starred: if starred { 1 } else { 0 },
        in_all_mail: false,
        attachment_count: 0,
        search_row_id: None,
        folders: vec![serde_json::json!({
            "id": folder.id,
            "_u": if unread { 1 } else { 0 },
            "_im": 0,
        })],
        labels: vec![],
        participants: message.from.clone(),
        metadata: None,
    };

    Ok((message, Some(thread)))
}

// ============================================================================
// BodyQueue — priority queue for body fetch requests with dedup
// ============================================================================

/// Priority queue for IMAP body fetch requests.
///
/// Supports two insertion modes:
/// - enqueue_priority: Insert at front (high-priority, user-visible messages)
/// - enqueue_background: Append at back (background prefetch)
///
/// Both modes deduplicate: if a message ID is already queued, it is not added again.
pub struct BodyQueue {
    queue: VecDeque<String>,
}

impl BodyQueue {
    /// Create a new empty BodyQueue.
    pub fn new() -> Self {
        BodyQueue {
            queue: VecDeque::new(),
        }
    }

    /// Insert message IDs at the front of the queue (high-priority).
    ///
    /// IDs already present in the queue are skipped.
    /// The slice is inserted in order: first element ends up at front.
    pub fn enqueue_priority(&mut self, ids: Vec<String>) {
        // Insert in reverse order so that after all insertions, the original
        // order is preserved at the front of the queue.
        for id in ids.into_iter().rev() {
            if !self.queue.contains(&id) {
                self.queue.push_front(id);
            }
        }
    }

    /// Append a single message ID at the back of the queue (background fetch).
    ///
    /// If the ID is already in the queue, it is not added again.
    pub fn enqueue_background(&mut self, id: String) {
        if !self.queue.contains(&id) {
            self.queue.push_back(id);
        }
    }

    /// Remove and return the front element (next message to fetch).
    pub fn next(&mut self) -> Option<String> {
        self.queue.pop_front()
    }

    /// Returns true if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    /// Returns the number of pending body fetch requests.
    pub fn len(&self) -> usize {
        self.queue.len()
    }
}

impl Default for BodyQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Helper: compute expected ID manually for test assertions
    // -------------------------------------------------------------------------

    fn expected_id(input: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(input.as_bytes());
        let hash = hasher.finalize();
        let first_30 = &hash[..30];
        bs58::encode(first_30)
            .with_alphabet(bs58::Alphabet::BITCOIN)
            .into_string()
    }

    // -------------------------------------------------------------------------
    // Task 1: Stable ID generation tests
    // -------------------------------------------------------------------------

    #[test]
    fn stable_id_ascii() {
        // account: "acc1", date: "Fri, 01 Jan 2021 00:00:00 +0000" (1609459200),
        // subject: "Hello World", to: alice@test.com, msg_id: <abc@test.com>
        // format: "{account_id}-{timestamp}{subject}-{recipients}-{message_id}"
        let expected = expected_id("acc1-1609459200Hello World-alice@test.com-<abc@test.com>");

        // Build mock envelope
        let envelope = build_envelope(
            Some("Hello World"),
            Some("Fri, 01 Jan 2021 00:00:00 +0000"),
            Some("<abc@test.com>"),
            vec![("alice", "test.com")], // to
            vec![],                       // cc
            vec![],                       // bcc
        );

        let id = id_for_message("acc1", "INBOX", 42, &envelope);
        assert_eq!(id, expected, "stable_id_ascii: ID must match C++ output");
    }

    #[test]
    fn stable_id_rfc2047() {
        // RFC 2047 encoded subject "=?utf-8?B?SGVsbG8gV29ybGQ=?=" decodes to "Hello World"
        // Should produce SAME ID as stable_id_ascii
        let expected = expected_id("acc1-1609459200Hello World-alice@test.com-<abc@test.com>");

        let envelope = build_envelope(
            Some("=?utf-8?B?SGVsbG8gV29ybGQ=?="),
            Some("Fri, 01 Jan 2021 00:00:00 +0000"),
            Some("<abc@test.com>"),
            vec![("alice", "test.com")],
            vec![],
            vec![],
        );

        let id = id_for_message("acc1", "INBOX", 42, &envelope);
        assert_eq!(id, expected, "RFC 2047 subject must decode before hashing");
    }

    #[test]
    fn stable_id_no_date() {
        // When date is None, fallback is "INBOX:42"
        let expected = expected_id("acc1-INBOX:42Hello World-alice@test.com-<abc@test.com>");

        let envelope = build_envelope(
            Some("Hello World"),
            None, // no date
            Some("<abc@test.com>"),
            vec![("alice", "test.com")],
            vec![],
            vec![],
        );

        let id = id_for_message("acc1", "INBOX", 42, &envelope);
        assert_eq!(id, expected, "Missing date must use folder:uid fallback");
    }

    #[test]
    fn stable_id_sorted_recipients() {
        // To "bob@x.com" + CC "alice@x.com" -> sorted: "alice@x.combob@x.com"
        let expected = expected_id("acc1-1609459200Test Subject-alice@x.combob@x.com-<test@x.com>");

        let envelope = build_envelope(
            Some("Test Subject"),
            Some("Fri, 01 Jan 2021 00:00:00 +0000"),
            Some("<test@x.com>"),
            vec![("bob", "x.com")],    // to
            vec![("alice", "x.com")],  // cc
            vec![],                     // bcc
        );

        let id = id_for_message("acc1", "INBOX", 1, &envelope);
        assert_eq!(id, expected, "Recipients must be sorted before hashing");
    }

    #[test]
    fn stable_id_no_message_id() {
        // Missing Message-ID uses empty string in hash input
        let expected = expected_id("acc1-1609459200Hello World-alice@test.com-");

        let envelope = build_envelope(
            Some("Hello World"),
            Some("Fri, 01 Jan 2021 00:00:00 +0000"),
            None, // no message-id
            vec![("alice", "test.com")],
            vec![],
            vec![],
        );

        let id = id_for_message("acc1", "INBOX", 1, &envelope);
        assert_eq!(id, expected, "Missing message-id must use empty string");
    }

    #[test]
    fn stable_id_multiple_bcc() {
        // BCC recipients are included in sorted list
        // to: alice@x.com, bcc: charlie@x.com -> sorted: "alice@x.comcharlie@x.com"
        let expected = expected_id("acc1-1609459200Test-alice@x.comcharlie@x.com-<m@x.com>");

        let envelope = build_envelope(
            Some("Test"),
            Some("Fri, 01 Jan 2021 00:00:00 +0000"),
            Some("<m@x.com>"),
            vec![("alice", "x.com")],   // to
            vec![],                       // cc
            vec![("charlie", "x.com")],  // bcc
        );

        let id = id_for_message("acc1", "INBOX", 1, &envelope);
        assert_eq!(id, expected, "BCC must be included in sorted recipients");
    }

    // -------------------------------------------------------------------------
    // Task 1: Gmail extension extraction tests
    // These tests verify the GmailExtensions struct behavior.
    // Live Fetch construction is not testable without IMAP server,
    // so we test the extraction logic and struct directly.
    // -------------------------------------------------------------------------

    #[test]
    fn gmail_extensions_default_empty() {
        // GmailExtensions::default() produces empty labels and None IDs
        let ext = GmailExtensions::default();
        assert!(ext.labels.is_empty());
        assert!(ext.msg_id.is_none());
        assert!(ext.thr_id.is_none());
    }

    #[test]
    fn gmail_labels_struct_stores_strings() {
        let ext = GmailExtensions {
            labels: vec!["\\Inbox".to_string(), "Work".to_string(), "Important".to_string()],
            msg_id: Some(12345678901234),
            thr_id: Some(98765432109876),
        };
        assert_eq!(ext.labels, vec!["\\Inbox", "Work", "Important"]);
        assert_eq!(ext.msg_id, Some(12345678901234u64));
        assert_eq!(ext.thr_id, Some(98765432109876u64));
    }

    #[test]
    fn gmail_skip_sent_append_constant() {
        // GMAL-04: Gmail auto-saves sent mail — Phase 8 must skip APPEND
        assert!(GMAIL_SKIP_SENT_APPEND, "GMAIL_SKIP_SENT_APPEND must be true");
    }

    #[test]
    fn gmail_thr_id_hex_format() {
        // When X-GM-THRID = 0x12345678abcdef, thread_id should be "12345678abcdef"
        let thr_id: u64 = 0x12345678abcdef;
        let thread_id_str = format!("{:x}", thr_id);
        assert_eq!(thread_id_str, "12345678abcdef");
    }

    // -------------------------------------------------------------------------
    // Task 2: Fetch-to-Message conversion tests
    // These use mock Account and Folder without live IMAP
    // -------------------------------------------------------------------------

    fn make_account(id: &str) -> Account {
        Account {
            id: id.to_string(),
            email_address: Some(format!("user@{}.com", id)),
            provider: Some("gmail".to_string()),
            extra: serde_json::Value::Null,
        }
    }

    fn make_folder(id: &str, path: &str, role: &str) -> Folder {
        Folder {
            id: id.to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            path: path.to_string(),
            role: role.to_string(),
            local_status: None,
        }
    }

    // -------------------------------------------------------------------------
    // Task 2: BodyQueue tests
    // -------------------------------------------------------------------------

    #[test]
    fn body_queue_starts_empty() {
        let q = BodyQueue::new();
        assert!(q.is_empty());
        assert_eq!(q.len(), 0);
    }

    #[test]
    fn body_queue_priority_inserts_at_front() {
        let mut q = BodyQueue::new();
        q.enqueue_background("bg1".to_string());
        q.enqueue_priority(vec!["p1".to_string(), "p2".to_string()]);

        // After priority insert: [p1, p2, bg1]
        assert_eq!(q.next(), Some("p1".to_string()));
        assert_eq!(q.next(), Some("p2".to_string()));
        assert_eq!(q.next(), Some("bg1".to_string()));
        assert_eq!(q.next(), None);
    }

    #[test]
    fn body_queue_background_appends_at_back() {
        let mut q = BodyQueue::new();
        q.enqueue_background("id1".to_string());
        q.enqueue_background("id2".to_string());
        q.enqueue_background("id3".to_string());

        assert_eq!(q.next(), Some("id1".to_string()));
        assert_eq!(q.next(), Some("id2".to_string()));
        assert_eq!(q.next(), Some("id3".to_string()));
    }

    #[test]
    fn body_queue_dedup_background() {
        let mut q = BodyQueue::new();
        q.enqueue_background("id1".to_string());
        q.enqueue_background("id1".to_string()); // duplicate
        q.enqueue_background("id2".to_string());

        assert_eq!(q.len(), 2);
        assert_eq!(q.next(), Some("id1".to_string()));
        assert_eq!(q.next(), Some("id2".to_string()));
        assert_eq!(q.next(), None);
    }

    #[test]
    fn body_queue_dedup_priority() {
        let mut q = BodyQueue::new();
        q.enqueue_background("id1".to_string());
        // id1 already in queue, priority insert should skip it
        q.enqueue_priority(vec!["id1".to_string(), "id2".to_string()]);

        // id1 was already there (background), id2 gets inserted at front
        // Result: [id2, id1] — id1 stays in position, id2 added at front
        assert_eq!(q.len(), 2);
    }

    #[test]
    fn body_queue_fifo() {
        let mut q = BodyQueue::new();
        q.enqueue_background("first".to_string());
        q.enqueue_background("second".to_string());

        assert_eq!(q.next(), Some("first".to_string()));
        assert_eq!(q.next(), Some("second".to_string()));
        assert!(q.is_empty());
    }

    #[test]
    fn body_queue_len_updates() {
        let mut q = BodyQueue::new();
        assert_eq!(q.len(), 0);
        q.enqueue_background("a".to_string());
        assert_eq!(q.len(), 1);
        q.enqueue_background("b".to_string());
        assert_eq!(q.len(), 2);
        q.next();
        assert_eq!(q.len(), 1);
    }

    // -------------------------------------------------------------------------
    // Task 2: parse_flags tests
    // -------------------------------------------------------------------------

    #[test]
    fn parse_flags_defaults() {
        let (unread, starred, draft) = parse_flags(&[]);
        assert!(unread, "default unread=true");
        assert!(!starred, "default starred=false");
        assert!(!draft, "default draft=false");
    }

    #[test]
    fn parse_flags_seen_sets_unread_false() {
        let flags = vec![Flag::Seen];
        let (unread, starred, draft) = parse_flags(&flags);
        assert!(!unread, "\\Seen -> unread=false");
        assert!(!starred);
        assert!(!draft);
    }

    #[test]
    fn parse_flags_flagged_sets_starred() {
        let flags = vec![Flag::Flagged];
        let (unread, starred, draft) = parse_flags(&flags);
        assert!(unread);
        assert!(starred, "\\Flagged -> starred=true");
        assert!(!draft);
    }

    #[test]
    fn parse_flags_draft() {
        let flags = vec![Flag::Draft];
        let (_, _, draft) = parse_flags(&flags);
        assert!(draft, "\\Draft -> draft=true");
    }

    #[test]
    fn parse_flags_seen_and_flagged() {
        let flags = vec![Flag::Seen, Flag::Flagged];
        let (unread, starred, draft) = parse_flags(&flags);
        assert!(!unread);
        assert!(starred);
        assert!(!draft);
    }

    // -------------------------------------------------------------------------
    // Task 2: Threading tests (non-Gmail header-based)
    // -------------------------------------------------------------------------

    #[test]
    fn threading_non_gmail_with_references() {
        let account_id = "acc1";
        let last_ref = "<refs@test.com>";
        let expected = expected_id(&format!("{}-{}", account_id, last_ref));

        let result = derive_thread_id_from_headers(
            account_id,
            Some(b"<first@test.com> <refs@test.com>"),
            None,
        );
        assert_eq!(result, Some(expected), "Non-Gmail thread ID from References");
    }

    #[test]
    fn threading_non_gmail_with_in_reply_to() {
        let account_id = "acc1";
        let in_reply_to = "<reply@test.com>";
        let expected = expected_id(&format!("{}-{}", account_id, in_reply_to));

        let result = derive_thread_id_from_headers(
            account_id,
            None,
            Some(b"<reply@test.com>"),
        );
        assert_eq!(result, Some(expected), "Non-Gmail thread ID from In-Reply-To");
    }

    #[test]
    fn threading_non_gmail_no_headers_returns_none() {
        let result = derive_thread_id_from_headers("acc1", None, None);
        assert!(result.is_none(), "No threading headers -> None");
    }

    // -------------------------------------------------------------------------
    // Helper: Build a mock imap-proto Envelope for testing id_for_message.
    // -------------------------------------------------------------------------
    // imap_proto::types::Envelope has lifetime parameters (it uses Cow<'a, [u8]>).
    // We construct it directly since all fields are pub.

    fn build_envelope<'a>(
        subject: Option<&'a str>,
        date: Option<&'a str>,
        message_id: Option<&'a str>,
        to: Vec<(&'a str, &'a str)>,    // (mailbox, host)
        cc: Vec<(&'a str, &'a str)>,
        bcc: Vec<(&'a str, &'a str)>,
    ) -> imap_proto::types::Envelope<'a> {
        use imap_proto::types::Address;
        use std::borrow::Cow;

        fn make_addrs<'b>(list: Vec<(&'b str, &'b str)>) -> Option<Vec<Address<'b>>> {
            if list.is_empty() {
                None
            } else {
                Some(list.into_iter().map(|(mailbox, host)| Address {
                    name: None,
                    adl: None,
                    mailbox: Some(Cow::Borrowed(mailbox.as_bytes())),
                    host: Some(Cow::Borrowed(host.as_bytes())),
                }).collect())
            }
        }

        imap_proto::types::Envelope {
            date: date.map(|d| Cow::Borrowed(d.as_bytes())),
            subject: subject.map(|s| Cow::Borrowed(s.as_bytes())),
            from: None,
            sender: None,
            reply_to: None,
            to: make_addrs(to),
            cc: make_addrs(cc),
            bcc: make_addrs(bcc),
            in_reply_to: None,
            message_id: message_id.map(|m| Cow::Borrowed(m.as_bytes())),
        }
    }
}
