---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Rewrite mailsync Engine in Rust
status: executing
stopped_at: Completed 07-05-PLAN.md
last_updated: "2026-03-05T00:07:15.676Z"
last_activity: 2026-03-04 — Completed 07-03-PLAN.md (ImapSession with TLS/STARTTLS connect, XOAUTH2/password auth, two-pass role detection, Gmail 6-folder whitelist)
progress:
  total_phases: 6
  completed_phases: 3
  total_plans: 12
  completed_plans: 12
  percent: 100
---

---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Rewrite mailsync Engine in Rust
status: executing
stopped_at: Completed 07-03-PLAN.md — ImapSession TLS/STARTTLS connect, password/XOAUTH2 auth, two-pass RFC 6154 role detection, Gmail 6-folder whitelist, 22 unit tests
last_updated: "2026-03-04T23:55:00.000Z"
last_activity: 2026-03-04 — Completed v1.0 milestone
progress:
  [██████████] 100%
  completed_phases: 3
  total_plans: 12
  completed_plans: 12
  percent: 55
---

---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Rewrite mailsync Engine in Rust
status: executing
stopped_at: Completed 06-03-PLAN.md — lifecycle hooks, pipeline tests, schema validation, full round-trips
last_updated: "2026-03-04T17:22:35.664Z"
last_activity: 2026-03-04 — Completed v1.0 milestone
progress:
  [██████░░░░] 55%
  completed_phases: 2
  total_plans: 5
  completed_plans: 5
---

---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Rewrite mailsync Engine in Rust
status: executing
stopped_at: Completed 06-03-PLAN.md — lifecycle hooks, pipeline tests, schema validation, full round-trips
last_updated: "2026-03-04T18:00:00.000Z"
last_activity: 2026-03-04 — Executing phase 06
progress:
  total_phases: 6
  completed_phases: 2
  total_plans: 8
  completed_plans: 8
---

---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Rewrite mailsync Engine in Rust
status: planning
stopped_at: Completed 05-02-PLAN.md — delta pipeline, stdin loop, sync mode skeleton, IPC contract tests
last_updated: "2026-03-04T14:37:50.221Z"
last_activity: 2026-03-04 — Completed v1.0 milestone
progress:
  total_phases: 6
  completed_phases: 1
  total_plans: 2
  completed_plans: 2
---

---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Rewrite mailsync Engine in Rust
status: planning
stopped_at: v1.0 milestone completed
last_updated: "2026-03-04"
last_activity: "2026-03-04 — Completed v1.0 milestone, archived to milestones/"
progress:
  total_phases: 6
  completed_phases: 0
  total_plans: 2
  completed_plans: 0
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-04)

**Core value:** Users can set up email accounts quickly and reliably through automatic provider detection and connection validation
**Current focus:** v2.0 planning — Rewrite mailsync Engine in Rust

## Current Position

Milestone: v2.0 — Rewrite mailsync Engine in Rust
Phase: 7 of 10 (IMAP Background Sync Worker) — Plan 05 of 07 complete
Status: Executing
Last activity: 2026-03-04 — Completed 07-05-PLAN.md (CONDSTORE incremental sync, UID-range fallback, UIDVALIDITY reset, folder priority sort, FolderSyncState with modseq-as-string, 42 tests)

## Completed Milestones

