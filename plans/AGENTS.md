<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# plans

## Purpose
Detailed implementation plans for CalDAV/CardDAV features, calendar enhancements, and provider compatibility. These plans cover specific tasks, edge cases, and provider-specific quirks for the calendar and contacts sync systems.

## Key Files

| File | Description |
|------|-------------|
| `caldav-04-deleted-calendar-cleanup.md` | Plan for handling cleanup of deleted CalDAV calendars |
| `caldav-08-recurring-event-exceptions.md` | Plan for recurring event exception handling (EXDATE, RDATE) |
| `caldav-09-multiple-vevents.md` | Plan for handling iCal files with multiple VEVENT components |
| `caldav-10-vtodo-support.md` | Plan for implementing VTODO (task/to-do) support |
| `calendar-feature-assessment.md` | Comprehensive assessment of calendar feature completeness and gaps |
| `carddav-02-multiple-address-books.md` | Plan for supporting multiple CardDAV address books |
| `carddav-05-addressbook-metadata.md` | Plan for syncing address book metadata (display name, colors) |
| `carddav-07-vcard-error-handling.md` | Plan for robust vCard parsing error handling and recovery |
| `implement-event-search.md` | Plan for implementing calendar event search functionality |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `caldav-provider-quirks/` | Provider-specific compatibility notes (Google, iCloud, Fastmail, etc.) |

## For AI Agents

### Working In This Directory
- These are reference documents — consult them when working on CalDAV/CardDAV features
- Plans are numbered (caldav-XX, carddav-XX) indicating implementation order
- Cross-reference with `calendar-feature-assessment.md` for overall progress tracking

### Common Patterns
- Plans follow structured sections: Problem, Solution, Implementation Steps, Edge Cases, Testing
- CalDAV/CardDAV plans reference RFC standards (RFC 4791, RFC 6352, RFC 5545)

## Dependencies

### Internal
- `app/internal_packages/main-calendar/` — Calendar UI implementation
- `app/internal_packages/events/` — Event handling package
- `app/internal_packages/contacts/` — Contact management package
- `app/src/flux/models/event.ts` — Event data model
- `app/src/flux/models/calendar.ts` — Calendar data model
- `app/src/flux/models/contact.ts` — Contact data model

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
