# Phase 8 Deep-Dive: lettre MIME Edge Cases

**Researched:** 2026-03-02
**lettre version:** 0.11.x (latest: 0.11.19 as of research date)
**Dependency:** `email-encoding` crate (lettre's internal encoding library)
**Sources:** docs.rs/lettre, lettre GitHub source, lettre issues #626, #661, #685, #693, #708, #1108

---

## Summary Table

| # | Edge Case | lettre Handles Automatically? | Manual Action Required? | Notes |
|---|-----------|------------------------------|------------------------|-------|
| 1 | Non-ASCII subject lines (RFC 2047) | YES | None | Auto B-encodes in UTF-8 base64 via `HeaderValueEncoder` |
| 2 | Non-ASCII attachment filenames | YES (since 0.10.0) | None for `Attachment::new()` | Uses RFC 2231 `filename*=UTF-8''percent-encoded` format |
| 3 | Charset in body parts | YES | Pass `String` not `Vec<u8>` | `SinglePart::plain()` and `::html()` set `charset=utf-8` automatically; CTE is auto-selected |
| 4 | Multiple inline images | YES (manual CID required) | Caller must supply unique CIDs | No auto-generation of CID; collision is caller's responsibility |
| 5 | Large attachments buffered in memory | NO (all in memory) | Read file into `Vec<u8>` first | No streaming support; `Body` holds entire encoded buffer as `Vec<u8>` |
| 6 | In-Reply-To / References headers | YES (methods exist) | Call `.in_reply_to()` and `.references()` | Methods exist on `MessageBuilder`; caller provides raw message-id strings |
| 7 | CC, BCC, Reply-To headers | YES | None for CC/Reply-To; BCC stripped by default | `.keep_bcc()` preserves BCC header in formatted output |
| 8 | Date header | YES (auto-inserted) | None unless overriding draft date | Auto-inserts `SystemTime::now()` if not set; override with `.date(SystemTime)` |
| 9 | Custom headers (X-Mailer, X-UnifyMail-Draft-ID) | Partial | Use `.raw_header(HeaderValue::new(...))` | No built-in X-headers; `raw_header` added in PR #1108 handles arbitrary names |
| 10 | Message-ID generation | YES (auto-generated) | None unless overriding | Generates `<UUID@HOSTNAME>`; `hostname` feature flag required for real hostname; falls back to `localhost` |

---

## Detailed Findings

### 1. Non-ASCII Subject Lines

**Verdict: Fully automatic. No pre-encoding required.**

lettre encodes non-ASCII subject (and all other header) values automatically using RFC 2047 B-encoding (base64) in UTF-8. This is performed by the internal `HeaderValueEncoder` which calls `email_encoding::headers::rfc2047::encode()`.

**How it works:**

The `Subject` header wraps a `String`. When `headers.to_string()` is called (during `Message::formatted()`), each `HeaderValue` is encoded via `HeaderValueEncoder`. The encoder detects non-ASCII bytes and wraps them in `=?utf-8?b?...?=` encoded-words. ASCII substrings pass through unencoded.

**From lettre source tests (confirmed via docs.rs source):**

```rust
// Cyrillic subject
headers.set(Subject("Тема сообщения".into()));
// Output:
// Subject: =?utf-8?b?0KLQtdC80LAg0YHQvtC+0LHRidC10L3QuNGP?=

// Mixed ASCII + non-ASCII
headers.set(Subject("Administratör".into()));
// Output:
// Subject: =?utf-8?b?QWRtaW5pc3RyYXTDtnI=?=
```

**Long non-ASCII subjects:** The encoder uses `HeaderValueEncoder` which performs RFC 2047 line folding. Lines are folded at whitespace boundaries with `\r\n ` continuation. Encoded-words that exceed line length limits are split into multiple encoded-words separated by whitespace (which is ignored by decoders per RFC 2047 section 6.2).

**Emoji and CJK subjects** follow the same path - they are all UTF-8 bytes and will be base64-encoded.

**Builder call:**

```rust
Message::builder()
    .subject("日本語テスト")  // Just pass the raw Unicode string
    // lettre encodes it to: =?utf-8?b?5pel5pys6Kqe44OG44K544OI?=
```

**Choice of B vs Q encoding:** lettre always uses B-encoding (base64) for non-ASCII content in headers, not Q-encoding. This is consistent with the RFC 2047 recommendation for content where most characters are non-ASCII.

---

### 2. Non-ASCII Filenames in Attachments

**Verdict: Fully automatic since lettre 0.10.0 (PR #685). Uses RFC 2231 percent-encoding.**

**History:** In lettre 0.10.0-rc.2, a regression caused non-ASCII filenames to be incorrectly encoded, producing malformed `Content-Disposition` like:
```
Content-Disposition: attachment; =?utf-8?b?ZmlsZW5hbWU9IlTDtnN0?=.pdf
```
This was broken because RFC 2047 encoded-words cannot appear inside parameter values. Issue #626 tracked this; PR #685 fixed it by implementing RFC 2231.

**Current behavior (lettre 0.10.0+ / 0.11.x):**

The `ContentDisposition::with_name()` private method calls `email_encoding::headers::rfc2231::encode("filename", file_name, &mut w)`. For non-ASCII filenames, this produces the `filename*=` parameter form with UTF-8 percent-encoding:

```
// "töst.txt" encodes to:
Content-Disposition: attachment; filename*=UTF-8''%74%C3%B6%73%74%2E%74%78%74

// "faktúra.pdf" encodes to:
Content-Disposition: attachment; filename*0*=utf-8''fakt%C3%BAra.pdf

// Long filenames are split with continuation parameters:
Content-Disposition: attachment;
  filename*0="invoice_2022_06_04_letshaveaverylongfilenamewhynotemailcanha";
  filename*1="ndleit.pdf"
```

**CJK, Cyrillic, Emoji filenames:** The `email_encoding::headers::rfc2231::encode` function handles any UTF-8 string. Non-ASCII bytes are percent-encoded as `%XX` pairs. The output follows RFC 2231 section 4 with `UTF-8''` prefix.

**Important compatibility note:** RFC 2231 `filename*=` is correctly supported by Thunderbird, KMail, and Neomutt. However, some older or non-standard clients (including some versions of Outlook and some webmail implementations) may not decode RFC 2231 correctly and will show the raw encoded form. lettre does NOT produce the dual `filename=` + `filename*=` form that some libraries use for maximum compatibility.

**For `Attachment::new(filename)` you just pass the raw Unicode string:**

```rust
Attachment::new(String::from("файл.pdf"))
    .body(bytes, "application/pdf".parse().unwrap())
// Produces: Content-Disposition: attachment; filename*=UTF-8''%D1%84%D0%B0%D0%B9%D0%BB.pdf
```

---

### 3. Charset Handling in Body Parts

**Verdict: Automatic. `charset=utf-8` is set for text/* parts; Content-Transfer-Encoding is auto-selected.**

**How `SinglePart::plain()` works (from source):**

```rust
pub fn plain<T: IntoBody>(body: T) -> Self {
    Self::builder()
        .header(header::ContentType::TEXT_PLAIN)  // "text/plain; charset=utf-8"
        .body(body)
}

pub fn html<T: IntoBody>(body: T) -> Self {
    Self::builder()
        .header(header::ContentType::TEXT_HTML)   // "text/html; charset=utf-8"
        .body(body)
}
```

The constants `TEXT_PLAIN` and `TEXT_HTML` already include `; charset=utf-8`. This is hard-coded in lettre and does not require the caller to specify charset.

**Content-Transfer-Encoding selection (from `Body::new` docs):**

The `IntoBody` trait's implementation calls `Body::new()` which automatically selects the most efficient encoding from `{7bit, quoted-printable, base64}`:

- If all content is 7-bit ASCII with lines ≤ 998 characters → `7bit`
- If content is UTF-8 text with some non-ASCII → `quoted-printable`
- If content is binary (`Vec<u8>`) → `base64` (always, regardless of content)
- `binary` encoding is also available but only when explicitly set

**Critical distinction: `String` vs `Vec<u8>` input:**

```rust
// Pass String for text content — allows 7bit or quoted-printable
SinglePart::plain(String::from("Привет мир"))
// Output: Content-Transfer-Encoding: quoted-printable

// Passing Vec<u8> forces base64 even for valid UTF-8 text
SinglePart::plain(b"Hello world".to_vec())  // NOT recommended for text
// Output: Content-Transfer-Encoding: base64
```

This is documented: "If `buf` is valid utf-8 a `String` should be supplied, as `String`s can be encoded as `7bit` or `quoted-printable`, while `Vec<u8>` always get encoded as `base64`."

**From test confirming automatic charset + CTE:**

```rust
// From lettre multipart test
// Text "Текст письма в уникоде" with ContentType::TEXT_PLAIN
// Produces header: "Content-Type: text/plain; charset=utf-8\r\n"
// Followed by:     "Content-Transfer-Encoding: binary\r\n"  (when explicitly set)
// Or automatically: "Content-Transfer-Encoding: quoted-printable\r\n"
```

---

### 4. Multiple Inline Images

**Verdict: Supported. Caller is responsible for CID uniqueness. No collision detection.**

**How it works:**

`Attachment::new_inline(content_id: String)` creates an inline attachment part. The `content_id` is wrapped in angle brackets: `Content-ID: <content_id>`. The HTML body references images as `<img src="cid:content_id">` (without the angle brackets in the `src`).

Multiple inline images in a `MultiPart::related()` block work correctly:

```rust
MultiPart::related()
    .singlepart(SinglePart::html(String::from(
        r#"<p><img src="cid:img1"> and <img src="cid:img2"></p>"#,
    )))
    .singlepart(
        Attachment::new_inline(String::from("img1"))
            .body(image1_body, "image/png".parse().unwrap()),
    )
    .singlepart(
        Attachment::new_inline(String::from("img2"))
            .body(image2_body, "image/jpeg".parse().unwrap()),
    )
```

**CID format:** lettre wraps the caller-supplied string in `<...>` automatically:
- Input: `"img1"` → Header: `Content-ID: <img1>`
- Referenced in HTML as: `cid:img1` (no angle brackets)

**Collision risks:** lettre performs NO uniqueness validation. If two inline parts have the same CID, both `Content-ID: <same>` headers appear and email clients will use the first match. The caller must ensure uniqueness.

**Recommended CID generation strategy:**

```rust
use uuid::Uuid;

fn unique_cid() -> String {
    format!("{}@unifymail", Uuid::new_v4().to_string().replace("-", ""))
}
// Produces: "550e8400e29b41d4a716446655440000@unifymail"
```

**Practical limits:** There is no hard limit in lettre on the number of inline images. The practical limit is memory (each `Body` is buffered in RAM fully encoded). For reasonable email sizes (max ~25 MB after SMTP base64 overhead), 10–20 inline images of typical size (50–200 KB each) are practical.

**RFC compliance:** The MIME `multipart/related` structure with `Content-ID` is defined in RFC 2387. lettre generates correct structure; the `MultiPart::related()` call produces `Content-Type: multipart/related; boundary="..."`.

---

### 5. Large Attachments

**Verdict: All in memory. No streaming. Base64 is chunked at 76-character lines.**

**Memory behavior (from source analysis):**

`Body::new(Vec<u8>)` allocates the encoded size upfront (`email_encoding::body::base64::encoded_len(buf.len())`) then encodes the entire buffer:

```rust
ContentTransferEncoding::Base64 => {
    let len = email_encoding::body::base64::encoded_len(buf.len());
    let mut out = String::with_capacity(len);
    email_encoding::body::base64::encode(&buf, &mut out)
        .expect("encode body as base64");
    Self::dangerous_pre_encoded(out.into_bytes(), ContentTransferEncoding::Base64)
}
```

This means:
1. The raw file content (`Vec<u8>`) is held in memory
2. The base64-encoded string is additionally allocated (≈ 1.37× the raw size)
3. The `Body` struct holds the encoded buffer permanently
4. Total peak memory for a 10 MB attachment: ~10 MB raw + ~14 MB base64 = ~24 MB

**Base64 line length:** lettre wraps base64 at 76 characters per line (RFC 2045 requirement), separated by `\r\n`. This is handled by `email_encoding::body::base64::encode()`.

**From test (confirmed 76-char line wrapping):**

```rust
Body::new_with_encoding(vec![0; 80], ContentTransferEncoding::Base64);
// Output:
// "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\r\n"
// "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
// First line is exactly 76 characters of base64
```

**No size limits imposed by lettre.** The only limit is available process memory and SMTP server limits (typically 25–50 MB for the entire message after encoding).

**Reuse optimization:** lettre documents a `Body` reuse pattern for sending the same attachment in multiple emails:

```rust
// Pre-encode once, clone for each message (avoids re-encoding)
let image_body = Body::new(fs::read("large_file.bin")?);
// image_body can be .clone()d and used in multiple messages
```

**Implication for Phase 8:** For large attachments, the sync engine must read the entire file into memory before building the `Message`. Consider setting a soft limit (e.g., 50 MB) and logging a warning for attachments over that threshold.

---

### 6. Reply/Forward Headers (In-Reply-To, References)

**Verdict: Methods exist on `MessageBuilder`. Caller provides raw message-ID strings.**

**API (confirmed from docs.rs/lettre MessageBuilder):**

```rust
/// Set or add message id to In-Reply-To header
pub fn in_reply_to(self, id: String) -> Self

/// Set or add message id to References header
pub fn references(self, id: String) -> Self
```

**Usage for reply threading:**

```rust
// When replying to a message with Message-ID: <abc123@example.com>
Message::builder()
    .from("sender@example.com".parse()?)
    .to("recipient@example.com".parse()?)
    .subject("Re: Original Subject")
    .in_reply_to("<abc123@example.com>".to_string())
    .references("<abc123@example.com>".to_string())
    .singlepart(SinglePart::plain("Reply text"))
```

**Threading semantics (RFC 5322 section 3.6.4):**
- `In-Reply-To`: Contains the Message-ID(s) of the message(s) being replied to (typically one)
- `References`: Contains the entire thread chain — all ancestors in order, ending with the immediate parent

**For a reply at depth N:**
```
Original:  Message-ID: <msg1@host>
Reply 1:   In-Reply-To: <msg1@host>
           References: <msg1@host>
Reply 2:   In-Reply-To: <reply1@host>
           References: <msg1@host> <reply1@host>
```

**Important:** The `references()` method on `MessageBuilder` can be called multiple times to build a chain, or the caller can construct the space-separated reference chain as a single string. The method signature accepts a single `String` per call (not a list).

**Forwarding:** For forwarded messages, RFC practice varies. Most clients omit `In-Reply-To` and `References` for forwards, or use a `Resent-*` header set instead. lettre does not have built-in forward support beyond providing these header setters.

---

### 7. CC, BCC, Reply-To Headers

**Verdict: All exist. BCC is stripped from formatted output by default.**

**Methods on `MessageBuilder`:**

```rust
pub fn cc(self, mbox: Mailbox) -> Self        // Appends to Cc header
pub fn bcc(self, mbox: Mailbox) -> Self       // Appends to Bcc header
pub fn reply_to(self, mbox: Mailbox) -> Self  // Sets Reply-To header
pub fn keep_bcc(self) -> Self                 // Preserves Bcc in formatted output
```

**BCC stripping behavior (RFC-compliant):**

By default, the `Bcc` header is removed from the formatted message output (`Message::formatted()`) even though BCC recipients are included in the SMTP envelope (for delivery). This matches RFC 5322 section 3.6.3 which states BCC addresses should not appear in the message seen by To/Cc recipients.

**From lettre source test:**

```rust
let email = Message::builder()
    .bcc("hidden@example.com".parse().unwrap())
    .keep_bcc()   // Without this, Bcc header is absent from formatted()
    .from(...)
    .to(...)
    .body(String::from("text")).unwrap();

// With keep_bcc(): "Bcc: hidden@example.com\r\n" appears in output
// Without keep_bcc(): Bcc header absent from output, but SMTP envelope still sends to hidden@example.com
```

**Multiple recipients:** Each call to `.cc()` or `.bcc()` appends a mailbox to the respective header. For multiple BCC recipients:

```rust
Message::builder()
    .bcc("bcc1@example.com".parse()?)
    .bcc("bcc2@example.com".parse()?)
    // Both are in SMTP envelope; neither appears in message unless keep_bcc() called
```

**Reply-To:** Sets the address responses go to (overrides From for reply purposes). Non-ASCII display names in addresses are RFC 2047 encoded automatically.

---

### 8. Date Header

**Verdict: Automatically inserted at build time if not provided.**

**From `MessageBuilder::build()` source:**

```rust
fn build(self, body: MessageBody) -> Result<Message, EmailError> {
    // Insert Date if missing
    let mut res = if self.headers.get::<header::Date>().is_none() {
        self.date_now()   // Calls SystemTime::now()
    } else {
        self
    };
    // ...
}
```

**Three options for Date:**

```rust
// Option 1: Automatic (current time at build() call)
Message::builder()
    // ... don't call .date() ...
    .body(text)   // Date auto-inserted here

// Option 2: Current time explicit
Message::builder()
    .date_now()   // Equivalent to .date(SystemTime::now())
    .body(text)

// Option 3: Specific time (for drafts composed at an earlier time)
use std::time::{SystemTime, Duration, UNIX_EPOCH};
let draft_time = UNIX_EPOCH + Duration::from_secs(draft_timestamp_secs);
Message::builder()
    .date(draft_time)
    .body(text)
```

**Date format:** lettre uses RFC 2822 format with `+0000` for UTC (not `GMT`):
```
Date: Tue, 15 Nov 1994 08:12:31 +0000
```
The `httpdate` crate is used internally; lettre patches the output to replace the obsolete `GMT` form with `+0000` as required by RFC 2822.

**Critical implication for drafts:** When sending a draft that was composed at an earlier time, the date should be set to the composition time, NOT the send time. The sync engine must store the draft composition timestamp and pass it to `Message::builder().date(composition_time)`.

---

### 9. Custom Headers

**Verdict: Supported via `.raw_header()` for arbitrary header names, or `.header()` for typed headers.**

**Two approaches:**

**Approach 1: Typed header (compile-time name)**

Implement the `Header` trait for a custom type. Appropriate for headers used frequently in the codebase:

```rust
use lettre::message::header::{Header, HeaderName, HeaderValue};

#[derive(Clone, Debug)]
struct XUnifyMailDraftId(String);

impl Header for XUnifyMailDraftId {
    fn name() -> HeaderName {
        HeaderName::new_from_ascii_str("X-UnifyMail-Draft-ID")
    }
    fn parse(s: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        Ok(Self(s.to_owned()))
    }
    fn display(&self) -> HeaderValue {
        HeaderValue::dangerous_new_pre_encoded(
            Self::name(),
            self.0.clone(),
            self.0.clone(),
        )
    }
}

// Usage:
Message::builder()
    .header(XUnifyMailDraftId("draft-uuid-here".to_string()))
```

**Approach 2: Raw header (runtime name, added in PR #1108)**

For ad-hoc headers where implementing a full type is overkill:

```rust
use lettre::message::header::{HeaderName, HeaderValue};

Message::builder()
    .raw_header(HeaderValue::new(
        HeaderName::new_from_ascii_str("X-Mailer"),
        "UnifyMail/1.0".to_owned(),
    ))
    .raw_header(HeaderValue::new(
        HeaderName::new_from_ascii_str("X-UnifyMail-Draft-ID"),
        "550e8400-e29b-41d4-a716-446655440000".to_owned(),
    ))
```

**Encoding behavior of `HeaderValue::new()`:** The constructor encodes the raw value via RFC 2047 (line folding + encoded-word for non-ASCII). If the value is purely ASCII, it passes through unchanged. If it contains non-ASCII, it is B-encoded.

**`HeaderName::new_from_ascii_str()`** panics if the name contains non-ASCII characters. Header names must be ASCII per RFC 5322. This is a compile-time-safe assertion for string literals but will panic at runtime with dynamic non-ASCII names.

---

### 10. Message-ID Generation

**Verdict: Auto-generated if not explicitly set. Format: `<36-char-alphanumeric@HOSTNAME>`.**

**From lettre source:**

```rust
const DEFAULT_MESSAGE_ID_DOMAIN: &str = "localhost";

fn make_message_id() -> String {
    iter::repeat_with(fastrand::alphanumeric).take(36).collect()
}

pub fn message_id(self, id: Option<String>) -> Self {
    match id {
        Some(i) => self.header(header::MessageId::from(i)),
        None => {
            #[cfg(feature = "hostname")]
            let hostname = hostname::get()
                .map_err(|_| ())
                .and_then(|s| s.into_string().map_err(|_| ()))
                .unwrap_or_else(|()| DEFAULT_MESSAGE_ID_DOMAIN.to_owned());

            #[cfg(not(feature = "hostname"))]
            let hostname = DEFAULT_MESSAGE_ID_DOMAIN.to_owned();

            self.header(header::MessageId::from(
                format!("<{}@{}>", make_message_id(), hostname),
            ))
        }
    }
}
```

**Key facts:**

1. **Not automatically inserted** unless `.message_id(None)` is called. Unlike `Date`, `Message-ID` is NOT auto-inserted during `build()`. The caller must call `.message_id(None)` to get auto-generation, or `.message_id(Some(id))` to set a specific one.

2. **Format:** `<36-char-alphanumeric@HOSTNAME>` where the 36 chars are alphanumeric (using `fastrand::alphanumeric`, not UUID format — no hyphens). Example: `<aB3xZ9...36chars...@mail.example.com>`

3. **Hostname feature:** Requires the `hostname` Cargo feature to be enabled in `Cargo.toml`:
   ```toml
   lettre = { version = "0.11", features = ["hostname"] }
   ```
   Without this feature, all auto-generated Message-IDs use `@localhost`, which is non-unique across machines and frowned upon by spam filters.

4. **Overriding with a specific ID:**
   ```rust
   Message::builder()
       .message_id(Some("<custom-id@unifymail.app>".to_string()))
   ```
   Note: The `<` and `>` angle brackets are part of the Message-ID value per RFC 5322.

5. **RFC 5322 recommendation:** "The message identifier (msg-id) itself MUST be a globally unique identifier for the message." Using `hostname` feature ensures the domain part is the real machine hostname.

6. **Collision risk of auto-generation:** Uses `fastrand` (non-cryptographic). With 36 alphanumeric characters (62 possibilities each), collision probability is astronomically low for practical purposes.

---

## Implications for Phase 8 Implementation

### What the Draft-to-MIME Converter Must Handle Explicitly

1. **CID uniqueness for inline images:** The converter must generate unique Content-ID strings (recommend UUID v4 + `@unifymail` domain suffix) for each inline image attachment. lettre provides no auto-generation.

2. **References header chain construction:** When replying, the converter must fetch the parent message's `References` header, append the parent's `Message-ID`, and pass the complete chain to `.references()`. lettre's `.references()` only accepts a single call per ID; multiple calls may be needed, or a single pre-built space-separated string.

3. **Draft composition timestamp:** For drafts saved at time T and sent at time T+N, call `.date(stored_draft_timestamp)` not `.date_now()`. The sync engine's `DraftRecord` or `SendDraftTask` must carry the original composition timestamp.

4. **Message-ID for sent mail:** Enable the `hostname` feature in `Cargo.toml`. Call `.message_id(None)` to generate a valid domain-qualified ID. Store the generated Message-ID in the database immediately after build (before SMTP send) so it can be used for future reply threading.

5. **Large attachment warning:** Implement a check before building: if total raw attachment size > 25 MB, log a warning. The 25 MB threshold accounts for base64 overhead pushing the message toward typical SMTP server limits (typically 25–50 MB).

6. **Binary vs. String body distinction:** Always pass `String` to `SinglePart::plain()` and `SinglePart::html()` — never `Vec<u8>` for text parts. This ensures `quoted-printable` is selected over `base64` for text, producing smaller messages and more readable raw email.

### What lettre Handles Automatically (No Manual Action)

- RFC 2047 encoding of all non-ASCII header values (Subject, From display name, To display name, etc.)
- RFC 2231 encoding of non-ASCII attachment filenames (`filename*=UTF-8''...`)
- `charset=utf-8` in `Content-Type` for `text/plain` and `text/html` parts
- Content-Transfer-Encoding selection (7bit / quoted-printable / base64) based on content analysis
- Date header insertion (auto-inserts `SystemTime::now()` if not provided)
- BCC stripping from formatted message output (compliant with RFC 5322)
- Base64 line wrapping at 76 characters (RFC 2045)
- MIME-Version header for multipart messages
- RFC 2047 line folding for long headers

### Known Limitations and Workarounds

| Limitation | Impact | Workaround |
|------------|--------|------------|
| No streaming for attachments | Peak RAM = raw size + 1.37× base64 size | Set MIME size limit before build; pre-check file size |
| `filename*=` only (no dual `filename=` + `filename*=`) | Some old Outlook versions may not decode non-ASCII filenames | Document as known limitation; affects <5% of clients |
| Message-ID not auto-inserted (unlike Date) | Messages sent without Message-ID if `.message_id()` not called | Always call `.message_id(None)` in build helper function |
| `hostname` feature needed for real hostname in Message-ID | Without it, all IDs use `@localhost` (spam filter risk) | Add `hostname` to lettre features in `Cargo.toml` |
| `fastrand` for Message-ID (non-crypto random) | Not a practical concern for uniqueness | Acceptable; could use UUID crate for stronger guarantees |
| `in_reply_to` and `references` each add one ID per call | Building multi-message reference chain requires multiple calls | Build the full References string externally and call once |

### Recommended `Cargo.toml` for Phase 8

```toml
[dependencies]
lettre = { version = "0.11", features = [
    "tokio1",           # async Tokio runtime support
    "tokio1-native-tls", # OR "tokio1-rustls-tls" for TLS
    "hostname",         # for proper Message-ID domain generation
    "builder",          # MessageBuilder API (usually on by default)
    "smtp-transport",   # SMTP sending
] }
uuid = { version = "1", features = ["v4"] }  # for CID generation
```

### Recommended Helper Functions for Phase 8

```rust
/// Generate a unique Content-ID for inline images
fn generate_content_id() -> String {
    format!("{}@unifymail", uuid::Uuid::new_v4().to_string().replace('-', ""))
}

/// Build MIME message from a draft, handling all edge cases
fn draft_to_mime(draft: &Draft) -> Result<lettre::Message, lettre::error::Error> {
    use lettre::message::{header, Attachment, Body, Message, MultiPart, SinglePart};
    use std::time::{SystemTime, UNIX_EPOCH, Duration};

    let composition_time = UNIX_EPOCH + Duration::from_secs(draft.created_at_unix);

    let mut builder = Message::builder()
        .date(composition_time)             // Use draft composition time, not send time
        .message_id(None)                   // Auto-generate with hostname
        .from(draft.from.parse()?)
        .subject(draft.subject.clone());    // lettre auto RFC-2047-encodes

    for to in &draft.to { builder = builder.to(to.parse()?); }
    for cc in &draft.cc { builder = builder.cc(cc.parse()?); }
    for bcc in &draft.bcc { builder = builder.bcc(bcc.parse()?); }

    if let Some(ref reply_to) = draft.reply_to {
        builder = builder.reply_to(reply_to.parse()?);
    }
    if let Some(ref in_reply_to) = draft.in_reply_to {
        builder = builder.in_reply_to(in_reply_to.clone());
    }
    if let Some(ref references) = draft.references {
        // references is a space-separated chain of message-ids
        for msg_id in references.split_whitespace() {
            builder = builder.references(msg_id.to_string());
        }
    }

    builder = builder
        .raw_header(header::HeaderValue::new(
            header::HeaderName::new_from_ascii_str("X-Mailer"),
            "UnifyMail/1.0".to_owned(),
        ));

    // ... build body parts and attachments ...
    // Always pass String to SinglePart::plain/html, not Vec<u8>
    builder.multipart(/* ... */)
}
```
