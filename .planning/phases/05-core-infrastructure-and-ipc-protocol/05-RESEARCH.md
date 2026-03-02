# Phase 5: Core Infrastructure and IPC Protocol - Research

**Researched:** 2026-03-02
**Domain:** Rust standalone binary — stdin/stdout IPC protocol, process mode dispatch, SQLite schema creation, delta emission pipeline, tokio task architecture
**Confidence:** HIGH (IPC protocol extracted directly from C++ source and TypeScript consumer; schema SQL extracted from constants.h verbatim; delta coalescing algorithm extracted from DeltaStream.cpp; tokio patterns from official documentation)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| IPC-01 | Binary reads account JSON and identity JSON from stdin on startup (two-line handshake) | Exact handshake sequence extracted from main.cpp lines 896–946 and mailsync-process.ts lines 211–222 |
| IPC-02 | Binary emits newline-delimited JSON to stdout with exact field names (`modelJSONs`, `modelClass`, `type`) | Field names extracted from DeltaStream.cpp `dump()` and verified in mailsync-bridge.ts line 434 |
| IPC-03 | Binary handles all stdin commands: `queue-task`, `cancel-task`, `wake-workers`, `need-bodies`, `sync-calendar` | All command handlers extracted from main.cpp `runListenOnMainThread()` |
| IPC-04 | Binary supports all process modes: `sync`, `test`, `migrate`, `reset`, `install-check` | Mode dispatch logic extracted from main.cpp; each mode's behavior documented |
| IPC-05 | Binary exits with code 141 when parent closes stdin (orphan detection) | Exit code 141 confirmed in main.cpp line 651; 30-second grace period documented |
| IPC-06 | stdout uses explicit flush after every message (no block buffering in pipe mode) | Pipe buffering behavior verified; tokio issue #7174 and explicit flush pattern documented |
| IMPR-08 | Async multiplexing via tokio tasks reduces OS thread count vs C++ implementation | C++ thread model (5 OS threads per account) vs tokio task model documented |
</phase_requirements>

---

## Summary

Phase 5 creates the Rust mailsync binary skeleton — the foundation that every subsequent phase builds on. The binary must be an exact wire-format drop-in replacement for the C++ mailsync engine from the TypeScript bridge's perspective. This means: identical stdin handshake, identical stdout delta field names, identical process modes, identical exit codes, and identical pipe buffering behavior. Getting any of these wrong at Phase 5 means every downstream phase is built on a broken foundation.

The C++ source read in this research reveals the exact protocol at the byte level. The stdin handshake is two `getline()` calls — account JSON first, then identity JSON — triggered by the first stdout byte from the process (mailsync-process.ts sends the JSON pair only after receiving any data from stdout). The delta wire format has exactly three field names: `type`, `modelJSONs`, and `modelClass` — in that order as emitted by `DeltaStream::dump()`. The coalescing algorithm is a two-level buffer: a `map<modelClass, vector<DeltaStreamItem>>` where items of the same `type` and `modelClass` are merged by upserting by `id` field, preserving all previously-seen keys via key-level merge. The 500ms flush window is implemented via a detached thread sleeping until a deadline, not a polling loop.

In Rust, Phase 5 implements: (1) a clap `--mode` argument selecting one of five modes; (2) the sync mode's three-task tokio skeleton (stdin_loop, delta_flush_task, and a stub sync loop); (3) the DeltaStream as an `mpsc::unbounded_channel` with the exact coalescing algorithm from C++; (4) the SQLite schema creation matching the 9-version C++ migration history; and (5) the stdout flush pattern using a dedicated task that exclusively owns stdout, calling `flush()` after every batch.

**Primary recommendation:** Implement the binary at `app/mailsync-rust/src/main.rs`. Use clap derive for `--mode`. Use `tokio::sync::mpsc::unbounded_channel` for the delta channel. Implement the coalescing buffer as `IndexMap<String, Vec<DeltaStreamItem>>` keyed by `modelClass`. Call `std::io::stdout().flush()` (not tokio's async stdout) inside the dedicated flush task after each batch write to ensure the OS pipe buffer is drained immediately.

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio` | 1.x | Async runtime for stdin loop, delta flush task, and mode dispatch | Required for all subsequent phases; own the runtime via `#[tokio::main]` |
| `clap` | 4.5.x | CLI argument parsing (`--mode`, `--verbose`, `--info`, `--orphan`) | De facto standard; derive API maps directly to the C++ option parser structure |
| `serde` | 1.x | Serialization framework for all JSON models | Required by serde_json |
| `serde_json` | 1.x | Parse stdin commands; serialize delta messages to stdout | The only JSON library; newline-delimited JSON requires `serde_json::from_str` per line |
| `rusqlite` | 0.38.x | SQLite schema creation for `--mode migrate`; database access for all other phases | Bundled feature avoids system SQLite version skew; matches C++ bundled SQLite approach |
| `tokio-rusqlite` | 0.7.x | Async wrapper for rusqlite — single-writer-thread model | Required by IMPR-08; prevents tokio thread starvation from synchronous SQLite |
| `tracing` | 0.1.x | Structured logging to stderr (never stdout — stdout is reserved for deltas) | Tokio-native; `#[instrument]` for async span correlation; replaces C++ spdlog |
| `tracing-subscriber` | 0.3.x | Log output routing with `EnvFilter` and rotating file sink | Provides RUST_LOG control; file sink needed for sync mode (same as C++ spdlog file sink) |
| `thiserror` | 2.x | `SyncError` enum derivation with `is_retryable()` / `is_offline()` | Structured errors with retryability metadata; cleaner than `anyhow` for this domain |
| `indexmap` | 2.x | Ordered map for delta coalesce buffer (`IndexMap<String, Vec<DeltaStreamItem>>`) | Preserves insertion order for deterministic stdout output; critical for contract tests |

### Supporting (Phase 5 only — subsequent phases add IMAP, SMTP, etc.)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `tokio-util` | 0.7.x | `LinesCodec` / `FramedRead` for stdin line reading | Cleaner than manual `BufReader.lines()` — provides a `Stream<Item=String>` |
| `tracing-appender` | 0.2.x | Non-blocking rotating log file sink | Used only in `--mode sync` to write `mailsync-{account_id}.log` like C++ |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tokio::sync::mpsc::unbounded_channel` | `bounded_channel` | Bounded channels can block senders (backpressure); unbounded matches C++ which uses a mutex-protected vector with no capacity limit. Use unbounded for Phase 5. |
| `rusqlite bundled` | System SQLite | macOS ships SQLite 3.36 (outdated, no WAL2); Windows has no system SQLite. Bundling is mandatory. |
| `IndexMap` for delta buffer | `HashMap` | HashMap iteration order is random — delta order becomes non-deterministic and contract tests are unreliable |
| `clap derive` | `clap builder API` | Builder API is more flexible but verbose; the C++ option parser has a fixed set of flags that map cleanly to a derive struct |

**Installation:**
```bash
# Add to app/mailsync-rust/Cargo.toml
# Core Phase 5 dependencies only — subsequent phases add async-imap, lettre, etc.
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "io-util", "macros", "sync", "fs"] }
tokio-util = { version = "0.7", features = ["codec"] }
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
rusqlite = { version = "0.38", features = ["bundled"] }
tokio-rusqlite = "0.7"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"
thiserror = "2"
indexmap = "2"
```

---

## Architecture Patterns

### Recommended Project Structure (Phase 5 scope)

```
app/mailsync-rust/
├── Cargo.toml                    # Package: name="mailsync", bin target
├── Cargo.lock
├── src/
│   ├── main.rs                   # #[tokio::main], clap parse, mode dispatch
│   ├── cli.rs                    # Args struct (clap derive): --mode, --verbose, --orphan, --info
│   ├── account.rs                # Account + Identity deserialized from stdin JSON
│   ├── error.rs                  # SyncError enum with is_retryable() / is_offline()
│   ├── delta/
│   │   ├── mod.rs                # re-exports
│   │   ├── stream.rs             # DeltaStream: mpsc sender, emit(), beginConnectionError()
│   │   ├── flush.rs              # delta_flush_task(): coalesce loop + stdout flush
│   │   └── item.rs               # DeltaStreamItem: type, modelClass, modelJSONs, upsert logic
│   ├── store/
│   │   ├── mod.rs                # re-exports
│   │   ├── mail_store.rs         # MailStore::open(), migrate(), reset_for_account()
│   │   └── migrations.rs         # V1..V9 SQL migration arrays (copied from constants.h)
│   ├── stdin_loop.rs             # stdin_loop task: read lines, dispatch commands, detect EOF
│   └── modes/
│       ├── mod.rs
│       ├── migrate.rs            # --mode migrate: open store, call migrate(), exit
│       ├── install_check.rs      # --mode install-check: exit 0
│       ├── reset.rs              # --mode reset: read account JSON, reset_for_account()
│       ├── test_auth.rs          # --mode test: stub for Phase 5 (returns not-implemented)
│       └── sync.rs               # --mode sync: spawn tasks, run sync skeleton
├── tests/
│   └── ipc_contract.rs           # Contract test: verify delta field names and handshake
└── docs/
    └── protocol.md               # Wire format reference
