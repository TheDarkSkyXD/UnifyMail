# Phase 8: Foreground IDLE and Task Execution - Research

**Researched:** 2026-03-02
**Domain:** async-imap IDLE extension, lettre MIME message construction, tokio channel-based task interruption, IMAP task execution (flags/folder/draft), crash recovery via SQLite state reset
**Confidence:** HIGH

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| IDLE-01 | Foreground IDLE monitoring on primary folder with 29-minute re-IDLE loop | `idle.wait_with_timeout(Duration::from_secs(25 * 60))` returns `(future, StopSource)`; re-IDLE loop wraps in `loop {}` calling `idle.done().await` then restarting; 25-min interval is safe margin for 29-min server timeout |
| IDLE-02 | IDLE interrupted on task arrival via internal tokio channel | `tokio::sync::mpsc::channel` sends interrupt signal; spawned task calls `drop(interrupt)` (drops the StopSource) on recv; IDLE future resolves as `ManualInterrupt` |
| IDLE-03 | Separate IMAP connection for IDLE (not shared with background sync session) | Two independent `Session<T>` instances from two independent TCP+TLS connections; IMAP protocol is single-threaded per connection; concurrency is via `tokio::spawn` on separate tasks |
| SEND-01 | SMTP send with TLS, STARTTLS, and clear connections via lettre | `AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous()` with `Tls::Wrapper` (TLS), `Tls::Required` (STARTTLS), `Tls::None` (clear); same pattern as Phase 3 |
| SEND-02 | SMTP authentication with password and XOAUTH2 | `Credentials::new()` + `Mechanism::Plain` for password; `Mechanism::Xoauth2` with `.authentication(vec![Mechanism::Xoauth2])` for OAuth2; confirmed from lettre source |
| SEND-03 | MIME message construction from draft JSON (multipart, attachments, inline images) | **BLOCKER RESOLVED**: `MultiPart::mixed()` + `MultiPart::alternative()` + `MultiPart::related()` + `Attachment::new_inline(cid)` all confirmed in lettre 0.11; complete example documented below |
| SEND-04 | SMTP connection timeout of 15 seconds prevents indefinite hang | `.timeout(Some(Duration::from_secs(15)))` per-command + outer `tokio::time::timeout()` wrapping full send; same pattern as Phase 3 |
| TASK-01 | Local phase executes immediately on queue-task receipt (DB write + delta emission) | `tokio_rusqlite::Connection::call()` closure for synchronous DB write inside async context; delta emitted via stdout mpsc channel; executes before remote phase begins |
| TASK-02 | Remote phase executes on foreground IMAP/SMTP connection | After local phase completes, task enum dispatched to IMAP/SMTP handler; IDLE interrupted via channel before remote phase starts |
| TASK-03 | All task types supported: SendDraft, DestroyDraft, ChangeLabels, ChangeFolder, ChangeStarred, ChangeUnread, SyncbackMetadata, SyncbackEvent, and contact/calendar tasks | Each maps to specific async-imap Session methods: uid_store, uid_mv, uid_copy+uid_expunge, append, plus lettre send; exact IMAP command mapping documented |
| TASK-04 | Startup reset of tasks stuck in remote state (crash recovery) | On startup: `UPDATE tasks SET status = 'local' WHERE status = 'remote'` via tokio-rusqlite call(); re-queues tasks for execution |
| TASK-05 | Runtime expiry of completed tasks after configurable period | `DELETE FROM tasks WHERE status = 'complete' AND completed_at < datetime('now', '-N seconds')` run on timer interval |
| IMPR-07 | Improved body sync progress updates emitted to UI during large syncs | Emit delta per-message during body fetch loop rather than batching all at once; use stdout mpsc channel for incremental flush |
</phase_requirements>

---

## Summary

Phase 8 assembles the foreground IDLE worker, task processor, and SMTP send capability into a complete user-facing pipeline. The three major technical challenges are: (1) IDLE with reliable task interruption and re-IDLE loop, (2) MIME message construction for the full range of email types including inline images, and (3) a task processor that handles 13+ task types with two-phase execution and crash recovery.

The **lettre MIME blocker from STATE.md is fully resolved**: lettre 0.11 supports multipart/alternative (HTML + plain text), multipart/related (inline images with CID), and multipart/mixed (file attachments) through a well-documented nesting pattern. `Attachment::new_inline(cid_string)` produces `Content-ID: <cid>` and `Content-Disposition: inline` headers. The correct nesting order is `mixed() → alternative() → related()` for the full structure.

The **async-imap IDLE API** uses a two-return-value pattern: `idle.wait_with_timeout(duration)` returns `(future, StopSource)`. Dropping the `StopSource` triggers `ManualInterrupt`, which causes the future to resolve immediately. For task interruption, a `tokio::sync::mpsc` channel delivers the signal to a spawned task that drops the `StopSource`. After the IDLE future resolves (for any reason), `idle.done().await` sends DONE to the server and reclaims the `Session`. The re-IDLE loop then re-enters `session.idle()` for the next cycle.

The **task processor** uses an enum over all task types, dispatched in a `match` block to per-type handlers. The two-phase design (local DB write first, then remote IMAP/SMTP) mirrors the C++ pattern exactly. Crash recovery is a startup SQL query that resets `remote` tasks to `local` state.

**Primary recommendation:** Implement the foreground worker as a single `tokio::spawn` task that owns an IMAP session and runs the IDLE + task dispatch loop. Use a bounded `tokio::sync::mpsc` channel (capacity 32) for task delivery from the stdin reader to the foreground worker.

---

## Standard Stack

### Core (all already in Cargo.toml from Phases 2–3)
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `async-imap` | 0.11.2 | IMAP session + IDLE extension | Phase 2 established; `session.idle()` returns Handle with `wait_with_timeout` |
| `lettre` | 0.11.19 | SMTP transport + MIME construction | Phase 3 established; `builder` feature enables MultiPart/Attachment API |
| `tokio` | 1.x | Async runtime + channels | Already present; `tokio::sync::mpsc` for IDLE interrupt channel |
| `tokio-rusqlite` | latest | Async SQLite access | Phase 6 established; `call()` closure for task state updates |
| `stop-token` | transitive | StopSource/StopToken used by async-imap IDLE | Pulled in by async-imap; not added directly |

### No New Dependencies Required
Phase 8 uses only libraries already in the dependency tree from Phases 2–7. No new crates are needed.

