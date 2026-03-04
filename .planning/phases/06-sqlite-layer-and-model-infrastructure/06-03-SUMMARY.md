---
phase: 06-sqlite-layer-and-model-infrastructure
plan: 03
subsystem: database
tags: [rust, sqlite, fts5, rusqlite, tokio-rusqlite, lifecycle-hooks]

requires:
  - phase: 06-02
    provides: MailStore CRUD, WAL reader connection, MailStoreTransaction with delta accumulation
provides:
  - after_save/after_remove lifecycle hooks for all models (Message, Thread, Folder, Label, Contact, ContactGroup, Event)
  - ModelPluginMetadata join table maintained for Message and Thread on save/remove
  - ThreadCategory join table populated on Thread save/remove
  - ThreadCounts row initialized on Folder and Label save v1, deleted on remove
  - ContactSearch FTS5 index maintained on Contact save/remove
  - EventSearch FTS5 index populated when search fields are set on Event save/remove
  - ContactContactGroup join table cleaned on ContactGroup remove
  - globalLabelsVersion counter incremented on Label save (via labels_version Arc<AtomicU64>)
  - End-to-end pipeline tests: save/remove -> delta channel -> wire format verified
  - Schema validation: all 23 tables, 6 indexes, FTS5 virtual tables, user_version=9
  - Full round-trip tests for all 11 DB-stored model types
affects: [07-imap-sync, 09-caldav-carddav, search, thread-list]

tech-stack:
  added: []
  patterns:
    - "lifecycle hooks (after_save/after_remove) run inside writer.call() closure for atomicity"
    - "supports_metadata() controls ModelPluginMetadata maintenance in MailStore (not model)"
    - "increments_labels_version() signals MailStore to bump Arc<AtomicU64> counter after save"
    - "Event search fields (#[serde(skip)]) are transient — set by ICS parsing, not persisted in data blob"
    - "Thread::after_save() issues DELETE+INSERT for ThreadCategory; ThreadCounts diff deferred to Phase 7"
    - "Contact::after_save() INSERT at v1, UPDATE only when source != 'mail'"

key-files:
  created:
    - ".planning/phases/06-sqlite-layer-and-model-infrastructure/06-03-SUMMARY.md"
  modified:
    - "app/mailsync-rs/src/models/message.rs"
    - "app/mailsync-rs/src/models/thread.rs"
    - "app/mailsync-rs/src/models/folder.rs"
    - "app/mailsync-rs/src/models/label.rs"
    - "app/mailsync-rs/src/models/contact.rs"
    - "app/mailsync-rs/src/models/contact_group.rs"
    - "app/mailsync-rs/src/models/event.rs"
    - "app/mailsync-rs/src/store/mail_store.rs"

key-decisions:
  - "Thread::after_save() implements full ThreadCategory maintenance (DELETE + INSERT per category) but defers ThreadCounts diff algorithm to Phase 7 — Phase 6 scope is the write path, Phase 7 needs the full snapshot-diff cycle"
  - "Event search fields use #[serde(skip)] transient pattern — not stored in data blob, populated by ICS parsing in Phase 9; EventSearch FTS5 is gated on search_title non-empty"
  - "Contact::after_save() does not update ContactSearch for source='mail' at version > 1 — mail-sourced contacts are ephemeral, only addressbook contacts (carddav, gpeople) get FTS5 updates"
  - "globalLabelsVersion incremented in MailStore (not in Label::after_save) via Arc<AtomicU64> because after_save receives only &Connection — the counter is a store-level concern"

requirements-completed: [DATA-02, DATA-04]

duration: 27min
completed: 2026-03-04
---

# Phase 6 Plan 3: Lifecycle Hooks and E2E Pipeline Tests Summary

**afterSave/afterRemove lifecycle hooks for all 7 models with side effects, wired into MailStore CRUD, with end-to-end delta pipeline verification and schema validation tests covering all 23 tables**

## Performance

- **Duration:** 27 min
- **Started:** 2026-03-04T10:32:00Z
- **Completed:** 2026-03-04T10:59:00Z
- **Tasks:** 2
- **Files modified:** 8

## Accomplishments

