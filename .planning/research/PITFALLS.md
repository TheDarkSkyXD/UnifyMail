# Pitfalls Research

**Domain:** Rewriting a C++ IMAP/SMTP/CalDAV sync engine in Rust for an Electron desktop email client (stdin/stdout JSON IPC)
**Researched:** 2026-03-02
**Confidence:** HIGH (IPC protocol verified against live UnifyMail codebase; IMAP/CONDSTORE pitfalls verified against RFC 7162 and Delta Chat implementation reports; SQLite async pitfalls verified against rusqlite/tokio-rusqlite docs and GitHub issues; TLS confirmed HIGH from v1.0 research)

---

> **Scope note:** This file covers v2.0 mailsync engine pitfalls only. The v1.0 napi-rs addon
> pitfalls (Tokio runtime lifecycle, V8 memory cage, BoringSSL, asar packaging of .node files)
> are documented in the original PITFALLS.md written for v1.0. Do not duplicate those here —
> the v2.0 engine is a standalone binary, not a Node.js addon.

---

## Critical Pitfalls

### Pitfall 1: stdout Buffering Causes the Electron UI to Never Update

**Severity:** CRITICAL

**What goes wrong:**
When Rust writes to stdout via `println!` or `serde_json::to_writer(std::io::stdout(), ...)`, the
output is buffered in the OS pipe buffer. When stdout is not connected to a TTY (which is always
the case when spawned as a child process by Node.js), Rust's stdout switches to full block
buffering. This means deltas written to stdout pile up in a buffer and are not delivered to the
Electron parent process until the buffer flushes — which may be thousands of lines later, at
process exit, or never if the process is killed. The Electron `_onIncomingMessages` handler in
`mailsync-bridge.ts` never fires, the UI never updates, and emails appear not to sync.

Additionally, tokio issue #7174 documents that when `process::exit()` is called (e.g., on fatal
error), tokio's async I/O does not guarantee stdout is flushed first, so the final status JSON
message that `mailsync-process.ts` parses from the last line of output can be silently lost.

**Why it happens:**
The C++ mailsync engine used `fflush(stdout)` after every write. Developers porting to Rust reach
for `println!` or `writeln!(stdout, ...)` which feel idiomatic but do not flush. Rust's `print!`
macro is documented as not flushing; `println!` is documented as flushing only when connected to
a TTY.

**How to avoid:**
- Use a dedicated stdout writer with explicit flush after every message:
  ```rust
  use std::io::{self, Write};
  let stdout = io::stdout();
  let mut handle = stdout.lock();
  serde_json::to_writer(&mut handle, &delta)?;
  handle.write_all(b"\n")?;
  handle.flush()?;
  ```
- Alternatively, wrap stdout in a `BufWriter` and flush after every newline-delimited message:
  ```rust
  let mut out = BufWriter::new(io::stdout());
  // ... write message ...
  out.flush()?;
  ```
- Before any `process::exit()` call, flush stdout synchronously. In async contexts, call
  `tokio::io::AsyncWriteExt::flush` on the tokio stdout handle before exiting.
- Write a test that spawns the Rust binary as a child process and asserts that a delta arrives
  within 500ms of the triggering event — not just that the binary exits eventually.

**Warning signs:**
- The UI does not update even though the binary is running (check with `ps`).
- Deltas arrive in large batches rather than one at a time.
- Reducing sync interval or adding sleep calls between writes suddenly "fixes" the issue.
- The final error JSON (parsed by `mailsync-process.ts`) is sometimes missing after a non-zero
  exit code.

**Phase to address:**
Phase 1 (IPC scaffolding) — establish the stdout flush pattern in the skeleton binary before
writing a single line of IMAP code. This is a go/no-go gate for the entire delta protocol.

---

### Pitfall 2: stdin Pipe Buffer Fills and Deadlocks Under Large Task Payloads

**Severity:** CRITICAL

**What goes wrong:**
The existing `mailsync-process.ts` configures stdin's `highWaterMark` to 1 MB (it is documented
in a comment: "Allow us to buffer up to 1MB on stdin instead of 16k. This is necessary because
some tasks ... can be gigantic amounts of HTML"). The OS pipe buffer on Linux is 64 KiB by
default. If the Rust engine reads stdin synchronously (blocking the main thread or a tokio thread)
while the Electron side is writing, and if the Electron write exceeds the OS buffer before the
engine drains it, the write blocks on the Electron side. Simultaneously, if the Rust engine is
trying to write a delta to stdout while Electron's stdout read buffer is full, both processes
deadlock: each is blocked waiting for the other to read.

