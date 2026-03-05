// smtp/ — SMTP send module for mailsync-rs.
//
// Provides:
// - sender.rs: SmtpSender — builds lettre transports with TLS/STARTTLS/clear and
//   password/XOAUTH2 auth; send_message() with 30-second outer timeout
// - mime_builder.rs: build_draft_email() — constructs lettre Message from draft JSON
//   with all MIME variants (plain, HTML+plain, inline CID images, attachments)

pub mod mime_builder;
pub mod sender;