- Implemented afterSave/afterRemove hooks for Message (metadata join table), Thread (ThreadCategory + ThreadSearch), Folder (ThreadCounts), Label (ThreadCounts + labels version counter), Contact (ContactSearch FTS5), ContactGroup (ContactContactGroup cleanup), and Event (EventSearch FTS5)
- Wired all hooks into MailStore.save() and MailStore.remove() inside the writer.call() closure for atomicity — hook failure rolls back the entire save/remove operation
- Added 16 lifecycle hook tests covering all secondary table maintenance scenarios
- Added 3 end-to-end delta pipeline tests verifying the save→delta channel→wire format path matches TypeScript DatabaseChangeRecord expectations (camelCase field names, __cls injection)
- Added 4 schema validation tests (tables, indexes, FTS5 tables, user_version=9)
- Added 11 full round-trip tests (save + find for all DB-stored model types)

## Task Commits

Each task was committed atomically:

1. **Task 1: Lifecycle hooks — metadata, FTS5, join tables, and ThreadCounts maintenance** - `6ed335c` (feat)
2. **Task 2: End-to-end pipeline tests and schema validation** - `d1a9827` (feat)

**Plan metadata:** (this summary commit)

_Note: Both tasks used TDD pattern — tests written alongside implementation_

## Files Created/Modified

- `app/mailsync-rs/src/models/message.rs` - No after_save override needed (metadata handled by MailStore); supports_metadata() returns true
- `app/mailsync-rs/src/models/thread.rs` - after_save() maintains ThreadCategory (DELETE+INSERT per folder/label), optionally updates ThreadSearch; after_remove() clears ThreadCategory
- `app/mailsync-rs/src/models/folder.rs` - after_save() inserts ThreadCounts row at version 1; after_remove() deletes ThreadCounts row
- `app/mailsync-rs/src/models/label.rs` - same as Folder for ThreadCounts; increments_labels_version() returns true so MailStore bumps Arc<AtomicU64> counter
- `app/mailsync-rs/src/models/contact.rs` - after_save() INSERT into ContactSearch at v1, UPDATE for non-mail sources at v>1; after_remove() deletes ContactSearch row
- `app/mailsync-rs/src/models/contact_group.rs` - after_remove() deletes ContactContactGroup join rows where value = group.id
- `app/mailsync-rs/src/models/event.rs` - after_save() INSERT OR REPLACE into EventSearch when search_title is non-empty (transient field); after_remove() deletes EventSearch row
- `app/mailsync-rs/src/store/mail_store.rs` - test-only schema query helpers (query_table_names, query_index_names, query_user_version), setup_test_store_no_delta(), and 18 new tests (e2e pipeline, schema validation, full round-trips)

## Decisions Made

- Thread::after_save() implements ThreadCategory maintenance fully but defers ThreadCounts diff to Phase 7. The C++ implementation computes unread/total diffs via applyMessageAttributeChanges (snapshot comparison), which requires the full message sync cycle. Phase 6 scope covers the write path; Phase 7 implements the diff algorithm when IMAP sync actually delivers changing message state.
- Event search fields use `#[serde(skip)]` to keep them out of the data blob and delta JSON. They will be populated by ICS parsing in Phase 9; the FTS5 insert/update path is already wired and tested.
- Contact FTS5 update is skipped for source="mail" contacts at version > 1. Mail-sourced contacts are ephemeral references built from message headers and not curated in an addressbook; only carddav/gpeople sources represent intentional addressbook entries that users expect to search.

## Deviations from Plan

None - plan executed exactly as written. All lifecycle hooks, tests, and schema validation implemented as specified.

## Issues Encountered

None - implementation matched the plan specification. All 145 unit tests and 9 integration tests pass.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- MailStore is production-ready: CRUD, transactions, delta emission, lifecycle hooks, and schema all verified
- Phase 7 (IMAP sync) can use MailStore directly — all secondary tables are maintained on save
- Phase 7 needs to implement ThreadCounts diff updates via applyMessageAttributeChanges snapshot algorithm (deferred from this plan per plan spec)
- Phase 9 (CalDAV/CardDAV) can populate Event search fields before calling store.save() and EventSearch FTS5 will be indexed automatically

---
*Phase: 06-sqlite-layer-and-model-infrastructure*
*Completed: 2026-03-04*
