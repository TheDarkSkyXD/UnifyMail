<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# global

## Purpose
Global module exports that define the public API surface of the UnifyMail core. These modules are what internal packages (plugins) import to access models, stores, actions, and UI components. They act as barrel exports aggregating the entire core API.

## Key Files

| File | Description |
|------|-------------|
| `UnifyMail-exports.js` | Main public API: exports all Models, Stores, Actions, Utils — imported as `UnifyMail-exports` |
| `UnifyMail-exports.d.ts` | TypeScript declarations for `UnifyMail-exports` |
| `UnifyMail-component-kit.js` | UI component library: exports all reusable React components — imported as `UnifyMail-component-kit` |
| `UnifyMail-component-kit.d.ts` | TypeScript declarations for `UnifyMail-component-kit` |
| `UnifyMail-store.ts` | Base class for Flux stores with event emitter pattern |
| `UnifyMail-observables.ts` | RxJS observable helpers for reactive database queries and list watching |

## For AI Agents

### Working In This Directory
- **When adding new models, stores, or components to the core, you MUST also add them to the corresponding barrel export file**
- `UnifyMail-exports` is the most important module — virtually every plugin imports from it
- `UnifyMail-component-kit` provides the component library — plugins use these instead of building their own
- `UnifyMail-store.ts` is the base class all stores extend — it provides `trigger()` and listener management
- `UnifyMail-observables.ts` provides `Rx.Observable` wrappers for database query subscriptions

### Testing Requirements
- Verify that all exported symbols are correctly re-exported
- Changes to exports can break any plugin — check all `internal_packages/` imports

### Common Patterns
- **Plugin imports**: `const { Actions, Message, DatabaseStore } = require('UnifyMail-exports')`
- **Component imports**: `const { RetinaImg, Spinner } = require('UnifyMail-component-kit')`
- **Store base**: Stores extend `UnifyMailStore` and call `this.trigger()` to notify listeners

## Dependencies

### Internal
- `app/src/flux/` — Models, stores, actions, tasks
- `app/src/components/` — React UI components
- `app/src/registries/` — Registry singletons exported publicly
- `app/src/` — Various utilities exported publicly

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