**Cargo.toml additions required for Phase 8:**
```toml
# lettre must include "builder" feature for Message/MultiPart/Attachment API
# (confirm it is in features list from Phase 3 — it is a default feature but Phase 3 uses default-features=false)
lettre = { version = "0.11", default-features = false, features = [
    "builder",           # REQUIRED for Phase 8: Message, MultiPart, Attachment, SinglePart
    "smtp-transport",
    "tokio1",
    "tokio1-rustls-tls", # confirmed feature name for lettre 0.11
    "hostname",
] }
```

**Critical:** `"builder"` feature must be in the lettre feature list. Phase 3 used `default-features = false` which excludes `"builder"` from defaults. The Phase 3 Cargo.toml must be updated to add `"builder"` before Phase 8 code can use `Message::builder()`, `MultiPart`, or `Attachment`.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `tokio::sync::mpsc` for interrupt | `tokio::sync::oneshot` | oneshot is correct for single task delivery; mpsc allows queuing multiple tasks; mpsc is better for task queue |
| `idle.wait_with_timeout(25min)` re-IDLE loop | Single `idle.wait()` (29min default) | Default 29min matches server timeout exactly; 25min provides a safety margin; prefer explicit 25min |
| `uid_mv` for folder move | `uid_copy` + `uid_store \Deleted` + `uid_expunge` | `uid_mv` is atomic IMAP MOVE extension (RFC 6851); prefer `uid_mv` when server supports it; fall back to copy+expunge if server lacks MOVE capability |

---

## Architecture Patterns

### Recommended Project Structure (additions to Phase 7 skeleton)

```
src/mailsync/
├── main.rs                    # existing — spawn foreground_worker task
├── imap/
│   ├── background_sync.rs     # Phase 7 — no changes
│   ├── foreground_worker.rs   # Phase 8 NEW — IDLE loop + task dispatch
│   └── task_executor.rs       # Phase 8 NEW — per-task-type IMAP command implementations
├── smtp/
│   └── sender.rs              # Phase 8 NEW — SMTP send + MIME construction
├── tasks/
│   ├── mod.rs                 # Phase 8 NEW — TaskKind enum, Task struct, state machine
│   └── recovery.rs            # Phase 8 NEW — startup reset, expiry cleanup
└── store/
    └── task_store.rs          # Phase 8 NEW — tokio-rusqlite task CRUD
```

### Pattern 1: IDLE Loop with Task Interruption

**What:** A loop that continuously re-enters IDLE, using a `StopSource` dropped by a spawned relay task when a task arrives on the mpsc channel.

**When to use:** The entire foreground worker main loop (IDLE-01, IDLE-02).

**Example:**
```rust
// Source: async-email/async-imap examples/src/bin/idle.rs + chatmail/async-imap IDLE extension docs
use async_imap::extensions::idle::IdleResponse;
use std::time::Duration;
use tokio::sync::mpsc;

// task_rx: receiver for TaskKind values sent by stdin reader
pub async fn run_foreground_worker(
    mut session: Session<TlsStream<TcpStream>>,
    mut task_rx: mpsc::Receiver<TaskKind>,
) -> Result<()> {
    // Select the primary folder (INBOX or configured folder) once
    session.select("INBOX").await?;

    loop {
        // Enter IDLE — session.idle() consumes the session, returns Handle
        let mut idle = session.idle();
        idle.init().await?;

        // wait_with_timeout returns (future, StopSource)
        // 25 minutes is safe margin; server-side timeout is 29 minutes
        let (idle_wait, interrupt) = idle.wait_with_timeout(Duration::from_secs(25 * 60));

        // Relay task: drop the StopSource when a task arrives
        // This causes idle_wait to resolve with ManualInterrupt
        let relay = tokio::task::spawn(async move {
            // Block until a task arrives OR the interrupt is no longer needed
            let _ = task_rx.recv().await;
            drop(interrupt); // Triggers ManualInterrupt on idle_wait
            task_rx           // Return receiver so we can use it again
        });

        // Wait for IDLE to end (new data, timeout, or manual interrupt)
        match idle_wait.await? {
            IdleResponse::NewData(data) => {
                // Server notified us of a change — sync the change
                handle_idle_notification(data).await?;
            }
            IdleResponse::Timeout => {
                // 25 minutes elapsed — re-IDLE to prevent 29-min server timeout
            }
            IdleResponse::ManualInterrupt => {
                // A task arrived — process it below
            }
        }

        // Send DONE to server, reclaim the session
        session = idle.done().await?;

        // Recover task_rx from the relay task
        task_rx = relay.await?;

        // If there are queued tasks, process them before re-entering IDLE
        while let Ok(task) = task_rx.try_recv() {
            execute_task(&mut session, task).await?;
        }
        // Loop back to re-enter IDLE
    }
}
```

**Critical:** `idle.done()` MUST be called after the IDLE future resolves, regardless of which variant triggered it. Skipping `done()` leaves the IMAP connection in an undefined state. The session is only reclaimed after `done()`.

### Pattern 2: IDLE Interrupt via tokio Channel

**What:** The stdin reader task sends task payloads over a bounded mpsc channel. The foreground worker's relay spawned task receives on this channel and drops the StopSource.

**When to use:** Main process architecture — connecting stdin reader to foreground worker (IDLE-02).

**Example:**
```rust
// Source: tokio docs + async-imap IDLE example interrupt pattern
use tokio::sync::mpsc;

// In main.rs / process startup:
let (task_tx, task_rx) = mpsc::channel::<TaskKind>(32); // bounded, capacity 32

// Spawn stdin reader task — sends tasks to foreground worker
tokio::spawn(async move {
    while let Some(line) = stdin_lines.next().await {
        if let Ok(cmd) = serde_json::from_str::<StdinCommand>(&line) {
            if let StdinCommand::QueueTask { task } = cmd {
                // Delivers task to foreground worker; interrupts IDLE via StopSource relay
                let _ = task_tx.send(task).await;
            }
        }
    }
});

// Foreground IMAP worker receives tasks and processes them
tokio::spawn(run_foreground_worker(imap_session, task_rx));
```

**Critical:** Use a bounded channel (capacity 32) not unbounded. Backpressure ensures the stdin reader doesn't get infinitely ahead of the foreground worker. The relay pattern (spawn inside the IDLE loop that owns task_rx) is required because the StopSource must be moved into the spawned task.

### Pattern 3: Two-Phase Task Execution

**What:** Every task runs local phase (DB write + delta emit) synchronously before the remote phase (IMAP/SMTP commands). This matches the C++ MailsyncBridge behavior exactly.

**When to use:** All task types (TASK-01, TASK-02).

