// smtp/mime_builder.rs — MIME message builder from draft JSON.
//
// Converts a DraftData struct into a properly nested lettre::Message with
// the following MIME structure variants:
//
// 1. Plain text only                → text/plain body
// 2. HTML + plain                   → multipart/alternative (plain + html)
// 3. HTML + plain + inline images   → multipart/alternative (plain + related(html + inline CIDs))
// 4. HTML + plain + attachments     → multipart/mixed (alternative + attachments)
// 5. Full (HTML + inlines + attch.) → multipart/mixed (alternative(plain + related(html+cids)) + attachments)
//
// Inline images are identified by non-None content_id field.
// File bytes are read from local temp paths created by Electron before task dispatch.

use std::fs;

use lettre::message::header::ContentType;
use lettre::message::{Attachment, Body, MultiPart, SinglePart};
use lettre::Message;

use crate::error::SyncError;

// ============================================================================
// Data structs — deserialized from task JSON payload
// ============================================================================

/// A named email address contact (from/to/cc/bcc/reply-to field).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactField {
    /// Display name (may be absent)
    pub name: Option<String>,
    /// Email address
    pub email: String,
}

/// A file to attach or embed inline.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAttachment {
    /// Unique attachment ID
    pub id: String,
    /// Filename shown to recipient
    pub filename: String,
    /// MIME content type (e.g. "image/png", "application/pdf")
    pub content_type: String,
    /// Content-ID for inline CID reference (e.g. "img001"). Non-None = inline image.
    /// HTML body will reference as src="cid:img001".
    pub content_id: Option<String>,
    /// Local filesystem path to read file bytes from.
    pub path: String,
}

/// Complete draft data deserialized from the SendDraftTask JSON payload.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftData {
    /// From address(es) — typically a single address
    pub from: Vec<ContactField>,
    /// Primary recipients
    pub to: Vec<ContactField>,
    /// CC recipients
    #[serde(default)]
    pub cc: Vec<ContactField>,
    /// BCC recipients
    #[serde(default)]
    pub bcc: Vec<ContactField>,
    /// Reply-To override address(es)
    #[serde(default)]
    pub reply_to: Vec<ContactField>,
    /// Email subject line
    pub subject: String,
    /// HTML body content
    pub body: String,
    /// Plain text fallback — derived from body if absent
    pub plain_body: Option<String>,
    /// Message-ID of the email being replied to (for threading)
    pub in_reply_to: Option<String>,
    /// References header chain (space-separated Message-IDs)
    pub references: Option<String>,
    /// Attached files (both inline CID images and regular attachments)
    #[serde(default)]
    pub files: Vec<FileAttachment>,
}

// ============================================================================
// parse_draft_data
// ============================================================================

/// Extracts DraftData from a SendDraftTask's JSON payload.
///
/// The draft data is nested inside the task JSON. This function handles
/// both direct DraftData JSON and the nested task format.
pub fn parse_draft_data(task_json: &serde_json::Value) -> Result<DraftData, SyncError> {
    // The draft data may be at the top level of the task JSON
    // (SendDraftTask stores the full draft as its payload fields)
    serde_json::from_value(task_json.clone()).map_err(|e| {
        SyncError::Json(format!("Failed to parse DraftData: {e}"))
    })
}

// ============================================================================
// build_draft_email
// ============================================================================

