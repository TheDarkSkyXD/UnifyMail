# Feature Research

**Domain:** Rust mailsync engine rewrite — production email sync for a desktop IMAP/SMTP client
**Researched:** 2026-03-02
**Confidence:** HIGH (C++ source read directly; RFC standards verified; existing behavior documented from source)

---

## Context: Scope of This Research

This is a **subsequent milestone** (v2.0). The v1.0 FEATURES.md covered connection testing and provider detection for the napi-rs addon. This file covers the full sync engine: the `mailsync` binary that handles ongoing IMAP sync, SMTP sending, CalDAV/CardDAV, SQLite delta emission, task execution, and crash recovery.

The existing C++ engine (~16,200 LOC, 50 source files) is the ground truth. Every behavior documented here either exists in the C++ source or is explicitly classified as a new addition, deferral, or anti-feature.

**The 5 threads in the C++ engine** (from `main.cpp` directly):
1. **Main thread** — stdin listener: routes `queue-task`, `cancel-task`, `wake-workers`, `need-bodies`, `sync-calendar`, `detect-provider`, `query-capabilities`, `subscribe-folder-status`
2. **Background thread** (`SyncWorker`) — folder iteration: CONDSTORE/incremental sync of all folders, body caching
3. **Foreground thread** (`SyncWorker`) — IDLE on inbox/all, remote task execution, on-demand body fetches
4. **CalContacts thread** (`DAVWorker` + `GoogleContactsWorker`) — CalDAV calendar + CardDAV/Google People contact sync
5. **Metadata thread** (`MetadataWorker`) — plugin metadata sync via identity server HTTP long-polling

---

## Feature Landscape

### Table Stakes (Users Expect These)

