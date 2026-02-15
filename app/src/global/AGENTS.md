<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# global

## Purpose
Global module exports that define the public API surface of the Mailspring core. These modules are what internal packages (plugins) import to access models, stores, actions, and UI components. They act as barrel exports aggregating the entire core API.

## Key Files

| File | Description |
|------|-------------|
| `mailspring-exports.js` | Main public API: exports all Models, Stores, Actions, Utils — imported as `mailspring-exports` |
| `mailspring-exports.d.ts` | TypeScript declarations for `mailspring-exports` |
| `mailspring-component-kit.js` | UI component library: exports all reusable React components — imported as `mailspring-component-kit` |
| `mailspring-component-kit.d.ts` | TypeScript declarations for `mailspring-component-kit` |
| `mailspring-store.ts` | Base class for Flux stores with event emitter pattern |
| `mailspring-observables.ts` | RxJS observable helpers for reactive database queries and list watching |

## For AI Agents

### Working In This Directory
- **When adding new models, stores, or components to the core, you MUST also add them to the corresponding barrel export file**
- `mailspring-exports` is the most important module — virtually every plugin imports from it
- `mailspring-component-kit` provides the component library — plugins use these instead of building their own
- `mailspring-store.ts` is the base class all stores extend — it provides `trigger()` and listener management
- `mailspring-observables.ts` provides `Rx.Observable` wrappers for database query subscriptions

### Testing Requirements
- Verify that all exported symbols are correctly re-exported
- Changes to exports can break any plugin — check all `internal_packages/` imports

### Common Patterns
- **Plugin imports**: `const { Actions, Message, DatabaseStore } = require('mailspring-exports')`
- **Component imports**: `const { RetinaImg, Spinner } = require('mailspring-component-kit')`
- **Store base**: Stores extend `MailspringStore` and call `this.trigger()` to notify listeners

## Dependencies

### Internal
- `app/src/flux/` — Models, stores, actions, tasks
- `app/src/components/` — React UI components
- `app/src/registries/` — Registry singletons exported publicly
- `app/src/` — Various utilities exported publicly

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
