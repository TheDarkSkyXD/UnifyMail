# Phase 5: Core Infrastructure and IPC Protocol - Context

**Gathered:** 2026-03-03
**Updated:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Rust standalone binary skeleton (`app/mailsync-rs/`) with correct stdin/stdout IPC protocol matching the C++ mailsync wire format, all 5 process modes (sync, test, migrate, reset, install-check), SQLite schema creation (full schema upfront), delta emission pipeline via tokio mpsc channel with dedicated flush task, and stdin EOF orphan detection. The Rust binary coexists alongside the C++ binary with a different name (`mailsync-rs`) during Phases 5-9; Phase 10 renames it to replace C++.

</domain>

<decisions>
## Implementation Decisions

### Binary coexistence strategy
- Rust binary uses a distinct name during development: `mailsync-rs.exe` (Windows) / `mailsync-rs.bin` (macOS/Linux)
- Source code lives at `app/mailsync-rs/` — mirrors the v1.0 pattern (`app/mailcore-rs/`)
- C++ `mailsync` binary stays fully functional and unchanged through Phases 5-9
- Phase 10 renames Rust binary to `mailsync` and deletes C++
- `mailsync-process.ts` is modified to check for `mailsync-rs` first, falling back to C++ `mailsync` — simple conditional in `_spawnProcess`
- Rust binary build is integrated into `npm start` (same pattern as mailcore-rs — Cargo incremental compilation handles no-op builds in ~1-2s)
- Debug builds for development (`npm start`), release builds only for production

### Stub behavior for sync mode
- `--mode sync`: Completes stdin handshake, emits a `ProcessState` delta (showing account as 'online'), then loops reading stdin commands. Unimplemented commands (queue-task, need-bodies, etc.) are accepted and logged but produce no action — the account shows as 'connected' in the Electron UI
- `--mode migrate`: Creates the full SQLite schema in one go — all tables, indexes, and FTS5 virtual tables matching the C++ baseline. Phase 6 focuses on model code, not schema changes
- `--mode reset`: Fully functional — drops and recreates the database
- `--mode install-check`: Fully functional — verifies binary runs and exits with code 0
- `--mode test`: Claude's discretion — the N-API `validateAccount` path in mailsync-process.ts handles account validation already (v1.0), so test mode is vestigial

### Logging and debug output
- Use the **tracing** crate for structured logging with spans and levels — industry standard for async Rust, integrates with tokio
- Write logs to `{configDirPath}/mailsync-{accountId}.log` files — same file pattern as C++. The MailsyncBridge `tailClientLog` method reads these files for error reporting, so same path = zero TypeScript changes
- **Verbose control:** Both `--verbose` flag (sets tracing to DEBUG level) and `RUST_LOG` env var (overrides if present). The existing UI toggle button passes `--verbose`, developers can use `RUST_LOG` for fine-grained control
- **Debug output goes to stderr only** — stdout is exclusively for IPC protocol messages (JSON deltas). Clean separation. mailsync-process.ts already captures stderr

### Error key compatibility
- Use **identical error key strings** as C++ binary: `ErrorAuthentication`, `ErrorConnection`, `ErrorTLSNotAvailable`, `ErrorCertificate`, `ErrorParse`, `ErrorGmailIMAPNotEnabled`, etc. — `LocalizedErrorStrings` in mailsync-process.ts maps these to user-facing messages without changes
- **Exact same JSON error shape** on failure: `{ error: "ErrorKey", error_service: "imap"|"smtp", log: "..." }` — mailsync-process.ts `_spawnAndWait` parses this shape at lines 290-293
- **Full error enum defined upfront** covering all ~20 C++ error keys as Rust enum variants. Each variant maps to the C++ error string. Later phases use existing variants rather than adding new ones
- **Same exit codes** as C++: 0 for success, non-zero with JSON error on stdout for failure, 141 for stdin EOF (orphan detection)

### Cargo workspace setup
- Create a Cargo workspace at `app/Cargo.toml` with members `mailcore-rs` and `mailsync-rs`
- Shared workspace dependency versions for tokio, serde, rustls, aws-lc-rs — consistent across both crates
- No shared library crate — mailcore-rs (N-API addon) and mailsync-rs (standalone binary) are independent members with no code coupling
- Dependency pinning strategy: Claude's discretion based on what worked in v1.0

### Windows build pipeline
- Use GNU target (`x86_64-pc-windows-gnu`) — same as mailcore-rs, proven toolchain with MSYS2 MinGW
- Build step added to `scripts/start-dev.js` before Electron launch — `cargo build` with incremental compilation (~1-2s no-op)
- Binary output at standard `app/mailsync-rs/target/debug/` (dev) and `target/release/` (prod) — mailsync-process.ts resolves path accordingly
- Binary name includes platform suffix: `mailsync-rs.exe` (Windows) / `mailsync-rs.bin` (macOS/Linux) — matches existing C++ naming convention

