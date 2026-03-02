# Phase 6: SQLite Layer and Model Infrastructure - Research

**Researched:** 2026-03-02 (updated 2026-03-02 with deep dive)
**Domain:** Rust async SQLite (tokio-rusqlite), data model serialization, FTS5 schema migration, delta coalescing
**Confidence:** HIGH (standard stack verified via official docs/crates.io; schema verified against C++ source; all open questions resolved from source code)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DATA-01 | SQLite database with WAL mode and `busy_timeout=5000` via tokio-rusqlite | tokio-rusqlite 0.7.0 `call()` closure + rusqlite `busy_timeout()` + `pragma_update("journal_mode", "WAL")` |
| DATA-02 | Delta emission with persist/unpersist types, 500ms coalescing window, transaction batching | `tokio::sync::mpsc` + `tokio::time::sleep` + HashMap coalescing per model class; mirrors C++ `DeltaStream` buffering; 500ms confirmed from SyncWorker.cpp |
| DATA-03 | All 13 data models implemented: Message, Thread, Folder, Label, Contact, ContactBook, ContactGroup, Calendar, Event, Task, File, Identity, ModelPluginMetadata | "Fat row" pattern: `data TEXT` JSON column + indexed columns; serde_json; full field mapping documented in Deep Dive section |
| DATA-04 | Schema migration matching C++ baseline (all tables, indexes, FTS5 for ThreadSearch/EventSearch/ContactSearch) | rusqlite_migration 2.4.x with `M::up()` for V1–V9 SQL from constants.h; exact SQL documented in Deep Dive section; V5 no-op confirmed required |
| DATA-05 | Single-writer pattern via tokio-rusqlite prevents blocking on async threads | Single `tokio_rusqlite::Connection` in a Mutex or Arc; all writes via `.call()` closure sent to dedicated background thread |
</phase_requirements>

---

## Summary

Phase 6 builds the complete MailStore Rust equivalent: a tokio-async SQLite layer that matches the C++ baseline schema exactly, serializes and deserializes all 13 data model types using a "fat row" pattern, emits deltas to the stdout writer task via a coalescing 500ms window, and enforces the single-writer constraint through tokio-rusqlite's background thread architecture.

The C++ `MailStore` uses a "fat row" design: every model table has a `data TEXT` column containing the full JSON blob, plus a small number of indexed projection columns (e.g., `unread`, `starred`, `threadId`) for query performance. The Rust implementation must replicate this exactly — the Electron TypeScript side reads both the `data` JSON and those indexed columns. The schema is fully defined in `constants.h` across V1–V9 migration blocks; all SQL must be reproduced verbatim.

The delta system mirrors the C++ `DeltaStream` singleton: saves and removes accumulate `DeltaStreamItem` values keyed by model class, same-object saves merge into a single entry (last-write wins with key-merge), and the buffer is flushed after a 500ms coalescing window. In Rust, this is a tokio task that receives `DeltaStreamItem` via mpsc channel, batches by model class in a `HashMap`, and flushes after a `tokio::time::sleep(500ms)` timer resets on each new arrival.

**Primary recommendation:** Use `tokio-rusqlite 0.7.0` (with rusqlite 0.37.0 dependency) plus `rusqlite_migration 2.4.x` for schema management. Store all model JSON in a `data TEXT` column serialized with `serde_json`. Implement the delta coalescing task using `tokio::sync::mpsc` + `tokio::time::sleep` — do NOT use an external debounce crate (the per-class key-merge logic requires a bespoke HashMap approach).

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| tokio-rusqlite | 0.7.0 | Async SQLite access with single-writer background thread | Mandated by STATE.md: prevents tokio thread starvation from blocking rusqlite calls |
| rusqlite | 0.37.0 | Underlying SQLite bindings (re-exported by tokio-rusqlite) | tokio-rusqlite 0.7.0 pins rusqlite ^0.37.0; use feature `bundled,serde_json` |
| serde | 1.x | Serialize/Deserialize derives for model structs | Universal Rust serialization framework |
| serde_json | 1.x | JSON serialization for the `data TEXT` column and delta output | Needed for fat-row JSON storage and delta JSON emission |
| rusqlite_migration | 2.4.x | Schema migration with `user_version` PRAGMA tracking | Manages V1-V9 migration chain; avoids hand-rolling migration state |

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
tokio-rusqlite = { version = "0.7", features = ["bundled", "serde_json"] }
rusqlite_migration = "2.4"
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

**What:** Use `rusqlite_migration` 2.4.x with `M::up()` entries mirroring the C++ V1–V9 migration blocks. Migration runs once on database open via `to_latest()`. The `user_version` PRAGMA is managed automatically by the library.

```rust
// Source: rusqlite_migration 2.4.x docs + constants.h V1-V9 blocks
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
| Schema migration with user_version tracking | Custom PRAGMA user_version + SQL runner | rusqlite_migration 2.4.x | Handles V1-V9 chain, user_version PRAGMA tracking, and M::up() batches correctly |
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
| Manual PRAGMA user_version for migrations | rusqlite_migration with M::up() | 2022+ | Cleaner migration chain, no boilerplate |
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
- [rusqlite_migration docs.rs](https://docs.rs/rusqlite_migration/latest/rusqlite_migration/) — version 2.4.1, M::up(), Migrations::to_latest(), user_version tracking
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

---

## Metadata

**Confidence breakdown:**
- Standard stack (tokio-rusqlite, rusqlite_migration, serde): HIGH — versions confirmed from official docs and GitHub releases as of 2026-03-02
- Architecture (fat row pattern, delta coalescing): HIGH — directly derived from C++ source code which is the authoritative reference
- Schema (V1-V9 SQL): HIGH — verbatim from constants.h in the repository; extracted character-for-character
- Model field mapping: HIGH — directly read from each model's .cpp file
- Delta delay values: HIGH — confirmed by grep of all setStreamDelay() callers (SyncWorker.cpp=500ms, main.cpp=5ms, DeltaStream.cpp=0ms)
- V5 gap: HIGH — confirmed by reading MailStore::migrate() line-by-line; no if(version<5) block exists
- TypeScript cross-check: HIGH — read directly from all model .ts files; mismatches documented

**Research date:** 2026-03-02
**Valid until:** 2026-06-01 (tokio-rusqlite and rusqlite APIs are stable; schema derived from committed C++ source)
