// Event model — fat-row struct matching C++ Event.cpp.
//
// CRITICAL: Event's bindToQuery() does NOT call MailModel::bindToQuery().
// It binds its own columns directly (no version column).
//
// C++ table: Event
// Supports metadata: NO
// columnsForQuery: {id, data, icsuid, recurrenceId, accountId, etag, calendarId,
//                  recurrenceStart, recurrenceEnd}

use serde::{Deserialize, Serialize};
use crate::models::mail_model::MailModel;

/// Calendar event model.
///
/// CRITICAL: bind_to_statement() does NOT bind version. The Event table
/// has no version column in its columnsForQuery — C++ Event.bindToQuery()
/// does NOT call MailModel::bindToQuery().
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Primary key
    pub id: String,

    /// Account ID — JSON key "aid"
    #[serde(rename = "aid")]
    pub account_id: String,

    /// Version counter — JSON key "v" (in JSON but NOT in SQL binding)
    #[serde(rename = "v")]
    pub version: i64,

    /// Calendar ID — JSON key "cid"
    #[serde(rename = "cid", default)]
    pub calendar_id: String,

    /// ICS UID (unique event identifier)
    #[serde(rename = "icsuid", default)]
    pub icsuid: String,

    /// Raw ICS string
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ics: Option<String>,

    /// CalDAV href (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub href: Option<String>,

    /// CalDAV etag
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,

    /// Recurrence ID (empty string if not a recurrence exception) — JSON key "rid"
    #[serde(rename = "rid", default)]
    pub recurrence_id: String,

    /// Event status (CONFIRMED/TENTATIVE/CANCELLED)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    /// Recurrence start (unix int) — JSON key "rs"
    #[serde(rename = "rs", default)]
    pub recurrence_start: i64,

    /// Recurrence end (unix int) — JSON key "re"
    #[serde(rename = "re", default)]
    pub recurrence_end: i64,

    // ---- Transient FTS5 search fields (not persisted, set during ICS parsing) ----
    // These are populated by ICS parsing (Phase 9). When non-empty, after_save() writes
    // to EventSearch FTS5 table. Skipped in serialization so they are not stored in
    // the `data` blob or emitted in deltas.

    /// FTS5 search title (from ICS SUMMARY field) — transient, not serialized
    #[serde(skip)]
    pub search_title: String,

    /// FTS5 search description (from ICS DESCRIPTION field) — transient, not serialized
    #[serde(skip)]
    pub search_description: String,

    /// FTS5 search location (from ICS LOCATION field) — transient, not serialized
    #[serde(skip)]
    pub search_location: String,

    /// FTS5 search participants (from ICS ATTENDEE fields) — transient, not serialized
    #[serde(skip)]
    pub search_participants: String,
}

