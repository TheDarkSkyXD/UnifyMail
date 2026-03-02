# Testing Patterns

**Analysis Date:** 2026-03-01

## Test Framework

**Runner:**
- Jasmine 2.x (custom spec runner running inside Electron)
- Config: No standalone config file; test infrastructure lives in `app/spec/spec-runner/`
- The test runner boots a full Electron window with `AppEnv` initialized before running specs
- Enzyme 3.x with `enzyme-adapter-react-16` for React component testing

**Assertion Library:**
- Jasmine built-in matchers (`expect`, `toBe`, `toEqual`, `toHaveBeenCalledWith`, etc.)
- Custom matcher: `toHaveLength` (defined in `app/spec/spec-runner/jasmine-extensions.ts`)
- Underscore's `_.isEqual` is registered as a custom equality tester for `toEqual` assertions

**Run Commands:**
```bash
npm test                    # Run all tests (core + internal package specs)
npm test-window             # Run window-specific tests (shows specs in a browser window)
electron ./app --enable-logging --test      # Direct electron test command
electron ./app --enable-logging --test=window  # Direct window test command
```

## Test File Organization

**Location:**
- **Core specs**: `app/spec/` directory, mirroring the source structure
- **Plugin specs**: `app/internal_packages/<plugin-name>/specs/` directory within each plugin
- Note: some plugins use `spec/` (singular) instead of `specs/` (e.g., `category-picker/spec/`)

**Naming:**
- Spec files use `-spec` suffix with various extensions: `.ts`, `.tsx`, `.jsx`, `.es6`
- Pattern: `<module-name>-spec.<ext>` (e.g., `database-store-spec.ts`, `message-spec.ts`)
- The spec loader matches files with regex: `/-spec\.(js|jsx|es6|es|ts|tsx)$/`

**Structure:**
```
app/
├── spec/                              # Core test specs
│   ├── spec-runner/                   # Test infrastructure
│   │   ├── spec-bootstrap.ts          # Entry point for test runner
│   │   ├── spec-runner.ts             # Main test runner class
│   │   ├── spec-loader.ts             # Discovers and loads spec files
│   │   ├── master-before-each.ts      # Global beforeEach setup
│   │   ├── master-after-each.ts       # Global afterEach teardown
│   │   ├── jasmine-extensions.ts      # Custom matchers and helpers
│   │   ├── test-constants.ts          # Shared test constants
│   │   ├── time-override.ts           # Fake timer implementation
│   │   ├── console-reporter.ts        # Console output reporter
│   │   ├── terminal-reporter.ts       # Terminal output reporter
│   │   └── gui-reporter.tsx           # In-window visual reporter
│   ├── fixtures/                      # Shared test fixtures
│   │   ├── db-test-model.ts           # Reusable test model
│   │   ├── table-data.ts              # Table test data
│   │   ├── task-spec-handler.ts       # Task test helper
│   │   ├── emails/                    # Sample email HTML files
│   │   └── sample-deltas*.json        # Sample database deltas
│   ├── models/                        # Model spec files
│   ├── stores/                        # Store spec files
│   ├── components/                    # Component spec files
│   ├── services/                      # Service spec files
│   ├── registries/                    # Registry spec files
│   ├── tasks/                         # Task spec files
│   ├── utils/                         # Utility spec files
│   └── unifymail-test-utils.ts        # Shared test utilities
├── internal_packages/
│   ├── composer/specs/                # Composer plugin specs
│   ├── message-list/specs/            # Message list plugin specs
│   ├── notifications/specs/           # Notification plugin specs
│   └── ...                            # Each plugin has its own specs/
```

## Test Structure

