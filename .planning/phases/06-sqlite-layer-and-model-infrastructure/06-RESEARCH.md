# Phase 6: SQLite Layer and Model Infrastructure - Research

**Researched:** 2026-03-02
**Domain:** Rust async SQLite (tokio-rusqlite), data model serialization, FTS5 schema migration, delta coalescing
**Confidence:** HIGH (standard stack verified via official docs/crates.io; schema verified against C++ source)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| DATA-01 | SQLite database with WAL mode and `busy_timeout=5000` via tokio-rusqlite | tokio-rusqlite 0.7.0 `call()` closure + rusqlite `busy_timeout()` + `pragma_update("journal_mode", "WAL")` |
| DATA-02 | Delta emission with persist/unpersist types, 500ms coalescing window, transaction batching | `tokio::sync::mpsc` + `tokio::time::sleep` + HashMap coalescing per model class; mirrors C++ `DeltaStream` buffering |
| DATA-03 | All 13 data models implemented: Message, Thread, Folder, Label, Contact, ContactBook, ContactGroup, Calendar, Event, Task, File, Identity, ModelPluginMetadata | "Fat row" pattern: `data TEXT` JSON column + indexed columns; serde_json with rusqlite's `serde_json` feature |
| DATA-04 | Schema migration matching C++ baseline (all tables, indexes, FTS5 for ThreadSearch/EventSearch/ContactSearch) | rusqlite_migration 2.4.x with `M::up()` for all V1–V9 SQL from constants.h; FTS5 requires `bundled` feature |
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

**Critical serde rename rules (C++ JSON key → Rust field):**
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
        // V5: (no V5 in C++, skip to V6)
        // V6: Event table rebuilt
        M::up("DROP TABLE IF EXISTS Event; CREATE TABLE IF NOT EXISTS Event ..."),
        // V7-V9: additional columns and indexes
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

**Critical:** The C++ schema jumps from V4 to V6 — there is no V5 migration block. The `rusqlite_migration` index is 0-based, but the C++ `user_version` counts from 1. The migration array must contain 9 entries (indices 0-8) matching C++ versions 1-9. V5 was apparently removed; insert a no-op `M::up("")` or handle the gap carefully to keep user_version in sync. Verify against the C++ code before finalizing.

### Anti-Patterns to Avoid

- **Calling rusqlite directly on async tasks:** Any `rusqlite::Connection` method call outside a `tokio_rusqlite::Connection::call()` closure will block the tokio thread pool thread, causing starvation. This is the exact problem tokio-rusqlite solves.
- **Creating multiple `tokio_rusqlite::Connection` instances for the same file:** Multiple writers create SQLITE_BUSY risk even in WAL mode. Use a single `Connection` for writes; read-only connections for Electron's side are separate.
- **Storing model JSON in the `data` column without all indexed column projections:** The TypeScript DatabaseStore queries use the indexed columns (e.g., `WHERE unread = 1`). Both must be updated atomically.
- **Emitting one delta per save without coalescing:** The C++ system explicitly coalesces to prevent "thrashing on the JS side." High-frequency saves (e.g., syncing 1000 messages) must batch through the coalescing window.
- **Using `#[serde(flatten)]` for the `data` column:** The entire model struct is serialized as one JSON blob to a TEXT column. Do NOT use `serde(flatten)` — serialize the whole struct with `serde_json::to_string()`.
- **Missing `__cls` key in dispatch JSON:** The C++ `MailModel::toJSON()` adds `_data["__cls"] = tableName()`. The Rust model's `to_json_dispatch()` must also inject `"__cls"` into the JSON. Electron uses it for dispatching.

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
**Why it happens:** V5 was apparently removed from the C++ codebase. The user_version jumps from 4 to 6 implicitly (V6 block is guarded by `if (version < 6)`).
**How to avoid:** Include a no-op migration at index 4 (version 5) in the Migrations array: `M::up("")`. Verify against the C++ `CURRENT_VERSION = 9` constant and the `if (version < N)` guards in `MailStore::migrate()`.
**Warning signs:** Rust binary run against an existing C++ database produces migration errors or re-runs V6 destructive DROP.

### Pitfall 4: JSON Key Mismatch Between Rust Struct and TypeScript Parser

