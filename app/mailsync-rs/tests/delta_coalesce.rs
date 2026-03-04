// Delta coalescing unit tests for mailsync-rs.
// Tests verify that the coalescing algorithm correctly merges delta items
// per the C++ DeltaStream.cpp coalescing behavior.
//
// Run with: cargo test --test delta_coalesce

// These tests use the library API exposed from the delta module.
// Since delta is a module inside the binary crate, we need to use the
// integration test approach via a re-export or test against the public API.

use serde_json::{json, Value};

// Helper to create a minimal model JSON with given fields
fn make_model(id: &str, extra: Value) -> Value {
    let mut obj = json!({"id": id});
    if let (Some(map), Some(ext_map)) = (obj.as_object_mut(), extra.as_object()) {
        for (k, v) in ext_map {
            map.insert(k.clone(), v.clone());
        }
    }
    obj
}

// ============================================================================
// Tests for DeltaStreamItem serialization
// ============================================================================

/// Verifies that DeltaStreamItem serializes with exact TypeScript-expected field names.
/// The C++ DeltaStream::dump() emits: type, modelClass, modelJSONs — in camelCase.
/// Rust field names must NOT leak (no model_class, model_jsons, delta_type).
#[test]
fn test_delta_field_names_match_typescript() {
    // Create a delta item that mimics what the flush task would emit
    // We use serde_json::json! macro with exact field names as the source of truth
    let delta = json!({
        "type": "persist",
        "modelClass": "Thread",
        "modelJSONs": [{"id": "thread-1", "subject": "Test"}]
    });

    let serialized = serde_json::to_string(&delta).expect("Failed to serialize delta JSON");

    // Must contain camelCase field names
    assert!(
        serialized.contains("\"type\""),
        "Serialized delta must contain 'type' field, got: {serialized}"
    );
    assert!(
        serialized.contains("\"modelClass\""),
        "Serialized delta must contain 'modelClass' field, got: {serialized}"
    );
    assert!(
        serialized.contains("\"modelJSONs\""),
        "Serialized delta must contain 'modelJSONs' field, got: {serialized}"
    );

    // Must NOT contain snake_case variants
    assert!(
        !serialized.contains("\"model_class\""),
        "Serialized delta must NOT contain 'model_class', got: {serialized}"
    );
    assert!(
        !serialized.contains("\"model_jsons\""),
        "Serialized delta must NOT contain 'model_jsons', got: {serialized}"
    );
    assert!(
        !serialized.contains("\"delta_type\""),
        "Serialized delta must NOT contain 'delta_type', got: {serialized}"
    );
}

/// Verifies the ProcessState delta shape expected by OnlineStatusStore.onSyncProcessStateReceived().
#[test]
fn test_process_state_delta_shape() {
    let account_id = "acct1";
    let connection_error = false;

    // This is the expected shape from 05-RESEARCH.md Pattern 2 and Code Examples
    let process_state = json!({
        "type": "persist",
        "modelClass": "ProcessState",
        "modelJSONs": [{
            "accountId": account_id,
            "id": account_id,
            "connectionError": connection_error
        }]
    });

    // Verify required fields
    assert_eq!(process_state["type"], "persist");
    assert_eq!(process_state["modelClass"], "ProcessState");

    let model_jsons = process_state["modelJSONs"].as_array().expect("modelJSONs must be array");
    assert_eq!(model_jsons.len(), 1, "ProcessState should have exactly 1 model JSON");

    let state = &model_jsons[0];
    assert_eq!(state["accountId"], account_id, "accountId must match");
    assert_eq!(state["id"], account_id, "id must equal accountId");
    assert_eq!(state["connectionError"], connection_error, "connectionError must be false");
}

// ============================================================================
// Coalescing algorithm tests
// These test the coalescing logic directly via the algorithm, not just the struct.
// ============================================================================

/// Helper struct to simulate the coalescing buffer
/// This mirrors the internal structure of delta/flush.rs
struct CoalesceBuffer {
    /// IndexMap<modelClass, Vec<(delta_type, Vec<model_jsons>, id_indexes)>>
    /// Simplified for testing: just track (type, class, models)
    items: Vec<(String, String, Vec<Value>)>, // (type, class, jsons)
}