**Example:**
```rust
// Source: C++ mailsync task lifecycle analysis + tokio-rusqlite call() pattern
async fn execute_task(
    session: &mut Session<TlsStream<TcpStream>>,
    db: &tokio_rusqlite::Connection,
    stdout_tx: &mpsc::Sender<DeltaMessage>,
    task: Task,
) -> Result<()> {
    // TASK-01: Local phase — DB write + delta emit (synchronous, immediate)
    let task_id = task.id.clone();
    db.call(|conn| {
        conn.execute(
            "UPDATE tasks SET status = 'remote' WHERE id = ?",
            rusqlite::params![&task_id],
        )?;
        // Also apply any local model changes (e.g., optimistic flag updates)
        apply_local_changes(conn, &task)?;
        Ok(())
    }).await?;

    // Emit delta for local change so UI updates immediately
    emit_delta(stdout_tx, &task, "persist").await?;

    // TASK-02: Remote phase — IMAP/SMTP commands
    let result = execute_remote_phase(session, &task).await;

    // Update task status to complete or failed
    let final_status = if result.is_ok() { "complete" } else { "failed" };
    let task_id2 = task.id.clone();
    db.call(move |conn| {
        conn.execute(
            "UPDATE tasks SET status = ?, completed_at = datetime('now') WHERE id = ?",
            rusqlite::params![final_status, &task_id2],
        )?;
        Ok(())
    }).await?;

    result
}
```

### Pattern 4: Task Type Dispatch (Enum Match)

**What:** A `TaskKind` enum covers all 13+ task types. A `match` block dispatches to per-type async handler functions.

**When to use:** The `execute_remote_phase` function (TASK-03).

**Example:**
```rust
// Source: Design based on C++ task class hierarchy + async-imap Session method list
#[derive(serde::Deserialize)]
#[serde(tag = "type")]
pub enum TaskKind {
    SendDraft { draft_id: String, /* ... */ },
    DestroyDraft { message_uid: u32, folder: String },
    ChangeLabels { uid_set: String, folder: String, labels_to_add: Vec<String>, labels_to_remove: Vec<String> },
    ChangeFolder { uid_set: String, from_folder: String, to_folder: String },
    ChangeStarred { uid_set: String, folder: String, starred: bool },
    ChangeUnread { uid_set: String, folder: String, unread: bool },
    SyncbackMetadata { /* plugin metadata fields */ },
    SyncbackEvent { /* calendar event fields */ },
    // contact/calendar tasks...
}

async fn execute_remote_phase(
    session: &mut Session<TlsStream<TcpStream>>,
    smtp: &AsyncSmtpTransport<Tokio1Executor>,
    task: &Task,
) -> Result<()> {
    match &task.kind {
        TaskKind::SendDraft { .. } => execute_send_draft(session, smtp, task).await,
        TaskKind::DestroyDraft { .. } => execute_destroy_draft(session, task).await,
        TaskKind::ChangeFolder { .. } => execute_change_folder(session, task).await,
        TaskKind::ChangeStarred { .. } => execute_change_starred(session, task).await,
        TaskKind::ChangeUnread { .. } => execute_change_unread(session, task).await,
        TaskKind::ChangeLabels { .. } => execute_change_labels(session, task).await,
        TaskKind::SyncbackMetadata { .. } => execute_syncback_metadata(task).await,
        TaskKind::SyncbackEvent { .. } => execute_syncback_event(task).await,
        // contact/calendar tasks routed to CalDAV/CardDAV handlers
    }
}
```

### Pattern 5: IMAP Command Mapping for Task Types

**What:** Exact async-imap `Session` method calls for each task's IMAP remote phase.

**When to use:** Per-task handler implementations (TASK-03).

**IMAP Command Mapping Table:**

| Task Type | IMAP Command | async-imap Method | Notes |
|-----------|-------------|-------------------|-------|
| ChangeStarred (star) | `UID STORE uid_set +FLAGS (\Flagged)` | `session.uid_store("uid", "+FLAGS (\\Flagged)")` | Remove: `-FLAGS (\\Flagged)` |
| ChangeUnread (mark unread) | `UID STORE uid_set -FLAGS (\Seen)` | `session.uid_store("uid", "-FLAGS (\\Seen)")` | Mark read: `+FLAGS (\\Seen)` |
| ChangeLabels | Gmail X-GM-LABELS via `UID STORE` | `session.uid_store(uid_set, "+X-GM-LABELS (label)")` | Gmail-specific; standard IMAP uses folders |
| ChangeFolder | `UID MOVE uid_set mailbox` | `session.uid_mv(uid_set, to_folder)` | Falls back to uid_copy + uid_store \Deleted + uid_expunge |
| DestroyDraft | `UID STORE uid +FLAGS (\Deleted)` then `UID EXPUNGE uid` | `session.uid_store(uid, "+FLAGS (\\Deleted)")` then `session.uid_expunge(uid)` | select drafts folder first |
| SendDraft | SMTP send + IMAP APPEND to Sent | `mailer.send(email).await` then `session.append("Sent", Some("(\\Seen)"), None, &raw_email)` | Gmail: skip APPEND (GMAL-04) |

### Pattern 6: MIME Construction — BLOCKER RESOLVED

**What:** Build a complete MIME email from draft JSON using lettre's MultiPart builder. Handles all three structure types: alternative (HTML+plain), related (inline CID images), and mixed (file attachments).

**When to use:** `execute_send_draft` (SEND-03).

**Complete lettre MIME example (verified from official docs):**
```rust
// Source: lettre docs.rs message/index.html Complex MIME body example (verified)
// This resolves the STATE.md blocker: inline images via CID ARE supported in lettre 0.11
use lettre::message::{header, Attachment, Body, Message, MultiPart, SinglePart};
use std::fs;

fn build_email_message(draft: &DraftJson) -> Result<Message, lettre::error::Error> {
    // Load any inline images
    let image_bytes = fs::read(&draft.inline_image_path)?;
    let image_body = Body::new(image_bytes);

    // Correct MIME nesting structure:
    //   multipart/mixed          <- outer: separates body from file attachments
    //     multipart/alternative  <- inner: plain text vs HTML variants
    //       text/plain           <- fallback for email clients without HTML
    //       multipart/related    <- HTML with inline image references
    //         text/html          <- HTML body with <img src="cid:img-id">
    //         image/png [inline] <- inline image with Content-ID: <img-id>
    //     application/pdf [attachment] <- file attachment

    Message::builder()
        .from(draft.from.parse()?)
        .to(draft.to.parse()?)
        .subject(&draft.subject)
        .multipart(
            MultiPart::mixed()
                .multipart(
                    MultiPart::alternative()
                        .singlepart(SinglePart::plain(draft.plain_body.clone()))
                        .multipart(
                            MultiPart::related()
                                .singlepart(SinglePart::html(draft.html_body.clone()))
                                .singlepart(
                                    Attachment::new_inline(String::from("img-id-123"))
                                        .body(image_body, "image/png".parse().unwrap()),
                                ),
                        ),
                )
                .singlepart(
                    Attachment::new(String::from("document.pdf"))
                        .body(
                            fs::read(&draft.attachment_path)?,
                            "application/pdf".parse().unwrap(),
                        ),
                ),
        )
}
```