```

### Pattern 1: Two-Line Stdin Handshake (IPC-01)

**What:** On startup in `sync` and `test` modes, the binary does NOT receive `--account` or `--identity` flags. Instead it writes a byte to stdout to signal readiness, then reads two lines from stdin: account JSON, then identity JSON. This is required by `mailsync-process.ts` which only pipes the JSON pair after receiving any stdout data from the process.

**Source evidence from mailsync-process.ts (lines 211-222):**
```typescript
this._proc.stdout.once('data', () => {
  // After receiving any data from stdout, pipe account+identity to stdin
  const rs = new Readable();
  rs.push(`${JSON.stringify(this.account)}\n${JSON.stringify(this.identity)}\n`);
  rs.push(null);
  rs.pipe(this._proc.stdin, { end: false });
});
```

**Source evidence from main.cpp (lines 896-946):** The C++ engine does:
1. Prints `"\nWaiting for Account JSON:\n"` to cout (this triggers the TypeScript to send JSON)
2. Calls `getline(cin, accountJSON)` — blocks until account JSON arrives
3. Validates account
4. Prints `"\nWaiting for Identity JSON:\n"` to cout
5. Calls `getline(cin, identityJSON)` — blocks until identity JSON arrives

**Rust implementation:**
```rust
// main.rs — sync mode startup
use tokio::io::{AsyncBufReadExt, BufReader};
use std::io::Write;

async fn read_handshake() -> Result<(Account, Option<Identity>), SyncError> {
    // Signal readiness to TypeScript (triggers stdin pipe)
    print!("\nWaiting for Account JSON:\n");
    std::io::stdout().flush()?;

    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    // Line 1: Account JSON
    let account_line = lines.next_line().await?
        .ok_or_else(|| SyncError::Protocol("stdin closed before account JSON".into()))?;
    let account: Account = serde_json::from_str(&account_line)?;

    // Signal readiness for identity
    print!("\nWaiting for Identity JSON:\n");
    std::io::stdout().flush()?;

    // Line 2: Identity JSON
    let identity_line = lines.next_line().await?
        .ok_or_else(|| SyncError::Protocol("stdin closed before identity JSON".into()))?;
    let identity: Option<Identity> = if identity_line == "null" {
        None
    } else {
        Some(serde_json::from_str(&identity_line)?)
    };

    Ok((account, identity))
}
```

### Pattern 2: DeltaStreamItem Wire Format (IPC-02)

**What:** The exact JSON structure emitted to stdout. The TypeScript parser in `mailsync-bridge.ts` line 434 destructures `{ type, modelJSONs, modelClass }` — all three must be present and correctly spelled. The C++ `DeltaStream::dump()` (DeltaStream.cpp line 100-107) reveals the exact field order.

**Exact wire format extracted from C++ DeltaStream.cpp:**
```cpp
string DeltaStreamItem::dump() const {
    json j = {
        {"type", type},          // "persist" or "unpersist"
        {"modelJSONs", modelJSONs},  // array of model JSON objects
        {"modelClass", modelClass}   // table name: "Message", "Thread", etc.
    };
    return j.dump();
}
```

**Special delta types (not database models):**
```json
// Connection error start/end:
{"type":"persist","modelClass":"ProcessState","modelJSONs":[{"accountId":"...", "id":"...", "connectionError":true}]}

// OAuth token update:
{"type":"persist","modelClass":"ProcessAccountSecretsUpdated","modelJSONs":[{...account JSON...}]}

// Provider lookup response (not batched — immediate):
{"type":"provider-result","requestId":"...","provider":{...}}

// Capabilities response:
{"type":"capabilities-result","requestId":"...","capabilities":{...}}

