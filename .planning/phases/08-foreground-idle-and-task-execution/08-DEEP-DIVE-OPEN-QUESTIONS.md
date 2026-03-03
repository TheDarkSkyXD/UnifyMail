# Phase 8 Deep-Dive: Open Questions Resolved

**Researched:** 2026-03-02
**Sources:** chatmail/async-imap source (GitHub), deltachat/deltachat-core-rust source (GitHub), RFC 6851, stop-token crate docs, async-imap docs.rs, tokio docs

---

## OQ1: uid_mv Behavior on Non-MOVE Servers

**Answer:** `uid_mv` sends `UID MOVE` blindly with no capability pre-check. On a server without MOVE capability, it returns `Error::Bad(String)` (not `Error::No`). The string payload contains the server's BAD response message. Phase 8 must check capabilities before calling `uid_mv` and fall back to copy+expunge when MOVE is absent.

**Evidence:**

Source: `chatmail/async-imap` `src/client.rs` — the `uid_mv` implementation:
```rust
pub async fn uid_mv<S1: AsRef<str>, S2: AsRef<str>>(
    &mut self,
    uid_set: S1,
    mailbox_name: S2,
) -> Result<()> {
    self.run_command_and_check_ok(&format!(
        "UID MOVE {} {}",
        uid_set.as_ref(),
        validate_str(mailbox_name.as_ref())?
    ))
    .await?;
    Ok(())
}
```

There is no capability check anywhere in the function. It calls `run_command_and_check_ok` which parses the server's tagged response and maps it to an error:
```rust
match status {
    Status::Ok  => Ok(()),
    Status::Bad => Err(Error::Bad(format!("code: {code:?}, info: {information:?}"))),
    Status::No  => Err(Error::No(format!("code: {code:?}, info: {information:?}")))
}
```

