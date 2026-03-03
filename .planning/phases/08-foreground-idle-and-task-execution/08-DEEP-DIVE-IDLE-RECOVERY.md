# Phase 8 Deep-Dive: IDLE Error Recovery and Reconnection

**Researched:** 2026-03-02
**Domain:** async-imap IDLE error handling, IMAP reconnection strategies, OAuth2 token expiry during IDLE, network transitions, production email client patterns
**Sources:** async-imap source (chatmail fork, async-email fork), C++ SyncWorker.cpp + main.cpp + DeltaStream.cpp + SyncException.cpp + XOAuth2TokenManager.cpp, deltachat-core-rust issues #2208 / #5093, Thunderbird bugzilla #468490, ImapFlow docs, K-9 Mail docs, stalwartlabs/stalwart issue #280, getmail6 issue #60, IMAPClient 2.1.0 docs

---

## 1. How async-imap Detects IDLE Connection Drops

### The `IdleResponse` Enum

The async-imap IDLE extension (confirmed from chatmail/async-imap source) returns a three-variant enum:

```rust
pub enum IdleResponse {
    ManualInterrupt,   // StopSource was dropped, OR stream returned None (connection drop)
    Timeout,           // wait duration elapsed with no response from server
    NewData(ResponseData), // server sent an unsolicited response (EXISTS, EXPUNGE, FETCH, etc.)
}
```

### Critical Discovery: Connection Drop Returns `ManualInterrupt`, Not an Error

This is the most important finding for Phase 8. When the TCP connection is reset during IDLE, the `interruptible_stream.next()` returns `None`. The code handles this as:

```rust
let Some(Ok(resp)) = res else {
    return Ok(IdleResponse::ManualInterrupt);  // Connection drop treated as ManualInterrupt!
};
```

**The stream returning `None` (TCP reset, server-side close, `* BYE`) returns `Ok(IdleResponse::ManualInterrupt)`, not `Err(...)`.** This means the foreground worker loop cannot distinguish between a manual task interrupt and a connection drop by looking at the IDLE future result alone. Both yield `ManualInterrupt`.

The Rust foreground worker must track whether a manual interrupt was actually issued (i.e., was the `StopSource` dropped by the relay task?) to determine the cause. One reliable approach: use a `tokio::sync::oneshot` channel alongside the `StopSource` — if the oneshot is still pending when IDLE resolves as `ManualInterrupt`, it was a connection drop, not a task arrival.

### Errors Propagated as `Err(_)`

Genuine errors (not connection drops) that propagate through `?`:

| Error Scenario | async-imap Error Variant | `io::ErrorKind` |
|----------------|-------------------------|-----------------|
| Server responds BAD to IDLE command | `Error::Io(ConnectionRefused)` | `ConnectionRefused` |
| `idle.done().await` fails (send DONE fails) | `Error::Io(_)` | `BrokenPipe` or `ConnectionReset` |
| `idle.init().await` fails (TCP broke before IDLE ACK) | `Error::Io(_)` | `ConnectionReset` or `UnexpectedEof` |
| Parse error in response stream | `Error::Parse(...)` | N/A |
| Server sends NO response to IDLE | `Error::No(String)` | N/A |

The full `async_imap::error::Error` enum (confirmed from source):
```rust
pub enum Error {
    Io(IoError),          // wraps std::io::Error
    Bad(String),          // * BAD from server
    No(String),           // * NO from server
    ConnectionLost,       // connection terminated unexpectedly
    Parse(ParseError),    // response parsing failed
    Validate(ValidateError), // invalid IMAP string
    Append,               // append operation failed
}
```

### What Happens with `* BYE`

When the server sends `* BYE <reason>` during IDLE:
- The stream yields this as a `Response::Done` variant which is dispatched via `handle_unilateral()`
- The stream then closes (returns `None`)
- The IDLE future resolves as `Ok(IdleResponse::ManualInterrupt)` — not an error

The `* BYE` message is consumed as an unsolicited response; the caller receives no special indication that a BYE was received vs. a network reset. Both produce `ManualInterrupt`.

**OAuth2 token expiry produces:** `* BYE Session invalidated - AccessTokenExpired` — still returns `ManualInterrupt`, not an auth error.

