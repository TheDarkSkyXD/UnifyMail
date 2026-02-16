<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# internal_packages

## Purpose
Internal plugin packages that implement the application's features. Each subdirectory is a self-contained plugin with its own `package.json`, React components, stores, styles, and assets. The `PackageManager` in `src/package-manager.ts` discovers and activates these packages at startup. This is the primary extension point for adding new features.

## Subdirectories

### Email & Thread Management
| Directory | Purpose |
|-----------|---------|
| `thread-list/` | Thread listing view: inbox, sent, archive list UI |
| `thread-search/` | Search functionality for threads/messages |
| `thread-sharing/` | Thread sharing/forwarding features |
| `thread-snooze/` | Snooze threads to reappear later |
| `message-list/` | Message conversation view: message rendering, headers, body |
| `message-autoload-images/` | Auto-load remote images in messages |
| `message-view-on-github/` | View message source on GitHub |
| `draft-list/` | Drafts list view |
| `list-unsubscribe/` | One-click mailing list unsubscribe |

### Composition
| Directory | Purpose |
|-----------|---------|
| `composer/` | Email composition UI: editor, recipients, attachments, send |
| `composer-signature/` | Email signature management and insertion |
| `composer-templates/` | Reusable email template system |
| `attachments/` | Attachment handling: upload, download, preview |

### Calendar & Events
| Directory | Purpose |
|-----------|---------|
| `main-calendar/` | Full calendar view: month/week/day views, event display |
| `events/` | Calendar event creation, editing, and RSVP handling |

### Contacts
| Directory | Purpose |
|-----------|---------|
| `contacts/` | Contact management: list, detail view, search |
| `participant-profile/` | Contact profile sidebar in message view |
| `github-contact-card/` | GitHub profile integration for contacts |

### Organization & Categories
| Directory | Purpose |
|-----------|---------|
| `account-sidebar/` | Left sidebar: account list, folder tree, labels |
| `category-picker/` | Label/folder picker for categorizing messages |
| `category-mapper/` | Category-to-role mapping (Inbox, Sent, Trash, etc.) |

### Productivity Features
| Directory | Purpose |
|-----------|---------|
| `send-later/` | Schedule emails to be sent at a future time |
| `send-reminders/` | Follow-up reminders for sent emails |
| `send-and-archive/` | Combined send + archive action |
| `undo-redo/` | Undo/redo system for destructive actions |
| `link-tracking/` | Track clicks on links in sent emails |
| `open-tracking/` | Track email opens via tracking pixels |

### Notifications & Alerts
| Directory | Purpose |
|-----------|---------|
| `notifications/` | In-app notification banner system |
| `unread-notifications/` | Desktop notifications for new unread messages |
| `activity/` | Activity/notification feed panel |

### Preferences & Settings
| Directory | Purpose |
|-----------|---------|
| `preferences/` | Settings UI: accounts, general, shortcuts, etc. |
| `onboarding/` | First-run setup wizard: account configuration, welcome |
| `custom-fonts/` | Custom font selection for reading/composing |
| `custom-sounds/` | Custom notification sound selection |

### Security & Privacy
| Directory | Purpose |
|-----------|---------|
| `phishing-detection/` | Phishing email detection and warnings |
| `remove-tracking-pixels/` | Strip tracking pixels from incoming emails |
| `personal-level-indicators/` | Personal/mailing-list message indicators |

### UI, Themes & Appearance
| Directory | Purpose |
|-----------|---------|
| `theme-picker/` | Theme selection and preview UI |
| `ui-dark/` | Dark theme stylesheet |
| `ui-darkside/` | Dark Side theme stylesheet |
| `ui-less-is-more/` | Minimal/clean theme stylesheet |
| `ui-light/` | Light theme stylesheet (default) |
| `ui-taiga/` | Taiga theme stylesheet |
| `ui-ubuntu/` | Ubuntu/GNOME theme stylesheet |
| `mode-switch/` | Switch between list/split view modes |
| `screenshot-mode/` | Screenshot mode (hides sensitive data for screenshots) |

### System
| Directory | Purpose |
|-----------|---------|
| `system-tray/` | System tray icon and menu |
| `print/` | Print email functionality |
| `translation/` | Message translation feature |

## For AI Agents

### Working In This Directory
- **Each package is self-contained** with its own `package.json`, `lib/` (source), and `styles/` directories
- To create a new plugin: copy an existing simple package, update `package.json`, register components
- Packages declare their `main` entry point and `windowTypes` they activate in
- Packages are loaded alphabetically by default; use `packageDependencies` for ordering
- **Theme packages** (ui-*) contain only LESS stylesheets and override base variables

### Testing Requirements
- Package-specific tests go in the package's own directory or in `app/spec/`
- Test with `npm test` from the root (runs all specs)

### Common Patterns
- **Activation**: Export an `activate()` function called on package load
- **Deactivation**: Export a `deactivate()` function for cleanup
- **Component registration**: `ComponentRegistry.register(MyComponent, { role: 'RoleName' })`
- **Store subscription**: `this.listenTo(SomeStore, this._onStoreChange)`
- **Actions**: Import from `UnifyMail-exports` and dispatch via `Actions.someAction()`
- **Styling**: Use LESS files in `styles/` directory, variables from the active theme

## Dependencies

### Internal
- `app/src/global/UnifyMail-exports` — Core API imports (Actions, Stores, Models)
- `app/src/global/UnifyMail-component-kit` — Reusable UI component imports
- `app/src/registries/` — Registration APIs
- `app/src/extensions/` — Extension point base classes

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
