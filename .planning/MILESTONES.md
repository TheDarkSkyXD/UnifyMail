# Milestones

## v1.0 — Rewrite mailcore N-API in Rust (Shipped: 2026-03-04)

**Started:** 2026-03-01
**Shipped:** 2026-03-04
**Phases:** 6 (1, 2, 3, 4, 4.1, 4.2) | **Plans:** 10 | **Tasks:** 20
**Requirements:** 27/27 satisfied
**Rust source:** 1,558 LOC | **Tests:** 1,699 LOC
**Files changed:** 1,749 | **Lines:** +36,782 / -387,153
**Git range:** feat(01-01) → feat(04.2-01)

### Delivered

Replaced the `app/mailcore/` C++ N-API addon (backed by the full mailcore2 library, ~1,500 files) with a minimal Rust napi-rs implementation exposing the same 5-function API. All C++ code, node-gyp configs, and vendored dependencies deleted.

### Key Accomplishments

1. Rust napi-rs addon with embedded 37-provider database, domain-regex matching, and 16 integration tests
2. IMAP connection testing with 3 TLS paths, XOAUTH2 SASL auth, 7-capability detection, 15s timeout
3. SMTP connection testing with lettre transport, LOGIN/XOAUTH2 auth, 9-test mock suite
4. Account validation with concurrent IMAP+SMTP+MX via `tokio::join!()`, per-protocol credentials
5. C++ elimination — ~1,500 mailcore2 files deleted, node-gyp removed, direct npm symlink routing
6. CI hardening — all 4 workflows build Rust for 5 targets with shared smoke test covering all 5 exports

### Tech Debt Carried Forward

- `cargo test` cannot run on Windows GNU target (aws-lc-sys MinGW nanosleep64 link error)
- Binary size gate only enforced on Linux x64 CI
- Pre-built `.node` binary in repo predates Phase 2/3

### Archives

- `.planning/milestones/v1.0-ROADMAP.md`
- `.planning/milestones/v1.0-REQUIREMENTS.md`
- `.planning/milestones/v1.0-MILESTONE-AUDIT.md`

---

## Planned

**v2.0 — Rewrite mailsync Engine in Rust**
- Goal: Replace C++ mailsync engine (~16,200 LOC) with Rust implementation
- Phases: 5-10 (6 phases)
- Depends on: v1.0 completion
- Status: Roadmap defined, ready to plan

---
*Last updated: 2026-03-04 after v1.0 milestone completion*
