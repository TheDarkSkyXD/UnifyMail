// models module — data model structs for all 13 mail model types.
//
// Each module contains a struct implementing MailModel (or a plain struct for Identity).
// The "fat row" pattern is used: each struct has all JSON fields plus the MailModel
// trait methods for SQLite binding and delta emission.
//
// Module structure:
// - mail_model: MailModel trait definition
// - message: Message struct
// - thread: Thread struct
// - folder: Folder struct (also used as base for Label)
// - label: Label struct
// - contact: Contact struct
// - contact_book: ContactBook struct
// - contact_group: ContactGroup struct
// - calendar: Calendar struct (no version column)
// - event: Event struct (no version column)
// - task_model: Task struct (named task_model to avoid Rust keyword clash)
// - file: File struct
// - identity: Identity plain struct (NOT implementing MailModel)
// - model_plugin_metadata: ModelPluginMetadata join table struct
//
// These types are exported here for use by subsequent plans (store save/find operations).
// Allow dead_code/unused_imports while models are not yet wired into the binary —
// they are used by Phase 6 plans 02 and 03.
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod mail_model;
pub mod message;
pub mod thread;
pub mod folder;
pub mod label;
pub mod contact;

// Task 2 modules — declared here as placeholders, implemented in Task 2
pub mod contact_book;
pub mod contact_group;
pub mod calendar;
pub mod event;
pub mod task_model;
pub mod file;
pub mod identity;
pub mod model_plugin_metadata;

// Re-export the trait and all model types for convenient access
pub use mail_model::MailModel;
pub use message::Message;
pub use thread::Thread;
pub use folder::Folder;
pub use label::Label;
pub use contact::Contact;
pub use contact_book::ContactBook;
pub use contact_group::ContactGroup;
pub use calendar::Calendar;
pub use event::Event;
pub use task_model::Task;
pub use file::File;
pub use identity::Identity;
pub use model_plugin_metadata::ModelPluginMetadata;