Features that must exist. Missing any = engine is not functional. These all exist in the C++ source today.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| **stdin/stdout JSON protocol** | The UI communicates exclusively via this channel; zero tolerance for protocol changes | MEDIUM | Input: newline-delimited JSON objects with `type` field. Output: newline-delimited `{"type":"persist"/"unpersist","modelClass":"...","modelJSONs":[...]}` deltas. Protocol is the contract — must be byte-for-byte compatible with the TypeScript `MailsyncBridge` consumer. Five input message types: `queue-task`, `cancel-task`, `wake-workers`, `need-bodies`, `sync-calendar`. Five output response types for query messages: `provider-result`, `capabilities-result`, `folder-status`, plus implicit delta streams. |
| **IMAP incremental sync (UID range-based)** | Core sync: download new messages since last sync | HIGH | For servers without CONDSTORE: `UID FETCH <bottomUID>:*` to get UIDs/flags for a range; compare against local state. Bottom UID = `fetchMessageUIDAtDepth(folder, 100, uidnext)` — the UID of the message 100 positions from top. `UIDNEXT` stored in folder `localStatus`. Full headers fetch for new UIDs (Subject, From, To, Date, Message-ID, Thread-ID, flags). |
| **IMAP incremental sync (CONDSTORE/QRESYNC)** | Required to handle flag changes without full re-scan; Gmail supports CONDSTORE (not QRESYNC) | HIGH | `SELECT INBOX (CONDSTORE)` returns `HIGHESTMODSEQ`. Next sync: `UID FETCH 1:* (CHANGEDSINCE <lastHighestModSeq>)` returns only changed messages. `HIGHESTMODSEQ` stored per-folder in `localStatus`. QRESYNC (RFC 7162): supports `SELECT INBOX (QRESYNC (uidvalidity, knownModSeq, knownUids))` which returns vanished UIDs in `VANISHED` response — reduces round trips. Not all servers support QRESYNC (Gmail supports CONDSTORE only). Must fall back to UID-range scan when neither is available. |
| **IMAP IDLE monitoring** | Real-time new mail notifications without polling | HIGH | IDLE on the primary folder (inbox role, or "all" for Gmail). Blocks foreground thread in `session.idle()` call. Interrupted by `idleInterrupt()` when a task arrives or `wake-workers` received. RFC 2177: max 29 minutes per IDLE command; re-issue IDLE after timeout. During IDLE, server sends untagged `EXISTS`/`RECENT`/`FETCH`/`EXPUNGE`/`VANISHED` notifications. On IDLE wakeup: process queued VANISHED UIDs, check folder status (CONDSTORE or UID range), fetch bodies. Keep-alive: re-issue IDLE every 10 minutes (shorter than RFC maximum to prevent NAT timeout). |
| **IMAP folder management** | Users need inbox, sent, drafts, trash, archive, spam — role assignments drive UI | HIGH | `LIST "" "*"` to enumerate all folders. Role detection: hardcoded rules + `XLIST` extension for Gmail. Special roles: inbox, sent, drafts, trash, archive, spam, all (Gmail). Gmail: sync only `INBOX`, `[Gmail]/All Mail`, `[Gmail]/Trash`, `[Gmail]/Spam` — all other Gmail folders are virtual label views that create duplicate messages. Stored in `Folder` and `Label` models. Folder `localStatus` JSON blob stores sync state (uidnext, highestmodseq, uidvalidity, syncedMinUID, busiesPresent, bodiesWanted). |
| **UIDVALIDITY change handling** | UIDVALIDITY change = server folder was recreated; all cached UIDs are invalid | HIGH | On SELECT response: compare returned UIDVALIDITY against stored value. If changed: delete all local messages in that folder (`resetForAccount` for the folder scope), clear highestmodseq, start fresh full sync. Track `uidvalidityResetCount` to detect pathological servers. RFC 4549 canonical behavior. |
| **Message header sync** | Subject, From, To, CC, Date, Message-ID, thread grouping — all required for list view | MEDIUM | `FETCH (UID RFC822.SIZE INTERNALDATE FLAGS ENVELOPE BODYSTRUCTURE)` for new messages. Thread grouping: by Message-ID + In-Reply-To/References headers. Stable message IDs: derived from `message-id` header + account ID (not UID — UIDs change when folders restructured). MIME structure parsed from BODYSTRUCTURE without downloading body. |
| **Message body caching (lazy, priority-based)** | User expects to read email; but downloading all bodies upfront is prohibitive for large mailboxes | HIGH | Bodies fetched lazily in background. Priority: (1) `need-bodies` requests from UI (user opened a thread), (2) recent messages in inbox (last N messages, configurable per folder type). Background body sync: `shouldCacheBodiesInFolder()` returns false for low-priority folders. `maxAgeForBodySync()` limits how old a message must be to skip body sync. `countBodiesNeeded()` / `countBodiesPresent()` tracked in folder `localStatus` as `bodiesWanted`/`bodiesPresent`. `FETCH (BODY[])` or `FETCH (BODY.PEEK[])` for the full raw RFC 2822 body. |
| **SMTP send with task queue** | User sends email; must eventually be delivered even if network is temporarily unavailable | HIGH | `performRemoteSendDraft(task)`: serialize draft from DB → RFC 2822 via MIME builder → submit via SMTP session. Task lifecycle: `local` → `remote` → `complete`/error. If SMTP fails with retryable error (transient): leave task in `remote` state for next foreground wake. Permanent failures (auth, invalid recipient): mark task error, report to UI. SendDraft task: handle multipart with attachments, inline images, plaintext fallback. |
| **Task processor (local + remote)** | All UI mutations go through tasks; split into immediate local phase and deferred remote phase | HIGH | `performLocal(task)`: immediate DB write (optimistic update). `performRemote(task)`: IMAP/SMTP/DAV operation. Tasks: `ChangeFolderTask` (IMAP MOVE/COPY), `ChangeStarredTask` (STORE FLAGS +/-\Flagged), `ChangeUnreadTask` (STORE FLAGS +/-\Seen), `ChangeLabelsTask` (Gmail X-GM-LABELS STORE), `SendDraftTask` (SMTP + save to Sent), `DestroyDraftTask` (IMAP STORE deleted + EXPUNGE), `SyncbackCategoryTask` (create/rename folder), `DestroyCategoryTask` (delete folder), `SyncbackMetadataTask` (HTTP to identity server), `SyncbackEventTask` (CalDAV PUT), `DestroyEventTask` (CalDAV DELETE), `SyncbackContactTask` (CardDAV PUT), `DestroyContactTask` (CardDAV DELETE). |
| **Task cleanup** | Completed/cancelled tasks must not grow unbounded in the DB | LOW | `cleanupOldTasksAtRuntime()`: delete tasks older than N days with status `complete` or `cancelled`. `cleanupTasksAfterLaunch()`: on startup, reset any tasks stuck in `remote` state from a previous crash — reset to `local` so they re-execute. |
| **SQLite delta emission** | The UI observes database changes as delta streams; all writes must emit deltas | HIGH | Every `store->save(model)` emits a `{"type":"persist","modelClass":"...","modelJSONs":[...]}` JSON line to stdout. Every `store->remove(model)` emits `unpersist`. Delta coalescing: multiple saves of the same object within a flush window are merged — only final state is emitted (prevents UI flooding during bulk operations). `MailStoreTransaction`: RAII wrapper that batches deltas until commit, then emits all at once. 500ms default stream delay for background sync; 5ms for main thread (task execution needs immediate UI feedback). |
| **SQLite schema migration** | DB schema evolves across app versions; must handle upgrades without data loss | MEDIUM | `store.migrate()` run at startup via `--mode migrate` before normal `--mode sync`. Migrations are version-stamped incremental DDL. Existing migrate mode already in C++ — must port all existing migrations as initial state for the Rust DB layer. |
| **Crash recovery** | Process crashes; on next start must resume cleanly without double-applying changes | HIGH | `cleanupTasksAfterLaunch()`: reset tasks in `remote` state to re-execute. Folder `localStatus` persisted in DB — sync can resume from last `highestmodseq` / `syncedMinUID`. UIDVALIDITY re-checked on every SELECT — detects server-side reset. `CrashTracker` in `MailsyncBridge` (TypeScript side) handles >5 crashes in 5 minutes by marking account as error and stopping relaunch — engine must cooperate by returning meaningful exit codes. Retryable exceptions: sleep 120s then retry. Non-retryable: `abort()` (triggers crash tracker). |
| **Process lifecycle: start delay** | Multiple accounts started simultaneously risk SQLite lock contention | LOW | `account->startDelay()` returns a per-account delay (0–N seconds based on account index). Background sync sleeps this many seconds before starting. CalContacts thread sleeps 15 + startDelay seconds. This prevents all accounts from opening SQLite simultaneously on app launch. |
| **OAuth2 token refresh** | OAuth2 access tokens expire (typically 1 hour); must refresh before IMAP/SMTP auth fails | MEDIUM | `XOAuth2TokenManager`: validates token expiry before sync. Expired tokens: HTTP POST to OAuth provider with refresh_token → new access_token. Account `oauthClientId`/`oauthClientSecret` used for the exchange. New token written back to account credentials (keychain). Emitted as updated-secrets delta to UI so it can persist. Gmail and Outlook use this path. Retry policy: attempt refresh once; if refresh fails (invalid_grant), stop sync with auth error. |
| **stdin orphan detection** | If parent Electron process dies, engine must self-terminate | LOW | Main thread polls `cin.good()`. If `cin` is not good for >30 seconds, exits with code 141. This prevents zombie mailsync processes after Electron crashes. |
| **Verbose logging mode** | Developers need to debug IMAP/SMTP protocol traffic | LOW | `--verbose` flag: enable mailcore2 connection logger on IMAP/SMTP sessions. In Rust: enable `async-imap` tracing or a custom `ConnectionLogger` that writes raw protocol lines to the spdlog-equivalent log file. |

