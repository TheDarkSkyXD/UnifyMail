# Phase 6 Deep Dive: Query Class and toJSON Implementation

**Researched:** 2026-03-03
**Confidence:** HIGH — all code traced directly from C++ source
**Scope:** Query builder class, toJSON()/toJSONDispatch(), nested object serialization, model construction from DB

---

## Query Builder Class

Source: `Query.hpp` and `Query.cpp`

### Structure

```cpp
class Query {
    json _clauses;  // JSON object keyed by column name: {"col": {"op":"=","rhs":"value"}}
    int _limit;     // 0 = no limit
};
```

### Methods (all return `*this` for chaining)

| Method | Clause Generated |
|---|---|
| `equal(col, string)` | `col = ?` |
| `equal(col, double)` | `col = ?` |
| `equal(col, vector<string>)` | `col IN (?,?,...)` — warns if >999 |
| `equal(col, vector<uint32_t>)` | `col IN (?,?,...)` — warns if >999 |
| `gt(col, double)` | `col > ?` |
| `gte(col, double)` | `col >= ?` |
| `lt(col, double)` | `col < ?` |
| `lte(col, double)` | `col <= ?` |
| `limit(int)` | Stored; appended by MailStore, NOT by Query |

### getSQL() — SQL Generation

```
if no clauses → return ""
result = " WHERE "
for each clause (in alphabetical key order):
    if not first: result += " AND "
    if rhs is array:
        if array empty: result += "0 = 1"    // always-false guard
        else: result += "col IN (?,?,?,...)"
    else:
        result += "col op ?"
return result
```

**CRITICAL — Binding order:** `nlohmann::json` objects iterate in **alphabetical key order**. So `Query().equal("remoteFolderId", x).equal("accountId", y)` generates `WHERE accountId = ? AND remoteFolderId = ?` and binds `y` first, then `x`. The `bind()` method follows the same iteration order, so parameters are always consistent.

### bind() — Parameter Binding

Iterates `_clauses` in alphabetical key order. For each clause:
- Scalar string → `stmt.bind(pos, string_value)`
- Scalar double → `stmt.bind(pos, double_value)`
- Array of strings → `stmt.bind(pos++, each_string)`
- Array of numbers → `stmt.bind(pos++, each_double)`

Uses 1-based positional binding.

### Usage in Lifecycle Hooks

```cpp
// Message::afterSave() — find parent thread
store->find<Thread>(Query().equal("id", threadId()))

// saveFolderStatus() — re-read folder
find<Folder>(Query().equal("accountId", folder->accountId()).equal("id", folder->id()))

// allLabelsCache() — fetch all labels
findAll<Label>(Query().equal("accountId", accountId))

// findLargeSet() — chunked IN query
findAll<ModelClass>(Query().equal(colname, chunk))  // chunk = vector<string>, max 900 items
```

### Recommendation for Rust

The Query class is a thin wrapper. For Phase 6, **replace with direct SQL strings**:

```rust
// Instead of Query().equal("id", thread_id):
"SELECT data FROM Thread WHERE id = ?1 LIMIT 1"

// Instead of Query().equal("accountId", aid).equal("id", fid):
"SELECT data FROM Folder WHERE accountId = ?1 AND id = ?2 LIMIT 1"

// Instead of findLargeSet with chunking:
fn find_large_set<T>(col: &str, ids: &[String]) -> Vec<T> {
    ids.chunks(900).flat_map(|chunk| {
        let placeholders = (1..=chunk.len()).map(|i| format!("?{}", i)).collect::<Vec<_>>().join(",");
        let sql = format!("SELECT data FROM {} WHERE {} IN ({})", T::TABLE_NAME, col, placeholders);
        // execute and collect
    }).collect()
}
```

The only non-trivial behavior to preserve is the empty-array → `0 = 1` guard.

---

## toJSON() Implementation

### Base Class: MailModel::toJSON()

Source: `MailModel.cpp` lines 113-120

```cpp
json MailModel::toJSON() {
    if (!_data.count("__cls")) {
        _data["__cls"] = this->tableName();
    }
    return _data;   // returns a COPY of the full _data JSON
}
```

**Key insight:** `_data` IS the model. `toJSON()` is nearly a no-op — it just ensures `__cls` is present, then returns the entire `_data` object. There is no field-by-field serialization. The `_data` JSON object is loaded from the `data` column at construction time and is the single source of truth.

### `__cls` Injection Rules

| Model | `__cls` value | When set |
|---|---|---|
| Most models | `tableName()` (e.g., `"Message"`, `"Thread"`, `"Folder"`) | Lazily on first `toJSON()` call |
| Task | Task type name (e.g., `"SendDraftTask"`) | Set in Task constructor; `toJSON()` guard preserves it |
| Label | `"Label"` | Via `tableName()` (Label is a separate table from Folder) |