**CID reference format:** In HTML, reference the inline image as `<img src="cid:img-id-123">`. The `Attachment::new_inline("img-id-123")` call sets `Content-ID: <img-id-123>` and `Content-Disposition: inline`. The `cid:` prefix in HTML maps to the Content-ID without angle brackets.

**For emails without inline images:**
```rust
// Simple alternative: just HTML + plain text, no inline images
MultiPart::alternative_plain_html(
    String::from("Plain text body"),
    String::from("<p>HTML body</p>"),
)
```

**For emails with no HTML at all:**
```rust
Message::builder()
    .from(draft.from.parse()?)
    .to(draft.to.parse()?)
    .subject(&draft.subject)
    .body(draft.plain_body.clone())?
```

### Pattern 7: Crash Recovery on Startup

**What:** On process startup, before entering the IDLE loop, reset any tasks stuck in `remote` state to `local` state so they re-execute.

**When to use:** Process initialization, before the foreground worker starts (TASK-04).

**Example:**
```rust
// Source: TASK-04 requirement + tokio-rusqlite call() pattern
pub async fn reset_stuck_tasks(db: &tokio_rusqlite::Connection) -> Result<()> {
    db.call(|conn| {
        // Tasks stuck in 'remote' = crash occurred during remote phase
        // Reset to 'local' so they re-run on next iteration
        let count = conn.execute(
            "UPDATE tasks SET status = 'local' WHERE status = 'remote'",
            [],
        )?;
        if count > 0 {
            log::warn!("Reset {} tasks stuck in remote state after crash", count);
        }
        Ok(())
    }).await?;
    Ok(())
}

pub async fn expire_completed_tasks(
    db: &tokio_rusqlite::Connection,
    retention_seconds: i64,
) -> Result<()> {
    db.call(move |conn| {
        conn.execute(
            "DELETE FROM tasks WHERE status = 'complete' \
             AND completed_at < datetime('now', ?)",
            rusqlite::params![format!("-{} seconds", retention_seconds)],
        )?;
        Ok(())
    }).await?;
    Ok(())
}
```

### Pattern 8: Separate IDLE and Background Sync Sessions

**What:** Two independent IMAP sessions from two independent TCP+TLS connections. Neither session is shared between tasks. Tokio tasks own sessions exclusively.

**When to use:** Process architecture — running IDLE alongside background sync (IDLE-03).

**Example:**
```rust
// Source: IDLE-03 requirement + tokio::spawn documentation
// In process main, after the startup handshake:

// Connection 1: Background sync (Phase 7)
let bg_session = connect_imap(&account).await?;
let bg_task = tokio::spawn(run_background_sync(bg_session, db.clone(), stdout_tx.clone()));

// Connection 2: Foreground IDLE (Phase 8)
// Separate TCP connection — completely independent from bg_session
let fg_session = connect_imap(&account).await?;
let fg_task = tokio::spawn(run_foreground_worker(fg_session, task_rx, db.clone(), stdout_tx.clone()));

// Both tasks run concurrently via tokio; no sharing of Session instances
tokio::join!(bg_task, fg_task);
```

**Critical:** `Session<T>` is NOT `Send` + `Sync` because it wraps an async stream. Each session must be owned by exactly one tokio task. Do NOT wrap sessions in `Arc<Mutex<>>` — IMAP protocol requires sequential command/response within a connection, which Mutex would serialize anyway. Independent sessions are the correct design.

### Anti-Patterns to Avoid

- **Sharing an IMAP Session between the IDLE worker and the background sync worker:** IMAP is a command/response protocol on a single TCP connection. Interleaving commands from two code paths on one Session causes protocol corruption. Use two sessions (IDLE-03).
- **Not calling `idle.done()` after the IDLE future resolves:** The `Handle` struct still owns the session. Without `done()`, the DONE command is never sent and the session is leaked. Always `idle.done().await` in every branch.
- **Using `drop(idle)` instead of `idle.done().await` to exit IDLE:** Dropping the Handle without calling `done()` does NOT send DONE to the server. The server continues waiting in IDLE mode while the client has moved on, causing a desync.
- **Running the IDLE loop with a single 29-minute timeout:** The server logs out clients that have been in IDLE for 29 minutes. Using exactly 29 minutes has no safety margin. Use 25 minutes to ensure re-IDLE happens before the server timeout.
- **Sending task payloads over an unbounded mpsc channel:** Without backpressure, a fast stdin reader can enqueue thousands of tasks while the IMAP worker is slow. Use a bounded channel (32 is sufficient).
- **Missing "builder" feature in lettre Cargo.toml:** `default-features = false` excludes the `builder` feature. `Message::builder()`, `MultiPart`, `Attachment`, and `SinglePart` are ALL behind the `builder` feature gate. Phase 3 code (testSMTPConnection) works without it since it only calls `test_connection()`, not message construction.
- **Wrong CID format in HTML:** The HTML must use `cid:` without angle brackets: `<img src="cid:my-id">`. The `Content-ID` header uses angle brackets: `<my-id>`. `Attachment::new_inline("my-id")` generates the correct header. Do NOT include angle brackets in the string passed to `new_inline`.
- **Gmail APPEND to Sent after send:** Gmail auto-saves sent messages. For Gmail accounts, skip the IMAP APPEND step after SMTP send (GMAL-04). Add a `is_gmail` flag check in `execute_send_draft`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| MIME multipart construction | Custom string concatenation of MIME parts | lettre `Message::builder()` + `MultiPart` + `Attachment` | MIME boundaries, Content-Transfer-Encoding selection (7bit/QP/base64), header folding — many edge cases |
| Content-ID inline image headers | Manual `Content-ID: <id>` header string | `Attachment::new_inline(id)` | Sets `Content-ID`, `Content-Disposition: inline`, and integrates into `multipart/related`; getting headers wrong breaks display in clients |
| IMAP flag modification | Raw IMAP STORE command string | `session.uid_store(uid_set, "+FLAGS (\\Flagged)")` | IMAP literal syntax, UID vs sequence number mode, response parsing — async-imap handles all of it |
| IMAP folder move | uid_copy + mark \Deleted + expunge manually | `session.uid_mv(uid_set, folder)` | UID MOVE (RFC 6851) is atomic; manual copy+delete has race conditions and is not atomic |
| SMTP state machine | Manual EHLO, AUTH, DATA commands | `lettre::AsyncSmtpTransport::send()` | SMTP has a complex session state machine with pipelining, multi-line responses, and auth challenge sequences |
| Task interrupt mechanism | Custom polling or sleep loop | `StopSource` drop pattern via mpsc relay | The stop-token crate integrates with async-imap IDLE internally; any other approach bypasses the protocol-level interrupt |
| Crash detection | Process monitor / heartbeat | Startup `UPDATE tasks SET status = 'local' WHERE status = 'remote'` | SQLite persistent task state is the source of truth; no need for external monitor |

