# Architecture Research

**Domain:** Rust async sync engine replacing C++ mailsync binary (v2.0 milestone)
**Researched:** 2026-03-02
**Confidence:** HIGH (process model, IPC protocol, SQLite pattern from source; crate APIs from docs.rs; async patterns from official tokio docs)

---

## System Overview

The mailsync binary is a standalone process (one per account) spawned by the Electron main process. It communicates exclusively via stdin/stdout JSON lines — no shared memory, no sockets, no N-API. The Rust replacement must be a drop-in binary replacement: same command-line interface, same environment variable requirements, same stdin/stdout JSON protocol.

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Electron Main Process                          │
│  mailsync-bridge.ts                                                   │
│  - spawns one mailsync process per Account                            │
│  - writes JSON commands to stdin                                       │
│  - reads JSON delta stream from stdout                                 │
│  - reads JSON errors from stderr                                       │
└──────────────┬────────────────────────────────────┬───────────────────┘
               │ stdin (newline-delimited JSON)      │ stdout (newline-delimited JSON)
               ▼                                     ▼
┌──────────────────────────────────────────────────────────────────────┐
│                   mailsync-rust binary (one per account)              │
│                                                                       │
│  ┌─────────────────────────────────────────────────────────────────┐ │
│  │                    main() — tokio::main                          │ │
│  │  parse args → load account JSON → init store → start workers    │ │
│  └──────┬──────────────┬──────────────┬──────────────┬─────────────┘ │
│         │              │              │              │               │
│    ┌────▼────┐   ┌──────▼─────┐ ┌────▼─────┐ ┌─────▼──────┐       │
│    │  stdin  │   │ background │ │foreground│ │  cal/dav   │       │
│    │  loop   │   │  sync task │ │idle task │ │  sync task │       │
│    │(cmd     │   │(folder iter│ │(IDLE on  │ │(CalDAV/    │       │
│    │dispatch)│   │+body fetch)│ │ INBOX)   │ │ CardDAV)   │       │
│    └────┬────┘   └──────┬─────┘ └────┬─────┘ └─────┬──────┘       │
│         │              │              │              │               │
│  ┌──────▼──────────────▼──────────────▼──────────────▼──────────┐  │
│  │                    Shared State Layer                           │  │
│  │  MailStore (rusqlite, WAL mode, single writer thread)          │  │
│  │  DeltaStream (tokio::mpsc → stdout flush task)                 │  │
│  │  Account + Identity (Arc<RwLock<_>>)                           │  │
│  └──────────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────────────┘
```

### Component Responsibilities

| Component | Responsibility | Maps From (C++) |
|-----------|----------------|----------------|
| `main.rs` | Argument parsing, mode dispatch (sync/test/reset/migrate), process lifecycle | `main.cpp` |
| `stdin_loop` task | Read JSON from stdin, dispatch to command handlers, detect orphan (stdin EOF) | `runListenOnMainThread()` |
| `background_sync` task | Iterate all folders, CONDSTORE incremental sync, body fetch scheduling | `runBackgroundSyncWorker()` / `SyncWorker::syncNow()` |
| `foreground_idle` task | IDLE on primary folder, interrupt on command, run task remote phase | `runForegroundSyncWorker()` / `SyncWorker::idleCycleIteration()` |
| `cal_contacts_sync` task | CalDAV calendar sync, CardDAV contact sync, rate limiting | `runCalContactsSyncWorker()` / `DAVWorker` |
| `metadata_sync` task | Sync plugin metadata to/from identity server | `MetadataWorker` |
| `MailStore` | SQLite read/write, delta emission on save/remove, migration, transaction management | `MailStore` / `MailStoreTransaction` |
| `DeltaStream` | Buffer deltas by model class, coalesce repeated saves, flush to stdout within window | `DeltaStream` |
| `TaskProcessor` | Local (immediate DB) and remote (network) task execution | `TaskProcessor` |
| `models/` | Data models: Message, Thread, Folder, Label, Contact, Calendar, Event, Task | `MailSync/Models/` |

---

## Recommended Project Structure

```
app/mailsync-rust/
├── Cargo.toml                   # workspace root (optional) or single crate
├── Cargo.lock
├── build.rs                     # optional (no napi — not needed unless codegen)
├── src/
│   ├── main.rs                  # tokio::main, arg parse, mode dispatch, worker spawn
│   ├── account.rs               # Account + Identity models (parsed from JSON args)
│   ├── store/
│   │   ├── mod.rs               # re-exports
│   │   ├── mail_store.rs        # rusqlite connection, WAL setup, migrate, find/save/remove
│   │   ├── transaction.rs       # RAII transaction wrapper, delta accumulation
│   │   └── migrations.rs        # SQL migration scripts (one fn per migration)
│   ├── delta/
│   │   ├── mod.rs
│   │   ├── stream.rs            # DeltaStream: mpsc sender, coalesce, timed flush
│   │   └── item.rs              # DeltaStreamItem: type, modelClass, modelJSONs merge
│   ├── imap/
│   │   ├── mod.rs
│   │   ├── session.rs           # IMAP connect/auth/TLS — wraps async-imap Session
│   │   ├── sync_worker.rs       # background folder iteration, CONDSTORE, body fetch
│   │   ├── idle_worker.rs       # foreground IDLE loop, interrupt channel, task remote phase
│   │   └── mail_processor.rs    # Parse IMAPMessage → Message/Thread models, stable IDs
│   ├── smtp/
│   │   └── sender.rs            # SendDraftTask remote phase via lettre
│   ├── dav/
│   │   ├── mod.rs
│   │   ├── dav_worker.rs        # CalDAV + CardDAV sync via libdav
│   │   └── google_contacts.rs   # Google People API contacts (Gmail accounts)
│   ├── metadata/
│   │   └── worker.rs            # Metadata sync to/from identity server via reqwest
│   ├── tasks/
│   │   ├── mod.rs
│   │   ├── processor.rs         # TaskProcessor: performLocal + performRemote dispatch
│   │   └── types.rs             # Task struct, TaskStatus enum, task variant types
│   ├── models/
│   │   ├── mod.rs
│   │   ├── mail_model.rs        # MailModel trait: tableName, toJSON, version, beforeSave
│   │   ├── message.rs
│   │   ├── thread.rs
│   │   ├── folder.rs
│   │   ├── label.rs
│   │   ├── contact.rs
│   │   ├── contact_book.rs
│   │   ├── calendar.rs
│   │   ├── event.rs
│   │   └── task_model.rs        # Task (calendar task), separate from TaskProcessor's Task
│   ├── error.rs                 # SyncError enum: thiserror-derived, with retryable() + offline()
│   ├── oauth2.rs                # XOAuth2 token manager, token refresh
│   └── utils.rs                 # ID generation, sleep/wake, chunked queries
├── tests/
│   └── integration/             # Round-trip delta protocol tests
└── docs/
    └── protocol.md              # stdin/stdout JSON message format reference