This is a documented Rust issue (rust-lang/rust#45572: "process::Command hangs if piped stdout
buffer fills") and is platform-specific — more likely to manifest with large email bodies on
Linux than on macOS.

**Why it happens:**
The C++ engine used separate threads for stdin reading and stdout writing, which prevents
deadlock by design. Developers writing the Rust engine in async may assume tokio's scheduler
prevents blocking, but if stdin is read in a `spawn_blocking` that holds a lock while the
async stdout writer is also waiting, the scheduler cannot resolve the deadlock.

**How to avoid:**
- Read stdin on a dedicated async task (`tokio::spawn`) that continuously feeds a `tokio::sync::mpsc`
  channel. Never hold a blocking read on the main async task.
- Write stdout on a separate dedicated writer task that drains a `tokio::sync::mpsc` channel.
- The stdin reader task and the stdout writer task must never share a mutex or wait on each other.
- Test with large task payloads: queue a task with a 500 KB HTML body via stdin and verify
  the engine processes it and emits a delta within 5 seconds.

**Warning signs:**
- Hanging only with large emails or when many tasks are queued simultaneously.
- `strace` on Linux shows the Rust binary blocked in `write(1, ...)` (stdout) while the
  Electron side is blocked in `write(child.stdin, ...)`.
- Works fine with small payloads, breaks intermittently with large drafts.

**Phase to address:**
Phase 1 (IPC scaffolding) — the stdin/stdout concurrency model must be established before
any task processing logic is written.

---

### Pitfall 3: Blocking SQLite Operations on the Tokio Async Thread Pool Cause Starvation

**Severity:** CRITICAL

**What goes wrong:**
`rusqlite::Connection` is synchronous and blocking. Calling it directly inside an `async fn`
body blocks the tokio worker thread executing the future. SQLite's `fsync()` on a slow disk
(or when WAL checkpointing is running) can block for hundreds of milliseconds. Under load with
multiple concurrent IMAP folders syncing simultaneously, all tokio worker threads become
occupied with blocked SQLite calls. New async tasks — including the stdin reader and stdout
writer — cannot be scheduled. The engine appears frozen despite the binary running.

Additionally, rusqlite issue #697 documents that transactions cannot safely span `.await` points
because `Connection` is not `Send` and the transaction lifetime cannot cross async boundaries.
Naive attempts to use `async fn` with rusqlite and transactions will fail to compile or panic.

**Why it happens:**
Developers accustomed to `async/await` assume all I/O in the tokio world is non-blocking.
SQLite is not async. The compile error from trying to hold a rusqlite transaction across an
`.await` is confusing; the workaround of calling `connection.execute()` directly inside an
`async fn` (without `spawn_blocking`) silently blocks threads.

**How to avoid:**
- Use `tokio-rusqlite` (crates.io), which wraps every `rusqlite` call in `spawn_blocking`
  internally and serializes all access through a dedicated SQLite thread per connection:
  ```rust
  use tokio_rusqlite::Connection;
  let conn = Connection::open("mailsync.db").await?;
  conn.call(|db| {
      db.execute("INSERT INTO messages ...", params![])?;
      Ok(())
  }).await?;
  ```
- Never call `rusqlite` methods directly inside an `async fn` without going through
  `tokio-rusqlite` or `tokio::task::spawn_blocking`.
- Enable WAL mode at database open time: `PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;`
  to allow concurrent reads while a write transaction is in progress (matches the existing
  C++ engine's SQLite configuration).
- Set `busy_timeout` to 5000ms to automatically retry on `SQLITE_BUSY` instead of returning
  an error: `PRAGMA busy_timeout = 5000;`
- Use a single writer connection through `tokio-rusqlite` for all writes. Use a separate
  read-only connection pool (or `async-sqlite` with `PoolBuilder`) for queries.

**Warning signs:**
- `cargo check` produces "future cannot be sent between threads safely" when trying to hold
  a `rusqlite::Transaction` across an `.await`.
- Sync appears to stall when 3+ folders are syncing simultaneously.
- `SQLITE_BUSY` errors in logs despite WAL mode being enabled.
- Tokio task count grows indefinitely (visible in `tokio-console`) — threads are all stuck
  in blocking SQLite calls.

**Phase to address:**
Phase 2 (SQLite layer) — must be designed correctly before IMAP sync code is written,
since every sync operation produces database writes.

---

### Pitfall 4: Delta JSON Format Incompatibility Breaks the Electron UI Silently

**Severity:** CRITICAL

**What goes wrong:**
The Electron `mailsync-bridge.ts` parses every newline-delimited JSON message from stdout and
expects exactly this format:
```json
{ "type": "persist" | "unpersist", "modelClass": "Thread" | "Message" | ..., "modelJSONs": [...] }
```
Special message types also exist: `{ "type": "folder-status", ... }` and request/response
messages with `requestId`. If the Rust engine emits JSON with different field names (e.g.
`"objectClass"` instead of `"modelClass"`, or `"objects"` instead of `"modelJSONs"`), the
bridge's condition `if (!modelJSONs || !type || !modelClass)` silently drops the message
with a console warning. The UI never updates. There is no error thrown — the binary is running
fine from Electron's perspective.

Additional format requirements from live code analysis:
- Messages that are not `{` as the first byte are dropped (line 421: `if (msg[0] !== '{')`).
- Response messages must end with `-result` suffix or be exactly `folder-status` to be routed
  to `sendRequest()` callers.
- `ProcessState` and `ProcessAccountSecretsUpdated` as `modelClass` values trigger special
  handling in `OnlineStatusStore` and `KeyManager`, respectively.

**Why it happens:**
Rust structs are serialized with `serde_json` using field names defined in Rust code. Developers
naturally use Rust naming conventions or logical naming that diverges from the TypeScript protocol.
Without a contract test that validates the wire format, the mismatch only becomes apparent at
runtime with silent drops rather than crashes.

**How to avoid:**
- Define delta message types in Rust with exact field names matching the TypeScript protocol:
  ```rust
  #[derive(Serialize)]
  #[serde(rename_all = "camelCase")]  // ensure camelCase field names
  struct DeltaMessage {
      #[serde(rename = "type")]
      msg_type: String,          // "persist" or "unpersist"
      model_class: String,       // maps to "modelClass" in JSON
      model_jsons: Vec<serde_json::Value>, // maps to "modelJSONs" - but see below
  }
  ```
  Important: `"modelJSONs"` uses uppercase `JSON` — serde's `rename_all = "camelCase"` will
  produce `"modelJsons"`. You must use `#[serde(rename = "modelJSONs")]` on that specific field.
- Write a contract test that deserializes every message type the Rust engine emits and
  asserts all required TypeScript keys are present. Run this test against the actual
  TypeScript parsing logic if possible.
- Implement a mock TypeScript consumer (`scripts/mock-receiver.ts`) that validates incoming
  messages — the existing `scripts/mock-mailsync.js` already provides the outgoing half.

**Warning signs:**
- Binary is running (process shows in task manager) but no emails appear in the UI.
- Electron console shows: "Sync worker sent a JSON formatted message with unexpected keys".
- Integration test passes but UI is empty after switching to the Rust binary.
- `modelJSONs` shows as `modelJsons` in received messages (camelCase auto-naming issue).

**Phase to address:**
Phase 1 (IPC scaffolding) — define and test the wire format contract before writing any
model serialization. Create a golden JSON fixture for each message type.

---

### Pitfall 5: IMAP IDLE 29-Minute Timeout Causes Silent Disconnection

**Severity:** HIGH

**What goes wrong:**
RFC 2177 states that IMAP servers MAY treat an IDLE connection as inactive and close it after
their own inactivity timeout, which varies by server but is often around 30 minutes. The client
is required to terminate IDLE and re-issue it at least every 29 minutes to prevent being logged
out. If the Rust engine starts an IDLE session and holds it indefinitely, the server closes the
TCP connection silently (sends FIN). The TCP stack on the client side moves the socket to
`CLOSE-WAIT`. The Rust `async-imap` IDLE handle is still `await`ing on the socket, which is
now closed. The future will never resolve or error — it hangs forever. New email notifications
stop arriving. This is a documented, real-world failure mode observed in Delta Chat's Rust
implementation (deltachat-core-rust issue #5093), where the connection enters `CLOSE-WAIT`
state after ~30 minutes and mail stops being received until the app is restarted.

**Why it happens:**
Developers implement IDLE once and assume TCP keepalives will detect a dead connection. TCP
keepalives operate at the OS level on a multi-minute interval and do not guarantee detection
of server-side session timeout within the IMAP-level 29-minute window. The `async-imap`
IDLE API provides no built-in keepalive mechanism — it is the caller's responsibility.

**How to avoid:**
- Wrap the IDLE `wait()` call in a `tokio::time::timeout` of 25 minutes. On timeout, send
  `DONE` to end IDLE, then immediately re-issue IDLE. This is the standard approach:
  ```rust
  loop {
      let mut idle = session.idle();
      idle.init().await?;
      let result = tokio::time::timeout(
          Duration::from_secs(25 * 60),
          idle.wait()
      ).await;
      idle.done().await?;  // always send DONE before re-using session
      match result {
          Ok(Ok(_)) => { /* server sent notification, handle new mail */ }
          Ok(Err(e)) => { /* IMAP error, reconnect */ }
          Err(_timeout) => { /* 25 min elapsed, re-issue IDLE silently */ }
      }
  }
  ```
- Send NOOP every 15 minutes as an additional keepalive when not in IDLE mode (during
  background folder sync phases).
- Detect `CLOSE-WAIT` by implementing a read-timeout on the underlying stream: if no bytes
  have been received within 60 seconds during IDLE, treat it as a dead connection.
- Test with a mock IMAP server that closes the connection after 60 seconds and verify the
  engine reconnects automatically without manual intervention.

**Warning signs:**
- Email notifications stop arriving after 25-35 minutes of the app being idle.
- `netstat`/`ss` shows a connection to the IMAP server in `CLOSE-WAIT` state while the binary is
  running.
- Log shows IDLE was started but no DONE was ever sent.
- Only affects IDLE path — background sync (periodic folder polling) still works.

**Phase to address:**
Phase 3 (IMAP IDLE implementation) — implement the 25-minute re-IDLE loop as the first
working version. Never write an IDLE implementation without this loop.

---

### Pitfall 6: CONDSTORE/QRESYNC ENABLE Must Precede SELECT — Protocol State Machine Error

**Severity:** HIGH

**What goes wrong:**
RFC 7162 requires that a client using QRESYNC issue `ENABLE QRESYNC` exactly once per
connection, after authentication and before any `SELECT` command. If `SELECT` is issued first
and QRESYNC is then enabled, the server MUST respond with a tagged BAD response. Many
implementations get the ordering wrong: they enable QRESYNC lazily (when they first need it
during a SELECT) rather than eagerly (after authentication). The `async-imap` library does
not enforce this ordering — it exposes CONDSTORE select as a parameter to the SELECT call,
but QRESYNC's ENABLE must be done separately as a raw command.

Additionally, Gmail supports CONDSTORE but does NOT support QRESYNC (as of 2025). Code
that gates sync on QRESYNC availability will silently degrade for all Gmail accounts if
the CONDSTORE-only fallback path is not implemented.

**Why it happens:**
CONDSTORE (flag changes) and QRESYNC (expunge tracking) are defined in the same RFC 7162
and often treated as a unit. Developers assume "if CONDSTORE works, QRESYNC works" — Gmail
proves this wrong. Additionally, the ENABLE command is not a standard IMAP command many
developers are familiar with; it is introduced specifically in RFC 5161 and used only for
a few extensions.

**How to avoid:**
- After authentication and capability fetch, immediately issue:
  1. `ENABLE QRESYNC` if QRESYNC is in capabilities.
  2. Otherwise, record that only CONDSTORE is available and prepare the CONDSTORE-only
     fallback sync path.
- Define a server capability struct early:
  ```rust
  struct ServerCapabilities {
      condstore: bool,
      qresync: bool,   // false for Gmail — always check separately
      idle: bool,
      compress_deflate: bool,
  }
  ```
- Test against a Gmail account and a non-Gmail IMAP server to verify both sync paths.
- When using QRESYNC SELECT, include the stored `UIDVALIDITY` and `MODSEQ` values:
  ```
  SELECT INBOX (QRESYNC (uidvalidity modseq))
  ```
  Missing the MODSEQ parameter causes the server to fall back to full resync, negating
  the performance benefit.

**Warning signs:**
- `SELECT` returns a tagged BAD response on servers that support QRESYNC.
- Gmail accounts never use the QRESYNC code path — check whether Gmail capability
  detection is reaching the fallback branch.
- After reconnect, full folder download occurs instead of incremental update.

**Phase to address:**
Phase 3 (IMAP sync implementation) — define capability detection and ENABLE sequencing
before implementing any SELECT-based sync logic.

---

### Pitfall 7: C++ Protocol Behavioral Contracts Are Implicit — Regression Without Test Coverage

**Severity:** HIGH

**What goes wrong:**
The C++ mailsync engine has accumulated behavioral contracts that are not documented:
- The `modelJSONs` array field name uses uppercase `JSON` (not `Json`).
- `ProcessState` model messages are used by `OnlineStatusStore` to update connection status
  in the UI — not emitting these causes the connection indicator to show "offline" forever.
- The first data sent on stdin after spawn is the account JSON followed by identity JSON,
  each on its own line (see `mailsync-process.ts` line 219). The Rust engine must read and
  parse both before doing anything else.
- The binary is force-killed by `process.kill()` during app quit — it must handle `SIGTERM`
  (Unix) and `WM_CLOSE`/`TerminateProcess` (Windows) without data corruption.
- In "test" mode (spawned with `--mode test`), the binary must write a single JSON result
  object as the last line of stdout before exiting with code 0 for success or non-0 for
  failure, with error keys matching `LocalizedErrorStrings` in `mailsync-process.ts`.

Without a test harness that exercises these contracts, the Rust rewrite can appear functional
while violating any of them.

**Why it happens:**
The contracts live in TypeScript source code (the consumer), not in specification documents.
Developers writing the Rust engine read the TypeScript code carefully for the happy path but
miss edge cases (mode handling, startup handshake, kill handling).

**How to avoid:**
- Extract all protocol contracts into a shared test fixture before writing Rust code:
  ```
  Protocol contracts to implement:
  1. On startup: read two newline-delimited JSON lines from stdin (account, identity)
  2. In "sync" mode: stream delta JSON to stdout indefinitely until killed
  3. In "test" mode: write one result JSON line, exit 0 (success) or non-0 (failure)
  4. In "migrate" mode: write status JSON, exit 0
  5. Handle SIGTERM: flush stdout, exit 0 without SIGABRT
  6. Emit ProcessState messages when connection status changes
  ```
- Run the existing `scripts/mock-mailsync.js` and read its source — it is the reference
  implementation of the protocol for development, already handling the handshake.
- Add integration tests that spawn the Rust binary via `std::process::Command` and verify
  each contract mode independently.

**Warning signs:**
- App shows "connection failed" for an account that is actually syncing (missing ProcessState).
- No mail appears in UI after switching to Rust binary in development mode.
- App crashes on quit with SIGABRT from the child process (not handling SIGTERM correctly).
- "test" mode account validation never returns — binary not exiting after test.

**Phase to address:**
Phase 1 (IPC scaffolding) — define and test every protocol contract before implementing
any sync logic. Build the binary skeleton that handles all modes correctly with no-op
implementations first.

---

### Pitfall 8: TLS BoringSSL Symbol Conflicts on Linux (Identical to v1.0 Risk, Now More Dangerous)

**Severity:** HIGH

**What goes wrong:**
Same root cause as documented in the v1.0 PITFALLS.md, but with higher risk in the mailsync
binary context: the engine is a standalone binary loaded by Electron via `spawn()`. Unlike a
`.node` addon (which is dlopen'd into Electron's process), the sync binary runs in its own
process. This means OpenSSL symbol conflicts do NOT occur between the Rust binary and Electron's
BoringSSL — they are separate processes.

However, the risk remains in a different form: if the Rust engine links against system OpenSSL
(via `native-tls` or `openssl-sys`), the build will fail on Linux CI systems without OpenSSL
dev headers, and on Alpine/musl-based systems (which use a different OpenSSL ABI). Cross-
compilation from x86_64 to arm64 Linux becomes impossible because OpenSSL requires host-specific
compilation.

Use rustls as the sole TLS backend. The reasoning is identical to v1.0: pure Rust, no system
library dependencies, correct certificate validation, works on all target platforms.

**How to avoid:**
- Enforce `cargo tree | grep openssl` returns nothing. Add this as a CI step.
- Use `rustls-platform-verifier` for OS-native cert validation (Windows CertStore, macOS
  Security.framework, Linux system CAs) — as established in v1.0 research.
- In the sync engine, TLS is needed for IMAP (port 993 + STARTTLS on 143), SMTP (port 465
  + STARTTLS on 587), CalDAV (HTTPS), and CardDAV (HTTPS). The `reqwest` HTTP client used
  for CalDAV/CardDAV must be configured with `rustls-tls` feature (not `native-tls-alpn`).

**Warning signs:**
- CI build fails on Ubuntu runners with "cannot find -lssl".
- Cross-compilation to `aarch64-unknown-linux-gnu` fails with OpenSSL linking errors.
- `cargo tree | grep openssl` shows any entry.

**Phase to address:**
Phase 1 (project scaffolding) — lock in `rustls`-only at project creation. This decision
cannot be changed later without auditing all dependencies.

---

### Pitfall 9: IMAP STARTTLS Stream Upgrade Requires Manual Connection Replacement

**Severity:** HIGH

**What goes wrong:**
IMAP STARTTLS (port 143) requires negotiating a plain TCP connection first, issuing the
STARTTLS command, and then upgrading the same TCP socket to a TLS stream. `async-imap` does
expose `starttls()` on the Client, but only for wrapping the connection — the caller is
responsible for providing the TLS connector. The common mistake is passing a `TlsConnector`
that does not have the hostname set for SNI (Server Name Indication), causing TLS handshake
failures on servers that require SNI (most modern servers).

Additionally, some servers (certain corporate Exchange configurations) advertise STARTTLS
but then fail the upgrade with "TLS not available". The code must handle this as a hard
error and report it with the user-friendly string `"StartTLS is not available"` (matching
the key `ErrorStartTLSNotAvailable` in `mailsync-process.ts` `LocalizedErrorStrings`).

**Why it happens:**
TLS connection examples online typically show connecting directly on port 993 (implicit TLS).
The STARTTLS code path is less commonly implemented and easy to get wrong with SNI.

**How to avoid:**
- When connecting on port 143, use `async-imap`'s `connect_starttls` method with an explicit
  hostname for SNI:
  ```rust
  let tls_connector = TlsConnector::from(Arc::new(
      rustls::ClientConfig::builder()
          .with_platform_verifier()
          .with_no_client_auth()
  ));
  let imap_client = async_imap::connect_starttls(
      (hostname.as_str(), 143),
      hostname.as_str(),  // SNI hostname — must match the server certificate
      tls_connector,
  ).await?;
  ```
- Test STARTTLS against at least Gmail (imap.gmail.com:143), Fastmail, and a self-hosted
  Dovecot instance.
- Map `StarttlsUnavailable` variant to the exact error string `"ErrorStartTLSNotAvailable"`.

**Warning signs:**
- `tls handshake failure: unrecognized name` errors — SNI hostname is missing.
- STARTTLS connection succeeds on Gmail but fails on corporate Exchange servers.
- Error message shown in UI is "tls error" rather than "StartTLS is not available".

**Phase to address:**
Phase 3 (IMAP implementation) — implement and test STARTTLS before considering IMAP done.

---

### Pitfall 10: CalDAV/CardDAV ETag-Based Sync Misses Server-Mutated Events

**Severity:** MEDIUM

**What goes wrong:**
RFC 4791 §5.3.4 states that when a server mutates a calendar object resource after PUT (e.g.,
normalizing timezone, adding scheduling metadata), the server MUST NOT return an ETag in the
PUT response. Clients that assume a successful PUT always returns a new ETag will cache a
stale ETag and miss server-side changes. On the next sync, the client compares its cached
(wrong) ETag against the server's actual ETag, finds a mismatch, re-downloads the object,
and the cycle continues — producing a never-ending sync loop that writes to the database on
every cycle and generates unnecessary network traffic.

Additionally, WebDAV sync-tokens (RFC 6578) are not universally supported. iCloud CalDAV
supports sync-tokens; some older Exchange versions and some self-hosted servers do not. Code
that requires sync-tokens will silently fail (no error, just no sync) against these servers.

**Why it happens:**
The CalDAV spec is complex and server behavior is inconsistent. Developers test against a single
server (typically Google Calendar or iCloud) and do not encounter the edge case until users
report sync loops.

**How to avoid:**
- After any PUT to CalDAV, check the response code and ETag header:
  - If the response includes an ETag header: cache it.
  - If the response does NOT include an ETag header (server mutated the object): immediately
    issue a GET request to fetch the stored version and its ETag before updating the local DB.
- For sync strategy, use a two-phase approach:
  1. Prefer REPORT with `sync-token` (RFC 6578) if the server advertises `{DAV:}sync-collection`.
  2. Fall back to REPORT with ETag comparison (fetch all ETags, diff against cached values)
     for servers that do not support sync-tokens.
- Test against Google Calendar, iCloud, and a self-hosted Nextcloud/Baikal instance.

**Warning signs:**
- Sync-loop logs: database writes occurring on every sync cycle for the same event UID.
- ETag stored in DB does not match ETag returned by server's PROPFIND within the same sync.
- CalDAV sync works for Gmail accounts but not for corporate Exchange accounts.

**Phase to address:**
Phase 5 (CalDAV/CardDAV implementation) — design the ETag caching layer correctly from the
start; retrofitting it requires touching every PUT code path.

---

### Pitfall 11: electron-builder Binary Path Resolution Differs Between Dev and Production

**Severity:** MEDIUM

**What goes wrong:**
`mailsync-process.ts` already handles this for the C++ binary:
```typescript
this.binaryPath = path.join(resourcePath, binaryName)
  .replace('app.asar', 'app.asar.unpacked');
```
The `app.asar.unpacked` replacement is required because native binaries cannot be executed from
inside an `.asar` archive — they must be on disk. If the Rust binary is not explicitly listed
in electron-builder's `asarUnpack` (or the equivalent), it will be packed inside `app.asar`,
the path replacement will not find it, and the `else if (fs.existsSync(mockPath))` fallback
will be triggered in production, which does not exist there either. The error thrown is:
"mailsync binary not found at ... and no mock available" — the app cannot sync email at all.

Additionally, the existing dev fallback path checks:
```typescript
path.join(resourcePath, 'mailsync', 'Windows', 'x64', 'Release', binaryName)
```
The Rust build system produces the binary at a different path
(`target/release/mailsync` or `target/x86_64-pc-windows-msvc/release/mailsync.exe`).
The dev fallback path must be updated to match the Rust build output location.

**Why it happens:**
The binary path is hardcoded in TypeScript. When the build system changes (C++ CMake → Cargo),
the output location changes, but the TypeScript code is not automatically updated.

**How to avoid:**
- Update `asarUnpack` in `electron-builder.json` (or equivalent config) to include the
  Rust binary: `"asarUnpack": ["mailsync.bin", "mailsync.exe"]`.
- Update the dev fallback path in `mailsync-process.ts` to match Cargo's output:
  ```typescript
  const devBuildPath = path.join(resourcePath, '..', '..', 'target',
    'release', binaryName);
  ```
- Add a CI step that packages the app with electron-builder and verifies the binary
  exists at the expected unpacked path before running integration tests.

**Warning signs:**
- App works in `npm start` (dev mode) but silently fails to sync after packaging.
- Electron console shows "mailsync binary not found" after distributing to testers.
- The `CLOSE-WAIT` workaround is never triggered — processes are never spawned at all.

**Phase to address:**
Phase 6 (packaging and distribution) — but the `asarUnpack` configuration must be planned in
Phase 1. The dev fallback path must be updated as soon as the Rust binary produces its first
output.

---

### Pitfall 12: OAuth2 Token Expiry During Long Sync Sessions Causes Silent Auth Failure

**Severity:** MEDIUM

**What goes wrong:**
OAuth2 access tokens for Gmail and Outlook expire after 1 hour. The C++ engine uses the
identity JSON (passed via stdin on startup) and calls a refresh endpoint as needed. If the
Rust engine receives a token on startup and uses it throughout the session without refreshing,
IMAP auth failures will start occurring after 60 minutes. The engine may interpret `NO [AUTHENTICATIONFAILED]`
as a permanent authentication failure and emit an `ErrorAuthentication` delta, causing the
Electron UI to mark the account as `SYNC_STATE_AUTH_FAILED` and stop syncing.

**Why it happens:**
The OAuth2 refresh flow is easy to omit when building the IMAP connection layer — the token
"works" during testing because tests run for less than 60 minutes.

**How to avoid:**
- Store the access token and its expiry timestamp. Before each IMAP `AUTHENTICATE XOAUTH2`
  call, check if the token expires within the next 5 minutes and trigger a refresh if so.
- Implement a background task that refreshes tokens every 50 minutes regardless of connection
  state.
- On `NO [AUTHENTICATIONFAILED]`, attempt one token refresh and retry before emitting an auth
  failure delta.
- The Rust engine must call the same OAuth2 token refresh endpoints as the C++ engine. Examine
  the C++ source to find which endpoints and parameters are used.

**Warning signs:**
- Sync stops exactly ~60 minutes after app launch for Gmail/Outlook OAuth2 accounts.
- Logs show `AUTHENTICATIONFAILED` 60 minutes into a session.
- Password-authenticated accounts never exhibit the failure.

**Phase to address:**
Phase 3 (IMAP implementation) — implement token refresh alongside the authentication layer,
not as an afterthought.

---

## Technical Debt Patterns

| Shortcut | Immediate Benefit | Long-term Cost | When Acceptable |
|----------|-------------------|----------------|-----------------|
| `println!` without flush for stdout | Simpler code | Deltas never arrive at Electron; UI never updates | Never — always flush explicitly |
| Calling rusqlite directly in async fn | No wrapper crate needed | Starves tokio thread pool; deadlocks under load | Never — use tokio-rusqlite or spawn_blocking |
| Single IDLE call without 29-min re-issue loop | Simpler loop code | Silent disconnection after ~30 min on most servers | Never — re-IDLE loop is mandatory |
| Skipping CONDSTORE-only fallback (QRESYNC only) | One sync path to implement | Gmail never syncs incrementally | Never — Gmail is the largest email provider |
| Embedding error strings as raw Rust strings | Fast to write | Must match `LocalizedErrorStrings` keys in TypeScript; mismatch causes fallback to raw error | Never — use constants from a shared protocol spec |
| Using native-tls for IMAP/CalDAV TLS | Familiar API | Cross-compilation fails on Linux; CI build fails without OpenSSL headers | Never — same reason as v1.0 |
| Sending the full model JSON in every delta | No change detection needed | SQLite "fat row" approach doubles write amplification | Acceptable — matches C++ engine's approach, keep for v2.0 |
| Skipping ProcessState messages | Fewer message types to implement | Connection status indicator stuck at "offline" in UI | Never — OnlineStatusStore depends on these |

---

## Integration Gotchas

| Integration | Common Mistake | Correct Approach |
|-------------|----------------|------------------|
| Electron → Rust binary stdin | Writing all startup data at once without checking if binary is ready | The existing TS code waits for the first `stdout.once('data')` event before piping account JSON + identity JSON; the Rust binary must emit at least one byte to signal readiness |
| Rust → Electron stdout delta | Using `println!` or `serde_json::to_writer` without flush | Lock stdout, write JSON, write `\n`, call `flush()` as an atomic group per message |
| SQLite WAL mode with multiple readers | Readers block on writer's lock even in WAL mode | Enable WAL mode and set `busy_timeout = 5000`; readers should use a separate read connection from the writer |
| IMAP IDLE + background sync | Running IDLE and folder iteration on the same session concurrently | IDLE occupies the IMAP session exclusively; use two connections per account: one for IDLE on INBOX, one for background folder sync |
| CalDAV REPORT request | Sending REPORT without `Depth: 1` header | CalDAV REPORT must include `Depth: 1` to list calendar resources; missing header causes empty response from many servers |
| CardDAV vCard parsing | Assuming all vCards are valid UTF-8 | vCards can contain arbitrary byte sequences; use a forgiving parser that skips invalid UTF-8 characters rather than returning an error |
| Binary path in dev vs. production | Hardcoding the Cargo `target/release/` path in TypeScript | Use the existing fallback mechanism in `mailsync-process.ts`; update the dev fallback path to match Cargo output |
| Task `queue-task` stdin message | Processing tasks before the initial account/identity handshake | The Rust engine must fully parse account JSON and identity JSON from stdin before processing any `queue-task` messages |

---

## Performance Traps

| Trap | Symptoms | Prevention | When It Breaks |
|------|----------|------------|----------------|
| Full folder sync on every reconnect (no CONDSTORE) | Slow reconnection, excessive IMAP bandwidth | Store MODSEQ and UIDVALIDITY per folder in SQLite; use CONDSTORE SELECT on reconnect | First reconnect after 10k+ message folder |
| Emitting individual deltas per message during initial sync | Electron UI update loop runs for every message — UI freezes | Batch deltas: emit `modelJSONs` arrays of up to 100 models per message during initial bulk sync | First sync of Gmail inbox with 50k+ messages |
| SQLite without WAL causing reader/writer contention | Sync pauses while queries are running | Enable WAL mode at DB open; use separate read/write connections | Measurable as soon as background sync and UI queries run simultaneously |
| Spawning a new tokio runtime per IMAP folder | Memory and CPU overhead grow with folder count | Single shared tokio runtime; one task per folder connection | At 20+ folders (labels) in Gmail accounts |
| Regex compilation in iCalendar parsing hot path | CPU spikes during CalDAV sync | Compile iCalendar parsing regexes once at startup using `once_cell::Lazy<Regex>` | Any repeated calendar sync |

---

## Security Mistakes

| Mistake | Risk | Prevention |
|---------|------|------------|
| Logging OAuth2 refresh tokens from the identity JSON | Tokens appear in `mailsync-*.log` files readable by other processes | Strip token values from all log output; log presence only: `"oauth2_refresh_token: present"` |
| Storing account passwords in a Rust static | Passwords persist in process memory beyond the connection lifetime | Pass passwords per-connection; zero the memory after use with `zeroize` crate |
| Trusting IMAP server banners without TLS | MITM can inject malicious server metadata | Always require TLS (or STARTTLS) before trusting any server capability or auth challenge |
| Emitting account credentials in delta JSON | Credentials appear in Electron IPC and may be logged | The `ProcessAccountSecretsUpdated` model class is used specifically for this — emit it with the KeyManager pattern, never inline credentials in normal deltas |
| Accepting self-signed certificates | MITM attacks | Use `rustls-platform-verifier` which applies OS trust decisions; never call `dangerous().disable_certificate_verification()` |

---

## "Looks Done But Isn't" Checklist

- [ ] **stdout flushing:** Run the binary with Electron, check that deltas appear within 1 second of a sync event — not only when the binary exits.
- [ ] **stdin deadlock:** Queue a task with a 500 KB HTML body and verify the engine processes it without hanging.
- [ ] **IMAP IDLE:** Leave the app running for 30 minutes with WiFi connected, verify new mail still appears (no silent IDLE disconnect).
- [ ] **CONDSTORE fallback:** Test sync against a Gmail account — verify incremental sync works (CONDSTORE without QRESYNC).
- [ ] **QRESYNC:** Test against a Fastmail/Dovecot account — verify VANISHED responses are handled correctly on reconnect.
- [ ] **STARTTLS SNI:** Test IMAP on port 143 with a Fastmail account — verify TLS handshake succeeds with correct SNI hostname.
- [ ] **OAuth2 token refresh:** Let a session run for 65 minutes with a Gmail OAuth2 account — verify sync continues without an auth failure.
- [ ] **Delta format:** Verify `modelJSONs` (not `modelJsons`) in emitted JSON; verify `modelClass` (not `objectClass`) key name.
- [ ] **ProcessState messages:** Verify the Electron UI connection status indicator changes state when the binary connects/disconnects.
- [ ] **Mode handling:** Verify the binary exits 0 in "test" mode with the correct JSON result structure.
- [ ] **SIGTERM handling:** Kill the binary with SIGTERM (Linux/macOS) and verify it exits 0 without SIGABRT — confirming no pending tokio tasks crash.
- [ ] **asar unpacking:** Package the app with electron-builder and verify the binary is in `app.asar.unpacked/`, not inside `app.asar`.
- [ ] **CalDAV ETag loop:** Verify a CalDAV sync against Google Calendar does not re-download the same events on every sync cycle.
- [ ] **Binary size:** Stripped release binary is under 15 MB on each platform (the C++ engine was ~8-12 MB).

---

## Recovery Strategies

| Pitfall | Recovery Cost | Recovery Steps |
|---------|---------------|----------------|
| stdout not flushing | LOW | Add explicit `handle.flush()` after every write; trivial fix but requires testing to confirm |
| stdin deadlock under large payloads | MEDIUM | Refactor stdin/stdout into dedicated tokio tasks with channel communication; structural change |
| SQLite blocking tokio threads | HIGH | Introduce `tokio-rusqlite` across all database call sites; requires touching every query |
| Delta format field name mismatch | LOW | Update `#[serde(rename)]` attributes; add contract test; rebuild |
| IDLE disconnect after 30 min | LOW | Wrap IDLE wait in `tokio::time::timeout(25 * 60s)`; add DONE before retry |
| CONDSTORE/QRESYNC sequence error | MEDIUM | Restructure connection setup to issue ENABLE before SELECT; audit all SELECT call sites |
| Missing protocol mode handling | MEDIUM | Add mode dispatch at binary entry point; add integration test per mode |
| OAuth2 token expiry | MEDIUM | Add expiry-aware token cache with background refresh; requires understanding C++ refresh flow |
| CalDAV ETag sync loop | MEDIUM | Add GET-after-PUT when server omits ETag; add sync-token fallback |
| Binary not found in production | LOW | Update `asarUnpack` config; update dev fallback path in `mailsync-process.ts` |

---

## Pitfall-to-Phase Mapping

| Pitfall | Prevention Phase | Verification |
|---------|------------------|--------------|
| stdout buffering / no flush | Phase 1: IPC scaffolding | Delta arrives in Electron within 1 second; final JSON present after binary exit |
| stdin deadlock under large payloads | Phase 1: IPC scaffolding | 500 KB payload processed without hang |
| Delta JSON field name mismatch | Phase 1: IPC scaffolding | Contract test validates all field names against TypeScript parser |
| Protocol mode handling (test/sync/migrate) | Phase 1: IPC scaffolding | Each mode integration test passes independently |
| SQLite blocking async thread pool | Phase 2: SQLite layer | `tokio-rusqlite` used everywhere; no direct rusqlite calls in async fn |
| WAL mode + busy_timeout | Phase 2: SQLite layer | Concurrent reader+writer test shows no `SQLITE_BUSY` errors |
| TLS BoringSSL / OpenSSL dependency | Phase 1: Scaffolding | `cargo tree \| grep openssl` returns nothing in CI |
| CONDSTORE-only fallback for Gmail | Phase 3: IMAP sync | Gmail test account syncs incrementally without QRESYNC |
| QRESYNC ENABLE-before-SELECT ordering | Phase 3: IMAP sync | Fastmail/Dovecot test shows QRESYNC SELECT succeeds |
| IMAP IDLE 29-min reconnect | Phase 3: IMAP IDLE | 30-minute passive test shows continuous mail receipt |
| STARTTLS SNI | Phase 3: IMAP implementation | Port 143 STARTTLS succeeds against Fastmail |
| OAuth2 token refresh during session | Phase 3: IMAP auth | 65-minute Gmail OAuth2 session stays connected |
| CalDAV ETag sync loop | Phase 5: CalDAV sync | Same Google Calendar event not re-downloaded on second sync |
| Binary path in dev vs. production | Phase 1: Scaffolding (plan) + Phase 6: Packaging | Packaged build opens and syncs on Windows and macOS |

---

## Sources

- [UnifyMail `mailsync-process.ts`](app/frontend/mailsync-process.ts) — stdin/stdout IPC protocol, mode handling, binary path resolution (HIGH confidence, live codebase)
- [UnifyMail `mailsync-bridge.ts`](app/frontend/flux/mailsync-bridge.ts) — delta message format, task queuing, ProcessState handling (HIGH confidence, live codebase)
- [Foundry376/Mailspring-Sync README](https://github.com/Foundry376/Mailspring-Sync) — newline-delimited JSON IPC protocol; stateless design; task queue in SQLite (MEDIUM confidence)
- [RFC 7162: IMAP CONDSTORE and QRESYNC](https://www.rfc-editor.org/rfc/rfc7162.html) — ENABLE-before-SELECT requirement; VANISHED response; Gmail CONDSTORE-only (HIGH confidence)
- [RFC 2177: IMAP IDLE](https://datatracker.ietf.org/doc/html/rfc2177) — 29-minute re-issue requirement (HIGH confidence)
- [deltachat-core-rust issue #5093: IDLE CLOSE-WAIT after 30 min](https://github.com/deltachat/deltachat-core-rust/issues/5093) — real-world IDLE disconnect confirmed in Rust email client (HIGH confidence)
- [deltachat-core-rust issue #2208: IMAP IDLE connection handling approach](https://github.com/deltachat/deltachat-core-rust/issues/2208) — short-loop timeout strategy (MEDIUM confidence)
- [rusqlite issue #697: Transactions don't work with async/await](https://github.com/rusqlite/rusqlite/issues/697) — rusqlite is not Send; transaction lifetime issue (HIGH confidence)
- [tokio-rusqlite docs](https://docs.rs/tokio-rusqlite/latest/tokio_rusqlite/) — dedicated background thread pattern; call() API (HIGH confidence)
- [tokio issue #7174: stdout data loss without explicit flush](https://github.com/tokio-rs/tokio/issues/7174) — async stdout not flushed at process exit (HIGH confidence)
- [rust-lang/rust issue #45572: piped stdout buffer deadlock](https://github.com/rust-lang/rust/issues/45572) — pipe buffer fills causing deadlock (HIGH confidence)
- [rust-lang/rust issue #60673: stdout buffering when not connected to TTY](https://github.com/rust-lang/rust/issues/60673) — full block buffering for piped stdout (HIGH confidence)
- [SQLite concurrent writes and "database is locked" errors](https://tenthousandmeters.com/blog/sqlite-concurrent-writes-and-database-is-locked-errors/) — WAL mode, busy_timeout, checkpoint starvation (HIGH confidence)
- [matrix-rust-sdk issue #5362: SQLite locked errors](https://github.com/matrix-org/matrix-rust-sdk/issues/5362) — real-world SQLITE_BUSY in Rust async context (MEDIUM confidence)
- [RFC 4791 §5.3.4: CalDAV ETag behavior after PUT](https://www.rfc-editor.org/rfc/rfc4791) — server may not return ETag if it mutates the resource (HIGH confidence)
- [sabre/dav: Building a CalDAV client](https://sabre.io/dav/building-a-caldav-client/) — ETag-after-PUT pattern, sync-token fallback (MEDIUM confidence)
- [Electron docs: Using Native Node Modules](https://www.electronjs.org/docs/latest/tutorial/using-native-node-modules) — asarUnpack requirement for native executables (HIGH confidence)
- [electron-builder issue #1285: Native module not being unpacked](https://github.com/electron-userland/electron-builder/issues/1285) — asarUnpack configuration (MEDIUM confidence)
- [How to rewrite a C++ codebase successfully](https://gaultier.github.io/blog/how_to_rewrite_a_cpp_codebase_successfully.html) — regression testing during rewrite; behavioral contract verification (MEDIUM confidence)

---
*Pitfalls research for: Rust mailsync engine rewrite replacing C++ sync binary in Electron email client*
*Researched: 2026-03-02*