### Differentiators (Competitive Advantage)

Features that the Rust rewrite can improve upon the C++ baseline. These are not required for feature parity but make the implementation better.

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| **Fully async IMAP/SMTP (tokio)** | C++ uses synchronous IMAP (blocking per-thread). Rust tokio allows multiplexed concurrent connections across accounts without 1-thread-per-connection overhead | HIGH | Use `async-imap` with tokio runtime. Background + foreground sync become tokio tasks rather than OS threads. Reduces thread count from 5 to ~3 tokio tasks. Critical: IDLE still requires a dedicated connection (cannot multiplex IDLE with other IMAP commands on the same connection). |
| **Explicit timeout on all network ops** | C++ has hardcoded timeouts in mailcore2 that are invisible. Rust allows per-operation timeouts with `tokio::time::timeout()`, surfaced as meaningful errors | LOW | Default: 30s connect, 60s for large fetches (bodies). On timeout: retryable SyncException. Improves robustness with flaky mobile hotspots. |
| **Structured error classification** | C++ uses `ErrorCode` integers mapped to strings. Rust can use an enum error type that distinguishes auth failures, TLS errors, network timeouts, server-side errors — enabling smarter retry policies | MEDIUM | `SyncError` enum: `Auth(String)`, `Tls(String)`, `Network(String)`, `ServerSide(String)`, `RateLimited(Duration)`. Auth and server-side errors are non-retryable. Network and TLS are retryable. Enables smarter backoff decisions. |
| **Delta coalescing with Rust channels** | C++ uses a mutex+condition_variable on a map. Rust's `tokio::sync::mpsc` and `Mutex<HashMap>` provide the same semantics with tokio-aware blocking | MEDIUM | `DeltaStream` becomes an `Arc<Mutex<DeltaBuffer>>` with a `tokio::time::sleep` timer task. Same 500ms coalescing semantics. Benefit: no raw OS thread mutex — compatible with tokio's cooperative scheduler. |
| **Clean process exit on non-retryable errors** | C++ uses `abort()` which produces unclean exit and may omit the error message. Rust can flush the error delta to stdout before exit | LOW | On non-retryable error: write `{"error":"...","accountId":"..."}` delta, flush stdout, then `std::process::exit(1)`. TypeScript `CrashTracker` sees the exit code and the last written error, giving better diagnostics. |
| **Incremental initial sync with progress reporting** | C++ marks folders `busy` and doesn't report granular progress. Rust can emit folder `bodiesPresent`/`bodiesWanted` updates more frequently during initial sync | LOW | Emit folder status delta every N messages during body sync. The TypeScript side already consumes `bodiesPresent`/`bodiesWanted` for progress display — just needs more frequent updates from the Rust engine. |
| **Rate limiting compliance (CalDAV/CardDAV)** | The C++ DAVWorker already implements RFC 6585 (429) and RFC 7231 (Retry-After) backoff — carry this forward cleanly in Rust | MEDIUM | The C++ has: `applyRateLimitDelay()`, `recordRateLimitResponse()`, `parseRetryAfter()`. In Rust: a `RateLimiter` struct tracking backoff state, applied before each CalDAV/CardDAV HTTP request. The C++ implementation is solid — direct port, not redesign. |