**Key insight:** MIME email format has decades of encoding edge cases (Content-Transfer-Encoding selection per content, base64 line length, header charset encoding, multipart boundary collision avoidance). lettre's builder encodes all of this — the complex structure becomes 15 lines of Rust rather than 500.

---

## Common Pitfalls

### Pitfall 1: idle.done() Not Called — Session Leaked

**What goes wrong:** Code handles `ManualInterrupt` to process a task but forgets to call `idle.done().await` before interacting with the session. The Handle still owns the session; the Session cannot be used until done() is called.

**Why it happens:** The pattern of `let mut idle = session.idle()` followed by `idle.done()` looks like it should reclaim the session, but the `session` variable is consumed by `session.idle()`. The Handle is the only way back to the session.

**How to avoid:** Structure the IDLE loop so that `idle.done().await?` is called unconditionally after the `idle_wait.await?` match, in every branch. Then rebind: `session = idle.done().await?`.

**Warning signs:** "IMAP session in unexpected state" errors on the second task execution. Connection drop after one task.

### Pitfall 2: ManualInterrupt Does Not Mean a Task Was Received

**What goes wrong:** Assuming that when `idle_wait` resolves with `ManualInterrupt`, there is always exactly one task queued. In the relay pattern, the relay task also terminates if the channel closes (sender dropped). A process shutdown or stdin EOF will drop the sender, triggering a `ManualInterrupt` with no tasks in the queue.

**Why it happens:** Dropping the StopSource triggers ManualInterrupt regardless of whether it was dropped due to a task arrival or due to the relay task completing for another reason.

**How to avoid:** After IDLE exits with ManualInterrupt (or any reason), use `task_rx.try_recv()` to drain any available tasks. If nothing is available, it was a spurious interrupt (connection reset or shutdown). Handle gracefully.

**Warning signs:** Panic or error on `task_rx.recv()` after ManualInterrupt.

### Pitfall 3: lettre "builder" Feature Missing

**What goes wrong:** Adding Phase 8 code that uses `Message::builder()`, `MultiPart::mixed()`, or `Attachment::new_inline()` fails to compile with "use of undeclared type `MultiPart`" or "no method named `multipart` on type `MessageBuilder`".

**Why it happens:** Phase 3 configured lettre with `default-features = false` but only added `smtp-transport`, `tokio1`, `tokio1-rustls-tls`, and `hostname`. The `builder` feature was not added because Phase 3 only needed `test_connection()`, not message construction.

**How to avoid:** Add `"builder"` to the lettre features list in Cargo.toml before writing any Phase 8 message construction code. Verify with `cargo check` after adding.

**Warning signs:** Compile error: "error[E0412]: cannot find type `MultiPart` in module `lettre::message`".

### Pitfall 4: Two IMAP Sessions from One Connection Object

**What goes wrong:** Attempting to create two simultaneous sessions by calling `client.login()` twice on the same client, or by cloning a Session.

**Why it happens:** Misunderstanding that `client.login()` consumes the client. Session does not implement Clone (it owns the TCP stream). Two "sessions" on one TCP connection would interleave commands and corrupt the IMAP protocol state.

**How to avoid:** Call `connect_imap()` twice to create two independent TCP connections. Each `Session` owns its TCP+TLS stream exclusively.

**Warning signs:** Compile error "cannot move out of `client` because it is borrowed" or runtime IMAP parsing errors when commands interleave.

### Pitfall 5: StopSource Must Be Moved Into the Relay Task

**What goes wrong:** Trying to keep the StopSource in the outer scope and trigger it from outside the spawned relay task. This doesn't work because `drop(interrupt)` inside a `tokio::select!` arm causes borrow issues when `idle_wait` is also being polled.

**Why it happens:** The async-imap `wait()` future borrows `&mut self` (the Handle), and the StopSource is returned alongside it. Dropping the StopSource while the future is being polled requires the drop to happen in a separate task or context.

**How to avoid:** Move the StopSource into a separate spawned task. The spawned task receives on the mpsc channel and drops the StopSource when a task arrives. Abort the spawned task after IDLE exits to clean up.

**Warning signs:** Borrow checker errors: "cannot move out of `interrupt` because it is borrowed" when trying to drop it while `idle_wait` is in a select arm.

### Pitfall 6: uid_mv Not Supported on All Servers

**What goes wrong:** Calling `session.uid_mv()` on a server that doesn't advertise the MOVE capability (RFC 6851). The server returns a NO response causing the task to fail.

**Why it happens:** IMAP MOVE is an extension, not part of RFC 3501. Older servers (Exchange < 2013, Dovecot < 2.2.9) don't support it.

**How to avoid:** Check capabilities before using `uid_mv`. If `MOVE` is not in capabilities, fall back to: `uid_copy` to destination + `uid_store` to add `\Deleted` flag + `uid_expunge` to delete from source.

**Warning signs:** Task failure with "NO [CANNOT] Unknown command: MOVE" or "BAD Invalid command".

### Pitfall 7: MIME Nesting Order for multipart/related

**What goes wrong:** Putting `Attachment::new_inline` inside `multipart/mixed` instead of `multipart/related`. The inline image shows up as a regular attachment in email clients instead of being displayed inline.

**Why it happens:** The RFC 2387 MIME multipart/related type specifically groups the HTML body with its inline resources. Email clients use the `Content-ID` header to match `cid:` references in HTML only when the attachment is in the same `multipart/related` part.

**How to avoid:** Use the nesting: `mixed() → alternative() → related()` where `related` contains both the HTML SinglePart and the `Attachment::new_inline()` parts.

**Warning signs:** Inline images appear as email attachments with a paperclip icon instead of displaying in the body.

### Pitfall 8: append() Raw Format for Sent Folder

**What goes wrong:** Passing `email.formatted()` (which is `Vec<u8>`) to `session.append()` works for the content argument. However, the flags string must use double backslash notation: `"(\\Seen)"` in Rust source, which becomes the literal string `(\Seen)` in IMAP protocol.

