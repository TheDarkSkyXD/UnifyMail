<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# docs

## Purpose
Feature specifications and task plans for upcoming or in-progress features. These documents define requirements, implementation strategies, and acceptance criteria for complex features like CalDAV/CardDAV integration, calendar event handling, and undo/redo patterns.

## Key Files

| File | Description |
|------|-------------|
| `EVENT-RSVP-TASK-SPECIFICATION.md` | Specification for calendar event RSVP (accept/decline/tentative) task implementation |
| `calendar-event-dragging-plan.md` | Implementation plan for drag-and-drop event rescheduling in the calendar view |
| `calendar-ics-helpers-plan.md` | Plan for ICS (iCalendar) file parsing and generation helper utilities |
| `calendar-ics-remaining-work.md` | Tracking document for remaining ICS helper implementation work |
| `undo-redo-task-pattern.md` | Design pattern document for undo/redo task architecture |

## For AI Agents

### Working In This Directory
- These are planning/specification documents — do NOT modify them unless the user explicitly requests updates
- Reference these docs when implementing related features to ensure alignment with the defined architecture
- If implementing a feature described here, cross-reference with the actual codebase to check what has already been built

### Common Patterns
- Documents follow a specification format with sections for: Overview, Architecture, Implementation Steps, Edge Cases, Testing
- Task specifications map to syncback tasks in `app/src/flux/tasks/`

## Dependencies

### Internal
- `app/src/flux/tasks/` — Task implementations that these specs describe
- `app/src/ics-event-helpers.ts` — ICS helper implementation
- `app/internal_packages/events/` — Events/calendar package
- `app/internal_packages/main-calendar/` — Calendar UI package

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