**Once set, `__cls` persists in `_data` and is written to the `data` column.** On reload from DB, `_data` already contains `__cls`, so the lazy inject branch is never triggered.

### toJSONDispatch() — Dispatch-Only Additions

**Base class:** `return this->toJSON()` — identical to `toJSON()`.

**Only override — Message::toJSONDispatch():**

Source: `Message.cpp` lines 499-512

```cpp
json Message::toJSONDispatch() {
    json j = toJSON();                          // standard _data with __cls
    if (_bodyForDispatch.length() > 0) {
        j["body"] = _bodyForDispatch;           // transient, never in data column
        j["fullSyncComplete"] = true;
    }
    if (version() == 1) {
        j["headersSyncComplete"] = true;        // signals first-save headers availability
    }
    return j;
}
```

| Field | In `toJSON()` (persisted) | In `toJSONDispatch()` (delta) |
|---|---|---|
| `body` | NEVER | Only when `_bodyForDispatch` is set |
| `fullSyncComplete` | NEVER | When body is present |
| `headersSyncComplete` | NEVER | When version == 1 (first save) |

No other model overrides `toJSONDispatch()`.

---

## Nested Object Serialization

### Contact Objects (from/to/cc/bcc/replyTo)

Source: `MailUtils::contactJSONFromAddress()`

Plain JSON objects with two optional string fields:
```json
{"name": "Alice", "email": "alice@example.com"}
```

- `name` — display name, omitted if null
- `email` — mailbox address, omitted if null
- No `__cls` field on contacts
- Stored as arrays in `_data["from"]`, `_data["to"]`, `_data["cc"]`, `_data["bcc"]`, `_data["replyTo"]`

### Folder Objects Embedded in Message

Source: `Message::setClientFolder()` and `Message::setRemoteFolder()`

```cpp
void setClientFolder(Folder * folder) {
    _data["folder"] = folder->toJSON();          // full Folder _data blob
    _data["folder"].erase("localStatus");        // strip sync-internal state
}
void setRemoteFolder(Folder * folder) {
    _data["remoteFolder"] = folder->toJSON();
    _data["remoteFolder"].erase("localStatus");
}
```

The embedded folder is the full `toJSON()` output of a Folder model (including `__cls: "Folder"`, `id`, `aid`, `v`, `path`, `role`) with `localStatus` explicitly stripped.

### Folder/Label Objects in Thread (with `_refs` and `_u`)

When a folder/label is added to a Thread's `folders`/`labels` array:

```cpp
// Folder in Thread
json f = next->clientFolder();     // full folder JSON from Message._data["folder"]
f["_refs"] = 1;                    // refcount: messages in thread with this folder
f["_u"] = isUnread ? 1 : 0;       // unread count for this folder in this thread

// Label in Thread
json l = label->toJSON();          // full Label model JSON (including __cls: "Label")
l["_refs"] = 1;
l["_u"] = (isUnread && inAllMail) ? 1 : 0;
```

**Thread folder/label entry structure:**
```json
{
    "id": "folder-id",
    "aid": "account-id",
    "v": 3,
    "__cls": "Folder",
    "path": "INBOX",
    "role": "inbox",
    "_refs": 5,
    "_u": 2
}
```

The `_refs` and `_u` fields are **Thread-local augmentations** — they don't exist in the standalone Folder/Label model's JSON.

### File Objects Embedded in Message

Source: `Message::setFiles()`

```cpp
void setFiles(vector<File> & files) {
    json arr = json::array();
    for (auto & file : files) {
        arr.push_back(file.toJSON());   // full File _data blob including __cls: "File"
    }
    _data["files"] = arr;
}
```

Each file is the full `toJSON()` of a File model: `id`, `aid`, `v`, `__cls`, `messageId`, `partId`, `contentId`, `contentType`, `filename`, `size`.

---

## Model Construction from Database

### MailStore::find<T>() — Single Result

Source: `MailStore.hpp` lines 123-132

```cpp
template<typename ModelClass>
shared_ptr<ModelClass> find(Query & query) {
    assertCorrectThread();
    SQLite::Statement statement(_db,
        "SELECT data FROM " + ModelClass::TABLE_NAME + query.getSQL() + " LIMIT 1");
    query.bind(statement);
    if (statement.executeStep()) {
        return make_shared<ModelClass>(statement);
    }
    return nullptr;
}
```

### MailStore::findAll<T>() — Multiple Results

Source: `MailStore.hpp` lines 134-150