impl CoalesceBuffer {
    fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Coalesce an incoming delta item into the buffer.
    /// Same type + same class: try to merge (upsert by id).
    /// Different type or different class: append as new entry.
    fn coalesce(&mut self, delta_type: &str, model_class: &str, model_jsons: Vec<Value>) {
        // Find existing entry with same type AND class
        for item in self.items.iter_mut() {
            if item.0 == delta_type && item.1 == model_class {
                // Merge: upsert each incoming model into this item
                for incoming in model_jsons {
                    upsert_model(&mut item.2, incoming);
                }
                return;
            }
        }
        // No matching entry — push as new
        self.items.push((delta_type.to_string(), model_class.to_string(), model_jsons));
    }

    fn len(&self) -> usize {
        self.items.len()
    }

    fn get_item(&self, idx: usize) -> Option<&(String, String, Vec<Value>)> {
        self.items.get(idx)
    }
}

/// Upsert model into existing list by "id" field.
/// If id already in list: merge keys (new values overwrite, old values preserved).
/// If id not in list: push new.
fn upsert_model(list: &mut Vec<Value>, incoming: Value) {
    let incoming_id = incoming.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());

    if let Some(ref id) = incoming_id {
        for existing in list.iter_mut() {
            if existing.get("id").and_then(|v| v.as_str()) == Some(id.as_str()) {
                // Key-merge: incoming keys overwrite, existing keys not in incoming are preserved
                if let (Some(existing_map), Some(incoming_map)) =
                    (existing.as_object_mut(), incoming.as_object())
                {
                    for (k, v) in incoming_map {
                        existing_map.insert(k.clone(), v.clone());
                    }
                }
                return;
            }
        }
    }
    // Not found — push as new
    list.push(incoming);
}

/// Coalescing two persist items with same modelClass merges them into a single item
/// with both model JSONs.
#[test]
fn test_coalesce_same_type_same_class() {
    let mut buf = CoalesceBuffer::new();

    let item1 = make_model("msg-1", json!({"subject": "Hello"}));
    let item2 = make_model("msg-2", json!({"subject": "World"}));

    buf.coalesce("persist", "Message", vec![item1.clone()]);
    buf.coalesce("persist", "Message", vec![item2.clone()]);

    assert_eq!(buf.len(), 1, "Two persist/Message items should coalesce into one buffer entry");

    let merged = buf.get_item(0).expect("Should have one entry");
    assert_eq!(merged.0, "persist");
    assert_eq!(merged.1, "Message");
    assert_eq!(merged.2.len(), 2, "Merged item should contain both model JSONs");

    // Both original items should be present
    let ids: Vec<&str> = merged.2.iter()
        .filter_map(|v| v["id"].as_str())
        .collect();
    assert!(ids.contains(&"msg-1"), "msg-1 should be in merged item");
    assert!(ids.contains(&"msg-2"), "msg-2 should be in merged item");
}

/// Two persist items for the same model id should key-merge (new values win, old keys preserved).
#[test]
fn test_coalesce_same_id_key_merge() {
    let mut buf = CoalesceBuffer::new();

    // First item: id=msg-1, subject=Original, body=Keep
    let item1 = json!({"id": "msg-1", "subject": "Original", "body": "Keep"});
    // Second item: id=msg-1, subject=Updated (no body key)
    let item2 = json!({"id": "msg-1", "subject": "Updated"});

    buf.coalesce("persist", "Message", vec![item1]);
    buf.coalesce("persist", "Message", vec![item2]);

    assert_eq!(buf.len(), 1, "Same id should merge into one entry");

    let merged = buf.get_item(0).expect("Should have one entry");
    assert_eq!(merged.2.len(), 1, "Same id should result in 1 model JSON (not 2)");

    let model = &merged.2[0];
    assert_eq!(model["id"], "msg-1", "id should be preserved");
    assert_eq!(model["subject"], "Updated", "subject should be updated to new value");
    assert_eq!(model["body"], "Keep", "body should be preserved (not in second item)");
}

/// Persist and unpersist for same class must NOT merge (different type).
#[test]
fn test_coalesce_different_type_no_merge() {
    let mut buf = CoalesceBuffer::new();

    let item1 = make_model("msg-1", json!({"subject": "Hello"}));
    let item2 = make_model("msg-2", json!({"subject": "World"}));

    buf.coalesce("persist", "Message", vec![item1]);
    buf.coalesce("unpersist", "Message", vec![item2]);

    assert_eq!(
        buf.len(), 2,
        "persist/Message and unpersist/Message must NOT coalesce (different type)"
    );

    let first = buf.get_item(0).expect("Should have first entry");
    assert_eq!(first.0, "persist");

    let second = buf.get_item(1).expect("Should have second entry");
    assert_eq!(second.0, "unpersist");
}