### Anti-Features (Commonly Requested, Often Problematic)

| Feature | Why Requested | Why Problematic | Alternative |
|---------|---------------|-----------------|-------------|
| **Full email body storage in DB** | "Store everything for offline access" | Message bodies can be 25MB each with attachments; 10,000 messages = potentially gigabytes in SQLite. SQLite performs poorly with large blobs; vacuum becomes slow; DB file size balloons | Keep current approach: body stored in `data` column of Message table (fat row pattern), but only fetch bodies for recent/prioritized messages. Old messages: re-fetch on demand from IMAP when user opens them |
| **Push email via SMTP submission notifications** | Real-time delivery confirmation | SMTP does not support push back-channel; servers return success/error at send time only. "Delivery" to recipient MX is asynchronous and servers rarely notify senders of success | Track `SendDraftTask` outcome (SMTP success = delivered to server). Bounce notifications (NDR) arrive as regular email messages to inbox |
| **Per-folder IDLE connections** | "Watch all folders simultaneously for new mail" | N folders × 1 IDLE connection each = N IMAP connections per account. IMAP servers enforce per-account connection limits (typically 10–15 for Gmail, 4 for some providers). Would hit limits immediately for users with many folders | IDLE on primary folder only (inbox or All Mail). Background sync thread handles other folders on a schedule (shallow: 2 min, deep: 10 min) |
| **IMAP NOTIFY extension (RFC 5465)** | Reduces polling entirely; server pushes updates for all folders | Only Dovecot and Cyrus support NOTIFY; Gmail and Outlook do not. Adds protocol complexity for ~30% of servers | IDLE on primary folder + periodic background scan covers all servers including Gmail/Outlook |
| **EAS (Exchange ActiveSync) protocol** | Native Exchange protocol for O365/Exchange On-Premise | Requires license from Microsoft (patents); reverse-engineered implementations are legally risky; adds an entire separate protocol stack | IMAP + SMTP works for Exchange/O365 accounts. CalDAV/CardDAV via Exchange DAV endpoint works for calendars/contacts. Already supported by the existing C++ engine |
| **POP3 support** | Legacy accounts at some ISPs still use POP3 | POP3 has no folder concept, no flag sync, no server-side threading, no efficient delta sync. Would require entirely separate code path. Not in the existing C++ engine | Recommend IMAP migration to users. If a provider only offers POP3, document it as out-of-scope |
| **Metadata server dependency** | Plugin metadata sync requires identity server HTTP access | Metadata sync can fail (network, server down) without breaking core email sync. But if the MetadataWorker crashes it should not kill IMAP sync | Keep MetadataWorker as an independent worker that fails gracefully. Non-retryable metadata errors: stop that worker, do not propagate to IMAP sync |
| **Full MIME parsing in the engine** | Re-parse MIME on every body fetch to extract inline images, attachment lists | mailcore2's BODYSTRUCTURE parsing gets attachment metadata from headers without downloading bodies. Full MIME parse needed only when rendering — belongs in the Electron UI layer (TypeScript DOMPurify + sanitize-transformer pipeline) | Sync engine: store raw RFC 2822 body. UI: parse MIME for rendering. This is the current architecture — maintain it. |

---

## Feature Dependencies

