// MailModel trait — the core contract for all persisted mail data model types.
//
// All model structs (Message, Thread, Folder, Label, Contact, etc.) implement this
// trait. It mirrors the C++ MailModel base class interface from MailModel.h.
//
// Key responsibilities:
// - table_name(): SQLite table name for INSERT/UPDATE/DELETE
// - id(), account_id(), version(): primary key accessors
// - to_json(): serialize to JSON with __cls injection for delta dispatch
// - bind_to_statement(): bind indexed columns to a prepared SQLite statement
// - supports_metadata(): whether ModelPluginMetadata join table is maintained
//
// The to_json() default implementation serializes the struct with serde_json,
// then injects "__cls": Self::table_name() into the resulting object.
// This matches C++ MailModel::toJSON() which calls _data["__cls"] = tableName().

use serde::{Deserialize, Serialize};

/// Core trait for all persistable mail data model types.
///
/// Implementors must provide table metadata, key accessors, JSON serialization,
/// and SQLite statement binding for the indexed columns.
pub trait MailModel:
    Serialize + for<'de> Deserialize<'de> + Clone + std::fmt::Debug + Send + 'static
{
    /// The SQLite table name (e.g., "Message", "Thread").
    /// Must match the C++ tableName() exactly — used in delta dispatch.
    fn table_name() -> &'static str
    where
        Self: Sized;

    /// The model's primary key (id field).
    fn id(&self) -> &str;

    /// The account this model belongs to (aid field).
    fn account_id(&self) -> &str;

    /// Current version counter — incremented on each save.
    fn version(&self) -> i64;

    /// Increment the version counter. Called by MailStore before saving.
    fn increment_version(&mut self);

    /// Serialize to JSON with __cls injection for delta dispatch.
    ///
    /// Default implementation: serialize with serde_json, then inject
    /// "__cls": Self::table_name() into the resulting JSON object.
    /// Matches C++ MailModel::toJSON() behavior.
    fn to_json(&self) -> serde_json::Value
    where
        Self: Sized,
    {
        let mut value = serde_json::to_value(self).expect("Model serialization failed");
        if let Some(obj) = value.as_object_mut() {
            obj.insert(
                "__cls".to_string(),
                serde_json::Value::String(Self::table_name().to_string()),
            );
        }
        value
    }

    /// Serialize for delta dispatch (same as to_json by default).
    /// Models can override to add runtime-computed fields (e.g., Message adds body
    /// and fullSyncComplete/headersSyncComplete conditionally).
    fn to_json_dispatch(&self) -> serde_json::Value
    where
        Self: Sized,
    {
        self.to_json()
    }

    /// The ordered list of columns for INSERT/UPDATE statements.
    /// Matches C++ columnsForQuery() — defines the bind order for bind_to_statement().
    fn columns_for_query() -> &'static [&'static str]
    where
        Self: Sized;

    /// Bind this model's values to a prepared SQLite statement.
    ///
    /// The statement SQL must have positional params matching columns_for_query() order.
    /// `data_json` is the pre-serialized JSON string from to_json() (the `data` column).
    ///
    /// CRITICAL EXCEPTIONS:
    /// - Calendar does NOT bind version (no version column in table)
    /// - Event does NOT bind version (no version column in table)
    ///
    /// All other models bind: id, data, accountId, version, then model-specific indexed cols.
    fn bind_to_statement(
        &self,
        stmt: &mut rusqlite::Statement<'_>,
        data_json: &str,
    ) -> rusqlite::Result<()>;

    /// Whether this model type maintains the ModelPluginMetadata join table.
    /// Returns true for Message and Thread (which support plugin metadata).
    /// Default is false.
    fn supports_metadata() -> bool
    where
        Self: Sized,
    {
        false
    }
}