```cpp
template<typename ModelClass>
vector<shared_ptr<ModelClass>> findAll(Query & query) {
    string sql = "SELECT data FROM " + ModelClass::TABLE_NAME + query.getSQL();
    if (query.getLimit() != 0) sql += " LIMIT " + to_string(query.getLimit());
    SQLite::Statement statement(_db, sql);
    query.bind(statement);
    vector<shared_ptr<ModelClass>> results;
    while (statement.executeStep()) {
        results.push_back(make_shared<ModelClass>(statement));
    }
    return results;
}
```

### MailStore::findAllGeneric() — Runtime Dispatch

Source: `MailStore.cpp` lines 498-516

Only supports 3 types (case-insensitive):
- `"message"` → `findAll<Message>`
- `"thread"` → `findAll<Thread>`
- `"contact"` → `findAll<Contact>`
- Any other type → `assert(false)` (hard crash)

### MailStore::findLargeSet<T>() — Chunked IN Query

Source: `MailStore.hpp` lines 156-169

```cpp
template<typename ModelClass>
vector<shared_ptr<ModelClass>> findLargeSet(string colname, vector<string> & set) {
    auto chunks = MailUtils::chunksOfVector(set, 900);
    vector<shared_ptr<ModelClass>> all;
    for (auto chunk : chunks) {
        auto results = findAll<ModelClass>(Query().equal(colname, chunk));
        all.insert(all.end(), results.begin(), results.end());
    }
    return all;
}
```

**WARNING:** `chunksOfVector` is destructive — it erases elements from the input vector. After `findLargeSet` returns, the input `set` is empty. Rust should take `&[String]` instead.

### Deserialization Path

For ALL model types:

```
1. SQLite returns: data TEXT column containing JSON string
2. MailModel(SQLite::Statement &):
     _data = json::parse(query.getColumn("data").getString())
     captureInitialMetadataState()  // for metadata dirty tracking
3. Subclass constructor (e.g., Message(SQLite::Statement &)):
     delegates to MailModel(query)
     then captures subclass-specific state (_lastSnapshot, etc.)
```

**Only the `data` column is read.** Indexed columns (unread, starred, threadId, etc.) are never read back — they exist solely for WHERE/ORDER BY clauses.

### Rust Equivalent

```rust
fn find_message(db: &rusqlite::Connection, id: &str) -> Result<Option<Message>> {
    let result = db.query_row(
        "SELECT data FROM Message WHERE id = ?1 LIMIT 1",
        [id],
        |row| row.get::<_, String>(0)
    ).optional()?;
    result.map(|s| serde_json::from_str::<Message>(&s)).transpose()
}
```

---

## MailStore Read API Summary

| Method | SQL Pattern | Return | LIMIT |
|---|---|---|---|
| `find<T>(query)` | `SELECT data FROM T WHERE ... LIMIT 1` | `Option<T>` | Always 1 |
| `findAll<T>(query)` | `SELECT data FROM T WHERE ... [LIMIT n]` | `Vec<T>` | Only if `query.limit()` called |
| `findAllGeneric(type, query)` | Same as findAll, runtime dispatch | `Vec<MailModel>` | Message/Thread/Contact only |
| `findLargeSet<T>(col, set)` | `SELECT data FROM T WHERE col IN (?,...)` per chunk | `Vec<T>` | Chunks of 900 |
| `findAllMap<T>(query, key)` | `SELECT key, data FROM T WHERE ...` | `HashMap<String, T>` | No |
| `findAllUINTMap<T>(query, key)` | `SELECT key, data FROM T WHERE ...` | `HashMap<u32, T>` | No |

---

## MailStore Helper Methods

### getKeyValue() / saveKeyValue()

Source: `MailStore.cpp` lines 280-297

```sql
-- getKeyValue:
SELECT value FROM _State WHERE id = ?
-- Returns: string value, or "" if no row

-- saveKeyValue:
REPLACE INTO _State (id, value) VALUES (?, ?)
-- Upsert via REPLACE (INSERT OR REPLACE)
```

Neither uses cached prepared statements.

### saveFolderStatus()

Source: `MailStore.cpp` lines 432-452

```
1. if changedStatus == initialStatus → return (no change)
2. BEGIN IMMEDIATE TRANSACTION
3. Re-read folder: SELECT data FROM Folder WHERE accountId = ? AND id = ? LIMIT 1
4. If folder was deleted → return
5. Merge loop:
     for each key k in changedStatus:
         if k not in initialStatus OR changedStatus[k] != initialStatus[k]:
             current.localStatus()[k] = changedStatus[k]
6. save(current)     // UPDATE Folder SET data=..., accountId=..., version=..., path=..., role=... WHERE id=...
7. COMMIT
```

**Purpose:** Safely merges only changed `localStatus` keys into the DB-current folder, avoiding overwriting concurrent changes to other keys. The re-read inside the transaction is the critical part — it gets the latest state, then patches only the changed fields.
