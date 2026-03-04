# Project Retrospective

*A living document updated after each milestone. Lessons feed forward into future planning.*

## Milestone: v1.0 — Rewrite mailcore N-API in Rust

**Shipped:** 2026-03-04
**Phases:** 6 | **Plans:** 10 | **Tasks:** 20

### What Was Built
- Rust napi-rs addon replacing C++ mailcore2 with identical 5-function API (providerForEmail, registerProviders, testIMAPConnection, testSMTPConnection, validateAccount)
- 1,558 LOC Rust source + 1,699 LOC Rust tests
- CI for 5 platform targets with shared smoke test script
- ~1,500 C++ files deleted, node-gyp removed entirely

### What Worked
- Strict dependency ordering (scaffolding -> IMAP -> SMTP -> packaging) prevented rework — each phase built on proven patterns from the previous one
- Mock server test suites (12 IMAP + 9 SMTP) caught real bugs before integration
- The greeting consumption bug fix (02-02) was caught by mock tests, not production
- Audit-driven gap closure (Phases 4.1, 4.2) ensured no integration issues slipped through
- 73 minutes total execution time across 10 plans — fast iteration

### What Was Inefficient
- Early phases (1-4) didn't populate `requirements_completed` in SUMMARY frontmatter — convention established too late
- Nyquist validation was partial for Phases 1 and 4 — wave 0 not fully executed
- Phase 4.1 ROADMAP checkbox wasn't updated to [x] after completion — manual tracking drift
- Pre-built .node binary in repo drifted out of date across phases

### Patterns Established
- `InternalResult<T>` for internal Rust functions, `napi::Error` conversion only at export boundary
- Custom `loader.js` for GNU .node loading in MSVC Node.js via N-API stable ABI
- `TEST_MUTEX` pattern for serializing integration tests sharing LazyLock<RwLock<...>> singletons
- Per-protocol credential split pattern (imapUsername/smtpUsername vs shared username)
- Shared `smoke.js` test script referenced by all CI workflows

### Key Lessons
1. **Read IMAP greeting explicitly** — async-imap requires greeting consumption after Client::new; failure causes XOAUTH2 to misroute greeting as SASL challenge (deadlock)
2. **rustls-platform-verifier only** — native-tls on Linux introduces OpenSSL symbols conflicting with Electron's BoringSSL; this is a hard constraint
3. **async-imap default-features = false** — default feature is runtime-async-std which conflicts with runtime-tokio; both enabled causes compile_error!()
4. **napi-rs Option<String> accepts undefined, not null** — JS callers must pass `undefined` for optional string fields
5. **Audit before shipping** — the milestone audit caught 6 integration gaps that would have shipped broken (username field mapping, CI smoke coverage, cache key)

### Cost Observations
- Model mix: ~70% opus, ~20% sonnet, ~10% haiku
- Sessions: ~5
- Notable: 73 minutes execution time for 1,558 LOC + 1,699 LOC tests — high throughput from strict phase ordering

---

## Cross-Milestone Trends

### Process Evolution

| Milestone | Sessions | Phases | Key Change |
|-----------|----------|--------|------------|
| v1.0 | ~5 | 6 | Established audit-driven gap closure pattern |

### Cumulative Quality

| Milestone | Tests | Coverage | Zero-Dep Additions |
|-----------|-------|----------|-------------------|
| v1.0 | 44+ (16 provider + 12 IMAP + 9 SMTP + 7 integration) | All 27 requirements | 0 |

### Top Lessons (Verified Across Milestones)

1. Audit milestones before shipping — gap closure phases are cheaper than post-ship fixes
2. Strict phase dependency ordering prevents rework and enables pattern reuse