**Why it happens:** Rust string escaping: `\\` in a Rust string literal is a single `\` character. IMAP requires `\Seen` (backslash-Seen) as the flag. Using a single backslash in the Rust string would produce `\S` which is incorrect.

**How to avoid:**
```rust
session.append("Sent", Some("(\\Seen)"), None, &email.formatted()).await?;
// "\\Seen" in Rust = \Seen in wire protocol = correct IMAP flag
```

---

## Code Examples

Verified patterns from official sources and research:

### Complete MIME Construction: All Cases

```rust
// Source: lettre docs.rs message/index.html Complex MIME body example (verified 2026-03-02)
use lettre::message::{Attachment, Body, Message, MultiPart, SinglePart};

fn build_draft_email(draft: &DraftJson) -> Result<Message> {
    let mut builder = Message::builder()
        .from(draft.from.parse()?)
        .subject(&draft.subject);

    for to in &draft.to {
        builder = builder.to(to.parse()?);
    }
    for cc in &draft.cc {
        builder = builder.cc(cc.parse()?);
    }

    // Case 1: HTML with inline images + plain fallback + file attachments
    if draft.has_html && draft.has_inline_images && draft.has_attachments {
        let html_part = build_related_part(draft)?; // html + inline images
        let email = builder.multipart(
            MultiPart::mixed()
                .multipart(
                    MultiPart::alternative()
                        .singlepart(SinglePart::plain(draft.plain_body.clone()))
                        .multipart(html_part),
                )
                .singlepart(build_file_attachment(draft)?),
        )?;
        return Ok(email);
    }

    // Case 2: HTML with inline images, no file attachments
    if draft.has_html && draft.has_inline_images {
        let email = builder.multipart(
            MultiPart::alternative()
                .singlepart(SinglePart::plain(draft.plain_body.clone()))
                .multipart(build_related_part(draft)?),
        )?;
        return Ok(email);
    }

    // Case 3: HTML + plain, no inline images
    if draft.has_html {
        let email = builder.multipart(
            MultiPart::alternative_plain_html(
                draft.plain_body.clone(),
                draft.html_body.clone(),
            ),
        )?;
        return Ok(email);
    }

    // Case 4: Plain text only
    Ok(builder.body(draft.plain_body.clone())?)
}

fn build_related_part(draft: &DraftJson) -> Result<MultiPart> {
    let mut related = MultiPart::related()
        .singlepart(SinglePart::html(draft.html_body.clone()));
    for img in &draft.inline_images {
        // img.cid = the ID used in HTML as: <img src="cid:{img.cid}">
        related = related.singlepart(
            Attachment::new_inline(img.cid.clone())
                .body(
                    Body::new(std::fs::read(&img.path)?),
                    img.mime_type.parse()?,
                ),
        );
    }
    Ok(related)
}
```

### IDLE Loop — Production Pattern

```rust
// Source: async-imap IDLE extension API + deltachat-core-rust interrupt pattern
use async_imap::extensions::idle::IdleResponse;
use tokio::sync::mpsc;

pub async fn foreground_idle_loop(
    mut session: Session<TlsStream<TcpStream>>,
    mut task_rx: mpsc::Receiver<Task>,
    db: tokio_rusqlite::Connection,
    stdout_tx: mpsc::Sender<DeltaMessage>,
) -> Result<()> {
    // Crash recovery: reset any tasks stuck in remote state from previous run
    reset_stuck_tasks(&db).await?;

    // Select primary folder once before entering the IDLE loop
    session.select("INBOX").await?;

    loop {
        // Enter IDLE mode
        let mut idle_handle = session.idle();
        idle_handle.init().await?;

        // 25-minute timeout: safe margin before 29-minute server-side disconnect
        let (idle_future, interrupt) = idle_handle.wait_with_timeout(
            Duration::from_secs(25 * 60)
        );

        // Relay task: owns interrupt, drops it when a task arrives
        let relay = tokio::spawn(async move {
            let task = task_rx.recv().await; // waits for next task
            drop(interrupt);                 // triggers ManualInterrupt
            (task_rx, task)                 // return both for reuse
        });

        // Wait for IDLE to complete
        match idle_future.await? {
            IdleResponse::NewData(_data) => {
                // New mail arrived — trigger a background sync cycle
                // (Background sync worker handles the actual fetch)
            }
            IdleResponse::Timeout => {
                // Timer elapsed — re-IDLE to reset the 29-minute server timeout
                // This is normal operation, no action needed
            }
            IdleResponse::ManualInterrupt => {
                // A task arrived (or channel closed during shutdown)
            }
        }

        // Always send DONE to recover the session — no exceptions
        session = idle_handle.done().await?;

        // Recover relay task and any task it received
        let (recovered_rx, maybe_task) = relay.await?;
        task_rx = recovered_rx;

        // Process any received task plus any additional queued tasks
        if let Some(task) = maybe_task {
            execute_task(&mut session, &db, &stdout_tx, task).await?;
        }
        // Drain any additional tasks before re-entering IDLE
        while let Ok(task) = task_rx.try_recv() {
            execute_task(&mut session, &db, &stdout_tx, task).await?;
        }
        // Loop: re-enter IDLE
    }
}
```

### Startup Crash Recovery

```rust
// Source: TASK-04 requirement + tokio-rusqlite call() pattern
pub async fn startup_recovery(
    db: &tokio_rusqlite::Connection,
    task_tx: &mpsc::Sender<Task>,
    task_retention_secs: i64,
) -> Result<()> {
    // Step 1: Reset tasks stuck in remote state (crash recovery — TASK-04)
    let stuck_tasks = db.call(|conn| {
        conn.execute(
            "UPDATE tasks SET status = 'local' WHERE status = 'remote'",
            [],
        )?;
        // Fetch tasks now in local state for re-queuing
        let mut stmt = conn.prepare(
            "SELECT id, type, payload FROM tasks WHERE status = 'local' ORDER BY created_at ASC"
        )?;
        let tasks: Vec<Task> = stmt.query_map([], |row| {
            Ok(Task {
                id: row.get(0)?,
                task_type: row.get(1)?,
                payload: row.get(2)?,
            })
        })?.filter_map(|r| r.ok()).collect();
        Ok(tasks)
    }).await?;

    // Step 2: Re-queue recovered tasks
    for task in stuck_tasks {
        let _ = task_tx.send(task).await;
    }

    // Step 3: Expire old completed tasks (TASK-05)
    db.call(move |conn| {
        conn.execute(
            "DELETE FROM tasks WHERE status = 'complete' \
             AND completed_at < datetime('now', ?)",
            rusqlite::params![format!("-{} seconds", task_retention_secs)],
        )?;
        Ok(())
    }).await?;

    Ok(())
}
```

### SMTP Send with MIME Message

```rust
// Source: lettre AsyncTransport::send() docs + Phase 3 SMTP transport pattern
use lettre::{AsyncTransport, Tokio1Executor};