```

### Structure Rationale

- **`store/` module:** Single responsibility — all SQLite access goes through MailStore. The transaction wrapper owns delta accumulation so deltas are always batched with their originating writes.
- **`delta/` module:** Separated from `store/` because the DeltaStream is a process-wide singleton that writes to stdout independently from the store's thread. It communicates with the stdout flush task via `tokio::sync::mpsc`.
- **`imap/` module:** Split into session management (connection lifecycle), sync worker (background), and idle worker (foreground). They share the same `MailStore` reference but own separate IMAP sessions — matching the C++ architecture where `bgWorker` and `fgWorker` are two separate `SyncWorker` instances each with their own `IMAPSession`.
- **`tasks/` vs `models/`:** TaskProcessor is behavior (network + DB operations); Task model is data (persisted state). Separating them avoids a circular dependency where models import the processor.
- **`error.rs` at crate root:** Every worker loop catches `SyncError` and queries `retryable()` / `offline()` to decide whether to abort or sleep. Centralizing the error type ensures consistent behavior.

---

## Architectural Patterns

### Pattern 1: Tokio Task Per Worker (Not OS Thread Per Worker)

**What:** Replace the C++ `std::thread` per worker (background sync, foreground IDLE, cal/contacts, metadata) with `tokio::task::spawn` for each. All workers share one multi-threaded tokio runtime.

**When to use:** All I/O-bound workers. The background sync and IDLE workers spend most of their time waiting on IMAP responses — they are perfect candidates for async tasks.

**Trade-offs:**
- Pro: Fewer OS threads (default tokio runtime uses `num_cpus` threads; C++ spawns 4-5 per account).
- Pro: Worker communication via `tokio::sync::watch` / `mpsc` channels instead of mutex+condvar.
- Con: SQLite writes must go to a dedicated single-writer task (see Pattern 2).
- Con: Blocking IMAP operations (if any synchronous calls remain) must use `spawn_blocking`.

**Example:**
```rust
// main.rs — mode "sync"
let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
let store = Arc::new(MailStore::open(&config_dir, &account.id)?);
let delta = Arc::new(DeltaStream::new());

tokio::spawn(stdin_loop(account.clone(), store.clone(), delta.clone(), shutdown_rx.resubscribe()));
tokio::spawn(background_sync(account.clone(), store.clone(), delta.clone(), shutdown_rx.resubscribe()));
// foreground idle task starts AFTER background completes first pass (same as C++)
tokio::spawn(cal_contacts_sync(account.clone(), store.clone(), delta.clone(), shutdown_rx.resubscribe()));
tokio::spawn(metadata_sync(account.clone(), store.clone(), delta.clone(), shutdown_rx.resubscribe()));

