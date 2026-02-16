<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# stores

## Purpose
Flux stores — singleton state containers that listen to Actions, manage application state, and notify UI components of changes. Stores are the central nervous system of the application — they handle accounts, database operations, drafts, messages, folders, preferences, and all other shared state.

## Key Files

### Core Infrastructure
| File | Description |
|------|-------------|
| `database-store.ts` | **DatabaseStore**: SQLite database abstraction — CRUD operations, query execution, change notifications, migrations |
| `database-change-record.ts` | Change record emitted when database objects are created, updated, or deleted |
| `database-agent.js` | Database worker agent for background query execution |
| `task-queue.ts` | **TaskQueue**: manages background task execution, ordering, and retry logic |

### Account & Identity
| File | Description |
|------|-------------|
| `account-store.ts` | **AccountStore**: manages email accounts — add, remove, reorder, current account selection |
| `identity-store.ts` | **IdentityStore**: manages UnifyMail identity (login, subscription, feature flags) |
| `feature-usage-store.tsx` | Tracks pro feature usage limits and displays upgrade prompts |

### Email Viewing
| File | Description |
|------|-------------|
| `message-store.ts` | **MessageStore**: manages messages for the currently focused thread |
| `message-store-extension.ts` | Extension point for message store customization |
| `message-body-processor.ts` | Processes message HTML body through transformation pipeline (sanitize, quote, autolink) |

### Composition
| File | Description |
|------|-------------|
| `draft-store.ts` | **DraftStore**: manages email draft lifecycle — create, open, send, destroy |
| `draft-editing-session.ts` | **DraftEditingSession**: manages a single draft's editing state with debounced persistence |
| `draft-change-set.ts` | Tracks uncommitted changes to a draft for batched saving |
| `draft-factory.ts` | Creates new drafts for replies, forwards, new messages with correct defaults |
| `send-actions-store.ts` | Manages available send actions (send, send-later, send-and-archive) |
| `signature-store.ts` | Manages email signatures — CRUD, per-account defaults |

### Navigation & Focus
| File | Description |
|------|-------------|
| `workspace-store.ts` | **WorkspaceStore**: manages UI layout — sheets, columns, view modes, navigation stack |
| `focused-content-store.ts` | Tracks currently focused/selected items (thread, message) across views |
| `focused-perspective-store.ts` | Tracks the active mailbox perspective (inbox, sent, label, etc.) |
| `focused-contacts-store.ts` | Tracks contacts associated with the currently focused thread |

### Categories & Sync
| File | Description |
|------|-------------|
| `category-store.ts` | **CategoryStore**: manages folders and labels — caches categories by account |
| `folder-sync-progress-store.ts` | Tracks sync progress per folder (percentage, messages synced) |
| `outbox-store.ts` | Manages the outbox (queued outgoing messages) |
| `online-status-store.ts` | Tracks network online/offline status |

### UI State
| File | Description |
|------|-------------|
| `preferences-ui-store.ts` | Manages preferences panel UI state (active tab, navigation) |
| `modal-store.tsx` | Manages modal dialog display queue |
| `popover-store.tsx` | Manages popover display state |
| `searchable-component-store.ts` | Manages in-page search/find state for searchable components |
| `undo-redo-store.ts` | Manages undo/redo action stack |

### Activity & Tracking
| File | Description |
|------|-------------|
| `attachment-store.ts` | **AttachmentStore**: manages file attachments — download, upload, preview, save |
| `contact-store.ts` | **ContactStore**: manages contact lookup, search, and ranking for typeahead |
| `badge-store.ts` | Manages dock/taskbar badge count (unread messages) |
| `recently-read-store.ts` | Tracks recently read threads for mark-as-read timing |
| `thread-counts-store.ts` | Manages thread counts per category (inbox count, etc.) |
| `mail-rules-store.ts` | Manages user-defined mail rules and their processing |

### Data Sources
| File | Description |
|------|-------------|
| `observable-list-data-source.ts` | List data source backed by observable database queries |

## For AI Agents

### Working In This Directory
- **Stores are singletons** — they're instantiated once and shared across the entire application
- **DatabaseStore is the most critical store** — every data operation goes through it
- All stores extend `UnifyMailStore` (from `app/src/global/UnifyMail-store.ts`)
- Stores listen to Actions: `this.listenTo(Actions.actionName, this._handler)`
- Stores notify components by calling `this.trigger()` — components subscribe via `FluxContainer`
- **DraftEditingSession** is complex (17KB) — it manages real-time draft editing with debounced saves
- When adding a new store: register it in `UnifyMail-exports.js`

### Testing Requirements
- Store tests: `app/spec/stores/`
- Test by dispatching actions and verifying resulting state
- Mock DatabaseStore for stores that depend on database queries
- Test edge cases: error states, empty results, concurrent operations

### Common Patterns
- **Store skeleton**:
  ```typescript
  class MyStore extends UnifyMailStore {
    constructor() {
      super();
      this.listenTo(Actions.myAction, this._onMyAction);
    }
    _onMyAction = (payload) => {
      // update state
      this.trigger();
    }
  }
  export default new MyStore(); // singleton
  ```
- **Database queries in stores**: `DatabaseStore.findAll(Model).where([...]).then(results => ...)`
- **Observables**: Stores can use `QuerySubscription` for live-updating data

## Dependencies

### Internal
- `app/src/flux/actions.ts` — Actions that stores listen to
- `app/src/flux/models/` — Data models that stores manage
- `app/src/global/UnifyMail-store.ts` — Base store class
- `app/src/registries/` — Used by some stores for component/extension discovery

### External
- `better-sqlite3` — SQLite database (via DatabaseStore)
- `rx-lite` — Observable streams
- `underscore` — Collection utilities

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
