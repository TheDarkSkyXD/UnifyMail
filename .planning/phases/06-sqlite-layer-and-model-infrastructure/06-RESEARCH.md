# Phase 6: SQLite Layer and Model Infrastructure - Research

**Researched:** 2026-03-02 (updated 2026-03-03 with deep dive rounds 1, 2, 3, and 4)
**Domain:** Rust async SQLite (tokio-rusqlite), data model serialization, FTS5 schema migration, delta coalescing, Electron delta parsing, MailStore read API, error handling, need-bodies flow, thread maintenance algorithm, Query builder, toJSON/toJSONDispatch
**Confidence:** HIGH (standard stack verified via official docs/crates.io; schema verified against C++ source; all open questions resolved from source code; thread maintenance algorithm traced line-by-line)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DATA-01 | SQLite database with WAL mode and `busy_timeout=5000` via tokio-rusqlite | tokio-rusqlite 0.7.0 `call()` closure + rusqlite `busy_timeout()` + `pragma_update("journal_mode", "WAL")` |
| DATA-02 | Delta emission with persist/unpersist types, 500ms coalescing window, transaction batching | `tokio::sync::mpsc` + `tokio::time::sleep` + HashMap coalescing per model class; mirrors C++ `DeltaStream` buffering; 500ms confirmed from SyncWorker.cpp |
| DATA-03 | All 13 data models implemented: Message, Thread, Folder, Label, Contact, ContactBook, ContactGroup, Calendar, Event, Task, File, Identity, ModelPluginMetadata | "Fat row" pattern: `data TEXT` JSON column + indexed columns; serde_json; full field mapping documented in Deep Dive section |
| DATA-04 | Schema migration matching C++ baseline (all tables, indexes, FTS5 for ThreadSearch/EventSearch/ContactSearch) | rusqlite_migration 2.3.x with `M::up()` for V1–V9 SQL from constants.h; exact SQL documented in Deep Dive section; V5 no-op confirmed required |
| DATA-05 | Single-writer pattern via tokio-rusqlite prevents blocking on async threads | Single `tokio_rusqlite::Connection` in a Mutex or Arc; all writes via `.call()` closure sent to dedicated background thread |
</phase_requirements>

---

## Summary

Phase 6 builds the complete MailStore Rust equivalent: a tokio-async SQLite layer that matches the C++ baseline schema exactly, serializes and deserializes all 13 data model types using a "fat row" pattern, emits deltas to the stdout writer task via a coalescing 500ms window, and enforces the single-writer constraint through tokio-rusqlite's background thread architecture.

The C++ `MailStore` uses a "fat row" design: every model table has a `data TEXT` column containing the full JSON blob, plus a small number of indexed projection columns (e.g., `unread`, `starred`, `threadId`) for query performance. The Rust implementation must replicate this exactly — the Electron TypeScript side reads both the `data` JSON and those indexed columns. The schema is fully defined in `constants.h` across V1–V9 migration blocks; all SQL must be reproduced verbatim.

The delta system mirrors the C++ `DeltaStream` singleton: saves and removes accumulate `DeltaStreamItem` values keyed by model class, same-object saves merge into a single entry (last-write wins with key-merge), and the buffer is flushed after a 500ms coalescing window. In Rust, this is a tokio task that receives `DeltaStreamItem` via mpsc channel, batches by model class in a `HashMap`, and flushes after a `tokio::time::sleep(500ms)` timer resets on each new arrival.

**Primary recommendation:** Use `tokio-rusqlite 0.7.0` (with rusqlite 0.37.0 dependency) plus `rusqlite_migration 2.3.x` for schema management. CRITICAL: do not use rusqlite_migration 2.4.x — it requires rusqlite ^0.38.0 which conflicts with tokio-rusqlite 0.7.0. Store all model JSON in a `data TEXT` column serialized with `serde_json`. Implement the delta coalescing task using `tokio::sync::mpsc` + `tokio::time::sleep` — do NOT use an external debounce crate (the per-class key-merge logic requires a bespoke HashMap approach).

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio-rusqlite | 0.7.0 | Async SQLite access with single-writer background thread | Mandated by STATE.md: prevents tokio thread starvation from blocking rusqlite calls |
| rusqlite | 0.37.0 | Underlying SQLite bindings (re-exported by tokio-rusqlite) | tokio-rusqlite 0.7.0 pins rusqlite ^0.37.0; use feature `bundled,serde_json` |
| serde | 1.x | Serialize/Deserialize derives for model structs | Universal Rust serialization framework |
| serde_json | 1.x | JSON serialization for the `data TEXT` column and delta output | Needed for fat-row JSON storage and delta JSON emission |
| rusqlite_migration | 2.3.x | Schema migration with `user_version` PRAGMA tracking | Manages V1-V9 migration chain; avoids hand-rolling migration state. NOTE: must use 2.3.x not 2.4.x — see Deep Dive: Library Version Verification |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| tokio | 1.x | Async runtime, mpsc channels, time::sleep for coalescing | Always — binary already uses tokio runtime |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| tokio-rusqlite | sqlx with SQLite | sqlx adds compile-time query checking overhead, no single-writer guarantee at library level; tokio-rusqlite is the team decision |
| tokio-rusqlite | async-sqlite | Less widely used, no re-export of all rusqlite feature flags; tokio-rusqlite is the team decision |
| rusqlite_migration | Hand-rolled migration | rusqlite_migration uses PRAGMA user_version (zero overhead), handles M::up() SQL batches, matches the C++ V1-V9 pattern cleanly |
| tokio::mpsc + sleep | tokio-debouncer 0.3.x | tokio-debouncer cannot implement the per-class key-merge (upsert) logic the C++ DeltaStream uses; hand-roll required |

**Installation:**
```toml
[dependencies]
# tokio-rusqlite 0.7.0 pins rusqlite ^0.37.0
tokio-rusqlite = { version = "0.7", features = ["bundled", "serde_json"] }
# rusqlite_migration 2.3.x is the last version compatible with rusqlite 0.37.x
# DO NOT use 2.4.x -- it requires rusqlite ^0.38.0 which conflicts with tokio-rusqlite 0.7.0
rusqlite_migration = "2.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

**Note on `bundled` feature:** Enabling `bundled` on tokio-rusqlite (which proxies it to rusqlite) compiles SQLite from source with `-DSQLITE_ENABLE_FTS5`. This is required for the FTS5 virtual tables (ThreadSearch, EventSearch, ContactSearch). Without `bundled`, FTS5 availability depends on the system SQLite, which may be compiled without FTS5 on some Linux distributions.

---

## Architecture Patterns

### Recommended Module Structure

```
mailsync-rs/src/
├── store/
│   ├── mod.rs          # MailStore struct, open/migrate, save/remove public API
│   ├── schema.rs       # All V1-V9 SQL constants (from constants.h)
│   ├── delta.rs        # DeltaStreamItem, DeltaCoalescer, delta task
│   └── transaction.rs  # Transaction RAII wrapper (begin/commit/rollback)
├── models/
│   ├── mod.rs          # Re-exports all model types
│   ├── mail_model.rs   # MailModel trait (tableName, bindToQuery, afterSave)
│   ├── message.rs      # Message struct
│   ├── thread.rs       # Thread struct
│   ├── folder.rs       # Folder struct
│   ├── label.rs        # Label struct
│   ├── contact.rs      # Contact struct
│   ├── contact_book.rs # ContactBook struct
│   ├── contact_group.rs# ContactGroup struct
│   ├── calendar.rs     # Calendar struct
│   ├── event.rs        # Event struct
│   ├── task.rs         # Task struct
│   ├── file.rs         # File struct (metadata only, not MessageBody)
│   ├── identity.rs     # Identity struct
│   └── model_plugin_metadata.rs  # ModelPluginMetadata struct
```

### Pattern 1: Fat Row Model — data TEXT + Indexed Columns

**What:** Every model is stored as a `data TEXT` column containing the full JSON blob, plus a small set of indexed projection columns for SQL queries. This exactly mirrors the C++ `MailModel::bindToQuery` pattern.

**When to use:** All 13 model types. Never store model fields only in indexed columns — the TypeScript side deserializes from `data`.

**Example — Message table columns (from constants.h):**
```sql
CREATE TABLE IF NOT EXISTS Message (
    id VARCHAR(40) PRIMARY KEY,
    accountId VARCHAR(8),
    version INTEGER,
    data TEXT,
    headerMessageId VARCHAR(255),
    gMsgId VARCHAR(255),
    gThrId VARCHAR(255),
    subject VARCHAR(500),
    date DATETIME,
    draft TINYINT(1),
    unread TINYINT(1),
    starred TINYINT(1),
    remoteUID INTEGER,
    remoteXGMLabels TEXT,
    remoteFolderId VARCHAR(40),
    replyToHeaderMessageId VARCHAR(255),
    threadId VARCHAR(40)
);
```

**Rust model struct pattern:**
```rust
// Source: derived from C++ MailModel/Message pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    // JSON key: "id"
    pub id: String,
    // JSON key: "aid"
    #[serde(rename = "aid")]
    pub account_id: String,
    // JSON key: "v"
    #[serde(rename = "v")]
    pub version: i64,
    // ... all other fields with correct JSON key renames ...
    #[serde(rename = "hMsgId")]
    pub header_message_id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
    pub subject: String,
    pub date: i64,
    pub draft: bool,
    pub unread: bool,
    pub starred: bool,
    #[serde(rename = "remoteUID")]
    pub remote_uid: u32,
    pub labels: Vec<String>,       // stored as remoteXGMLabels in indexed col
    pub folder: serde_json::Value, // embedded folder JSON
    #[serde(rename = "remoteFolder")]
    pub remote_folder: serde_json::Value,
    // Optional fields use Option<T>
    #[serde(rename = "rthMsgId", skip_serializing_if = "Option::is_none")]
    pub reply_to_header_message_id: Option<String>,
}
```

**Critical serde rename rules (C++ JSON key -> Rust field):**
| C++ JSON Key | Rust Field |
|---|---|
| `id` | `id` |
| `aid` | `account_id` |
| `v` | `version` |
| `hMsgId` | `header_message_id` |
| `rthMsgId` | `reply_to_header_message_id` |
| `fwdMsgId` | `forwarded_header_message_id` |
| `gMsgId` | `g_msg_id` |
| `gThrId` | `g_thr_id` |
| `lmt` | `last_message_timestamp` (Thread) |
| `fmt` | `first_message_timestamp` (Thread) |
| `lmst` | `last_message_sent_timestamp` (Thread) |
| `lmrt` | `last_message_received_timestamp` (Thread) |
| `_sa` | `synced_at` (Message) |
| `_suc` | `sync_unsaved_changes` (Message) |
| `__cls` | `class_name` (added by toJSON) |

### Pattern 2: MailModel Trait

**What:** A Rust trait that all model structs implement, providing table name, column list for query binding, and lifecycle hooks.

```rust
// Source: derived from C++ MailModel interface
pub trait MailModel: Serialize + for<'de> Deserialize<'de> {
    fn table_name() -> &'static str;
    fn id(&self) -> &str;
    fn account_id(&self) -> &str;
    fn version(&self) -> i64;
    fn increment_version(&mut self);
    fn to_json(&self) -> serde_json::Value;
    fn to_json_dispatch(&self) -> serde_json::Value { self.to_json() }
    fn bind_to_statement(&self, stmt: &mut rusqlite::Statement<'_>) -> rusqlite::Result<()>;
    fn after_save(&mut self, store: &MailStore) -> Result<()> { Ok(()) }
    fn after_remove(&self, store: &MailStore) -> Result<()> { Ok(()) }
    fn supports_metadata() -> bool { false }
}
```

### Pattern 3: tokio-rusqlite Single-Writer Save

**What:** All writes go through a single `tokio_rusqlite::Connection` via `.call()` closures. The library serializes all calls through an internal mpsc channel to a dedicated background thread — no additional Mutex needed on the Connection itself (it is `Clone`).

```rust
// Source: tokio-rusqlite 0.7.0 docs
// Connection::call executes the closure on the single background thread
conn.call(move |db| {
    let mut stmt = db.prepare_cached(
        "INSERT INTO Message (id, data, accountId, version, ...) VALUES (?1, ?2, ?3, ?4, ...)"
    )?;
    stmt.execute(rusqlite::params![
        msg.id(),
        serde_json::to_string(&msg.to_json())
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?,
        msg.account_id(),
        msg.version(),
        // ... other indexed column values ...
    ])?;
    Ok(())
}).await?;
```

**Important:** `conn.call()` accepts `FnOnce(&mut rusqlite::Connection) -> Result<R, E>`. The closure receives a `&mut rusqlite::Connection`, NOT a `&tokio_rusqlite::Connection`. Inside the closure you use synchronous rusqlite APIs only.

### Pattern 4: Transaction Batching

**What:** Multiple saves within a transaction are batched. Delta emission happens on commit, not on each individual save. This mirrors C++ `MailStoreTransaction`.

```rust
// Pattern: accumulate deltas in a Vec during transaction, emit on commit
pub struct MailStoreTransaction {
    deltas: Vec<DeltaStreamItem>,
    committed: bool,
}

// In MailStore::save() — when inside a transaction, append to deltas
// In MailStore::commit_transaction() — send accumulated deltas to delta channel
```

### Pattern 5: Delta Coalescing Task

**What:** A dedicated tokio task receives `DeltaStreamItem` values from an mpsc channel and coalesces them. Items for the same model class and type are merged (upsert by id). The buffer is flushed 500ms after the last received item. This directly mirrors the C++ `DeltaStream::flushWithin()` + `queueDeltaForDelivery()` logic.

```rust
// Source: derived from C++ DeltaStream architecture
// DeltaStreamItem wire format matches: {"type":"persist","modelClass":"Message","modelJSONs":[{...}]}

#[derive(Debug, Clone)]
pub struct DeltaStreamItem {
    pub delta_type: String,       // "persist" or "unpersist"
    pub model_class: String,      // "Message", "Thread", etc.
    pub model_jsons: Vec<serde_json::Value>, // merged array, deduped by "id"
    id_indexes: HashMap<String, usize>,     // id -> index in model_jsons
}

impl DeltaStreamItem {
    // Upsert: if id exists, merge keys (last-write wins); else append
    pub fn upsert_model_json(&mut self, item: serde_json::Value) {
        let id = item["id"].as_str().unwrap_or("").to_string();
        if let Some(&idx) = self.id_indexes.get(&id) {
            // Merge: overwrite keys of existing entry
            if let Some(existing) = self.model_jsons.get_mut(idx) {
                if let (Some(existing_obj), Some(new_obj)) =
                    (existing.as_object_mut(), item.as_object())
                {
                    for (k, v) in new_obj {
                        existing_obj.insert(k.clone(), v.clone());
                    }
                }
            }
        } else {
            self.id_indexes.insert(id, self.model_jsons.len());
            self.model_jsons.push(item);
        }
    }
}

// Coalescing buffer: keyed by model_class, contains list of items per class
// (same class+type concatenates; different type creates separate entry)
type DeltaBuffer = HashMap<String, Vec<DeltaStreamItem>>;

// The coalescing task
async fn delta_coalesce_task(
    mut rx: mpsc::Receiver<DeltaStreamItem>,
    stdout_tx: mpsc::Sender<String>, // stdout writer task channel
    coalesce_ms: u64,  // 500ms for normal saves; 0 for immediate ProcessState deltas
) {
    let mut buffer: DeltaBuffer = HashMap::new();
    let mut flush_deadline: Option<tokio::time::Instant> = None;

    loop {
        let sleep_future = async {
            match flush_deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending().await,
            }
        };

        tokio::select! {
            item = rx.recv() => {
                match item {
                    None => {
                        flush_buffer(&buffer, &stdout_tx).await;
                        break;
                    }
                    Some(item) => {
                        let key = item.model_class.clone();
                        let entry = buffer.entry(key).or_default();
                        // Try to concatenate onto last item of same type
                        if entry.last_mut().map_or(false, |last| last.delta_type == item.delta_type) {
                            let last = entry.last_mut().unwrap();
                            for json in &item.model_jsons {
                                last.upsert_model_json(json.clone());
                            }
                        } else {
                            entry.push(item);
                        }
                        // Reset 500ms timer
                        flush_deadline = Some(
                            tokio::time::Instant::now() +
                            tokio::time::Duration::from_millis(coalesce_ms)
                        );
                    }
                }
            }
            _ = sleep_future => {
                flush_buffer(&buffer, &stdout_tx).await;
                buffer.clear();
                flush_deadline = None;
            }
        }
    }
}
```

### Pattern 6: Schema Migration with rusqlite_migration

**What:** Use `rusqlite_migration` 2.3.x with `M::up()` entries mirroring the C++ V1–V9 migration blocks. Migration runs once on database open via `to_latest()`. The `user_version` PRAGMA is managed automatically by the library.

```rust
// Source: rusqlite_migration 2.3.x docs + constants.h V1-V9 blocks
use rusqlite_migration::{Migrations, M};