**Suite Organization:**
```typescript
// From app/spec/stores/database-store-spec.ts
describe('DatabaseStore', function DatabaseStoreSpecs() {
  beforeEach(() => {
    TestModel.configureBasic();
    spyOn(ModelQuery.prototype, 'where').andCallThrough();

    this.performed = [];
    jasmine.unspy(DatabaseStore, '_query');
    spyOn(DatabaseStore, '_query').andCallFake((query, values = []) => {
      this.performed.push({ query, values });
      return Promise.resolve([]);
    });
  });

  describe('find', () =>
    it('should return a ModelQuery for retrieving a single item by Id', () => {
      const q = DatabaseStore.find(TestModel, '4');
      expect(q.sql()).toBe(
        "SELECT `TestModel`.`data` FROM `TestModel`  WHERE `TestModel`.`id` = '4'  LIMIT 1"
      );
    }));

  describe('findBy', () => {
    it('should pass the provided predicates on to the ModelQuery', () => {
      DatabaseStore.findBy<TestModel>(TestModel, testMatchers);
      expect(ModelQuery.prototype.where).toHaveBeenCalledWith(testMatchers);
    });
  });
});
```

**Patterns:**
- Use `describe` blocks with named `function` for the outer suite: `describe('Name', function nameTests() { })`
- Use nested `describe` blocks for sub-features
- Use arrow functions for `it`, `beforeEach`, `afterEach` blocks
- Store test state on `this` context: `this.fakeThread = new Thread(...)`, `this.performed = []`
- The test runner auto-wraps `it` blocks to support returning promises (async tests work without `waitsForPromise` in newer specs)

**Setup/Teardown:**
- Global `beforeEach` (in `app/spec/spec-runner/master-before-each.ts`) automatically:
  - Resets `AppEnv` (prevents window state saves, menu sends)
  - Resets `DatabaseStore` (spies on `_query` to return empty results)
  - Resets `TaskQueue` (clears queue and completed lists)
  - Resets timers (replaces `setTimeout`/`setInterval` with fakes)
  - Resets `AccountStore` with two fake test accounts
  - Resets `Config` with fake persisted config
  - Resets clipboard
  - Clears `ComponentRegistry`
  - Advances clock by 1000ms
- Global `afterEach` (in `app/spec/spec-runner/master-after-each.ts`) automatically:
  - Deactivates all packages
  - Clears `#jasmine-content` DOM
  - Unmounts all React components
  - Restores spied functions
  - Validates `TaskQueue` is empty (throws if not)

## Mocking

**Framework:** Jasmine built-in spies

**Patterns:**

**spyOn for method spying/stubbing:**
```typescript
// Spy and replace return value
spyOn(AppEnv, 'inDevMode').andReturn(true);

// Spy and provide custom implementation
spyOn(DatabaseStore, '_query').andCallFake((query, values = []) => {
  this.performed.push({ query, values });
  return Promise.resolve([]);
});

// Spy and call through to original
spyOn(ModelQuery.prototype, 'where').andCallThrough();

// Create standalone spy
const onSelectItem = jasmine.createSpy('onSelectItem');

// Verify spy was called
expect(onSelectItem).toHaveBeenCalledWith('1', 0);
expect(onSelectItem).not.toHaveBeenCalled();
```

**Unspying (restoring originals):**
```typescript
// Custom unspy utility (defined in jasmine-extensions.ts, attached to jasmine global)
jasmine.unspy(DatabaseStore, '_query');

// Used when you need to replace the default spy from master-before-each
jasmine.unspy(DatabaseStore, '_query');
spyOn(DatabaseStore, '_query').andCallFake(/* new implementation */);
```

**What to Mock:**
- `DatabaseStore._query` - Always mocked globally (DB not opened in test mode)
- `AppEnv` methods - `saveWindowState`, `menu.sendToBrowserProcess`, `inDevMode`, `newWindow`
- `DraftFactory` methods - `createDraftForReply`, `createDraft`, etc.
- `CategoryStore` / `AccountStore` lookup methods
- `clipboard` read/write methods
- Timers (`setTimeout`, `setInterval`) - automatically faked via `TimeOverride`

**What NOT to Mock:**
- Model constructors and their methods (test real model behavior)
- Query builders and SQL generation (test real query construction)
- Utility functions (test real logic)
- React component rendering (use real render with Enzyme/ReactTestUtils)

## Fixtures and Factories

**Test Data:**