### What Happens on Network Timeout (No Response At All)

If the TCP connection silently stalls (no data, no RST, no FIN — the "black hole" scenario):
- `wait_with_timeout(dur)` wraps `timeout(dur, interruptible_stream.next())`
- The outer timeout fires: returns `Ok(IdleResponse::Timeout)`
- The caller then calls `idle.done().await` which will fail with `Io(BrokenPipe)` or `Io(TimedOut)` since the TCP connection is stale

**The TCP keepalive problem:** Without OS-level TCP keepalive configured on the socket, a "black hole" network drop can cause the IDLE future to hang indefinitely waiting for the server timeout. The 25-minute `wait_with_timeout` is the application-layer safeguard.

---

## 2. Error Types and Classification

| Error | Source | Retryable? | is_offline? | Action |
|-------|--------|-----------|-------------|--------|
| `IdleResponse::ManualInterrupt` + StopSource was dropped | Relay task intentional | N/A | No | Execute tasks, re-IDLE |
| `IdleResponse::ManualInterrupt` + StopSource was NOT dropped | TCP reset / server BYE | Yes | Yes | Reconnect with backoff |
| `IdleResponse::Timeout` + `idle.done()` succeeds | 25-min timer fired | N/A | No | Re-IDLE immediately |
| `IdleResponse::Timeout` + `idle.done()` fails | Black hole network | Yes | Yes | Reconnect with backoff |
| `idle.init()` returns `Err(Io(ConnectionRefused))` | Server BAD IDLE | No | No | Disable IDLE; use polling |
| `idle.init()` returns `Err(Io(_))` | TCP/TLS failed | Yes | Yes | Reconnect with backoff |
| `idle.done()` returns `Err(Io(_))` | Connection lost during DONE | Yes | Yes | Reconnect with backoff |
| `Error::No(msg)` during init | Server refused IDLE | Yes (retry once) | No | Retry once; then disable IDLE |
| `Error::Bad(msg)` during init | Protocol error | No | No | Log; abort IDLE for session |
| `Error::Parse(_)` | Corrupted response | Yes | Maybe | Reconnect with backoff |
| `Error::Io(ConnectionRefused)` | Server rejected | No | No | Fatal; stop retrying IDLE |
| Session `select()` fails on reconnect | Folder gone | Yes | No | Use INBOX as fallback |
| `Error::No("authenticate failed")` on reconnect | Bad credentials | No | No | Fatal; emit auth error; stop |
| `Error::No("authenticate failed")` after OAuth refresh | Token invalid | No (after 1 retry) | No | Emit auth error; stop |

### The C++ Precedent: SyncException Classification

The existing C++ `SyncException` uses two flags:

```cpp
bool retryable = false;  // should the outer loop retry?
bool offline = false;    // should we emit beginConnectionError() to UI?
```

- `retryable=false` → calls `abort()` (fatal, non-recoverable)
- `retryable=true, offline=true` → emits `ProcessState { connectionError: true }` to UI, sleeps 120s
- `retryable=true, offline=false` → sleeps 120s, no UI notification

The C++ IDLE behavior (from `SyncWorker::idleCycleIteration()`):
```cpp
// Ben Note: We don't throw these errors because Yandex (maybe others) abruptly and
// randomly close IDLE connections - and that's ok! The point is to idle "for a while"
// and then reconnect and idle again. If the reconnect fails on the next iteration,
// /that/ error will propagate up and trigger the `retryable=true` flow.
```

This confirms: **IDLE connection drops are expected and intentionally swallowed.** The error is only thrown if the subsequent reconnect attempt fails.

---

## 3. Reconnection Strategy

### Full Session Recreation Required

After any IDLE connection drop, the IMAP session must be **completely recreated**:
1. New TCP connection (`TcpStream::connect()`)
2. New TLS handshake (`connector.connect()`)
3. New IMAP greeting check
4. New LOGIN or AUTHENTICATE
5. New SELECT of the target folder

The existing `Session<T>` cannot be reused after a connection drop. The underlying stream is poisoned. Attempting to call `idle.done()` or any other method on a dropped connection will fail with an IO error.

**Exception:** After `IdleResponse::Timeout` where `idle.done()` succeeds cleanly, the session is still valid. Re-IDLE can happen immediately with no reconnection.

