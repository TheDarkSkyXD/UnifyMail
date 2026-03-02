# Phase 9: CalDAV, CardDAV, and Metadata Workers - Research

**Researched:** 2026-03-02
**Domain:** libdav 0.10.2 CalDAV/CardDAV, icalendar crate, Google People API v1, reqwest long-polling, RFC 6578 sync-collection, RFC 6585 rate limiting
**Confidence:** HIGH (libdav API verified via mirror.whynothugo.nl docs; C++ DAVWorker.cpp, GoogleContactsWorker.cpp, MetadataWorker.cpp, MetadataExpirationWorker.cpp read directly; Google People API verified via official docs; RFC 6578/6585 verified; reqwest 0.13.x confirmed)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| CDAV-01 | CalDAV calendar discovery and enumeration via service discovery | libdav `CalDavClient::new_via_bootstrap()` + `find_calendar_home_set()` + `find_calendars()` documented; C++ principal discovery chain read directly (provider-specific overrides for Gmail, Mail.ru, Yandex, Zoho); VEVENT filter required at discovery time |
| CDAV-02 | CalDAV sync-collection REPORT for incremental calendar sync | libdav `names::SYNC_COLLECTION` + `request()` for raw REPORT; C++ `runForCalendarWithSyncToken()` and `runForCalendar()` (fallback) read in full; sync-token expiry (403/409/410) → empty-token retry → legacy fallback documented |
| CDAV-03 | CalDAV CREATE/UPDATE/DELETE events via iCalendar format | libdav `create_resource()` + `update_resource()` + `delete()` methods confirmed; MIME type `text/calendar; charset=utf-8`; If-Match ETag header for updates |
| CDAV-04 | CalDAV ETag handling with GET-after-PUT fallback for servers that omit ETag on mutation | C++ `writeAndResyncEvent()` does unconditional GET-after-PUT via calendar-multiget REPORT; `update_resource()` returns `Option<String>` for ETag; always do multiget after PUT to guarantee consistency |
| CDAV-05 | Rate limiting with RFC 6585 Retry-After compliance | C++ rate limiting state machine read in full: `Retry-After` header parsed as integer seconds OR HTTP-date; `rateLimitedUntil` absolute wall-clock time; exponential backoff (100ms min, 60s max); 3 consecutive successes halves backoff |
| CRDV-01 | CardDAV contact discovery and enumeration via service discovery | libdav `CardDavClient::new_via_bootstrap()` + `find_addressbook_home_set()` equivalent; C++ DNS SRV via identity server API + .well-known redirect chain documented; discovery cached in-memory across sync cycles |
| CRDV-02 | CardDAV sync-collection REPORT for incremental contact sync | Same libdav raw REPORT pattern as CalDAV; C++ `runForAddressBookWithSyncToken()` read in full; RFC 6578 pagination (507 truncation) documented; 90-item multiget chunks |
| CRDV-03 | CardDAV CREATE/UPDATE/DELETE contacts via vCard format | libdav `create_resource()` + `update_resource()` + `delete()`; MIME type `text/vcard; charset=utf-8`; GET-after-PUT via addressbook-multiget REPORT to refresh ETag and server-side data |
| CRDV-04 | Google People API v1 contacts for Gmail accounts (separate from CardDAV path) | C++ `GoogleContactsWorker.cpp` read in full; endpoint `https://people.googleapis.com/v1/people/me/connections`; sync token (`nextSyncToken`/`syncToken`), pagination (`pageToken`), 2-second per-request debounce; scope `https://www.googleapis.com/auth/contacts` |
| META-01 | Metadata worker with HTTP long-polling from identity server | C++ `MetadataWorker::fetchDeltasBlocking()` streams from `/deltas/{accountId}/streaming?cursor=...`; reqwest 0.13.x `bytes_stream()` replaces curl; low-speed disconnect threshold (1 byte/30s); backoff table [3,3,5,10,20,30,60,120,300,300] |
| META-02 | Metadata expiration worker cleans up stale metadata | C++ `MetadataExpirationWorker::run()` reads `ModelPluginMetadata WHERE expiration <= now`, emits `DELTA_TYPE_METADATA_EXPIRATION` delta, sleeps until next expiration; tokio `notify()` replaces condition_variable |
| META-03 | Plugin metadata sync via SyncbackMetadata task type | C++ `MetadataWorker::applyMetadataJSON()` does `upsertMetadata()` on model if present, or writes to `ModelPluginMetadata` waiting table if model not yet synced; version comparison prevents stale overwrites |
</phase_requirements>

---

## Summary

Phase 9 adds three workers to the Rust mailsync binary: a `DavWorker` (CalDAV + CardDAV), a `GoogleContactsWorker` (Gmail contacts via People API), and a `MetadataWorker` / `MetadataExpirationWorker` pair. The C++ reference implementations for all four workers exist in `app/mailsync/MailSync/` and were read directly — they are the authoritative source.

The key architectural decision already made is to use libdav 0.10.2. However, libdav's documented public API does NOT include a `sync_collection` method. Instead, the library provides the named XML property constants (`names::SYNC_COLLECTION`, `names::SYNC_TOKEN`, `names::SYNC_LEVEL`) plus a raw `request()` escape hatch. The sync-collection REPORT XML must be constructed manually, matching what the C++ does verbatim. libdav handles the HTTP transport, authentication, and connection management, but the REPORT payload and multi-status response parsing is the responsibility of the calling code.

The CalDAV/CardDAV sync design follows two paths: sync-token (RFC 6578, efficient) with automatic fallback to legacy ETag-based sync for non-compliant servers. The server compatibility matrix is significant — Robur, GMX, Zimbra, Posteo, Bedework, Synology, DAViCal, and Nextcloud all have documented quirks. The C++ implementation's `normalizeHref()` function (URL-decode until stable + strip trailing slashes) is a critical correctness requirement for href comparison.

The metadata worker uses HTTP streaming (long-polling): a persistent connection to the identity server's `/deltas/{id}/streaming` endpoint delivers newline-delimited JSON. In Rust, reqwest 0.13.x `bytes_stream()` provides the equivalent of curl's write callback. The metadata expiration worker is a separate tokio task using `tokio::time::sleep_until()` + a `tokio::sync::Notify` for wake-up, replacing the C++ `condition_variable`.

**Primary recommendation:** Use libdav 0.10.2 for HTTP/WebDAV transport and named property constants; construct sync-collection REPORT XML bodies manually matching the C++ reference; use icalendar 0.17.x (with `parser` feature) for ICS parsing; use reqwest 0.13.x for metadata long-polling; follow the C++ implementation as the primary specification for all four workers.