fn build_migrations() -> Migrations<'static> {
    Migrations::new(vec![
        // V1: core schema
        M::up(include_str!("migrations/v1_setup.sql")),
        // V2: MessageUIDScanIndex
        M::up("CREATE INDEX IF NOT EXISTS MessageUIDScanIndex ON Message(accountId, remoteFolderId, remoteUID);"),
        // V3: MessageBody.fetchedAt column
        M::up("ALTER TABLE MessageBody ADD COLUMN fetchedAt DATETIME; UPDATE MessageBody SET fetchedAt = datetime('now');"),
        // V4: Task indexes
        M::up("DELETE FROM Task WHERE Task.status = 'complete' OR Task.status = 'cancelled'; CREATE INDEX IF NOT EXISTS TaskByStatus ON Task(accountId, status);"),
        // V5: NO-OP — C++ MailStore.migrate() has no "if (version < 5)" block.
        // This placeholder keeps user_version numbering in sync with the C++ CURRENT_VERSION=9.
        M::up(""),
        // V6: Event table rebuilt
        M::up("DROP TABLE IF EXISTS Event; CREATE TABLE IF NOT EXISTS Event (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), etag VARCHAR(40), calendarId VARCHAR(40), recurrenceStart INTEGER, recurrenceEnd INTEGER); CREATE INDEX IF NOT EXISTS EventETag ON Event(calendarId, etag);"),
        // V7: Event.icsuid
        M::up("ALTER TABLE Event ADD COLUMN icsuid VARCHAR(150); CREATE INDEX IF NOT EXISTS EventUID ON Event(accountId, icsuid);"),
        // V8: Contact extensions + ContactGroup + ContactBook
        M::up("DELETE FROM Contact WHERE refs = 0; ALTER TABLE Contact ADD COLUMN hidden TINYINT(1) DEFAULT 0; ALTER TABLE Contact ADD COLUMN source VARCHAR(10) DEFAULT 'mail'; ALTER TABLE Contact ADD COLUMN bookId VARCHAR(40); ALTER TABLE Contact ADD COLUMN etag VARCHAR(40); CREATE INDEX IF NOT EXISTS ContactBrowseIndex ON Contact(hidden,refs,accountId); CREATE TABLE ContactGroup (id varchar(40), accountId varchar(40), bookId varchar(40), data BLOB, version INTEGER, name varchar(300), PRIMARY KEY (id)); CREATE TABLE ContactContactGroup (id varchar(40), value varchar(40), PRIMARY KEY (id, value)); CREATE TABLE ContactBook (id varchar(40), accountId varchar(40), data BLOB, version INTEGER, PRIMARY KEY (id));"),
        // V9: Event.recurrenceId
        M::up("ALTER TABLE Event ADD COLUMN recurrenceId VARCHAR(50) DEFAULT ''; CREATE INDEX IF NOT EXISTS EventRecurrenceId ON Event(calendarId, icsuid, recurrenceId);"),
    ])
}

// Applied inside tokio-rusqlite .call() before normal operations
conn.call(|db| {
    // WAL + performance pragmas (must be set on every connection open)
    db.pragma_update(None, "journal_mode", "WAL")?;
    db.pragma_update(None, "page_size", 4096)?;
    db.pragma_update(None, "cache_size", 10000)?;
    db.pragma_update(None, "synchronous", "NORMAL")?;
    db.busy_timeout(std::time::Duration::from_millis(5000))?;

    // Then run migrations
    let migrations = build_migrations();
    migrations.to_latest(db).map_err(|e| rusqlite::Error::InvalidQuery)?;
    Ok(())
}).await?;
```

**Critical:** `PRAGMA journal_mode = WAL` must be set on EVERY connection open, not just once. It is a connection-level property even though WAL persists in the database file. The C++ code does this in the `MailStore` constructor.

**Critical:** The C++ schema jumps from V4 to V6 — there is no V5 migration block. `rusqlite_migration` sets `user_version = array_index + 1`, so a 9-entry array produces user_version 1-9, matching `CURRENT_VERSION = 9` in C++. The no-op `M::up("")` at index 4 preserves this alignment when opening existing C++ databases.

### Anti-Patterns to Avoid

- **Calling rusqlite directly on async tasks:** Any `rusqlite::Connection` method call outside a `tokio_rusqlite::Connection::call()` closure will block the tokio thread pool thread, causing starvation. This is the exact problem tokio-rusqlite solves.
- **Creating multiple `tokio_rusqlite::Connection` instances for the same file:** Multiple writers create SQLITE_BUSY risk even in WAL mode. Use a single `Connection` for writes; read-only connections for Electron's side are separate.
- **Storing model JSON in the `data` column without all indexed column projections:** The TypeScript DatabaseStore queries use the indexed columns (e.g., `WHERE unread = 1`). Both must be updated atomically.
- **Emitting one delta per save without coalescing:** The C++ system explicitly coalesces to prevent "thrashing on the JS side." High-frequency saves (e.g., syncing 1000 messages) must batch through the coalescing window.
- **Using `#[serde(flatten)]` for the `data` column:** The entire model struct is serialized as one JSON blob to a TEXT column. Do NOT use `serde(flatten)` — serialize the whole struct with `serde_json::to_string()`.
- **Missing `__cls` key in dispatch JSON:** The C++ `MailModel::toJSON()` adds `_data["__cls"] = this->tableName()`. The Rust model's `to_json_dispatch()` must also inject `"__cls"` into the JSON. Electron uses it for dispatching.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Schema migration with user_version tracking | Custom PRAGMA user_version + SQL runner | rusqlite_migration 2.3.x | Handles V1-V9 chain, user_version PRAGMA tracking, and M::up() batches correctly |
| JSON serialization of model structs | Custom JSON builder | serde + serde_json | Standard; `#[serde(rename = "hMsgId")]` handles all the C++ key mappings |
| SQLite `ToSql` / `FromSql` for `serde_json::Value` | Custom SQL binding for JSON | rusqlite's built-in `serde_json` feature | Already implemented; `Value` implements `ToSql` as TEXT and `FromSql` back |
| Async SQLite access | `tokio::task::spawn_blocking` wrapping rusqlite calls | tokio-rusqlite 0.7.0 | Provides queued single-background-thread; `spawn_blocking` creates unbounded threads under load |
| FTS5 tokenizer configuration | Custom FTS5 setup | `bundled` feature + SQL `CREATE VIRTUAL TABLE ... USING fts5(tokenize = 'porter unicode61', ...)` | The bundled SQLite includes the porter + unicode61 tokenizers; must match C++ exactly |

**Key insight:** The "fat row" JSON pattern shifts complexity from SQL schema design to JSON key mapping. The 50+ `#[serde(rename)]` annotations are unavoidable — they must exactly match the C++ JSON keys or the TypeScript parser will fail silently (it expects specific field names).

---

## Common Pitfalls

### Pitfall 1: WAL Mode Not Set on Every Connection Open

**What goes wrong:** SQLite WAL mode is set once per database file but must be configured on each connection open via PRAGMA. If the Rust code opens a new connection (e.g., for migration) without `PRAGMA journal_mode = WAL`, writes may fall back to delete-journal mode, causing SQLITE_BUSY for concurrent readers.
**Why it happens:** WAL is a per-database property, but the PRAGMA must be executed per connection to engage WAL. The C++ code does this in `MailStore()` constructor before any other operations.
**How to avoid:** Set WAL, `busy_timeout`, `page_size`, `cache_size`, and `synchronous` inside the `tokio_rusqlite::Connection::call()` initialization block, before running migrations.
**Warning signs:** Any `SQLITE_BUSY` error when the Electron reader is active.

### Pitfall 2: tokio-rusqlite 0.7.0 `bundled` Feature Not Propagated

**What goes wrong:** FTS5 `CREATE VIRTUAL TABLE ... USING fts5(...)` fails with "no such module: fts5" at runtime.
**Why it happens:** tokio-rusqlite 0.7.0 changed the `bundled` feature from a default to opt-in. Without `bundled`, the system SQLite is used, which may lack FTS5. Ubuntu 20.04 ships SQLite 3.31 without FTS5.
**How to avoid:** Add `features = ["bundled", "serde_json"]` to the tokio-rusqlite dependency in Cargo.toml.
**Warning signs:** Migration fails with "no such module: fts5" on Linux CI but not macOS (system SQLite on macOS includes FTS5).

### Pitfall 3: V5 Migration Gap in C++ Schema

**What goes wrong:** The C++ `constants.h` defines V1, V2, V3, V4, V6, V7, V8, V9 — there is no V5 block. An off-by-one in the rusqlite_migration index results in mismatched user_version values. Existing C++ databases open with Rust code will try to re-run migrations or skip the wrong ones.
**Why it happens:** V5 was removed from the C++ codebase. The `MailStore::migrate()` method has `if (version < 4)` then immediately `if (version < 6)` — there is no `if (version < 5)` guard. The `user_version` goes from 4 to 9 when version is 4 (all blocks 6,7,8,9 run). CONFIRMED by reading MailStore.cpp lines 138-163.
**How to avoid:** Include `M::up("")` at index 4 (version 5) in the Migrations array. This preserves user_version alignment: the 9-element array sets user_version 1-9, matching C++ `CURRENT_VERSION = 9`.
**Warning signs:** Rust binary run against an existing C++ database produces migration errors or re-runs V6 destructive DROP.

### Pitfall 4: JSON Key Mismatch Between Rust Struct and TypeScript Parser

**What goes wrong:** TypeScript `message.headerMessageId` is undefined because the Rust struct serialized it as `header_message_id` instead of `hMsgId`.
**Why it happens:** Rust's default serde serialization uses snake_case. The C++ code uses camelCase and abbreviated keys (`hMsgId`, `rthMsgId`, `aid`, `v`). Without `#[serde(rename = "...")]` on every field, the JSON keys will not match.
**How to avoid:** Cross-reference every field with the C++ `_data["key"]` assignments in each model's `.cpp` file. Map TypeScript `Attributes.String({ jsonKey: 'hMsgId' })` entries to verify. Full mapping is in the Deep Dive section below.
**Warning signs:** TypeScript runtime errors about undefined properties on model objects after receiving a delta.

### Pitfall 5: Delta `model_class` Must Match C++ `tableName()`

**What goes wrong:** Electron's `DatabaseChangeRecord` does not recognize the modelClass and discards the delta silently.
**Why it happens:** The TypeScript `DatabaseStore` dispatches deltas by `objectClass` string matching C++ table names: `"Message"`, `"Thread"`, `"Folder"` — not Rust enum names or snake_case names.
**How to avoid:** The `model_class` field in every `DeltaStreamItem` must be the exact C++ table name string: `Message`, `Thread`, `Folder`, `Label`, `Contact`, `ContactBook`, `ContactGroup`, `Calendar`, `Event`, `Task`, `File`, `Identity`, `ModelPluginMetadata`.
**Warning signs:** Electron UI does not update after sync, despite deltas being emitted to stdout.

### Pitfall 6: Transaction Atomicity With Coalescing

**What goes wrong:** When a transaction saves 50 messages, the coalescing window emits a single delta per model class (correct). But if the transaction is rolled back, deltas accumulated during the transaction must not be emitted.
**Why it happens:** The C++ `_transactionOpen` flag gates `_emit()`: deltas go to `_transactionDeltas` (not the stream) during a transaction, and are emitted only on `commitTransaction()`.
**How to avoid:** Track in-transaction deltas in a separate `Vec<DeltaStreamItem>` within `MailStore`. On commit, send the batch to the delta channel. On rollback, clear the Vec. Do not send any deltas to the mpsc channel during an open transaction.
**Warning signs:** Rolled-back partial syncs cause phantom model updates in the Electron UI.

### Pitfall 7: ModelPluginMetadata Join Table Maintenance

**What goes wrong:** Metadata plugin queries from TypeScript (for snooze, read receipts, etc.) return stale data because `ModelPluginMetadata` table is not updated when a model with metadata is saved.
**Why it happens:** The C++ `MailModel::afterSave()` maintains the `ModelPluginMetadata` table by deleting and re-inserting rows for each `pluginId`. The Rust `MailModel::after_save()` must implement the same logic for any model where `supports_metadata()` is true (Message, Thread).
**How to avoid:** In `after_save()` for metadata-supporting models: `DELETE FROM ModelPluginMetadata WHERE id = ?` then re-insert one row per non-empty metadata entry.
**Warning signs:** Snoozed messages never reappear; plugin-dependent UI features stop working.

---

## Code Examples

Verified patterns from C++ source and tokio-rusqlite/rusqlite official docs:

### Opening and Initializing the Database

```rust
// Source: tokio-rusqlite 0.7.0 docs + C++ MailStore constructor
use tokio_rusqlite::Connection;

pub async fn open_mail_store(db_path: &str) -> anyhow::Result<Connection> {
    let conn = Connection::open(db_path).await?;

    conn.call(|db| {
        // These are per-connection pragmas — set on every open
        db.pragma_update(None, "journal_mode", "WAL")?;
        db.pragma_update(None, "page_size", 4096)?;
        db.pragma_update(None, "cache_size", 10000)?;
        db.pragma_update(None, "synchronous", "NORMAL")?;
        db.busy_timeout(std::time::Duration::from_millis(5000))?;
        Ok(())
    }).await?;

    Ok(conn)
}
```

### Running Schema Migrations

```rust
// Source: rusqlite_migration 2.4.x docs + constants.h V1-V9
use rusqlite_migration::{Migrations, M};

async fn run_migrations(conn: &Connection) -> anyhow::Result<()> {
    conn.call(|db| {
        let migrations = Migrations::new(vec![
            M::up(V1_SETUP_SQL),  // All V1_SETUP_QUERIES joined by ";"
            M::up(V2_SETUP_SQL),  // MessageUIDScanIndex
            M::up(V3_SETUP_SQL),  // MessageBody.fetchedAt
            M::up(V4_SETUP_SQL),  // Task cleanup + index
            M::up(""),            // V5 never existed — no-op to preserve user_version numbering
            M::up(V6_SETUP_SQL),  // Event table rebuild
            M::up(V7_SETUP_SQL),  // Event.icsuid
            M::up(V8_SETUP_SQL),  // Contact extensions + ContactGroup + ContactBook
            M::up(V9_SETUP_SQL),  // Event.recurrenceId
        ]);
        migrations.to_latest(db)
            .map_err(|e| rusqlite::Error::InvalidQuery)
    }).await?;
    Ok(())
}
```

### Saving a Model (Generic Pattern)

```rust
// Source: derived from C++ MailStore::save() + tokio-rusqlite 0.7.0
pub async fn save_message(conn: &Connection, msg: &mut Message) -> anyhow::Result<()> {
    msg.increment_version();
    let data_json = serde_json::to_string(&msg.to_json())?;
    let id = msg.id.clone();
    let account_id = msg.account_id.clone();
    let version = msg.version;
    let header_message_id = msg.header_message_id.clone();
    // ... clone all indexed fields before move into closure ...

    conn.call(move |db| {
        if version > 1 {
            let mut stmt = db.prepare_cached(
                "UPDATE Message SET data=?1, accountId=?2, version=?3, \
                 headerMessageId=?4, unread=?5, starred=?6, draft=?7, \
                 threadId=?8, remoteUID=?9, remoteFolderId=?10 WHERE id=?11"
            )?;
            stmt.execute(rusqlite::params![
                data_json, account_id, version, header_message_id,
                // ... other indexed fields ...
                id
            ])?;
        } else {
            let mut stmt = db.prepare_cached(
                "INSERT INTO Message (id, data, accountId, version, headerMessageId, ...) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ...)"
            )?;
            stmt.execute(rusqlite::params![id, data_json, account_id, version, ...])?;
        }
        Ok(())
    }).await?;
    Ok(())
}
```

### FTS5 Virtual Table Creation (from V1 schema)

```sql
-- Source: constants.h V1_SETUP_QUERIES — exact SQL to replicate
CREATE VIRTUAL TABLE IF NOT EXISTS `ThreadSearch`
    USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, subject, to_, from_, categories, body);

CREATE VIRTUAL TABLE IF NOT EXISTS `EventSearch`
    USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, title, description, location, participants);

CREATE VIRTUAL TABLE IF NOT EXISTS `ContactSearch`
    USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, content);
```

### Delta Wire Format