// stdout flush task — drains the delta channel and writes to stdout
tokio::spawn(delta_flush_task(delta.clone()));
```

### Pattern 2: Single-Writer SQLite via Dedicated Task

**What:** Route all SQLite writes through a single dedicated task that owns the write connection. Readers use a second connection opened in WAL mode. This mirrors SQLite's WAL concurrency model and avoids the "database is locked" errors that plague multi-threaded write patterns.

**When to use:** Any time multiple async tasks need to write to the database. This is mandatory — tokio-rusqlite's `Connection` (one-thread-per-connection model) must not be shared across tasks without explicit care.

**Trade-offs:**
- Pro: No write-write lock contention. WAL allows concurrent reads during writes.
- Pro: Transaction batching is natural — all deltas accumulate in the writer task.
- Con: Writes require a channel round-trip (slightly higher latency than direct call).
- Con: One additional OS thread for the write connection (tokio-rusqlite spawns one thread per `Connection`).

**Implementation approach:**
```rust
// store/mail_store.rs
use tokio_rusqlite::Connection;

pub struct MailStore {
    // Writer: one tokio-rusqlite Connection (one background thread)
    writer: Connection,
    // Reader: separate Connection for find() queries from any task
    // In WAL mode, readers don't block writers
    reader: Connection,
}

impl MailStore {
    pub async fn open(path: &Path) -> Result<Self> {
        let writer = Connection::open(path).await?;
        writer.call(|conn| {
            conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
            Ok(())
        }).await?;
        let reader = Connection::open(path).await?;
        Ok(Self { writer, reader })
    }

    pub async fn save<M: MailModel>(&self, model: &M) -> Result<()> {
        // All writes go through writer connection
        self.writer.call(move |conn| {
            // execute upsert, emit delta to DeltaStream channel
            Ok(())
        }).await
    }

    pub async fn find<M: MailModel>(&self, query: &Query) -> Result<Option<M>> {
        // Reads use reader connection — concurrent with writes in WAL mode
        self.reader.call(move |conn| {
            // execute SELECT
            Ok(None)
        }).await
    }
}
```

### Pattern 3: DeltaStream as Tokio Channel + Timed Flush Task

**What:** Replace the C++ DeltaStream's condition-variable timer with a tokio channel. Each worker sends `DeltaStreamItem` values into an `mpsc::unbounded_channel`. A dedicated flush task receives items, coalesces them (same merge logic as C++), and flushes to stdout every 500ms or when explicitly triggered.

**When to use:** All delta emission. This is the core IPC mechanism — every model save/remove produces a delta.

**Trade-offs:**
- Pro: No mutex+condvar complexity. The tokio channel is the synchronization primitive.
- Pro: The flush task owns stdout exclusively — no interleaving of JSON lines.
- Pro: Unbounded channel means senders never block (workers are never slowed by stdout backpressure).
- Con: Memory grows if stdout is slow and deltas accumulate. Bound the channel if this becomes a concern.

**Example:**
```rust
// delta/stream.rs
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration};
use std::io::Write;

pub struct DeltaStream {
    tx: mpsc::UnboundedSender<DeltaStreamItem>,
}

impl DeltaStream {
    pub fn emit(&self, item: DeltaStreamItem) {
        let _ = self.tx.send(item); // fire-and-forget; channel never closes while process runs
    }
}

// Launched as a dedicated tokio task
pub async fn delta_flush_task(mut rx: mpsc::UnboundedReceiver<DeltaStreamItem>) {
    let flush_interval = Duration::from_millis(500);
    let mut buffer: IndexMap<String, Vec<DeltaStreamItem>> = IndexMap::new();

    loop {
        // Collect available items without blocking longer than flush_interval
        let deadline = tokio::time::Instant::now() + flush_interval;
        loop {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(item)) => coalesce_into(&mut buffer, item),
                Ok(None) => return, // channel closed — process shutting down
                Err(_) => break,    // timeout reached — flush now
            }
        }
        flush_buffer(&buffer);
        buffer.clear();
    }
}

fn flush_buffer(buffer: &IndexMap<String, Vec<DeltaStreamItem>>) {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    for items in buffer.values() {
        for item in items {
            writeln!(out, "{}", item.to_json()).unwrap();
        }
    }
    out.flush().unwrap();
}
```

### Pattern 4: IMAP Connection Lifecycle with Reconnect Loop

**What:** Each IMAP worker (background, foreground) wraps its entire work loop in a reconnect outer loop. On any `SyncError` that is `retryable()`, the connection is dropped and re-established after a backoff. The foreground idle worker uses `tokio::select!` to simultaneously wait on IDLE notifications and an interrupt channel signal.

**When to use:** Any IMAP operation. Network connections fail. The reconnect loop is the primary reliability mechanism.

**Trade-offs:**
- Pro: Worker failure is contained. A broken connection restarts the worker, not the process.
- Pro: `tokio::select!` in the IDLE loop allows clean interruption for task execution (matching C++ `idleInterrupt()`).
- Con: Must carefully cancel IDLE with the `Done` command before reconnecting — failure to send `DONE` leaves the server's connection in an undefined state.

**Example (foreground idle worker):**
```rust
// imap/idle_worker.rs
pub async fn run_foreground_idle(
    account: Arc<Account>,
    store: Arc<MailStore>,
    delta: Arc<DeltaStream>,
    mut interrupt_rx: watch::Receiver<()>,
    mut shutdown_rx: broadcast::Receiver<()>,
) {
    loop {
        match run_idle_cycle(&account, &store, &delta, &mut interrupt_rx, &mut shutdown_rx).await {
            Ok(()) => return, // clean shutdown
            Err(e) if e.is_offline() => {
                delta.emit(DeltaStreamItem::connection_error(&account.id, true));
                sleep(Duration::from_secs(120)).await;
            }
            Err(e) if e.is_retryable() => {
                tracing::warn!("IMAP foreground error, reconnecting: {}", e);
                sleep(Duration::from_secs(5)).await;
            }
            Err(e) => {
                tracing::error!("IMAP foreground fatal error: {}", e);
                std::process::abort();
            }
        }
    }
}

