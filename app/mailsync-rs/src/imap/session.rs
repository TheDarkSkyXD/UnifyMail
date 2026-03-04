// imap/session.rs — IMAP connection and session management stub.
//
// Full implementation in Phase 7 Plans 03 (IMAP connection/TLS) and 05 (OAuth2 SASL auth).
// This stub establishes the type so other modules can reference ImapSession.

/// Stub IMAP session wrapper.
///
/// Plan 03 implements: TLS connection setup (tokio-rustls + rustls-platform-verifier),
/// IMAP LOGIN/AUTHENTICATE, SELECT/EXAMINE, capability negotiation.
/// Plan 05 implements: OAuth2 SASL XOAUTH2 authentication via TokenManager.
#[allow(dead_code)]
pub struct ImapSession {
    // Fields populated in Plans 03/05
}

#[cfg(test)]
mod tests {}
