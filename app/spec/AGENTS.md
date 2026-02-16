<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# spec

## Purpose
Jasmine 2.x test specifications for the UnifyMail application. Tests run inside the Electron process to have access to the full application environment (DOM, IPC, native modules). Organized to mirror the `src/` directory structure.

## Key Files

| File | Description |
|------|-------------|
| `UnifyMail-test-utils.ts` | Shared test utilities: mock factories, test helpers, spy setup |
| `action-bridge-spec.ts` | Tests for the IPC action bridge between main/renderer processes |
| `autoupdate-manager-spec.ts` | Tests for the auto-update system |
| `database-object-registry-spec.ts` | Tests for the database object type registry |
| `linux-dnd-utils-spec.ts` | Tests for Linux drag-and-drop utilities |
| `list-selection-spec.ts` | Tests for list selection logic (multi-select, range-select) |
| `mail-rules-processor-spec.ts` | Tests for the mail rules processing engine |
| `mailbox-perspective-spec.ts` | Tests for mailbox perspective query building |
| `UnifyMail-protocol-handler-spec.ts` | Tests for `UnifyMail://` protocol handler |
| `menu-manager-spec.ts` | Tests for application menu construction |
| `spellchecker-spec.ts` | Tests for spellchecker integration |
| `async-test-spec.ts` | Tests for async/promise test utilities |
| `tsconfig.json` | TypeScript configuration for spec files |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `components/` | Tests for React UI components |
| `models/` | Tests for Flux data models (Message, Thread, Contact, etc.) |
| `stores/` | Tests for Flux stores (AccountStore, DraftStore, etc.) |
| `tasks/` | Tests for background tasks (send draft, change folder, etc.) |
| `services/` | Tests for application services |
| `registries/` | Tests for registries |
| `utils/` | Tests for utility modules |
| `fixtures/` | Test fixture data: sample emails, JSON responses, etc. |
| `spec-runner/` | Test runner bootstrapping and configuration |

## For AI Agents

### Working In This Directory
- Run all tests: `npm test` from the root directory
- Run specific window tests: `npm run test-window`
- Tests execute inside Electron — they have access to the full app environment
- Use `UnifyMail-test-utils.ts` for creating mock objects and test helpers
- Name test files with `-spec.ts` suffix matching the source file they test

### Testing Requirements
- Every new source file should have a corresponding spec file
- Follow the AAA pattern: Arrange, Act, Assert
- Mock external dependencies (database, IPC, network) using Jasmine spies
- Use `beforeEach` for test setup, `afterEach` for cleanup

### Common Patterns
- Import from `UnifyMail-exports` for models, stores, actions
- Use `waitsForPromise` for async test assertions
- Fixtures in `fixtures/` provide sample data for complex test scenarios
- Component tests use React test renderer or direct DOM assertions

## Dependencies

### Internal
- `app/src/` — Source code under test
- `app/src/global/UnifyMail-exports` — Test imports

### External
- Jasmine 2.x — Test framework (assertions, spies, suites)
- Enzyme — React component testing (if used)

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
