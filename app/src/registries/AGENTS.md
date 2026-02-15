<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# registries

## Purpose
Dependency injection registries that enable the plugin architecture. Plugins register their components, commands, extensions, services, and sounds here, and the core application discovers and uses them dynamically. This is the primary mechanism for loose coupling between core and plugins.

## Key Files

| File | Description |
|------|-------------|
| `component-registry.ts` | **ComponentRegistry**: Plugins register React components to named roles (e.g., `MessageList:Header`). Core UI uses `<InjectedComponent>` to render registered components. |
| `command-registry.ts` | **CommandRegistry**: Registers named commands (e.g., `core:delete-item`) that can be bound to keyboard shortcuts and menu items |
| `extension-registry.ts` | **ExtensionRegistry**: Registers extensions for composer, message-view, thread-list, and sidebar customization |
| `service-registry.ts` | **ServiceRegistry**: Registers named services (e.g., search providers) |
| `serializable-registry.ts` | **SerializableRegistry**: Registers serializable task and model classes for JSON deserialization |
| `database-object-registry.ts` | **DatabaseObjectRegistry**: Registers model classes that can be stored in the database |
| `sound-registry.ts` | **SoundRegistry**: Registers sound effects mapped to application events (new mail, send, etc.) |

## For AI Agents

### Working In This Directory
- **ComponentRegistry is the most-used registry** — it's how plugins inject UI into the app
- Registry instances are singletons — they're created once and shared globally
- When creating a new plugin, you'll typically use `ComponentRegistry.register()` and `ExtensionRegistry.register()`
- `SerializableRegistry` is critical for task persistence — every Task subclass must be registered
- Don't modify registry interfaces without checking all consuming plugins

### Testing Requirements
- Tests in `app/spec/registries/`
- Test component registration, unregistration, and retrieval
- Verify that duplicate registrations are handled correctly

### Common Patterns
- **Register**: `ComponentRegistry.register(MyComponent, { role: 'RoleName', location: LocationConstant })`
- **Unregister**: `ComponentRegistry.unregister(MyComponent)` (in plugin `deactivate()`)
- **Query**: `ComponentRegistry.findComponentsMatching({ role: 'RoleName' })`
- **Extension points**: Extensions are registered by type (Composer, MessageView, ThreadList)
- **Sound events**: `SoundRegistry.playSound('send')` looks up registered sound files

## Dependencies

### Internal
- `app/src/components/injected-component.tsx` — Renders components from ComponentRegistry
- `app/src/components/injected-component-set.tsx` — Renders sets of components from ComponentRegistry
- `app/src/extensions/` — Base classes for extensions registered in ExtensionRegistry
- `app/src/flux/tasks/task.ts` — Base class for tasks in SerializableRegistry

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