async fn run_idle_cycle(
    account: &Account,
    store: &MailStore,
    delta: &DeltaStream,
    interrupt_rx: &mut watch::Receiver<()>,
    shutdown_rx: &mut broadcast::Receiver<()>,
) -> Result<(), SyncError> {
    let mut session = ImapSession::connect(account).await?;
    let task_processor = TaskProcessor::new(account.clone(), store.clone());

    // Run task remote phases accumulated since last cycle
    task_processor.run_pending_remote(&mut session).await?;

    loop {
        let mut idle = session.idle();
        idle.init().await?;

        tokio::select! {
            // IDLE notification from server — new mail or flag change
            result = idle.wait() => {
                idle.done().await?;
                match result? {
                    // Process the update
                    _ => { /* fetch changed messages */ }
                }
            }
            // Interrupt signal from stdin loop (queue-task, wake-workers, need-bodies)
            _ = interrupt_rx.changed() => {
                idle.done().await?;
                task_processor.run_pending_remote(&mut session).await?;
            }
            // Graceful shutdown
            _ = shutdown_rx.recv() => {
                idle.done().await?;
                return Ok(());
            }
        }
    }
}
```

### Pattern 5: Task Processor Split (Local / Remote)

**What:** Replicate the C++ task lifecycle exactly. `performLocal` runs synchronously (in the write task) immediately when a `queue-task` command arrives on stdin. `performRemote` runs in the foreground IDLE worker thread when interrupted, using the live IMAP session.

**When to use:** All email operations that modify state (send, move, flag, delete, label, contact sync, event sync).

**Trade-offs:**
- Pro: UI gets immediate feedback (local phase updates the DB and emits a delta) before the network round-trip.
- Pro: Matches the Electron app's existing task state machine — no changes needed to the TypeScript task system.
- Con: The local→remote handoff requires coordinating two async tasks. Use a `tokio::sync::Notify` (equivalent to C++ condvar wakeup) to signal the foreground worker to interrupt its IDLE.

**Example (stdin loop side):**
```rust
// Receive queue-task from stdin
if msg_type == "queue-task" {
    let task = Task::from_json(&packet["task"])?;
    // Local phase: runs on writer task, produces immediate delta
    store.run_in_transaction(|txn| {
        task_processor.perform_local(&task, txn)
    }).await?;
    // Signal foreground worker to wake from IDLE and run remote phase
    foreground_interrupt.notify_one();
}
```

### Pattern 6: Stdin EOF as Orphan Detection

**What:** The C++ engine detects orphan state by checking `cin.good()`. If stdin has been closed for more than 30 seconds, the process exits. In Rust, `tokio::io::stdin()` returns `Ok(0)` (EOF) when the parent closes stdin. Read this as a shutdown signal.

**When to use:** Always — this is how the Electron app kills the sync process when an account is removed or the app exits.

**Example:**
```rust
// stdin_loop task
use tokio::io::{AsyncBufReadExt, BufReader};

pub async fn stdin_loop(/* ... */) {
    let stdin = tokio::io::stdin();
    let mut lines = BufReader::new(stdin).lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) if !line.is_empty() => {
                dispatch_command(&line, /* ... */).await;
            }
            Ok(Some(_)) => {} // empty line — ignore
            Ok(None) => {
                // EOF — parent process closed stdin
                tracing::info!("stdin closed — initiating graceful shutdown");
                shutdown_tx.send(()).ok();
                return;
            }
            Err(e) => {
                tracing::warn!("stdin read error: {}", e);
                // Give it 30 seconds before treating as orphan (matches C++ behavior)
            }
        }
    }
}
```

### Pattern 7: Error Type Hierarchy

**What:** Use `thiserror` for the `SyncError` enum that workers match against. `SyncError` carries `is_retryable()` and `is_offline()` methods that control reconnect behavior. Wrap external crate errors using `#[from]`.

**When to use:** Throughout. Every worker function returns `Result<_, SyncError>`.

