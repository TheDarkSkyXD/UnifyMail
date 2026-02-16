<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# tasks

## Purpose
Background tasks that represent operations to be synced with mail servers via the mailsync C++ engine. Tasks are queued in the `TaskQueue`, executed locally first (optimistic UI updates), then synced to the server. They support undo, retry, and dependency ordering.

## Key Files

### Base
| File | Description |
|------|-------------|
| `task.ts` | **Task** base class: defines lifecycle (`performLocal`, `performRemote`), undo, retry, dependencies |
| `task-factory.ts` | Factory methods for creating common tasks with correct parameters |

### Email Operations
| File | Description |
|------|-------------|
| `send-draft-task.ts` | Send an email draft — validates, uploads attachments, dispatches to mailsync |
| `syncback-draft-task.ts` | Save draft to server (IMAP draft folder sync) |
| `destroy-draft-task.ts` | Delete a draft locally and from server |
| `get-message-rfc2822-task.ts` | Fetch raw RFC2822 message source |

### Folder & Label Operations
| File | Description |
|------|-------------|
| `change-mail-task.ts` | Base class for tasks that modify multiple messages/threads |
| `change-folder-task.ts` | Move messages between folders (e.g., Archive, Trash) |
| `change-labels-task.ts` | Add/remove Gmail labels from messages |
| `change-starred-task.ts` | Star/unstar messages |
| `change-unread-task.ts` | Mark messages as read/unread |
| `expunge-all-in-folder-task.ts` | Empty a folder (e.g., "Empty Trash") |

### Category Management
| File | Description |
|------|-------------|
| `syncback-category-task.ts` | Create a new folder/label on the server |
| `destroy-category-task.ts` | Delete a folder/label from the server |
| `change-role-mapping-task.ts` | Change folder role mapping (e.g., set a folder as Trash) |

### Contact Operations
| File | Description |
|------|-------------|
| `syncback-contact-task.ts` | Create/update a contact on the server (CardDAV) |
| `destroy-contact-task.ts` | Delete a contact from the server |
| `syncback-contactgroup-task.ts` | Create/update a contact group |
| `destroy-contactgroup-task.ts` | Delete a contact group |
| `change-contactgroup-membership-task.ts` | Add/remove contacts from a group |

### Calendar Operations
| File | Description |
|------|-------------|
| `syncback-event-task.ts` | Create/update a calendar event on the server (CalDAV) |
| `destroy-event-task.ts` | Delete a calendar event |
| `event-rsvp-task.ts` | Respond to a calendar event invitation (Accept/Decline/Tentative) |

### Metadata & Model Operations
| File | Description |
|------|-------------|
| `syncback-metadata-task.ts` | Sync plugin metadata for a model to the UnifyMail API |
| `destroy-model-task.ts` | Generic model deletion task |
| `send-feature-usage-event-task.ts` | Report feature usage event to UnifyMail API |

## For AI Agents

### Working In This Directory
- **Tasks follow a two-phase pattern**: `performLocal()` for optimistic UI update, then mailsync handles server sync
- All task classes must be registered in `SerializableRegistry` for persistence/deserialization
- Tasks are queued in `TaskQueue` — the queue handles ordering, dependencies, and retries
- `send-draft-task.ts` is the most complex task (8KB) — handles attachment uploads, validation, error recovery
- When creating a new task: extend `Task`, implement `performLocal()`, register in serializable registry, add to `UnifyMail-exports`

### Testing Requirements
- Task tests: `app/spec/tasks/`
- Test `performLocal()` behavior (database changes, optimistic updates)
- Test edge cases: network failure, conflict resolution, undo behavior
- Mock `DatabaseStore` and `mailsync-bridge` for isolation

### Common Patterns
- **Task definition**:
  ```typescript
  class MyTask extends Task {
    static attributes = { ...Task.attributes, myField: Attributes.String({...}) };
    performLocal() { /* update local DB */ }
  }
  ```
- **Dispatching**: `Actions.queueTask(new ChangeStarredTask({ threads, starred: true }))`
- **Undo**: Tasks can implement `canBeUndone()` and `createUndoTask()`
- **Factory usage**: `TaskFactory.taskForMovingToTrash({ threads, source: 'Swipe' })`

## Dependencies

### Internal
- `app/src/flux/stores/task-queue.ts` — Manages task execution
- `app/src/flux/stores/database-store.ts` — Local database operations
- `app/src/flux/mailsync-bridge.ts` — Sends tasks to mailsync for server execution
- `app/src/registries/serializable-registry.ts` — Task class registration

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
