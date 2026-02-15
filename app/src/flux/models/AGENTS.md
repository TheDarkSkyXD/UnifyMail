<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# models

## Purpose
Data model classes forming the ORM layer. Each model maps to a database table and defines its attributes (columns), relationships, and serialization behavior. Models are the core data types used throughout the application — Message, Thread, Contact, Account, Event, Category, etc.

## Key Files

### Core Base
| File | Description |
|------|-------------|
| `model.ts` | **Model** base class: defines attribute declaration, serialization, JSON conversion, database table mapping |
| `model-with-metadata.ts` | **ModelWithMetadata**: extends Model with plugin metadata support (key-value store per model instance) |
| `utils.ts` | Model utility functions: ID generation, date parsing, HTML stripping, attribute diffing |

### Email Domain Models
| File | Description |
|------|-------------|
| `account.ts` | **Account**: email account configuration — provider, credentials, sync settings |
| `message.ts` | **Message**: individual email — subject, body, participants, headers, attachments, flags |
| `thread.ts` | **Thread**: conversation thread — aggregates messages, latest message, participants, labels |
| `file.ts` | **File**: email attachment metadata — filename, size, content type, content ID |
| `category.ts` | **Category**: email folder or label (abstract base for Folder and Label) |
| `folder.ts` | **Folder**: IMAP folder (extends Category) |
| `label.ts` | **Label**: Gmail-style label (extends Category) |
| `message-utils.ts` | Message-specific utility functions |

### Calendar & Contacts
| File | Description |
|------|-------------|
| `event.ts` | **Event**: calendar event — title, start/end time, location, recurrence, attendees |
| `calendar.ts` | **Calendar**: calendar container — name, color, account association |
| `contact.ts` | **Contact**: contact record — name, email, phone, address, vCard data |
| `contact-book.ts` | **ContactBook**: address book container for CardDAV |
| `contact-group.ts` | **ContactGroup**: contact group / distribution list |

### Database Query System
| File | Description |
|------|-------------|
| `query.ts` | **ModelQuery**: fluent query builder — `find()`, `where()`, `order()`, `limit()`, `include()` |
| `query-subscription.ts` | **QuerySubscription**: observable database query that re-runs when relevant data changes |
| `query-subscription-pool.ts` | Pool managing shared query subscriptions (deduplication) |
| `query-result-set.ts` | **QueryResultSet**: immutable result set from a database query |
| `mutable-query-result-set.ts` | Mutable version of QueryResultSet for incremental updates |
| `mutable-query-subscription.ts` | Mutable query subscription for live-updating lists |
| `unread-query-subscription.ts` | Specialized subscription for unread message counts |
| `query-range.ts` | Range specification for paginated queries (offset + limit) |
| `provider-syncback-request.ts` | Model for pending syncback operations to the mail server |

## For AI Agents

### Working In This Directory
- **Every model must define a static `attributes` getter** returning attribute descriptors — this drives the ORM
- When adding a new model: define `attributes`, register in `DatabaseObjectRegistry`, add to `mailspring-exports`
- `query.ts` implements a chainable query builder — `DatabaseStore.findAll(Thread).where([matcher]).order(attr)`
- Query subscriptions are the reactive data layer — they re-execute when the underlying data changes
- **`utils.ts` is large (22KB)** — contains many essential utility functions (ID gen, date parse, etc.)

### Testing Requirements
- Model tests: `app/spec/models/`
- Test attribute definitions, serialization round-trips, and query building
- Test query subscriptions with mock database triggers

### Common Patterns
- **Model definition**: `static attributes = { name: Attributes.String({ modelKey: 'name', jsonKey: 'name' }) }`
- **Fluent queries**: `DatabaseStore.findAll(Message).where([Message.attributes.threadId.equal(id)])`
- **Subscriptions**: `new QuerySubscription(query, { emitResultSet: true })`
- **Serialization**: Models serialize to/from JSON and SQLite row objects
- **Metadata**: `ModelWithMetadata.pluginMetadata` stores per-plugin data on model instances

## Dependencies

### Internal
- `app/src/flux/attributes/` — Attribute type classes used in model definitions
- `app/src/flux/stores/database-store.ts` — Executes queries against SQLite
- `app/src/registries/database-object-registry.ts` — Model registration

### External
- `better-sqlite3` — Underlying SQLite engine
- `rx-lite` — Observable streams for query subscriptions

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