```json
// Source: C++ DeltaStreamItem::dump() — exact wire format Electron expects
{"type":"persist","modelClass":"Message","modelJSONs":[{"id":"abc","aid":"x","v":2,"hMsgId":"...","__cls":"Message"}]}
{"type":"unpersist","modelClass":"Thread","modelJSONs":[{"id":"t:abc"}]}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `spawn_blocking` wrapping rusqlite | tokio-rusqlite single background thread | 2021+ | Avoids thread pool exhaustion under load |
| Manual PRAGMA user_version for migrations | rusqlite_migration 2.3.x with M::up() | 2022+ | Cleaner migration chain, no boilerplate |
| System SQLite (may lack FTS5) | `bundled` feature in rusqlite/tokio-rusqlite | rusqlite ~0.28 | Deterministic FTS5 availability |
| tokio-rusqlite `bundled` as default | Must opt-in with `features = ["bundled"]` in 0.7.0 | Nov 2025 (0.7.0) | Breaking change — CI may silently use system SQLite |

**Deprecated/outdated:**
- `tokio::task::spawn_blocking` for rusqlite: still works but creates unbounded OS threads under load; replaced by tokio-rusqlite's queued single-thread design.
- Direct `rusqlite::Connection` on async tasks: blocked by Rust's type system (`Connection: !Send` in some configurations) and causes tokio thread starvation.

---

## Open Questions (RESOLVED)

All three open questions have been resolved by reading the C++ source directly.

### Q1: V5 Migration No-Op Strategy — RESOLVED

**Resolution:** CONFIRMED no V5 exists. Reading `MailStore.cpp` lines 115-157 shows the migrate() function has guards: `if (version < 1)`, `if (version < 2)`, `if (version < 3)`, `if (version < 4)`, `if (version < 6)`, `if (version < 7)`, `if (version < 8)`, `if (version < 9)`. There is NO `if (version < 5)` block — V5 is permanently skipped. `CURRENT_VERSION = 9` is confirmed at line 103.

**Action:** Include `M::up("")` at array index 4 in the Migrations vec. rusqlite_migration sets `user_version = index + 1`, so 9 entries produces user_version 1 through 9. The empty string migration is valid (rusqlite executes an empty SQL batch as a no-op). This ensures a database created at C++ user_version=9 is recognized as fully migrated by the Rust code.

### Q2: `prepare_cached` Safety — RESOLVED

**Resolution:** CONFIRMED fully safe. The C++ MailStore enforces single-thread access via `assertCorrectThread()` — every public method asserts the caller is on `_owningThread`. This is the same invariant tokio-rusqlite enforces: its single background thread is the only thread that ever calls the rusqlite Connection. Since closures are executed sequentially (one at a time), there is no possibility of concurrent access to the LRU statement cache. `prepare_cached` is the correct choice for all hot paths (save, find, remove).

**Additional finding:** The C++ MailStore caches prepared statements in `map<string, shared_ptr<SQLite::Statement>> _saveUpdateQueries` and `_saveInsertQueries` — one statement per table name. This is the moral equivalent of `prepare_cached`, confirming the approach.

### Q3: Delta Coalescing Delay Values — RESOLVED

**Resolution:** The `_streamMaxDelay` field has NO default initializer in the C++ constructor — it is uninitialized and set exclusively via `setStreamDelay()`. Reading the callers via grep confirms:

| Caller | Delay | Context |
|--------|-------|---------|
| `SyncWorker.cpp:62` | **500ms** | Background/foreground IMAP sync workers — the normal case |
| `main.cpp:621` | **5ms** | Task processor in test/migrate mode |
| `DeltaStream::sendUpdatedSecrets()` | **0ms** | ProcessAccountSecretsUpdated — immediate, bypasses channel |
| `DeltaStream::beginConnectionError()` | **0ms** | ProcessState — immediate, bypasses channel |
| `DeltaStream::endConnectionError()` | **0ms** | ProcessState — immediate, bypasses channel |

**Action for Rust implementation:**
- Normal model saves (Message, Thread, etc.) use the coalescing channel with 500ms window.
- `ProcessState` and `ProcessAccountSecretsUpdated` deltas bypass the coalescing channel entirely and write directly to the stdout mpsc sender with no delay.
- The 5ms value in task processing (test mode) is effectively immediate and can be treated as 0ms in the Rust implementation.

---

## Deep Dive: Model Field Mapping

Complete audit of all 13 model `.cpp` files. For each model: table name, JSON keys in `_data`, indexed columns for SQL binding, and metadata support.

### Base Class: MailModel

All models inherit three JSON keys from the base constructor and `bindToQuery()`:

| JSON Key | C++ Field | TypeScript jsonKey | Indexed Column |
|----------|-----------|-------------------|----------------|
| `id` | `_data["id"]` | `id` (no rename) | `id` (PRIMARY KEY) |
| `aid` | `_data["aid"]` | `aid` | `accountId` |
| `v` | `_data["v"]` | `v` | `version` |
| `__cls` | injected by `toJSON()` | `__cls` (used for dispatch) | not stored |
| `metadata` | `_data["metadata"]` | `metadata` via `pluginMetadata` | join table only |

**Note:** `MailModel::bindToQuery()` binds `:id`, `:data`, `:accountId`, `:version`. All subclass `bindToQuery()` implementations call `MailModel::bindToQuery()` first then bind additional columns.

---

### Message

**Table:** `Message`
**Supports metadata:** YES (`supportsMetadata()` returns true)
**`columnsForQuery()`:** `{id, data, accountId, version, headerMessageId, subject, gMsgId, date, draft, unread, starred, remoteUID, remoteXGMLabels, remoteFolderId, threadId}`

**JSON keys in `_data` (from Message.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | message id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `_sa` | syncedAt (unix time_t) | no |
| `_suc` | syncUnsavedChanges (int) | no |
| `remoteUID` | IMAP UID (uint32_t) | `remoteUID` |
| `files` | array of File JSON blobs | no |
| `date` | header date or receivedDate | `date` |
| `hMsgId` | headerMessageId | `headerMessageId` |
| `subject` | email subject | `subject` |
| `gMsgId` | Gmail message ID string | `gMsgId` |
| `rthMsgId` | replyToHeaderMessageId (nullable) | no |
| `fwdMsgId` | forwardedHeaderMessageId (nullable) | no |
| `unread` | bool | `unread` |
| `starred` | bool | `starred` |
| `labels` | array of X-GM-LABELS strings | `remoteXGMLabels` (as JSON dump) |
| `draft` | bool | `draft` |
| `extraHeaders` | object of extra IMAP headers | no |
| `from` | array of contact JSON | no |
| `to` | array of contact JSON | no |
| `cc` | array of contact JSON | no |
| `bcc` | array of contact JSON | no |
| `replyTo` | array of contact JSON | no |
| `folder` | client folder JSON (Folder.toJSON()) | no |
| `remoteFolder` | remote folder JSON (Folder.toJSON()) | `remoteFolderId` (via remoteFolder["id"]) |
| `threadId` | thread id | `threadId` |
| `snippet` | short preview (set separately) | no |
| `plaintext` | bool (set separately) | no |
| `metadata` | array of plugin metadata objects | join table |
| `__cls` | "Message" (injected by toJSON) | no |

**toJSONDispatch() additions (conditional, not in data column):**
- `body` — string, only when `_bodyForDispatch.length() > 0`
- `fullSyncComplete` — true, only when body is present
- `headersSyncComplete` — true, only when `version() == 1`

**Indexed column → C++ binding:**
- `remoteXGMLabels`: bound as `remoteXGMLabels().dump()` (JSON array serialized to string)
- `remoteFolderId`: bound as `remoteFolderId()` which returns `_data["remoteFolder"]["id"]`

---

### Thread

**Table:** `Thread`
**Supports metadata:** YES (`supportsMetadata()` returns true)
**`columnsForQuery()`:** `{id, data, accountId, version, gThrId, unread, starred, inAllMail, subject, lastMessageTimestamp, lastMessageReceivedTimestamp, lastMessageSentTimestamp, firstMessageTimestamp, hasAttachments}`

**JSON keys in `_data` (from Thread.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | "t:" + msgId | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `subject` | thread subject | `subject` |
| `lmt` | lastMessageTimestamp | `lastMessageTimestamp` |
| `fmt` | firstMessageTimestamp | `firstMessageTimestamp` |
| `lmst` | lastMessageSentTimestamp | `lastMessageSentTimestamp` |
| `lmrt` | lastMessageReceivedTimestamp | `lastMessageReceivedTimestamp` |
| `gThrId` | Gmail thread ID string | `gThrId` |
| `unread` | int (count) | `unread` |
| `starred` | int (count) | `starred` |
| `inAllMail` | bool | `inAllMail` |
| `attachmentCount` | int | `hasAttachments` (column name differs!) |
| `searchRowId` | FTS5 rowid | no |
| `folders` | array of folder objects with `_refs`, `_u` | no |
| `labels` | array of label objects with `_refs`, `_u` | no |
| `participants` | array of contact JSON | no |
| `metadata` | array of plugin metadata objects | join table |
| `lmrt_is_fallback` | bool, transient, erased when real value found | no |
| `__cls` | "Thread" | no |

**Note:** `hasAttachments` in the indexed column is bound as `(double)attachmentCount()`. The column is named `hasAttachments` in the DB but the JSON key is `attachmentCount`. This is a name mismatch that the Rust code must replicate exactly.

**afterSave() side effects:**
- Maintains `ThreadCategory` join table (DELETE + INSERT per category id)
- Maintains `ThreadCounts` table (UPDATE unread/total counters)
- Optionally updates `ThreadSearch` FTS5 index (categories field)

---

### Folder

**Table:** `Folder`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, data, accountId, version, path, role}`

**JSON keys in `_data`:**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | folder id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `path` | IMAP path | `path` |
| `role` | folder role (inbox, sent, etc.) | `role` |
| `localStatus` | object with sync state | no |
| `__cls` | "Folder" | no |

**beforeSave() side effect:** On version==1, inserts `ThreadCounts (categoryId, 0, 0)` row.
**afterRemove() side effect:** Deletes `ThreadCounts WHERE categoryId = id`.

---

### Label

**Table:** `Label`
**Supports metadata:** NO (inherits from Folder)
**`columnsForQuery()`:** Same as Folder: `{id, data, accountId, version, path, role}`

**JSON keys in `_data`:** Same as Folder. Label is a subclass of Folder in C++ — it uses the same `bindToQuery()` and same column set.

**Note:** When a Label is saved, `globalLabelsVersion` atomic is incremented (in `MailStore::save()`), invalidating the label cache. The Rust implementation must maintain an equivalent label cache invalidation mechanism.

---

### Contact

**Table:** `Contact`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, data, accountId, version, refs, email, hidden, source, etag, bookId}`

**JSON keys in `_data` (from Contact.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | contact id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `email` | email address | `email` |
| `s` | source (mail, carddav, gpeople) | `source` |
| `refs` | reference count | `refs` |
| `gis` | array of group ids | no |
| `info` | object (vcf or google contact info) | no |
| `name` | display name | no |
| `grn` | Google resource name (optional) | no |
| `etag` | CardDAV etag (optional) | `etag` |
| `bid` | book id (optional) | `bookId` |
| `h` | hidden bool | `hidden` |
| `__cls` | "Contact" | no |

**afterSave() side effects:**
- On version==1: INSERT INTO ContactSearch (content_id, content)
- On version>1 AND source != 'mail': UPDATE ContactSearch SET content = ?

**afterRemove() side effect:** DELETE FROM ContactSearch WHERE content_id = ?

---

### ContactBook

**Table:** `ContactBook`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, accountId, version, data}` (NOTE: no extra indexed columns beyond base)

**JSON keys in `_data` (from ContactBook.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | book id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `url` | CardDAV URL | no |
| `source` | source type | no |
| `ctag` | CardDAV ctag (optional) | no |
| `syncToken` | CardDAV sync-token (optional) | no |
| `__cls` | "ContactBook" | no |

**Note:** ContactBook's `bindToQuery()` calls `MailModel::bindToQuery()` only — no additional bindings. The `columnsForQuery()` deliberately omits `source` and `url` from indexed columns (they are only in the `data` blob).

---

### ContactGroup

**Table:** `ContactGroup`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, accountId, version, data, name, bookId}`

**JSON keys in `_data` (from ContactGroup.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | group id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `name` | group name | `name` |
| `bid` | book id | `bookId` |
| `grn` | Google resource name (optional) | no |
| `__cls` | "ContactGroup" | no |

**afterRemove() side effect:** DELETE FROM ContactContactGroup WHERE value = id.

---

### Calendar

**Table:** `Calendar`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, data, accountId}` (NOTE: no version column!)

**JSON keys in `_data` (from Calendar.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | calendar id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version (via base) | NOT indexed (omitted from columnsForQuery) |
| `path` | CalDAV path | no |
| `name` | calendar name | no |
| `ctag` | CalDAV ctag (optional) | no |
| `syncToken` | CalDAV sync-token (optional) | no |
| `color` | hex color string | no |
| `description` | description (optional) | no |
| `read_only` | bool | no |
| `order` | display order int | no |
| `__cls` | "Calendar" | no |

**CRITICAL:** Calendar's `bindToQuery()` does NOT call `MailModel::bindToQuery()` — it binds only `:id`, `:data`, `:accountId`. There is no `:version` binding. The table definition has no `version` column. The Rust Calendar model must NOT bind version.

---

### Event

**Table:** `Event`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, data, icsuid, recurrenceId, accountId, etag, calendarId, recurrenceStart, recurrenceEnd}`

**JSON keys in `_data` (from Event.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | event id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version (via base) | no (not in columnsForQuery) |
| `cid` | calendarId | `calendarId` |
| `icsuid` | ICS UID | `icsuid` |
| `ics` | raw ICS string | no |
| `href` | CalDAV href (optional) | no |
| `etag` | CalDAV etag | `etag` |
| `rid` | recurrenceId (optional, empty string if not exception) | `recurrenceId` |
| `status` | CONFIRMED/TENTATIVE/CANCELLED | no |
| `rs` | recurrenceStart (unix int) | `recurrenceStart` |
| `re` | recurrenceEnd (unix int) | `recurrenceEnd` |
| `__cls` | "Event" | no |

**CRITICAL:** Event's `bindToQuery()` does NOT call `MailModel::bindToQuery()` — it binds `:id`, `:data`, `:icsuid`, `:recurrenceId`, `:accountId`, `:etag`, `:calendarId`, `:recurrenceStart`, `:recurrenceEnd` directly. No `:version` binding.

**afterSave() side effects:**
- INSERT/UPDATE EventSearch FTS5 table (title, description, location, participants)
- Only triggered when `_searchTitle`/`_searchDescription`/`_searchLocation`/`_searchParticipants` are non-empty (i.e., event was constructed from ICS data, not loaded from DB)

**afterRemove() side effect:** DELETE FROM EventSearch WHERE content_id = ?

---

### Task (mail task model, not async task)

**Table:** `Task`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, data, accountId, version, status}`

**JSON keys in `_data` (from Task.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | random id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `__cls` | task type (e.g., "SendDraftTask") | no |
| `status` | "local", "remote", "complete", "cancelled" | `status` |
| `should_cancel` | bool (optional) | no |
| `error` | error JSON (optional) | no |
| (task-specific fields) | varies per task type | no |

**Note:** Task's `toJSON()` is NOT overridden (inherited from MailModel). The base class adds `__cls` only if not already present. For Task, `__cls` is set in the constructor to the task type name (e.g., `"SendDraftTask"`), so it is already present in `_data` when `toJSON()` runs.

---

### File

**Table:** `File`
**Supports metadata:** NO
**`columnsForQuery()`:** `{id, data, accountId, version, filename}`

**JSON keys in `_data` (from File.cpp):**

| JSON Key | Value | Indexed Column |
|----------|-------|----------------|
| `id` | file id | `id` (PRIMARY KEY) |
| `aid` | account id | `accountId` |
| `v` | version | `version` |
| `messageId` | parent message id | no |
| `partId` | IMAP BODYSTRUCTURE part id | no |
| `contentId` | CID for inline attachments (optional) | no |
| `contentType` | MIME type | no |
| `filename` | display name | `filename` |
| `size` | byte size | no |
| `__cls` | "File" | no |

**Note:** File objects are embedded in `Message._data["files"]` as a JSON array AND stored in the File table. Both must be maintained.

---

### Identity

**Table:** NONE — Identity is not stored in the database.

**Notes from Identity.cpp:**
- `tableName()` calls `assert(false)` — Identity is never saved to SQLite
- `columnsForQuery()` calls `assert(false)`
- `bindToQuery()` calls `assert(false)`
- Used only as a global singleton: `Identity::GetGlobal()` / `Identity::SetGlobal()`
- Contains: `emailAddress`, `firstName`, `lastName`, `token`, `createdAt` JSON fields

**Rust implementation:** Identity should be a plain struct (not implementing MailModel trait) used only for process-level state management. Do NOT attempt to persist it.

---

### ModelPluginMetadata

**Table:** `ModelPluginMetadata`
**This model is a join table, not a "fat row" model.**

**Schema (from constants.h V1):**
```sql
CREATE TABLE IF NOT EXISTS `ModelPluginMetadata` (
    id VARCHAR(40),
    `accountId` VARCHAR(8),
    `objectType` VARCHAR(15),
    `value` TEXT,
    `expiration` DATETIME,
    PRIMARY KEY (`value`, `id`)
)
```

**Columns and their meaning:**
| Column | Value |
|--------|-------|
| `id` | The model's id (e.g., a Thread id or Message id) |
| `accountId` | account id |
| `objectType` | "Thread" or "Message" (the model's tableName) |
| `value` | pluginId (e.g., "snooze-plugin") |
| `expiration` | unix timestamp if metadata has expiration, else NULL |

**How it is maintained:** Via `MailModel::afterSave()` in the base class, for any model where `supportsMetadata()` is true. The logic: DELETE all rows WHERE id = ?, then re-INSERT one row per non-empty metadata entry. The `value` column stores the pluginId string, NOT the metadata value JSON.

**Note:** There is NO `data` blob column on this table. It is a join/lookup table only. The actual metadata values are embedded in the parent model's `data` JSON under the `metadata` array.

---

## Deep Dive: Schema SQL

Exact SQL from `constants.h` for all migration versions. This is the authoritative source for the Rust `schema.rs` file.

### V1 Setup Queries (Initial Schema)

```sql
CREATE TABLE IF NOT EXISTS `_State` (id VARCHAR(40) PRIMARY KEY, value TEXT);

CREATE TABLE IF NOT EXISTS `File` (id VARCHAR(40) PRIMARY KEY, version INTEGER, data BLOB, accountId VARCHAR(8), filename TEXT);

CREATE TABLE IF NOT EXISTS `Event` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), calendarId VARCHAR(40), _start INTEGER, _end INTEGER, is_search_indexed INTEGER DEFAULT 0);
CREATE INDEX IF NOT EXISTS EventIsSearchIndexedIndex ON `Event` (is_search_indexed, id);

CREATE VIRTUAL TABLE IF NOT EXISTS `EventSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, title, description, location, participants);

CREATE TABLE IF NOT EXISTS Label (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data TEXT, path VARCHAR(255), role VARCHAR(255), createdAt DATETIME, updatedAt DATETIME);

CREATE TABLE IF NOT EXISTS Folder (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data TEXT, path VARCHAR(255), role VARCHAR(255), createdAt DATETIME, updatedAt DATETIME);

CREATE TABLE IF NOT EXISTS Thread (id VARCHAR(42) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data TEXT, gThrId VARCHAR(20), subject VARCHAR(500), snippet VARCHAR(255), unread INTEGER, starred INTEGER, firstMessageTimestamp DATETIME, lastMessageTimestamp DATETIME, lastMessageReceivedTimestamp DATETIME, lastMessageSentTimestamp DATETIME, inAllMail TINYINT(1), isSearchIndexed TINYINT(1), participants TEXT, hasAttachments INTEGER);

CREATE INDEX IF NOT EXISTS ThreadDateIndex ON `Thread` (lastMessageReceivedTimestamp DESC);
CREATE INDEX IF NOT EXISTS ThreadUnreadIndex ON `Thread` (accountId, lastMessageReceivedTimestamp DESC) WHERE unread = 1 AND inAllMail = 1;
CREATE INDEX IF NOT EXISTS ThreadUnifiedUnreadIndex ON `Thread` (lastMessageReceivedTimestamp DESC) WHERE unread = 1 AND inAllMail = 1;
CREATE INDEX IF NOT EXISTS ThreadStarredIndex ON `Thread` (accountId, lastMessageReceivedTimestamp DESC) WHERE starred = 1 AND inAllMail = 1;
CREATE INDEX IF NOT EXISTS ThreadUnifiedStarredIndex ON `Thread` (lastMessageReceivedTimestamp DESC) WHERE starred = 1 AND inAllMail = 1;
CREATE INDEX IF NOT EXISTS ThreadGmailLookup ON `Thread` (gThrId) WHERE gThrId IS NOT NULL;
CREATE INDEX IF NOT EXISTS ThreadIsSearchIndexedIndex ON `Thread` (isSearchIndexed, id);
CREATE INDEX IF NOT EXISTS ThreadIsSearchIndexedLastMessageReceivedIndex ON `Thread` (isSearchIndexed, lastMessageReceivedTimestamp);

CREATE TABLE IF NOT EXISTS ThreadReference (threadId VARCHAR(42), accountId VARCHAR(8), headerMessageId VARCHAR(255), PRIMARY KEY (threadId, accountId, headerMessageId));

CREATE TABLE IF NOT EXISTS ThreadCategory (id VARCHAR(40), value VARCHAR(40), inAllMail TINYINT(1), unread TINYINT(1), lastMessageReceivedTimestamp DATETIME, lastMessageSentTimestamp DATETIME, PRIMARY KEY (id, value));

CREATE INDEX IF NOT EXISTS `ThreadCategory_id` ON `ThreadCategory` (`id` ASC);
CREATE UNIQUE INDEX IF NOT EXISTS `ThreadCategory_val_id` ON `ThreadCategory` (`value` ASC, `id` ASC);
CREATE INDEX IF NOT EXISTS ThreadListCategoryIndex ON `ThreadCategory` (lastMessageReceivedTimestamp DESC, value, inAllMail, unread, id);
CREATE INDEX IF NOT EXISTS ThreadListCategorySentIndex ON `ThreadCategory` (lastMessageSentTimestamp DESC, value, inAllMail, unread, id);

CREATE TABLE IF NOT EXISTS `ThreadCounts` (`categoryId` TEXT PRIMARY KEY, `unread` INTEGER, `total` INTEGER);

CREATE VIRTUAL TABLE IF NOT EXISTS `ThreadSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, subject, to_, from_, categories, body);

CREATE TABLE IF NOT EXISTS `Account` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), email_address TEXT);

CREATE TABLE IF NOT EXISTS Message (id VARCHAR(40) PRIMARY KEY, accountId VARCHAR(8), version INTEGER, data TEXT, headerMessageId VARCHAR(255), gMsgId VARCHAR(255), gThrId VARCHAR(255), subject VARCHAR(500), date DATETIME, draft TINYINT(1), unread TINYINT(1), starred TINYINT(1), remoteUID INTEGER, remoteXGMLabels TEXT, remoteFolderId VARCHAR(40), replyToHeaderMessageId VARCHAR(255), threadId VARCHAR(40));

CREATE INDEX IF NOT EXISTS MessageListThreadIndex ON Message(threadId, date ASC);
CREATE INDEX IF NOT EXISTS MessageListHeaderMsgIdIndex ON Message(headerMessageId);
CREATE INDEX IF NOT EXISTS MessageListDraftIndex ON Message(accountId, date DESC) WHERE draft = 1;
CREATE INDEX IF NOT EXISTS MessageListUnifiedDraftIndex ON Message(date DESC) WHERE draft = 1;

CREATE TABLE IF NOT EXISTS `ModelPluginMetadata` (id VARCHAR(40), `accountId` VARCHAR(8), `objectType` VARCHAR(15), `value` TEXT, `expiration` DATETIME, PRIMARY KEY (`value`, `id`));
CREATE INDEX IF NOT EXISTS `ModelPluginMetadata_id` ON `ModelPluginMetadata` (`id` ASC);
CREATE INDEX IF NOT EXISTS `ModelPluginMetadata_expiration` ON `ModelPluginMetadata` (`expiration` ASC) WHERE expiration IS NOT NULL;

CREATE TABLE IF NOT EXISTS `DetatchedPluginMetadata` (objectId VARCHAR(40), objectType VARCHAR(15), accountId VARCHAR(8), pluginId VARCHAR(40), value BLOB, version INTEGER, PRIMARY KEY (`objectId`, `accountId`, `pluginId`));

CREATE TABLE IF NOT EXISTS `MessageBody` (id VARCHAR(40) PRIMARY KEY, `value` TEXT);
CREATE UNIQUE INDEX IF NOT EXISTS MessageBodyIndex ON MessageBody(id);

CREATE TABLE IF NOT EXISTS `Contact` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), email TEXT, version INTEGER, refs INTEGER DEFAULT 0);
CREATE INDEX IF NOT EXISTS ContactEmailIndex ON Contact(email);
CREATE INDEX IF NOT EXISTS ContactAccountEmailIndex ON Contact(accountId, email);

CREATE VIRTUAL TABLE IF NOT EXISTS `ContactSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, content);

