# Coding Conventions

**Analysis Date:** 2026-03-01

## Naming Patterns

**Files:**
- Source files use **kebab-case**: `draft-editing-session.ts`, `database-store.ts`, `change-starred-task.ts`
- React components also use kebab-case filenames: `retina-img.tsx`, `editable-list.tsx`, `fixed-popover.tsx`
- Test spec files use **kebab-case with `-spec` suffix**: `database-store-spec.ts`, `message-spec.ts`, `editable-list-spec.tsx`
- Style files use **kebab-case with `.less` extension**: `composer.less`
- Internal packages (plugins) use **kebab-case directory names**: `composer-signature/`, `thread-search/`, `send-later/`

**Classes:**
- Use **PascalCase** for all classes: `DraftStore`, `DatabaseStore`, `ChangeStarredTask`, `ComposerView`
- Model classes are singular nouns: `Thread`, `Message`, `Contact`, `Account`, `Folder`, `Label`, `Event`
- Store classes end with `Store`: `DatabaseStore`, `DraftStore`, `AccountStore`, `UndoRedoStore`
- Task classes end with `Task`: `SendDraftTask`, `ChangeStarredTask`, `DestroyDraftTask`

**Functions/Methods:**
- Use **camelCase** for all functions and methods: `findBy()`, `modelify()`, `hasRunLocally()`
- Event handler methods use `_on` prefix with camelCase: `_onComposeReply`, `_onSendDraft`, `_onDataChanged`
- Private/internal methods use `_` prefix: `_resetDatabase()`, `_query()`, `_createSession()`
- Boolean getter methods use `is`/`has` prefix: `isMainWindow()`, `hasRunLocally()`, `hasEmptyBody()`
- React arrow-function event handlers use `_on` prefix: `_onDraftReady = async () => {}`

**Variables:**
- Use **camelCase** for local variables and instance properties: `headerMessageId`, `draftSessions`, `fakeThread`
- Use **UPPER_SNAKE_CASE** for module-level constants: `PLUGIN_ID`, `PLUGIN_URL`, `BASE_RETRY_LOCK_DELAY`, `AGENT_PATH`
- Use **PascalCase** for enum-like constant objects: `Status.Local`, `Mode.ContentIsMask`
- Private instance properties use `_` prefix: `_draftSessions`, `_emitter`, `_registry`

**Types/Interfaces:**
- Use **PascalCase** for TypeScript types and interfaces: `ModelAttributes`, `AttributeValues`, `InjectedComponentProps`
- Props types follow `ComponentNameProps` pattern: `InjectedComponentProps`, `InjectedComponentState`
- Prefix `I` for interface is NOT used (no `IThread`, just `Thread`)

## Code Style

**Formatting:**
- Tool: Prettier v1.x
- Config: `.prettierrc`
- Key settings:
  - `printWidth`: 100
  - `singleQuote`: true
  - `trailingComma`: "es5"

**Linting:**
- Tool: ESLint v7 with `@typescript-eslint/parser`
- Config: `.eslintrc`
- Extends: `eslint:recommended`, `plugin:@typescript-eslint/recommended`, `prettier`
- Key relaxed rules (many TypeScript strict rules are disabled):
  - `@typescript-eslint/no-explicit-any`: off (any is used liberally throughout)
  - `@typescript-eslint/explicit-function-return-type`: off
  - `@typescript-eslint/no-unused-vars`: off
  - `@typescript-eslint/no-var-requires`: off (dynamic requires are common)
  - `no-console`: off (console.log is used freely)
- Lint is run via Grunt: `npm run lint` which calls `grunt lint --gruntfile=build/Gruntfile.js`

## Import Organization

**Order:**
1. Node.js built-in modules (`path`, `fs`, `child_process`, `events`)
2. Electron modules (`electron`, `@electron/remote`)
3. Third-party libraries (`underscore`, `react`, `react-dom`, `moment`, `better-sqlite3`)
4. UnifyMail global modules (`unifymail-exports`, `unifymail-component-kit`, `unifymail-store`)
5. Relative internal imports (`../../unifymail-frontend/src/flux/...`, `../lib/...`)

**Example pattern from `app/unifymail-frontend/src/flux/stores/draft-store.ts`:**
```typescript
import { ipcRenderer } from 'electron';
import UnifyMailStore from 'unifymail-store';
import { DraftEditingSession } from './draft-editing-session';
import DraftFactory, { ReplyType, ReplyBehavior } from './draft-factory';
import DatabaseStore from './database-store';
import { SendActionsStore } from './send-actions-store';
import * as Actions from '../actions';
```

**Path Aliases:**
- `unifymail-exports` - Global API exports (`app/unifymail-frontend/src/global/unifymail-exports.js`)
- `unifymail-component-kit` - Reusable UI components (`app/unifymail-frontend/src/global/unifymail-component-kit.js`)
- `unifymail-store` - Base store class (`app/unifymail-frontend/src/global/unifymail-store.ts`)
- `unifymail-observables` - Observable utilities (`app/unifymail-frontend/src/global/unifymail-observables.ts`)
- These are configured in `app/tsconfig.json` via `paths` and resolve to `app/unifymail-frontend/src/global/*`

**Import style:**
- Use named imports for models and multiple exports: `import { Thread, Message, Contact } from 'unifymail-exports'`
- Use default imports for stores and single-export modules: `import DatabaseStore from './database-store'`
- Use namespace imports for Actions and Attributes: `import * as Actions from '../actions'`, `import * as Attributes from '../attributes'`
- `underscore` is always imported as `_`: `import _ from 'underscore'`

## Error Handling