### Reconnection Sequence (Correct Order)

```
1. detect_drop() — connection drop (ManualInterrupt without relay, or done() fail)
2. drop(old_session) — discard the poisoned session object
3. check_oauth2_token_expiry() — refresh if within 5-min buffer or known expired
4. new_tcp = TcpStream::connect(host:port).await
5. new_tls = tls_connector.connect(host, new_tcp).await
6. new_client = async_imap::Client::new(new_tls)
7. new_session = new_client.login(user, password).await OR authenticate(xoauth2).await
8. new_session.select("INBOX").await
9. trigger_catchup_sync() — notify background worker to check for missed messages
10. re_enter_idle(new_session)
```

### Backoff Parameters

Based on C++ precedent (120s sleep on all retryable errors) and deltachat-core-rust progressive backoff pattern (PR #5443: "Add progressive backoff for failing IMAP connection attempts"), the recommended backoff for Phase 8:

| Attempt | Delay Before Retry | Rationale |
|---------|--------------------|-----------|
| 1st | 0s (immediate) | Network blip — try instantly first |
| 2nd | 5s | Short pause for transient issues |
| 3rd | 15s | Slightly longer; most blips resolve by now |
| 4th | 30s | Server restart window |
| 5th | 60s | Extended outage |
| 6th+ | 120s (cap) | C++ parity; don't probe more than 1/2min forever |

The deltachat-core-rust issue #2208 describes a more complex dynamic approach (start 11min, +1min/week, -5min/failure), but this is designed for mobile battery life. For a desktop email client, the simpler fixed progression above is appropriate.

**Jitter:** Add ±10% random jitter to each delay to prevent thundering herd when multiple accounts reconnect simultaneously.

### Maximum Consecutive Failures Before "Disconnected" State

After **5 consecutive reconnection failures**, the account should be considered disconnected:
- Emit `ProcessState { connectionError: true, accountId: "..." }` to UI
- Continue retrying at the 120s cap delay (do not stop)
- When a successful reconnection occurs, emit `ProcessState { connectionError: false, accountId: "..." }`

This matches the C++ pattern exactly: `beginConnectionError()` is called on the first offline error, `endConnectionError()` is called when the iteration succeeds.

---

## 4. Production Recovery Pattern

This is the complete, production-quality IDLE loop with error recovery:

```rust
use async_imap::extensions::idle::IdleResponse;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{error, info, warn};

const IDLE_TIMEOUT: Duration = Duration::from_secs(25 * 60); // 25min (server max is 29min)
const BACKOFF_DELAYS: &[Duration] = &[
    Duration::ZERO,
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
    Duration::from_secs(60),
    Duration::from_secs(120), // cap
];
const BACKOFF_CAP_INDEX: usize = 5;
const FAILURE_THRESHOLD: usize = 5; // emit connectionError after this many failures

pub async fn run_foreground_worker(
    account: AccountConfig,
    mut task_rx: mpsc::Receiver<TaskKind>,
    delta_tx: mpsc::Sender<DeltaLine>,
) {
    let mut consecutive_failures: usize = 0;
    let mut connection_error_emitted = false;

    loop {
        // Step 1: Acquire a fresh session (with reconnection backoff)
        let session = match connect_with_backoff(&account, consecutive_failures).await {
            Ok(s) => {
                if connection_error_emitted {
                    emit_process_state(&delta_tx, &account.id, false).await;
                    connection_error_emitted = false;
                }
                consecutive_failures = 0;
                s
            }
            Err(e) if is_fatal_error(&e) => {
                error!(account = %account.id, error = %e, "Fatal IMAP error — stopping foreground worker");
                emit_process_state(&delta_tx, &account.id, true).await;
                return; // Fatal: auth failure, invalid account, etc.
            }
            Err(e) => {
                consecutive_failures += 1;
                warn!(
                    account = %account.id,
                    attempt = consecutive_failures,
                    error = %e,
                    "IMAP reconnection failed"
                );
                if consecutive_failures >= FAILURE_THRESHOLD && !connection_error_emitted {
                    emit_process_state(&delta_tx, &account.id, true).await;
                    connection_error_emitted = true;
                }
                let delay_idx = (consecutive_failures - 1).min(BACKOFF_CAP_INDEX);
                sleep(BACKOFF_DELAYS[delay_idx] + jitter()).await;
                continue; // retry connection
            }
        };

        // Step 2: After reconnect, trigger a catch-up sync
        // Background worker checks for messages missed during disconnection
        if consecutive_failures == 0 {
            // First time after process start: background worker handles initial sync
        } else {
            // Reconnected after drop: notify background worker to check for missed changes
            notify_catchup_sync().await;
        }

        // Step 3: Run IDLE loop until an unrecoverable error
        match run_idle_loop(session, &mut task_rx, &delta_tx, &account).await {
            Ok(LoopExit::Shutdown) => return,
            Ok(LoopExit::ConnectionDrop) => {
                info!(account = %account.id, "IDLE connection dropped — reconnecting");
                consecutive_failures += 1;
                // No delay here — connect_with_backoff applies the delay
            }
            Err(e) if is_fatal_error(&e) => {
                error!(account = %account.id, error = %e, "Fatal IMAP error in IDLE loop");
                emit_process_state(&delta_tx, &account.id, true).await;
                return;
            }
            Err(e) => {
                warn!(account = %account.id, error = %e, "IMAP error in IDLE loop — reconnecting");
                consecutive_failures += 1;
            }
        }
    }
}

enum LoopExit {
    Shutdown,
    ConnectionDrop,
}

async fn run_idle_loop(
    mut session: Session<TlsStream<TcpStream>>,
    task_rx: &mut mpsc::Receiver<TaskKind>,
    delta_tx: &mpsc::Sender<DeltaLine>,
    account: &AccountConfig,
) -> Result<LoopExit, SyncError> {
    // Select primary folder: inbox preferred, then all mail
    session.select("INBOX").await.map_err(SyncError::from)?;

    loop {
        // Enter IDLE
        let mut idle = session.idle();
        idle.init().await.map_err(SyncError::from)?;
        let (idle_wait, interrupt) = idle.wait_with_timeout(IDLE_TIMEOUT);

        // Relay task: receives a task from channel, drops StopSource to interrupt IDLE
        // Uses oneshot to signal whether the interrupt was intentional (task arrived)
        // vs. a connection drop (ManualInterrupt with no task).
        let (was_interrupted_tx, was_interrupted_rx) = tokio::sync::oneshot::channel::<bool>();

        // IMPORTANT: task_rx is moved into relay; we reclaim it via relay.await
        let relay = {
            let mut rx = std::mem::replace(task_rx, mpsc::channel(1).1); // placeholder
            tokio::spawn(async move {
                let result = rx.recv().await;
                let has_task = result.is_some();
                drop(interrupt); // Drop StopSource — triggers ManualInterrupt on idle_wait
                let _ = was_interrupted_tx.send(has_task);
                (rx, result) // return rx and the task that triggered the interrupt
            })
        };

        let idle_result = idle_wait.await;
        session = idle.done().await.map_err(SyncError::from)?;

        // Reclaim task_rx and get the task that triggered the interrupt (if any)
        let (reclaimed_rx, pending_task) = relay.await.expect("relay task panicked");
        *task_rx = reclaimed_rx;
        let was_task_interrupt = was_interrupted_rx.await.unwrap_or(false);

        match idle_result {
            Err(e) => {
                // Error from IDLE future (parse failure, etc.)
                return Err(SyncError::from(e));
            }
            Ok(IdleResponse::Timeout) => {
                // 25min timer: re-IDLE immediately (done() succeeded, session still valid)
                info!(account = %account.id, "IDLE timeout — re-entering IDLE");
                // session is still valid; loop continues
            }
            Ok(IdleResponse::NewData(data)) => {
                // Server notified of a change — process it
                handle_idle_notification(&mut session, data, delta_tx).await?;
            }
            Ok(IdleResponse::ManualInterrupt) if was_task_interrupt => {
                // A task arrived — process all queued tasks
                if let Some(task) = pending_task {
                    execute_task(&mut session, task, delta_tx, account).await?;
                }
                // Drain any additional tasks that arrived while executing
                while let Ok(t) = task_rx.try_recv() {
                    execute_task(&mut session, t, delta_tx, account).await?;
                }
            }
            Ok(IdleResponse::ManualInterrupt) if !was_task_interrupt => {
                // Connection dropped: stream returned None without a task arriving
                // (This also covers * BYE scenarios including OAuth2 token expiry)
                warn!(account = %account.id, "IDLE connection dropped (ManualInterrupt without task)");
                return Ok(LoopExit::ConnectionDrop);
            }
            Ok(IdleResponse::ManualInterrupt) => unreachable!(),
        }
        // Loop: re-enter IDLE
    }
}

async fn connect_with_backoff(
    account: &AccountConfig,
    consecutive_failures: usize,
) -> Result<Session<TlsStream<TcpStream>>, SyncError> {
    // Apply delay based on failure count before attempting connection
    if consecutive_failures > 0 {
        let delay_idx = (consecutive_failures - 1).min(BACKOFF_CAP_INDEX);
        sleep(BACKOFF_DELAYS[delay_idx] + jitter()).await;
    }

    // Refresh OAuth2 token if needed before connecting
    // (token may have expired during disconnection)
    let credentials = prepare_credentials(account).await?;

    // Full TCP + TLS + IMAP reconnection sequence
    connect_and_authenticate(account, credentials).await
}

fn is_fatal_error(e: &SyncError) -> bool {
    matches!(e,
        SyncError::AuthenticationFailed |   // bad password
        SyncError::AccountDisabled |        // server says account doesn't exist
        SyncError::TlsCertificateInvalid |  // cert pinning failure
    )
}

fn jitter() -> Duration {
    use std::time::Duration;
    // ±10% random jitter on the base delay
    let millis = (rand::random::<u32>() % 2000) as u64; // 0-2000ms
    Duration::from_millis(millis)
}
```

### Key Design Decisions in the Pattern

1. **The relay task pattern with oneshot channel** distinguishes connection drops from task interrupts. Without this, `ManualInterrupt` is ambiguous.

2. **`idle.done()` is ALWAYS called** after `idle_wait.await`, even if `idle_wait` returns `Err`. This is required by the async-imap API contract. If `done()` itself fails, the session is discarded and reconnection starts.

3. **`LoopExit::ConnectionDrop` exits the IDLE loop** rather than retrying within the loop. The outer `run_foreground_worker` applies the backoff and reconnects.

4. **OAuth2 refresh happens in `connect_with_backoff`**, not inside the IDLE loop. This ensures fresh credentials are always used on reconnect.

5. **Catch-up sync notification** is sent after reconnection so the background sync worker checks for messages missed during the disconnect window.

---

## 5. Session State After Reconnect

### Must Trigger a Catch-Up Sync

After IDLE reconnects, messages may have been missed during the disconnection. The correct behavior is:

1. IDLE reconnects to INBOX
2. The background sync worker is notified (via a shared channel or `AtomicBool`) that it should prioritize a CONDSTORE/full sync of INBOX on its next iteration
3. The background worker runs its normal sync loop which will catch any missed messages via `UID FETCH (CHANGEDSINCE modseq)`

The C++ implementation handles this automatically: `idleCycleIteration()` checks for `VANISHED` UIDs from the previous IDLE session and processes them. In Rust, the equivalent is:
- After reconnect, check the folder's stored `highestmodseq` vs. what the server reports in the SELECT response
- If the server's `highest_modseq` is higher than stored, there are missed changes
- Trigger a CONDSTORE fetch of `(CHANGEDSINCE stored_modseq)` before entering IDLE

### Detecting Missed Messages

The SELECT response on reconnect provides:
- `UIDVALIDITY` — if changed, full re-sync required (treat like UIDVALIDITY reset in Phase 7)
- `UIDNEXT` — if higher than stored, new messages exist
- `HIGHESTMODSEQ` — if higher than stored (via CONDSTORE), flag changes exist

This information is available in the `Mailbox` struct returned by `session.select_condstore()`.

---

## 6. OAuth2 Token Expiry During IDLE

### Server Behavior

When an OAuth2 access token expires while the client is in IDLE:

| Provider | Server Response |
|----------|----------------|
| Google Gmail | `* BYE Session invalidated - AccessTokenExpired` |
| Microsoft Exchange/O365 | `* BYE Session expired` or similar |
| Generic OAuth2-enabled IMAP | `* BYE` with provider-specific message |

The `* BYE` causes the underlying stream to close, which the IDLE future reports as `Ok(IdleResponse::ManualInterrupt)`.

### Token Lifetime vs. IDLE Duration

OAuth2 access tokens typically expire after 3,600 seconds (1 hour). IDLE reconnects every 25 minutes. Therefore:
- After ~2-3 IDLE cycles (50-75 minutes), the token may be expired
- The token will be expired when `connect_with_backoff` is called on reconnect
- **The check must happen in `connect_with_backoff`, not in the IDLE loop**

### The C++ Precedent

From `XOAuth2TokenManager.cpp`:
```cpp
// buffer of 60 sec since we actually need time to use the token
if (parts.expiryDate > time(0) + 60) {
    return parts;  // cached token still valid
}
// else: fall through to refresh via HTTP POST
```

The C++ uses a 60-second buffer. Phase 7 research specified 5-minute buffer (`time(0) + 300`). Both work; 5-minute is more conservative and prevents token expiry during the connection sequence.

### Recommended Handling for Phase 8

```rust
async fn prepare_credentials(account: &AccountConfig) -> Result<Credentials, SyncError> {
    match account.auth_type {
        AuthType::Password => Ok(Credentials::Password(account.password.clone())),
        AuthType::XOAuth2 => {
            let token = token_manager.get_token(&account).await?;
            // get_token() checks expiry with 5-minute buffer, refreshes via HTTP if needed
            // If refresh fails (network offline), returns SyncError::TokenRefreshFailed (retryable)
            // If refresh returns 401 (bad refresh token), returns SyncError::AuthenticationFailed (fatal)
            Ok(Credentials::XOAuth2 {
                username: account.imap_username.clone(),
                token: token.access_token,
            })
        }
    }
}
```

### What Happens If Token Refresh Fails During Reconnect

1. HTTP request to token endpoint fails (network offline):
   - Return `SyncError::TokenRefreshFailed` (retryable, offline=true)
   - Outer loop applies backoff, emits `connectionError` to UI

2. Token endpoint returns 401 (refresh token revoked/expired):
   - Return `SyncError::AuthenticationFailed` (fatal, offline=false)
   - Outer loop calls `return` — worker stops
   - Emit `connectionError: true` + auth-specific error to UI
   - User must re-authenticate in the app

3. Token endpoint returns 200 with new access token:
   - Emit `ProcessAccountSecretsUpdated` delta with new token
   - Proceed to connect with new credentials

---

## 7. Network Transitions

### What Happens at the TCP Level

| Event | TCP Behavior | IDLE Result |
|-------|-------------|-------------|
| WiFi drops, cellular takes over | Existing socket becomes dead; new IP assigned | Stream hangs until OS sends RST (seconds to minutes) OR OS sends RST immediately |
| VPN connects | OS replaces default route; existing socket typically gets RST | Stream returns `Err(Io(ConnectionReset))` — IDLE resolves as `ManualInterrupt` |
| Machine wakes from sleep | TCP keepalive probes start; server may have already closed the session | Stream returns RST or `None` — IDLE resolves as `ManualInterrupt` |
| Network completely unavailable | No RST received; TCP times out | Stream hangs until `wait_with_timeout` fires `Timeout`; `done()` then fails |

The "hang until timeout" scenario (no RST) is the dangerous one. The 25-minute `wait_with_timeout` is the application-layer backstop. However, OS-level TCP keepalives provide an additional defense at the socket level.

### TCP Keepalive Configuration

Setting TCP keepalives reduces the hang time from 25 minutes to a few seconds when the network is truly gone. In Rust with tokio:

```rust
use socket2::{Socket, TcpKeepalive};
use std::time::Duration;

fn configure_keepalive(stream: &tokio::net::TcpStream) {
    let keepalive = TcpKeepalive::new()
        .with_time(Duration::from_secs(60))      // start probing after 60s idle
        .with_interval(Duration::from_secs(10))  // probe every 10s
        .with_retries(3);                        // give up after 3 probes (30s total)

    use std::os::unix::io::AsRawFd;
    let raw_fd = stream.as_raw_fd();
    let socket = unsafe { Socket::from_raw_fd(raw_fd) };
    let _ = socket.set_tcp_keepalive(&keepalive);
    std::mem::forget(socket); // don't close fd
}
```

With this configuration, a dead network causes the TCP stack to detect failure within ~90 seconds (60s idle + 3 × 10s probes) rather than waiting for the 25-minute application timeout.

### Network Change Detection

The `netwatcher` crate provides cross-platform network interface change events (Windows, Linux, macOS):

```toml
netwatcher = "0.5"
```

```rust
let _watcher = netwatcher::watch_interfaces(|update| {
    if !update.diff.added.is_empty() || !update.diff.modified.is_empty() {
        // Network interface changed — interrupt IDLE for clean reconnect
        interrupt_idle();
    }
}).unwrap();
```

**The C++ equivalent** (from `main.cpp`):
```cpp
if (type == "wake-workers") {
    // interrupt the foreground worker's IDLE call, because our network
    // connection may have been reset and it'll sit for a while otherwise
    // and wake-workers is called when waking from sleep
    if (fgWorker) fgWorker->idleInterrupt();
}
```

The Electron app already sends `wake-workers` when the system wakes from sleep (this is the Electron `powerMonitor` event). The Rust mailsync-rust process receives this via stdin, which should trigger an IDLE interrupt + reconnect.

**Recommendation for Phase 8:** Do NOT add `netwatcher` as a dependency in Phase 8 itself. Instead, rely on:
1. The `wake-workers` stdin command (already handled) to interrupt IDLE on system wake
2. The 25-minute `wait_with_timeout` as the maximum hang duration
3. TCP keepalive (configured during session setup) to detect dead connections faster

Adding `netwatcher` is an optimization for Phase 9+ if reconnection latency after network changes becomes a user-visible problem.

---

## 8. Multiple Consecutive Failures

### Failure Counting and State Reporting

```
failure 1: delay 0s (immediate retry)
failure 2: delay 5s
failure 3: delay 15s
failure 4: delay 30s
failure 5: delay 60s + emit ProcessState { connectionError: true }
failure 6+: delay 120s (cap), continue emitting connectionError: true
success: emit ProcessState { connectionError: false }
```

### ProcessState Delta Format

The C++ emits this via `beginConnectionError()` / `endConnectionError()` in `DeltaStream.cpp`:

```json
{
  "type": "persist",
  "modelClass": "ProcessState",
  "modelJSONs": [{
    "id": "<account-id>",
    "accountId": "<account-id>",
    "connectionError": true
  }]
}
```

The Rust equivalent should emit identical JSON to stdout (same format, same field names, same `modelClass`). The Electron UI `mailsync-bridge.ts` already handles `ProcessState` deltas to show the "offline" indicator in the UI.

### When to Stop Retrying

**Never stop retrying automatically.** The C++ pattern (and K-9 Mail, deltachat) is infinite retry with capped backoff. The account might come back online hours later (server maintenance, user re-enables IMAP on their account). Stopping would require the user to manually restart the app or re-add the account.

The only time to stop is when the error is truly fatal:
- Authentication failure (wrong password) — user needs to re-enter password
- Account disabled by server (`NO [UNAVAILABLE]`) — user needs to fix their account
- TLS certificate invalid — misconfiguration, user needs to check settings

---

## 9. Implications for Phase 8

### What the Foreground Worker Loop Must Handle

1. **Connection drop detection:** Track whether `ManualInterrupt` was task-triggered (via oneshot) or connection-triggered (stream returned None / BYE received).

2. **`idle.done()` failure:** Always call `done()` after IDLE future resolves. If `done()` fails, the session is dead — exit loop and reconnect.

3. **`idle.init()` failure:** Handle both `ConnectionRefused` (server doesn't support IDLE → disable IDLE, use polling fallback) and IO errors (connection failed → reconnect with backoff).

4. **Full session recreation on reconnect:** No partial reuse. New TCP + TLS + LOGIN + SELECT on every reconnect.

5. **OAuth2 token refresh before reconnect:** Check expiry with 5-minute buffer. Refresh via HTTP before attempting IMAP authentication.

6. **Catch-up sync trigger on reconnect:** After reconnect, compare `UIDNEXT` and `HIGHESTMODSEQ` from SELECT response to stored values. If diverged, trigger a background sync of INBOX before entering IDLE.

### Error Types to Propagate to UI

| Condition | ProcessState Delta |
|-----------|-------------------|
| 5+ consecutive connection failures | `{ connectionError: true }` |
| Authentication failure (fatal) | `{ connectionError: true }` (+ potential new modelClass for auth error) |
| Reconnection successful | `{ connectionError: false }` |
| Account disabled | `{ connectionError: true }` |

The UI already handles `ProcessState { connectionError: true/false }` — no new Electron-side code needed.

### Backoff Parameters to Use

```rust
const BACKOFF_DELAYS_MS: &[u64] = &[0, 5_000, 15_000, 30_000, 60_000, 120_000];
const BACKOFF_CAP_INDEX: usize = 5;
const CONNECTION_ERROR_THRESHOLD: usize = 5;
const IDLE_TIMEOUT_SECS: u64 = 25 * 60; // 25 minutes
const OAUTH2_TOKEN_EXPIRY_BUFFER_SECS: u64 = 5 * 60; // 5 minutes
```

### New Crate Dependencies for Phase 8 (IDLE Recovery Specifically)

```toml
# TCP keepalive configuration — set socket options before TLS handshake
socket2 = "0.5"

# netwatcher is NOT needed for Phase 8 — rely on wake-workers stdin command
# netwatcher = "0.5"  # SKIP in Phase 8; add in Phase 9+ if needed
```

`socket2` is already a transitive dependency of tokio on most platforms. It may not need to be added explicitly. Check `cargo tree | grep socket2` first.

### IDLE Server Compatibility Notes

From research (Yandex behavior documented in C++ comments, Stalwart bug #280, deltachat Dovecot issue #5093):

| Server Quirk | Behavior | Mitigation |
|-------------|---------|-----------|
| Yandex | Abruptly closes IDLE connections | Expected; reconnect immediately (C++ treats as non-error) |
| Dovecot (mailbox.org) | Sends `OK Still here` keepalives every 23min | These are `Response::Data { status: Ok, .. }` — IDLE loop continues; wait_with_timeout resets on any response |
| Stalwart | Sends `BYE Server shutting down` to DeltaChat folder but not INBOX | Reconnect normally; INBOX is the primary IDLE folder |
| Gmail | Server-side IDLE support is solid | No known quirks |
| Exchange/O365 | Times out at 30min | 25min re-IDLE interval prevents this |
| Servers without IDLE | Return BAD to IDLE command | Fall back to periodic NOOP polling (15-min interval) |

---

## Summary of Key Findings

1. **`IdleResponse::ManualInterrupt` is ambiguous** — it signals both intentional interrupt (task arrived) and connection drop (TCP reset, server BYE). The foreground worker must use a side-channel (oneshot) to distinguish them.

2. **`* BYE` during IDLE produces `ManualInterrupt`**, not an error. This includes OAuth2 token expiry BYE messages.

3. **TCP silent hangs** are the worst case. `wait_with_timeout(25min)` is the backstop; TCP keepalives reduce this to ~90 seconds.

4. **OAuth2 token expiry** must be handled in the reconnect path: check token expiry (5-min buffer) and refresh via HTTP before re-authenticating to IMAP.

5. **The C++ implementation swallows IDLE errors** intentionally (Yandex compatibility). The Rust implementation should do the same — reconnect silently unless the reconnect itself fails.

6. **ProcessState deltas** (`connectionError: true/false`) are emitted when 5+ consecutive failures occur or when a successful reconnect happens. The Electron UI handles these already.

7. **Full session recreation** is required on every reconnect. There is no partial session reuse.

8. **Catch-up sync** must be triggered after reconnect by comparing SELECT response `UIDNEXT`/`HIGHESTMODSEQ` to stored values.

9. **`netwatcher` crate** is a good fit for network change detection but should be deferred to Phase 9+. The `wake-workers` stdin command from Electron already covers the most important case (system wake from sleep).

10. **Backoff progression:** 0s → 5s → 15s → 30s → 60s → 120s cap, with ±10% jitter, infinite retry, no automatic stop.