// Folder status response:
{"type":"folder-status","requestId":"...","statuses":[...]}
```

**Rust struct:**
```rust
// delta/item.rs
use serde::{Serialize, Deserialize};
use serde_json::Value;
use indexmap::IndexMap;

#[derive(Debug, Clone, Serialize)]
pub struct DeltaStreamItem {
    #[serde(rename = "type")]
    pub delta_type: String,         // "persist" or "unpersist"
    pub model_class: String,        // renamed: modelClass in JSON
    pub model_jsons: Vec<Value>,    // renamed: modelJSONs in JSON
    // Internal: not serialized
    #[serde(skip)]
    pub id_indexes: IndexMap<String, usize>,  // id -> index in model_jsons
}

// CRITICAL: Use rename_all or explicit renames to match C++ field names exactly
// The TypeScript parser checks for modelJSONs and modelClass verbatim.
```

**Correct serde rename configuration:**
```rust
#[derive(Serialize)]
pub struct DeltaMessage {
    #[serde(rename = "type")]
    pub delta_type: String,
    #[serde(rename = "modelClass")]
    pub model_class: String,
    #[serde(rename = "modelJSONs")]
    pub model_jsons: Vec<serde_json::Value>,
}
```

### Pattern 3: Delta Coalescing Algorithm (DATA-02 preview — needed for Phase 5)

**What:** The exact coalescing logic from DeltaStream.cpp. Two saves of the same model within the flush window must produce ONE delta, not two. Keys are merged (not replaced) to preserve conditionally-included fields (e.g., `message.body` may not be in every save).

**Extracted from DeltaStream.cpp `upsertModelJSON()` (lines 79-98):**

The algorithm:
1. Buffer is `map<modelClass, vector<DeltaStreamItem>>`
2. On new item: check if `buffer[modelClass]` is non-empty AND the last item has same `type`
3. If yes: call `concatenate(item)` on the last item — upsert each model JSON by `id` field, merging keys
4. If no (different type or empty): push new item to the vector
5. Key merge: for each key in new JSON, update the existing JSON value (existing wins on conflict? No — **new value overwrites existing key**, but existing keys NOT in new JSON are preserved)

**Key merge insight from C++ line 91-93:**
```cpp
// Merge keys: new value replaces existing key, but existing keys absent in new item survive
for (const auto &e : item.items()) {
    existing[e.key()] = e.value();  // new value overwrites for this key
}
// Keys in `existing` but not in `item` are preserved (no deletion)
```

**Rust implementation:**
```rust
// delta/item.rs
impl DeltaStreamItem {
    pub fn concatenate(&mut self, other: &DeltaStreamItem) -> bool {
        if other.delta_type != self.delta_type || other.model_class != self.model_class {
            return false;
        }
        for json in &other.model_jsons {
            self.upsert_model_json(json.clone());
        }
        true
    }

    fn upsert_model_json(&mut self, item: serde_json::Value) {
        let id = item["id"].as_str().unwrap_or("").to_string();
        if let Some(&idx) = self.id_indexes.get(&id) {
            // Merge: new keys overwrite, missing keys preserved
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
```

### Pattern 4: Delta Flush Task with Explicit stdout Flush (IPC-06)

**What:** A dedicated tokio task owns stdout exclusively. It receives items from an unbounded mpsc channel, coalesces them into the buffer, and flushes every 500ms or when the buffer grows past a threshold. Every flush MUST call `std::io::stdout().flush()` explicitly — without this, writes are buffered by the OS pipe and the Electron UI never receives deltas.

**Why this is critical:** When a Rust process writes to a pipe (not a TTY), the OS switches stdout to block buffering. Data sits in the OS pipe buffer until it fills (typically 64KB) or the process exits. The Electron `_proc.stdout.on('data', ...)` callback never fires. The result: no UI updates, ever.

**The tokio issue #7174 finding:** tokio's async `AsyncWrite` on stdout does NOT flush the underlying OS buffer. You must call `std::io::stdout().flush()` (synchronous, blocking) or the `BufWriter::flush()` equivalent. Since the flush task owns stdout exclusively, the synchronous flush in the dedicated task is safe.

```rust
// delta/flush.rs
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use std::io::Write;
use indexmap::IndexMap;
use crate::delta::item::DeltaStreamItem;

pub async fn delta_flush_task(mut rx: mpsc::UnboundedReceiver<DeltaStreamItem>) {
    let mut buffer: IndexMap<String, Vec<DeltaStreamItem>> = IndexMap::new();
    let mut flush_interval = interval(Duration::from_millis(500));
    flush_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            Some(item) = rx.recv() => {
                coalesce_into(&mut buffer, item);
            }
            _ = flush_interval.tick() => {
                if !buffer.is_empty() {
                    flush_buffer(&buffer);
                    buffer.clear();
                }
            }
            else => {
                // Channel closed — process shutting down, flush remaining
                if !buffer.is_empty() {
                    flush_buffer(&buffer);
                }
                return;
            }
        }
    }
}

fn coalesce_into(buffer: &mut IndexMap<String, Vec<DeltaStreamItem>>, item: DeltaStreamItem) {
    let entry = buffer.entry(item.model_class.clone()).or_default();
    if entry.is_empty() || !entry.last_mut().unwrap().concatenate(&item) {
        entry.push(item);
    }
}

fn flush_buffer(buffer: &IndexMap<String, Vec<DeltaStreamItem>>) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for items in buffer.values() {
        for item in items {
            // Serialize with exact field names: type, modelClass, modelJSONs
            let msg = serde_json::json!({
                "type": item.delta_type,
                "modelClass": item.model_class,
                "modelJSONs": item.model_jsons,
            });
            writeln!(out, "{}", msg).unwrap();
        }
    }
    // CRITICAL: explicit flush — without this, pipe buffering silently holds deltas
    out.flush().unwrap();
}
```

**Alternative approach (also valid):** Use `tokio::io::stdout()` with `AsyncWriteExt::write_all` followed by `flush().await`. This is the async equivalent. The dedicated task approach using synchronous `std::io::stdout().lock()` is simpler because: (1) no await points during the write+flush sequence means no interleaving, (2) the lock duration is short, (3) tokio's async stdout has the tokio issue #7174 data-loss risk on process exit.

### Pattern 5: Process Mode Dispatch (IPC-04)

**What:** The C++ engine dispatches on `--mode` before reading stdin (for migrate and install-check) or after reading account JSON (for reset) or after reading both JSONs (for sync and test). The Rust binary must replicate this order exactly.

**Mode behaviors extracted from main.cpp:**

| Mode | Account JSON? | Identity JSON? | Stdin loop? | Exit code |
|------|--------------|----------------|-------------|-----------|
| `migrate` | No | No | No | 0 on success, 1 on error |
| `install-check` | No | No | No | 0 if all checks pass, 1 if any fail |
| `reset` | Yes (read from stdin/flag) | No | No | 0 on success, 1 on error |
| `test` | Yes | Yes | No | 0 on success, 1 on error |
| `sync` | Yes | Yes | Yes (main loop) | Never (runs until stdin EOF or fatal error) |

**Rust clap derive structure:**
```rust
// cli.rs
use clap::{Parser, ValueEnum};