RFC 6851 (https://www.rfc-editor.org/rfc/rfc6851.html) states: a server without the MOVE capability will respond to an unknown `MOVE` or `UID MOVE` command with `BAD`. The IMAP spec is clear that `BAD` means "unknown command or arguments invalid" — a server that does not implement RFC 6851 at all returns `BAD`, not `NO`. A server that implements RFC 6851 but cannot move those specific messages (permissions error, read-only mailbox) returns `NO`.

The `Error` enum in async-imap has exactly two variants for server responses (from `src/error.rs`):
- `Error::Bad(String)` — the server returned BAD
- `Error::No(String)` — the server returned NO

The `uid_mv` documentation note says "This command requires that the server supports RFC 6851 as indicated by the MOVE capability" but this is advisory documentation only — there is no enforcement in the code.

The 08-RESEARCH.md Pitfall 6 identified this issue and noted the fallback pattern. This deep-dive confirms the exact error type.

**Recommendation:** Phase 8 must implement capability-gated `uid_mv` as follows:

```rust
/// Move messages atomically using UID MOVE (RFC 6851) if supported,
/// falling back to uid_copy + uid_store \Deleted + uid_expunge otherwise.
async fn move_messages(
    session: &mut Session<TlsStream<TcpStream>>,
    uid_set: &str,
    from_folder: &str,
    to_folder: &str,
) -> Result<()> {
    // Fetch capabilities (cached per session, not per-call in production)
    let caps = session.capabilities().await?;
    let has_move = caps.has_str("MOVE");

    if has_move {
        // Atomic UID MOVE (RFC 6851) — server returns BAD if capability absent,
        // but we've already checked so this path is only taken when supported.
        session.uid_mv(uid_set, to_folder).await?;
    } else {
        // Fallback: non-atomic copy + delete (RFC 6851 section 3.3 equivalent)
        // Note: this is not atomic — a crash between copy and expunge leaves a
        // duplicate in from_folder. Acceptable for our recovery model (task retry).
        session.uid_copy(uid_set, to_folder).await?;
        session.uid_store(uid_set, "+FLAGS.SILENT (\\Deleted)").await?;
        session.uid_expunge(uid_set).await?;
    }
    Ok(())
}
```

Cache the capability check at session creation time (store `has_move: bool` in the foreground worker struct) rather than querying capabilities on every ChangeFolder task. Capabilities do not change during a session.

Match on `Error::Bad` vs `Error::No` in the ChangeFolder task error handler to provide correct diagnostics: `Bad` means the server does not understand MOVE (capability bug), `No` means the server understood MOVE but refused this specific operation (permissions, destination doesn't exist).

---

## OQ2: Relay Task Ownership Across IDLE Cycles

**Answer:** The relay-and-return pattern proposed in the original research (where the relay task owns `task_rx` and returns it via `JoinHandle`) is broken for the Timeout and NewData branches. Aborting the relay task drops `task_rx` permanently, losing any queued tasks. The correct pattern — confirmed by deltachat-core-rust — is for the relay task to own only the `StopSource` (interrupt handle), while `task_rx` stays in the outer scope of the IDLE loop and is used directly via `try_recv()` after IDLE exits.

**Evidence:**

The original research Pattern 1 in 08-RESEARCH.md proposed this structure:
```rust
let relay = tokio::task::spawn(async move {
    let _ = task_rx.recv().await;  // relay OWNS task_rx
    drop(interrupt);
    task_rx  // return receiver via JoinHandle
});
// ... after IDLE ...
task_rx = relay.await?;  // PROBLEM: only works if relay completed naturally
```

This breaks in two ways:

1. **Timeout branch:** After `idle_wait.await?` returns `IdleResponse::Timeout`, the relay task is still blocked on `task_rx.recv().await`. Calling `relay.await` would block forever (deadlock) because the relay task is waiting for a task that may never come. The original research says "must abort it, but that drops task_rx" — this is the exact problem.

2. **NewData branch:** Same situation. The relay task is still blocked on recv. Any call to `relay.abort()` drops the JoinHandle which aborts the task, and since the task owns `task_rx`, the receiver is destroyed. All queued tasks are lost. The next IDLE cycle has no way to receive tasks.

**Deltachat-core-rust solution** (`deltachat/deltachat-core-rust` `src/imap/idle.rs`):

The correct pattern is confirmed by inspecting the production deltachat implementation:

```rust
// idle_interrupt_receiver is async_channel::Receiver<()>
// It is passed in per IDLE cycle, NOT owned by the relay task permanently
pub async fn idle(
    mut self,
    context: &Context,
    idle_interrupt_receiver: Receiver<()>,  // one-shot signaling channel
    folder: &str,
) -> Result<Self> {
    // ... setup ...
    let (idle_wait, interrupt) = idle.wait_with_timeout(IDLE_TIMEOUT);

    // Relay task owns ONLY the interrupt handle (StopSource).
    // idle_interrupt_receiver is MOVED into relay, but it is a CLONE-able
    // async_channel::Receiver — a new receiver is provided each cycle.
    let interrupt_relay = tokio::spawn(async move {
        idle_interrupt_receiver.recv().await.ok();
        drop(interrupt);  // relay owns StopSource, not task_rx
    });

    match idle_wait.await {
        Ok(IdleResponse::NewData(_)) => { /* log */ }
        Ok(IdleResponse::Timeout)    => { /* log */ }
        Ok(IdleResponse::ManualInterrupt) => { /* log */ }
        Err(err) => { warn!(...) }
    }

    // Abort relay unconditionally in ALL branches — safe because relay
    // only owns interrupt handle, not task_rx
    interrupt_relay.abort();
    interrupt_relay.await.ok();

    // Finalize IDLE
    let session = tokio::time::timeout(
        Duration::from_secs(15),
        idle.done(),
    ).await??;
    // ...
}
```

The key structural insight: deltachat uses **two separate channels**:
- `idle_interrupt_receiver: async_channel::Receiver<()>` — passed per-call, owned by relay task, carries only a unit signal. This channel is cloneable; the scheduler holds the sender and can send `()` to wake the relay. The relay task consumes the receiver; it is recreated for each IDLE cycle.
- The actual work queue (equivalent to `task_rx`) lives in the **outer scheduler loop**, not in the relay task. After the IDLE function returns, the outer scheduler checks the work queue via `try_recv()`.

This separation means:
- The relay task can safely be `abort()`ed in any branch without data loss
- `task_rx` is never moved into a spawned task where it can be lost
- Each IDLE cycle gets a fresh interrupt receiver via `channel::bounded(1)` or by cloning the `async_channel::Receiver`

**The correct pattern for Phase 8:**

Split the single `task_rx: mpsc::Receiver<TaskKind>` into two channels:
1. `idle_interrupt_tx/rx: tokio::sync::mpsc::channel::<()>(1)` — a unit-only wakeup channel for the relay task
2. `task_tx/rx: tokio::sync::mpsc::channel::<TaskKind>(32)` — the actual task queue in the outer loop

```rust
pub async fn run_foreground_worker(
    mut session: Session<TlsStream<TcpStream>>,
    mut task_rx: mpsc::Receiver<TaskKind>,       // STAYS in outer scope
    mut interrupt_rx: mpsc::Receiver<()>,        // relay task receives from this
    // interrupt_rx is replenished each cycle via clone or recreation
) -> Result<()> {
    session.select("INBOX").await?;

    loop {
        let mut idle = session.idle();
        idle.init().await?;
        let (idle_wait, interrupt) = idle.wait_with_timeout(Duration::from_secs(25 * 60));

        // Relay task owns interrupt (StopSource) only — NOT task_rx
        let relay = tokio::spawn(async move {
            interrupt_rx.recv().await;
            drop(interrupt);  // triggers ManualInterrupt on idle_wait
            // NOTE: interrupt_rx is dropped here — it was a one-shot wakeup
        });

        match idle_wait.await? {
            IdleResponse::NewData(_) | IdleResponse::Timeout | IdleResponse::ManualInterrupt => {}
        }

        // Abort relay in ALL branches — safe because relay does not own task_rx
        relay.abort();
        relay.await.ok();

        session = idle.done().await?;

        // Drain task_rx which has been in outer scope the entire time
        while let Ok(task) = task_rx.try_recv() {
            execute_task(&mut session, task).await?;
        }
        // Recreate interrupt_rx for next cycle
        // (or: use async_channel::Receiver which can be .recv() again after abort)
    }
}
```

**Alternative using async_channel (matches deltachat exactly):**

Use `async_channel::bounded::<()>(1)` instead of `tokio::sync::mpsc` for the interrupt signal. `async_channel::Receiver` is `Clone`, so the outer loop can pass a clone to each IDLE cycle without recreating the channel. The outer loop keeps `task_rx: tokio::sync::mpsc::Receiver<TaskKind>` for itself.

**Recommendation:** Use the two-channel pattern. The stdin reader task sends on both channels when a task arrives: `interrupt_tx.try_send(())` to wake the IDLE, and `task_tx.send(task).await` to deliver the actual task. The foreground worker's relay task uses only the interrupt channel. `task_rx` never leaves the outer `loop {}`.

The original research Pattern 1 must be replaced. The relay-and-return pattern (returning `task_rx` via `JoinHandle`) cannot recover `task_rx` when the relay is aborted due to Timeout or NewData — the receiver is destroyed. This was correctly identified as a problem in OQ2.

---

## OQ3: Server Keepalives and the IDLE Timer

**Answer:** `wait_with_timeout` resets its timer on every server response, including `* OK Still here` keepalives. The timeout is NOT absolute from the start of IDLE — it is per-response-gap. If the server sends keepalives more frequently than the timeout interval, IDLE runs indefinitely. A 25-minute timeout does not mean IDLE exits after 25 minutes on a chatty server; it means IDLE exits if 25 minutes pass with NO response from the server.

**Evidence:**

Source: `chatmail/async-imap` `src/extensions/idle.rs`, lines 131-178:

```rust
pub fn wait_with_timeout(
    &mut self,
    dur: Duration,
) -> (
    impl Future<Output = Result<IdleResponse>> + '_,
    stop_token::StopSource,
) {
    let interrupt = stop_token::StopSource::new();
    let raw_stream = IdleStream::new(self);
    let mut interruptible_stream = raw_stream.timeout_at(interrupt.token());

    let fut = async move {
        loop {
            // timeout() wraps EACH call to .next() — timer resets every iteration
            let Ok(res) = timeout(dur, interruptible_stream.next()).await else {
                return Ok(IdleResponse::Timeout);
            };

            let Some(Ok(resp)) = res else {
                return Ok(IdleResponse::ManualInterrupt);
            };

            let resp = resp?;
            match resp.parsed() {
                Response::Data { status: Status::Ok, .. } => {
                    // Keepalive "* OK Still here" — continue loop, timer resets
                }
                Response::Continue { .. } => {
                    // Continuation — continue loop, timer resets
                }
                _ => return Ok(IdleResponse::NewData(resp)),
            }
        }
    };
    (fut, interrupt)
}
```

The critical line is `timeout(dur, interruptible_stream.next())`. This is called inside a `loop {}`. Each iteration of the loop makes a fresh `timeout()` call with the same `dur`. When the server sends `* OK Still here` (matching `Response::Data { status: Status::Ok, .. }`), the code executes `// all good continue` and loops back, creating a new `timeout(dur, ...)` call. The timer has effectively restarted.

This was confirmed by deltachat issue #5093 (https://github.com/deltachat/deltachat-core-rust/issues/5093) which explicitly documents the behavior:

> "reset the timer every time keepalive ('OK Still here' untagged response) is received so we never finish IDLE if keepalives are arriving frequently enough"

PR #5096 merged into deltachat-core-rust updated async-imap to depend on this exact behavior. K-9 Mail uses a 28-minute timer that resets on each keepalive, matching what async-imap provides.

**Practical implications for Phase 8:**

1. **The 25-minute timer is a gap timer, not an absolute timer.** On a server that sends `* OK Still here` every 2 minutes, the foreground worker stays in IDLE indefinitely — the 25-minute timeout never fires. This is correct behavior.

2. **The 25-minute timer is still necessary.** Servers that do NOT send keepalives (or where keepalives are disabled) will cause the timer to fire after 25 minutes of silence. This is the intended use: force a re-IDLE before the server's 29-minute connection timeout.

3. **Phase 8 should NOT change the 25-minute interval** based on this finding. The 25-minute gap timer is correct. A 29-minute absolute timer would still be wrong even if keepalives reset it, because some servers may not send any keepalive in 29 minutes and the server-side timeout would trigger first.

4. **The implementation is simpler than anticipated.** There is no need to implement a secondary "keepalive watchdog" timer because `wait_with_timeout` already handles keepalives correctly.

**Recommendation:** Keep the 25-minute `Duration::from_secs(25 * 60)` unchanged. The gap-timer semantics of `wait_with_timeout` mean keepalive-heavy servers automatically get longer IDLE sessions while keepalive-free servers get the correct 25-minute re-IDLE cadence.

---

## OQ4: Optimal IDLE Interrupt Pattern

**Answer:** Use Pattern C — the deltachat relay pattern with two separate channels. The IDLE future returned by `wait_with_timeout` is `impl Future + '_` (borrows `&mut Handle`), making it incompatible with `tokio::select!` alongside independent futures without unsafe pinning. Pattern A (relay-and-return with `task_rx` moved into relay) cannot recover `task_rx` when aborted in Timeout/NewData branches. Pattern B (oneshot trigger + select!) is architecturally sound but cannot work because the IDLE future borrows `&mut Handle` exclusively while it is being polled. The deltachat pattern (relay owns only `StopSource`, work receiver stays in outer scope) is the production-proven solution.

**Evidence:**

**Why Pattern B (oneshot + select!) does not work:**

`wait_with_timeout` returns `impl Future<Output = Result<IdleResponse>> + '_`. The `'_` lifetime means the future borrows `&mut self` (the `Handle`) for its entire lifetime. From `src/extensions/idle.rs`:

```rust
pub fn wait_with_timeout(
    &mut self,          // <-- mutable borrow of Handle
    dur: Duration,
) -> (
    impl Future<Output = Result<IdleResponse>> + '_,  // <-- borrows self for '_
    stop_token::StopSource,
)
```

The `async move` inside `wait_with_timeout` captures `interruptible_stream` which borrows from `self`. This means:

- The returned future holds the only mutable borrow on `Handle`
- While the future is being polled, no other code can access `Handle`
- `tokio::select!` requires all branch futures to be pinned and polled concurrently on the same task
- You cannot place the IDLE future in one `select!` branch and `task_rx.recv()` in another because the IDLE future is not `Unpin` (it captures a non-Unpin stream reference) and requires `Pin<&mut>` to poll

Attempting to use `tokio::select!` directly:
```rust
// DOES NOT COMPILE — idle_wait is not Unpin, cannot be pinned for select!
tokio::select! {
    result = idle_wait => { /* handle IDLE response */ }
    task = task_rx.recv() => { /* handle task */ }
}
```

Even with `tokio::pin!(idle_wait)`, the underlying `interruptible_stream` captures a `&mut IdleStream<'_>` which creates a self-referential structure incompatible with safe pinning. This is why the async-imap library returns a `StopSource` separately — external interrupt must come via the stop-token mechanism, not via concurrent future polling.

**Why Pattern A (relay-and-return) fails on Timeout/NewData:**

The relay task:
```rust
let relay = tokio::spawn(async move {
    let _ = task_rx.recv().await;  // relay owns task_rx
    drop(interrupt);
    task_rx  // return via JoinHandle
});
```

After `idle_wait.await?` returns `IdleResponse::Timeout` or `IdleResponse::NewData`, the relay task is still blocked on `task_rx.recv().await`. At this point:
- Calling `relay.await` deadlocks (relay is waiting for recv that may never come)
- Calling `relay.abort()` and `relay.await.ok()` destroys the task, dropping `task_rx`
- No `task_rx` remains — the next IDLE cycle has no way to receive tasks

The `JoinHandle` returns the receiver only if the task ran to completion naturally (i.e., only on the ManualInterrupt path). On the Timeout and NewData paths, the relay must be aborted and `task_rx` is destroyed with it.

**The deltachat Pattern C (production-proven):**

Source: `deltachat/deltachat-core-rust` `src/imap/idle.rs` and `src/scheduler.rs`:

The implementation uses TWO channels:
1. `async_channel::bounded::<()>(1)` for wakeup signals — relay task owns the `Receiver<()>`
2. The work queue (equivalent to `task_rx`) lives in the outer scheduler, accessed via `try_recv()` after IDLE exits

```rust
// In scheduler.rs: channel setup
let (idle_interrupt_sender, idle_interrupt_receiver) = async_channel::bounded::<()>(1);

// When a task arrives (from UI or network):
idle_interrupt_sender.try_send(()).ok();  // signal relay to wake IDLE
work_queue.push(task);                   // add to work queue (separate from interrupt channel)

// In idle.rs: the idle function
pub async fn idle(
    mut self,
    context: &Context,
    idle_interrupt_receiver: Receiver<()>,  // unit-only wakeup signal
    folder: &str,
) -> Result<Self> {
    let mut idle = self.inner.take().unwrap().idle();
    idle.init().await?;
    let (idle_wait, interrupt) = idle.wait_with_timeout(IDLE_TIMEOUT);

    // Relay owns StopSource ONLY — wakeup channel is consumed here
    let interrupt_relay = tokio::spawn(async move {
        idle_interrupt_receiver.recv().await.ok();
        drop(interrupt);
    });

    match idle_wait.await {
        Ok(IdleResponse::NewData(_))      => { /* log */ }
        Ok(IdleResponse::Timeout)         => { /* log */ }
        Ok(IdleResponse::ManualInterrupt) => { /* log */ }
        Err(err)                          => { warn!("IDLE error: {err:#}") }
    }

    // Safe abort in ALL branches — relay owns no application data
    interrupt_relay.abort();
    interrupt_relay.await.ok();

    // Finalize IDLE (15-second guard against hung servers)
    let session = tokio::time::timeout(
        Duration::from_secs(15),
        idle.done(),
    ).await??;
    self.inner = Some(session);
    Ok(self)
}

// In the outer loop (scheduler): after idle() returns
while let Some(task) = work_queue.pop() {
    execute_task(task).await?;
}
```

**Recommended Pattern for Phase 8 (complete implementation):**

```rust
use async_imap::extensions::idle::IdleResponse;
use tokio::sync::mpsc;
use std::time::Duration;

pub struct ForegroundWorker {
    session: Session<TlsStream<TcpStream>>,
    task_rx: mpsc::Receiver<TaskKind>,         // NEVER moved into relay task
    interrupt_tx: mpsc::Sender<()>,            // held by stdin reader task
    interrupt_rx: mpsc::Receiver<()>,          // passed to relay each cycle
}

impl ForegroundWorker {
    pub async fn run(mut self) -> Result<()> {
        self.session.select("INBOX").await?;

        loop {
            // Enter IDLE
            let mut idle = self.session.idle();
            idle.init().await?;
            let (idle_wait, interrupt) = idle.wait_with_timeout(Duration::from_secs(25 * 60));

            // Create a one-shot wakeup receiver for this IDLE cycle.
            // Use tokio::sync::oneshot so it cannot be received more than once.
            let (wakeup_tx, wakeup_rx) = tokio::sync::oneshot::channel::<()>();

            // Relay task owns interrupt (StopSource) and wakeup_rx only.
            // self.task_rx remains in the outer loop.
            let relay = tokio::spawn(async move {
                let _ = wakeup_rx.await;  // wait for wakeup signal
                drop(interrupt);          // drop StopSource → ManualInterrupt
            });

            // The stdin reader sends () on interrupt_tx when a task arrives.
            // For the current cycle, we bridge: when interrupt_rx fires, send to wakeup_tx.
            // Alternative: pass interrupt_tx directly to stdin reader per-cycle.
            // Simplest approach: use interrupt_rx in the relay task directly.

            // ... yield control to idle_wait ...
            match idle_wait.await? {
                IdleResponse::NewData(_)      => { /* new mail arrived, will sync on next loop */ }
                IdleResponse::Timeout         => { /* re-IDLE to prevent server timeout */ }
                IdleResponse::ManualInterrupt => { /* task arrived or shutdown */ }
            }

            // Always abort relay — it owns no application data
            relay.abort();
            relay.await.ok();

            // Always finalize IDLE with 15s guard
            self.session = tokio::time::timeout(
                Duration::from_secs(15),
                idle.done(),
            ).await??;

            // Drain tasks that arrived during IDLE
            while let Ok(task) = self.task_rx.try_recv() {
                execute_task(&mut self.session, task).await?;
            }
            // Loop back to re-enter IDLE
        }
    }
}

// In process main — the stdin reader sends on BOTH channels when a task arrives:
// interrupt_tx.try_send(()).ok();     ← wakes IDLE relay
// task_tx.send(task).await.ok();     ← delivers task to outer loop
```

**Simplest correct implementation using async_channel (matches deltachat exactly):**

```rust
// Two channels at startup:
let (task_tx, task_rx) = tokio::sync::mpsc::channel::<TaskKind>(32);
let (interrupt_tx, interrupt_rx) = async_channel::bounded::<()>(1);

// stdin reader task:
tokio::spawn(async move {
    while let Some(line) = stdin_lines.next().await {
        if let Ok(StdinCommand::QueueTask { task }) = serde_json::from_str(&line) {
            let _ = task_tx.send(task).await;
            let _ = interrupt_tx.try_send(());  // wake IDLE (non-blocking, bounded 1)
        }
    }
});

// In the IDLE loop, pass a CLONE of interrupt_rx to each cycle:
// async_channel::Receiver is Clone — same underlying channel, new receiver handle
let cycle_interrupt_rx = interrupt_rx.clone();
let relay = tokio::spawn(async move {
    cycle_interrupt_rx.recv().await.ok();
    drop(interrupt);
});
```

**Why async_channel is preferred over tokio::sync::mpsc for the interrupt signal:**
- `async_channel::Receiver` implements `Clone`, so each IDLE cycle can clone the receiver without recreating the channel
- The bounded capacity of 1 means `try_send` is non-blocking and at-most-one pending wakeup is queued
- `tokio::sync::mpsc::Receiver` is NOT Clone, requiring either recreation per cycle or a wrapper

**Summary of pattern comparison:**

| Pattern | task_rx in relay? | Abort safe? | select! compatible? | Production use? |
|---------|-------------------|-------------|---------------------|-----------------|
| A: relay-and-return | YES — broken | NO — drops task_rx | N/A | No |
| B: oneshot + select! | No | N/A | NO — IDLE future is `'_` borrow | No |
| C: deltachat relay-with-two-channels | NO — safe | YES | N/A (not needed) | deltachat-core-rust |

Use Pattern C.
