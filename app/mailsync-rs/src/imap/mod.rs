// imap/ — IMAP sync module for mailsync-rs.
//
// This module contains the IMAP background sync worker, session management,
// and mail processing logic.
//
// Sub-modules:
//   session.rs     — IMAP connection/session management (Plans 03, 05)
//   sync_worker.rs — Background sync loop and folder orchestration (Plan 02, 03)
//   mail_processor.rs — Message parsing and database persistence (Plan 04)

pub mod foreground_worker;
pub mod mail_processor;
pub mod session;
pub mod sync_worker;
pub mod task_executor;