/// Builds a lettre Message from a DraftData with the correct MIME structure.
///
/// Chooses nesting based on content availability:
/// - Plain only: no multipart needed
/// - HTML + plain: multipart/alternative
/// - HTML + plain + inline CID images: multipart/alternative (plain + related(html + images))
/// - HTML + plain + regular attachments: multipart/mixed (alternative + attachments)
/// - Full: multipart/mixed (alternative(plain + related(html + inlines)) + attachments)
///
/// File bytes are read synchronously via std::fs::read() — files are local temp files.
pub fn build_draft_email(draft: &DraftData) -> Result<Message, SyncError> {
    // ---- Build message header builder ----
    let mut builder = Message::builder();

    // From
    for contact in &draft.from {
        let mailbox = parse_mailbox(contact)?;
        builder = builder.from(mailbox);
    }

    // To (all recipients)
    for contact in &draft.to {
        let mailbox = parse_mailbox(contact)?;
        builder = builder.to(mailbox);
    }

    // Cc
    for contact in &draft.cc {
        let mailbox = parse_mailbox(contact)?;
        builder = builder.cc(mailbox);
    }

    // Bcc — Bcc recipients are suppressed in MIME headers but sent by transport
    for contact in &draft.bcc {
        let mailbox = parse_mailbox(contact)?;
        builder = builder.bcc(mailbox);
    }

    // Reply-To
    for contact in &draft.reply_to {
        let mailbox = parse_mailbox(contact)?;
        builder = builder.reply_to(mailbox);
    }

    // Subject
    builder = builder.subject(draft.subject.clone());

    // In-Reply-To
    if let Some(ref in_reply_to) = draft.in_reply_to {
        builder = builder.in_reply_to(in_reply_to.clone());
    }

    // References
    if let Some(ref references) = draft.references {
        builder = builder.references(references.clone());
    }

    // ---- Determine MIME structure ----
    let plain_text = draft
        .plain_body
        .clone()
        .unwrap_or_else(|| html_to_plain(&draft.body));

    let html_text = draft.body.clone();
    let has_html = !html_text.is_empty();

    // Split files into inline images (content_id is Some) vs regular attachments
    let inline_images: Vec<&FileAttachment> = draft
        .files
        .iter()
        .filter(|f| f.content_id.is_some())
        .collect();
    let regular_attachments: Vec<&FileAttachment> = draft
        .files
        .iter()
        .filter(|f| f.content_id.is_none())
        .collect();

    let has_inlines = !inline_images.is_empty();
    let has_attachments = !regular_attachments.is_empty();

    // ---- Build MIME body ----
    let message = if !has_html {
        // Case 1: Plain text only
        builder
            .header(ContentType::TEXT_PLAIN)
            .body(plain_text)
            .map_err(|e| SyncError::Unexpected(format!("Failed to build plain message: {e}")))?
    } else if !has_inlines && !has_attachments {
        // Case 2: HTML + plain, no inline images, no attachments
        let multipart = MultiPart::alternative_plain_html(plain_text, html_text);
        builder
            .multipart(multipart)
            .map_err(|e| SyncError::Unexpected(format!("Failed to build alternative message: {e}")))?
    } else if has_inlines && !has_attachments {
        // Case 3: HTML + plain + inline images, no attachments
        let related = build_related_part(html_text, &inline_images)?;
        let alternative = MultiPart::alternative()
            .singlepart(SinglePart::plain(plain_text))
            .multipart(related);
        builder
            .multipart(alternative)
            .map_err(|e| SyncError::Unexpected(format!("Failed to build related message: {e}")))?
    } else if !has_inlines && has_attachments {
        // Case 4: HTML + plain + attachments (no inline images)
        let alternative = MultiPart::alternative_plain_html(plain_text, html_text);
        let mut mixed = MultiPart::mixed().multipart(alternative);
        for attachment in &regular_attachments {
            mixed = mixed.singlepart(build_attachment_part(attachment)?);
        }
        builder
            .multipart(mixed)
            .map_err(|e| SyncError::Unexpected(format!("Failed to build mixed message: {e}")))?
    } else {
        // Case 5: Full — HTML + plain + inline images + regular attachments
        let related = build_related_part(html_text, &inline_images)?;
        let alternative = MultiPart::alternative()
            .singlepart(SinglePart::plain(plain_text))
            .multipart(related);
        let mut mixed = MultiPart::mixed().multipart(alternative);
        for attachment in &regular_attachments {
            mixed = mixed.singlepart(build_attachment_part(attachment)?);
        }
        builder
            .multipart(mixed)
            .map_err(|e| SyncError::Unexpected(format!("Failed to build full mixed message: {e}")))?
    };

    Ok(message)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Parses a ContactField into a lettre Mailbox.
fn parse_mailbox(contact: &ContactField) -> Result<lettre::message::Mailbox, SyncError> {
    let mailbox = if let Some(ref name) = contact.name {
        format!("{} <{}>", name, contact.email)
    } else {
        contact.email.clone()
    };
    mailbox
        .parse::<lettre::message::Mailbox>()
        .map_err(|e| SyncError::Unexpected(format!("Invalid email address '{}': {e}", mailbox)))
}

/// Builds a multipart/related part containing the HTML body and inline CID images.
fn build_related_part(
    html_text: String,
    inline_images: &[&FileAttachment],
) -> Result<MultiPart, SyncError> {
    let mut related = MultiPart::related().singlepart(SinglePart::html(html_text));

    for image in inline_images {
        let bytes = fs::read(&image.path).map_err(|e| {
            SyncError::Io(format!("Failed to read inline image '{}': {e}", image.path))
        })?;
        let content_type: lettre::message::header::ContentType = image
            .content_type
            .parse()
            .map_err(|e| SyncError::Unexpected(format!("Invalid content type '{}': {e:?}", image.content_type)))?;
        let cid = image.content_id.as_deref().unwrap_or("");
        let part = Attachment::new_inline(cid.to_string())
            .body(Body::new(bytes), content_type);
        related = related.singlepart(part);
    }

    Ok(related)
}

/// Builds a singlepart attachment for a regular (non-inline) file.
fn build_attachment_part(file: &FileAttachment) -> Result<SinglePart, SyncError> {
    let bytes = fs::read(&file.path).map_err(|e| {
        SyncError::Io(format!("Failed to read attachment '{}': {e}", file.path))
    })?;
    let content_type: lettre::message::header::ContentType = file
        .content_type
        .parse()
        .map_err(|e| SyncError::Unexpected(format!("Invalid content type '{}': {e:?}", file.content_type)))?;
    Ok(Attachment::new(file.filename.clone()).body(Body::new(bytes), content_type))
}

/// Strips HTML tags from body to produce a plain text fallback.
/// Simple implementation: removes angle-bracket tags and decodes common entities.
fn html_to_plain(html: &str) -> String {
    // Remove script and style blocks first
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => result.push(ch),
            _ => {}
        }
    }
    // Decode common HTML entities
    result
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&nbsp;", " ")
        .trim()
        .to_string()
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // Helper: build minimal DraftData with just plain text
    fn plain_draft() -> DraftData {
        DraftData {
            from: vec![ContactField { name: Some("Alice".to_string()), email: "alice@example.com".to_string() }],
            to: vec![ContactField { name: Some("Bob".to_string()), email: "bob@example.com".to_string() }],
            cc: vec![],
            bcc: vec![],
            reply_to: vec![],
            subject: "Hello".to_string(),
            body: String::new(), // empty = plain text only
            plain_body: Some("Hello, World!".to_string()),
            in_reply_to: None,
            references: None,
            files: vec![],
        }
    }

    // Helper: build DraftData with HTML + plain
    fn html_draft() -> DraftData {
        DraftData {
            from: vec![ContactField { name: None, email: "alice@example.com".to_string() }],
            to: vec![ContactField { name: None, email: "bob@example.com".to_string() }],
            cc: vec![],
            bcc: vec![],
            reply_to: vec![],
            subject: "HTML Email".to_string(),
            body: "<p>Hello, World!</p>".to_string(),
            plain_body: Some("Hello, World!".to_string()),
            in_reply_to: None,
            references: None,
            files: vec![],
        }
    }

    // Helper: build DraftData with inline image
    fn html_with_inline_draft(img_path: &str) -> DraftData {
        let mut draft = html_draft();
        draft.files.push(FileAttachment {
            id: "img1".to_string(),
            filename: "image.png".to_string(),
            content_type: "image/png".to_string(),
            content_id: Some("img001".to_string()),
            path: img_path.to_string(),
        });
        draft
    }

    // Helper: build DraftData with file attachment
    fn html_with_attachment_draft(attach_path: &str) -> DraftData {
        let mut draft = html_draft();
        draft.files.push(FileAttachment {
            id: "file1".to_string(),
            filename: "document.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            content_id: None,
            path: attach_path.to_string(),
        });
        draft
    }

    // Helper: build a temp file with some bytes
    fn make_temp_file(content: &[u8]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(content).unwrap();
        f.flush().unwrap();
        f
    }

    // ---- Test 1: Plain text only ----

    #[test]
    fn plain_text_only_builds_message() {
        let draft = plain_draft();
        let msg = build_draft_email(&draft).expect("Plain text draft should build");
        let raw_bytes = msg.formatted();
        let raw = String::from_utf8_lossy(&raw_bytes);
        assert!(raw.contains("Hello, World!"), "Plain body should appear in message");
        // Should NOT contain multipart
        assert!(!raw.contains("multipart"), "Plain text should not be multipart");
        assert!(raw.contains("text/plain"), "Should have text/plain content type");
    }

    // ---- Test 2: HTML + plain text (multipart/alternative) ----

    #[test]
    fn html_and_plain_builds_alternative() {
        let draft = html_draft();
        let msg = build_draft_email(&draft).expect("HTML+plain draft should build");
        let raw_bytes = msg.formatted();
        let raw = String::from_utf8_lossy(&raw_bytes);
        assert!(raw.contains("multipart/alternative"), "Should be multipart/alternative");
        assert!(raw.contains("text/plain"), "Should have text/plain part");
        assert!(raw.contains("text/html"), "Should have text/html part");
    }

    // ---- Test 3: HTML + plain + inline image (multipart/related inside alternative) ----

    #[test]
    fn html_with_inline_image_builds_related() {
        // Create temp image file
        let tmp_img = make_temp_file(b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR"); // PNG header bytes
        let draft = html_with_inline_draft(tmp_img.path().to_str().unwrap());
        let msg = build_draft_email(&draft).expect("HTML+inline draft should build");
        let raw_bytes = msg.formatted();
        let raw = String::from_utf8_lossy(&raw_bytes);
        assert!(raw.contains("multipart/alternative"), "Should have multipart/alternative");
        assert!(raw.contains("multipart/related"), "Should have multipart/related for inline image");
        assert!(raw.contains("img001"), "Should have Content-ID for inline image");
    }

    // ---- Test 4: HTML + plain + file attachment (multipart/mixed) ----

    #[test]
    fn html_with_attachment_builds_mixed() {
        let tmp_pdf = make_temp_file(b"%PDF-1.4");
        let draft = html_with_attachment_draft(tmp_pdf.path().to_str().unwrap());
        let msg = build_draft_email(&draft).expect("HTML+attachment draft should build");
        let raw_bytes = msg.formatted();
        let raw = String::from_utf8_lossy(&raw_bytes);
        assert!(raw.contains("multipart/mixed"), "Should be multipart/mixed");
        assert!(raw.contains("attachment"), "Should have attachment disposition");
        assert!(raw.contains("document.pdf"), "Should have attachment filename");
    }

    // ---- Test 5: Full draft (HTML + inline images + file attachments) — 3-level nesting ----

    #[test]
    fn full_draft_builds_correct_nesting() {
        let tmp_img = make_temp_file(b"\x89PNG\r\n\x1a\n");
        let tmp_pdf = make_temp_file(b"%PDF-1.4");

        let mut draft = html_draft();
        draft.files.push(FileAttachment {
            id: "img1".to_string(),
            filename: "image.png".to_string(),
            content_type: "image/png".to_string(),
            content_id: Some("img001".to_string()),
            path: tmp_img.path().to_str().unwrap().to_string(),
        });
        draft.files.push(FileAttachment {
            id: "file1".to_string(),
            filename: "document.pdf".to_string(),
            content_type: "application/pdf".to_string(),
            content_id: None,
            path: tmp_pdf.path().to_str().unwrap().to_string(),
        });

        let msg = build_draft_email(&draft).expect("Full draft should build");
        let raw_bytes = msg.formatted();
        let raw = String::from_utf8_lossy(&raw_bytes);

        // All 3 multipart types must be present: mixed -> alternative -> related
        assert!(raw.contains("multipart/mixed"), "Should have multipart/mixed (outermost)");
        assert!(raw.contains("multipart/alternative"), "Should have multipart/alternative");
        assert!(raw.contains("multipart/related"), "Should have multipart/related (innermost)");
        // Inline image CID
        assert!(raw.contains("img001"), "Should have Content-ID for inline image");
        // File attachment
        assert!(raw.contains("document.pdf"), "Should have attachment filename");
    }

    // ---- Test 6: Multiple To, Cc, Bcc recipients ----

    #[test]
    fn multiple_recipients_all_present_in_headers() {
        let draft = DraftData {
            from: vec![ContactField { name: None, email: "alice@example.com".to_string() }],
            to: vec![
                ContactField { name: None, email: "bob@example.com".to_string() },
                ContactField { name: None, email: "charlie@example.com".to_string() },
            ],
            cc: vec![ContactField { name: None, email: "dave@example.com".to_string() }],
            bcc: vec![ContactField { name: None, email: "eve@example.com".to_string() }],
            reply_to: vec![],
            subject: "Multi-recipient".to_string(),
            body: String::new(),
            plain_body: Some("Test".to_string()),
            in_reply_to: None,
            references: None,
            files: vec![],
        };

        let msg = build_draft_email(&draft).expect("Multi-recipient draft should build");
        let raw_bytes = msg.formatted();
        let raw = String::from_utf8_lossy(&raw_bytes);

        assert!(raw.contains("bob@example.com"), "Bob should be in To");
        assert!(raw.contains("charlie@example.com"), "Charlie should be in To");
        assert!(raw.contains("dave@example.com"), "Dave should be in Cc");
        // Bcc is suppressed from headers (SMTP protocol handles delivery separately)
        // but some implementations include it. We just verify the message builds.
    }

    // ---- Test 7: Reply-To and In-Reply-To headers ----

    #[test]
    fn reply_to_and_in_reply_to_headers_set() {
        let draft = DraftData {
            from: vec![ContactField { name: None, email: "alice@example.com".to_string() }],
            to: vec![ContactField { name: None, email: "bob@example.com".to_string() }],
            cc: vec![],
            bcc: vec![],
            reply_to: vec![ContactField { name: None, email: "replies@example.com".to_string() }],
            subject: "Re: Original".to_string(),
            body: String::new(),
            plain_body: Some("Reply body".to_string()),
            in_reply_to: Some("<original-msg-id@example.com>".to_string()),
            references: Some("<original-msg-id@example.com>".to_string()),
            files: vec![],
        };

        let msg = build_draft_email(&draft).expect("Reply draft should build");
        let raw_bytes = msg.formatted();
        let raw = String::from_utf8_lossy(&raw_bytes);

        assert!(raw.contains("Reply-To"), "Should have Reply-To header");
        assert!(raw.contains("replies@example.com"), "Reply-To address should be present");
        assert!(raw.contains("In-Reply-To"), "Should have In-Reply-To header");
        assert!(raw.contains("original-msg-id@example.com"), "In-Reply-To should contain message ID");
    }
}
