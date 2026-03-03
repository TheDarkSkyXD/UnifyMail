# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-01)

**Core value:** Users can set up email accounts quickly and reliably through automatic provider detection and connection validation
**Current focus:** Phase 1 — Scaffolding and Provider Detection

## Current Position

Phase: 1 of 4 (Scaffolding and Provider Detection)
Plan: 0 of ? in current phase
Status: Ready to plan
Last activity: 2026-03-03 — Phase 1 context gathered (11 areas discussed: switchover, build, API, testing, errors, structure, MX, logging, deps, docs, code style)

Progress: [░░░░░░░░░░] 0%

## Performance Metrics

**Velocity:**
- Total plans completed: 0
- Average duration: —
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**
- Last 5 plans: —
- Trend: —

*Updated after each plan completion*

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Pre-Phase 1]: Use napi-rs v3 over node-addon-api — cross-platform builds without node-gyp, auto-generated TS types
- [Pre-Phase 1]: Use rustls exclusively (tokio-rustls + rustls-platform-verifier) — native-tls on Linux introduces OpenSSL symbols conflicting with Electron's BoringSSL; hard constraint with no alternative
- [Pre-Phase 1]: Embed providers.json via include_str!() at compile time — eliminates runtime path resolution across dev/production/packaged Electron environments
- [Pre-Phase 1]: Use async-imap 0.11.2 with runtime-tokio feature — only maintained async IMAP crate; must use tokio feature or async-std conflict occurs

### Pending Todos

None yet.

### Blockers/Concerns

- [Phase 2 risk]: IMAP STARTTLS stream upgrade (TcpStream to TlsStream inside async-imap) is not abstracted by the library — consult deltachat-core-rust patterns before implementing
- [Phase 4 risk]: electron-builder asarUnpack interaction with napi-rs single-package binary distribution needs hands-on verification; napi-rs/node-rs issue #376 documents the problem

## Session Continuity

Last session: 2026-03-03
Stopped at: Phase 1 context gathered — ready to plan
Resume file: .planning/phases/01-scaffolding-and-provider-detection/01-CONTEXT.md

---
*Last updated: 2026-03-03*