**What goes wrong:** TypeScript `message.headerMessageId` is undefined because the Rust struct serialized it as `header_message_id` instead of `hMsgId`.
**Why it happens:** Rust's default serde serialization uses snake_case. The C++ code uses camelCase and abbreviated keys (`hMsgId`, `rthMsgId`, `aid`, `v`). Without `#[serde(rename = "...")]` on every field, the JSON keys will not match.
**How to avoid:** Cross-reference every field with the C++ `_data["key"]` assignments in each model's `.cpp` file. Map TypeScript `Attributes.String({ jsonKey: 'hMsgId' })` entries to verify.
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

## Open Questions

1. **V5 Migration No-Op Strategy**
   - What we know: C++ has V1, V2, V3, V4, V6, V7, V8, V9 — no V5.
   - What's unclear: Does `rusqlite_migration` handle a gap gracefully, or must we include a no-op M::up("") at index 4 to prevent user_version misalignment when opening existing C++ databases?
   - Recommendation: Include `M::up("")` at index 4. Verify with a test: open a C++ database at user_version=9, run Rust migrations, confirm user_version remains 9 and no migration re-runs.

2. **`prepare_cached` vs `prepare` Inside `.call()`**
   - What we know: The C++ uses a map of pre-compiled statements per table. `prepare_cached` uses an LRU cache on the `Connection`, but the tokio-rusqlite background thread holds the connection — cache is preserved between calls.
   - What's unclear: Under high concurrency (many `.call()` invocations), does `prepare_cached`'s LRU evict statements needed by concurrent in-flight closures? There are no concurrent closures (single thread), so this should be safe.
   - Recommendation: Use `prepare_cached` for all hot paths (save, find). Confirm no LRU eviction issues by checking rusqlite's `CachedStatement` lifetime docs.

3. **Delta Immediate vs Coalesced Emission**
   - What we know: The C++ uses `maxDeliveryDelay = 0` for `ProcessState` and `ProcessAccountSecretsUpdated` deltas (immediate flush). For normal model saves, delay is `_streamMaxDelay` (default 500ms).
   - What's unclear: The `_streamMaxDelay` field is set by `setStreamDelay()` — what value does the C++ code actually set? The constant `500` does not appear in `DeltaStream.cpp`; it's passed from callers.
   - Recommendation: Default to 500ms coalescing for model saves. Immediate-flush deltas bypass the coalescing channel entirely by writing directly to stdout_tx with priority.

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
- C++ source: `app/mailsync/MailSync/constants.h` — exact V1-V9 SQL schema, tables, indexes, FTS5 virtual tables
- C++ source: `app/mailsync/MailSync/DeltaStream.cpp` — delta wire format, coalescing logic, `queueDeltaForDelivery`, `flushWithin`
- C++ source: `app/mailsync/MailSync/MailStore.cpp` — save/remove pattern, transaction pattern, `_emit` logic
- C++ source: `app/mailsync/MailSync/Models/MailModel.cpp` — fat row pattern, `bindToQuery`, `toJSON`, `__cls` injection, metadata afterSave
- C++ source: `app/mailsync/MailSync/Models/Message.cpp` — column list, JSON key names, indexed column binding

### Secondary (MEDIUM confidence)

- [rusqlite serde_json.rs source](https://github.com/rusqlite/rusqlite/blob/master/src/types/serde_json.rs) — ToSql/FromSql for serde_json::Value: NULL→NULL, JSON object/array→TEXT, numbers→INT/REAL
- [rusqlite_migration README](https://github.com/cljoly/rusqlite_migration/blob/master/README.md) — Migration pattern with WAL pragma before to_latest()

### Tertiary (LOW confidence)

- WebSearch results on delta coalescing patterns with tokio — no authoritative single source; pattern derived from C++ source analysis

---

## Metadata

**Confidence breakdown:**
- Standard stack (tokio-rusqlite, rusqlite_migration, serde): HIGH — versions confirmed from official docs and GitHub releases as of 2026-03-02
- Architecture (fat row pattern, delta coalescing): HIGH — directly derived from C++ source code which is the authoritative reference
- Schema (V1-V9 SQL): HIGH — verbatim from constants.h in the repository
- Pitfalls: MEDIUM — most derived from code analysis + known SQLite/rusqlite behavior; V5 gap is a real risk requiring test validation

**Research date:** 2026-03-02
**Valid until:** 2026-06-01 (tokio-rusqlite and rusqlite APIs are stable; schema derived from committed C++ source)