CREATE TABLE IF NOT EXISTS `Calendar` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8));

CREATE TABLE IF NOT EXISTS `Task` (id VARCHAR(40) PRIMARY KEY, version INTEGER, data BLOB, accountId VARCHAR(8), status VARCHAR(255));
```

### V2 Setup Queries

```sql
CREATE INDEX IF NOT EXISTS MessageUIDScanIndex ON Message(accountId, remoteFolderId, remoteUID);
```

### V3 Setup Queries

```sql
ALTER TABLE `MessageBody` ADD COLUMN fetchedAt DATETIME;
UPDATE `MessageBody` SET fetchedAt = datetime('now');
```

### V4 Setup Queries

```sql
DELETE FROM Task WHERE Task.status = "complete" OR Task.status = "cancelled";
CREATE INDEX IF NOT EXISTS TaskByStatus ON Task(accountId, status);
```

### V5 — DOES NOT EXIST

There is no V5 in the C++ codebase. Insert `M::up("")` (no-op) in the rusqlite_migration array at index 4 to preserve user_version numbering alignment with C++ `CURRENT_VERSION = 9`.

### V6 Setup Queries

```sql
DROP TABLE IF EXISTS `Event`;
CREATE TABLE IF NOT EXISTS `Event` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), etag VARCHAR(40), calendarId VARCHAR(40), recurrenceStart INTEGER, recurrenceEnd INTEGER);
CREATE INDEX IF NOT EXISTS EventETag ON Event(calendarId, etag);
```

### V7 Setup Queries

```sql
ALTER TABLE `Event` ADD COLUMN icsuid VARCHAR(150);
CREATE INDEX IF NOT EXISTS EventUID ON Event(accountId, icsuid);
```

### V8 Setup Queries

```sql
DELETE FROM Contact WHERE refs = 0;
ALTER TABLE `Contact` ADD COLUMN hidden TINYINT(1) DEFAULT 0;
ALTER TABLE `Contact` ADD COLUMN source VARCHAR(10) DEFAULT 'mail';
ALTER TABLE `Contact` ADD COLUMN bookId VARCHAR(40);
ALTER TABLE `Contact` ADD COLUMN etag VARCHAR(40);
CREATE INDEX IF NOT EXISTS ContactBrowseIndex ON Contact(hidden,refs,accountId);
CREATE TABLE `ContactGroup` (`id` varchar(40),`accountId` varchar(40),`bookId` varchar(40), `data` BLOB, `version` INTEGER, `name` varchar(300), PRIMARY KEY (id));
CREATE TABLE `ContactContactGroup` (`id` varchar(40),`value` varchar(40), PRIMARY KEY (id, value));
CREATE TABLE `ContactBook` (`id` varchar(40),`accountId` varchar(40), `data` BLOB, `version` INTEGER, PRIMARY KEY (id));
```

### V9 Setup Queries

```sql
ALTER TABLE `Event` ADD COLUMN recurrenceId VARCHAR(50) DEFAULT '';
CREATE INDEX IF NOT EXISTS EventRecurrenceId ON Event(calendarId, icsuid, recurrenceId);
```

---

## Deep Dive: TypeScript Cross-Check

Cross-reference of TypeScript `jsonKey` values (from `app/frontend/flux/models/`) against C++ `_data["key"]` assignments. This section documents mismatches that require attention in the Rust implementation.

### Base Model (model.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `id` | `id` | `id` | MATCH |
| `accountId` | `aid` | `aid` | MATCH |
| `version` | `v` | `v` | MATCH |

### Message (message.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `to` | `to` | `to` | MATCH |
| `cc` | `cc` | `cc` | MATCH |
| `bcc` | `bcc` | `bcc` | MATCH |
| `from` | `from` | `from` | MATCH |
| `replyTo` | `replyTo` | `replyTo` | MATCH |
| `date` | `date` | `date` | MATCH |
| `body` | `body` | not in data; via MessageBody table | SPECIAL (JoinedData) |
| `files` | `files` | `files` | MATCH |
| `unread` | `unread` | `unread` | MATCH |
| `starred` | `starred` | `starred` | MATCH |
| `snippet` | `snippet` | `snippet` | MATCH |
| `threadId` | `threadId` | `threadId` | MATCH |
| `headerMessageId` | `hMsgId` | `hMsgId` | MATCH |
| `subject` | `subject` | `subject` | MATCH |
| `draft` | `draft` | `draft` | MATCH |
| `pristine` | `pristine` | `pristine` (if set by UI) | MATCH (UI-only field, C++ ignores) |
| `plaintext` | `plaintext` | `plaintext` | MATCH |
| `version` | `v` | `v` | MATCH |
| `replyToHeaderMessageId` | `rthMsgId` | `rthMsgId` | MATCH |
| `forwardedHeaderMessageId` | `fwdMsgId` | `fwdMsgId` | MATCH |
| `folder` | `folder` | `folder` | MATCH |
| `listUnsubscribe` | `hListUnsub` | not explicitly set by C++ | C++ includes extra headers in `extraHeaders` object |
| `listUnsubscribePost` | `hListUnsubPost` | not explicitly set by C++ | Same — extra headers stored in `extraHeaders` |
| `events` | `events` | not in C++ Message | TS-only field for calendar events attached to messages |
| `pluginMetadata` | `metadata` | `metadata` | MATCH |

**MISMATCH/NOTES:**
- TS reads `hListUnsub` and `hListUnsubPost` as top-level keys. C++ stores all extra headers under `_data["extraHeaders"]["List-Unsubscribe"]` — these are NOT promoted to top-level keys by C++. The TS side may not receive these unless the C++ or Rust code explicitly promotes them. **Flag for Phase 7 IMAP body sync investigation.**
- TS has `events` (Event collection). C++ Message has no such field. This is populated by the TypeScript side when parsing inline calendar invitations — not relevant to the Rust data layer.

### Thread (thread.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `snippet` | `snippet` | `snippet` (not set in C++ Thread constructor) | C++ Thread does not store snippet; TS attribute exists but may be empty |
| `subject` | `subject` | `subject` | MATCH |
| `unread` | `unread` | `unread` | MATCH |
| `starred` | `starred` | `starred` | MATCH |
| `version` | `v` | `v` | MATCH |
| `folders` | `folders` | `folders` | MATCH |
| `labels` | `labels` | `labels` | MATCH |
| `participants` | `participants` | `participants` | MATCH |
| `attachmentCount` | `attachmentCount` | `attachmentCount` | MATCH |
| `firstMessageTimestamp` | `fmt` | `fmt` | MATCH |
| `lastMessageReceivedTimestamp` | `lmrt` | `lmrt` | MATCH |
| `lastMessageSentTimestamp` | `lmst` | `lmst` | MATCH |
| `inAllMail` | `inAllMail` | `inAllMail` | MATCH |
| `pluginMetadata` | `metadata` | `metadata` | MATCH |

**MISMATCH/NOTES:**
- TS Thread does NOT have `lmt` (lastMessageTimestamp). C++ Thread stores `lmt` in `_data` and binds it to `lastMessageTimestamp` indexed column. The TS side does not expose it as an attribute but the column value is used for sorting queries on the DB side. The Rust Thread model must still populate the `lastMessageTimestamp` indexed column even though TS has no attribute for it.
- TS Thread has a `categories` virtual attribute that combines `folders` and `labels` arrays — not a stored JSON key.
- C++ Thread also stores `gThrId` and `searchRowId` in `_data` which TS does not expose as typed attributes.

### Folder / Label (folder.ts, label.ts, category.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `role` | `role` | `role` | MATCH |
| `path` | `path` | `path` | MATCH |
| `localStatus` | `localStatus` | `localStatus` | MATCH |

Both Folder and Label inherit from Category. Label.cpp is a subclass of Folder.cpp — same JSON keys, same indexed columns. MATCH across the board.

### Contact (contact.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `name` | `name` | `name` | MATCH |
| `hidden` | `h` | `h` | MATCH |
| `source` | `s` | `s` | MATCH |
| `email` | `email` | `email` | MATCH |
| `contactGroups` | `gis` | `gis` | MATCH |
| `refs` | `refs` | `refs` | MATCH |
| `info` | `info` | `info` | MATCH |

**NOTES:**
- TS does NOT have `grn` (googleResourceName), `etag`, `bid` (bookId) as typed attributes. These are in C++ `_data` and therefore in the `data` blob, but the TypeScript side does not expose them as Model attributes. They are used server-side only.
- TS `contactGroups` jsonKey is `gis` which matches C++ `_data["gis"]`. MATCH.

### Calendar (calendar.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `name` | `name` | `name` | MATCH |
| `description` | `description` | `description` | MATCH |
| `readOnly` | `read_only` | `read_only` | MATCH |
| `color` | `color` | `color` | MATCH |
| `order` | `order` | `order` | MATCH |

**NOTES:**
- TS does not expose `path`, `ctag`, `syncToken` as typed attributes — these are sync-internal fields stored in `_data` but not read by TypeScript.
- Full MATCH on all TS-exposed attributes.

### Event (event.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `calendarId` | `cid` | `cid` | MATCH |
| `ics` | `ics` | `ics` | MATCH |
| `icsuid` | `icsuid` | `icsuid` | MATCH |
| `recurrenceId` | `rid` | `rid` | MATCH |
| `status` | `status` | `status` | MATCH |
| `recurrenceStart` | `rs` | `rs` | MATCH |
| `recurrenceEnd` | `re` | `re` | MATCH |

**NOTES:**
- TS does NOT have `href` or `etag` as typed attributes — sync-internal.
- Full MATCH on all TS-exposed attributes.

### File (file.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `filename` | `filename` | `filename` | MATCH |
| `size` | `size` | `size` | MATCH |
| `contentType` | `contentType` | `contentType` | MATCH |
| `messageId` | `messageId` | `messageId` | MATCH |
| `contentId` | `contentId` | `contentId` | MATCH |

Full MATCH.

### ContactBook (contact-book.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `readonly` | `readonly` | NOT in C++ (C++ uses `url`, `source`, `ctag`, `syncToken`) | MISMATCH — TS has different fields |
| `source` | `source` | `source` | MATCH |

**MISMATCH:** TS ContactBook has `readonly` attribute that C++ ContactBook does not store. C++ ContactBook stores `url`, `ctag`, `syncToken` that TS does not expose. The `source` field matches.

**Assessment:** ContactBook is primarily a sync-internal model. TS only needs `source` to know whether to show CardDAV vs Google contacts UI. The other fields are sync-engine state. No action required in the Rust data layer — just store the full `_data` JSON.

### ContactGroup (contact-group.ts)

| TS modelKey | TS jsonKey | C++ key | Match? |
|-------------|-----------|---------|--------|
| `name` | `name` | `name` | MATCH |

**MISMATCH:** TS ContactGroup does NOT expose `bookId` (C++ JSON key: `bid`) or `grn` (googleResourceName). These are sync-internal. The TS side only needs `name` for display.

**Assessment:** The `bookId`/`bid` value is in the `data` JSON blob but TS reads it only from the `data` field — there is no TypeScript attribute definition for it. Since the C++ indexes `bookId` as a DB column, the Rust implementation must also bind it as a column even though TS does not have a typed attribute for it.

### Summary of Critical Mismatches

| Model | Mismatch | Risk Level | Action |
|-------|---------|-----------|--------|
| Message | `hListUnsub`/`hListUnsubPost` not populated by C++ as top-level keys | LOW | Phase 7 investigation; TS may not use these from deltas |
| Thread | TS has no `lmt` attribute but DB column `lastMessageTimestamp` still required | MEDIUM | Rust Thread must populate `lastMessageTimestamp` indexed column even without TS attribute |
| Thread | TS has no `gThrId` attribute but DB column required | LOW | Rust Thread must populate `gThrId` indexed column for Gmail lookup index |
| ContactBook | TS `readonly` field not in C++ JSON; C++ fields `url`/`ctag`/`syncToken` not in TS | LOW | No action: sync-internal fields; TS only reads `source` |
| ContactGroup | TS missing `bookId` (C++ `bid`) and `grn` typed attributes | LOW | Rust still binds `bookId` indexed column; values present in `data` blob |
| Calendar | No `version` indexed column despite base class having `v` in JSON | MEDIUM | Rust Calendar must NOT bind `:version` — Calendar.bindToQuery does NOT call MailModel::bindToQuery |
| Event | No `version` indexed column; Event.bindToQuery does NOT call MailModel::bindToQuery | MEDIUM | Same as Calendar — do not bind version column |

---

## Validation Architecture

> `workflow.nyquist_validation` is not set in .planning/config.json — skip this section.

---

## Sources

### Primary (HIGH confidence)

- [tokio-rusqlite 0.7.0 docs.rs](https://docs.rs/tokio-rusqlite/latest/tokio_rusqlite/) — Connection API, call(), open(), error handling
- [tokio-rusqlite GitHub releases](https://github.com/programatik29/tokio-rusqlite/releases) — version 0.7.0 confirmed Nov 2025, rusqlite ^0.37.0 dependency
- [rusqlite docs.rs](https://docs.rs/rusqlite/latest/rusqlite/) — busy_timeout, pragma_update, execute_batch, serde_json feature
- [rusqlite GitHub releases](https://github.com/rusqlite/rusqlite/releases) — version 0.38.0 (Dec 2024), bundled = FTS5 included
- [rusqlite_migration docs.rs](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/) — version 2.4.1 (latest) requires rusqlite ^0.38.0; USE version 2.3.x which requires rusqlite ^0.37.0 to match tokio-rusqlite 0.7.0. M::up(), Migrations::to_latest(), user_version tracking API is identical between 2.3.x and 2.4.x
- C++ source: `app/mailsync/MailSync/constants.h` — exact V1-V9 SQL schema (all tables, indexes, FTS5); V5 absence confirmed
- C++ source: `app/mailsync/MailSync/DeltaStream.cpp` — delta wire format, coalescing logic, `queueDeltaForDelivery`, `flushWithin`; immediate emit (0ms) for ProcessState/ProcessAccountSecretsUpdated
- C++ source: `app/mailsync/MailSync/MailStore.cpp` — save/remove pattern, transaction pattern, `_emit` logic, `CURRENT_VERSION = 9`, V5 gap confirmed in migrate()
- C++ source: `app/mailsync/MailSync/MailStore.hpp` — `_streamMaxDelay` field declaration, `setStreamDelay()` signature
- C++ source: `app/mailsync/MailSync/SyncWorker.cpp` line 62 — `store->setStreamDelay(500)` CONFIRMED 500ms for sync workers
- C++ source: `app/mailsync/MailSync/main.cpp` line 621 — `store.setStreamDelay(5)` for task processor (effectively immediate)
- C++ source: `app/mailsync/MailSync/Models/MailModel.cpp` — base class JSON keys (`id`, `aid`, `v`), `toJSON()` adds `__cls`, `bindToQuery()` binds id/data/accountId/version, `afterSave()` metadata join table logic
- C++ source: `app/mailsync/MailSync/Models/Message.cpp` — all JSON keys, columnsForQuery, bindToQuery, afterSave thread update
- C++ source: `app/mailsync/MailSync/Models/Thread.cpp` — all JSON keys, columnsForQuery (note: `hasAttachments` column bound as `attachmentCount` value), afterSave ThreadCategory/ThreadCounts/ThreadSearch maintenance
- C++ source: `app/mailsync/MailSync/Models/Folder.cpp` — JSON keys, columnsForQuery, ThreadCounts side effects
- C++ source: `app/mailsync/MailSync/Models/Label.cpp` — Folder subclass, same JSON keys
- C++ source: `app/mailsync/MailSync/Models/Contact.cpp` — all JSON keys including abbreviated (`s`, `h`, `gis`, `bid`), ContactSearch side effects
- C++ source: `app/mailsync/MailSync/Models/ContactBook.cpp` — JSON keys, minimal columnsForQuery (no extra indexed cols)
- C++ source: `app/mailsync/MailSync/Models/ContactGroup.cpp` — JSON keys (`bid`, `grn`, `name`), ContactContactGroup join table maintenance
- C++ source: `app/mailsync/MailSync/Models/Calendar.cpp` — JSON keys, CRITICAL: does NOT call MailModel::bindToQuery, no version column
- C++ source: `app/mailsync/MailSync/Models/Event.cpp` — JSON keys (`cid`, `ics`, `icsuid`, `rid`, `rs`, `re`, `etag`, `href`, `status`), CRITICAL: does NOT call MailModel::bindToQuery, EventSearch side effects
- C++ source: `app/mailsync/MailSync/Models/Task.cpp` — JSON keys, columnsForQuery, `__cls` pre-set in constructor
- C++ source: `app/mailsync/MailSync/Models/File.cpp` — JSON keys, columnsForQuery, embedded in Message.files
- C++ source: `app/mailsync/MailSync/Models/Identity.cpp` — NOT stored in DB; assert(false) on table/query methods
- TypeScript source: `app/frontend/flux/models/model.ts` — base Model attributes with jsonKey values
- TypeScript source: `app/frontend/flux/models/message.ts` — all Message attribute jsonKeys
- TypeScript source: `app/frontend/flux/models/thread.ts` — all Thread attribute jsonKeys (note: no `lmt` attribute)
- TypeScript source: `app/frontend/flux/models/category.ts` — Folder/Label base attributes
- TypeScript source: `app/frontend/flux/models/contact.ts` — Contact attribute jsonKeys (`h`, `s`, `gis`)
- TypeScript source: `app/frontend/flux/models/calendar.ts` — Calendar attribute jsonKeys
- TypeScript source: `app/frontend/flux/models/event.ts` — Event attribute jsonKeys (`cid`, `ics`, `icsuid`, `rid`, `rs`, `re`)
- TypeScript source: `app/frontend/flux/models/file.ts` — File attribute jsonKeys
- TypeScript source: `app/frontend/flux/models/contact-book.ts` — ContactBook attributes (mismatch from C++)
- TypeScript source: `app/frontend/flux/models/contact-group.ts` — ContactGroup attributes (minimal)
- TypeScript source: `app/frontend/flux/models/model-with-metadata.ts` — metadata jsonKey = `metadata`, pluginMetadata join table

### Secondary (MEDIUM confidence)

- [rusqlite serde_json.rs source](https://github.com/rusqlite/rusqlite/blob/master/src/types/serde_json.rs) — ToSql/FromSql for serde_json::Value: NULL->NULL, JSON object/array->TEXT, numbers->INT/REAL
- [rusqlite_migration README](https://github.com/cljoly/rusqlite_migration/blob/master/README.md) — Migration pattern with WAL pragma before to_latest()

### Tertiary (LOW confidence)

- WebSearch results on delta coalescing patterns with tokio — no authoritative single source; pattern derived from C++ source analysis

### Round 2 Sources (HIGH confidence)

- [tokio-rusqlite 0.7.0 feature flags](https://docs.rs/crate/tokio-rusqlite/0.7.0/features) — bundled feature confirmed opt-in; 42 rusqlite features re-exported
- [rusqlite 0.38.0 docs.rs](https://docs.rs/crate/rusqlite/latest) — latest version 0.38.0 (2025-12-20), bundled SQLite 3.51.1
- [rusqlite_migration 2.4.1 docs.rs](https://docs.rs/crate/rusqlite_migration/latest) — requires rusqlite ^0.38.0; VERSION CONFLICT with tokio-rusqlite 0.7.0; use 2.3.x instead
- [rusqlite_migration changelog](https://cj.rs/rusqlite_migration_docs/changelog/) — confirmed 2.3.0 = rusqlite 0.37, 2.4.0 = rusqlite 0.38; tokio feature removed in 2.0.0
- [M struct docs](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/struct.M.html) — M::up(sql: &str) API; empty string is valid no-op
- C++ source (round 2): Thread.cpp — afterSave() full SQL for ThreadCategory/ThreadCounts/ThreadSearch; afterRemove() delegation
- C++ source (round 2): Contact.cpp — afterSave() ContactSearch INSERT/UPDATE; source != mail guard; searchContent() format
- C++ source (round 2): Event.cpp — afterSave() EventSearch with _searchTitle guard; transient search fields from ICS only
- C++ source (round 2): ContactGroup.cpp — afterRemove() ContactContactGroup cleanup; syncMembers() full SQL
- C++ source (round 2): Folder.cpp — beforeSave() ThreadCounts INSERT OR IGNORE (v==1); afterRemove() DELETE
- C++ source (round 2): Message.cpp — afterSave() thread propagation; afterRemove() MessageBody cleanup; _skipThreadUpdatesAfterSave
- C++ source (round 2): MailModel.cpp — afterSave() ModelPluginMetadata DELETE+INSERT with expiration; beforeSave() DetatchedPluginMetadata attach
- C++ source (round 2): Account.cpp — confirms Account is NOT a DB model (all methods assert(false))
- C++ source (round 2): MailStore.cpp — save()/remove() codepath; transaction flow; _emit(); globalLabelsVersion; key-value store; DetatchedPluginMetadata CRUD; unsafeEraseTransactionDeltas
- C++ source (round 2): MailStoreTransaction.cpp — RAII wrapper; 80ms slow transaction warning; noexcept rollback
- C++ source (round 2): constants.h ACCOUNT_RESET_QUERIES — complete table list confirming all auxiliary table roles

---

## Metadata

**Confidence breakdown:**
- Standard stack (tokio-rusqlite, rusqlite_migration, serde): HIGH — CORRECTED: must use rusqlite_migration 2.3.x (not 2.4.x); rusqlite ^0.37 vs ^0.38 conflict confirmed via docs.rs
- Architecture (fat row pattern, delta coalescing): HIGH — directly derived from C++ source code which is the authoritative reference
- Schema (V1-V9 SQL): HIGH — verbatim from constants.h in the repository; extracted character-for-character
- Model field mapping: HIGH — directly read from each model's .cpp file
- Delta delay values: HIGH — confirmed by grep of all setStreamDelay() callers (SyncWorker.cpp=500ms, main.cpp=5ms, DeltaStream.cpp=0ms)
- V5 gap: HIGH — confirmed by reading MailStore::migrate() line-by-line; no if(version<5) block exists
- TypeScript cross-check: HIGH — read directly from all model .ts files; mismatches documented
- afterSave/afterRemove side effects: HIGH — exact SQL extracted from Thread.cpp, Message.cpp, Contact.cpp, Event.cpp, ContactGroup.cpp, Folder.cpp, MailModel.cpp
- Auxiliary tables: HIGH — purpose and SQL verified from MailStore.cpp + constants.h ACCOUNT_RESET_QUERIES
- MailStore codepath: HIGH — traced line-by-line from MailStore.cpp and MailStoreTransaction.cpp
- Library version conflict: HIGH — verified via docs.rs and rusqlite_migration changelog

**Research date:** 2026-03-02
**Valid until:** 2026-06-01 (tokio-rusqlite and rusqlite APIs are stable; schema derived from committed C++ source)

---

## Deep Dive: afterSave/afterRemove Side Effects (Round 2)

Full SQL and logic for every model's lifecycle hooks. This section complements the field mapping above with the exact database side effects that must be replicated in Rust.

### MailModel::afterSave() — Base Class (ModelPluginMetadata Maintenance)

Source: `MailModel.cpp` lines 151-211

Runs for ANY model where `supportsMetadata()` returns true (Message and Thread). Guards on whether metadata plugin IDs have changed since the model was loaded (`_initialMetadataPluginIds != metadataPluginIds`).

```sql
-- Step 1: Delete all existing metadata join rows for this model id
DELETE FROM ModelPluginMetadata WHERE id = ?;
-- [bind: model.id()]

