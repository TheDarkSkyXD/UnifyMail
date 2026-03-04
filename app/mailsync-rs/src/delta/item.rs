// DeltaStreamItem — represents a single delta message emitted to stdout.
//
// Wire format (matches C++ DeltaStream::dump() exactly):
// { "type": "persist"|"unpersist", "modelClass": "...", "modelJSONs": [...] }
//
// CRITICAL: The serde field names MUST match the TypeScript consumer expectations.
// - "type" (not "delta_type") — mailsync-bridge.ts parses this field
// - "modelClass" (not "model_class") — mailsync-bridge.ts parses this field
// - "modelJSONs" (not "model_jsons") — mailsync-bridge.ts parses this field
//
// Implementation follows 05-RESEARCH.md Pattern 3 (coalescing algorithm).

use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;

/// A single delta message to be emitted to stdout.
///
/// The struct fields use Rust snake_case naming but serialize to the exact
/// camelCase/PascalCase field names required by the TypeScript IPC protocol.
///
/// NOTE: Do NOT use `#[serde(rename_all = "camelCase")]` — it would rename
/// `delta_type` to `deltaType` instead of the required `"type"`.
/// Use per-field explicit renames only.
#[allow(dead_code)]
#[derive(Debug, Clone, Serialize)]
pub struct DeltaStreamItem {
    /// The delta operation: "persist" or "unpersist"
    #[serde(rename = "type")]
    pub delta_type: String,

    /// The model class name (e.g., "Thread", "Message", "ProcessState")
    #[serde(rename = "modelClass")]
    pub model_class: String,

    /// The model JSON objects in this delta batch
    #[serde(rename = "modelJSONs")]
    pub model_jsons: Vec<Value>,

    /// Internal tracking: maps model id -> index in model_jsons for O(1) upsert.
    /// Skipped during serialization — not part of the wire format.
    #[serde(skip)]
    pub id_indexes: IndexMap<String, usize>,
}

#[allow(dead_code)]
impl DeltaStreamItem {
    /// Creates a new DeltaStreamItem and builds the id_indexes map.
    pub fn new(delta_type: &str, model_class: &str, model_jsons: Vec<Value>) -> Self {
        let mut id_indexes = IndexMap::new();
        for (i, json) in model_jsons.iter().enumerate() {
            if let Some(id) = json.get("id").and_then(|v| v.as_str()) {
                id_indexes.insert(id.to_string(), i);
            }
        }
        Self {
            delta_type: delta_type.to_string(),
            model_class: model_class.to_string(),
            model_jsons,
            id_indexes,
        }
    }

    /// Creates a ProcessState delta for the given account.
    ///
    /// ProcessState is the delta that tells Electron the account is "online" or offline.
    /// It is consumed by OnlineStatusStore.onSyncProcessStateReceived() in TypeScript.
    ///
    /// Per 05-RESEARCH.md Code Examples — the shape is:
    /// { "type": "persist", "modelClass": "ProcessState",
    ///   "modelJSONs": [{ "accountId": "...", "id": "...", "connectionError": false }] }
    pub fn process_state(account_id: &str, connection_error: bool) -> Self {
        let model_json = serde_json::json!({
            "accountId": account_id,
            "id": account_id,
            "connectionError": connection_error
        });
        Self::new("persist", "ProcessState", vec![model_json])
    }

    /// Attempts to coalesce another DeltaStreamItem into this one.
    ///
    /// Returns true if merge was possible (same delta_type AND same model_class).
    /// Returns false if types differ — caller should push as a new item.
    ///
    /// Per 05-RESEARCH.md Pattern 3: items of the same type + class are merged
    /// by upserting each JSON from `other` into `self` using `upsert_model_json`.
    pub fn concatenate(&mut self, other: &DeltaStreamItem) -> bool {
        if self.delta_type != other.delta_type || self.model_class != other.model_class {
            return false;
        }
        for item in &other.model_jsons {
            self.upsert_model_json(item.clone());
        }
        true
    }