---

## Standard Stack

### Core

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| libdav | 0.10.2 | CalDAV/CardDAV HTTP transport, discovery, resource CRUD, named XML properties | Mandated by STATE.md; replaces ~1,000 lines of PROPFIND/XML; provides `CalDavClient`, `CardDavClient`, `names::*` constants |
| reqwest | 0.13.x | HTTP client for Google People API + metadata long-polling streaming | Binary already uses tokio; reqwest 0.13.x is the current tokio-native async HTTP client; `bytes_stream()` for streaming |
| icalendar | 0.17.6 | Parse and generate RFC 5545 iCalendar (.ics) data | Active maintenance; `contents.parse::<Calendar>()` gives typed Event access; supports RRULE, RECURRENCE-ID, DTSTART, DTEND, UID |
| serde + serde_json | 1.x | JSON serialization for Google People API responses and metadata protocol | Already in binary from Phases 5–8 |
| tokio | 1.x | Async runtime, mpsc channels, `Notify` for expiration wake, `sleep_until` | Already mandated; all workers are tokio tasks |

### Supporting

| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| base64 | 0.22.x | Basic auth header encoding (`user:pass` → base64) | Non-OAuth CalDAV/CardDAV accounts |
| httpdate | 1.0.x | Parse HTTP-date format in Retry-After headers | `Retry-After: Fri, 31 Dec 2024 23:59:59 GMT` format |
| url | 2.x | URL parsing and path manipulation | Replacing C++ `replacePath()` for building full URLs from hrefs |
| anyhow | 1.x | Error type propagation across worker code | Already in binary |

### Alternatives Considered

| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| icalendar 0.17.x | calcard (stalwartlabs) | calcard supports ICS+vCard+JSCalendar in one crate with RRULE expansion; but is less mature than icalendar; icalendar is adequate for Phase 9's read/modify use case |
| icalendar 0.17.x | ical (ical-rs) | ical is a lower-level tokenizer; requires more manual parsing; icalendar provides typed access to Event.uid(), .starts(), .ends() |
| reqwest | hyper directly | reqwest wraps hyper; direct hyper gives more control but is significantly more code |
| manual sync-collection XML | libdav `sync_collection` API | libdav 0.10.2 does NOT expose a `sync_collection()` method; `names::SYNC_COLLECTION` XML property is available; raw `request()` must be used |

**Installation:**
```toml
[dependencies]
libdav = "0.10.2"
reqwest = { version = "0.13", features = ["json", "stream"] }
icalendar = { version = "0.17", features = ["parser"] }
base64 = "0.22"
httpdate = "1.0"
url = "2"
```

---

## Architecture Patterns

### Recommended Module Structure

```
mailsync-rs/src/
├── workers/
│   ├── mod.rs               # Worker spawn and task handle management
│   ├── dav_worker.rs        # DavWorker: CalDAV + CardDAV sync (runs after IMAP workers)
│   ├── google_contacts.rs   # GoogleContactsWorker: People API contacts for Gmail
│   ├── metadata_worker.rs   # MetadataWorker: identity server long-polling
│   └── metadata_expiry.rs   # MetadataExpirationWorker: stale metadata cleanup
├── dav/
│   ├── mod.rs               # DAV type exports
│   ├── client.rs            # libdav client builder + auth header helpers
│   ├── caldav.rs            # CalDAV discovery, sync-collection REPORT, PUT/DELETE
│   ├── carddav.rs           # CardDAV discovery, sync-collection REPORT, PUT/DELETE
│   ├── rate_limit.rs        # RateLimitState: Retry-After parsing, backoff, success tracking
│   └── href_utils.rs        # normalizeHref, replacePath, urlDecode helpers
└── ical/
    ├── mod.rs               # iCalendar parse/serialize wrappers
    └── vcard.rs             # vCard parse/serialize wrappers
```

### Pattern 1: libdav Client Construction

**What:** Build libdav clients with Basic or Bearer auth. libdav handles rustls TLS automatically (no OpenSSL). The `CalDavClient::new_via_bootstrap()` performs the .well-known + PROPFIND principal discovery chain.

**When to use:** Account setup, after receiving `sync-calendar` stdin command.

```rust
// Source: mirror.whynothugo.nl/vdirsyncer/main/libdav/struct.CalDavClient.html
use libdav::{CalDavClient, auth::Auth, dav::WebDavClient};
use http::Uri;

async fn build_caldav_client(
    base_url: &str,
    username: &str,
    password: Option<&str>,
    oauth_token: Option<&str>,
) -> anyhow::Result<CalDavClient<impl hyper::client::connect::Connect>> {
    let auth = match oauth_token {
        Some(token) => Auth::Bearer(token.to_string()),
        None => Auth::Basic {
            username: username.to_string(),
            password: password.unwrap_or("").to_string(),
        },
    };
    let uri: Uri = base_url.parse()?;
    let webdav = WebDavClient::new(uri, auth, /* connector */);
    let client = CalDavClient::new_via_bootstrap(webdav).await?;
    Ok(client)
}
```

**Critical:** For Gmail accounts, the CalDAV host is `apidata.googleusercontent.com` and the principal path is `/caldav/v2/{email}` — do NOT use bootstrap discovery. Set these statically as the C++ does.

### Pattern 2: CalDAV/CardDAV sync-collection REPORT (Manual XML)

**What:** libdav 0.10.2 does not expose a `sync_collection()` method. Use `client.request()` with a manually constructed REPORT body. The `names::SYNC_COLLECTION`, `names::SYNC_TOKEN`, `names::SYNC_LEVEL` constants are available from `libdav::names`.

**When to use:** Every incremental sync cycle (with non-empty sync token) and initial discovery sync (empty token).

```rust
// Source: C++ DAVWorker::runForCalendarWithSyncToken() + RFC 6578
// libdav names constants verified from docs.rs/libdav/latest/libdav/names

fn build_sync_collection_report(sync_token: &str, is_initial: bool) -> String {
    let token_element = if sync_token.is_empty() {
        "<D:sync-token/>".to_string()
    } else {
        format!("<D:sync-token>{}</D:sync-token>", sync_token)
    };

    // Initial sync: request only etags (use multiget for data in chunks)
    // Incremental sync: request full calendar-data (changeset is small)
    let props = if is_initial {
        "<D:getetag/>"
    } else {
        "<D:getetag/><C:calendar-data/>"
    };

    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:sync-collection xmlns:D="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav">
  {token_element}
  <D:sync-level>1</D:sync-level>
  <D:prop>{props}</D:prop>
</D:sync-collection>"#,
        token_element = token_element,
        props = props
    )
}
```