**Patterns:**
- Custom error classes are used sparingly. Only `APIError` exists in `app/unifymail-frontend/src/flux/errors.ts`
- Prefer native `new Error("descriptive message")` for most error cases
- Tasks validate in `willBeQueued()` by throwing errors:
  ```typescript
  willBeQueued() {
    if (this.threadIds.length === 0) {
      throw new Error('ChangeStarredTask: You must provide a `threads` Array of models or IDs.');
    }
    super.willBeQueued();
  }
  ```
- Database errors are handled via `handleUnrecoverableDatabaseError()` in `app/unifymail-frontend/src/flux/stores/database-store.ts` which reports to error logger and triggers DB reset
- Promises use `.catch()` chains rather than try/catch in many places (legacy pattern)
- Error reporting uses `AppEnv.errorLogger.reportError(err)` for production error tracking
- Store validation uses thrown string errors: `throw 'Listener is not able to listen to itself'` (in `unifymail-store.ts`)

## Logging

**Framework:** `debug` npm package + `console`

**Patterns:**
- Use `debug` for targeted debug logging in performance-sensitive areas:
  ```typescript
  import createDebug from 'debug';
  const debug = createDebug('app:RxDB');
  const debugVerbose = createDebug('app:RxDB:all');
  ```
- Use `console.log` / `console.error` for general logging (no restrictions - `no-console` rule is off)
- Use `console.inspect` for debugging object state in tests
- Error logging for production goes through `AppEnv.errorLogger.reportError(err)`

## Comments

**When to Comment:**
- Use block comments (`/* */`) with a doc-comment style for public API documentation. The format follows Atom's documentation syntax:
  ```typescript
  /*
  Public: The Thread model represents an email thread.

  Attributes

  `snippet`: {AttributeString} A short, ~140 character string...

  Section: Models
  @class Thread
  */
  ```
- Inline comments for explaining non-obvious behavior or workarounds
- `// TODO`, `// FIXME`, `// HACK` markers for known issues
- Many files contain `decaffeinate suggestions` comments from the original CoffeeScript-to-JS migration

**JSDoc/TSDoc:**
- Minimal TypeScript JSDoc usage - the codebase relies more on the Atom-style documentation format
- Some `/** */` JSDoc comments appear on public-facing components like `InjectedComponent`
- No enforced TSDoc standard

## Function Design

**Size:** No strict size limits enforced. Functions range from small helpers to larger lifecycle methods.

**Parameters:**
- Options objects are preferred for functions with multiple optional parameters:
  ```typescript
  constructor(data: AttributeValues<typeof ChangeStarredTask.attributes> & {
    threads?: Thread[];
    messages?: Message[];
  } = {})
  ```
- Default parameter values are used: `function morning(momentDate, morningHour = Hours.Morning)`

**Return Values:**
- Stores return `void` from action handlers and call `this.trigger()` to notify listeners
- Database queries return `Promise`-based results
- Model methods like `toJSON()` / `fromJSON()` are chainable (return `this`)

## Module Design

**Exports:**
- **Stores** are exported as singleton instances (instantiated at module level):
  ```typescript
  export default new UndoRedoStore();
  ```
- **Models** and **Tasks** are exported as named class exports:
  ```typescript
  export class Thread extends ModelWithMetadata { ... }
  export class ChangeStarredTask extends ChangeMailTask { ... }
  ```
- **Utility modules** use named exports: `export function waitsForPromise(...)`
- **Plugin entry points** export `activate()`, `deactivate()`, and optionally `serialize()` as named exports

**Barrel Files:**
- `app/unifymail-frontend/src/global/unifymail-exports.js` acts as the main barrel file, using lazy-loading via `Object.defineProperty` getters
- `app/unifymail-frontend/src/global/unifymail-component-kit.js` similarly lazy-loads UI components
- Internal packages import from these barrels: `import { Thread, Actions, DatabaseStore } from 'unifymail-exports'`

## React Component Patterns

**Class Components:**
- The codebase uses React 16.9 **class components** (not function components or hooks):
  ```typescript
  export default class InjectedComponent extends React.Component<
    InjectedComponentProps,
    InjectedComponentState
  > {
    static displayName = 'InjectedComponent';
    static propTypes = { ... };
    static defaultProps = { ... };
  }
  ```
- Always set `static displayName` on components
- Use `propTypes` for runtime prop validation (in addition to TypeScript types)
- Use arrow function class properties for event handlers: `_onDraftReady = async () => { ... }`

**Plugin Component Registration:**
- Components are registered via `ComponentRegistry.register(Component, { role: 'RoleName' })` in `activate()`
- Components are unregistered via `ComponentRegistry.unregister(Component)` in `deactivate()`
- Plugins specify which window types they load in via `package.json` `windowTypes` field

**Store Pattern:**
- Stores extend `UnifyMailStore` (custom Flux implementation in `app/unifymail-frontend/src/global/unifymail-store.ts`)
- Stores use `this.listenTo(Actions.someAction, this._handler)` in constructor
- Stores call `this.trigger()` to notify subscribers of state changes
- No Redux, no MobX - this is a custom lightweight Flux implementation

## TypeScript Configuration

- Target: ES2017
- Module: CommonJS
- JSX: React
- `strict` mode is NOT enabled
- `allowJs`: true (mixed JS/TS codebase)
- `experimentalDecorators`: true
- `skipLibCheck`: true
- Source maps: inline (`inlineSourceMap: true`)
- Config file: `app/tsconfig.json`

## Legacy Code Markers

- Many files contain `decaffeinate suggestions` comments indicating they were auto-converted from CoffeeScript
- Some files still use `.es6` and `.jsx` extensions alongside `.ts`/`.tsx`
- The `eslint` config references `mailspring-exports` and `mailspring-component-kit` as core modules (legacy names from the Mailspring fork)
- `proxyquire` v1.3.1 is available for module mocking but appears minimally used

---

*Convention analysis: 2026-03-01*
