<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# flux

## Purpose
Flux architecture implementation — the state management backbone of the application. Contains the action dispatcher, data stores (singletons managing application state), data models (ORM-like classes mapping to database tables), background tasks (operations that sync with the server), database query system (attributes, matchers, queries), and the Mailsync bridge.

## Key Files

| File | Description |
|------|-------------|
| `actions.ts` | All application actions: defines Flux action creators dispatched by UI and services |
| `action-bridge.ts` | IPC bridge that synchronizes Flux actions between main and renderer processes |
| `mailsync-bridge.ts` | Bridge to the native mailsync C++ process: sends commands, receives events, handles sync status |
| `UnifyMail-api-request.ts` | HTTP request utility for UnifyMail web API calls (identity, billing) |
| `attributes.ts` | Barrel export for all attribute types |
| `errors.ts` | Custom error classes for Flux operations |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `models/` | Data model classes (ORM): Account, Message, Thread, Contact, Event, Category, etc. (see `models/AGENTS.md`) |
| `stores/` | Flux stores (singletons): AccountStore, DatabaseStore, DraftStore, MessageStore, etc. (see `stores/AGENTS.md`) |
| `tasks/` | Background tasks: send email, change folder, destroy draft, syncback operations (see `tasks/AGENTS.md`) |
| `attributes/` | Database query attribute types: Boolean, String, Number, DateTime, Collection, JoinedData (see `attributes/AGENTS.md`) |

## For AI Agents

### Working In This Directory
- **`actions.ts` is the action registry** — add new action creators here when adding new features
- **`mailsync-bridge.ts` is critical infrastructure** — it bridges JavaScript and the C++ sync engine
- Actions flow: UI → Actions dispatch → Stores respond → Components re-render
- The `DatabaseStore` is the local SQLite abstraction — all data reads/writes go through it
- Tasks represent operations that need to sync with the mail server (via mailsync)

### Testing Requirements
- Store tests: `app/spec/stores/`
- Model tests: `app/spec/models/`
- Task tests: `app/spec/tasks/`
- Test stores by dispatching actions and asserting state changes
- Test models by verifying serialization, attributes, and query building

### Common Patterns
- **Actions**: `Actions.actionName(payload)` dispatches to all listening stores
- **Stores**: Extend `UnifyMailStore`, listen to actions, trigger change events
- **Models**: Extend `Model` or `ModelWithMetadata`, define `attributes` static getter for ORM mapping
- **Tasks**: Extend `Task`, implement `performLocal()` and server syncback logic
- **Database queries**: `DatabaseStore.findAll(Model).where([Matcher]).order(Attribute)`

## Dependencies

### Internal
- `app/src/global/UnifyMail-exports` — Exports models, stores, actions publicly
- `app/src/registries/` — Component and extension registries
- `app/src/services/` — Services that stores depend on

### External
- `rx-lite` — Observable streams for reactive database queries
- `better-sqlite3` — SQLite database engine
- `underscore` — Collection utilities

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
