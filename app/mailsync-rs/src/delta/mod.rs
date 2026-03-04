// Delta module — delta emission pipeline for mailsync-rs.
//
// The delta pipeline sends database change notifications from the Rust sync engine
// to the TypeScript Electron process via stdout (newline-delimited JSON).
//
// Architecture:
// 1. DeltaStreamItem: A single delta message with type, modelClass, modelJSONs.
//    Coalescing logic is embedded here (upsert by id, key-merge).
// 2. DeltaStream: Arc-shared sender wrapper. Calling emit() queues items to the channel.
// 3. delta_flush_task: Dedicated tokio task that owns stdout. Receives from channel,
//    coalesces into buffer, flushes every 500ms.
//
// References: 05-RESEARCH.md Patterns 2, 3, 4
//
// NOTE: Items are declared pub here so they can be used by modes/sync.rs (Plan 02).
// The #[allow(unused_imports)] suppresses dead_code warnings until sync mode is wired up.

pub mod flush;
pub mod item;
pub mod stream;

#[allow(unused_imports)]
pub use flush::delta_flush_task;
#[allow(unused_imports)]
pub use item::DeltaStreamItem;
#[allow(unused_imports)]
pub use stream::DeltaStream;