    /// Upserts a model JSON into this item's model_jsons list.
    ///
    /// Per 05-RESEARCH.md Pattern 3:
    /// - If the model's "id" is already in model_jsons: merge keys.
    ///   New value overwrites existing key, existing keys NOT in new item are preserved.
    /// - If "id" is not found: push new model and track in id_indexes.
    fn upsert_model_json(&mut self, incoming: Value) {
        let incoming_id = incoming
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        if let Some(ref id) = incoming_id {
            if let Some(&existing_idx) = self.id_indexes.get(id) {
                // Key-merge: new values overwrite, old keys not in incoming are preserved
                if let (Some(existing_map), Some(incoming_map)) = (
                    self.model_jsons[existing_idx].as_object_mut(),
                    incoming.as_object(),
                ) {
                    for (k, v) in incoming_map {
                        existing_map.insert(k.clone(), v.clone());
                    }
                }
                return;
            }
        }

        // Not found — push new model and track its index
        let new_idx = self.model_jsons.len();
        if let Some(ref id) = incoming_id {
            self.id_indexes.insert(id.clone(), new_idx);
        }
        self.model_jsons.push(incoming);
    }

    /// Serializes this item to a JSON string using serde (with field renames applied).
    pub fn to_json_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|e| {
            tracing::error!("Failed to serialize DeltaStreamItem: {e}");
            "{}".to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_builds_id_indexes() {
        let item = DeltaStreamItem::new(
            "persist",
            "Thread",
            vec![
                serde_json::json!({"id": "t1", "subject": "Hello"}),
                serde_json::json!({"id": "t2", "subject": "World"}),
            ],
        );
        assert_eq!(item.id_indexes.get("t1"), Some(&0));
        assert_eq!(item.id_indexes.get("t2"), Some(&1));
    }

    #[test]
    fn test_serialize_field_names() {
        let item = DeltaStreamItem::new(
            "persist",
            "Thread",
            vec![serde_json::json!({"id": "t1"})],
        );
        let json_str = item.to_json_string();
        let parsed: serde_json::Value =
            serde_json::from_str(&json_str).expect("Should be valid JSON");

        assert!(parsed.get("type").is_some(), "Must have 'type' field");
        assert!(parsed.get("modelClass").is_some(), "Must have 'modelClass' field");
        assert!(parsed.get("modelJSONs").is_some(), "Must have 'modelJSONs' field");
        assert!(parsed.get("delta_type").is_none(), "Must NOT have 'delta_type' field");
        assert!(parsed.get("model_class").is_none(), "Must NOT have 'model_class' field");
        assert!(parsed.get("model_jsons").is_none(), "Must NOT have 'model_jsons' field");
        assert!(parsed.get("id_indexes").is_none(), "Must NOT serialize 'id_indexes'");
    }

    #[test]
    fn test_process_state_shape() {
        let item = DeltaStreamItem::process_state("acct1", false);
        assert_eq!(item.delta_type, "persist");
        assert_eq!(item.model_class, "ProcessState");
        assert_eq!(item.model_jsons.len(), 1);
        let model = &item.model_jsons[0];
        assert_eq!(model["accountId"], "acct1");
        assert_eq!(model["id"], "acct1");
        assert_eq!(model["connectionError"], false);
    }

    #[test]
    fn test_concatenate_same_type_same_class() {
        let mut item1 = DeltaStreamItem::new(
            "persist",
            "Message",
            vec![serde_json::json!({"id": "m1", "subject": "Hello"})],
        );
        let item2 = DeltaStreamItem::new(
            "persist",
            "Message",
            vec![serde_json::json!({"id": "m2", "subject": "World"})],
        );
        let merged = item1.concatenate(&item2);
        assert!(merged, "Same type and class should concatenate");
        assert_eq!(item1.model_jsons.len(), 2, "Should have 2 models after merge");
    }

    #[test]
    fn test_concatenate_different_type_returns_false() {
        let mut item1 = DeltaStreamItem::new(
            "persist",
            "Message",
            vec![serde_json::json!({"id": "m1"})],
        );
        let item2 = DeltaStreamItem::new(
            "unpersist",
            "Message",
            vec![serde_json::json!({"id": "m2"})],
        );
        let merged = item1.concatenate(&item2);
        assert!(!merged, "Different type should NOT concatenate");
        assert_eq!(item1.model_jsons.len(), 1, "Item should be unchanged");
    }

    #[test]
    fn test_upsert_key_merge() {
        let mut item = DeltaStreamItem::new(
            "persist",
            "Message",
            vec![serde_json::json!({"id": "m1", "subject": "Original", "body": "Keep"})],
        );
        let update = DeltaStreamItem::new(
            "persist",
            "Message",
            vec![serde_json::json!({"id": "m1", "subject": "Updated"})],
        );
        item.concatenate(&update);
        assert_eq!(item.model_jsons.len(), 1, "Same id should result in 1 model");
        assert_eq!(item.model_jsons[0]["subject"], "Updated", "subject updated");
        assert_eq!(item.model_jsons[0]["body"], "Keep", "body preserved");
    }
}