**Key sync-collection behavior to implement:**
1. Send REPORT with `Depth: 0` at the calendar/addressbook collection URL
2. Parse `//D:response` nodes from 207 Multi-Status response
3. `./D:status/text()` as DIRECT child (not `//`) determines deleted (404) vs changed
4. Extract new `//D:sync-token/text()` from response
5. Handle 507 truncation: loop until no 507 status in response
6. On 403/409/410 or `valid-sync-token` in error body: clear token, retry once, then fall back to legacy ETag sync
7. On other errors (404, 405, 406, 500): fall back to legacy ETag sync immediately

### Pattern 3: Legacy ETag-Based CalDAV Sync (Fallback)

**What:** When sync-token is not supported, use a calendar-query REPORT to get all event ETags, diff against local ETags, then multiget new/changed events. This is `runForCalendar()` in the C++.

**When to use:** When `runForCalendarWithSyncToken()` returns false.

```rust
// Source: C++ DAVWorker::runForCalendar() — time-range query + multiget pattern
// IMPORTANT: Always include <comp-filter name="VEVENT"> — many servers fail without it:
// SOGo/Xandikos: return empty results; Nextcloud/Cyrus/Posteo/Robur: throw errors
let calendar_query = format!(
    r#"<c:calendar-query xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop><d:getetag /></d:prop>
  <c:filter>
    <c:comp-filter name="VCALENDAR">
      <c:comp-filter name="VEVENT">
        <c:time-range start="{start}" end="{end}"/>
      </c:comp-filter>
    </c:comp-filter>
  </c:filter>
</c:calendar-query>"#,
    start = range.start_str,  // Format: "20231201T000000Z"
    end = range.end_str
);

// Time range: 12 months past, 18 months future (matching C++ constants)
// Multiget chunks: max 90 hrefs per request (matching C++ chunksOfVector pattern)
```

### Pattern 4: ETag Handling — Always GET-After-PUT

**What:** After every CalDAV PUT or CardDAV PUT, unconditionally perform a calendar-multiget (or addressbook-multiget) REPORT to read back the server's version and obtain the new ETag. This is the C++ `writeAndResyncEvent()` pattern — it does NOT try to use the PUT response ETag because many servers (iCloud, Fastmail, Nextcloud) omit ETag from PUT responses or modify server-side fields.

**When to use:** Every CREATE or UPDATE operation.

```rust
// Source: C++ DAVWorker::writeAndResyncEvent() lines 1990-2034
// Always do multiget after PUT regardless of whether update_resource() returned Some(etag)
async fn write_and_resync_event(
    client: &CalDavClient<C>,
    calendar_url: &str,
    href: &str,
    ics_data: Vec<u8>,
    existing_etag: &str,
) -> anyhow::Result<(String, String)> { // Returns (new_etag, server_ics_data)
    // PUT the event (use update_resource for existing, create_resource for new)
    if existing_etag.is_empty() {
        let _ = client.create_resource(href, ics_data, b"text/calendar; charset=utf-8").await?;
    } else {
        let _ = client.update_resource(href, ics_data, existing_etag, b"text/calendar; charset=utf-8").await?;
    }

    // Always do GET-after-PUT via calendar-multiget REPORT
    // This guarantees ETag consistency even for servers that omit ETag from PUT responses
    let multiget = format!(
        r#"<c:calendar-multiget xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav">
  <d:prop><d:getetag /><c:calendar-data /></d:prop>
  <d:href>{href}</d:href>
</c:calendar-multiget>"#,
        href = href
    );
    let resources = client.get_calendar_resources(calendar_url, [href]).await?;
    // Parse resources[0] for new etag and ics content
    // ...
    todo!()
}
```

**RFC basis:** RFC 4791 §5.3.4: "If a server modifies the data as a result of a PUT request... a strong entity tag MUST NOT be returned in the response." This is why iCloud, Fastmail, and others omit ETag from PUT responses — they modify DTSTART/DTEND on server side.

### Pattern 5: Rate Limiting State Machine

**What:** Per-worker mutable state tracks the rate limit backoff. Applied before every HTTP request. Updated on 429/503 response. Reduced after 3 consecutive successes.

**When to use:** All DAV HTTP requests (CalDAV and CardDAV share the same state struct per C++ design).

```rust
// Source: C++ DAVWorker.hpp + DAVWorker.cpp rate limit section (lines 229-363)
const MAX_BACKOFF_MS: u64 = 60_000;
const MIN_BACKOFF_MS: u64 = 100;

struct RateLimitState {
    backoff_ms: u64,
    consecutive_successes: u32,
    rate_limited_until: Option<tokio::time::Instant>,
}

impl RateLimitState {
    async fn apply_delay(&self) {
        if let Some(until) = self.rate_limited_until {
            let now = tokio::time::Instant::now();
            if until > now {
                tokio::time::sleep_until(until).await;
            }
        }
        if self.backoff_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.backoff_ms)).await;
        }
    }

    fn record_success(&mut self) {
        self.consecutive_successes += 1;
        if self.consecutive_successes >= 3 && self.backoff_ms > 0 {
            self.backoff_ms /= 2;
            self.consecutive_successes = 0;
            if self.backoff_ms < MIN_BACKOFF_MS {
                self.backoff_ms = 0;
            }
        }
    }

    fn record_rate_limit(&mut self, http_code: u16, retry_after: Option<&str>) {
        self.consecutive_successes = 0;
        let retry_seconds = retry_after.and_then(parse_retry_after);
        match retry_seconds {
            Some(secs) if secs > 0 => {
                self.rate_limited_until = Some(
                    tokio::time::Instant::now() + Duration::from_secs(secs as u64)
                );
            }
            _ => {
                self.backoff_ms = if self.backoff_ms == 0 {
                    MIN_BACKOFF_MS
                } else {
                    (self.backoff_ms * 2).min(MAX_BACKOFF_MS)
                };
            }
        }
    }
}

// Retry-After header parsing: try integer seconds first, then HTTP-date
// Source: C++ DAVWorker::parseRetryAfter() + httpdate crate
fn parse_retry_after(value: &str) -> Option<u64> {
    // Try integer seconds
    if let Ok(secs) = value.trim().parse::<u64>() {
        return Some(secs);
    }
    // Try HTTP-date format: "Fri, 31 Dec 2024 23:59:59 GMT"
    if let Ok(system_time) = httpdate::parse_http_date(value) {
        let now = std::time::SystemTime::now();
        if let Ok(duration) = system_time.duration_since(now) {
            return Some(duration.as_secs());
        }
    }
    None
}
```