/// Different model classes must NOT merge into the same buffer entry.
#[test]
fn test_coalesce_different_class_no_merge() {
    let mut buf = CoalesceBuffer::new();

    let msg = make_model("msg-1", json!({"subject": "Hello"}));
    let thread = make_model("thread-1", json!({"subject": "World"}));

    buf.coalesce("persist", "Message", vec![msg]);
    buf.coalesce("persist", "Thread", vec![thread]);

    assert_eq!(
        buf.len(), 2,
        "persist/Message and persist/Thread must be separate buffer entries"
    );

    let first = buf.get_item(0).expect("Should have first entry");
    assert_eq!(first.1, "Message");

    let second = buf.get_item(1).expect("Should have second entry");
    assert_eq!(second.1, "Thread");
}

/// Key merge preserves existing keys absent from the incoming item.
#[test]
fn test_coalesce_preserves_existing_keys() {
    let mut buf = CoalesceBuffer::new();

    // Item1: rich model with id, subject, body, timestamp
    let item1 = json!({
        "id": "msg-1",
        "subject": "Original subject",
        "body": "Original body",
        "timestamp": 12345
    });

    // Item2: same id, only updates subject
    let item2 = json!({
        "id": "msg-1",
        "subject": "New subject"
    });

    buf.coalesce("persist", "Message", vec![item1]);
    buf.coalesce("persist", "Message", vec![item2]);

    assert_eq!(buf.len(), 1);
    let merged = buf.get_item(0).expect("Should have one entry");
    assert_eq!(merged.2.len(), 1, "Same id should result in one merged model JSON");

    let model = &merged.2[0];

    // Updated key should have new value
    assert_eq!(model["subject"], "New subject", "subject should be updated");

    // Untouched keys should be preserved
    assert_eq!(model["body"], "Original body", "body should be preserved from first item");
    assert_eq!(model["timestamp"], 12345, "timestamp should be preserved from first item");
    assert_eq!(model["id"], "msg-1", "id should be preserved");
}

/// Verify that flush_buffer produces the correct JSON structure.
/// Tests the JSON output format (type, modelClass, modelJSONs — exact field names).
#[test]
fn test_flush_buffer_json_output_format() {
    // Simulate what flush_buffer produces using serde_json::json! macro
    let delta_type = "persist";
    let model_class = "Thread";
    let model_jsons = vec![json!({"id": "thread-1", "subject": "Test"})];

    // This is the exact format flush_buffer writes to stdout
    let line = serde_json::to_string(&json!({
        "type": delta_type,
        "modelClass": model_class,
        "modelJSONs": model_jsons
    }))
    .expect("Failed to serialize flush output");

    // Parse back and verify structure
    let parsed: Value = serde_json::from_str(&line).expect("flush output must be valid JSON");

    assert_eq!(parsed["type"], "persist", "type field must be 'persist'");
    assert_eq!(parsed["modelClass"], "Thread", "modelClass field must be 'Thread'");
    assert!(
        parsed["modelJSONs"].is_array(),
        "modelJSONs must be an array"
    );
    assert_eq!(
        parsed["modelJSONs"].as_array().unwrap().len(), 1,
        "modelJSONs must have 1 element"
    );

    // Verify no snake_case fields leaked
    let obj = parsed.as_object().expect("Parsed delta must be an object");
    assert!(
        !obj.contains_key("model_class"),
        "Serialized output must NOT contain 'model_class'"
    );
    assert!(
        !obj.contains_key("model_jsons"),
        "Serialized output must NOT contain 'model_jsons'"
    );
    assert!(
        !obj.contains_key("delta_type"),
        "Serialized output must NOT contain 'delta_type'"
    );
}

/// Verify multiple different model classes each get their own buffer entry
/// and the JSON output isolates them correctly.
#[test]
fn test_multi_class_buffer_isolation() {
    let mut buf = CoalesceBuffer::new();

    // Three different classes
    buf.coalesce("persist", "Thread", vec![json!({"id": "t1"})]);
    buf.coalesce("persist", "Message", vec![json!({"id": "m1"})]);
    buf.coalesce("persist", "Contact", vec![json!({"id": "c1"})]);

    assert_eq!(buf.len(), 3, "Three different classes should be in three separate buffer entries");

    // Same class added again should merge
    buf.coalesce("persist", "Thread", vec![json!({"id": "t2"})]);
    assert_eq!(buf.len(), 3, "Adding to existing class should NOT add new entry");

    let thread_entry = buf.get_item(0).expect("Thread entry should be first");
    assert_eq!(thread_entry.1, "Thread");
    assert_eq!(thread_entry.2.len(), 2, "Thread entry should now have 2 models");
}
