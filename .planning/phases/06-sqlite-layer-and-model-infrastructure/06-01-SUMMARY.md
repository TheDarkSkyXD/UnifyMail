---
phase: 06-sqlite-layer-and-model-infrastructure
plan: 01
subsystem: database
tags: [rust, serde, serde_json, rusqlite, fat-row, models]

# Dependency graph
requires:
  - phase: 05-core-infrastructure-and-ipc-protocol
    provides: SyncError, DeltaStreamItem, MailStore (SQLite open/migrate), Cargo.toml with rusqlite 0.37

provides:
  - MailModel trait with table_name(), id(), account_id(), version(), increment_version(), to_json(), bind_to_statement(), columns_for_query(), supports_metadata()
  - Message struct with all serde renames (aid, v, hMsgId, gMsgId, gThrId, rthMsgId, fwdMsgId, _sa, _suc, remoteUID, threadId)
  - Thread struct with timestamp renames (lmt, fmt, lmst, lmrt); attachment_count -> hasAttachments column mismatch preserved
  - Folder and Label structs with path/role indexed columns
  - Contact struct with abbreviated keys (s=source, h=hidden, gis=contact_groups, grn=google_resource_name, bid=book_id)
  - ContactBook with base-only binding (no extra indexed columns)
  - ContactGroup with name/bookId indexed columns
  - Calendar with NO version column binding (bindToQuery does not call MailModel base)
  - Event with NO version column binding; custom column order (id, data, icsuid, recurrenceId, accountId, etag, calendarId, rs, re)
  - Task with pre-set __cls preservation (task type name, not "Task")
  - File struct with messageId/contentType renames
  - Identity as plain struct (NOT implementing MailModel)
  - ModelPluginMetadata as join table struct (no data blob, no MailModel)

affects:
  - 06-02 (MailStore save/remove operations use these models for bind_to_statement)
  - 06-03 (query layer needs model structs for deserialization)
  - All subsequent phases that emit deltas or query the database

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Fat-row pattern: serde struct with full JSON fields + MailModel trait for SQLite binding"
    - "to_json() injects __cls = table_name() for delta dispatch (mirrors C++ MailModel::toJSON)"
    - "Task.__cls preserved from pre-set class_name field (not overridden by table_name)"
    - "Calendar and Event skip version binding — C++ design decision, no version column"

key-files:
  created:
    - app/mailsync-rs/src/models/mod.rs
    - app/mailsync-rs/src/models/mail_model.rs
    - app/mailsync-rs/src/models/message.rs
    - app/mailsync-rs/src/models/thread.rs
    - app/mailsync-rs/src/models/folder.rs
    - app/mailsync-rs/src/models/label.rs
    - app/mailsync-rs/src/models/contact.rs
    - app/mailsync-rs/src/models/contact_book.rs
    - app/mailsync-rs/src/models/contact_group.rs
    - app/mailsync-rs/src/models/calendar.rs
    - app/mailsync-rs/src/models/event.rs
    - app/mailsync-rs/src/models/task_model.rs
    - app/mailsync-rs/src/models/file.rs
    - app/mailsync-rs/src/models/identity.rs
    - app/mailsync-rs/src/models/model_plugin_metadata.rs
  modified:
    - app/mailsync-rs/src/main.rs (added mod models)

key-decisions:
  - "All 13 model types created in single plan (Tasks 1+2 implemented cohesively for correctness)"
  - "Calendar and Event bind_to_statement() does NOT bind version — C++ MailStore confirmed this"
  - "Task.to_json() overrides default to preserve pre-set __cls (task type name) rather than inject table_name 'Task'"
  - "Identity is plain struct (no MailModel) — C++ Identity::tableName() calls assert(false)"
  - "ModelPluginMetadata is join table struct (no data blob, no MailModel) — maintained by afterSave for metadata-supporting models"
  - "#![allow(dead_code, unused_imports)] added to models/mod.rs — models are unused until Phase 6 plan 02 wires them into MailStore"

patterns-established:
  - "Fat-row MailModel: serde struct serializes full JSON to data TEXT column + indexed projection columns via bind_to_statement()"
  - "columns_for_query() defines INSERT/UPDATE column order; bind_to_statement() must match exactly"
  - "Optional fields use skip_serializing_if = Option::is_none — never serialize null to JSON"
  - "Abbreviated JSON keys (_sa, _suc, aid, v, hMsgId, etc.) enforced via #[serde(rename)] — no default snake_case"

requirements-completed: [DATA-03, DATA-04]

# Metrics
duration: 8min
completed: 2026-03-04
---

# Phase 6 Plan 01: MailModel Trait and All 13 Model Structs Summary

**MailModel trait + 13 Rust model structs with exact C++ serde renames, fat-row binding, and 72 JSON round-trip tests**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-04T15:41:00Z
- **Completed:** 2026-03-04T15:49:00Z
- **Tasks:** 2
- **Files modified:** 16