```
stdin/stdout JSON protocol
    └──required by──> all other features (communications channel)

SQLite schema migration
    └──required by──> SQLite delta emission (schema must exist before saves)
    └──required by──> IMAP incremental sync (folder localStatus storage)
    └──required by──> Task processor (Task table must exist)

IMAP folder management
    └──required by──> IMAP incremental sync (need folder list before syncing)
    └──required by──> IMAP IDLE monitoring (need inbox/all folder to IDLE on)
    └──required by──> Message header sync (need folderId for message storage)

IMAP incremental sync (UID range)
    └──required by──> IMAP incremental sync CONDSTORE (baseline behavior needed for fallback)
    └──required by──> Message body caching (need header-synced messages before bodies)

IMAP IDLE monitoring
    └──requires──> IMAP folder management (to know which folder to IDLE on)
    └──requires──> foreground/background thread split (IDLE blocks a connection)

Task processor (local phase)
    └──required by──> Task processor (remote phase) (local must run before remote)

Task processor (remote phase - IMAP ops)
    └──requires──> IMAP folder management (move/copy need folder paths)
    └──requires──> Message header sync (tasks reference message UIDs)

SMTP send
    └──requires──> Task processor (SendDraftTask is a task)
    └──enhances──> Message header sync (Sent message appears in Sent folder after send)

OAuth2 token refresh
    └──required by──> IMAP incremental sync (XOAUTH2 accounts need valid tokens)
    └──required by──> SMTP send (sending via OAuth2 accounts needs valid token)
    └──required by──> CalDAV/CardDAV sync (Google CalDAV/CardDAV use same OAuth2 token)

CalDAV calendar sync
    └──requires──> SQLite delta emission (calendar events emitted as deltas)
    └──enhances──> Task processor (SyncbackEventTask, DestroyEventTask via DAVWorker)

CardDAV contact sync
    └──requires──> SQLite delta emission (contacts emitted as deltas)
    └──enhances──> Task processor (SyncbackContactTask, DestroyContactTask via DAVWorker)

Full-text search indexing
    └──requires──> Message body caching (can only index content we have locally)
    └──requires──> Message header sync (index subject, from, to fields)
    └──can run independently of──> IMAP IDLE monitoring

Metadata worker (plugin metadata sync)
    └──requires──> SQLite delta emission (metadata saves emit deltas)
    └──independent of──> IMAP sync (different HTTP channel, different thread)

Crash recovery
    └──requires──> Task processor (reset stuck tasks on launch)
    └──requires──> SQLite schema migration (DB must be valid before crash recovery reads)
```

### Dependency Notes

- **IMAP folder management before everything else:** The foreground worker cannot IDLE until it knows which folder is "inbox". The background worker cannot sync folders until it has the folder list. First background iteration runs `syncFoldersAndLabels()` before the foreground worker is even started.
- **OAuth2 token refresh is a cross-cutting concern:** Must be checked before any authenticated IMAP, SMTP, or CalDAV/CardDAV operation for OAuth2 accounts. In Rust, this belongs in a shared `TokenManager` accessed from all three worker types.
- **CONDSTORE requires QRESYNC for vanished messages:** Without QRESYNC, expunged messages during IDLE are processed from the IDLE VANISHED response but may not appear in CONDSTORE CHANGEDSINCE results (the server considers this connection already notified). The C++ code explicitly documents this gap and accepts it.
- **SQLite thread safety:** The C++ `MailStore::assertCorrectThread()` ensures thread affinity per MailStore instance. In Rust: each tokio task gets its own SQLite connection, or use `rusqlite::Connection` wrapped in an Arc<Mutex> per task. WAL mode (already used) allows concurrent reads from multiple connections.

---

## Gmail-Specific Behaviors

Gmail deviates from standard IMAP in significant ways that require explicit handling:

| Behavior | Standard IMAP | Gmail Behavior | Implementation Note |
|----------|--------------|----------------|---------------------|
| **Folder structure** | Hierarchical folders | Labels masquerading as folders. `[Gmail]/All Mail` contains all messages with labels as virtual views | Sync only: `INBOX`, `[Gmail]/All Mail`, `[Gmail]/Trash`, `[Gmail]/Spam`. Skip all `[Gmail]/Sent Mail` (sent messages appear in All Mail with \Sent flag). Skip all user label folders — they're subsets of All Mail. |
| **Folder capability** | `CONDSTORE` or `QRESYNC` | `CONDSTORE` only (no `QRESYNC`) | Must handle VANISHED during IDLE without QRESYNC. C++ workaround: process `session.idleVanishedMessages()` before the next CONDSTORE call. |
| **Label assignment** | Flags on messages | `X-GM-LABELS` FETCH attribute | Fetch `X-GM-LABELS` alongside standard flags to populate `Label` models. STORE with `X-GM-LABELS +FLAGS (label)` to add labels. `X-GM-THRID` provides native Gmail thread IDs. `X-GM-MSGID` provides the stable Gmail message ID (use instead of deriving from Message-ID header). |
| **Message deduplication** | Message appears in one folder | Same message in INBOX and All Mail | Detection: `X-GM-MSGID` is the same for the inbox view and All Mail view. Must not create two `Message` records for the same Gmail message ID. Reconcile: if a message exists in All Mail and also appears in INBOX fetch, upsert — don't insert. |
| **Sent Mail** | SMTP sends → Sent folder, IMAP APPEND | Gmail auto-adds sent messages to All Mail; SMTP submission does not require separate APPEND | Do not APPEND to `[Gmail]/Sent Mail` after sending. The message will appear automatically in All Mail with `\Sent` flag via Gmail's server-side behavior. |
| **Delete vs Archive** | IMAP STORE +FLAGS \Deleted + EXPUNGE | `[Gmail]/Trash` MOVE for delete; removing \Inbox label for archive | Delete: COPY to `[Gmail]/Trash`, then STORE +FLAGS \Deleted + EXPUNGE in source folder. Archive: STORE X-GM-LABELS -FLAGS \\Inbox. |
| **Contacts** | CardDAV at standard path | Google People API (separate from CardDAV) | `GoogleContactsWorker` uses Google People API v1 (OAuth2), not CardDAV. Standard CardDAV path still works for non-Gmail accounts. Must maintain both paths. |

