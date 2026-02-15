<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# extensions

## Purpose
Base classes for the extension point APIs. Plugins extend these classes to hook into specific parts of the application — the composer, message view, thread list, and account sidebar. Each extension class defines lifecycle methods that the core calls at appropriate times.

## Key Files

| File | Description |
|------|-------------|
| `composer-extension.ts` | **ComposerExtension**: Hook into the email composition lifecycle — modify draft before send, add toolbar buttons, transform body HTML |
| `message-view-extension.ts` | **MessageViewExtension**: Hook into message rendering — modify message HTML, add action buttons, process attachments |
| `thread-list-extension.ts` | **ThreadListExtension**: Customize thread list display — add columns, modify sorting |
| `account-sidebar-extension.ts` | **AccountSidebarExtension**: Customize the account sidebar — add sections, modify folder tree |
| `extension-utils.ts` | Shared utility functions for extension implementations |

## For AI Agents

### Working In This Directory
- These are **abstract base classes** — plugins subclass them and register via `ExtensionRegistry`
- Extension methods are called at specific lifecycle points by the core application
- `ComposerExtension` is the most commonly extended — used for link tracking, open tracking, templates, etc.
- Methods like `finalizeSessionBeforeSending()` run just before email send — use for last-minute modifications
- `MessageViewExtension` can transform rendered HTML — use for pixel stripping, image blocking, etc.

### Common Patterns
- Plugins: `class MyExtension extends ComposerExtension { static finalizeSessionBeforeSending(session) { ... } }`
- Registration: `ExtensionRegistry.Composer.register(MyExtension)`
- Static methods: Most extension methods are static (called on the class, not instances)

## Dependencies

### Internal
- `app/src/registries/extension-registry.ts` — Where extensions are registered
- `app/src/flux/stores/draft-editing-session.ts` — Passed to composer extension methods
- `app/src/flux/stores/message-body-processor.ts` — Invokes message view extensions

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
