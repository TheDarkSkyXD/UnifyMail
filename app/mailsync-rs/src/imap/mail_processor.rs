// imap/mail_processor.rs — Message parsing and database persistence stub.
//
// Full implementation in Phase 7 Plan 04 (message parsing, HTML sanitization,
// attachment extraction, thread ID derivation).

use crate::error::SyncError;

/// Compute a stable, deterministic message ID from IMAP metadata (stub).
///
/// Full implementation in Plan 04: derives ID from Message-ID header,
/// envelope sender, date, and subject using SHA-256 + base58 encoding.
#[allow(dead_code)]
pub fn id_for_message(_envelope: &str) -> String {
    // Implemented in Plan 04
    String::new()
}

/// Parse a raw RFC822 message and persist it to the mail store (stub).
///
/// Full implementation in Plan 04: uses mail-parser for MIME parsing,
/// ammonia for HTML sanitization, rfc2047-decoder for header decoding,
/// and writes Message/Thread/Folder records via MailStore.
#[allow(dead_code)]
pub async fn process_fetched_message(_raw: &[u8]) -> Result<(), SyncError> {
    Err(SyncError::NotImplemented("process_fetched_message".into()))
}

#[cfg(test)]
mod tests {}