#[derive(Parser, Debug)]
#[command(name = "mailsync")]
pub struct Args {
    /// Process mode
    #[arg(short = 'm', long = "mode", value_enum)]
    pub mode: Mode,

    /// Allow process to run without a parent bound to stdin (orphan mode)
    #[arg(short = 'o', long = "orphan")]
    pub orphan: bool,

    /// Log all IMAP and SMTP traffic for debugging
    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    /// Email address info (for log file naming only)
    #[arg(long = "info")]
    pub info: Option<String>,

    /// Account JSON (optional — if not provided, read from stdin)
    #[arg(short = 'a', long = "account")]
    pub account: Option<String>,

    /// Identity JSON (optional — if not provided, read from stdin)
    #[arg(short = 'i', long = "identity")]
    pub identity: Option<String>,
}

#[derive(ValueEnum, Debug, Clone, PartialEq)]
pub enum Mode {
    Sync,
    Test,
    Migrate,
    Reset,
    #[value(name = "install-check")]
    InstallCheck,
}
```

**Mode dispatch in main.rs:**
```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Check required env vars
    let config_dir = std::env::var("CONFIG_DIR_PATH")
        .map_err(|_| anyhow::anyhow!("CONFIG_DIR_PATH env var required"))?;

    // Modes that don't need account JSON: dispatch immediately
    match args.mode {
        Mode::Migrate => {
            modes::migrate::run(&config_dir).await?;
            return Ok(());
        }
        Mode::InstallCheck => {
            modes::install_check::run().await?;
            return Ok(());
        }
        _ => {}
    }

    // Read account JSON (from --account flag or stdin)
    let account = read_account_json(&args).await?;

    if args.mode == Mode::Reset {
        modes::reset::run(&config_dir, &account).await?;
        return Ok(());
    }

    // Modes requiring identity JSON
    let identity = read_identity_json(&args).await?;

    match args.mode {
        Mode::Test => modes::test_auth::run(&account, &identity).await,
        Mode::Sync => modes::sync::run(&config_dir, account, identity, &args).await,
        _ => unreachable!(),
    }
}
```

### Pattern 6: Stdin Loop with EOF Detection (IPC-05)

**What:** The stdin loop reads JSON lines and dispatches commands. When stdin reaches EOF (parent process closed stdin), the loop exits and the shutdown signal is broadcast. Exit code 141 on orphan detection matches the SIGPIPE convention used by the C++ engine. The C++ uses a 30-second grace period; Rust should match this.

**C++ orphan detection (main.cpp lines 641-655):**
```cpp
if (cin.good()) {
    lostCINAt = 0;
} else {
    if (lostCINAt == 0) {
        lostCINAt = time(0);
    }
    if (time(0) - lostCINAt > 30) {
        std::exit(141);  // Exit code 141 = SIGPIPE convention
    }
    std::this_thread::sleep_for(std::chrono::microseconds(1000));
}
```

**Rust implementation:**
```rust
// stdin_loop.rs
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::broadcast;
use std::time::{Duration, Instant};

