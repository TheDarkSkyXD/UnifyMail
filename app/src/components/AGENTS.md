<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# components

## Purpose
Reusable React UI components that form the application's component library. These components are available to all internal packages via `mailspring-component-kit` imports. Includes everything from primitive UI elements (buttons, spinners, switches) to complex interactive widgets (list views, editors, popovers, tokenizing text fields).

## Key Files

### Layout & Containers
| File | Description |
|------|-------------|
| `flexbox.tsx` | Flexbox layout container component |
| `scroll-region.tsx` | Custom scrollbar region with OS-native feel and scroll tracking |
| `resizable-region.tsx` | Draggable resizable panel dividers |
| `focus-container.tsx` | Focus management container for keyboard navigation |
| `tab-group-region.tsx` | Tab key navigation group for accessibility |
| `key-commands-region.tsx` | Keyboard command handler region (hotkey scoping) |

### Lists & Tables
| File | Description |
|------|-------------|
| `list-tabular.tsx` | Virtual-scrolling tabular list (used for thread list) |
| `list-tabular-item.tsx` | Individual row in the tabular list |
| `lazy-rendered-list.tsx` | Lazily rendered list for performance with many items |
| `multiselect-list.tsx` | List with multi-selection (checkboxes, range select, bulk actions) |
| `multiselect-list-interaction-handler.ts` | Keyboard/mouse interaction handler for multiselect lists |
| `multiselect-split-interaction-handler.ts` | Split-pane interaction handler for multiselect |
| `multiselect-toolbar.tsx` | Bulk action toolbar shown during multi-selection |
| `multiselect-dropdown.tsx` | Dropdown variant of multiselect |
| `selectable-table.tsx` | Table with cell/row selection support |
| `editable-table.tsx` | Editable data table (inline editing) |
| `editable-list.tsx` | Sortable, editable list (add/remove/reorder items) |
| `list-data-source.ts` | Abstract data source interface for list components |
| `list-selection.ts` | Selection state management (single, multi, range) |

### Form Inputs
| File | Description |
|------|-------------|
| `tokenizing-text-field.tsx` | Token-based input (used for email recipients: To, CC, BCC) |
| `participants-text-field.tsx` | Specialized tokenizing field for email participants with typeahead |
| `date-input.tsx` | Date text input with natural language parsing |
| `date-picker.tsx` | Visual date picker component |
| `date-picker-popover.tsx` | Date picker in a popover container |
| `time-picker.tsx` | Time selection component |
| `mini-month-view.tsx` | Compact month calendar view |
| `switch.tsx` | Toggle switch component |

### Buttons & Actions
| File | Description |
|------|-------------|
| `button-dropdown.tsx` | Button with dropdown menu |
| `metadata-composer-toggle-button.tsx` | Toggle button for composer metadata features (tracking, etc.) |
| `open-identity-page-button.tsx` | Button linking to Mailspring identity page |

### Overlays & Modals
| File | Description |
|------|-------------|
| `modal.tsx` | Modal dialog component |
| `fixed-popover.tsx` | Fixed-position popover with arrow (used for menus, pickers) |
| `notification.tsx` | In-app notification banner |
| `billing-modal.tsx` | Subscription/billing information modal |
| `feature-used-up-modal.tsx` | Feature usage limit reached modal |

### Display & Feedback
| File | Description |
|------|-------------|
| `spinner.tsx` | Loading spinner animation |
| `retina-img.tsx` | Retina-aware image component (@1x, @2x asset resolution) |
| `disclosure-triangle.tsx` | Expandable/collapsible disclosure triangle |
| `bolded-search-result.tsx` | Search result with highlighted matching text |
| `account-color-bar.tsx` | Colored bar indicating account identity |
| `code-snippet.tsx` | Syntax-highlighted code display |
| `scrollbar-ticks.tsx` | Tick marks on scrollbar (e.g., search match positions) |

### Data Display
| File | Description |
|------|-------------|
| `mail-label.tsx` | Single email label/tag chip |
| `mail-label-set.tsx` | Set of email label chips |
| `mail-important-icon.tsx` | Gmail-style importance indicator icon |
| `contact-profile-photo.tsx` | Contact avatar/profile photo with fallback |

### Navigation & Views
| File | Description |
|------|-------------|
| `outline-view.tsx` | Sidebar outline/tree view |
| `outline-view-item.tsx` | Individual item in the outline view (with expand, count badge, drag) |
| `menu.tsx` | Command palette / menu component with keyboard navigation |
| `dropdown-menu.tsx` | Simple dropdown menu |

### Plugin Integration
| File | Description |
|------|-------------|
| `injected-component.tsx` | Renders a single component registered to a named role |
| `injected-component-set.tsx` | Renders all components registered to a named role |
| `injected-component-label.tsx` | Label for component injection points (debug mode) |
| `injected-component-error-boundary.tsx` | Error boundary wrapping injected plugin components |
| `flux-container.tsx` | HOC connecting React components to Flux stores |
| `config-prop-container.tsx` | HOC that passes config values as props |
| `bind-global-commands.ts` | Utility to bind global keyboard commands to a component |

### Content Display
| File | Description |
|------|-------------|
| `evented-iframe.tsx` | Iframe component with event forwarding (used to render email HTML) |
| `webview.tsx` | Electron webview component wrapper |
| `swipe-container.tsx` | Swipe gesture container for mobile-style interactions |
| `drop-zone.tsx` | Drag-and-drop target zone |

### Scenario/Rules Editor
| File | Description |
|------|-------------|
| `scenario-editor.tsx` | Visual editor for mail rules / filter scenarios |
| `scenario-editor-row.tsx` | Single condition/action row in the scenario editor |
| `scenario-editor-models.ts` | Data models for scenario conditions and actions |

### Lists State
| File | Description |
|------|-------------|
| `empty-list-state.tsx` | Empty state placeholder for lists (no results / getting started) |
| `syncing-list-state.tsx` | Syncing state placeholder for lists |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `composer-editor/` | Slate-based rich text editor for email composition |
| `decorators/` | React component decorators/HOCs |
| `table/` | Table-related sub-components |

## For AI Agents

### Working In This Directory
- Components are exported via `app/src/global/mailspring-component-kit.js` — add new components to that barrel export
- Follow existing component patterns: functional or class-based React with TypeScript
- Most components accept both `className` and `style` props for customization
- Complex components (list-tabular, tokenizing-text-field) have significant performance optimizations — modify carefully
- Use `RetinaImg` instead of `<img>` for proper HiDPI support

### Testing Requirements
- Component tests go in `app/spec/components/`
- Use React test renderer or shallow rendering for unit tests
- Test keyboard navigation, accessibility, and edge cases

### Common Patterns
- **Props interfaces**: Defined above the component, suffixed with `Props`
- **Refs**: Use `React.createRef()` for DOM access
- **Event handlers**: Named `_onEventName` (underscore prefix for private handlers)
- **CSS classes**: Use BEM-like naming matching the component name
- **Observables**: Some components subscribe to stores using `componentDidMount` / `componentWillUnmount`

## Dependencies

### Internal
- `app/src/flux/` — Stores and actions used by stateful components
- `app/src/registries/component-registry.ts` — Component registration system
- `app/static/style/` — Base LESS variables and mixins

### External
- React 16.x — Component framework
- Slate — Rich text editor (composer-editor)
- classnames / clsx — Conditional CSS class names

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