-- Step 2: Re-insert one row per non-empty metadata entry
-- (metadata entries with empty value are skipped, which effectively "removes" a plugin
--  metadata entry while keeping the version counter incrementing)
INSERT INTO ModelPluginMetadata (id, accountId, objectType, value, expiration)
VALUES (?, ?, ?, ?, ?);
-- [bind: model.id(), model.accountId(), model.tableName(), pluginId, expiration_or_null]
-- repeated once per metadata entry where value.size() > 0
```

**Expiration handling:** If any metadata entry has `value["expiration"]` as a number, that unix timestamp is bound to the `expiration` column. Otherwise NULL is bound. The lowest expiration across all entries is also reported to `MetadataExpirationWorker` via `isSavingMetadataWithExpiration()`.

**`beforeSave()` in MailModel:** If `version() == 1` AND `supportsMetadata()`, it calls `store->findAndDeleteDetachedPluginMetadata(accountId, id)` and applies any waiting metadata via `upsertMetadata()`. This attaches "detatched" metadata that arrived before the model existed.

### MailModel::afterRemove() — Base Class

```sql
DELETE FROM ModelPluginMetadata WHERE id = ?;
-- [bind: model.id()]
```

Only runs for models where `supportsMetadata()` is true.

---

### Thread::afterSave()

Source: `Thread.cpp` lines 348-415

Three conditional side effects triggered by changes detected against `_initialCategoryIds`, `_initialLMRT`, and `_initialLMST` (captured in `captureInitialState()` on load):

**Side Effect 1: ThreadCategory join table** (triggers if categories OR timestamps changed)

```sql
-- Delete all category rows for this thread
DELETE FROM ThreadCategory WHERE id = ?;
-- [bind: thread.id()]

-- Re-insert one row per category (folder or label)
INSERT INTO ThreadCategory (id, value, inAllMail, unread, lastMessageReceivedTimestamp, lastMessageSentTimestamp)
VALUES (?, ?, ?, ?, ?, ?);
-- [bind: thread.id(), categoryId, inAllMail, unread_bool, lmrt, lmst]
-- repeated once per entry in captureCategoryIDs() result
```

**How categories are built — `captureCategoryIDs()`:**
Returns a `map<string, bool>` where key = category id (folder id or label id), value = whether category has unread (`_u > 0`). Iterates `folders()` and `labels()` arrays in the Thread JSON. Each folder/label entry has `_refs` (reference count from messages) and `_u` (unread count).

**Side Effect 2: ThreadCounts table** (triggers only if categories changed, NOT just timestamps)

Computes a diff map from `_initialCategoryIds` vs new `categoryIds`. For each categoryId with a non-zero diff:

```sql
UPDATE ThreadCounts SET unread = unread + ?, total = total + ? WHERE categoryId = ?;
-- [bind: unread_delta, total_delta, categoryId]
-- unread_delta: new_unread - old_unread (negative if unread removed)
-- total_delta: +1 if newly in category, -1 if removed, 0 if still in category
```

**Side Effect 3: ThreadSearch FTS5 update** (triggers only if categories changed AND thread is search-indexed)

```sql
UPDATE ThreadSearch SET categories = ? WHERE rowid = ?;
-- [bind: categoriesSearchString(), thread.searchRowId()]
```

`categoriesSearchString()` builds a space-separated string of folder/label roles or paths (role preferred over path).

### Thread::afterRemove()

Source: `Thread.cpp` lines 418-432

Calls `afterSave(this)` first (which clears ThreadCategory and adjusts ThreadCounts based on now-empty state), then:

```sql
DELETE FROM ThreadSearch WHERE rowid = ?;
-- [bind: thread.searchRowId()]
-- Only runs if searchRowId() > 0
```

**Critical:** `afterRemove` calls `afterSave` with the thread in its current (now-empty) state. Since the thread is being removed and all messages should already be removed (zeroing out folders/labels), this effectively clears the ThreadCategory entries and decrements ThreadCounts to zero for all categories.

---

### Thread::beforeSave() — NONE

Thread has no `beforeSave()` override. Only `afterSave()` and `afterRemove()`.

---

### Message::afterSave()

Source: `Message.cpp` lines 449-469

Propagates message attribute changes to the parent Thread. Skips if `_skipThreadUpdatesAfterSave` is true or `threadId()` is empty.

```
1. Find Thread: store->find<Thread>(Query().equal("id", threadId()))
   -- SELECT ... FROM Thread WHERE id = ?
2. Get all labels cache: store->allLabelsCache(accountId())
   -- uses _labelCache (invalidated on globalLabelsVersion change)
3. Call thread->applyMessageAttributeChanges(_lastSnapshot, this, allLabels)
   -- updates thread->folders(), labels(), unread, starred, attachmentCount, timestamps
4. store->save(thread.get())
   -- triggers Thread::afterSave() which maintains ThreadCategory + ThreadCounts
5. _lastSnapshot = getSnapshot()
   -- capture new state for next diff
```

**`_skipThreadUpdatesAfterSave`:** Set to true in `ThreadUtils::BuildAndSaveThread()` when bulk-rebuilding thread state from scratch. Prevents N^2 thread updates when adding N messages at once.

### Message::afterRemove()

Source: `Message.cpp` lines 471-497

```
1. Find parent Thread by threadId()
2. Call thread->applyMessageAttributeChanges(_lastSnapshot, nullptr, allLabels)
   -- nullptr means "this message is gone": decrements all its contributions
3. If thread->folders().size() == 0:
   store->remove(thread)         -- thread has no messages left, delete it
   else:
   store->save(thread.get())     -- thread still has other messages, update it
4. DELETE FROM MessageBody WHERE id = ?
   -- [bind: message.id()]
   -- cleanup draft body on draft delete
```

---

### Contact::afterSave()

Source: `Contact.cpp` lines 174-188

Two branches based on version number:

```sql
-- Branch 1: New contact (version == 1) — INSERT into FTS5
INSERT INTO ContactSearch (content_id, content) VALUES (?, ?);
-- [bind: contact.id(), contact.searchContent()]

-- Branch 2: Updated non-mail contact (version > 1, source != 'mail') — UPDATE FTS5
UPDATE ContactSearch SET content = ? WHERE content_id = ?;
-- [bind: contact.searchContent(), contact.id()]
```

**When is ContactSearch skipped?** When `version() > 1` AND `source() == 'mail'`. Mail-sourced contacts (seen in email headers) are only indexed once on first save and never updated. Only contacts from CardDAV or Google People get updated FTS5 entries.

**`searchContent()` string format:** The `@` in the email address is replaced with a space so both parts are separately tokenizable, then a space and the display name are appended. Example: `"user@example.com"` with name `"Alice"` becomes `"user example.com Alice"`.

### Contact::afterRemove()

```sql
DELETE FROM ContactSearch WHERE content_id = ?;
-- [bind: contact.id()]
```

---

### Event::afterSave()

Source: `Event.cpp` lines 197-224

**Guard:** Skips entirely if `_searchTitle`, `_searchDescription`, `_searchLocation`, and `_searchParticipants` are all empty. These transient fields are only populated by `applyICSEventData()` (called when constructing from live ICS data). Events loaded from the DB or constructed from client JSON have empty search fields and do NOT update EventSearch.

```sql
-- New event (version == 1)
INSERT INTO EventSearch (content_id, title, description, location, participants)
VALUES (?, ?, ?, ?, ?);
-- [bind: event.id(), _searchTitle, _searchDescription, _searchLocation, _searchParticipants]