**Example:**
```rust
// error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("IMAP error: {0}")]
    Imap(#[from] async_imap::error::Error),

    #[error("Network error (offline): {0}")]
    Offline(String),

    #[error("Authentication error: {0}")]
    Auth(String),

    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Fatal: {0}")]
    Fatal(String),
}

impl SyncError {
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Imap(_) | Self::Offline(_) | Self::Protocol(_))
    }

    pub fn is_offline(&self) -> bool {
        matches!(self, Self::Offline(_))
    }
}
```

---

## Data Flow

### Command Flow: queue-task

```
Electron (mailsync-bridge.ts)
  stdin.write(JSON.stringify({ type: "queue-task", task: {...} }) + "\n")
    |
    v
stdin_loop task (tokio)
  deserialize JSON → Task struct
    |
    v
TaskProcessor::perform_local(task, &mut txn)
  modify DB (Message/Thread flags, etc.)
  txn.commit() → DeltaStream.emit(DeltaStreamItem::persist(...))
    |
    v
foreground_interrupt.notify_one()          // wake IDLE worker
    |
    v  [later, on foreground task]
TaskProcessor::perform_remote(task, &mut imap_session)
  network operation (IMAP UID STORE, APPEND, etc.)
  update task status in DB → emit delta
    |
    v
DeltaStream channel → delta_flush_task
  coalesce by modelClass → stdout.write_all(json_line)
    |
    v
Electron stdout reader (mailsync-bridge.ts)
  DatabaseStore.trigger() → UI updates
```

### Delta Emission Flow

```
Any async task calls store.save(model)
  MailStore (write connection thread via tokio-rusqlite)
    SQL upsert
    delta_tx.send(DeltaStreamItem { type: "persist", modelClass, modelJSONs: [model.to_json()] })
      |
      v [unbounded mpsc]
delta_flush_task
  receives item, calls coalesce_into(buffer, item)
    - if buffer[modelClass].last().type == item.type:
        merge modelJSONs (upsert by id, merge keys)
    - else: push new item
  every 500ms: flush buffer to stdout as newline-delimited JSON
```

### IMAP Background Sync Flow

```
background_sync task starts
  ImapSession::connect(account)   // TLS or STARTTLS on port 993/143
  session.select("INBOX")
  session.list("", "*")           // discover all folders
    → store.save(folder) for each new/changed folder

  loop over folders:
    folder_status = session.status(folder)
    if server supports CONDSTORE:
      session.select_condstore(folder)  // enables CHANGEDSINCE modifiers
      fetch uids changed since lastHighestModSeq
    else:
      UID FETCH ALL (FLAGS) for full flag reconcile

    for new UIDs:
      UID FETCH (RFC822.HEADER) in batches of 100
        → MailProcessor::parse_message(imap_msg) → Message + Thread
        → store.save(message), store.save(thread)

    schedule body fetch for recent messages:
      UID FETCH (BODY[]) for messages < maxAge
        → store.save(message_with_body)

  store.save(folder, { localStatus: "up-to-date" })

  sleep(120s) or until woken by wake-workers command
  loop
```

### IMAP Foreground IDLE Flow

```
foreground_idle task (starts AFTER background completes first folder pass)
  ImapSession::connect(account)
  select primary folder (Inbox or All Mail for Gmail)

  loop:
    run pending remote tasks from task queue
    idle.init()  // send IDLE command to server
    tokio::select!:
      | idle.wait() fires      → idle.done(); fetch new/changed messages
      | interrupt_rx changes   → idle.done(); run task remote phase
      | shutdown_rx fires      → idle.done(); return

    on reconnect-worthy error: drop session, sleep 5s, reconnect
```

---

## Integration Points

### Electron IPC Protocol (Unchanged from C++)

The Rust binary must maintain exact wire compatibility with `mailsync-bridge.ts`.

**stdin commands (Electron → Rust):**

| Command type | JSON shape | Handler |
|---|---|---|
| `queue-task` | `{ type, task: { taskType, ... } }` | `TaskProcessor::perform_local` → interrupt foreground |
| `cancel-task` | `{ type, taskId }` | `TaskProcessor::cancel` |
| `wake-workers` | `{ type }` | `broadcast::send(Wake)` to bg + fg workers |
| `need-bodies` | `{ type, ids: [string] }` | Queue body fetch in foreground worker |
| `sync-calendar` | `{ type }` | Trigger one-shot CalDAV sync |
| `detect-provider` | `{ type, email, requestId }` | Look up provider, write result to stdout |
| `query-capabilities` | `{ type, requestId }` | Report IDLE/CONDSTORE status to stdout |
| `subscribe-folder-status` | `{ type, folderIds, requestId }` | Query folder localStatus, write to stdout |

**stdout deltas (Rust → Electron):**