### Pattern 6: Google People API Contact Sync

**What:** Gmail accounts use the Google People API instead of CardDAV. Uses OAuth2 Bearer token from the account's existing token manager. Sync tokens (7-day TTL) enable incremental sync. 2-second debounce per request (C++ hardcoded; mirrors Google's 90 req/sec quota).

**Endpoint:** `https://people.googleapis.com/v1/people/me/connections`

```rust
// Source: C++ GoogleContactsWorker.cpp lines 27-199
const GOOGLE_PEOPLE_ROOT: &str = "https://people.googleapis.com/v1/";
const PERSON_FIELDS: &str = "emailAddresses,genders,names,nicknames,phoneNumbers,urls,birthdays,addresses,userDefined,relations,occupations,organizations,photos";
const PERSON_UPDATE_FIELDS: &str = "emailAddresses,genders,names,nicknames,phoneNumbers,urls,birthdays,addresses,userDefined,relations,occupations,organizations";

// List connections with sync token
async fn list_connections(
    client: &reqwest::Client,
    access_token: &str,
    sync_token: Option<&str>,
    page_token: Option<&str>,
) -> anyhow::Result<serde_json::Value> {
    let mut url = format!(
        "{}people/me/connections?personFields={}&pageSize=400",
        GOOGLE_PEOPLE_ROOT, PERSON_FIELDS
    );
    if let Some(token) = page_token {
        url.push_str(&format!("&pageToken={}", token));
    }
    if let Some(sync) = sync_token {
        url.push_str(&format!("&syncToken={}", sync));
    } else {
        url.push_str("&requestSyncToken=true");
    }

    // 2-second debounce per request — C++ hardcoded, prevents 90 req/sec quota breach
    tokio::time::sleep(Duration::from_millis(2000)).await;

    let resp = client.get(&url)
        .bearer_auth(access_token)
        .send().await?
        .json::<serde_json::Value>().await?;
    Ok(resp)
}

// Sync token expires after 7 days — detected by error message "Sync token is expired"
// Reset: store "" for syncTokenKey, retry full sync
// Required OAuth2 scope: https://www.googleapis.com/auth/contacts
```

**PATCH for update (not PUT):** The People API uses `PATCH /v1/{resourceName}:updateContact?updatePersonFields=...` not PUT.

**Delete for contacts:** `DELETE /v1/{resourceName}:deleteContact`

**Create contacts:** `POST /v1/people:createContact`

### Pattern 7: Metadata Long-Polling Worker

**What:** Opens a persistent HTTP streaming connection to the identity server's `/deltas/{accountId}/streaming?cursor=...` endpoint. Receives newline-delimited JSON. Each non-empty line is a delta event. Heartbeat is 16 empty newlines every 10 seconds (server keeps connection alive). Disconnect detection via low-speed threshold.

```rust
// Source: C++ MetadataWorker::fetchDeltasBlocking() lines 111-147
// Rust replacement using reqwest bytes_stream()
async fn fetch_deltas_blocking(
    identity_url: &str,
    access_token: &str,
    account_id: &str,
    cursor: &str,
    delta_tx: mpsc::Sender<serde_json::Value>,
) -> anyhow::Result<()> {
    let url = format!(
        "{}/deltas/{}/streaming?cursor={}&p={}&ih={}",
        identity_url, account_id, cursor,
        std::env::consts::OS,  // "windows" | "macos" | "linux"
        urlencoded_imap_host
    );

    // 30-second low-speed timeout: disconnect if < 1 byte in 30 seconds
    // Server sends 16 x "\n" every 10 seconds (heartbeat), so 30 seconds is safe
    let response = client.get(&url)
        .bearer_auth(access_token)
        .timeout(Duration::from_secs(90))  // hard timeout as safety net
        .send().await?;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));

        // Process complete lines (newline-delimited JSON)
        while let Some(pos) = buffer.find('\n') {
            let line = buffer[..pos].trim().to_string();
            buffer = buffer[pos + 1..].to_string();

            if line.len() > 1 {  // Skip heartbeat newlines
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                    let _ = delta_tx.send(json).await;
                }
            }
        }
    }
    Ok(())
}
```

**Backoff table (from C++):** `[3, 3, 5, 10, 20, 30, 60, 120, 300, 300]` seconds. Non-retryable errors call `abort()` in C++; in Rust, stop only the metadata tokio task (do not propagate to IMAP workers).

### Pattern 8: Metadata Expiration Worker

**What:** A separate tokio task that queries `ModelPluginMetadata WHERE expiration <= now`, emits `DELTA_TYPE_METADATA_EXPIRATION` deltas for each expired entry, then sleeps until the next earliest expiration. Can be woken early via `tokio::sync::Notify` when new metadata with expiration is saved.

```rust
// Source: C++ MetadataExpirationWorker::run() (entire file read)
async fn metadata_expiration_task(
    store: Arc<MailStore>,
    account_id: String,
    notifier: Arc<tokio::sync::Notify>,
    delta_tx: mpsc::Sender<DeltaStreamItem>,
) {
    // Initial delay: 15 seconds (plugins may take longer than binary to load)
    tokio::time::sleep(Duration::from_secs(15)).await;

    loop {
        let next_wake = process_expired_metadata(&store, &account_id, &delta_tx).await;

        // Sleep until next expiration or woken by notifier
        tokio::select! {
            _ = tokio::time::sleep_until(next_wake) => {}
            _ = notifier.notified() => {
                // Woken by new metadata save — wait 1 second before processing
                // (transaction may not have committed yet)
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

// Minimum interval after sending expirations: 15 seconds (prevents repeat sending)
// Minimum interval when woken by new metadata: 5 seconds (min undo-send interval)
// Default next wake when no expirations: 2 hours
```

### Pattern 9: href Normalization

**What:** URL hrefs from CalDAV/CardDAV responses are inconsistently encoded across servers. Must normalize before comparison. Matches C++ `normalizeHref()` exactly.

**When to use:** Before storing hrefs locally, before comparing remote hrefs to local hrefs, before using hrefs as map keys.

