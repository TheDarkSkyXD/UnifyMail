# Phase 7: IMAP Background Sync Worker - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Full IMAP background sync worker that syncs real email accounts end-to-end: folder enumeration with role detection, CONDSTORE-based incremental sync (UID-range fallback for servers without CONDSTORE), message header/body caching, OAuth2 token management with automatic refresh, and Gmail-specific behaviors (folder whitelist, X-GM-LABELS, X-GM-THRID). The worker replaces `background_sync_stub` in `sync.rs` and integrates with the existing MailStore CRUD, delta emission pipeline, and stdin command dispatch from Phases 5-6. IMAP IDLE monitoring, task execution, and SMTP send belong to Phase 8.

</domain>

<decisions>
## Implementation Decisions

### Sync cycle strategy
- Priority-based folder ordering: Inbox first, then Sent/Drafts, then remaining folders in round-robin
- Fixed interval with backoff between sync cycles: ~60 seconds base, backing off to ~5 minutes when no changes detected. Re-accelerate immediately when `wake-workers` stdin command arrives
- UIDVALIDITY changes trigger silent re-sync — discard cached UIDs and full re-fetch per RFC 4549 with no user notification (internal consistency operation)
- CONDSTORE modseq-based incremental sync as primary strategy; UID range sync as fallback for servers without CONDSTORE capability

### OAuth2 token management
- Credentials read from Account JSON `extra` field on stdin during handshake — no separate credential store
- Sync worker handles token refresh HTTP requests directly (reqwest + rustls) — self-contained, matches C++ XOAuth2TokenManager pattern
- On refresh, emit `ProcessAccountSecretsUpdated` delta so Electron persists the new token
- On refresh failure: retry 2-3 times with exponential backoff, then emit ProcessState with connectionError=true and stop syncing. Resume when `wake-workers` arrives
- Check token expiry within 5-minute buffer before every IMAP authenticate
- Support Gmail + Microsoft OAuth2 (XOAUTH2) from the start — covers ~80% of OAuth users

### Gmail-specific behaviors
- Hardcoded folder whitelist: only sync INBOX, [Gmail]/All Mail, [Gmail]/Trash, [Gmail]/Spam, [Gmail]/Drafts, [Gmail]/Sent Mail. All other Gmail virtual folders hidden
- X-GM-LABELS stored as Vec<String> JSON array on Message.labels field — matches existing Phase 6 Message model (revised from join table approach per user approval 2026-03-04)
- X-GM-THRID used as primary Thread record ID for Gmail accounts — gives exact Gmail threading behavior. Non-Gmail accounts fall back to References/In-Reply-To header-based threading
- X-GM-MSGID stored on Message.gMsgId field for stable message identity
- Gmail detection via `account.provider == "gmail"` from handshake JSON — set by onboarding providerForEmail()

### Body caching and need-bodies
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

</decisions>

<specifics>
## Specific Ideas

- The 07-RESEARCH.md has comprehensive documentation of async-imap Session API, CONDSTORE patterns, C++ SyncWorker.cpp scan loop, and folder role detection algorithm
- `background_sync_stub` in `sync.rs` (line 121) is the exact insertion point — replace with real IMAP sync loop
- `stdin_loop.rs` already parses `need-bodies` (with message_ids) and `wake-workers` commands — wire dispatch to sync worker via tokio channels
- Account `extra` field (serde flatten) captures all credential fields including `refreshToken`, `accessToken`, `refreshClientId`, `provider`
- Environment variables `GMAIL_CLIENT_ID`, `GMAIL_OAUTH_PROXY_URL` passed to binary by mailsync-process.ts — needed for Gmail OAuth refresh
- ProcessState delta already emitted on startup in sync.rs — extend with sync progress fields
- async-imap 0.11 already in mailcore-rs Cargo.toml (can share workspace version); reqwest needed for OAuth HTTP

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailsync-rs/src/modes/sync.rs`: Contains `background_sync_stub` placeholder — replace with real sync worker
- `app/mailsync-rs/src/stdin_loop.rs`: Already parses `need-bodies` and `wake-workers` commands — wire to sync worker
- `app/mailsync-rs/src/delta/stream.rs`: DeltaStream with `emit_process_state()` — extend for sync progress
- `app/mailsync-rs/src/store/mail_store.rs`: MailStore with save/remove/find/find_all/count — use for all model persistence
- `app/mailsync-rs/src/models/folder.rs`: Folder model with `role`, `path`, `local_status` fields
- `app/mailsync-rs/src/models/message.rs`: Message model with `remote_uid`, `g_msg_id`, `remote_folder_id`, `thread_id`, `body` fields
- `app/mailsync-rs/src/models/thread.rs`: Thread model for thread record management
- `app/mailsync-rs/src/models/label.rs`: Label model for Gmail X-GM-LABELS storage
- `app/mailsync-rs/src/error.rs`: SyncError enum with all C++ error keys — use existing variants for IMAP errors
- `app/mailsync-rs/src/account.rs`: Account struct with `extra` serde flatten for credential access
- `app/mailcore-rs/Cargo.toml`: async-imap 0.11 and lettre already declared — share workspace versions

### Established Patterns
- Fat-row pattern: `data` JSON blob + indexed projection columns for all models (Phase 6)
- Delta emission: DeltaStream → mpsc → 500ms coalescing → stdout flush task (Phase 5)
- tokio-rusqlite `call()` for all DB access — synchronous rusqlite inside closures on background thread (Phase 6)
- serde rename for C++ JSON key compatibility: `aid`, `hMsgId`, `gThrId`, `v`, etc. (Phase 6)
- Structured logging via tracing crate — debug level for operational details, info for state changes (Phase 5)
- ProcessState + ProcessAccountSecretsUpdated as control deltas that bypass coalescing (Phase 6)

### Integration Points
- `app/mailsync-rs/src/modes/sync.rs`: Entry point — `run()` creates sync worker tokio task
- `app/mailsync-rs/src/stdin_loop.rs`: `dispatch_command()` needs real handlers for `NeedBodies` and `WakeWorkers`
- `app/mailsync-rs/src/delta/stream.rs`: `emit_process_state()` needs sync progress fields
- `app/frontend/flux/stores/online-status-store.ts`: Consumes ProcessState deltas including sync progress
- `app/frontend/key-manager.ts`: Consumes ProcessAccountSecretsUpdated for OAuth token persistence
- `app/frontend/flux/models/folder.ts`: TypeScript Folder model — delta emission must produce compatible JSON
- `app/frontend/flux/models/thread.ts`: TypeScript Thread model — threading must produce compatible records

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 07-imap-background-sync-worker*
*Context gathered: 2026-03-04*
