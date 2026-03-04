# Phase 1: Scaffolding and Provider Detection - Context

**Gathered:** 2026-03-03
**Status:** Ready for planning

<domain>
## Phase Boundary

Prove Electron integration is sound by scaffolding a Rust napi-rs addon at `app/mailcore-rs/` and implementing the two provider functions (`registerProviders`, `providerForEmail`) with full regex cross-validation against the C++ addon. The Rust addon becomes the primary addon at end of this phase, with C++ retained as a fallback for the 3 unimplemented network functions (testIMAPConnection, testSMTPConnection, validateAccount) until Phases 2-3 implement them in Rust.

</domain>

<decisions>
## Implementation Decisions

### Switchover timing
- Switch the Electron app to the Rust addon at end of Phase 1, once cross-validation passes
- Update `app/package.json` dependency from `"mailcore-napi": "file:mailcore"` to `"file:mailcore-rs"` after validation
- C++ addon stays in the repo and remains loadable as a fallback for the 3 network functions not yet implemented in Rust
- The fallback routing mechanism (how the app calls Rust for provider functions but C++ for IMAP/SMTP/validate) is Claude's discretion

### Build workflow
- Integrate Rust addon build into `npm start` so developers don't need to manually run `napi build`
- Use debug builds for development (`npm start`), release builds only for production (`npm run build`)
- Always rebuild on `npm start` — Cargo incremental compilation handles no-op builds efficiently (~1-2s)
- Keep the npm package name as `mailcore-napi` — consumers continue to `require('mailcore-napi')` with zero code changes

### API contract
- Runtime values must match C++ exactly — same field names (`hostname`, `port`, `connectionType`), same values (`'tls'`, `'starttls'`, `'clear'`)
- TypeScript types may be stricter than C++ (e.g., `connectionType: 'tls' | 'starttls' | 'clear'` instead of `string`) — same runtime, better types
- `providerForEmail` returns `null` for all non-match cases: unknown domains, malformed emails, missing '@', empty strings — matches C++ behavior
- `registerProviders(jsonPath)` usage check and implementation depth is Claude's discretion

### Testing & validation
- Deliver two levels of testing: cross-validation script + Rust unit tests via `cargo test`
- Cross-validation is a standalone script (`npm run cross-validate` or similar), not part of `npm test` — requires both addons built
- Cross-validation performs full object comparison (identifier AND server configs: hostname, port, connectionType), not just identifier matching
- Rust unit tests use standard `#[cfg(test)]` with `#[test]` functions — edge cases like malformed emails, empty providers, regex anchoring

### Claude's Discretion
- Testing approach during Phase 1 development (before final switchover)
- Fallback routing mechanism for C++ network functions (wrapper module, re-export, or other approach)
- `registerProviders` implementation depth based on actual usage in the codebase
- Exact integration point for the Rust build step in the npm scripts

</decisions>

<specifics>
## Specific Ideas

No specific requirements — open to standard approaches. The research document (01-RESEARCH.md) has thorough coverage of the C++ algorithm, providers.json schema, and napi-rs patterns.

</specifics>

<code_context>
## Existing Code Insights

### Reusable Assets
- `app/mailcore/resources/providers.json`: The 37-provider database to copy into `app/mailcore-rs/resources/` — source of truth for domain matching
- `app/mailcore/types/index.d.ts`: TypeScript interface the Rust addon's generated types must match
- `app/mailcore/src/napi/napi_provider.cpp`: C++ implementation to replicate — contains exact function signatures and return shapes

### Established Patterns
- Native addons loaded via `require('mailcore-napi')` in consumer code — the Rust addon uses the same package name
- `app/package.json` uses `"file:mailcore"` for local native addon resolution — will change to `"file:mailcore-rs"`
- Build tooling uses Grunt (`build/Gruntfile.js`) — the Rust build step needs to integrate or run alongside

### Integration Points
- `app/internal_packages/onboarding/lib/onboarding-helpers.ts`: Primary consumer of `providerForEmail` and the 3 network functions
- `app/frontend/mailsync-process.ts`: Consumer of `registerProviders` (to be verified)
- `app/package.json`: Dependency resolution for `mailcore-napi` — the switchover point

</code_context>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 01-scaffolding-and-provider-detection*
*Context gathered: 2026-03-03*