```rust
// Source: C++ DAVWorker::normalizeHref() lines 192-227
// Provider quirks: Confluence double-encodes ("%2540" vs "%40"), Yahoo/FastMail add trailing slashes,
// some servers return full URLs, others return absolute paths
fn normalize_href(href: &str) -> String {
    // Strip scheme + host if present
    let path = if let Some(pos) = href.find("://") {
        let after_scheme = &href[pos + 3..];
        if let Some(slash) = after_scheme.find('/') {
            &after_scheme[slash..]
        } else {
            href
        }
    } else {
        href
    };

    // URL-decode repeatedly until stable (handles double-encoding)
    let mut result = path.to_string();
    let mut prev = String::new();
    let mut iterations = 0;
    while result != prev && iterations < 5 {
        prev = result.clone();
        result = percent_decode_str(&result).decode_utf8_lossy().into_owned();
        iterations += 1;
    }

    // Strip trailing slashes
    result.trim_end_matches('/').to_string()
}
```

### Anti-Patterns to Avoid

- **Using the PUT response ETag directly without GET-after-PUT:** Servers modify event data server-side (iCloud adjusts DTSTART/DTEND, Fastmail regenerates UID). Always multiget after PUT.
- **Using `//D:status` instead of `./D:status` for deletion detection:** RFC 6578 says deleted resources have `<D:status>` as DIRECT child of `<D:response>`. Using `//` instead of `./` misidentifies propstat-level 404s (property not found) as deletions — Google Calendar specifically triggers this bug.
- **Comparing hrefs without normalization:** FastMail returns `%40` encoded hrefs; local contacts stored with decoded `@`. Comparison fails silently.
- **Assuming sync-token support:** Robur and GMX have NO sync-token support; Zimbra/Posteo return HTTP errors instead of declining gracefully. Must implement fallback.
- **Crashing on CardDAV 404/405/406 during discovery:** Many servers (especially non-GroupDAV servers) return these when CardDAV is not configured. Treat as "CardDAV not supported" and skip contact sync.
- **Not including `<comp-filter name="VEVENT">`:** SOGo, Xandikos, Nextcloud, Cyrus, Posteo, Robur all misbehave when comp-filter is omitted from calendar-query REPORT.
- **Long-polling metadata in the same task as IMAP sync:** Non-retryable metadata errors must stop only the metadata task. IMAP sync continues independently.
- **Using provider-specific hostname for Gmail CalDAV:** Gmail uses `apidata.googleusercontent.com` not the user's IMAP host. Set statically, do not use bootstrap discovery.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| WebDAV HTTP requests (PROPFIND, REPORT, PUT, DELETE) | Custom HTTP + XML client | libdav 0.10.2 | Authentication, rustls TLS, named XML property constants, typed responses |
| iCalendar parsing from .ics strings | Custom VCALENDAR tokenizer | icalendar 0.17.x with `parser` feature | RFC 5545 is complex; RRULE, RECURRENCE-ID, timezone handling all built in |
| Retry-After HTTP-date parsing | Custom date parser | httpdate 1.0.x | Handles RFC 7231 HTTP-date format; handles all edge cases |
| OAuth2 Bearer token management | Custom token refresh | Use token manager from Phase 7 | OAuth2 token management already implemented; re-use XOAuth2TokenManager equivalent |
| sync-token state persistence | In-memory only | tokio-rusqlite MailStore key-value store | sync-tokens and cursors must survive process restarts; C++ uses `store->saveKeyValue()` / `store->getKeyValue()` |
| Google People API JSON deserialization | Manual JSON traversal | serde_json + typed structs | `personFields` returns polymorphic JSON; serde handles it cleanly |

**Key insight:** The sync-collection REPORT XML must be hand-rolled because libdav 0.10.2 does not expose a `sync_collection()` method — but libdav handles ALL the HTTP-level complexity (auth, TLS, connection reuse). The XML construction and multi-status response parsing is straightforward; the hard part was already done by the C++ team.

---

## Common Pitfalls

### Pitfall 1: libdav Has No sync_collection Method

**What goes wrong:** Developer assumes libdav abstracts sync-collection REPORT and looks for `client.sync_collection()` — it does not exist.
**Why it happens:** libdav 0.10.2 exposes named XML property constants (`names::SYNC_COLLECTION`, `names::SYNC_TOKEN`) but does not implement the full RFC 6578 sync-collection workflow as a method. The `request()` escape hatch on WebDavClient must be used with manually constructed XML.
**How to avoid:** Use `libdav::names::SYNC_COLLECTION` as documentation only. Build the REPORT XML string manually matching the C++ `runForCalendarWithSyncToken()` body. Use `client.request()` for the raw HTTP call.
**Warning signs:** Looking for `CalDavClient::sync_collection` in docs — it does not exist.

### Pitfall 2: RFC 6578 Deletion Detection — Direct vs. Descendant Status

**What goes wrong:** Events that were moved or created on server are incorrectly treated as deleted. Google Calendar specifically triggers this.
**Why it happens:** Using XPath `//D:status` (any descendant) instead of `./D:status` (direct child of `<D:response>`). When `<D:propstat>` contains a `<D:status>404 Not Found</D:status>` meaning "property not available", it is misread as "resource deleted."
**How to avoid:** Always use `./D:status` (direct child) for deletion detection in sync-collection responses. Use `//D:getetag` and `//D:propstat/D:prop/...` for property extraction.
**Warning signs:** Events in Google Calendar disappear locally then re-appear on next full sync.

### Pitfall 3: ETag Missing After PUT — iCloud, Fastmail, Nextcloud

**What goes wrong:** After updating an event, the local ETag is wrong. Next PUT with `If-Match: wrong-etag` returns 412 Precondition Failed.
**Why it happens:** RFC 4791 §5.3.4 says servers MAY omit ETag from PUT response if they modify the data server-side. iCloud, Fastmail, and Nextcloud all do this. Using `update_resource()` → `Option<String>::None` and assuming the old ETag is still valid is incorrect.
**How to avoid:** Always perform GET-after-PUT via calendar-multiget REPORT, regardless of whether `update_resource()` returned `Some(etag)`. The C++ `writeAndResyncEvent()` does this unconditionally (lines 1990-2034).
**Warning signs:** 412 errors on subsequent edits of events that were previously PUT.

### Pitfall 4: sync-token Expiry Handling — One Retry, Not Infinite