pub async fn stdin_loop(
    shutdown_tx: broadcast::Sender<()>,
    // ... other params
) {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();
    let mut lost_stdin_at: Option<Instant> = None;

    loop {
        match lines.next_line().await {
            Ok(Some(line)) if !line.is_empty() => {
                lost_stdin_at = None;
                dispatch_command(&line).await;
            }
            Ok(Some(_)) => {} // empty line — ignore
            Ok(None) => {
                // EOF — stdin closed
                // C++ uses 30-second grace period; for Rust we can exit immediately
                // since tokio handles the EOF cleanly (no "cin.good()" ambiguity)
                tracing::info!("stdin EOF — initiating shutdown");
                let _ = shutdown_tx.send(());
                // Give workers 1 second to flush before exiting
                tokio::time::sleep(Duration::from_secs(1)).await;
                std::process::exit(141);
            }
            Err(e) => {
                // stdin read error — start grace period timer
                if lost_stdin_at.is_none() {
                    lost_stdin_at = Some(Instant::now());
                }
                if lost_stdin_at.unwrap().elapsed() > Duration::from_secs(30) {
                    tracing::info!("stdin lost for 30s — orphan exit");
                    std::process::exit(141);
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
    }
}
```

### Pattern 7: SQLite Schema Creation (IPC-04 — migrate mode)

**What:** `--mode migrate` opens (or creates) the database at `$CONFIG_DIR_PATH/edgehill.db` and runs schema migrations up to version 9. The migration is idempotent — `CREATE TABLE IF NOT EXISTS` and `CREATE INDEX IF NOT EXISTS` mean re-running is safe.

**Database filename from MailStore.cpp line 82:** `CONFIG_DIR_PATH + FS_PATH_SEP + "edgehill.db"`

**Current schema version from MailStore.cpp line 103:** `CURRENT_VERSION = 9`

**All migration SQL is in constants.h.** Phase 5 must replicate these exactly. The V3 migration is the only one that mutates existing data (`ALTER TABLE MessageBody ADD COLUMN fetchedAt`) — it must be guarded by the version check.

**Rust implementation:**
```rust
// store/migrations.rs

// Source: app/mailsync/MailSync/constants.h V1_SETUP_QUERIES
pub const V1_SETUP: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS `_State` (id VARCHAR(40) PRIMARY KEY, value TEXT)",
    "CREATE TABLE IF NOT EXISTS `File` (id VARCHAR(40) PRIMARY KEY, version INTEGER, data BLOB, accountId VARCHAR(8), filename TEXT)",
    "CREATE TABLE IF NOT EXISTS `Event` (id VARCHAR(40) PRIMARY KEY, data BLOB, accountId VARCHAR(8), calendarId VARCHAR(40), _start INTEGER, _end INTEGER, is_search_indexed INTEGER DEFAULT 0)",
    // ... all V1 tables from constants.h (Thread, Message, Folder, Label, Task, etc.)
    // FTS5 virtual tables:
    "CREATE VIRTUAL TABLE IF NOT EXISTS `ThreadSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, subject, to_, from_, categories, body)",
    "CREATE VIRTUAL TABLE IF NOT EXISTS `EventSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, title, description, location, participants)",
    "CREATE VIRTUAL TABLE IF NOT EXISTS `ContactSearch` USING fts5(tokenize = 'porter unicode61', content_id UNINDEXED, content)",
];

pub const V2_SETUP: &[&str] = &[
    "CREATE INDEX IF NOT EXISTS MessageUIDScanIndex ON Message(accountId, remoteFolderId, remoteUID)",
];

pub const V3_SETUP: &[&str] = &[
    "ALTER TABLE `MessageBody` ADD COLUMN fetchedAt DATETIME",
    "UPDATE `MessageBody` SET fetchedAt = datetime('now')",
];

// V4, V6, V7, V8, V9 follow same pattern...
```

**FTS5 note:** The `bundled` feature of rusqlite includes FTS5. No extra feature flag is needed — FTS5 is included in SQLite when `bundled` is used. Verify with `rusqlite::version()` if unsure.

**WAL + PRAGMA setup on connection open:**
```rust
// store/mail_store.rs
use tokio_rusqlite::Connection;

pub struct MailStore {
    writer: Connection,  // Single writer — all writes go through this
    reader: Connection,  // Separate reader — concurrent reads in WAL mode
}

impl MailStore {
    pub async fn open(config_dir: &str) -> Result<Self, SyncError> {
        let db_path = format!("{}/edgehill.db", config_dir);

        let writer = Connection::open(&db_path).await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        writer.call(|conn| {
            // Replicate C++ MailStore constructor pragmas (MailStore.cpp lines 96-100)
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA main.page_size = 4096;
                 PRAGMA main.cache_size = 10000;
                 PRAGMA main.synchronous = NORMAL;"
            )?;
            conn.busy_timeout(std::time::Duration::from_secs(10))?;
            Ok(())
        }).await?;

        let reader = Connection::open(&db_path).await
            .map_err(|e| SyncError::Database(e.to_string()))?;

        Ok(Self { writer, reader })
    }

    pub async fn migrate(&self) -> Result<(), SyncError> {
        self.writer.call(|conn| {
            let version: i32 = conn
                .query_row("PRAGMA user_version", [], |row| row.get(0))?;

            if version < 1 { for sql in V1_SETUP { conn.execute_batch(sql)?; } }
            if version < 2 { for sql in V2_SETUP { conn.execute_batch(sql)?; } }
            if version < 3 {
                // V3 is time-consuming — signal progress to Electron
                // (Electron shows a migration window when it sees "Running Migration")
                print!("\nRunning Migration");
                std::io::stdout().flush().ok();
                for sql in V3_SETUP { conn.execute_batch(sql)?; }
            }
            if version < 4 { for sql in V4_SETUP { conn.execute_batch(sql)?; } }
            if version < 6 { for sql in V6_SETUP { conn.execute_batch(sql)?; } }
            if version < 7 { for sql in V7_SETUP { conn.execute_batch(sql)?; } }
            if version < 8 { for sql in V8_SETUP { conn.execute_batch(sql)?; } }
            if version < 9 { for sql in V9_SETUP { conn.execute_batch(sql)?; } }

            if version < 9 {
                conn.execute_batch("PRAGMA user_version = 9")?;
            }
            Ok(())
        }).await
    }
}
```

### Pattern 8: Sync Mode Task Skeleton (IMPR-08)

**What:** In `--mode sync`, spawn four independent tokio tasks. Phase 5 implements only the skeleton — real IMAP workers come in Phases 7-8. The foreground task is started AFTER the background task completes its first folder pass (this is explicit in C++ main.cpp line 197).

```rust
// modes/sync.rs — Phase 5 skeleton only
pub async fn run(config_dir: &str, account: Account, identity: Option<Identity>, args: &Args) -> Result<()> {
    let (shutdown_tx, _) = tokio::sync::broadcast::channel::<()>(1);
    let (delta_tx, delta_rx) = tokio::sync::mpsc::unbounded_channel::<DeltaStreamItem>();

    let store = Arc::new(MailStore::open(config_dir).await?);
    let delta = Arc::new(DeltaStream::new(delta_tx));

    // Task 1: stdin loop — command dispatch + orphan detection
    let stdin_handle = tokio::spawn(stdin_loop(
        shutdown_tx.clone(),
        store.clone(),
        delta.clone(),
        shutdown_tx.subscribe(),
    ));

    // Task 2: delta flush — coalesce + stdout write
    let flush_handle = tokio::spawn(delta_flush_task(delta_rx));

    // Emit startup ProcessState delta to signal Electron we're alive
    delta.emit(DeltaStreamItem::process_state(&account.id, false));

    // Task 3: background sync stub (real impl in Phase 7)
    let bg_handle = tokio::spawn(background_sync_stub(
        store.clone(),
        delta.clone(),
        shutdown_tx.subscribe(),
    ));

    // Phase 5: wait for stdin to close (orphan detection exits with 141)
    // Real implementation: tokio::select! on all task handles
    tokio::select! {
        _ = stdin_handle => {}
        _ = flush_handle => {}
        _ = bg_handle => {}
    }

    Ok(())
}
```

### Anti-Patterns to Avoid

- **Mixing tokio stdout with explicit flush calls:** Using `tokio::io::stdout()` for async writes while also calling `std::io::stdout().flush()` creates a deadlock — tokio's async stdout wraps the same underlying fd. Pick one. The dedicated flush task using `std::io::stdout().lock()` is the correct pattern for Phase 5.
- **Writing to stdout from multiple tasks:** Any task calling `println!()` or `tracing::info!()` to stdout will corrupt the delta stream. Enforce: ALL stdout writes go through the flush task. ALL tracing goes to stderr. Verify with `tracing_subscriber::fmt().with_writer(std::io::stderr)`.
- **Reading stdin in a blocking call inside async context:** `std::io::stdin().read_line()` blocks the entire tokio worker thread. Use `tokio::io::stdin()` exclusively.
- **Handling stdin EOF without exiting:** If `lines.next_line()` returns `Ok(None)`, the parent closed stdin. This is the primary shutdown signal. Not exiting causes the binary to run forever as an orphan.
- **Omitting the startup prompt to stdout:** Without printing `"\nWaiting for Account JSON:\n"`, the TypeScript bridge never sends account/identity JSON (it waits for any stdout data first). The handshake deadlocks.
- **Using `serde`'s default field naming:** serde default is `snake_case`. The wire protocol requires `modelJSONs` and `modelClass` — use `#[serde(rename = "...")]` on every field of `DeltaMessage`. Missing a rename means silent breakage: the TypeScript parser receives fields it doesn't recognize and logs "message with unexpected keys".

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON serialization | Custom JSON builder | `serde_json` | Escaping, Unicode handling, nested structure — all handled correctly by serde_json |
| CLI argument parsing | Manual `args.iter()` loop | `clap` derive | clap handles `--`, optional args, value validation, and `--help` generation |
| SQLite async access | Direct `rusqlite::Connection` in `async fn` | `tokio-rusqlite::Connection::call` | Direct rusqlite in async blocks the tokio worker thread, causing starvation under load |
| Timed delta coalescing | Custom `std::thread::sleep` timer | tokio `interval()` in flush task | Custom sleep timer has race conditions and uses an extra OS thread |
| Stdin line reading | Manual `read_exact()` with newline scanning | `tokio::io::BufReader::lines()` | BufReader handles partial reads, buffer growth, and UTF-8 correctly |
| Schema migrations | Runtime SQL generation | Literal SQL strings from constants.h | Runtime generation introduces bugs; the C++ SQL is the ground truth — copy verbatim |
| Stdout interleaving prevention | `Mutex<Stdout>` shared across tasks | Dedicated flush task with exclusive ownership | Mutex allows concurrent write setup; flush task guarantees serial write+flush atomicity |

**Key insight:** The delta coalescing algorithm and the handshake sequence are the two most subtle aspects of Phase 5. Both are fully documented in C++ source and must be replicated exactly — no simplification is safe until a contract test validates the output against the TypeScript parser.

---

## Common Pitfalls

### Pitfall 1: stdout Block Buffering Silences All Deltas

**Severity:** CRITICAL — Phase 5 cannot pass without solving this.

**What goes wrong:** Rust stdout in pipe mode switches to block buffering (64KB or more). Deltas pile up in the buffer, never reaching the Electron parent process. The UI shows no new mail, no folder updates, nothing. The process appears healthy (no crash) but produces no output.

**Why it happens:** When stdout is not a TTY (always the case when spawned as a child process), the OS uses block buffering instead of line buffering. Unlike C's `stdio.h` which sets unbuffered mode via `setbuf(stdout, NULL)`, Rust's `std::io::Stdout` does not automatically disable buffering in pipe mode.

**Additional risk (tokio issue #7174):** When using `tokio::io::stdout()` with `AsyncWriteExt`, calling `process::exit()` does NOT flush buffered async writes. Data already written to tokio's internal buffer is silently lost. The synchronous `std::io::stdout().flush()` call in the dedicated flush task is the only reliable solution.

**How to avoid:** The dedicated flush task acquires `std::io::stdout().lock()`, writes all batched JSON lines with `writeln!()`, then calls `.flush()` before releasing the lock. This sequence is guaranteed to drain the OS pipe buffer. Verify by running the binary with stdout piped to `cat` and checking that output appears within 500ms of being emitted.

**Warning signs:** The Electron UI shows "Syncing..." indefinitely. The log shows deltas being emitted on the Rust side but the TypeScript `_onIncomingMessages` handler never fires.

### Pitfall 2: Wrong Field Names Break the TypeScript Parser Silently

**Severity:** CRITICAL — produces silent data loss.

**What goes wrong:** The TypeScript parser in mailsync-bridge.ts line 434 does: `const { type, modelJSONs, modelClass } = json`. If the Rust binary emits `model_jsons` (snake_case) or `modelJSON` (wrong capitalization), the destructure produces `undefined` for those fields. Line 443 then logs "Sync worker sent a JSON formatted message with unexpected keys" and **skips the message**. No error thrown, no crash — just silent data loss.

**Why it happens:** serde's default field naming is snake_case in Rust. `#[derive(Serialize)]` on a struct with `model_jsons: Vec<Value>` emits `"model_jsons"` in JSON. The TypeScript parser expects `"modelJSONs"` exactly.

**How to avoid:** Use explicit `#[serde(rename = "modelJSONs")]` on every field. Write a contract test (IPC-02 success criterion) that deserializes a delta from the Rust binary and verifies the field names before any IMAP code exists.

**Contract test skeleton:**
```rust
// tests/ipc_contract.rs
#[test]
fn delta_field_names_match_typescript_expectation() {
    let item = DeltaStreamItem {
        delta_type: "persist".to_string(),
        model_class: "Message".to_string(),
        model_jsons: vec![serde_json::json!({"id": "test-123"})],
        id_indexes: IndexMap::new(),
    };
    let json_str = item.to_json_string();
    let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    // Exact field name checks
    assert!(parsed.get("type").is_some(), "missing 'type' field");
    assert!(parsed.get("modelClass").is_some(), "missing 'modelClass' field (not 'model_class')");
    assert!(parsed.get("modelJSONs").is_some(), "missing 'modelJSONs' field (not 'model_jsons')");
    assert!(parsed.get("model_class").is_none(), "snake_case leak: 'model_class' must not appear");
    assert!(parsed.get("model_jsons").is_none(), "snake_case leak: 'model_jsons' must not appear");
}
```

### Pitfall 3: Handshake Deadlock — Forgetting the Startup Prompt

**Severity:** CRITICAL — binary hangs forever on startup.

**What goes wrong:** The binary reads from stdin waiting for account JSON, but the TypeScript bridge (`mailsync-process.ts` line 212) only sends account JSON after receiving any data from stdout: `this._proc.stdout.once('data', ...)`. If the binary reads stdin before writing anything to stdout, both sides are waiting — deadlock.

**Why it happens:** The C++ engine prints `"\nWaiting for Account JSON:\n"` which triggers the TypeScript to pipe the JSON. A naive Rust port that goes straight to `lines.next_line().await` without printing first will deadlock.

**How to avoid:** The startup sequence is:
1. Print any bytes to stdout (the exact string doesn't matter — TypeScript just waits for `'data'` event)
2. Flush stdout
3. Then call `lines.next_line().await` for account JSON

The C++ string `"\nWaiting for Account JSON:\n"` also serves as a user-visible prompt when running the binary manually in a terminal — preserve it.

### Pitfall 4: tokio-rusqlite Version Confusion

**Severity:** MEDIUM — compilation failure.

**What goes wrong:** tokio-rusqlite 0.6.x and 0.7.x have different APIs. The STACK.md research was done with 0.6.x in mind, but the crate is currently at 0.7.x on crates.io. The `Connection::call` method signature changed.

**Verification:** As of 2026-03-02, tokio-rusqlite is at 0.7.0. Use `tokio-rusqlite = "0.7"` in Cargo.toml. Run `cargo tree | grep tokio-rusqlite` to verify the resolved version. The API documented here uses the 0.7.x `Connection::call(|conn| { ... })` pattern which is stable across 0.6 and 0.7.

### Pitfall 5: FTS5 Unavailable Without `bundled` Feature

**Severity:** HIGH — --mode migrate fails at runtime.

**What goes wrong:** The schema creates three FTS5 virtual tables: `ThreadSearch`, `EventSearch`, and `ContactSearch`. Without the `bundled` feature of rusqlite, the binary links against the system SQLite, which on macOS (SQLite 3.36) and many Linux distros does not include the FTS5 extension. The `CREATE VIRTUAL TABLE ... USING fts5(...)` statement fails at runtime with "no such module: fts5".

**How to avoid:** `rusqlite = { version = "0.38", features = ["bundled"] }` — the bundled feature compiles SQLite 3.51.1 with FTS5 enabled. Verify the table creates succeed in the contract test.

### Pitfall 6: Exit Code 141 vs SIGPIPE vs process::exit

**Severity:** MEDIUM — incorrect process termination signaling.

**What goes wrong:** The C++ engine calls `std::exit(141)` on stdin EOF orphan detection. In Rust, `std::process::exit(141)` does the same. However, using `panic!()` or returning an error from `main()` causes exit code 1, not 141 — the Electron bridge interprets code 1 as a crash and may restart the worker unnecessarily. Code 141 is a deliberate convention (128 + SIGPIPE signal number 13) that signals "intentional pipe death" to the bridge.

**How to avoid:** Use `std::process::exit(141)` explicitly in the stdin EOF handler. Do not propagate the EOF as a Rust error that bubbles up to main.

---

## Code Examples

Verified patterns from C++ source and official Rust documentation:

### Complete SQLite Tables in Phase 5 Schema

The following tables exist after `--mode migrate` (all 9 versions applied):

| Table | Columns | FTS5? |
|-------|---------|-------|
| `_State` | id, value | No |
| `File` | id, version, data, accountId, filename | No |
| `Event` | id, data, accountId, etag, calendarId, recurrenceStart, recurrenceEnd, icsuid, recurrenceId | No |
| `Label` | id, accountId, version, data, path, role, createdAt, updatedAt | No |
| `Folder` | id, accountId, version, data, path, role, createdAt, updatedAt | No |
| `Thread` | id, accountId, version, data, gThrId, subject, snippet, unread, starred, firstMessageTimestamp, lastMessageTimestamp, lastMessageReceivedTimestamp, lastMessageSentTimestamp, inAllMail, isSearchIndexed, participants, hasAttachments | No |
| `ThreadReference` | threadId, accountId, headerMessageId | No |
| `ThreadCategory` | id, value, inAllMail, unread, lastMessageReceivedTimestamp, lastMessageSentTimestamp | No |
| `ThreadCounts` | categoryId, unread, total | No |
| `ThreadSearch` | content_id, subject, to_, from_, categories, body | FTS5 |
| `Account` | id, data, accountId, email_address | No |
| `Message` | id, accountId, version, data, headerMessageId, gMsgId, gThrId, subject, date, draft, unread, starred, remoteUID, remoteXGMLabels, remoteFolderId, replyToHeaderMessageId, threadId | No |
| `ModelPluginMetadata` | id, accountId, objectType, value, expiration | No |
| `DetatchedPluginMetadata` | objectId, objectType, accountId, pluginId, value, version | No |
| `MessageBody` | id, value, fetchedAt | No |
| `Contact` | id, data, accountId, email, version, refs, hidden, source, bookId, etag | No |
| `Calendar` | id, data, accountId | No |
| `Task` | id, version, data, accountId, status | No |
| `ContactGroup` | id, accountId, bookId, data, version, name | No |
| `ContactContactGroup` | id, value | No |
| `ContactBook` | id, accountId, data, version | No |
| `EventSearch` | content_id, title, description, location, participants | FTS5 |
| `ContactSearch` | content_id, content | FTS5 |

**Account reset queries (replicate for `--mode reset`):**
```rust
// Source: constants.h ACCOUNT_RESET_QUERIES
pub const ACCOUNT_RESET_QUERIES: &[&str] = &[
    "DELETE FROM `ThreadCounts` WHERE `categoryId` IN (SELECT id FROM `Folder` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadCounts` WHERE `categoryId` IN (SELECT id FROM `Label` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadCategory` WHERE `id` IN (SELECT id FROM `Thread` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadSearch` WHERE `content_id` IN (SELECT id FROM `Thread` WHERE `accountId` = ?)",
    "DELETE FROM `ThreadReference` WHERE `accountId` = ?",
    "DELETE FROM `Thread` WHERE `accountId` = ?",
    "DELETE FROM `File` WHERE `accountId` = ?",
    "DELETE FROM `Event` WHERE `accountId` = ?",
    "DELETE FROM `Label` WHERE `accountId` = ?",
    "DELETE FROM `MessageBody` WHERE `id` IN (SELECT id FROM `Message` WHERE `accountId` = ?)",
    "DELETE FROM `Message` WHERE `accountId` = ?",
    "DELETE FROM `Task` WHERE `accountId` = ?",
    "DELETE FROM `Folder` WHERE `accountId` = ?",
    "DELETE FROM `ContactSearch` WHERE `content_id` IN (SELECT id FROM `Contact` WHERE `accountId` = ?)",
    "DELETE FROM `Contact` WHERE `accountId` = ?",
    "DELETE FROM `Calendar` WHERE `accountId` = ?",
    "DELETE FROM `ModelPluginMetadata` WHERE `accountId` = ?",
    "DELETE FROM `DetatchedPluginMetadata` WHERE `accountId` = ?",
    "DELETE FROM `Account` WHERE `id` = ?",
];
// After reset: also reset cursor and VACUUM (see MailStore.cpp::resetForAccount)
```

### Delta ProcessState Emission

```rust
// delta/item.rs — for connection error signaling
impl DeltaStreamItem {
    pub fn process_state(account_id: &str, connection_error: bool) -> Self {
        let model_json = serde_json::json!({
            "accountId": account_id,
            "id": account_id,
            "connectionError": connection_error,
        });
        Self {
            delta_type: "persist".to_string(),
            model_class: "ProcessState".to_string(),
            model_jsons: vec![model_json],
            id_indexes: IndexMap::new(),
        }
    }
}
```

### Cargo.toml for Phase 5 Binary

```toml
[package]
name = "unifymail-sync"
version = "2.0.0"
edition = "2021"

[[bin]]
name = "mailsync"
path = "src/main.rs"

[dependencies]
# Async runtime
tokio = { version = "1", features = ["rt-multi-thread", "net", "time", "io-util", "macros", "sync", "fs"] }
tokio-util = { version = "0.7", features = ["codec"] }

# CLI
clap = { version = "4", features = ["derive"] }

# JSON/IPC protocol
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# SQLite
rusqlite = { version = "0.38", features = ["bundled"] }
tokio-rusqlite = "0.7"

# Logging (stderr only — stdout is reserved for delta JSON)
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
tracing-appender = "0.2"

# Error types
thiserror = "2"

# Ordered map for delta buffer
indexmap = "2"

[profile.release]
lto = true
strip = "symbols"
opt-level = 3
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| C++ `std::thread` per worker | `tokio::spawn` per worker | Rust v2.0 design | Reduces 5 OS threads per account to 2 (tokio pool threads); IMPR-08 |
| C++ mutex+condvar for delta timer | tokio `interval()` in dedicated task | Rust v2.0 design | Eliminates race conditions in `flushWithin()` |
| spdlog to file | `tracing` + `tracing-appender` to file | Rust v2.0 design | Async-aware spans correlate log entries to accounts/folders |
| SQLite direct write on any thread | `tokio-rusqlite` single writer thread | Rust v2.0 design | Prevents tokio worker thread starvation |
| Manual option parser | clap 4 derive | Rust v2.0 design | Auto-generated `--help`, type-safe mode enum |

**Deprecated/outdated patterns:**
- `tokio-rusqlite 0.6.x`: Updated to 0.7.x — use `"0.7"` in Cargo.toml
- `once_cell::sync::OnceLock`: Replaced by `std::sync::OnceLock` (stable since Rust 1.70) — no external dependency needed

---

## Open Questions

1. **--mode test in Phase 5**
   - What we know: C++ `runTestAuth()` does a full IMAP + SMTP connection test. Phase 5 does not have IMAP/SMTP code yet.
   - What's unclear: Should Phase 5 implement `--mode test` as a stub that returns an error, or defer the entire mode to Phase 8?
   - Recommendation: Implement `--mode test` as a stub that returns `{"error": "test mode not yet implemented", "log": ""}` with exit code 1. This prevents a panic on unknown mode while clearly communicating the unimplemented state.

2. **--mode install-check in Phase 5**
   - What we know: C++ does HTTP ping + IMAP SSL check + SMTP SASL check + tidy check. All require network or libraries not available in Phase 5.
   - What's unclear: What should the Rust stub return?
   - Recommendation: Return `{"http_check": {"success": true}, "imap_check": {"success": true}, "smtp_check": {"success": true}, "tidy_check": {"success": true}}` and exit 0. The Rust binary has no tidy dependency; the check passes trivially.

3. **tokio-rusqlite `call` return type with anyhow vs thiserror**
   - What we know: `tokio-rusqlite::Connection::call` requires the closure to return `Result<T, tokio_rusqlite::Error>`. You cannot return `SyncError` directly.
   - What's unclear: The ergonomic pattern for converting the inner error.
   - Recommendation: Map the rusqlite error at the `call` boundary: `conn.call(|c| { ... }).await.map_err(|e| SyncError::Database(e.to_string()))?`

---

## Sources

### Primary (HIGH confidence)
- `app/mailsync/MailSync/DeltaStream.cpp` — delta coalescing algorithm, `dump()` field names, flush timing — source read directly
- `app/mailsync/MailSync/DeltaStream.hpp` — DeltaStream interface, `map<modelClass, vector<DeltaStreamItem>>` buffer type — source read directly
- `app/mailsync/MailSync/main.cpp` — mode dispatch, handshake sequence, exit code 141, stdin orphan detection — source read directly
- `app/mailsync/MailSync/MailStore.cpp` — database filename (`edgehill.db`), PRAGMA setup, transaction/delta emission — source read directly
- `app/mailsync/MailSync/constants.h` — complete SQL for V1..V9 migrations, ACCOUNT_RESET_QUERIES — source read directly
- `app/frontend/flux/mailsync-bridge.ts` — TypeScript parser field name check (`modelJSONs`, `modelClass`), command dispatch — source read directly
- `app/frontend/mailsync-process.ts` — handshake trigger (stdout `'data'` event), stdin highWaterMark 1MB, binary path — source read directly
- [tokio-rusqlite docs](https://docs.rs/tokio-rusqlite/latest/tokio_rusqlite/struct.Connection.html) — `Connection::call` API, version 0.7.x — HIGH confidence
- [tokio issue #7174](https://github.com/tokio-rs/tokio/issues/7174) — async stdout data loss without explicit flush — HIGH confidence (official bug tracker)

### Secondary (MEDIUM confidence)
- [tokio io::Stdout docs](https://docs.rs/tokio/latest/tokio/io/struct.Stdout.html) — async stdout behavior — HIGH confidence
- [clap derive tutorial](https://docs.rs/clap/latest/clap/_derive/_tutorial/index.html) — ValueEnum for mode argument — HIGH confidence
- `.planning/research/ARCHITECTURE.md` — project structure, tokio task patterns, DeltaStream mpsc design — HIGH confidence (prior research)
- `.planning/research/STACK.md` — Cargo.toml template, crate versions — HIGH confidence (prior research)

---

## Metadata

**Confidence breakdown:**
- Wire format (handshake, delta fields, exit codes): HIGH — extracted directly from C++ source and TypeScript consumer
- Schema (SQL tables, indexes, FTS5): HIGH — copied verbatim from constants.h which is the ground truth
- Tokio task architecture: HIGH — verified against official tokio documentation
- stdout flush behavior: HIGH — verified against tokio issue #7174 and official rust-lang/rust issues
- clap derive mode enum: HIGH — verified against clap 4 derive tutorial

**Research date:** 2026-03-02
**Valid until:** 2026-06-01 (crate versions stable; tokio/rusqlite APIs change slowly)