-- Updated event (version > 1)
UPDATE EventSearch SET title = ?, description = ?, location = ?, participants = ?
WHERE content_id = ?;
-- [bind: _searchTitle, _searchDescription, _searchLocation, _searchParticipants, event.id()]
```

**`_searchParticipants` format:** Space-joined attendee strings from `ICalendarEvent.Attendees`.

### Event::afterRemove()

```sql
DELETE FROM EventSearch WHERE content_id = ?;
-- [bind: event.id()]
```

---

### ContactGroup::afterRemove()

Source: `ContactGroup.cpp` lines 75-82

```sql
DELETE FROM ContactContactGroup WHERE value = ?;
-- [bind: contactGroup.id()]
```

Note: In the ContactContactGroup table, `id` = contact id, `value` = group id. So this deletes all rows where a contact was a member of this group (removes the group from all contacts).

### ContactGroup::syncMembers() — Full Group Membership Maintenance

Source: `ContactGroup.cpp` lines 94-139

This is the full group-sync operation (not a lifecycle hook, but triggered during CardDAV sync):

```sql
-- 1. Read existing members
SELECT id FROM ContactContactGroup WHERE value = ?;
-- [bind: groupId]

-- 2. Delete all join rows for this group
DELETE FROM ContactContactGroup WHERE value = ?;
-- [bind: groupId]

-- 3. Re-insert new join rows
INSERT OR IGNORE INTO ContactContactGroup (id, value) VALUES (?, ?);
-- [bind: contactId, groupId]
-- repeated per new member

-- 4. Update affected Contact models (add/remove groupId from contact._data["gis"])
-- This triggers store->save(contact) which in turn:
--   a. increments contact.version
--   b. runs Contact::afterSave() (updates ContactSearch)
--   c. emits a persist delta for the contact
```

---

### Folder::beforeSave() — ThreadCounts Creation

Source: `Folder.cpp` lines 70-79

```sql
-- On first save (version == 1): ensure ThreadCounts row exists
INSERT OR IGNORE INTO ThreadCounts (categoryId, unread, total) VALUES (?, 0, 0);
-- [bind: folder.id()]
```

Label inherits this behavior since Label extends Folder (Label.cpp has no beforeSave override).

### Folder::afterRemove() — ThreadCounts Cleanup

Source: `Folder.cpp` lines 81-87

```sql
DELETE FROM ThreadCounts WHERE categoryId = ?;
-- [bind: folder.id()]
```

Label inherits this behavior.

---

### Summary: Rust Implementation Requirements for Lifecycle Hooks

| Model | Hook | Required SQL |
|-------|------|-------------|
| MailModel (base) | `after_save` | DELETE + INSERT ModelPluginMetadata (metadata-supporting models only) |
| MailModel (base) | `before_save` | findAndDelete DetatchedPluginMetadata + upsertMetadata (metadata-supporting models, version==1 only) |
| MailModel (base) | `after_remove` | DELETE ModelPluginMetadata (metadata-supporting models only) |
| Thread | `after_save` | DELETE + INSERT ThreadCategory; UPDATE ThreadCounts (diff); UPDATE ThreadSearch categories |
| Thread | `after_remove` | Calls after_save (clears ThreadCategory/ThreadCounts), then DELETE ThreadSearch row |
| Message | `after_save` | Find Thread, applyMessageAttributeChanges, store.save(thread) |
| Message | `after_remove` | Find Thread, applyMessageAttributeChanges(None), remove or save thread; DELETE MessageBody |
| Contact | `after_save` | INSERT or UPDATE ContactSearch |
| Contact | `after_remove` | DELETE ContactSearch |
| Event | `after_save` | INSERT or UPDATE EventSearch (only when search fields populated from ICS) |
| Event | `after_remove` | DELETE EventSearch |
| ContactGroup | `after_remove` | DELETE ContactContactGroup WHERE value = groupId |
| Folder | `before_save` | INSERT OR IGNORE ThreadCounts (version==1 only) |
| Folder | `after_remove` | DELETE ThreadCounts WHERE categoryId = id |
| Label | (inherits Folder) | Same as Folder |

---

## Deep Dive: Auxiliary Tables

These tables exist in the V1 schema but are NOT among the 13 "fat row" data models. They are maintained by side effects in model lifecycle hooks or by dedicated store methods.

### Account Table

**Schema (from V1):**
```sql
CREATE TABLE IF NOT EXISTS `Account` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), email_address TEXT);
```

**Status:** Account is NOT a stored data model. `Account.cpp` has `tableName()`, `columnsForQuery()`, and `bindToQuery()` all calling `assert(false)` — they are never called. The Account table in the schema exists as a legacy artifact. The actual Account object is passed to the mailsync process at startup via stdin JSON (two-line handshake: identity JSON then account JSON), parsed once, and held in memory as a global.

`ACCOUNT_RESET_QUERIES` includes `DELETE FROM Account WHERE id = ?` — so the table is cleared on account reset. This implies Account was saved to the DB historically, or this row is written by the Electron side, not the sync engine.

**Rust implementation:** Do NOT implement Account as a MailModel. Parse from stdin JSON into an `Account` struct. The Account table row (if it exists) is written by the Electron main process, not the sync engine.

---

### MessageBody Table

**Schema (from V1 + V3):**
```sql
CREATE TABLE IF NOT EXISTS `MessageBody` (id VARCHAR(40) PRIMARY KEY, `value` TEXT);
CREATE UNIQUE INDEX IF NOT EXISTS MessageBodyIndex ON MessageBody(id);
-- V3 adds:
ALTER TABLE `MessageBody` ADD COLUMN fetchedAt DATETIME;
```

**Purpose:** Stores full email body HTML/text content separately from the Message table. Message rows store only headers and metadata in the `data` column. Bodies are fetched lazily on demand.

**How it is used:**
- `Message::toJSONDispatch()` adds `body` and `fullSyncComplete` to the delta JSON only when `_bodyForDispatch` is set (the fetched body is attached in memory during the body fetch phase)
- `Message::afterRemove()` deletes the MessageBody row: `DELETE FROM MessageBody WHERE id = ?`
- The `need-bodies` stdin command triggers a priority fetch for specific message ids
- Body lookup: `SELECT value FROM MessageBody WHERE id = ?`
- Body insert: `INSERT OR REPLACE INTO MessageBody (id, value, fetchedAt) VALUES (?, ?, datetime('now'))`

**Rust implementation:** MessageBody is a separate table with no model struct. Access it directly via SQL in the body fetch worker (Phase 7). The `fetchedAt` column is used for the 3-month age policy (older bodies are evicted to save disk space).

---

### _State Table

**Schema (from V1):**
```sql
CREATE TABLE IF NOT EXISTS `_State` (id VARCHAR(40) PRIMARY KEY, value TEXT);
```

**Purpose:** A simple key-value store for sync state. Used by `MailStore::getKeyValue()` and `MailStore::saveKeyValue()`.

**Read/Write SQL:**
```sql
-- Read
SELECT value FROM _State WHERE id = ?;

-- Write (upsert)
REPLACE INTO _State (id, value) VALUES (?, ?);
```

**Known key patterns (from C++ source):**

| Key Pattern | Value | Used By |
|-------------|-------|---------|
| `"VACUUM_TIME"` | Unix timestamp string | `MailStore::migrate()` — 14-day VACUUM timer |
| `"cursor-{accountId}"` | Cursor string | `MetadataWorker` — long-polling cursor for metadata server; reset to "0" on account reset |

**Folder sync state:** Note that folder sync state (uidvalidity, highestmodseq, etc.) is NOT stored in `_State`. It is stored in `Folder._data["localStatus"]` JSON blob, written via `store->saveFolderStatus()` which does a merge-save of the Folder model.

**Rust implementation:** Implement `get_key_value(key: &str) -> Option<String>` and `save_key_value(key: &str, value: &str)` methods on `MailStore`. Both are simple CRUD against `_State`. No model struct needed.

---

### ThreadReference Table

**Schema (from V1):**
```sql
CREATE TABLE IF NOT EXISTS ThreadReference (
    threadId VARCHAR(42),
    accountId VARCHAR(8),
    headerMessageId VARCHAR(255),
    PRIMARY KEY (threadId, accountId, headerMessageId)
);
```

**Purpose:** Maps email header message IDs to thread IDs. Used to associate replies with existing threads via the `In-Reply-To` and `References` headers. When a new message arrives, its `headerMessageId` and `replyToHeaderMessageId` are looked up in this table to find the existing thread to attach to.

**How it is maintained:** Written by the IMAP sync code (MailProcessor / SyncWorker) when building and updating threads. Not maintained via model lifecycle hooks — managed directly by ThreadUtils.

`ACCOUNT_RESET_QUERIES` includes `DELETE FROM ThreadReference WHERE accountId = ?` — cleared on account reset.

**Rust implementation:** ThreadReference is not a model. Implement as direct SQL in the thread-building logic (Phase 7). The Phase 6 requirement is only to create the table schema, not the thread-building logic.

---

### DetatchedPluginMetadata Table

**Schema (from V1):**
```sql
CREATE TABLE IF NOT EXISTS `DetatchedPluginMetadata` (
    objectId VARCHAR(40),
    objectType VARCHAR(15),
    accountId VARCHAR(8),
    pluginId VARCHAR(40),
    value BLOB,
    version INTEGER,
    PRIMARY KEY (`objectId`, `accountId`, `pluginId`)
);
```

**Note on spelling:** The C++ codebase consistently spells this "Detatched" (with two t's) rather than "Detached". The Rust implementation must use the same spelling in the table name to remain compatible with existing databases.

**Purpose:** Stores plugin metadata that arrived from the metadata server BEFORE the target model (Message or Thread) was synced to the local DB. When the target model is eventually synced and saved for the first time (version==1), `MailModel::beforeSave()` calls `findAndDeleteDetachedPluginMetadata()` to retrieve and attach any waiting metadata.

**Distinct from ModelPluginMetadata:** `ModelPluginMetadata` is the join table for metadata already attached to existing models. `DetatchedPluginMetadata` is the "waiting room" for metadata whose target model has not arrived yet.

**Read/Write SQL (from MailStore.cpp):**
```sql
-- Read and delete all detatched metadata for a given objectId
SELECT version, value, pluginId, objectType
FROM DetatchedPluginMetadata WHERE objectId = ? AND accountId = ?;

DELETE FROM DetatchedPluginMetadata WHERE objectId = ? AND accountId = ?;

-- Write (upsert) — used by MetadataWorker when model does not exist yet
REPLACE INTO DetatchedPluginMetadata (objectId, objectType, accountId, pluginId, value, version)
VALUES (?, ?, ?, ?, ?, ?);
```

**Rust implementation:** Implement `find_and_delete_detached_plugin_metadata()` and `save_detached_plugin_metadata()` on `MailStore`. Used by Phase 9 MetadataWorker. In Phase 6, ensure the table is created by the V1 schema migration.

---

### ContactContactGroup Table

**Schema (from V8):**
```sql
CREATE TABLE `ContactContactGroup` (`id` varchar(40), `value` varchar(40), PRIMARY KEY (id, value));
```

**Purpose:** Many-to-many join table between Contact and ContactGroup. `id` is the contact id; `value` is the group id.

**Note on naming:** The column naming is counter-intuitive. `id` = contact id, `value` = group id. This mirrors the C++ join table pattern used for ThreadCategory (where `id` = thread id, `value` = category id).

**How it is maintained:**
- `ContactGroup::syncMembers()` — full replace: DELETE all + INSERT new for a group
- `ContactGroup::afterRemove()` — DELETE all entries WHERE value = groupId (when a group is deleted)
- Read pattern: `SELECT id FROM ContactContactGroup WHERE value = ?` (get all contact ids in a group)

**Rust implementation:** No model struct. Maintained directly by `ContactGroup` sync logic (Phase 9). In Phase 6, ensure the table is created in the V8 migration block.

---

## Deep Dive: MailStore Save/Remove Full Flow

Complete codepath traced from `MailStore.cpp` and `MailStoreTransaction.cpp`.

### save() Codepath

Source: `MailStore.cpp` lines 372-430

```
MailStore::save(model):
1. assertCorrectThread()
   -- Guards: only the owning thread may call save()
   -- Rust equivalent: enforced by tokio-rusqlite single background thread

2. model->incrementVersion()
   -- _data["v"] += 1
   -- version 0 -> 1 means INSERT; version > 1 means UPDATE

3. model->beforeSave(this)
   -- MailModel::beforeSave(): if version==1 AND supportsMetadata():
   --   findAndDeleteDetachedPluginMetadata() then upsertMetadata()
   -- Folder::beforeSave(): if version==1: INSERT OR IGNORE ThreadCounts row

4. Build and cache prepared statement by tableName:
   -- version > 1:
   --   "UPDATE {tableName} SET col1=:col1, col2=:col2, ... WHERE id=:id"
   --   Cached in _saveUpdateQueries[tableName]
   -- version == 1:
   --   "INSERT INTO {tableName} (col1, col2, ...) VALUES (:col1, :col2, ...)"
   --   Cached in _saveInsertQueries[tableName]
   -- Column list comes from model->columnsForQuery()

5. query->reset() + query->clearBindings()
   -- Resets the cached prepared statement for reuse

6. model->bindToQuery(query.get())
   -- Binds all column values (id, data JSON, accountId, version, indexed fields)
   -- MailModel::bindToQuery() is called by all subclasses first

7. query->exec()
   -- Executes INSERT or UPDATE

8. model->afterSave(this)
   -- All side effects: ThreadCategory, ThreadCounts, ThreadSearch, FTS5, join tables

9. if (tableName == "Label") globalLabelsVersion += 1
   -- Atomic counter invalidates allLabelsCache() on next call

10. DeltaStreamItem delta {DELTA_TYPE_PERSIST, model}
    -- type: "persist", modelClass: tableName, modelJSONs: [model.toJSONDispatch()]

11. _emit(delta)
    -- If _transactionOpen: append to _transactionDeltas
    -- Else: SharedDeltaStream()->emit(delta, _streamMaxDelay)
```

### remove() Codepath

Source: `MailStore.cpp` lines 454-474

```
MailStore::remove(model):
1. assertCorrectThread()

2. Build and cache prepared statement:
   -- "DELETE FROM {tableName} WHERE id = ?"
   -- Cached in _removeQueries[tableName]

3. query->reset() + clearBindings() + bind(model.id()) + exec()
   -- Executes DELETE

4. model->afterRemove(this)
   -- All cleanup side effects (FTS5 tables, join tables, ThreadCounts)
   -- afterRemove does NOT decrement version; model is already deleted

5. if (tableName == "Label") globalLabelsVersion += 1

6. DeltaStreamItem delta {DELTA_TYPE_UNPERSIST, model}
   -- type: "unpersist", modelClass: tableName, modelJSONs: [model.toJSONDispatch()]
   -- For unpersist, Electron only needs id and __cls to remove from its cache

7. _emit(delta)
```

### Transaction Flow

Source: `MailStore.cpp` lines 308-370, `MailStoreTransaction.cpp`

```
beginTransaction():
1. "BEGIN IMMEDIATE TRANSACTION"
   -- IMMEDIATE acquires write lock immediately, preventing SQLITE_BUSY from concurrent writers
2. _transactionOpen = true

rollbackTransaction():
1. Clear _saveUpdateQueries, _saveInsertQueries, _removeQueries
   -- Discards cached prepared statements that may reference uncommitted state
2. "ROLLBACK"
3. _transactionOpen = false

commitTransaction():
1. "COMMIT"
2. if (_transactionDeltas.size() > 0):
   SharedDeltaStream()->emit(_transactionDeltas, _streamMaxDelay)
   _transactionDeltas = {}
3. _transactionOpen = false
```

**MailStoreTransaction RAII wrapper (MailStoreTransaction.cpp):**
```
Constructor: store->beginTransaction()
commit():    store->commitTransaction(); mCommited = true
Destructor:  if (!mCommited) store->rollbackTransaction()
             (noexcept — SQLite exceptions swallowed in destructor)
```

**Slow transaction warning:** MailStoreTransaction::commit() logs a warning if the transaction takes >80ms.

### _emit() — Delta Routing

```
_emit(delta):
  if _transactionOpen:
    _transactionDeltas.push_back(delta)  -- accumulate during transaction
  else:
    SharedDeltaStream()->emit(delta, _streamMaxDelay)  -- flush to coalescing buffer