| Delta type | When emitted |
|---|---|
| `{ type: "persist", modelClass, modelJSONs }` | Model saved (Message, Thread, Folder, ...) |
| `{ type: "unpersist", modelClass, modelJSONs }` | Model removed |
| `{ type: "persist", modelClass: "ProcessState", ... }` | Connection error begin/end |
| `{ type: "persist", modelClass: "ProcessAccountSecretsUpdated", ... }` | OAuth token refresh |
| `{ type: "provider-result", requestId, provider }` | Response to detect-provider |
| `{ type: "capabilities-result", requestId, capabilities }` | Response to query-capabilities |
| `{ type: "folder-status", requestId, statuses }` | Response to subscribe-folder-status |

### New vs Modified Components

| Component | Status | Notes |
|---|---|---|
| `app/mailsync/` (entire C++ source) | DELETED | Replaced by `app/mailsync-rust/` |
| `app/mailsync-rust/` | NEW | Rust cargo project |
| `app/mailsync-rust/src/main.rs` | NEW | Binary entry point |
| All Rust source modules | NEW | See project structure above |
| `app/src/browser/mailsync-bridge.ts` | UNCHANGED | Reads same stdin/stdout protocol |
| `app/src/browser/mailsync-process.ts` | UNCHANGED | Spawns binary by path; path may change |
| `app/package.json` mailsync binary path | MODIFIED | Point to new Rust binary output path |
| SQLite database schema | UNCHANGED | WAL mode, fat-row JSON design preserved |
| `app/mailsync/Vendor/` (libetpan, mailcore2) | DELETED | No longer needed |

### Build Integration

The Rust binary is compiled via `cargo build --release` (or `cargo build` for debug). The Electron build process needs to invoke this and copy the resulting binary into the app package alongside the existing mailsync binary location.

```bash
# Development
cargo build --manifest-path app/mailsync-rust/Cargo.toml

# Release (production)
cargo build --manifest-path app/mailsync-rust/Cargo.toml --release

# Cross-compile for macOS universal (requires cargo-lipo or lipo post-processing)
cargo build --target x86_64-apple-darwin --release
cargo build --target aarch64-apple-darwin --release
lipo -create -output mailsync target/x86_64-apple-darwin/release/mailsync target/aarch64-apple-darwin/release/mailsync
```

The binary name must remain `mailsync` (or whatever `mailsync-process.ts` expects) so no path changes are required in the bridge.

---

## Crate Dependency Decisions

| Crate | Version | Purpose | Why |
|---|---|---|---|
| `tokio` | 1.x (rt-multi-thread, sync, io, time, fs, macros) | Async runtime | Industry standard; required for async-imap, lettre, reqwest |
| `async-imap` | 0.11.2 (runtime-tokio) | IMAP client | Actively maintained; IDLE + CONDSTORE support confirmed (v0.6.0+); used by deltachat |
| `imap-proto` | latest | IMAP response parsing for CONDSTORE/QRESYNC raw frames | Low-level parser when async-imap's high-level API is insufficient |
| `lettre` | 0.11.x (tokio1, rustls-tls) | SMTP send (SendDraftTask) | Same crate as v1.0; XOAUTH2 built-in |
| `tokio-rustls` | 0.26.x | TLS for IMAP + SMTP | No OpenSSL dependency; same as v1.0 |
| `rustls-platform-verifier` | 0.6.x | OS certificate trust | Same as v1.0 |
| `rusqlite` | 0.32.x (bundled) | SQLite access | Direct C bindings; bundled feature avoids system sqlite3 version mismatch across Linux distros |
| `tokio-rusqlite` | 0.6.x | Async bridge for rusqlite | Single-writer-thread model; `Connection::call` sends closures via mpsc |
| `reqwest` | 0.12.x (rustls-tls, json) | HTTP for CalDAV/CardDAV/metadata | Async, rustls-backed, widely used |
| `libdav` | 0.10.x | CalDAV + CardDAV client | Tokio-native; wraps reqwest; provides CalDAV REPORT + CardDAV sync |
| `serde` + `serde_json` | 1.x | JSON for protocol, models, delta stream | Industry standard |
| `tracing` | 0.1.x | Structured async logging | Tokio-native; replaces spdlog; `#[instrument]` for async spans |
| `tracing-appender` | 0.2.x | Non-blocking rotating log file | Replaces spdlog rotating file sink |
| `thiserror` | 2.x | SyncError enum derivation | Structured errors with `is_retryable()`/`is_offline()` methods |
| `hickory-resolver` | 0.25.x (tokio-runtime) | MX lookups (detect-provider command) | Same as v1.0 |
| `base64` | 0.22.x | XOAUTH2 token encoding | Required for IMAP AUTHENTICATE XOAUTH2 |
| `indexmap` | 2.x | Ordered map for delta buffer by modelClass | Preserves insertion order for deterministic stdout output |
| `uuid` | 1.x | Stable message ID generation | Replaces C++ hash-based ID scheme |
| `chrono` | 0.4.x | Date/time parsing (RFC 2822 email dates) | Comprehensive date handling |

**Crates to explicitly avoid:**

