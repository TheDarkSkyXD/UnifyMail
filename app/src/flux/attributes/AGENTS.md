<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# attributes

## Purpose
Database attribute type classes used in Model definitions. Each attribute type handles serialization, deserialization, query matching, and SQL generation for its data type. The attribute system is the core of the ORM layer — it maps TypeScript model properties to SQLite columns.

## Key Files

| File | Description |
|------|-------------|
| `attribute.ts` | **Attribute** base class: defines `modelKey`, `jsonKey`, `queryable`, and generates matcher/sort methods |
| `attribute-string.ts` | **AttributeString**: string column type with `equal`, `like`, `in` matchers |
| `attribute-number.ts` | **AttributeNumber**: numeric column type with comparison matchers (`greaterThan`, `lessThan`) |
| `attribute-boolean.ts` | **AttributeBoolean**: boolean column type (stored as 0/1 in SQLite) |
| `attribute-datetime.ts` | **AttributeDateTime**: date/time column type (stored as Unix timestamp) |
| `attribute-object.ts` | **AttributeObject**: JSON-serialized object column (stored as TEXT) |
| `attribute-collection.ts` | **AttributeCollection**: collection/array column (stored in join table or JSON) |
| `attribute-joined-data.ts` | **AttributeJoinedData**: large data stored in a separate table (e.g., message body HTML) |
| `matcher.ts` | **Matcher** classes: SQL WHERE clause generators for query predicates (`Equal`, `Like`, `In`, `And`, `Or`, `Not`) |
| `sort-order.ts` | **SortOrder**: SQL ORDER BY clause generator (ascending/descending) |

## For AI Agents

### Working In This Directory
- These are **internal ORM types** — rarely modified directly
- When adding new attribute types: extend `Attribute`, implement `toJSON`, `fromJSON`, and generate matchers
- `matcher.ts` is complex (9KB) — it generates SQL WHERE clauses from declarative predicates
- `AttributeJoinedData` uses a separate SQLite table for large data (message bodies) to keep main table small
- `queryable: true` means the attribute gets a real SQLite column; `false` means JSON-embedded only

### Common Patterns
- **Model usage**: `static attributes = { subject: Attributes.String({ modelKey: 'subject', queryable: true }) }`
- **Query matchers**: `Message.attributes.subject.like('%invoice%')` generates SQL `WHERE subject LIKE '%invoice%'`
- **Sort**: `Message.attributes.date.descending()` generates `ORDER BY date DESC`

## Dependencies

### Internal
- `app/src/flux/models/` — Models use these attribute types in their `attributes` declarations
- `app/src/flux/models/query.ts` — Uses matchers and sort orders to build SQL
- `app/src/flux/stores/database-store.ts` — Executes the generated SQL

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