pub async fn send_draft_smtp(
    smtp: &AsyncSmtpTransport<Tokio1Executor>,
    draft: &DraftJson,
) -> Result<()> {
    let email = build_draft_email(draft)?;

    // Outer timeout wraps entire send (SEND-04: 15 seconds total)
    let result = tokio::time::timeout(
        Duration::from_secs(15),
        smtp.send(email),
    ).await;

    match result {
        Ok(Ok(_response)) => Ok(()),
        Ok(Err(e)) => Err(format!("SMTP send failed: {}", e).into()),
        Err(_elapsed) => Err("SMTP send timed out after 15 seconds".into()),
    }
}

// After successful SMTP send, APPEND to Sent folder (skip for Gmail — GMAL-04)
pub async fn append_to_sent_folder(
    session: &mut Session<TlsStream<TcpStream>>,
    email: &Message,
    is_gmail: bool,
) -> Result<()> {
    if is_gmail {
        return Ok(()); // Gmail auto-saves sent messages via SMTP
    }
    let raw = email.formatted();
    // "(\\Seen)" in Rust = "(\Seen)" on wire = marks message as read in Sent folder
    session.append("Sent", Some("(\\Seen)"), None, raw.as_slice()).await?;
    Ok(())
}
```

### ChangeFolder with MOVE Capability Fallback

```rust
// Source: async-imap Session method list + RFC 6851 MOVE extension
pub async fn execute_change_folder(
    session: &mut Session<TlsStream<TcpStream>>,
    uid_set: &str,
    from_folder: &str,
    to_folder: &str,
    has_move_capability: bool,
) -> Result<()> {
    session.select(from_folder).await?;

    if has_move_capability {
        // Atomic MOVE (RFC 6851) — preferred
        session.uid_mv(uid_set, to_folder).await?;
    } else {
        // Fallback: copy + mark deleted + expunge (not atomic)
        session.uid_copy(uid_set, to_folder).await?;
        session.uid_store(uid_set, "+FLAGS (\\Deleted)").await?
            .collect::<Vec<_>>().await; // drain the stream
        session.uid_expunge(uid_set).await?
            .collect::<Vec<_>>().await; // drain the stream
    }
    Ok(())
}
```

### Progress Updates During Body Sync (IMPR-07)

```rust
// Source: IMPR-07 requirement + stdout mpsc channel pattern from Phase 5
pub async fn fetch_bodies_with_progress(
    session: &mut Session<TlsStream<TcpStream>>,
    uids: Vec<u32>,
    stdout_tx: &mpsc::Sender<DeltaMessage>,
    db: &tokio_rusqlite::Connection,
) -> Result<()> {
    let total = uids.len();
    for (i, uid) in uids.iter().enumerate() {
        // Fetch body for this UID
        let body = fetch_single_body(session, *uid).await?;

        // Persist to database
        store_body_in_db(db, *uid, &body).await?;

        // Emit progress delta so UI shows incremental loading
        // (not batched — each message emits immediately)
        stdout_tx.send(DeltaMessage {
            model_class: "Message".into(),
            delta_type: "persist".into(),
            // Include progress metadata for UI
            objects: vec![body_to_model_json(*uid, &body)],
        }).await?;

        // Optional: yield to tokio scheduler every N messages
        if i % 10 == 0 {
            tokio::task::yield_now().await;
        }
    }
    Ok(())
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| Single long IDLE timeout (23–29 min) | Short-cycle IDLE with explicit re-IDLE (25 min) | deltachat-core-rust issue #2208 (2022) | Prevents silent server disconnects; provides predictable reconnection point |
| Manual DONE + sleep re-IDLE | `wait_with_timeout()` built-in timer | async-imap 0.9+ | Library handles re-IDLE timeout internally; no manual timer management |
| COPY + STORE \Deleted + EXPUNGE for move | UID MOVE (RFC 6851) with fallback | RFC 6851 (2014), broad server adoption 2020+ | Atomic move prevents orphaned messages; check MOVE capability before using |
| Platform-native TLS (OpenSSL/SChannel) | rustls exclusively | Project decision | Eliminates OpenSSL symbol conflict with Electron's BoringSSL |
| String concatenation MIME building | lettre `MultiPart` builder | lettre 0.10 (2021) | Type-safe construction; automatic encoding selection; boundary generation |
| `trust-dns-resolver` | `hickory-resolver` | crate rename v0.24 (2023) | Same crate, new name; use hickory-resolver |

**Deprecated/outdated:**
- `async-smtp` crate: archived, unmaintained. Use lettre instead.
- `tokio-imap` crate: unmaintained (last commit 2021). Use async-imap instead.
- `imap` crate (sync, jonhoo/rust-imap): blocking-only. Use async-imap for tokio-based code.
- `lettre::Tls::Opportunistic` for STARTTLS: silently falls back to plaintext. Use `Tls::Required`.
- Deltachat's old 5-minute IDLE poll pattern: superseded by `wait_with_timeout` with 25-min cycles.

---

## Open Questions

1. **Does `uid_mv` in async-imap check for MOVE capability or does it fail silently?**
   - What we know: `session.uid_mv()` sends `UID MOVE` command directly. The async-imap Session does not pre-check capabilities.
   - What's unclear: Whether `uid_mv` returns an IMAP-level error (Err result) or silently does nothing when the server lacks MOVE support.
   - Recommendation: Check server capabilities during session setup (Phase 7 pattern); store `has_move_capability` flag in account state. In Phase 8 `execute_change_folder`, check this flag and use `uid_copy` + `uid_store` + `uid_expunge` fallback when false.

2. **Does the relay task pattern (spawn + return task_rx) work correctly across multiple IDLE cycles?**
   - What we know: The pattern returns `task_rx` from the spawned relay task via the JoinHandle. This requires the relay task to be awaited before re-entering the IDLE loop.
   - What's unclear: Whether `relay.await?` correctly recovers `task_rx` in all exit paths (timeout, new data, manual interrupt). The relay task spawned for `Timeout` and `NewData` will remain blocked on `task_rx.recv()` until the join handle is awaited.
   - Recommendation: Always `relay.await?` unconditionally after the IDLE future resolves. If the relay task is not yet done (for Timeout/NewData paths where no task arrived), abort it instead: `relay.abort(); task_rx = ...` — but this drops the receiver ownership. Alternative: use `relay.abort()` and keep `task_rx` in the outer scope. Research the abort-and-recover pattern before implementation.
   - Better alternative: Keep `task_rx` in the outer scope. Use a separate `Notify` or oneshot channel for the StopSource trigger rather than the relay-and-return pattern. This is simpler.

3. **How does async-imap handle IDLE when the server sends `* OK Still here` keepalives?**
   - What we know: RFC 2177 says servers MAY send keepalive messages. The `wait_with_timeout` resets the timeout on any server response including keepalives (confirmed in chatmail/async-imap issue #5093).
   - What's unclear: Whether a flurry of server keepalives could prevent the 25-minute timer from ever expiring, causing the IDLE to run indefinitely.
   - Recommendation: This is benign — if the server sends keepalives, the connection is healthy. The 25-minute re-IDLE is a safety mechanism for silent disconnects. If the server is active, staying in IDLE longer is fine.

4. **Optimal task_rx ownership pattern for the relay task**
   - What we know: The relay task must own the StopSource AND receive from task_rx. The example above returns task_rx from the relay task via the JoinHandle.
   - What's unclear: Whether a simpler design (e.g., oneshot channel for the trigger signal, task_rx in outer scope) is less error-prone.
   - Recommendation: Use a `tokio::sync::oneshot::channel()` for the interrupt trigger. Keep `task_rx` in the outer foreground worker loop. When a task arrives on `task_rx`, send the oneshot trigger. The relay task waits on the oneshot and drops the StopSource. This avoids the ownership shuffle.

---

## Sources

### Primary (HIGH confidence)
- [async-imap extensions/idle.rs (chatmail fork)](https://github.com/chatmail/async-imap/blob/main/src/extensions/idle.rs) — Handle struct, wait_with_timeout(), done(), StopSource pattern, IdleResponse enum
- [async-imap IDLE example (async-email/async-imap)](https://github.com/async-email/async-imap/blob/main/examples/src/bin/idle.rs) — complete IDLE example: idle.init(), idle.wait(), drop(interrupt), idle.done()
- [lettre message docs (lettre 0.11.19)](https://docs.rs/lettre/latest/lettre/message/index.html) — Complex MIME body example confirmed: MultiPart::mixed() → alternative() → related() + Attachment::new_inline()
- [lettre Attachment docs (lettre 0.11.19)](https://docs.rs/lettre/latest/lettre/message/struct.Attachment.html) — new(), new_inline(), new_inline_with_name() — confirms CID support with Content-ID and Content-Disposition: inline
- [lettre MultiPart docs (lettre 0.11.19)](https://docs.rs/lettre/latest/lettre/message/struct.MultiPart.html) — mixed(), alternative(), related(), alternative_plain_html() constructors confirmed
- [lettre AsyncTransport docs (lettre 0.11.19)](https://docs.rs/lettre/latest/lettre/trait.AsyncTransport.html) — send(Message) owned Message, send_raw(), shutdown()
- [lettre Mechanism docs (lettre 0.11.19)](https://docs.rs/lettre/latest/lettre/transport/smtp/authentication/enum.Mechanism.html) — Xoauth2 variant confirmed to exist
- [async-imap Session docs](https://docs.rs/async-imap/latest/async_imap/struct.Session.html) — Full method list: uid_store, uid_mv (EXISTS), uid_copy, uid_expunge, append (with signature), idle
- [tokio-rusqlite Connection docs](https://docs.rs/tokio-rusqlite/latest/tokio_rusqlite/struct.Connection.html) — call() method signature, call_unwrap()
- [stop-token docs](https://docs.rs/stop-token/latest/stop_token/) — StopSource::new(), drop-to-cancel mechanism

### Secondary (MEDIUM confidence)
- [lettre Discussion #751](https://github.com/lettre/lettre/discussions/751) — Confirmed `tokio1-rustls-tls` feature name; `default-features = false` required
- [deltachat-core-rust issue #2208](https://github.com/deltachat/deltachat-core-rust/issues/2208) — Short-cycle IDLE pattern rationale; 25-minute cycle documented as stable
- [deltachat-core-rust IDLE implementation](https://rs.delta.chat/deltachat/imap/idle/index.html) — async_channel::Receiver used for interrupt; relay spawn-abort pattern confirmed
- [joelparkerhenderson demo-rust-lettre-async-tokio1-rustls-tls](https://github.com/joelparkerhenderson/demo-rust-lettre-async-tokio1-rustls-tls) — Feature flags `["builder", "hostname", "smtp-transport", "tokio1", "tokio1-rustls", "rustls-tls"]` used in production demo

### Tertiary (LOW confidence — verify before use)
- relay task pattern (return task_rx from JoinHandle): described in research, not verified against compiler. Open Question 2 documents alternative approaches.
- `uid_mv` server capability behavior on non-MOVE servers: behavior described as IMAP-level error but not verified against async-imap source.

---

## Metadata

**Confidence breakdown:**
- IDLE API (init, wait_with_timeout, done, StopSource): HIGH — verified from chatmail/async-imap source + async-email example
- lettre MIME builder (MultiPart, Attachment::new_inline, CID): HIGH — verified from official lettre docs + Complex MIME body example confirmed
- lettre Mechanism::Xoauth2: HIGH — confirmed variant exists in docs
- lettre feature flags (tokio1-rustls-tls, builder): HIGH — confirmed from docs.rs features page + Discussion #751
- async-imap Session methods (uid_mv, uid_store, append): HIGH — verified from docs.rs Session method list
- tokio-rusqlite call() pattern: HIGH — verified from docs.rs Connection::call signature
- Task processor design (enum dispatch, two-phase): MEDIUM — architectural pattern; no single authoritative source; grounded in C++ mailsync analysis and Rust enum patterns
- Relay task ownership pattern: MEDIUM — described but compiler behavior of return-from-JoinHandle not verified; Open Question 2 offers alternatives
- uid_mv fallback on non-MOVE servers: MEDIUM — RFC 6851 specifies error response but async-imap error handling not verified from source

**Research date:** 2026-03-02
**Valid until:** 2026-09-02 (async-imap 0.11.x stable; lettre 0.11.x stable; tokio 1.x stable)

**Blocker status:** STATE.md blocker "[Phase 8 research flag]: Validate lettre multipart MIME builder API coverage for inline images (CID references) and text/html + text/plain alternatives" — **RESOLVED**. lettre 0.11 fully supports all required MIME constructs. See Pattern 6 and Code Examples for the verified implementation.