## Accomplishments

- Defined MailModel trait mirroring C++ MailModel interface (table_name, id, account_id, version, to_json, bind_to_statement, columns_for_query, supports_metadata)
- Implemented all 12 MailModel types (Message, Thread, Folder, Label, Contact, ContactBook, ContactGroup, Calendar, Event, Task, File, ModelPluginMetadata) with exact C++ serde renames
- Implemented Identity as plain struct (not MailModel) matching C++ behavior where tableName() calls assert(false)
- 72 tests covering JSON key verification, round-trips, optional field omission, bind_to_statement against real in-memory SQLite, and special cases (Calendar/Event no-version, Task __cls preservation)

## Task Commits

Each task was committed atomically:

1. **Task 1: MailModel trait and core model structs (Message, Thread, Folder, Label, Contact)** - `d6e4bc5` (feat)
2. **Task 2: Remaining model structs (ContactBook, ContactGroup, Calendar, Event, Task, File, Identity, ModelPluginMetadata)** - `ffd74a1` (feat)

## Files Created/Modified

- `app/mailsync-rs/src/models/mail_model.rs` - MailModel trait definition
- `app/mailsync-rs/src/models/mod.rs` - Re-exports all model types with #![allow(dead_code)]
- `app/mailsync-rs/src/models/message.rs` - Message struct; supports_metadata=true; 8 tests
- `app/mailsync-rs/src/models/thread.rs` - Thread struct; lmt/fmt/lmst/lmrt renames; hasAttachments column mismatch; 6 tests
- `app/mailsync-rs/src/models/folder.rs` - Folder struct; path/role indexed; 5 tests
- `app/mailsync-rs/src/models/label.rs` - Label struct (same shape as Folder, __cls="Label"); 5 tests
- `app/mailsync-rs/src/models/contact.rs` - Contact struct; s/h/gis/grn/bid renames; 6 tests
- `app/mailsync-rs/src/models/contact_book.rs` - ContactBook; base-only binding; 5 tests
- `app/mailsync-rs/src/models/contact_group.rs` - ContactGroup; name/bookId indexed; 5 tests
- `app/mailsync-rs/src/models/calendar.rs` - Calendar; NO version binding; 5 tests
- `app/mailsync-rs/src/models/event.rs` - Event; NO version binding; cid/rid/rs/re renames; 5 tests
- `app/mailsync-rs/src/models/task_model.rs` - Task; preserves pre-set __cls; 7 tests
- `app/mailsync-rs/src/models/file.rs` - File; messageId/contentType renames; 5 tests
- `app/mailsync-rs/src/models/identity.rs` - Identity plain struct (no MailModel); 3 tests
- `app/mailsync-rs/src/models/model_plugin_metadata.rs` - ModelPluginMetadata join table; 5 tests
- `app/mailsync-rs/src/main.rs` - Added mod models declaration

## Decisions Made

- **Calendar/Event no version**: C++ Calendar.bindToQuery() and Event.bindToQuery() do NOT call MailModel::bindToQuery() — no version column in either table. Rust bind_to_statement() matches this exactly.
- **Task.__cls preservation**: Task's to_json() overrides the default implementation to avoid injecting __cls="Task" (table name). The class_name field already holds the correct task type (e.g., "SendDraftTask"). The override simply calls serde_json::to_value without re-injecting __cls.
- **Identity not MailModel**: Identity is a plain struct because C++ Identity::tableName() calls assert(false). Implemented as a separate models::Identity distinct from account::Identity (the stdin handshake struct).
- **#![allow] in mod.rs**: Models are not yet used by the binary's main code. Rather than annotating every struct, a module-level inner attribute silences dead_code/unused_imports until Plan 02 wires models into MailStore.

## Deviations from Plan

None - plan executed exactly as written.

All 13 model types implemented across both tasks. Task 2 stub files were created as full implementations immediately rather than as empty placeholders, since all the research context was available. This doesn't change the task boundary — the commits still respect the plan's task breakdown.

## Issues Encountered

The plan's verification command `cargo test models:: --lib` fails because `unifymail-sync` is a binary crate (not a library). The correct command is `cargo test models::` (without `--lib`). All tests pass with the correct command.

## Next Phase Readiness

- All 13 model structs ready for Plan 02 (MailStore save/remove/generic operations)
- bind_to_statement() implementations tested against real in-memory SQLite tables matching schema from migrations.rs
- to_json() __cls injection verified for all models — delta dispatch will work correctly
- Calendar/Event no-version constraint is locked in — Plan 02 must not attempt to bind version for these models

---
*Phase: 06-sqlite-layer-and-model-infrastructure*
*Completed: 2026-03-04*