---

## MVP Definition

This is a rewrite, not a new product. "MVP" means: **what is the minimum needed to fully replace the C++ binary** — the Rust binary must handle a production email account without regressing any existing behavior.

### Launch With (v2.0 — Full Engine Parity)

These features must all be present before the C++ binary can be deleted.

- [ ] **stdin/stdout JSON protocol** — exact wire format compatibility with TypeScript `MailsyncBridge`
- [ ] **IMAP folder management** — LIST, role detection, Gmail-specific folder filtering
- [ ] **IMAP incremental sync (UID range)** — for servers without CONDSTORE
- [ ] **IMAP incremental sync (CONDSTORE)** — for servers with CONDSTORE (Gmail, Dovecot, Cyrus, iCloud)
- [ ] **IMAP IDLE monitoring** — foreground worker, keep-alive, interrupt on task arrival
- [ ] **UIDVALIDITY change handling** — detect, reset, full re-sync
- [ ] **Message header sync** — FETCH ENVELOPE + BODYSTRUCTURE, thread grouping, stable IDs
- [ ] **Message body caching (lazy)** — background fetch, priority queue from `need-bodies`, per-folder policy
- [ ] **SMTP send** — RFC 2822 construction, TLS, password + XOAUTH2, multipart with attachments
- [ ] **Task processor (local + remote)** — all task types from TaskProcessor.hpp
- [ ] **Task cleanup** — startup reset + runtime expiry
- [ ] **SQLite delta emission** — persist/unpersist, coalescing, transactions, stream delay
- [ ] **SQLite schema migration** — all existing migrations as baseline, `--mode migrate`
- [ ] **OAuth2 token refresh** — XOAUTH2 SASL format, HTTP token exchange, updated-secrets delta
- [ ] **Crash recovery** — task state reset on launch, retryable/non-retryable classification
- [ ] **CalDAV calendar sync** — REPORT sync-collection, CREATE/UPDATE/DELETE events via iCalendar
- [ ] **CardDAV contact sync** — REPORT sync-collection, CREATE/UPDATE/DELETE contacts via vCard
- [ ] **Gmail Google People contact sync** — Google People API v1, separate from CardDAV path
- [ ] **Metadata worker** — HTTP long-polling from identity server, plugin metadata sync
- [ ] **Metadata expiration worker** — clean expired metadata entries
- [ ] **stdin orphan detection** — exit code 141 when parent dies
- [ ] **Process modes** — `sync`, `test` (auth validation), `reset`, `migrate`, `install-check`
- [ ] **Gmail-specific behaviors** — X-GM-LABELS, X-GM-MSGID, X-GM-THRID, folder whitelist, no APPEND for Sent

### Add After Validation (v2.x)

These improve the engine without being required for feature parity.

- [ ] **Explicit per-operation timeouts** — once parity is validated, add configurable timeouts to replace hardcoded ones
- [ ] **Structured error enum** — improves debugging and crash log quality; no user-facing impact
- [ ] **More frequent body sync progress updates** — UI improvement once core sync is proven
- [ ] **Async multiplexing of background/foreground workers** — reduce thread count using tokio tasks vs OS threads

### Future Consideration (v3+)

