# Project Research Summary

**Project:** UnifyMail v2.0 — Rust Mailsync Engine Rewrite
**Domain:** Standalone Rust binary replacing a C++ IMAP/SMTP/CalDAV/CardDAV sync engine for a desktop Electron email client
**Researched:** 2026-03-02
**Confidence:** HIGH

## Executive Summary

This milestone rewrites the existing C++ mailsync binary (~16,200 LOC, 50 source files) as a standalone Rust binary that maintains exact wire-format compatibility with the existing TypeScript `MailsyncBridge` consumer in Electron. The Rust binary is a drop-in replacement — same command-line interface, same stdin/stdout newline-delimited JSON protocol, same SQLite schema. The key architectural reality is that the Rust engine is a child process, not a Node.js addon, which eliminates the tokio runtime ownership constraints and BoringSSL conflicts present in the v1.0 N-API milestone. Experts building Rust email sync engines (Delta Chat being the primary production reference) use `async-imap` with the `runtime-tokio` feature, `tokio-rusqlite` for non-blocking SQLite access, and `lettre` for SMTP — a well-established trio for async email in Rust.

The recommended approach follows a strict dependency-driven build order: protocol and database infrastructure first, then IMAP sync, then IDLE and task execution, then CalDAV/CardDAV and metadata. This order is non-negotiable because the delta emission layer must exist before any IMAP sync code can be tested end-to-end with the Electron UI. A critical architectural pattern is the single dedicated stdout flush task that owns exclusive write access to stdout — all other tokio tasks route deltas through an `mpsc` channel to this task. Missing this pattern produces the most severe bug class: the UI appears frozen because deltas never reach Electron (Rust's stdout uses full block buffering when not connected to a TTY).

The primary risk is behavioral regression from implicit protocol contracts. The C++ engine has accumulated contracts that live only in the TypeScript consumer code: field names like `modelJSONs` (uppercase JSON — serde's auto-camelCase produces `modelJsons` which is wrong), `ProcessState` model class for connection status indicators, and the two-line startup handshake (account JSON then identity JSON on stdin before any sync). All 20+ features required for C++ binary deletion are P1 with no deferral possible — this is a parity rewrite, not a new product. The recommended mitigation is building a contract test harness in Phase 1, before writing any IMAP code, that validates every message type against the TypeScript parser expectations.

## Key Findings

### Recommended Stack

The Rust sync engine requires approximately 25 Cargo dependencies, all from the tokio ecosystem. Every major async crate in the stack shares tokio 1.x, eliminating runtime conflicts. The `bundled` feature in rusqlite is mandatory to embed SQLite 3.51.1 — macOS ships an outdated system SQLite and Windows has none. The most significant crate discovery is `libdav 0.10.2` (released 2026-02-10), a production-ready CalDAV/CardDAV client that replaces approximately 1,000 lines of WebDAV discovery and PROPFIND parsing that would otherwise need to be implemented manually.

See `.planning/research/STACK.md` for the complete Cargo.toml template and version compatibility matrix.

**Core technologies:**
- `tokio 1.x` (rt-multi-thread, sync, io-util, macros, fs): Async runtime — the binary owns its own runtime via `#[tokio::main]`; no napi-rs ownership conflict
- `async-imap 0.11.2` (runtime-tokio): IMAP client — the only maintained async IMAP crate; IDLE and CONDSTORE confirmed via source inspection; QRESYNC typed API absent (CONDSTORE-only for MVP)
- `lettre 0.11.19` (tokio1, rustls-tls, builder, smtp-transport): SMTP send — XOAUTH2 built-in; MIME builder handles multipart attachments
- `tokio-rustls 0.26.4` + `rustls-platform-verifier 0.6.2`: TLS — pure Rust, no OpenSSL, OS trust store
- `rusqlite 0.38.0` (bundled, serde_json): SQLite — bundled SQLite 3.51.1; WAL mode; fat-row JSON schema preserved from C++
- `tokio-rusqlite 0.6`: Async SQLite bridge — serializes all access through a dedicated background thread per connection; prevents tokio thread starvation
- `libdav 0.10.2`: CalDAV/CardDAV — service discovery, PROPFIND parsing, sync-collection support
- `mail-parser 0.11.2`: MIME parsing — zero-copy, 41 character sets, production-validated in Stalwart Mail Server
- `calcard 0.3.2`: iCalendar and vCard parsing — replaces C++ vendored icalendarlib and inline vCard parsing in DAVWorker
- `ammonia 4.1.2`: HTML sanitization — whitelist-based; 4.1.2 fixes RUSTSEC-2025-0071 (do not use 4.1.1 or earlier)
- `oauth2 5.0.0` (reqwest): Token refresh — provider-agnostic RFC 6749; covers Gmail, Outlook, Yahoo
- `serde 1.x` + `serde_json 1.x`: Protocol serialization — newline-delimited JSON for stdin/stdout IPC
- `tracing 0.1` + `tracing-subscriber 0.3`: Logging — async-aware spans; stderr only (stdout reserved for delta protocol)

**Critical version requirement:** `ammonia` must be `4.1.2` or later — earlier versions have RUSTSEC-2025-0071.

### Expected Features

The feature set is fully defined by the C++ source code. Every feature below already exists in the C++ engine and must be replicated exactly. There is no "launch with subset" option — the Rust binary must handle a production account without any regression before the C++ binary can be deleted.

See `.planning/research/FEATURES.md` for full behavioral specifications, feature dependency graph, and phase-specific behavior groupings.

**Must have (table stakes — all P1, required before C++ deletion):**
- stdin/stdout JSON protocol — exact wire format; `modelJSONs` (uppercase JSON), `modelClass`, `type` field names required
- IMAP folder management — LIST, role detection, Gmail folder whitelist (INBOX, All Mail, Trash, Spam only)
- IMAP incremental sync (UID range) — fallback for servers without CONDSTORE
- IMAP incremental sync (CONDSTORE) — modseq-based; Gmail supports CONDSTORE but not QRESYNC
- IMAP IDLE monitoring — foreground worker; 29-minute re-IDLE loop; interrupt on task arrival
- UIDVALIDITY change handling — detect, reset, full re-sync; RFC 4549 canonical behavior
- Message header sync — FETCH ENVELOPE + BODYSTRUCTURE; stable IDs from message headers; thread grouping
- Message body caching (lazy) — priority from `need-bodies` stdin command; per-folder age policy
- SMTP send — RFC 2822 MIME construction; TLS; password + XOAUTH2; multipart with attachments
- Task processor (local + remote) — all 13+ task types from TaskProcessor.hpp; idempotent local phase
- Task cleanup — startup reset of `remote`-state tasks; runtime expiry of completed tasks
- SQLite delta emission — persist/unpersist; 500ms coalescing window; transaction batching
- SQLite schema migration — all C++ migrations as baseline; `--mode migrate` entry point
- OAuth2 token refresh — XOAUTH2 SASL; HTTP token exchange; updated-secrets delta to UI
- Crash recovery — task reset on launch; retryable/non-retryable error classification
- CalDAV calendar sync — sync-collection REPORT; CREATE/UPDATE/DELETE via iCalendar
- CardDAV contact sync — sync-collection REPORT; CREATE/UPDATE/DELETE via vCard
- Gmail Google People API — OAuth2 contacts path (separate from standard CardDAV)
- Metadata worker — HTTP long-polling from identity server; plugin metadata sync
- Process modes — `sync`, `test`, `reset`, `migrate`, `install-check`
- Gmail-specific behaviors — X-GM-LABELS, X-GM-MSGID, X-GM-THRID; no APPEND for Sent
- stdin orphan detection — exit code 141 when parent closes stdin

**Should have (v2.x improvements, not required for C++ deletion):**
- Explicit per-operation timeouts via `tokio::time::timeout()` — surfaces meaningful errors for flaky networks
- Structured `SyncError` enum — distinguishes auth/TLS/network/server errors for smarter retry policy
- More frequent body sync progress updates — UI improvement once parity is proven
- Async multiplexing of workers via tokio tasks vs OS threads — reduce thread count

**Defer (v3+):**
- IMAP NOTIFY extension (RFC 5465) — only ~30% of servers support it; Gmail/Outlook do not
- IMAP BINARY extension (RFC 3516) — only if body sync is measured as a bottleneck
- In-engine FTS5 full-text search indexing — requires design decision on index location
- QRESYNC typed API — contribute upstream to chatmail/async-imap after CONDSTORE parity is validated

### Architecture Approach

The architecture mirrors the C++ threading model with tokio tasks replacing OS threads. The process has five primary components: a stdin loop task dispatching JSON commands, a background sync task iterating all folders with CONDSTORE, a foreground IDLE task monitoring the primary folder and executing task remote phases, a cal/contacts sync task for CalDAV/CardDAV, and a metadata sync task for the identity server. All components share three singleton types via `Arc`: `MailStore` (SQLite access), `DeltaStream` (stdout channel), and `Account` (credentials). The single-writer SQLite pattern via `tokio-rusqlite` and the exclusive-stdout DeltaStream flush task are the two non-negotiable structural patterns — both prevent deadlocks that are impossible to diagnose after the fact.

See `.planning/research/ARCHITECTURE.md` for complete project directory structure, all seven architectural patterns with code examples, and the full build order diagram.

**Major components:**
1. `stdin_loop` task — reads newline-delimited JSON commands; dispatches all 8 command types; detects stdin EOF as orphan signal (exit code 141)
2. `MailStore` (rusqlite via tokio-rusqlite) — single writer connection; separate reader connection; WAL mode; `busy_timeout=5000`; all writes produce delta items
3. `DeltaStream` — `mpsc::UnboundedSender` into dedicated flush task; 500ms coalescing window; exclusive stdout ownership; merges repeated saves of same model
4. `background_sync` task — IMAP session for folder iteration; CONDSTORE incremental sync; body fetch scheduling; UID range fallback
5. `foreground_idle` task — second IMAP session for IDLE on primary folder; `tokio::select!` on IDLE notification vs interrupt channel vs shutdown; task remote phase execution
6. `TaskProcessor` — local phase (immediate DB write) on `queue-task` receipt; remote phase (IMAP/SMTP/DAV operation) in foreground task; 13+ task type dispatch
7. `cal_contacts_sync` task — libdav CalDAV/CardDAV; Google People API for Gmail contacts; rate limiting (RFC 6585 Retry-After)
8. `metadata_sync` task — HTTP long-polling from identity server via reqwest; independent failure (non-retryable errors stop only this worker, not IMAP sync)

### Critical Pitfalls

See `.planning/research/PITFALLS.md` for full details including warning signs, recovery strategies, and phase assignments.

1. **stdout not flushed when spawned as child process** — Rust's stdout uses full block buffering when not connected to a TTY. Use `BufWriter::new(io::stdout())` with explicit `.flush()` after every message. Deltas pile up invisibly; UI never updates. Address in Phase 1 before writing any IMAP code.

2. **Blocking rusqlite calls on tokio worker threads cause starvation** — `rusqlite::Connection` is synchronous; calling it in `async fn` blocks tokio worker threads. Under load with multiple folders syncing, all threads block on SQLite fsync and the binary appears frozen. Use `tokio-rusqlite::Connection::call()` for every database operation. Address in Phase 2 before IMAP sync code exists.

3. **Delta JSON field name mismatch causes silent UI drops** — TypeScript parser requires `modelJSONs` (uppercase JSON — serde's `rename_all = "camelCase"` produces `modelJsons` which is wrong), `modelClass` (not `objectClass`), `type`. Silent drops with console warning only. Use `#[serde(rename = "modelJSONs")]` explicitly. Address in Phase 1.

4. **IMAP IDLE 29-minute timeout causes silent disconnection** — Server closes TCP connection after ~30 minutes of IDLE; socket enters `CLOSE-WAIT`; `async-imap` IDLE future hangs forever. Real-world failure confirmed in deltachat-core-rust issue #5093. Wrap IDLE `wait()` in `tokio::time::timeout(25 * 60s)`. Address in Phase 4.

5. **stdin deadlock under large task payloads** — OS pipe buffer is 64 KiB; large HTML drafts (~500 KB) exceed it. If stdin reading and stdout writing are not on independent tasks, both processes deadlock. stdin reader and stdout writer must be dedicated independent tokio tasks sharing no mutex. Address in Phase 1.

6. **Implicit C++ protocol behavioral contracts** — TypeScript consumer enforces: two-line startup handshake (account then identity JSON), ProcessState messages for connection status, specific exit codes and JSON structure per mode, SIGTERM must exit 0 not SIGABRT. Missing any causes silent failure or app crash. Build a binary skeleton handling all modes with these contracts before sync logic. Address in Phase 1.

7. **CONDSTORE-only fallback missing for Gmail** — Gmail supports CONDSTORE but not QRESYNC. `async-imap 0.11.2` has `select_condstore()` but no typed QRESYNC API. Use CONDSTORE-only for MVP; QRESYNC via raw command is high complexity with fragile response parsing. Address in Phase 3.

## Implications for Roadmap

The dependency graph from FEATURES.md is unambiguous: infrastructure before protocol, protocol before IMAP, IMAP background before IMAP foreground (IDLE + tasks), CalDAV/CardDAV independently after IMAP is stable. The phase structure maps directly from the architecture research build order.

### Phase 1: Core Infrastructure and IPC Protocol

**Rationale:** Every subsequent phase depends on the delta emission pipeline and protocol contract being correct. No IMAP sync can be tested with the Electron UI until the binary can emit valid deltas. Five of the seven critical pitfalls strike in this phase — it is the highest-risk phase per unit of code written.

**Delivers:** A binary skeleton that handles all five process modes (`sync`, `test`, `migrate`, `reset`, `install-check`) with correct startup handshake, stdout flushing, stdin EOF detection, delta emission structure, and SQLite schema creation. Produces no mail yet but passes contract tests against the TypeScript protocol parser.

**Addresses (from FEATURES.md):**
- stdin/stdout JSON protocol (wire format compatibility)
- SQLite schema migration (`--mode migrate`)
- Delta emission infrastructure (transactions, coalescing, 500ms stream delay)
- Process modes and two-line startup handshake
- stdin orphan detection

**Avoids (from PITFALLS.md):**
- stdout buffering (flush pattern established at project start)
- stdin deadlock (dedicated independent stdin/stdout tokio tasks)
- Delta JSON format mismatch (contract test validates all field names)
- Protocol behavioral contracts (all modes tested in isolation)
- TLS OpenSSL dependency (rustls locked in at project creation via `cargo tree | grep openssl`)

**Research flag:** No additional research needed. Patterns are well-established; tokio-rusqlite and delta coalescing are documented.

### Phase 2: SQLite Layer and Model Infrastructure

**Rationale:** All IMAP sync output lands in the database before being emitted as deltas. The SQLite layer must be proven correct — especially the tokio-rusqlite single-writer pattern and WAL mode configuration — before any concurrent IMAP workers write to it.

**Delivers:** Complete `MailStore` with reader/writer connections, WAL mode, `busy_timeout=5000`, all data model types (Message, Thread, Folder, Label, Contact, Calendar, Event, Task), and migration scripts matching the existing C++ schema.

**Addresses (from FEATURES.md):**
- SQLite delta emission (complete implementation with coalescing and transaction batching)
- All model types required for IMAP sync output

**Avoids (from PITFALLS.md):**
- Blocking rusqlite calls on tokio threads (tokio-rusqlite enforced at layer boundary)
- WAL mode + busy_timeout configured correctly (no SQLITE_BUSY under concurrent load)

**Uses (from STACK.md):** `rusqlite 0.38` (bundled), `tokio-rusqlite 0.6`, `serde`/`serde_json`

**Research flag:** No additional research needed. rusqlite and tokio-rusqlite patterns are well-documented.

### Phase 3: IMAP Background Sync Worker

**Rationale:** The background sync worker is the engine's primary value delivery. It must exist and be correct before the foreground IDLE worker, because the foreground worker starts only after the background worker completes its first folder iteration — the same sequencing as the C++ engine (confirmed in C++ main.cpp).

**Delivers:** Full IMAP sync against a live account: folder enumeration, CONDSTORE incremental sync, UID range fallback, message header parsing, body caching, UIDVALIDITY handling, OAuth2 token refresh, Gmail-specific behaviors (X-GM-LABELS, X-GM-MSGID, folder whitelist).

**Addresses (from FEATURES.md):**
- IMAP folder management
- IMAP incremental sync (UID range and CONDSTORE)
- UIDVALIDITY change handling
- Message header sync
- Message body caching (lazy)
- OAuth2 token refresh
- Gmail-specific behaviors (X-GM-LABELS, X-GM-MSGID, X-GM-THRID)

**Avoids (from PITFALLS.md):**
- CONDSTORE-only fallback for Gmail (no QRESYNC in v2.0; `select_condstore()` used directly)
- QRESYNC ENABLE-before-SELECT ordering (capability detection before SELECT, ENABLE issued post-auth)
- OAuth2 token expiry during session (check expiry within 5 minutes before every IMAP authenticate)

**Uses (from STACK.md):** `async-imap 0.11.2` (runtime-tokio), `tokio-rustls`, `rustls-platform-verifier`, `mail-parser 0.11.2`, `oauth2 5.0.0`, `ammonia 4.1.2`

**Research flag:** No additional research needed. async-imap CONDSTORE API confirmed via source inspection; Delta Chat is a working reference implementation.

### Phase 4: Foreground IDLE and Task Execution

**Rationale:** IDLE requires a separate IMAP connection from the background sync session — sharing one session is an anti-pattern (IMAP is strictly sequential at the protocol level; concurrent commands on one session produce protocol errors). Task execution (remote phase) runs in the foreground task after IDLE interruption, using the live session. SMTP send is a task and belongs here.

**Delivers:** IDLE monitoring with 29-minute re-IDLE loop, task interrupt mechanism via `tokio::sync::watch`, all task remote phases (move, flag, label, expunge, send), SMTP send via lettre, crash recovery on launch.

**Addresses (from FEATURES.md):**
- IMAP IDLE monitoring
- Task processor (local and remote phases, all 13+ task types)
- SMTP send (SendDraftTask)
- Task cleanup (startup reset, runtime expiry)
- Crash recovery

**Avoids (from PITFALLS.md):**
- IDLE 29-minute disconnect (25-minute `tokio::time::timeout` loop with `DONE` before re-IDLE)
- IMAP IDLE + background sync on same session (two separate IMAP connections per account)
- Task atomicity (performLocal idempotent; performRemote retry via cleanupTasksAfterLaunch)
- STARTTLS SNI missing (hostname passed explicitly as SNI parameter)

**Uses (from STACK.md):** `lettre 0.11.19`, `tokio::select!`, `tokio::sync::watch` for interrupt channel

**Research flag:** Verify lettre's MIME multipart builder API covers inline images (CID references) and text/html + text/plain alternatives before coding begins. The C++ `performRemoteSendDraft` handles these cases via mailcore2's MIME builder.

### Phase 5: CalDAV, CardDAV, and Metadata Workers

**Rationale:** These workers are independent of IMAP sync — they share only the `MailStore` and `DeltaStream`. They are grouped because they share the `libdav` crate and HTTP infrastructure. The metadata worker is the simplest and should be implemented last within this phase.

**Delivers:** CalDAV calendar sync (sync-collection REPORT, CREATE/UPDATE/DELETE events), CardDAV contact sync (CREATE/UPDATE/DELETE contacts), Gmail Google People API contacts, metadata worker HTTP long-polling, sync-calendar stdin command handling.

**Addresses (from FEATURES.md):**
- CalDAV calendar sync
- CardDAV contact sync
- Gmail Google People contact sync
- Metadata worker and expiration worker
- `sync-calendar` stdin command

**Avoids (from PITFALLS.md):**
- CalDAV ETag sync loop (GET-after-PUT when server omits ETag; sync-token fallback for servers without sync-collection)
- CalDAV REPORT missing `Depth: 1` header
- vCard invalid UTF-8 (calcard handles forgivingly)
- Rate limiting (RFC 6585 Retry-After backoff; port C++ RateLimiter pattern)
- CalDAV/CardDAV token expiry (shared OAuth2 `TokenManager` handles all three worker types)

**Uses (from STACK.md):** `libdav 0.10.2`, `reqwest 0.13`, `calcard 0.3.2`, `quick-xml 0.37`

**Research flag:** CalDAV server behavior variation for ETag after PUT, sync-token expiry (Google 29-day limit), and `507 Insufficient Storage` reset warrants a focused research pass before implementing the sync-collection state machine. Server matrix: Google Calendar, iCloud, Nextcloud/Baikal, Exchange Online.

### Phase 6: Cross-Platform Builds, Packaging, and C++ Deletion

**Rationale:** The binary is useless if it cannot reach users. Packaging must be verified before the C++ binary is deleted. The `asarUnpack` configuration and dev fallback path in `mailsync-process.ts` must be updated to match Cargo's output path (not the C++ CMake path).

**Delivers:** Cross-platform binaries (Windows MSVC, macOS Intel, macOS Apple Silicon, Linux x64, Linux arm64), verified asar unpacking in production build, stripped release binaries under 15 MB, C++ source deletion.

**Addresses (from FEATURES.md):** All process modes fully tested; binary distributed and loadable in production packaging

**Avoids (from PITFALLS.md):**
- Binary not found in production (update `asarUnpack` in electron-builder config; update dev fallback path in `mailsync-process.ts`)
- Cross-compilation failures (cargo-xwin for Windows MSVC, cargo-zigbuild for Linux arm64)

**Uses (from STACK.md):** `cargo-xwin`, `cargo-zigbuild`, `cargo-bloat`, `cargo-audit`

**Research flag:** No additional research needed. Cross-compilation targets and tooling are identical to v1.0.

### Phase Ordering Rationale

- **Phases 1-2 before everything:** The delta protocol and SQLite layer are the foundation. No IMAP code can be tested with Electron until Phase 1 is complete. Protocol contract failures discovered late require refactoring across all worker code.
- **Phase 3 before Phase 4:** Background sync must complete its first folder iteration before the foreground IDLE task is safe to start — this is the same sequencing as the C++ engine and is enforced architecturally.
- **Phase 5 independent of Phases 3-4:** CalDAV/CardDAV workers share no IMAP session state. They can be developed in parallel with Phases 3-4 if team size allows, but require Phase 2 (MailStore) to be complete.
- **Phase 6 last:** The C++ binary cannot be deleted until all other phases are proven in production with live accounts. Packaging validation is a gate condition for deletion.

### Research Flags

Phases likely needing deeper research during planning:
- **Phase 4:** lettre multipart MIME API for attachments and inline images — validate builder API coverage against all cases in C++ `performRemoteSendDraft` before coding begins
- **Phase 5:** CalDAV server behavior variation (ETag after PUT, sync-token expiry, `507` reset, Exchange Online compatibility) — a server compatibility matrix research session is recommended before implementing the sync-collection state machine

Phases with standard patterns (skip research-phase):
- **Phase 1:** tokio task architecture and stdout flush patterns are thoroughly documented in official tokio docs and confirmed pitfalls
- **Phase 2:** rusqlite + tokio-rusqlite single-writer pattern is well-established with official API docs
- **Phase 3:** async-imap CONDSTORE API confirmed via direct source inspection; Delta Chat (deltachat-core-rust) is a working reference implementation
- **Phase 6:** Cross-compilation tools and targets are identical to the v1.0 milestone — no new territory

## Confidence Assessment

| Area | Confidence | Notes |
|------|------------|-------|
| Stack | HIGH | All crate versions verified against docs.rs; CONDSTORE confirmed via async-imap source inspection; QRESYNC gap confirmed via multiple searches and source inspection; libdav 0.10.2 freshness verified (released 2026-02-10); ammonia RUSTSEC fix version confirmed |
| Features | HIGH | C++ source read directly (main.cpp, SyncWorker, TaskProcessor, DAVWorker, MetadataWorker, DeltaStream, MailStore, XOAuth2TokenManager); RFC standards verified; every feature traced to a specific C++ source file |
| Architecture | HIGH | C++ source and CLAUDE.md read directly for threading model; tokio patterns from official docs; tokio-rusqlite API from docs.rs; all async-imap IDLE/CONDSTORE patterns verified; anti-patterns documented with concrete examples |
| Pitfalls | HIGH | IPC pitfalls verified against live TypeScript source (mailsync-bridge.ts, mailsync-process.ts); IDLE disconnect confirmed via deltachat-core-rust issue #5093; rusqlite async issue verified against rusqlite#697; stdout buffering confirmed via rust-lang/rust#60673 and tokio#7174; pipe deadlock confirmed via rust-lang/rust#45572 |

**Overall confidence:** HIGH

### Gaps to Address

- **QRESYNC decision point:** Research confirms CONDSTORE-only for MVP and defers QRESYNC. This decision should be explicitly tracked. If any production server shows significantly worse reconnect behavior without QRESYNC, the raw-command approach should be evaluated during Phase 3. Estimated: affects <5% of use cases.

- **lettre multipart attachment API coverage:** The SMTP send task in C++ handles multipart with attachments and inline images. lettre's `builder` API can do this, but the exact API surface for inline image CID references should be validated before Phase 4 coding begins.

- **CalDAV server compatibility matrix:** The research identifies the ETag loop pitfall and sync-token expiry as known risks, but server-specific behaviors for Exchange Online, iCloud, and Nextcloud are not fully characterized. A targeted Phase 5 research pass is recommended before implementation.

- **Google People API v1 currency:** The C++ engine uses Google People API for Gmail contacts. Verify the API v1 endpoint and OAuth2 scope requirements are still current before Phase 5 implementation — Google has been migrating People API surfaces.

## Sources

### Primary (HIGH confidence)

- C++ source read directly: `app/mailsync/MailSync/main.cpp`, `SyncWorker.hpp/cpp`, `TaskProcessor.hpp`, `DAVWorker.hpp`, `MetadataWorker.hpp`, `MailStore.hpp`, `DeltaStream.hpp`, `XOAuth2TokenManager.hpp` — ground truth for all feature and architecture research
- `app/mailsync/CLAUDE.md` — threading model, vendor library list, build system overview
- `CLAUDE.md` (project root) — IPC protocol, task system, sync engine communication diagram
- `app/src/browser/mailsync-bridge.ts` — live TypeScript consumer; delta message format; ProcessState handling
- `app/src/browser/mailsync-process.ts` — live TypeScript consumer; mode handling; binary path resolution; startup handshake
- [docs.rs/async-imap](https://docs.rs/async-imap/latest/async_imap/) — version 0.11.2; IDLE, CONDSTORE, QUOTA, ID extensions confirmed
- [github.com/chatmail/async-imap src/client.rs](https://github.com/chatmail/async-imap/blob/main/src/client.rs) — `select_condstore()` present; no `select_qresync()` confirmed via source inspection
- [docs.rs/rusqlite](https://docs.rs/rusqlite/latest/rusqlite/) — version 0.38.0; bundled SQLite 3.51.1
- [docs.rs/tokio-rusqlite](https://docs.rs/tokio-rusqlite/latest/tokio_rusqlite/) — single-writer-thread model; `Connection::call` API
- [docs.rs/libdav](https://docs.rs/libdav/latest/libdav/) — version 0.10.2 (2026-02-10); CalDAV + CardDAV
- [docs.rs/lettre](https://docs.rs/lettre/latest/lettre/) — version 0.11.19; tokio1, rustls-tls, builder features
- [docs.rs/ammonia](https://docs.rs/ammonia/latest/ammonia/) — version 4.1.2; RUSTSEC-2025-0071 fix
- [docs.rs/mail-parser](https://docs.rs/mail-parser/latest/mail_parser/) — version 0.11.2; zero-copy; 41 charsets
- [docs.rs/calcard](https://docs.rs/calcard/latest/calcard/) — version 0.3.2; iCalendar + vCard; Stalwart Labs
- [docs.rs/oauth2](https://docs.rs/oauth2/latest/oauth2/) — version 5.0.0; reqwest backend; PKCE; refresh flow
- [RFC 7162: IMAP CONDSTORE + QRESYNC](https://datatracker.ietf.org/doc/html/rfc7162) — ENABLE-before-SELECT requirement; Gmail CONDSTORE-only
- [RFC 2177: IMAP IDLE](https://datatracker.ietf.org/doc/html/rfc2177) — 29-minute re-issue requirement
- [RFC 4549: Disconnected IMAP Clients](https://datatracker.ietf.org/doc/html/rfc4549) — UIDVALIDITY handling
- [RFC 4791 §5.3.4: CalDAV ETag after PUT](https://www.rfc-editor.org/rfc/rfc4791) — server may omit ETag after mutation
- [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown) + [Tokio channels](https://tokio.rs/tokio/tutorial/channels) — official patterns used in architecture
- [tokio issue #7174](https://github.com/tokio-rs/tokio/issues/7174) — async stdout not flushed at process exit
- [rust-lang/rust issue #60673](https://github.com/rust-lang/rust/issues/60673) — stdout block buffering when not TTY
- [rust-lang/rust issue #45572](https://github.com/rust-lang/rust/issues/45572) — pipe buffer fill deadlock
- [rusqlite issue #697](https://github.com/rusqlite/rusqlite/issues/697) — transactions not safe across await points

### Secondary (MEDIUM confidence)

- [deltachat-core-rust issue #5093](https://github.com/deltachat/deltachat-core-rust/issues/5093) — IDLE CLOSE-WAIT confirmed in production Rust email client after ~30 minutes
- [Gmail IMAP Extensions](https://developers.google.com/workspace/gmail/imap/imap-extensions) — X-GM-LABELS, X-GM-MSGID, X-GM-THRID, CONDSTORE without QRESYNC
- [sabre/dav: Building a CalDAV client](https://sabre.io/dav/building-a-caldav-client/) — ETag-after-PUT pattern; sync-token fallback
- [Google CardDAV API](https://developers.google.com/people/carddav) — Google People API for Gmail contacts
- [SQLite concurrent writes and "database is locked"](https://tenthousandmeters.com/blog/sqlite-concurrent-writes-and-database-is-locked-errors/) — WAL mode, busy_timeout, checkpoint starvation

---
*Research completed: 2026-03-02*
*Ready for roadmap: yes*