**Test Constants** (from `app/spec/spec-runner/test-constants.ts`):
```typescript
export default {
  TEST_TIME_ZONE: 'America/Los_Angeles',
  TEST_PLUGIN_ID: 'test-plugin-id-123',
  TEST_ACCOUNT_ID: 'test-account-server-id',
  TEST_ACCOUNT_NAME: 'UnifyMail Test',
  TEST_ACCOUNT_EMAIL: 'tester@UnifyMail.com',
  TEST_ACCOUNT_CLIENT_ID: 'local-test-account-client-id',
  TEST_ACCOUNT_ALIAS_EMAIL: 'tester+alternative@UnifyMail.com',
};
```
These constants are exposed as globals: `TEST_ACCOUNT_ID`, `TEST_ACCOUNT_EMAIL`, etc.

**Test Model** (from `app/spec/fixtures/db-test-model.ts`):
```typescript
// Reusable configurable test model for database tests
class TestModel extends Model {
  static attributes = { ...Model.attributes, clientId: ..., serverId: ... };
}
TestModel.configureBasic = () => { /* set minimal attributes */ };
TestModel.configureWithAllAttributes = () => { /* set all attribute types */ };
TestModel.configureWithCollectionAttribute = () => { /* add collection */ };
TestModel.configureWithJoinedDataAttribute = () => { /* add joined data */ };
```

**Inline Fixture Construction:**
```typescript
// Models are constructed inline in tests with minimal required data
const evan = new Contact({ name: 'Evan Morikawa', email: 'evan@UnifyMail.com' });
this.fakeThread = new Thread({ id: 'fake-thread', headerMessageId: 'fake-thread' });
const draft = new Message({ id: 'A', subject: 'B', headerMessageId: 'A', body: '123' });
```

**Location:**
- Shared fixtures: `app/spec/fixtures/`
- Email HTML fixtures: `app/spec/fixtures/emails/`
- Sample JSON data: `app/spec/fixtures/sample-deltas.json`, `app/spec/fixtures/sample-deltas-clustered.json`
- Test utilities: `app/spec/unifymail-test-utils.ts`

## Coverage

**Requirements:** None enforced. No coverage thresholds are configured.

**View Coverage:** Not configured. No coverage reporting tool is set up in the test pipeline.

## Test Types

**Unit Tests:**
- **Models**: Test serialization, query generation, business logic methods
  - Location: `app/spec/models/`
  - Example: `app/spec/models/message-spec.ts` tests `hasEmptyBody()`, `participants()`, `participantsForReply()`
- **Stores**: Test action handling, state management, data flow
  - Location: `app/spec/stores/`
  - Example: `app/spec/stores/database-store-spec.ts` tests query construction methods
- **Tasks**: Test task creation, validation, undo behavior
  - Location: `app/spec/tasks/`
  - Example: `app/spec/tasks/task-factory-spec.ts` tests category application tasks
- **Services**: Test transformation logic, parsing
  - Location: `app/spec/services/`
  - Example: `app/spec/services/search/search-query-parser-spec.ts` tests query parsing

**Component Tests:**
- Use Enzyme `mount()` or React `ReactTestUtils.renderIntoDocument()` for rendering
- Location: `app/spec/components/` and `app/internal_packages/*/specs/`
- Example: `app/internal_packages/notifications/specs/dev-mode-notif-spec.tsx` tests notification rendering

**Integration Tests:**
- No dedicated integration test suite
- Some store specs effectively test integration between stores, models, and actions

**E2E Tests:**
- Not used. No Playwright, Cypress, or Spectron setup.

## Common Patterns

**Async Testing:**
```typescript
// Pattern 1: waitsForPromise helper (legacy, still common)
it('resolves with an array of models', () =>
  waitsForPromise(() => {
    return DatabaseStore.modelify(Thread, input).then(output => {
      expect(output).toEqual(expectedOutput);
    });
  }));

// Pattern 2: Return promise directly (supported by custom spec runner)
it('resolves models', () => {
  return DatabaseStore.modelify(Thread, input).then(output => {
    expect(output).toEqual(expectedOutput);
  });
});

// Pattern 3: advanceClock for timer-based async
it('should attempt to focus the new draft', () => {
  DraftStore._onComposeReply({ threadId: 'fake', type: 'reply', behavior: 'prefer-existing' });
  advanceClock();
  advanceClock();
  // assertions here
});
```

**Timer Testing:**
```typescript
// TimeOverride replaces all timers with synchronous fakes
// Use advanceClock(ms) to move time forward
advanceClock(1000);  // Move 1 second forward, triggers any pending timeouts
```