- v1.0 — Rewrite mailcore N-API in Rust (shipped 2026-03-04)
  - 6 phases, 10 plans, 20 tasks, 27/27 requirements
  - See: .planning/MILESTONES.md

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
v1.0 decisions archived with outcomes — see PROJECT.md.
- [Phase 05-01]: rusqlite pinned to 0.37 for tokio-rusqlite 0.7 compatibility; io-std added to workspace tokio features; ThreadListSortIndex moved to V8 migration (column doesn't exist in V1)
- [Phase 05-02]: Single shared BufReader/Lines for stdin: multiple BufReader instances cause OS pipe data loss; shared Lines iterator passed through handshake reads into stdin_loop
- [Phase 05-02]: process::exit(141) called from sync::run() after awaiting delta_flush_task completion, NOT from stdin_loop, ensuring all pending deltas flush before exit
- [Phase 06]: Calendar/Event bind_to_statement does NOT bind version (no version column in C++ table design)
- [Phase 06]: Task.to_json() overrides default to preserve pre-set __cls (task type name) rather than inject table_name 'Task'
- [Phase 06]: Identity is plain struct (no MailModel) — C++ Identity::tableName() calls assert(false)
- [Phase 06-02]: SqlParam enum for owned ToSql values — tokio-rusqlite closures must be Send + 'static, reference params cannot be captured
- [Phase 06-02]: MailStoreTransaction takes &MailStore for commit/rollback — transaction cannot own connection; store.execute_commit/rollback are pub(crate)
- [Phase 06-02]: busy_timeout corrected to 5000ms — DATA-01 spec matches C++ MailStore.cpp, Phase 5's 10s was conservative placeholder
- [Phase 06-03]: Thread::after_save() implements ThreadCategory maintenance but defers ThreadCounts diff algorithm to Phase 7 (requires full message snapshot-diff cycle from IMAP sync)
- [Phase 06-03]: Event search fields use #[serde(skip)] transient pattern — not stored in data blob, gated on search_title non-empty, populated by ICS parsing in Phase 9
- [Phase 07-01]: async-imap Error::Tls variant doesn't exist in 0.11; TLS errors surface via Io(IoError), mapped to SyncError::Connection
- [Phase 07-01]: reqwest uses rustls-native-certs (platform cert store) not rustls-tls — consistent with rustls-platform-verifier
- [Phase 07-03]: ImapPreAuth has only client field — capabilities deferred to authenticate(); async-imap Client (pre-auth) does not expose capabilities(), only Session (post-auth) does
- [Phase 07-03]: Concrete ImapTlsStream = TlsStream<TcpStream> for both SSL/TLS and STARTTLS — avoids Box<dyn AsyncReadWrite + Send> trait object complexity
- [Phase 07]: reqwest form feature needed explicitly for .form() method; emit_secrets_updated() uses DeltaStreamItem factory; Windows MSYS2 PATH must include C:\msys64\mingw64\bin for cargo test
- [Phase 07-04]: gmail_thread_id() takes &[AttributeValue] not &Fetch: async-imap 0.11 Fetch.response is private; X-GM-THRID extracted via free function from parsed attribute slice
- [Phase 07-04]: id_for_message RFC 2047 decode for subject/message-id only: C++ MailUtils::idForMessage() decodes subject via mailcore2 but treats addresses as raw bytes — our implementation matches this exactly
- [Phase 07]: CONDSTORE decision logic extracted as pure functions for unit testability — async IMAP wrappers deferred to Phase 8 integration
- [Phase 07]: highestmodseq serialized as JSON string via custom serialize_modseq/deserialize_modseq — prevents JavaScript Number precision loss above 2^53
- [Phase 07-06]: background_sync stub replaced — stub replaced with full function; stdin channels: try_send for WakeWorkers/NeedBodies (non-blocking); Account has no Clone — Arc::new(account) consumes owned value
- [Phase 07-imap-background-sync-worker]: async-imap::error::Error has no Tls variant — TLS errors surface as Io(IoError); mapped to SyncError::Connection rather than SslHandshakeFailed
- [Phase 07-imap-background-sync-worker]: reqwest uses rustls-native-certs feature (not rustls-tls) — platform certificate store, consistent with rustls-platform-verifier approach
- [Phase 07-07]: save_body() added to MailStore — MessageBody table has no MailModel impl; added as missing critical functionality for body persistence
- [Phase 07-07]: + Send added to uid_fetch() return type — dyn Stream trait object must be Send for tokio::spawn background_sync future
- [Phase 07-07]: Priority body_queue drain deferred to Phase 8 — message-ID-to-UID mapping requires find_all helper not yet available; background body prefetch via find_messages_needing_bodies() works fully
- [Phase 07-imap-background-sync-worker]: reqwest form feature needed explicitly for .form() method; emit_secrets_updated() uses DeltaStreamItem factory; Windows MSYS2 PATH must include C:\msys64\mingw64\bin for cargo test

### Pending Todos

None.

### Blockers/Concerns

- [Phase 8 research flag]: Validate lettre multipart MIME builder API coverage for inline images (CID references) and text/html + text/plain alternatives before Phase 8 coding begins
- [Phase 9 research flag]: CalDAV server compatibility matrix (ETag after PUT, sync-token expiry, Exchange Online, iCloud, Nextcloud) — targeted research pass recommended before implementing sync-collection state machine
- [Phase 9 watch]: Verify Google People API v1 endpoint and OAuth2 scope requirements are current before Phase 9 — Google has been migrating People API surfaces

## Session Continuity

Last session: 2026-03-05T00:07:09.410Z
Stopped at: Completed 07-05-PLAN.md
Resume file: None

---
*Last updated: 2026-03-04 after v1.0 milestone completion*