- [ ] **IMAP NOTIFY extension (RFC 5465)** — only after verifying >50% of target server population supports it
- [ ] **IMAP BINARY extension (RFC 3516)** — efficient large attachment fetch; only if body sync performance is measured to be a bottleneck
- [ ] **Full-text search indexing in engine** — currently search is server-side IMAP SEARCH or client-side SQLite FTS5; in-engine indexing would accelerate offline search. Deferred: requires design decision on where FTS5 index lives (currently in Electron's SQLite via DatabaseStore)

---

## Feature Prioritization Matrix

| Feature | User Value | Implementation Cost | Priority |
|---------|------------|---------------------|----------|
| stdin/stdout JSON protocol | HIGH | MEDIUM | P1 |
| SQLite schema migration + delta emission | HIGH | HIGH | P1 |
| IMAP folder management | HIGH | MEDIUM | P1 |
| IMAP incremental sync (UID range) | HIGH | HIGH | P1 |
| IMAP IDLE monitoring | HIGH | HIGH | P1 |
| Message header sync | HIGH | MEDIUM | P1 |
| Task processor (local + remote, IMAP tasks) | HIGH | HIGH | P1 |
| SMTP send (SendDraftTask) | HIGH | MEDIUM | P1 |
| IMAP incremental sync (CONDSTORE) | HIGH | HIGH | P1 |
| UIDVALIDITY change handling | HIGH | LOW | P1 |
| OAuth2 token refresh | HIGH | MEDIUM | P1 |
| Crash recovery + task cleanup | HIGH | MEDIUM | P1 |
| Message body caching (lazy) | HIGH | HIGH | P1 |
| CalDAV calendar sync | MEDIUM | HIGH | P1 |
| CardDAV contact sync | MEDIUM | HIGH | P1 |
| Gmail-specific behaviors (X-GM-*) | HIGH | MEDIUM | P1 |
| Metadata worker | MEDIUM | MEDIUM | P1 |
| Process modes (test, reset, migrate) | MEDIUM | LOW | P1 |
| stdin orphan detection | LOW | LOW | P1 |
| Gmail Google People contact sync | MEDIUM | MEDIUM | P1 |
| Explicit timeouts | MEDIUM | LOW | P2 |
| Structured error enum | LOW | MEDIUM | P2 |
| Async multiplexing (tokio tasks vs threads) | LOW | HIGH | P2 |
| IMAP NOTIFY | LOW | HIGH | P3 |
| In-engine FTS5 indexing | MEDIUM | HIGH | P3 |

**Priority key:**
- P1: Must have before C++ binary can be deleted
- P2: Should have, add when parity is proven
- P3: Nice to have, future release

---

## Complexity Hotspots

The following features have higher implementation complexity than their brief descriptions suggest:

### SQLite Delta Emission (Highest Systemic Risk)

The delta stream is not a simple "emit on save" — it has coalescing, transactions, stream delays, and thread-safety requirements. Key subtleties from the C++ source:

- `_transactionDeltas` buffer: deltas accumulated within a transaction are held in memory and emitted only on `commitTransaction()`. If `rollbackTransaction()` is called, deltas are discarded (never emitted).
- Coalescing: multiple saves of the same object ID within a flush window are merged. The merge strategy: take the union of all JSON keys, with later values overwriting earlier ones. This preserves conditional fields (e.g., `body` is only included when body has been fetched; a save without `body` must not overwrite a previous save that had `body`).
- Stream delay: background sync uses 500ms delay (prevents UI flooding during bulk operations). Task execution (main thread) uses 5ms (user wants immediate feedback).
- Thread safety: the C++ uses `bufferMtx` to protect the buffer map. In Rust: `Arc<Mutex<DeltaBuffer>>` with a tokio task flushing on a timer.

### IMAP CONDSTORE + IDLE Interaction (Medium Risk)

The C++ source documents a known issue: VANISHED messages seen during IDLE may not appear in the subsequent `FETCH CHANGEDSINCE` because the server considers this connection already notified. The solution in C++:

```cpp
// Process VANISHED notifications received during the previous IDLE session.
// The server sends VANISHED during IDLE when messages are expunged, but won't
// re-report them in the subsequent FETCH CHANGEDSINCE since it considers this
// connection already informed.
IndexSet * idleVanished = session.idleVanishedMessages();
```

In Rust: the foreground task must accumulate `VANISHED` UIDs from IDLE responses in a per-session buffer and process them at the start of the next sync cycle, before issuing `FETCH CHANGEDSINCE`.

### Gmail Folder Whitelist + Label Reconciliation (Medium Risk)

Gmail exposes all labels as IMAP folders. Without the whitelist, syncing all folders causes:
1. Message duplication: INBOX and All Mail both contain the same messages
2. Combinatorial explosion: every label folder contains a subset of All Mail messages

The whitelist: sync only `INBOX`, `[Gmail]/All Mail`, `[Gmail]/Trash`, `[Gmail]/Spam`. All other `[Gmail]/*` folders: create a `Label` model (for display in sidebar) but do not sync messages from them.

Message reconciliation: when a message appears in both `INBOX` and `All Mail` during the same sync iteration, the All Mail copy is authoritative (it carries `X-GM-LABELS`). Use `X-GM-MSGID` as the deduplication key — if a message with the same Gmail ID exists, update instead of insert.

### CalDAV/CardDAV Sync-Token State Machine (Medium Risk)

The sync-collection REPORT protocol has a state machine:

1. First sync: PROPFIND to get initial `sync-token`
2. Store token and all resource ETAGs
3. Next sync: REPORT with stored `sync-token` → server returns changed/deleted resources + new token
4. If server returns `507 Insufficient Storage` or reports token expired: fall back to full re-sync
5. Google CardDAV: tokens valid for 29 days; after expiry, `sync-token` REPORT returns 403 — must re-sync

The C++ `DAVWorker::runForAddressBookWithSyncToken()` handles this with a `retryCount` parameter. In Rust: same state machine, but use `reqwest` (or `ureq`) for HTTP requests instead of `libcurl`.

### Task Atomicity (Low-Medium Risk)

Tasks must be atomic: if `performLocal` succeeds but `performRemote` fails, the local DB change is already committed. On next startup, `cleanupTasksAfterLaunch()` resets the task to `local` state so `performLocal` runs again. This means `performLocal` must be idempotent — running it twice must produce the same result as running it once. Verify idempotency for all task types, especially `SyncbackCategoryTask` (folder creation) and `SyncbackContactTask` (contact upsert).

---

## Phase-Specific Behaviors

Ordering from the C++ main.cpp thread startup sequence:

### Phase 1 Behaviors: Protocol + SQLite Foundation
These must exist before any network activity:
- stdin/stdout JSON protocol (wire format compatibility test)
- SQLite schema migration (`--mode migrate`)
- Delta emission infrastructure (transactions, coalescing, stream delay)
- Account parsing from `--account` JSON arg or stdin
- Process modes: `migrate`, `reset`, `install-check`

### Phase 2 Behaviors: IMAP Core Sync
These implement the background thread iteration:
- IMAP folder management (LIST, role detection)
- IMAP incremental sync — UID range (no CONDSTORE fallback)
- IMAP incremental sync — CONDSTORE
- UIDVALIDITY change handling
- Message header sync
- Gmail folder whitelist and X-GM-LABELS fetch
- OAuth2 token refresh

### Phase 3 Behaviors: Foreground (IDLE + Tasks)
These implement the foreground thread:
- IMAP IDLE monitoring
- Task processor — local phase (all task types)
- Task processor — remote phase (IMAP tasks: move, flag, label, expunge)
- SMTP send (SendDraftTask remote phase)
- Message body caching (lazy, priority queue from `need-bodies`)
- Crash recovery (task cleanup at launch)

### Phase 4 Behaviors: CalDAV/CardDAV + Metadata
These implement the calContacts and metadata threads:
- CalDAV calendar sync
- CardDAV contact sync
- Gmail Google People contact sync
- Metadata worker (HTTP long-polling)
- Metadata expiration worker
- Process mode: `sync-calendar` (stdin command)

---

## Sources

- C++ source read directly: `app/mailsync/MailSync/main.cpp`, `SyncWorker.cpp`, `SyncWorker.hpp`, `TaskProcessor.hpp`, `DAVWorker.hpp`, `MetadataWorker.hpp`, `MailStore.hpp`, `DeltaStream.hpp`, `XOAuth2TokenManager.hpp`
- C++ CLAUDE.md read directly: `app/mailsync/CLAUDE.md` (architecture and threading model)
- [RFC 7162 — IMAP CONDSTORE + QRESYNC](https://datatracker.ietf.org/doc/html/rfc7162) — HIGH confidence (authoritative standard)
- [RFC 2177 — IMAP IDLE](https://datatracker.ietf.org/doc/html/rfc2177) — HIGH confidence (authoritative standard)
- [RFC 4549 — Disconnected IMAP Clients](https://datatracker.ietf.org/doc/html/rfc4549) — HIGH confidence (canonical UIDVALIDITY handling)
- [Gmail IMAP Extensions](https://developers.google.com/workspace/gmail/imap/imap-extensions) — HIGH confidence (Google official docs)
- [sabre/dav — Building a CalDAV client](https://sabre.io/dav/building-a-caldav-client/) — MEDIUM confidence (reference implementation guide)
- [Google CardDAV API](https://developers.google.com/people/carddav) — HIGH confidence (Google official docs)
- [async-imap crate docs](https://docs.rs/async-imap/) — HIGH confidence (docs.rs)
- [MailKit CONDSTORE/QRESYNC Issue #805](https://github.com/jstedfast/MailKit/issues/805) — MEDIUM confidence (real-world implementation reference)
- [SQLite FTS5 documentation](https://www.sqlite.org/fts5.html) — HIGH confidence (SQLite official docs)

---

*Feature research for: Rust mailsync engine rewrite (v2.0 milestone)*
*Researched: 2026-03-02*