**React Component Testing:**
```typescript
// Pattern 1: Enzyme mount (preferred for new tests)
import { mount } from 'enzyme';
this.notif = mount(<DevModeNotification />);
expect(this.notif.find('.notification').exists()).toEqual(true);

// Pattern 2: ReactTestUtils (legacy, still used)
import MTestUtils from '../unifymail-test-utils';
const list = MTestUtils.renderIntoDocument(<EditableList items={items} />);
const item = scryRenderedDOMComponentsWithClass(list, 'editable-item')[0];
Simulate.click(item);

// Pattern 3: DOM queries on mounted components
const visibleElems = wrapper.getDOMNode().querySelectorAll('.highest-priority');
expect(visibleElems.length).toEqual(1);
```

**Data-Driven Testing:**
```typescript
// Pattern: Array of test cases with forEach
const cases = [
  { itMsg: 'is an empty string', body: '', isEmpty: true },
  { itMsg: 'has plain text', body: 'Hi', isEmpty: false },
  { itMsg: 'is null', body: null, isEmpty: true },
];
cases.forEach(({ itMsg, body, isEmpty }) =>
  it(itMsg, function() {
    const msg = new Message({ body, pristine: false, draft: true });
    expect(msg.hasEmptyBody()).toBe(isEmpty);
  })
);
```

**Store Testing:**
```typescript
// Pattern: Spy on dependencies, test action handlers directly
beforeEach(() => {
  spyOn(DatabaseStore, 'run').andCallFake(query => {
    if (query._klass === Thread) return Promise.resolve(this.fakeThread);
    return Promise.reject(new Error(`Not Stubbed for class ${query._klass.name}`));
  });
});

it('calls the factory method', () => {
  DraftStore._onComposeReply({ threadId: 'fake-thread', type: 'reply', behavior: 'prefer-existing' });
  advanceClock();
  expect(DraftFactory.createOrUpdateDraftForReply).toHaveBeenCalledWith({ ... });
});
```

**Disabled Tests:**
- Use `xdescribe` or `xit` to skip entire suites or individual tests
- Several specs are currently disabled with `xdescribe` (e.g., `DraftStore`, `OpenTrackingComposerExtension`, `TaskFactory.tasksForApplyingCategories`)
- Some spec files are placeholder stubs: `app/internal_packages/send-and-archive/specs/send-and-archive-spec.ts` contains only `describe('SendAndArchive', function() {});`
- Some specs are intentionally trivial: `app/internal_packages/phishing-detection/specs/main-spec.tsx` contains `expect(true).toBe(true)`

## Test Utilities Reference

**`app/spec/unifymail-test-utils.ts`:**
- `renderIntoDocument(component)` - Renders React component and attaches to DOM (unlike React's version which doesn't)
- `removeFromDocument(reactElement)` - Removes rendered component from DOM
- `simulateCommand(target, command)` - Dispatches custom command events (for keyboard shortcut testing)
- `mockObservable(data, { dispose })` - Creates a mock observable that triggers immediately with data

**`app/spec/spec-runner/jasmine-extensions.ts`:**
- `waitsForPromise(fn)` - Waits for a promise-returning function to resolve before continuing
- `toHaveLength(expected)` - Custom matcher for checking array/string length
- `unspy(object, methodName)` - Restores a spied method to its original implementation
- `attachToDOM(element)` - Appends element to `#jasmine-content` div
- `testNowMoment()` - Returns a fixed moment (2016-03-15 12:00 in America/Los_Angeles timezone)

**Global Test Functions:**
These are available in all spec files without import:
- `describe`, `it`, `beforeEach`, `afterEach`, `expect`, `spyOn` (Jasmine)
- `waitsForPromise` (custom async helper)
- `advanceClock(ms)` (fake timer advancement)
- `testNowMoment()` (fixed date for deterministic tests)
- `TEST_ACCOUNT_ID`, `TEST_ACCOUNT_NAME`, `TEST_ACCOUNT_EMAIL`, `TEST_ACCOUNT_ALIAS_EMAIL` (test constants)

---

*Testing analysis: 2026-03-01*