```

### unsafeEraseTransactionDeltas()

A special escape hatch. Calling `store->unsafeEraseTransactionDeltas()` drops all accumulated transaction deltas. Used where internal-only DB changes (e.g., updating sync state) must not notify the Electron UI. C++ comment: "If you KNOW the transaction is only changing internal data, you can safely do this."

**Rust equivalent:** Implement as `mail_store.erase_transaction_deltas()` which clears the accumulated delta Vec.

### globalLabelsVersion Atomic

Source: `MailStore.cpp` line 25

```cpp
std::atomic<int> globalLabelsVersion {1};
```

Incremented every time a Label is saved or removed. `allLabelsCache()` compares the stored `_labelCacheVersion` against `globalLabelsVersion` to detect staleness. When stale, it re-fetches all labels via `findAll<Label>()`.

**Rust equivalent:** Use `std::sync::atomic::AtomicI32` as a process-level global (or `Arc<AtomicI32>` passed into `MailStore`). Since all access is on the same tokio-rusqlite background thread, mutation never races.

### Statement Caching

The C++ MailStore caches prepared statements in three maps:
- `_saveUpdateQueries`: one UPDATE statement per table name
- `_saveInsertQueries`: one INSERT statement per table name
- `_removeQueries`: one DELETE statement per table name

Each cached statement is `reset()` and `clearBindings()` before reuse. **Rust equivalent:** Use `rusqlite::Connection::prepare_cached()` inside the `call()` closure. This is the Connection's built-in LRU statement cache. Since all calls go through the single tokio-rusqlite background thread, the cache is never accessed concurrently.

### saveFolderStatus() — Special Case

`saveFolderStatus(folder, initialStatus)` updates only the `localStatus` sub-object within a Folder's `data` JSON, doing a merge rather than a full replace:
1. Checks if `localStatus` actually changed (avoids unnecessary saves)
2. Opens a nested `MailStoreTransaction`
3. Re-fetches the folder from DB
4. Merges the changed keys from `changedStatus` into the DB version
5. Calls `save(current.get())`
6. Commits

This avoids overwriting IMAP sync state that may have been updated by another operation between when the folder was loaded and when this call runs.

---

## Deep Dive: Library Version Verification (Round 2)

Verified via docs.rs and cljoly/rusqlite_migration changelog as of 2026-03-02.

### tokio-rusqlite

| Property | Verified Value | Source |
|----------|---------------|--------|
| Latest version | 0.7.0 (released 2025-11-16) | docs.rs/crate/tokio-rusqlite/latest |
| rusqlite dependency | `^0.37.0` | docs.rs/crate/tokio-rusqlite/latest |
| `bundled` feature | Present, NOT enabled by default | docs.rs/crate/tokio-rusqlite/0.7.0/features |
| Feature count | 42 total, 0 enabled by default | docs.rs feature list |
| Feature behavior | Re-exports ALL 42 rusqlite feature flags | WebSearch: tokio-rusqlite 0.7.0 changelog |

**Confirmed:** The `bundled` feature in 0.7.0 is opt-in (not the default). The existing Standard Stack section correctly documents this breaking change.

### rusqlite

| Property | Verified Value | Source |
|----------|---------------|--------|
| Latest version | 0.38.0 (released 2025-12-20) | docs.rs/crate/rusqlite/latest |
| Bundled SQLite version | 3.51.1 | docs.rs/crate/rusqlite/latest |
| Minimum SQLite (system) | 3.34.1 | docs.rs/crate/rusqlite/latest |
| tokio-rusqlite 0.7.0 pins | `^0.37.0` — NOT 0.38.0 | tokio-rusqlite 0.7.0 crate metadata |

### rusqlite_migration

| Property | Verified Value | Source |
|----------|---------------|--------|
| Latest version | 2.4.1 (released 2026-01-25) | docs.rs/crate/rusqlite_migration/latest |
| rusqlite dependency in 2.4.x | `^0.38.0` | docs.rs/crate/rusqlite_migration/latest |
| rusqlite dependency in 2.3.x | `^0.37.0` | cljoly changelog: "2.3.0: rusqlite updated from 0.36 to 0.37" |
| tokio-rusqlite integration | Removed in 2.0.0 | cljoly changelog |
| `M::up("")` empty string | Accepted — runs as no-op SQL batch | M struct docs, rusqlite execute_batch("") behavior |
| user_version tracking | Uses PRAGMA user_version | docs.rs |

### CRITICAL VERSION CONFLICT — Standard Stack Correction Required

**rusqlite_migration 2.4.x requires `rusqlite ^0.38.0`. tokio-rusqlite 0.7.0 requires `rusqlite ^0.37.0`. These are incompatible when both are listed as direct dependencies in Cargo.toml.**

Cargo resolves semver by finding a version satisfying ALL constraints. `^0.37.0` allows 0.37.x only; `^0.38.0` allows 0.38.x only. These ranges do not overlap — Cargo will refuse to build.

**The existing Standard Stack section incorrectly lists `rusqlite_migration = "2.4"`. This must be changed to `rusqlite_migration = "2.3"`.**

**Resolution: Use rusqlite_migration 2.3.x**

rusqlite_migration 2.3.0 updated its rusqlite dependency from 0.36 to 0.37, making it the last 2.x version compatible with tokio-rusqlite 0.7.0's rusqlite ^0.37.0 pin.

**Corrected Cargo.toml (replaces the Standard Stack installation block):**

```toml
[dependencies]
# tokio-rusqlite 0.7.0 pins rusqlite ^0.37.0
tokio-rusqlite = { version = "0.7", features = ["bundled", "serde_json"] }
# rusqlite_migration 2.3.x is the last version compatible with rusqlite 0.37.x
# DO NOT use 2.4.x — it requires rusqlite ^0.38.0 which conflicts with tokio-rusqlite 0.7.0
rusqlite_migration = "2.3"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
```

**Note:** If tokio-rusqlite releases 0.8.0 with rusqlite ^0.38.0, upgrade both crates together. As of 2026-03-02, 0.8.0 is not available.

### tokio-rusqlite + rusqlite_migration Async Integration

tokio-rusqlite and rusqlite_migration do NOT share an async integration. rusqlite_migration 2.0.0 removed its `alpha-async-tokio-rusqlite` feature entirely. The recommended pattern (from rusqlite_migration maintainer) is to run synchronous migrations inside a `tokio_rusqlite::Connection::call()` closure — which is exactly the pattern documented in the existing Code Examples section. CONFIRMED correct.

### M::up("") Empty String Behavior

`M::up(sql: &'u str)` accepts any `&str`. The implementation calls `db.execute_batch(sql)`. Calling `execute_batch("")` on rusqlite with an empty SQL string is a valid no-op — SQLite's C API `sqlite3_exec("")` succeeds immediately. The existing V5 no-op `M::up("")` recommendation is confirmed correct.

---

## Deep Dive: Electron Delta Parsing (Round 3)

Complete investigation of how the TypeScript side receives, parses, and dispatches stdout JSON deltas from the C++ (and future Rust) mailsync process.

### Sources Read

- `app/frontend/mailsync-process.ts` — MailsyncProcess class, `sync()` method, stdout buffering
- `app/frontend/flux/mailsync-bridge.ts` — MailsyncBridge class, `_onIncomingMessages`, `_onFetchBodies`
- `app/frontend/flux/stores/database-change-record.ts` — DatabaseChangeRecord class
- `app/frontend/flux/stores/database-store.ts` — DatabaseStore, `trigger()` method, read-only SQLite

### stdout Stream Parsing — Exact Protocol

Source: `mailsync-process.ts` `sync()` method (lines 316-393)

The stdout parser uses a string-split approach — NOT a readline stream:

```typescript
let outBuffer = '';

this._proc.stdout.on('data', data => {
    const added = data.toString();
    outBuffer += added;

    if (added.indexOf('\n') !== -1) {
        const msgs = outBuffer.split('\n');
        outBuffer = msgs.pop(); // retain partial line for next event
        this.emit('deltas', msgs);
    }
});
```

**Exact parsing contract:**
1. **Delimiter:** Single newline `\n`. Each JSON object is terminated by exactly one `\n`.
2. **Buffering:** Accumulates across Node.js `data` events. Splits only when current chunk contains at least one `\n`.
3. **Partial lines:** Last array element after split (partial line without `\n`) retained in `outBuffer`.
4. **stdin highWaterMark:** Set to 1MB (`1024 * 1024`) to handle large payloads (HTML draft bodies).
5. **Exit handling:** On process close, remaining `outBuffer` parsed as JSON. Lines starting with `dbg::` skipped. If the final JSON has `error`, it becomes an Error; otherwise emitted as final delta.

### _onIncomingMessages — Delta Dispatch Logic

Source: `mailsync-bridge.ts` `_onIncomingMessages` (lines 416-472)

The handler receives an array of complete JSON strings (one per completed line):

**Routing table (evaluated in order):**

| Condition | Action |
|-----------|--------|
| `msg.length === 0` | Skip silently |
| `msg[0] !== '{'` | Log warning, skip |
| JSON.parse fails | Log warning, skip |
| `type.endsWith('-result')` OR `type === 'folder-status'` | Route to `_responseEmitter` (request/response protocol) |
| Any of `type`, `modelJSONs`, `modelClass` is missing | Log warning, skip |
| `modelClass === 'ProcessState'` | Route to `OnlineStatusStore` (not a DB delta) |
| `modelClass === 'ProcessAccountSecretsUpdated'` | Route to `KeyManager` (not a DB delta) |
| All above pass | Normal model delta: IPC broadcast + DatabaseChangeRecord |

**CRITICAL:** ALL THREE fields (`type`, `modelJSONs`, `modelClass`) must be present or the message is silently discarded.

### Required JSON Wire Format Fields

From line 434: `const { type, modelJSONs, modelClass } = json;`

| Field | Type | Required | Valid Values |
|-------|------|----------|-------------|
| `type` | string | YES | `"persist"`, `"unpersist"`, `"metadata-expiration"` |
| `modelClass` | string | YES | C++ tableName: `"Message"`, `"Thread"`, `"Folder"`, etc. |
| `modelJSONs` | array | YES | Array of model JSON objects |

**Third delta type found (Round 3):** `"metadata-expiration"` is defined as `DELTA_TYPE_METADATA_EXPIRATION` in `DeltaStream.hpp`. Used by `MetadataExpirationWorker`. The TypeScript bridge has no special handling — it passes through as a standard model delta.

**Non-delta message formats (also accepted over stdout):**

| `type` value | JSON shape | Handler |
|--------------|-----------|---------|
| ends with `-result` | `{type, requestId?, ...}` | `_responseEmitter` |
| `"folder-status"` | `{type, requestId?, ...}` | `_responseEmitter` |
| (modelClass=ProcessState) | `{type, modelClass, modelJSONs:[{accountId,id,connectionError}]}` | `OnlineStatusStore` |
| (modelClass=ProcessAccountSecretsUpdated) | `{type, modelClass, modelJSONs:[accountJSON]}` | `KeyManager` |

### DatabaseChangeRecord — Exact Shape

Source: `database-change-record.ts` (complete 29-line file)

```typescript
class DatabaseChangeRecord<T extends Model> {
    objects: T[];                           // deserialized TypeScript Model instances
    objectsRawJSON: Record<string, any>[];  // raw JSON array from delta
    type: string;                           // 'persist' or 'unpersist'
    objectClass: any;                       // modelClass string from delta

    constructor({ type, objectClass, objects, objectsRawJSON }) { /* assigns fields */ }
}
```

**Field mapping (delta JSON -> DatabaseChangeRecord):**

| Delta field | DatabaseChangeRecord field |
|------------|--------------------------|
| `type` | `type` |
| `modelClass` | `objectClass` |
| `modelJSONs` (after `Utils.convertToModel`) | `objects` |
| `modelJSONs` (raw) | `objectsRawJSON` |

### DatabaseStore — Read-Only SQLite

Source: `database-store.ts` lines 51-73

```typescript
// Electron opens the database read-only
const db = new Sqlite3(dbPath, { readonly: true, timeout: 10000 });
db.pragma('journal_mode = WAL');
db.pragma('main.page_size = 8192');   // different from C++ 4096 — page_size set at creation, C++ value wins
db.pragma('main.cache_size = 20000'); // reader-side cache only
db.pragma('main.synchronous = NORMAL');
```

All writes come exclusively from the C++/Rust mailsync process. The Electron side is purely a consumer.

### Multi-Window Rebroadcast

After dispatching locally, the main window sends the raw JSON string to other windows:
```typescript
ipcRenderer.send('mailsync-bridge-rebroadcast-to-all', msg); // raw JSON string
```
Other windows parse it and call `DatabaseStore.trigger()` on their own instance. The Rust wire format must be parseable by any window.

### stdin Wire Format (Electron -> C++/Rust)

Source: `mailsync-process.ts` `sendMessage(json)`:
```typescript
const msg = `${JSON.stringify(json)}\n`;
this._proc.stdin.write(msg, 'utf-8');
```

Both directions (stdin and stdout) use newline-delimited JSON (`\n` terminator). The C++ reads stdin via `getline(cin, buffer)` in `DeltaStream::waitForJSON()`.

---

## Deep Dive: MailStore Query/Read Patterns (Round 3)

Complete investigation of the C++ MailStore read API from `MailStore.hpp` and `MailStore.cpp`.

### Core Invariant: All Model Reads Use the `data` Column

**All model deserialization uses `SELECT data FROM {Table} WHERE ...`.** Indexed columns (unread, starred, threadId, etc.) appear only in WHERE/ORDER BY — they are never deserialized back to model objects. Model construction always comes from the `data` TEXT/BLOB column.

**Exception — scan-only queries (no model construction):**
- `fetchMessagesAttributesInRange` — reads `id, unread, starred, remoteUID, remoteXGMLabels` directly
- `fetchMessageUIDAtDepth` — reads `remoteUID` only
- `syncMessageBodies` — reads `id, remoteUID` only

### find<T>(query) — Single Result

```cpp
// SQL: SELECT data FROM {T::TABLE_NAME} {query.getSQL()} LIMIT 1
// Returns: shared_ptr<T> or nullptr
```

**Rust equivalent:**
```rust
let result = db.query_row(
    "SELECT data FROM Message WHERE id = ?1 LIMIT 1",
    [&id],
    |row| row.get::<_, String>(0)
).optional()?;
let model: Option<Message> = result
    .map(|s| serde_json::from_str(&s))
    .transpose()?;
```

### findAll<T>(query) — Multiple Results

```cpp
// SQL: SELECT data FROM {T::TABLE_NAME} {query.getSQL()} [LIMIT N]
// Returns: vector<shared_ptr<T>>
```

**Rust equivalent:**
```rust
let mut stmt = db.prepare_cached(
    "SELECT data FROM Thread WHERE accountId = ?1 ORDER BY lastMessageReceivedTimestamp DESC LIMIT ?2"
)?;
let results: Vec<Thread> = stmt
    .query_map([account_id, limit], |row| row.get::<_, String>(0))?
    .filter_map(|r| r.ok())
    .filter_map(|s| serde_json::from_str(&s).ok())
    .collect();
```

### findAllMap<T>(query, keyField) — Map by String Key Column

```cpp
// SQL: SELECT {keyField}, data FROM {T::TABLE_NAME} {query.getSQL()}
// Returns: map<string, shared_ptr<T>>
```

Used when the caller needs O(1) lookup by a key column after fetching.

### findAllUINTMap<T>(query, keyField) — Map by u32 Key Column

```cpp
// SQL: SELECT {keyField}, data FROM {T::TABLE_NAME} {query.getSQL()}
// Returns: map<uint32_t, shared_ptr<T>>
```

Used for IMAP UID-keyed maps during incremental sync.

### findLargeSet<T>(colname, set) — Chunked IN Query

```cpp
// Splits set into chunks of 900 (SQLite SQLITE_MAX_VARIABLE_NUMBER = 999 default)
// Calls findAll<T>(Query().equal(colname, chunk)) for each chunk
```

**CRITICAL:** SQLite rejects IN queries with more than 999 bound parameters by default. The C++ uses chunks of 900 as a safety margin. **Rust implementations doing large IN queries must also chunk at 900 or fewer.**

### findGeneric / findAllGeneric — Runtime Type Dispatch

Non-template methods dispatching to `find<Message>`, `find<Thread>`, or `find<Contact>` based on a lowercase string. Only 3 types are supported. Used by TaskProcessor where model type is known only at runtime.

### WAL Mode and Concurrent Read/Write

WAL mode allows multiple simultaneous Electron readers + one Rust writer concurrently.

**Connection strategy:**
- **Rust sync engine:** Single `tokio_rusqlite::Connection` for all reads AND writes (single background thread).
- **Electron:** Separate `better-sqlite3` connection, read-only, in each renderer process.
- SQLite WAL provides the isolation between the two processes automatically.

No separate reader connections in the C++ or Rust code. The C++ uses one `SQLite::Database` for everything on the owning thread.

### Statement Caching for Reads

The C++ does NOT cache read query statements. Only INSERT/UPDATE/DELETE use statement caches.

**Rust equivalent:** For hot-path reads (e.g., find-by-id called thousands of times during sync), use `db.prepare_cached(sql)` inside `call()` closures. For one-off reads, inline statement creation is acceptable.

### Key Read-Only Queries (Special Cases — Phase 7 Concern)

```sql
-- fetchMessagesAttributesInRange (IMAP incremental sync — performance critical)
SELECT id, unread, starred, remoteUID, remoteXGMLabels
FROM Message
WHERE accountId = ? AND remoteFolderId = ? AND remoteUID >= ? AND remoteUID <= ?

-- fetchMessageUIDAtDepth (finding sync boundary UID)
SELECT remoteUID FROM Message
WHERE accountId = ? AND remoteFolderId = ? AND remoteUID < ?
ORDER BY remoteUID DESC LIMIT 1 OFFSET ?

-- countBodiesDownloaded (folder sync progress)
SELECT COUNT(Message.id) FROM Message
INNER JOIN MessageBody ON MessageBody.id = Message.id
WHERE MessageBody.value IS NOT NULL AND Message.remoteFolderId = ?

-- syncMessageBodies — find messages needing bodies (auto-fetch)
SELECT Message.id, Message.remoteUID FROM Message
LEFT JOIN MessageBody ON MessageBody.id = Message.id
WHERE Message.accountId = ? AND Message.remoteFolderId = ?
  AND (Message.date > ? OR Message.draft = 1)
  AND Message.remoteUID > 0 AND MessageBody.id IS NULL
ORDER BY Message.date DESC LIMIT 30
```

These are Phase 7 (IMAP sync) concerns. In Phase 6, ensure the required indexes exist in the schema to support them.

---

## Deep Dive: Error Handling and Recovery (Round 3)

### Save Failure Behavior

Source: `MailStore.cpp` lines 372-430

The C++ `save()` throws `SQLite::Exception` on any SQL failure. There is **no return-code error handling** — exceptions are the only error propagation mechanism.

```cpp
query->exec();           // throws SQLite::Exception on SQLITE_CONSTRAINT, SQLITE_BUSY, etc.
model->afterSave(this);  // throws if any side-effect SQL fails
```

Exception handling happens at the transaction boundary:
```cpp
{
    MailStoreTransaction transaction(store, "operationName");
    store->save(model1);  // throws -> propagates out of block
    store->save(model2);
    transaction.commit();
}
// RAII destructor calls rollbackTransaction() if commit() was not reached
```

**File save exceptions in body fetch are swallowed locally:**
```cpp
// MailProcessor.cpp lines 377-383:
try {
    store->save(&file);
} catch (SQLite::Exception &) {
    logger->warn("Unable to insert file ID {} - it must already exist.", file.id());
}
```

**Rust equivalent:** `save()` returns `Result<(), MailStoreError>`. The `?` operator propagates. RAII `MailStoreTransaction` struct's `Drop` implementation calls `rollback_transaction()` if `committed` flag not set.

### Transaction Rollback Behavior

Source: `MailStore.cpp` lines 323-340

`rollbackTransaction()` does three things:
1. **Clears ALL cached prepared statement maps:** `_saveUpdateQueries`, `_saveInsertQueries`, `_removeQueries` all reset to empty. Prevents reuse of statements in inconsistent post-rollback state.
2. **Executes ROLLBACK SQL.**
3. **Sets `_transactionOpen = false`.**

**Accumulated deltas on rollback:** `_transactionDeltas` is NOT explicitly cleared in `rollbackTransaction()`. They are abandoned — never emitted because `commitTransaction()` is never called.

**Rust equivalent:** Clear `transaction_deltas` Vec explicitly in rollback (for safety):
```rust
fn rollback_transaction(&mut self) -> Result<()> {
    self.transaction_deltas.clear(); // explicit: don't emit abandoned deltas
    self.transaction_open = false;
    // execute ROLLBACK
    Ok(())
}
```

### MailStoreTransaction RAII — Rust Equivalent

Source: `MailStoreTransaction.hpp/.cpp`

```rust
pub struct MailStoreTransaction<'a> {
    store: &'a mut MailStore,
    committed: bool,
    started: std::time::Instant,
    name_hint: &'static str,
}

impl<'a> MailStoreTransaction<'a> {
    pub fn new(store: &'a mut MailStore, name_hint: &'static str) -> Result<Self> {
        store.begin_transaction()?;
        Ok(Self { store, committed: false, started: std::time::Instant::now(), name_hint })
    }
    pub fn commit(mut self) -> Result<()> {
        self.store.commit_transaction()?;
        self.committed = true;
        if self.started.elapsed().as_millis() > 80 {
            tracing::warn!("[SLOW] Transaction={} > 80ms", self.name_hint);
        }
        Ok(())
    }
}

impl<'a> Drop for MailStoreTransaction<'a> {
    fn drop(&mut self) {
        if !self.committed {
            let _ = self.store.rollback_transaction(); // noexcept equivalent
        }
    }
}
```

### Error Propagation to Electron (ProcessState Deltas)

Source: `DeltaStream.cpp` lines 205-225

There is NO error delta type. Errors reach Electron via `ProcessState` deltas emitted at 0ms (immediate):

```json
{"type":"persist","modelClass":"ProcessState","modelJSONs":[{"accountId":"x","id":"x","connectionError":true}]}
```

Cleared on recovery:
```json
{"type":"persist","modelClass":"ProcessState","modelJSONs":[{"accountId":"x","id":"x","connectionError":false}]}
```

These bypass the 500ms coalescing channel — emitted directly with 0ms delay.

**TypeScript handling (mailsync-bridge.ts line 450):**
```typescript
if (modelClass === 'ProcessState' && modelJSONs.length) {
    OnlineStatusStore.onSyncProcessStateReceived(modelJSONs[0]);
    continue; // does NOT create a DatabaseChangeRecord
}
```

**Fatal errors:** C++ calls `abort()`. Process exits non-zero. `CrashTracker` records crash. After 5 crashes in 5 minutes, account is marked `SYNC_STATE_ERROR` — worker not relaunched.

### Crash Recovery

**WAL crash recovery:** SQLite WAL is fully self-recovering on next open:
- Committed WAL transactions are checkpointed into the main database file.
- Uncommitted WAL transactions are discarded.
- No explicit checkpoint code needed in the Rust binary.

**Task crash recovery:** `runListenOnMainThread()` calls `processor.cleanupTasksAfterLaunch()` at startup. Handles Tasks in `status = 'remote'` (local phase done, remote phase interrupted). Behavior is task-type-specific — Phase 8 concern.

**Sync state recovery:** Folder sync state (`uidvalidity`, `highestmodseq`) stored in `Folder._data["localStatus"]` — committed JSON blob. IMAP sync resumes from last committed state.

### Error Handling Summary for Rust

| Scenario | C++ Behavior | Rust Implementation |
|----------|-------------|---------------------|
| SQL constraint violation | Throws `SQLite::Exception` | `Err(MailStoreError::SqlError(e))` |
| Exception in transaction | RAII destructor calls rollback | `Drop` calls `rollback_transaction()` |
| Deltas on rollback | Abandoned (not emitted) | Clear `transaction_deltas` Vec |
| IMAP connection failure | `beginConnectionError()` + retry | Emit ProcessState delta at 0ms + retry |
| Non-retryable error | `abort()` | `std::process::exit(1)` |
| WAL crash recovery | Automatic | Automatic — no code needed |
| Tasks in 'remote' state | `cleanupTasksAfterLaunch()` | Phase 8 implementation |

---

## Deep Dive: End-to-End need-bodies Flow (Round 3)

### Step 1: Electron Sends need-bodies (TypeScript)

Source: `mailsync-bridge.ts` `_onFetchBodies()` lines 508-517

Triggered by `Actions.fetchBodies(messages)`:

```typescript
_onFetchBodies(messages) {
    const byAccountId = {};
    for (const msg of messages) {
        byAccountId[msg.accountId] = byAccountId[msg.accountId] || [];
        byAccountId[msg.accountId].push(msg.id);
    }
    for (const accountId of Object.keys(byAccountId)) {
        this.sendMessageToAccount(accountId, { type: 'need-bodies', ids: byAccountId[accountId] });
    }
}
```

**stdin wire format:**
```json
{"type":"need-bodies","ids":["msg-id-1","msg-id-2","msg-id-3"]}
```

Written as `JSON.stringify(json) + '\n'` to process stdin.

### Step 2: C++ Binary Dispatches need-bodies

Source: `main.cpp` lines 703-711

```cpp
if (type == "need-bodies") {
    vector<string> ids{};
    for (auto id : packet["ids"]) {
        ids.push_back(id.get<string>());
    }
    if (fgWorker) fgWorker->idleQueueBodiesToSync(ids); // mutex-protected handoff
    if (fgWorker) fgWorker->idleInterrupt(); // wake from IMAP IDLE immediately
}
```

Runs on the main thread (stdin loop). `idleQueueBodiesToSync` safely passes IDs to the foreground thread via mutex.

### Step 3: Foreground Worker Fetches from IMAP

Source: `SyncWorker.cpp` lines 83-127

```cpp
void SyncWorker::idleQueueBodiesToSync(vector<string> & ids) {
    std::unique_lock<std::mutex> lck(idleMtx);
    for (string & id : ids) { idleFetchBodyIDs.push_back(id); }
}

// idleCycleIteration() pops from idleFetchBodyIDs:
auto msg = store->find<Message>(Query().equal("id", id));
// SELECT data FROM Message WHERE id = ?  LIMIT 1
syncMessageBody(msg.get());
// IMAP FETCH {uid} (BODY[]) via mailcore2 -> MessageParser
processor->retrievedMessageBody(message, messageParser);
```

### Step 4: retrievedMessageBody() Stores the Body

Source: `MailProcessor.cpp` lines 283-424

Inside a `MailStoreTransaction`:

```sql
-- 1. Store body in MessageBody table
REPLACE INTO MessageBody (id, value, fetchedAt) VALUES (?, ?, datetime('now'))

-- 2. File attachments (ignore duplicate key exceptions)
-- INSERT INTO File ... (via store->save(&file), SQLite::Exception caught locally)

-- 3. Update ThreadSearch FTS5 body text
UPDATE ThreadSearch SET body = ? WHERE rowid = ?

-- 4. Update Message (snippet, plaintext flag, files, extra headers)
-- UPDATE Message SET data=..., snippet=..., etc. (via store->save(message))
```

Before calling `store->save(message)`:
```cpp
message->setSnippet(text->substringToIndex(400));  // stored in data column
message->setPlaintext(bodyIsPlaintext);             // stored in data column
message->setBodyForDispatch(bodyRepresentation);    // IN MEMORY ONLY — NOT in data column
message->setFiles(files);                           // updates data["files"] array
```

### Step 5: Message Delta Contains the Body

Source: `Message.cpp` lines 499-505 — `toJSONDispatch()`

```cpp
json Message::toJSONDispatch() {
    json j = toJSON();  // standard data JSON — body NOT included
    if (_bodyForDispatch.length() > 0) {
        j["body"] = _bodyForDispatch;    // added only when body is fetched
        j["fullSyncComplete"] = true;    // signals body completeness to Electron
    }
    return j;
}
```

**Delta wire format when body is included:**
```json
{
    "type": "persist",
    "modelClass": "Message",
    "modelJSONs": [{
        "id": "msg-abc",
        "v": 3,
        "snippet": "First 400 chars of body...",
        "body": "<html>...</html>",
        "fullSyncComplete": true,
        "__cls": "Message"
    }]
}
```

**CRITICAL DISTINCTION:**
- `data` TEXT column in Message table: Contains `toJSON()` output — **no `body` field**.
- Delta JSON to Electron: Contains `toJSONDispatch()` output — **has `body` when `_bodyForDispatch` is set**.
- `MessageBody` table: Body stored separately for direct Electron DB reads.

### Step 6: Electron Receives Delta and Renders Body

The delta flows through: `_onIncomingMessages` -> `DatabaseChangeRecord` -> `DatabaseStore.trigger()` -> all subscribers notified -> message thread view re-renders with body content.

### Age Policy: fetchedAt and 3-Month Body Retention

Source: `SyncWorker.cpp` lines 986-988, 971-976

**maxAgeForBodySync:**
```cpp
time_t SyncWorker::maxAgeForBodySync(Folder & folder) {
    return 24 * 60 * 60 * 30 * 3; // 3 months = 7,776,000 seconds
}
```

**Body eviction SQL (folder cleanup):**
```sql
DELETE FROM MessageBody
WHERE MessageBody.fetchedAt < datetime('now', '-14 days')
  AND MessageBody.id IN (
      SELECT Message.id FROM Message
      WHERE Message.remoteFolderId = ?
        AND Message.draft = 0
        AND Message.date < ?  -- bind: unix_time - 7,776,000
  )
```

**Both conditions required for eviction:**
1. Body fetched more than 14 days ago (prevents evicting recently-viewed bodies).
2. Message itself older than 3 months.

**`fetchedAt` purpose:** Set to `datetime('now')` on INSERT. Used only for eviction. Never NULL in current databases (V3 migration backfilled all nulls).

### Placeholder Pattern — Preventing Duplicate Fetch

Source: `SyncWorker.cpp` lines 1039-1065

Before IMAP fetch, a NULL placeholder is inserted:

```sql
INSERT OR IGNORE INTO MessageBody (id, value) VALUES (?, ?)
-- bind: message.id(), NULL
```

**Purpose:** The auto-fetch query uses `WHERE MessageBody.id IS NULL` — a placeholder row (with non-NULL id) prevents re-scheduling the same fetch. Only an explicit `need-bodies` command bypasses this.

**On crash mid-fetch:** Placeholder exists but value is NULL. Auto-fetch skips this message. Only explicit `need-bodies` re-queues it.

### Complete Flow Diagram

```
Electron: Actions.fetchBodies([msg])
  -> MailsyncBridge._onFetchBodies()
  -> stdin: {"type":"need-bodies","ids":["msg-id"]}\n

C++ main thread (stdin loop via DeltaStream::waitForJSON()):
  -> type == 'need-bodies'
  -> fgWorker->idleQueueBodiesToSync(ids)  [mutex handoff to foreground thread]
  -> fgWorker->idleInterrupt()  [interrupts IMAP IDLE]

C++ foreground thread (wakes from IMAP IDLE):
  -> idleCycleIteration(): drains idleFetchBodyIDs queue
  -> store->find<Message>(id)  [SELECT data FROM Message WHERE id=? LIMIT 1]
  -> syncMessageBody(msg)
  -> IMAP FETCH {uid} (BODY[])  [network call]
  -> MessageParser::messageParserWithData(rawData)
  -> processor->retrievedMessageBody(message, parser)

MailProcessor::retrievedMessageBody():
  -> render HTML via tidy + mailcore2
  -> BEGIN IMMEDIATE TRANSACTION
  -> REPLACE INTO MessageBody (id, value, fetchedAt) VALUES (?, ?, datetime('now'))
  -> INSERT File rows [SQLite::Exception caught locally for duplicates]
  -> UPDATE ThreadSearch body text [FTS5]
  -> message->setBodyForDispatch(html)  [in-memory only — NOT saved to data col]
  -> store->save(message)
     -> UPDATE Message SET data=... [toJSON() — no body field in data col]
     -> afterSave() -> Thread update
     -> toJSONDispatch() -> includes body:"<html>..." [body added to dispatch JSON]
     -> DeltaStreamItem{persist, Message, json-with-body}
     -> _transactionDeltas.push_back(delta)
  -> COMMIT -> SharedDeltaStream()->emit(deltas, 500ms)

DeltaStream flush thread (500ms later):
  -> cout << delta.dump() + "\n" << flush

Electron stdout reader (mailsync-process.ts):
  -> 'data' event -> split('\n') -> emit('deltas', complete_msgs)

MailsyncBridge._onIncomingMessages:
  -> JSON.parse -> {type:'persist', modelClass:'Message', modelJSONs:[{...body...}]}
  -> Utils.convertToModel -> Message instance with .body = '<html>...'
  -> DatabaseChangeRecord{type:'persist', objectClass:'Message', objects:[msg]}
  -> DatabaseStore.trigger(record)
  -> UI subscribers notified -> message thread view renders body
```

### Rust Implementation Requirements for need-bodies

| Concern | Phase | Notes |
|---------|-------|-------|
| `need-bodies` stdin dispatch | Phase 5 (stdin loop) | Parse `packet["ids"]` array |
| IMAP FETCH BODY[] | Phase 7 (IMAP sync) | Via IMAP library |
| `REPLACE INTO MessageBody (id, value, fetchedAt)` | Phase 7 body fetch | Direct SQL — NOT a MailModel |
| `body_for_dispatch: Option<String>` | Phase 6 Message struct | Transient field, not serialized to data col |
| `to_json_dispatch()` includes body | Phase 6 Message impl | Conditional: include body when `is_some()` |
| `store.save(message)` emits delta with body | Phase 6 MailStore | Standard save path; dispatch JSON includes body |
| Age eviction SQL | Phase 7 sync cleanup | Exact SQL from SyncWorker.cpp lines 972-975 |
| Placeholder pattern | Phase 7 syncMessageBodies | `INSERT OR IGNORE INTO MessageBody (id, value) VALUES (?1, NULL)` |

---

## Metadata (Updated Round 3)

**Confidence breakdown (updated):**
- Standard stack (tokio-rusqlite, rusqlite_migration, serde): HIGH — versions verified via docs.rs
- Architecture (fat row pattern, delta coalescing): HIGH — derived from C++ source
- Schema (V1-V9 SQL): HIGH — verbatim from constants.h
- Model field mapping: HIGH — all .cpp files read directly
- Delta delay values: HIGH — grep of all setStreamDelay() callers confirmed
- V5 gap: HIGH — confirmed from MailStore::migrate() line-by-line
- TypeScript cross-check: HIGH — all .ts model files read
- afterSave/afterRemove side effects: HIGH — exact SQL from all model files
- Auxiliary tables: HIGH — verified from MailStore.cpp + constants.h
- MailStore codepath: HIGH — traced line-by-line
- Library version conflict: HIGH — verified via docs.rs and changelog
- **Electron delta parsing (Round 3): HIGH** — mailsync-process.ts and mailsync-bridge.ts read directly; exact routing logic documented
- **MailStore read API (Round 3): HIGH** — MailStore.hpp read directly; all template methods documented; chunk-at-900 pattern confirmed
- **Error handling and recovery (Round 3): HIGH** — MailStore.cpp, MailStoreTransaction.cpp, DeltaStream.cpp, main.cpp all read directly
- **need-bodies flow (Round 3): HIGH** — complete end-to-end trace from all 6 relevant source files

**Research date:** 2026-03-03 (updated with Round 4 deep dives)
**Valid until:** 2026-06-01 (stable APIs; protocol and schema from committed C++ source)

### Round 4 Deep Dive Files (2026-03-03)

- **`06-DEEP-DIVE-THREAD-MAINTENANCE.md`** — Complete `Thread::applyMessageAttributeChanges()` algorithm (5-phase diff/patch), `MessageSnapshot`, `captureInitialState()`, `ThreadCounts` diff, `categoriesSearchString()`, label resolution, participant merge, full rebalance pattern, `_skipThreadUpdatesAfterSave`, `allLabelsCache()`
- **`06-DEEP-DIVE-QUERY-AND-TOJSON.md`** — Query builder class (can be replaced with raw SQL in Rust), `toJSON()` lazy `__cls` injection, `toJSONDispatch()` Message-only override (body/fullSyncComplete/headersSyncComplete), nested object serialization (contacts, embedded folders/labels with `_refs`/`_u`, files), model construction from DB, `find<T>`/`findAll<T>`/`findLargeSet<T>` internals, `saveFolderStatus()` merge pattern, `getKeyValue`/`saveKeyValue`
