// Store module — SQLite database access for mailsync-rs.
//
// These types are defined here and used by Phase 7+ plans (IMAP sync, task execution).
// Allow dead_code while they are not yet wired into the main sync loop.
#![allow(dead_code)]
#![allow(unused_imports)]

pub mod mail_store;
pub mod migrations;
pub mod task_store;
pub mod transaction;

pub use mail_store::{MailStore, SqlParam};
pub use transaction::MailStoreTransaction;