impl MailModel for Event {
    fn table_name() -> &'static str {
        "Event"
    }

    fn id(&self) -> &str {
        &self.id
    }

    fn account_id(&self) -> &str {
        &self.account_id
    }

    fn version(&self) -> i64 {
        self.version
    }

    fn increment_version(&mut self) {
        self.version += 1;
    }

    fn columns_for_query() -> &'static [&'static str] {
        // CRITICAL: No version column for Event
        &[
            "id",
            "data",
            "icsuid",
            "recurrenceId",
            "accountId",
            "etag",
            "calendarId",
            "recurrenceStart",
            "recurrenceEnd",
        ]
    }

    fn bind_to_statement(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        data_json: &str,
    ) -> rusqlite::Result<()> {
        // CRITICAL: Event does NOT bind version — custom column order, no version
        stmt.execute(rusqlite::params![
            self.id,               // ?1 id
            data_json,             // ?2 data
            self.icsuid,           // ?3 icsuid
            self.recurrence_id,    // ?4 recurrenceId
            self.account_id,       // ?5 accountId
            self.etag,             // ?6 etag
            self.calendar_id,      // ?7 calendarId
            self.recurrence_start, // ?8 recurrenceStart
            self.recurrence_end,   // ?9 recurrenceEnd
        ])?;
        Ok(())
    }

    /// Event::after_save — populates EventSearch FTS5 index if search fields are set.
    ///
    /// Search fields (search_title, search_description, search_location, search_participants)
    /// are transient (#[serde(skip)]) — they are set by ICS parsing (Phase 9) and are NOT
    /// stored in the `data` blob. Only populate EventSearch when at least title is non-empty.
    ///
    /// Phase 9 will set these fields from parsed ICS SUMMARY/DESCRIPTION/LOCATION/ATTENDEE.
    fn after_save(&self, conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        if !self.search_title.is_empty() {
            conn.execute(
                "INSERT OR REPLACE INTO EventSearch \
                 (content_id, title, description, location, participants) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    self.id,
                    self.search_title,
                    self.search_description,
                    self.search_location,
                    self.search_participants,
                ],
            )?;
        }
        Ok(())
    }

    /// Event::after_remove — deletes the EventSearch FTS5 row for this event.
    fn after_remove(&self, conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "DELETE FROM EventSearch WHERE content_id = ?1",
            rusqlite::params![self.id],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_event() -> Event {
        Event {
            id: "event1".to_string(),
            account_id: "acc1".to_string(),
            version: 1,
            calendar_id: "cal1".to_string(),
            icsuid: "uid@example.com".to_string(),
            ics: Some("BEGIN:VCALENDAR\r\nEND:VCALENDAR".to_string()),
            href: Some("/caldav/user/calendar/event1.ics".to_string()),
            etag: Some("etag_event1".to_string()),
            recurrence_id: "20240101T100000Z".to_string(),
            status: Some("CONFIRMED".to_string()),
            recurrence_start: 1704067200,
            recurrence_end: 1704070800,
            search_title: String::new(),
            search_description: String::new(),
            search_location: String::new(),
            search_participants: String::new(),
        }
    }

    #[test]
    fn event_serializes_with_correct_json_keys() {
        let event = sample_event();
        let json = serde_json::to_value(&event).unwrap();

        assert!(json.get("id").is_some(), "missing 'id'");
        assert!(json.get("aid").is_some(), "missing 'aid'");
        assert!(json.get("v").is_some(), "missing 'v'");
        assert!(json.get("cid").is_some(), "missing 'cid' (calendar_id)");
        assert!(json.get("icsuid").is_some(), "missing 'icsuid'");
        assert!(json.get("ics").is_some(), "missing 'ics'");
        assert!(json.get("href").is_some(), "missing 'href'");
        assert!(json.get("etag").is_some(), "missing 'etag'");
        assert!(json.get("rid").is_some(), "missing 'rid' (recurrence_id)");
        assert!(json.get("status").is_some(), "missing 'status'");
        assert!(json.get("rs").is_some(), "missing 'rs' (recurrence_start)");
        assert!(json.get("re").is_some(), "missing 're' (recurrence_end)");

        // No snake_case keys
        assert!(json.get("calendar_id").is_none(), "should be 'cid'");
        assert!(json.get("recurrence_id").is_none(), "should be 'rid'");
        assert!(json.get("recurrence_start").is_none(), "should be 'rs'");
        assert!(json.get("recurrence_end").is_none(), "should be 're'");
    }

    #[test]
    fn event_to_json_includes_cls() {
        let event = sample_event();
        let json = event.to_json();
        assert_eq!(json.get("__cls").and_then(|v| v.as_str()), Some("Event"));
    }

    #[test]
    fn event_json_roundtrip() {
        let original = sample_event();
        let json_str = serde_json::to_string(&original).unwrap();
        let deserialized: Event = serde_json::from_str(&json_str).unwrap();
        assert_eq!(original.id, deserialized.id);
        assert_eq!(original.calendar_id, deserialized.calendar_id);
        assert_eq!(original.icsuid, deserialized.icsuid);
        assert_eq!(original.recurrence_start, deserialized.recurrence_start);
    }

    #[test]
    fn event_bind_to_statement_does_not_bind_version() {
        // Event table has no version column in columnsForQuery
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE Event (
                id TEXT PRIMARY KEY,
                data TEXT,
                icsuid TEXT,
                recurrenceId TEXT,
                accountId TEXT,
                etag TEXT,
                calendarId TEXT,
                recurrenceStart INTEGER,
                recurrenceEnd INTEGER
                -- NOTE: no version column
            )"
        ).unwrap();

        let event = sample_event();
        let data_json = serde_json::to_string(&event.to_json()).unwrap();

        let mut stmt = db.prepare(
            "INSERT INTO Event (id, data, icsuid, recurrenceId, accountId, etag, calendarId,
             recurrenceStart, recurrenceEnd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
        ).unwrap();

        // Must succeed without version binding
        event.bind_to_statement(&mut stmt, &data_json).unwrap();

        let (fetched_icsuid, fetched_cal_id): (String, String) = db.query_row(
            "SELECT icsuid, calendarId FROM Event WHERE id = ?1",
            rusqlite::params![event.id],
            |row| Ok((row.get(0)?, row.get(1)?))
        ).unwrap();
        assert_eq!(fetched_icsuid, "uid@example.com");
        assert_eq!(fetched_cal_id, "cal1");
    }

    #[test]
    fn event_optional_fields_omitted_when_none() {
        let mut event = sample_event();
        event.ics = None;
        event.href = None;
        event.etag = None;
        event.status = None;
        let json = serde_json::to_value(&event).unwrap();
        assert!(json.get("ics").is_none());
        assert!(json.get("href").is_none());
        assert!(json.get("etag").is_none());
        assert!(json.get("status").is_none());
    }
}
