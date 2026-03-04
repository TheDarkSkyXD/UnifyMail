// Store module — SQLite database access for mailsync-rs.

pub mod mail_store;
pub mod migrations;

pub use mail_store::MailStore;