**What goes wrong:** After token expires, code enters infinite retry loop.
**Why it happens:** Token expiry (403/409/410 or "valid-sync-token" in error body) triggers a retry with empty token. If the second attempt also fails (server genuinely doesn't support sync-collection), must fall back to legacy sync — NOT retry again.
**How to avoid:** `retryCount` parameter, `maxRetries = 1`. On second failure or initial empty-token failure, return `false` to trigger `runForCalendar()` fallback. Match C++ `runForCalendarWithSyncToken()` exactly.
**Warning signs:** Spinning on token expiry for servers like Zimbra that return errors even for empty-token requests.

### Pitfall 5: Gmail CalDAV Uses Hardcoded Host — No Bootstrap Discovery

**What goes wrong:** Gmail CalDAV bootstrap discovery fails or hits the wrong server.
**Why it happens:** Gmail does NOT use SRV records for its CalDAV endpoint. The C++ constructor hardcodes: `calHost = "apidata.googleusercontent.com"` and `calPrincipal = "/caldav/v2/{email}"`. Bootstrap discovery must be skipped.
**How to avoid:** Check `account.provider == "gmail"` before calling `CalDavClient::new_via_bootstrap()`. For Gmail, use `CalDavClient::new(webdav)` with the hardcoded base URL and principal path.
**Warning signs:** Gmail CalDAV connection timeouts or 404 errors during initial discovery.

### Pitfall 6: VEVENT comp-filter Required in Legacy Calendar Query

**What goes wrong:** calendar-query REPORT returns empty or errors on SOGo, Nextcloud, Cyrus, Posteo, Robur.
**Why it happens:** RFC 4791 makes comp-filter optional, but multiple server implementations fail without it. The C++ comment documents this explicitly (lines 1320-1343).
**How to avoid:** Always include `<c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT">...</c:comp-filter></c:comp-filter>` in all calendar-query REPORTs. Do not make comp-filter conditional.
**Warning signs:** Empty event lists from non-Google servers on initial sync.

### Pitfall 7: CardDAV Discovery via Identity Server API (Not Local DNS)

**What goes wrong:** CardDAV discovery fails to resolve DNS SRV records on some platforms.
**Why it happens:** The C++ does NOT do local DNS SRV lookup. Instead, it calls the identity server API `/api/resolve-dav-hosts` which does the DNS resolution server-side. The comment explains: "On Win it's a pain and on Linux it generates a binary that is bound to a specific version of glibc."
**How to avoid:** Use the identity server `/api/resolve-dav-hosts` POST API for domain → CardDAV host resolution. Only fall back to local DNS if the identity server API is unavailable.
**Warning signs:** CardDAV DNS SRV crate dependency on platform-specific resolvers.

### Pitfall 8: Metadata Long-Poll — Non-Retryable Errors Must Not Affect IMAP

**What goes wrong:** Metadata worker error cascades and kills the entire binary process.
**Why it happens:** The C++ calls `abort()` on non-retryable SyncException. In Rust, panicking in a tokio task causes the runtime to shut down if not caught.
**How to avoid:** Wrap metadata worker in `tokio::spawn()`. Catch `Err` from the task. Non-retryable errors log and exit the metadata task cleanly via `break` or `return`. Do NOT propagate to IMAP worker tasks. The metadata worker is isolated via its own tokio task handle.
**Warning signs:** IMAP sync stops when identity server is unreachable.

### Pitfall 9: Google People API Sync Token Expires After 7 Days

**What goes wrong:** Incremental contact sync silently returns stale data or errors after 7 days without a full sync.
**Why it happens:** The Google People API `nextSyncToken` expires after 7 days. The error message contains "Sync token is expired." The C++ catches this specific string in the SyncException debuginfo and clears the stored sync token (then retries with full sync).
**How to avoid:** When Google People API returns an error containing "Sync token is expired," clear the stored `gsynctoken-contacts-{accountId}` key-value from the MailStore and retry with a full sync (empty syncToken parameter + `requestSyncToken=true`).
**Warning signs:** Google contacts stop syncing after 7 days of account inactivity.

---

## Code Examples

Verified patterns from C++ source and official documentation:

### CalDAV Discovery Chain (Non-Gmail)

```rust
// Source: C++ DAVWorker constructor + resolveAddressBook() pattern
// 1. Identity server resolves DNS SRV: POST /api/resolve-dav-hosts {"domain": "...", "imapHost": "..."}
// 2. .well-known redirect: GET https://{host}/.well-known/caldav (follow redirect)
// 3. Bootstrap discovery: CalDavClient::new_via_bootstrap() does PROPFIND for current-user-principal
// 4. Calendar home set: find_calendar_home_set(&principal_uri)
// 5. Calendar list: find_calendars(&home_set_uri) -> Vec<FoundCollection>
// 6. Filter by supported-calendar-component-set containing VEVENT
```

### Provider-Specific Host Overrides

```rust
// Source: C++ DAVWorker constructor lines 375-399
fn get_caldav_host(account: &Account) -> Option<(String, String)> {
    match account.provider.as_str() {
        "gmail" => Some((
            "apidata.googleusercontent.com".to_string(),
            format!("/caldav/v2/{}", account.email_address),
        )),
        _ if account.imap_host.contains("imap.mail.ru") => {
            Some(("calendar.mail.ru".to_string(), "discover".to_string()))
        }
        _ if account.imap_host.contains("imap.yandex.com") => {
            Some(("yandex.ru".to_string(), "discover".to_string()))
        }
        _ if account.imap_host.contains("securemail.a1.net") => {
            Some(("caldav.a1.net".to_string(), "discover".to_string()))
        }
        _ if account.imap_host.contains("imap.zoho.com") => {
            Some(("calendar.zoho.com".to_string(), "discover".to_string()))
        }
        _ => None,  // Use bootstrap discovery
    }
}
```

### sync-collection Token Expiry Detection

```rust
// Source: C++ DAVWorker::runForCalendarWithSyncToken() lines 1584-1605
// Token expiry conditions from the C++:
let is_token_expired = error.code == 403
    || error.code == 409
    || error.code == 410
    || error.body.contains("valid-sync-token");

if is_token_expired {
    if sync_token.is_empty() || retry_count >= 1 {
        // Fall back to legacy ETag sync
        return Ok(false);
    }
    // Clear token, retry once with empty token
    calendar.set_sync_token("");
    store.save(&calendar).await?;
    return run_for_calendar_with_sync_token(calendar, retry_count + 1).await;
}
```

### Metadata Delta Protocol (Identity Server Wire Format)

```json
// Source: C++ MetadataWorker::onDelta() — what the streaming endpoint sends
// Heartbeat: "\n" (16 bytes every 10 seconds, ignored by pos > 1 check)
{"object":"metadata","cursor":"12345","attributes":{"pluginId":"...","objectType":"Thread","objectId":"...","accountId":"...","version":1,"value":{...},"expiration":1735689600}}
```

### Metadata Application Logic

```rust
// Source: C++ MetadataWorker::applyMetadataJSON() lines 192-220
async fn apply_metadata_json(store: &MailStore, metadata: &MetadataJSON) {
    // 1. Find the associated model (Thread or Message)
    let model = store.find_generic(&metadata.object_type, &metadata.object_id).await;

    match model {
        Some(mut m) => {
            // 2. Attach metadata if version is newer than stored
            if m.upsert_metadata(&metadata.plugin_id, &metadata.value, metadata.version) > 0 {
                store.save(&m).await;
            }
            // else: local model has >= version, ignore
        }
        None => {
            // 3. Model not yet synced — save to waiting table (ModelPluginMetadata)
            // When model arrives via IMAP sync, it picks up waiting metadata
            store.save_detached_plugin_metadata(metadata).await;
        }
    }
}
```

### Google People API Pagination Pattern

```rust
// Source: C++ GoogleContactsWorker::paginateGoogleCollection() lines 152-199
async fn paginate_google_collection<F: Fn(serde_json::Value)>(
    client: &reqwest::Client,
    url_root: &str,
    access_token: &str,
    sync_token_key: &str,
    store: &MailStore,
    yield_block: F,
) -> anyhow::Result<()> {
    let mut sync_token = store.get_key_value(sync_token_key).await;
    let mut next_page_token = String::new();
    let mut next_sync_token = String::new();

    loop {
        // 2-second debounce (C++ hardcoded for 90 req/sec quota)
        tokio::time::sleep(Duration::from_millis(2000)).await;

        let mut url = url_root.to_string();
        if !next_page_token.is_empty() {
            url.push_str(&format!("&pageToken={}", next_page_token));
        }
        if !sync_token.is_empty() {
            url.push_str(&format!("&syncToken={}", sync_token));
        } else if url.contains("connections") {
            url.push_str("&requestSyncToken=true");
        }

        let json = client.get(&url).bearer_auth(access_token)
            .send().await?.json::<serde_json::Value>().await
            .map_err(|e| {
                // Detect expired sync token
                if e.to_string().contains("Sync token is expired") {
                    store.save_key_value(sync_token_key, "");
                }
                e
            })?;

        if let Some(token) = json["nextSyncToken"].as_str() {
            next_sync_token = token.to_string();
        }
        next_page_token = json["nextPageToken"].as_str().unwrap_or("END").to_string();

        yield_block(json);

        if next_page_token == "END" { break; }
    }

    if !next_sync_token.is_empty() {
        store.save_key_value(sync_token_key, &next_sync_token).await;
    }
    Ok(())
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| libetpan / curl for WebDAV requests | libdav 0.10.2 with rustls | 2024 (project decision) | Eliminates ~1,000 lines of PROPFIND/XML hand-rolling; native async |
| Contacts API (deprecated) | Google People API v1 | Google migration 2022 | Old Contacts API shut down; People API is the current path |
| vCard 3.0 only | vCard 3.0 + 4.0 (group KIND field) | RFC 6350 (2011) but adoption recent | iCloud uses vCard 4.0 `KIND:group` for groups; legacy uses `X-VCARD3-MEMBER` |
| Polling for metadata deltas | HTTP long-polling streaming | Mailspring design | Single persistent connection; server sends heartbeat newlines every 10 seconds |
| Thread.sleep in rate limiting | tokio::time::sleep_until (async) | Rust rewrite | Non-blocking rate limit delay; IMAP worker can continue on other accounts |

**Deprecated/outdated:**
- Google Contacts API (v3): Shut down. ALL Gmail contact operations must use People API v1 at `people.googleapis.com`.
- vCard 2.1: Not supported by any modern server. Only handle 3.0 and 4.0.
- CalDAV without comp-filter: Worked on old Apple CalendarServer; fails on most modern implementations.

---

## Server Compatibility Matrix (CalDAV/CardDAV)

This is the research-flagged area from STATE.md. Key findings from the C++ codebase comments and python-caldav project documentation:

| Server | ETag on PUT | sync-token | Quirks |
|--------|-------------|------------|--------|
| Google Calendar | No | Yes | Empty body bug: sometimes returns event with empty calendar-data; skip these. `./D:status` critical for deletion detection. |
| iCloud | No | Yes | Regression issues. Uses CalendarServer with known bugs. vCard 4.0 `KIND:group`. |
| Nextcloud | Sometimes | Yes (fixed PR #44130) | Was buggy with deleted objects in sync reports (old versions). comp-filter required. |
| Exchange Online | Yes | Limited | MS Graph API preferred over CalDAV; CalDAV support via EWS adapter is fragile |
| Fastmail | No | Yes | May reassign contact ID/UID on POST; check `if (contact.id() != serverside.id())` pattern |
| Radicale | Sometimes | Yes | Fails on open-ended time-range searches. Always provide both start AND end bounds. |
| Zimbra | No | Errors | Returns HTTP errors on sync-collection instead of 404/405. Must fall back to legacy sync. |
| Posteo | No | Errors | Same as Zimbra: errors instead of graceful decline. |
| Robur / GMX | No | No | No sync-token support at all. Always legacy ETag sync. |
| SOGo | No | Second-precision | Time-based tokens; broken time-range searches. Client-side `eventOverlapsRange()` validation required. |
| Synology / DAViCal | No | Unreliable | Unpredictable token behavior. Fall back to legacy on any error. |
| Bedework | No | Yes (excess data) | Returns excess content outside requested time-range. Client-side range validation required. |
| Baikal | Sometimes | Yes | HTTP/2 multiplexing can cause auth failures with nginx Docker image. Retry with HTTP/1.1 if 401 persists. |
| Yandex | No | Partial | Uses `yandex.ru` as CalDAV host, not IMAP host. |

**Key compatibility rules derived from the matrix:**
1. Always do GET-after-PUT (never trust PUT response ETag)
2. Always include `comp-filter name="VEVENT"` in calendar-query REPORT
3. Try sync-token first, fall back on any error (not just 404/405)
4. Always provide both start AND end bounds in time-range filters
5. Validate `eventOverlapsRange()` client-side for sync-token responses (SOGo/Bedework)
6. Normalize hrefs with repeated URL-decode before comparison

---

## Open Questions

1. **libdav 0.10.2 exact CardDavClient method list**
   - What we know: `CardDavClient::new_via_bootstrap()`, `find_addressbooks()` equivalent via `find_collections()`, `create_resource()`, `update_resource()`, `delete()` confirmed via WebDavClient Deref.
   - What's unclear: Whether `CardDavClient` has `get_address_book_resources()` equivalent to `CalDavClient::get_calendar_resources()`. The mirror docs showed only the CalDavClient.
   - Recommendation: Check `CardDavClient` docs specifically during implementation. If missing, use `client.request()` for addressbook-multiget REPORT manually.

2. **Identity Server API Stability for /api/resolve-dav-hosts**
   - What we know: C++ calls `PerformIdentityRequest("/api/resolve-dav-hosts", "POST", payload)` to resolve CardDAV host from domain + imapHost. This is a Mailspring identity server API.
   - What's unclear: Whether this endpoint is documented, stable, and accessible from the Rust binary with the same credentials as the metadata worker.
   - Recommendation: Verify endpoint availability against the identity server before Phase 9 implementation. If unavailable, implement local DNS SRV lookup as fallback using the `trust-dns-resolver` or `hickory-resolver` crate.

3. **reqwest 0.13.x Streaming Timeout Behavior**
   - What we know: The C++ uses `CURLOPT_LOW_SPEED_LIMIT=1` and `CURLOPT_LOW_SPEED_TIME=30` to disconnect if < 1 byte/30 seconds. reqwest 0.13.x does not have a direct low-speed timeout.
   - What's unclear: How to replicate the low-speed timeout with `bytes_stream()`. Options: tokio::time::timeout per chunk, or a background task that monitors idle duration.
   - Recommendation: Use `tokio::time::timeout(Duration::from_secs(35), stream.next())` per chunk as the low-speed equivalent. If `None` or timeout fires, reconnect.

4. **icalendar 0.17.x RECURRENCE-ID Parsing**
   - What we know: C++ reads `icsEvent->RecurrenceId` for recurrence exception detection. icalendar 0.17.x supports RECURRENCE-ID but parsing behavior for different DATE vs DATE-TIME formats may vary.
   - What's unclear: Whether icalendar 0.17.x correctly exposes RECURRENCE-ID on exception events when the ICS file contains both master and exception VEVENTs.
   - Recommendation: Test against a real recurring event ICS from Google Calendar (which uses DATE-TIME) and iCloud (which uses DATE) before finalizing the Event model update logic.

---

## Validation Architecture

> `workflow.nyquist_validation` is not set in .planning/config.json — skip this section.

---

## Sources

### Primary (HIGH confidence)

- C++ source: `app/mailsync/MailSync/DAVWorker.cpp` — full CalDAV/CardDAV sync implementation (2,065 lines read)
- C++ source: `app/mailsync/MailSync/DAVWorker.hpp` — DAVWorker class interface
- C++ source: `app/mailsync/MailSync/GoogleContactsWorker.cpp` — full Google People API contact sync
- C++ source: `app/mailsync/MailSync/MetadataWorker.cpp` — metadata long-polling implementation
- C++ source: `app/mailsync/MailSync/MetadataExpirationWorker.cpp` — metadata expiration cleanup
- C++ source: `app/mailsync/MailSync/DAVUtils.cpp` — vCard group helper utilities
- [libdav CalDavClient methods](https://mirror.whynothugo.nl/vdirsyncer/main/libdav/struct.CalDavClient.html) — all public method signatures confirmed
- [libdav WebDavClient methods](https://mirror.whynothugo.nl/vdirsyncer/main/libdav/dav/struct.WebDavClient.html) — create_resource, update_resource, delete confirmed
- [libdav::names module](https://docs.rs/libdav/latest/libdav/names/index.html) — SYNC_COLLECTION, SYNC_TOKEN, SYNC_LEVEL constants confirmed
- [Google People API Reference](https://developers.google.com/people/api/rest/v1/people.connections/list) — sync token 7-day TTL, pagination, personFields, deleted contact detection
- [Google People API OAuth2 Scopes](https://developers.google.com/identity/protocols/oauth2/scopes) — `https://www.googleapis.com/auth/contacts` required for CRUD
- [reqwest 0.13.2 Response](https://docs.rs/reqwest/latest/reqwest/struct.Response.html) — `bytes_stream()`, `chunk()`, `headers()` confirmed

### Secondary (MEDIUM confidence)

- [icalendar 0.17.6 docs.rs](https://docs.rs/icalendar/latest/icalendar/) — `Calendar`, `Event`, `.parse()`, `.uid()`, `.starts()`, `.ends()` methods; RECURRENCE-ID support
- [RFC 4791 CalDAV](https://www.ietf.org/rfc/rfc4791.txt) — §5.3.4 ETag omission on server-modified PUT; comp-filter rules
- [RFC 6578 sync-collection](https://www.rfc-editor.org/rfc/rfc6578.html) — Depth: 0 requirement; 507 truncation; `./D:status` direct child rule (§3.5)
- [RFC 6585 429 Too Many Requests](https://datatracker.ietf.org/doc/html/rfc6585) — Retry-After header semantics
- [Google People API contacts.list](https://developers.google.com/people/api/rest/v1/people.connections/list) — sync token expiry, pagination, deleted contacts via metadata.deleted

### Tertiary (LOW confidence)

- Server compatibility matrix facts (ETag omission by iCloud, Fastmail, Nextcloud) — derived from C++ code comments and python-caldav project documentation cross-references within DAVWorker.cpp; not independently verified against current server versions
- `CardDavClient::find_addressbooks()` existence — inferred from CalDavClient pattern and WebDavClient Deref; not verified from CardDavClient-specific docs (mirror page for CardDavClient was not accessible separately)

---

## Metadata

**Confidence breakdown:**
- Standard stack (libdav 0.10.2, reqwest 0.13, icalendar 0.17): HIGH — versions confirmed from mirror docs and docs.rs
- libdav API surface (CalDavClient methods): HIGH — mirror.whynothugo.nl docs read directly
- libdav sync_collection absence: HIGH — exhaustive method listing confirms no such method exists; raw request() must be used
- CalDAV/CardDAV sync algorithms: HIGH — C++ source read line-by-line; all edge cases documented in code comments
- Google People API: HIGH — official Google developer docs read directly; sync token 7-day TTL confirmed
- Metadata worker protocol: HIGH — C++ source read completely; wire format documented
- Server compatibility matrix: MEDIUM — derived from C++ code comments citing python-caldav project; not independently re-tested against current server versions

**Research date:** 2026-03-02
**Valid until:** 2026-06-01 (libdav API is stable at 0.10.2; Google People API v1 is stable; C++ reference is the ground truth)