### Testing strategy
- **IPC contract tests**: Rust integration tests that spawn the binary as a child process, pipe stdin/stdout, verify handshake and delta format — runs in `cargo test`
- **Schema validation**: Diff-based validation against C++ constants.h schema, then snapshot locked for regression
- **Mode coverage**: All 5 modes have at least one integration test verifying correct behavior and exit code
- **Electron E2E**: Claude's discretion on whether to include minimal Electron integration test in Phase 5 or defer to Phase 6

### Stdin command handling
- **Structured dispatch**: Parse all 10+ C++ command types into a Rust enum with handler functions — unimplemented handlers log at debug level and return silently
- **Full command enum upfront**: All command variants defined (queue-task, cancel-task, wake-workers, need-bodies, sync-calendar, detect-provider, query-capabilities, subscribe-folder-status, test-crash, test-segfault) — later phases implement real handlers
- **Unknown commands**: Warn-level log and continue — forward compatible if TypeScript adds new commands before Rust catches up
- **No response for stubs**: Silent accept + debug log. Electron UI doesn't expect responses for most commands

### Claude's Discretion
- `--mode test` implementation depth (stub vs minimal success response)
- Exact Cargo.toml dependency versions for tracing, tokio-rusqlite, clap, serde
- Internal module organization (ipc.rs, schema.rs, delta.rs, etc.)
- Delta coalescing window implementation (500ms from research)
- tokio task architecture details (stdin reader, stdout writer, command dispatcher)
- Whether to use clap for CLI argument parsing or hand-roll the `--mode` flag
- Log rotation strategy (if any) for the mailsync-{accountId}.log files
- Electron E2E test inclusion in Phase 5 vs deferral

</decisions>

<specifics>
## Specific Ideas

- The research document (05-RESEARCH.md) has the complete C++ schema SQL extracted from constants.h, the exact delta emission format from DeltaStream.cpp, and the stdin command handler signatures from main.cpp
- The handshake protocol waits for the first stdout byte before piping account+identity JSON on stdin (mailsync-process.ts line 211-222) — the Rust binary must emit something on stdout first to trigger the handshake
- The `ProcessState` delta format uses `modelClass: "ProcessState"` with `modelJSONs` containing status info — OnlineStatusStore.onSyncProcessStateReceived() handles it in the TypeScript side
- `ProcessAccountSecretsUpdated` is a special delta type for OAuth2 token refresh — Phase 5 doesn't need to emit this but the delta pipeline should support it structurally
- CONFIG_DIR_PATH, GMAIL_CLIENT_ID, GMAIL_OAUTH_PROXY_URL, IDENTITY_SERVER are passed as environment variables to the binary (mailsync-process.ts `_spawnProcess` lines 164-174)
- C++ main.cpp has 10 stdin command types in `runListenOnMainThread` — all should be captured as Rust enum variants for structured dispatch
- mailcore-rs is currently a standalone crate (not a workspace member) — workspace migration needed as part of Phase 5

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailcore-rs/Cargo.toml`: Will become workspace member — shared dependency versions for tokio, serde, rustls
- `app/mailcore-rs/build.rs`: Example of napi-rs build integration — mailsync-rs won't need napi but references the Cargo config patterns
- `05-RESEARCH.md`: Contains complete C++ schema SQL, delta format, IPC protocol details, and tokio architecture patterns
- `scripts/start-dev.js`: Existing build pipeline — already builds mailcore-rs before Electron launch

### Established Patterns
- Binary path resolution in mailsync-process.ts: `path.join(resourcePath, binaryName).replace('app.asar', 'app.asar.unpacked')` — Rust binary follows same pattern
- Environment variable passing: CONFIG_DIR_PATH, GMAIL_CLIENT_ID, etc. passed via `env` in `spawn()` — Rust binary reads these
- Newline-delimited JSON on stdout, parsed by splitting on `\n` — must emit `\n` after every JSON message
- Stdin high water mark set to 1MB (line 206) — Rust stdin reader should handle large payloads
- `--mode` and `--verbose` CLI flags, `--info` for email address (cosmetic)
- v1.0 test pattern: Rust integration tests (`cargo test`) + JS cross-validation + Electron integration test

### Integration Points
- `app/frontend/mailsync-process.ts`: Primary consumer — spawns the binary, pipes stdin/stdout, handles exit codes
- `app/frontend/flux/mailsync-bridge.ts`: Routes deltas to DatabaseStore, handles task queueing, manages per-account process lifecycle
- `app/frontend/flux/stores/online-status-store.ts`: Consumes `ProcessState` deltas
- `app/frontend/key-manager.ts`: Consumes `ProcessAccountSecretsUpdated` deltas
- `app/frontend/flux/stores/database-change-record.ts`: Wraps delta messages as DatabaseChangeRecord objects

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 05-core-infrastructure-and-ipc-protocol*
*Context gathered: 2026-03-03*
