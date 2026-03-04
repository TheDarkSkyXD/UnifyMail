# Phase 6: SQLite Layer and Model Infrastructure - Context

**Gathered:** 2026-03-04
**Status:** Ready for planning

<domain>
## Phase Boundary

Complete MailStore with all 13 data model types (Message, Thread, Folder, Label, Contact, ContactBook, ContactGroup, Calendar, Event, Task, File, Identity, ModelPluginMetadata) persisting and round-tripping through SQLite correctly. WAL mode with tokio-rusqlite single-writer access, separate reader connection for concurrent queries, and delta emission wired to store writes with 500ms coalescing. Schema migrations already exist from Phase 5 — this phase builds the model layer and CRUD API on top.

</domain>

<decisions>
## Implementation Decisions

### Model fidelity strategy
- Fully typed Rust structs for all 13 models — every field has a concrete Rust type (String, i64, bool, Option<T>, Vec<T>)
- Serde derives handle JSON serialization/deserialization with explicit `#[serde(rename = "...")]` for C++ JSON key compatibility (e.g., `aid`, `hMsgId`, `gThrId`, `lmt`)
- Optional/nullable fields use `Option<T>` with `#[serde(skip_serializing_if = "Option::is_none")]` — absent fields omitted from JSON, matching C++ toJSON behavior
- The `__cls` field is NOT a struct member — the MailModel trait's `to_json()` method dynamically injects `"__cls": "ModelName"` into the serialized serde_json::Value before emission
- BLOB vs TEXT column types kept exactly as C++ schema defines — no normalization. migrations.rs from Phase 5 is the source of truth

### MailStore read API design
- Generic trait-based methods: `find<T: MailModel>(id)`, `find_all<T: MailModel>(where_clause, params)`, `count<T: MailModel>(where_clause, params)`
- One implementation works for all 13 models via the MailModel trait (table_name, from_row, bind_to_statement)
- WHERE clauses are raw SQL fragments with rusqlite parameter binding — e.g., `store.find_all::<Message>("accountId = ?1 AND unread = 1", params![acct_id])`
- Separate read-only tokio-rusqlite Connection for queries — WAL mode allows concurrent readers while writer is active. MailStore holds both `writer: Connection` and `reader: Connection`
- FTS5 search query methods deferred until the consuming phase needs them (Phase 7+ for thread search, Phase 9 for event/contact search). Phase 6 creates FTS5 tables (already in migrations) and populates indexes during save, but no search API yet

### Transaction and delta wiring
- MailStoreTransaction accumulates deltas in a `Vec<DeltaStreamItem>` during saves — deltas emit only on commit(), discarded on rollback
- Single-model saves outside explicit transactions auto-wrap: `store.save(model)` performs SQL insert/update, then immediately sends delta to the channel
- ProcessState and control deltas (ProcessAccountSecretsUpdated) bypass the 500ms coalescing window — sent directly to stdout writer for immediate delivery. Data model deltas (Message, Thread, etc.) go through normal coalescing
- Delta channel sender (`mpsc::Sender<DeltaStreamItem>`) injected via MailStore constructor — `MailStore::new(writer_conn, reader_conn, delta_tx)`. Keeps MailStore testable with mock receivers

### Testing and validation
- All 13 model types get full round-trip tests: create struct, save to DB, read back, assert field equality
- Snapshot tests against C++ JSON output: capture expected JSON fixtures per model type, Rust tests serialize and assert exact match (catches serde rename mismatches)
- Schema validation: test opens fresh DB, runs migrations, queries sqlite_master, compares table/index definitions against expected values from migrations.rs
- End-to-end pipeline test: save model → capture delta from channel → verify delta wire format matches Electron consumer expectations (type, modelClass, modelJSONs field names and shapes)
- Diff against C++ SQL strings (no C++ binary needed in CI) — migrations.rs already contains the verbatim SQL

### Claude's Discretion
- Internal module organization within `store/` and `models/`
- Exact MailModel trait method signatures and helper utilities
- How `bind_to_statement` maps struct fields to SQL columns for each model
- Performance optimizations for bulk saves (batch INSERT)
- Whether to use `prepare_cached` vs `prepare` for repeated queries
- FTS5 index population strategy during save (trigger vs explicit INSERT)
- Error type design for store operations

</decisions>

<specifics>
## Specific Ideas

- The 06-RESEARCH.md has complete field mappings for all 13 models including serde rename rules (C++ JSON key -> Rust field name)
- Phase 5 already built the delta pipeline: DeltaStreamItem with coalescing, DeltaStream sender wrapper, delta_flush_task with 500ms window — Phase 6 wires store saves into this existing pipeline
- Phase 5 MailStore has `open()`, `migrate()`, `reset_for_account()` — Phase 6 extends with `save()`, `remove()`, `find()`, `find_all()`, `count()`, `begin_transaction()`
- The "fat row" pattern from C++ must be preserved exactly: `data` column contains full JSON blob, indexed columns are projections for SQL queries
- C++ MailStore uses `prepare_cached` for repeated queries (MailStore.cpp) — Rust should follow for performance

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailsync-rs/src/store/mail_store.rs` (Phase 5): MailStore with open(), migrate(), reset_for_account() — extend with CRUD methods
- `app/mailsync-rs/src/store/migrations.rs` (Phase 5): V1-V9 SQL constants copied from C++ constants.h — schema is ready
- `app/mailsync-rs/src/delta/item.rs` (Phase 5): DeltaStreamItem with coalescing, upsert_model_json, concatenate — wire to store saves
- `app/mailsync-rs/src/delta/stream.rs` (Phase 5): DeltaStream Arc wrapper with emit() method — MailStore uses this to send deltas
- `app/mailsync-rs/src/delta/flush.rs` (Phase 5): delta_flush_task with 500ms coalescing — already running in sync mode
- TypeScript models at `app/frontend/flux/models/*.ts`: Reference for field names, types, and JSON structure

### Established Patterns
- Fat row: `data BLOB/TEXT` + indexed projection columns — all 13 models follow this
- serde rename: C++ uses short JSON keys (`aid`, `hMsgId`, `v`) that must be preserved via `#[serde(rename)]`
- tokio-rusqlite `call()`: All DB access through closures on the background thread — synchronous rusqlite inside
- Delta emission: DeltaStreamItem → mpsc channel → coalescing buffer → stdout flush task

### Integration Points
- `app/mailsync-rs/src/modes/sync.rs`: Will call MailStore CRUD methods during IMAP sync (Phase 7)
- `app/mailsync-rs/src/stdin_loop.rs`: Routes `queue-task` commands that will write to MailStore (Phase 8)
- `app/frontend/flux/stores/database-change-record.ts`: Wraps delta messages — delta JSON must match this parser
- `app/frontend/flux/models/*.ts`: TypeScript models deserialize from the `data` JSON blob — field names must match exactly

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 06-sqlite-layer-and-model-infrastructure*
*Context gathered: 2026-03-04*