| Avoid | Why |
|---|---|
| `openssl` / `native-tls` | Conflicts with Electron's BoringSSL on Linux (same issue as v1.0) |
| `async-std` | Conflicts with tokio runtime (async-imap must use `runtime-tokio` feature) |
| `imap` (sync, jonhoo) | Synchronous — requires `spawn_blocking` wrapper; async-imap is preferred |
| `sqlx` | Async-first but brings migration framework overhead; rusqlite direct control is cleaner for this schema |
| `diesel` | ORM is overkill for a fat-row JSON schema |
| `once_cell` / `lazy_static` | Unnecessary since Rust 1.70 (`OnceLock`, `LazyLock` in std) |

---

## Scaling Considerations

This is a desktop application. Scaling means: multiple accounts, slow networks, large mailboxes.

| Concern | At 1-3 accounts (typical) | At 10+ accounts | At 100k messages per mailbox |
|---|---|---|---|
| OS threads | ~2-3 (tokio pool + 1-2 rusqlite writer threads) | ~6-8 (tokio pool shared; rusqlite thread per account DB) | No change — I/O bound |
| Memory | ~20MB per account (SQLite page cache + buffers) | Monitor; rusqlite page cache is the main consumer | Increase SQLite cache_size pragma |
| Delta backpressure | None — unbounded channel | None | Consider bounded channel with back-pressure if stdout blocks |
| IMAP connection count | 2 per account (bg + fg) | 20+ connections — may hit server limits | Reduce to 1 IMAP connection; serialize background + IDLE |
| Body download | Runs after flag sync — acceptable lag | Same | Configurable age threshold (already in C++) |

**First bottleneck:** SQLite write contention when multiple accounts are syncing simultaneously. Mitigation: each account has its own SQLite database file (current design), so accounts never share a writer.

**Second bottleneck:** IMAP server connection limits. Mitigation: add startup delay (already in C++ as `account->startDelay()`), implement in Rust as `tokio::time::sleep(Duration::from_secs(account.start_delay))` before first IMAP connect.

---

## Anti-Patterns

### Anti-Pattern 1: Blocking Operations on Tokio Worker Threads

**What people do:** Call `rusqlite::Connection::execute` directly inside an `async fn` without using `tokio-rusqlite`'s `call` abstraction, or call a synchronous HTTP library inside an async context.

**Why it's wrong:** Blocks one of the tokio worker threads. With 4 worker threads and 4 concurrent IMAP operations, all threads can block simultaneously, preventing any async progress including the stdin loop.

**Do this instead:** Use `tokio-rusqlite::Connection::call` for all SQLite operations. Use `reqwest` (async) not `ureq` (sync) for HTTP. Use `tokio::task::spawn_blocking` only as a last resort for unavoidably synchronous third-party code.

### Anti-Pattern 2: Writing to Stdout from Multiple Tasks

**What people do:** Call `println!()` or `writeln!(stdout, ...)` from the stdin loop, the background sync worker, and the foreground worker concurrently.

**Why it's wrong:** `println!()` acquires a mutex on stdout, but multiple tasks printing concurrently can still interleave JSON lines if they construct multi-write sequences. A JSON line printed as two separate writes can be interleaved with another task's write.

**Do this instead:** Route all stdout writes through the single `delta_flush_task`. Workers send items to the `mpsc` channel; only the flush task calls `write_all` + `flush` on stdout. Exception: synchronous one-shot responses (`detect-provider`, `query-capabilities`) can use `println!()` only if guaranteed to complete before async tasks start — safer to route through the same channel with a `Priority::Immediate` flush signal.

### Anti-Pattern 3: Sharing an IMAP Session Across Tasks

**What people do:** Put the `async-imap::Session` in an `Arc<Mutex<Session>>` and share it between the background sync task and foreground IDLE task.

**Why it's wrong:** IMAP is strictly sequential at the protocol level — commands must be issued and responses received in order. Concurrent use of a shared session produces interleaved commands and protocol errors. Additionally, `async-imap::Session` is not `Send + Sync`.

**Do this instead:** Two separate IMAP sessions (two separate TCP connections), one per worker — exactly as the C++ engine did with two `IMAPSession` objects.

### Anti-Pattern 4: Aborting on All Errors

**What people do:** Use `unwrap()` or `expect()` throughout worker code, or `std::process::abort()` on any `Err`.

**Why it's wrong:** The C++ engine only aborts on non-retryable errors. Network blips, temporary server unavailability, and authentication expiry are retryable. Aborting on these causes the Electron app to continuously respawn the worker.

**Do this instead:** Use the `SyncError::is_retryable()` gate in every worker loop. Retryable errors get exponential backoff sleep; non-retryable errors (like database corruption or authentication permanently revoked) call `std::process::exit(1)` to signal the bridge that the account is in an error state.

### Anti-Pattern 5: Omitting the Delta Batching Window

**What people do:** Write each model save immediately to stdout as a separate JSON line without any batching.

**Why it's wrong:** The Electron UI processes each stdout line as a separate database trigger. Initial sync of a 10,000-message mailbox would trigger 10,000 separate React re-renders if each message is flushed individually. The C++ engine uses a 500ms batching window precisely to coalesce these into grouped deltas.

**Do this instead:** Implement the `delta_flush_task` with the 500ms window. During initial sync, all messages from one batch of fetched headers are coalesced into a single `{ type: "persist", modelClass: "Message", modelJSONs: [...100 messages...] }` delta.

---

## Build Order (Phase Dependencies)

The v2.0 rewrite has clear build dependencies. Each phase must be functional before the next starts because earlier phases provide the foundational infrastructure.

```
Phase 1: Core Infrastructure
  models/mail_model.rs (trait)
  models/message.rs, thread.rs, folder.rs, label.rs
  store/mail_store.rs + transaction.rs (rusqlite + WAL)
  store/migrations.rs (schema matches existing SQLite DB)
  delta/item.rs + stream.rs (coalesce logic + mpsc channel)
  error.rs (SyncError)
  main.rs (arg parse, mode dispatch, "migrate" + "reset" modes)

  Testable: cargo run -- --mode migrate; verify schema created

Phase 2: stdin/stdout Protocol + Task Infrastructure
  stdin_loop (command dispatch, orphan detection)
  delta_flush_task (timed flush to stdout)
  tasks/types.rs + processor.rs (perform_local stubs only)
  account.rs, oauth2.rs

  Testable: pipe JSON commands in, verify JSON deltas out; smoke test
  with Electron bridge disconnected

Phase 3: IMAP Sync Workers
  imap/session.rs (connect, TLS, auth: password + XOAUTH2)
  imap/mail_processor.rs (parse IMAPMessage → Message/Thread)
  imap/sync_worker.rs (background: folder list, CONDSTORE, body fetch)
  imap/idle_worker.rs (foreground: IDLE + interrupt loop)
  tasks/processor.rs (perform_remote for IMAP tasks: move, flag, send)
  smtp/sender.rs (SendDraftTask via lettre)

  Testable: full end-to-end sync with live IMAP account in Electron dev mode

Phase 4: CalDAV/CardDAV + Metadata
  dav/dav_worker.rs (libdav CalDAV + CardDAV)
  dav/google_contacts.rs (Google People API)
  metadata/worker.rs (identity server polling)
  models/calendar.rs, event.rs, contact.rs, contact_book.rs

  Testable: calendar events and contacts appear in Electron UI

Phase 5: Modes, Packaging, Cross-Platform CI
  main.rs "test" mode (runTestAuth equivalent — IMAP + SMTP validation)
  main.rs "install-check" mode
  Cross-platform cargo builds (Windows MSVC, macOS universal, Linux x64/arm64)
  Binary size optimization (cargo bloat, LTO, strip)
  C++ source deletion
```

---

## Sources

- C++ source read directly: `app/mailsync/MailSync/main.cpp`, `DeltaStream.cpp/.hpp`, `MailStore.hpp`, `SyncWorker.hpp`, `TaskProcessor.hpp`, `DAVWorker.hpp`, `MetadataWorker.hpp` — HIGH confidence (primary source)
- `app/mailsync/CLAUDE.md` — Architecture description, reactive data flow — HIGH confidence
- `CLAUDE.md` (project root) — IPC protocol, task system, sync engine communication diagram — HIGH confidence
- [Tokio graceful shutdown](https://tokio.rs/tokio/topics/shutdown) — broadcast + mpsc shutdown pattern — HIGH confidence
- [Tokio channels](https://tokio.rs/tokio/tutorial/channels) — mpsc channel architecture — HIGH confidence
- [async-imap docs](https://docs.rs/async-imap/latest/async_imap/) — extensions module: IDLE, CONDSTORE (v0.6.0+), QUOTA, ID, METADATA, COMPRESS — HIGH confidence
- [async-imap CHANGELOG](https://github.com/chatmail/async-imap/blob/main/CHANGELOG.md) — CONDSTORE added v0.6.0, version 0.11.2 (2026-02-10) confirmed latest — HIGH confidence
- [tokio-rusqlite docs](https://docs.rs/tokio-rusqlite/latest/tokio_rusqlite/) — one-thread-per-connection, mpsc+oneshot architecture — HIGH confidence
- [libdav docs](https://docs.rs/libdav/latest/libdav/) — CalDAV + CardDAV, tokio 1.x, version 0.10.2 — HIGH confidence
- [tracing crate](https://docs.rs/tracing) — structured async logging, #[instrument] — HIGH confidence
- [thiserror](https://docs.rs/thiserror) — error derivation for structured SyncError — HIGH confidence
- [tokio-retry docs](https://docs.rs/tokio-retry) — exponential backoff strategy — MEDIUM confidence
- [RFC 7162](https://datatracker.ietf.org/doc/html/rfc7162) — CONDSTORE + QRESYNC protocol specification — HIGH confidence

---

*Architecture research for: Rust async mailsync engine (v2.0 milestone) — replacement for C++ mailsync binary*
*Researched: 2026-03-02*
