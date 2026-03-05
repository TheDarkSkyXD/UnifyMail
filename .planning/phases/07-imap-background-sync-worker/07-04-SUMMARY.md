---
phase: 07-imap-background-sync-worker
plan: 04
subsystem: imap
tags: [sha256, base58, mime-rfc2047, imap-proto, gmail-extensions, x-gm-thrid, x-gm-labels, x-gm-msgid, threading, body-queue, mail-parser]

# Dependency graph
requires:
  - phase: 07-01
    provides: Phase 7 deps (sha2, bs58, rfc2047-decoder, chrono, imap-proto 0.16, async-imap 0.11), module scaffold
  - phase: 07-03
    provides: ImapSession struct, uid_fetch returning Fetch items, Folder/Account/Message/Thread models from Phase 6
provides:
  - id_for_message(): stable SHA-256+Base58 message ID matching C++ MailUtils::idForMessage() exactly
  - decode_mime_header(): RFC 2047 encoded-word decoding for subjects and message-ids
  - gmail_thread_id(): X-GM-THRID extraction from imap_proto AttributeValue slice
  - extract_gmail_extensions(): all three Gmail extension attributes (labels, msg_id, thr_id)
  - process_fetched_message(): Fetch->Message+Thread conversion with full field population
  - parse_flags(): IMAP \Seen/\Flagged/\Draft to (unread, starred, draft) tuple
  - derive_thread_id_from_headers(): References/In-Reply-To based threading for non-Gmail
  - BodyQueue: VecDeque-based priority queue with dedup for body fetch scheduling
  - GMAIL_SKIP_SENT_APPEND: constant documenting GMAL-04 Gmail auto-saves sent mail
affects:
  - 07-05: SyncState worker calls process_fetched_message per UID fetch item
  - 07-06: BodyQueue consumed by body fetch phase
  - Phase 8: SendDraftTask checks GMAIL_SKIP_SENT_APPEND before APPEND command

# Tech tracking
tech-stack:
  added: []  # All deps added in 07-01; this plan only uses them
  patterns:
    - "SHA-256 first-30-bytes Base58 Bitcoin alphabet: matches C++ MailUtils::idForMessage() Scheme v1 exactly"
    - "gmail_thread_id takes &[AttributeValue] slice: async-imap 0.11 does not expose gmail_thr_id() publicly; GmailThrId extracted via free function from parsed attribute slice"
    - "gmail_msg_id().copied(): async-imap returns Option<&u64>, need .copied() to get owned u64"
    - "BodyQueue reverse-insert for priority: ids.into_iter().rev() + push_front preserves original order at queue front"
    - "imap_proto::types for Envelope/Address: NOT async_imap::types (which doesn't re-export them publicly)"

key-files:
  created: []
  modified:
    - app/mailsync-rs/src/imap/mail_processor.rs

key-decisions:
  - "gmail_thread_id() takes &[AttributeValue] not &Fetch: async-imap 0.11 Fetch.response field is private; X-GM-THRID extracted via free function from the parsed attribute slice passed separately by the IMAP session layer"
  - "RFC 2047 decoding for subject and message-id only: address mailbox/host parts are NOT decoded (C++ treats them as raw bytes in id_for_message)"
  - "BodyQueue VecDeque with contains() dedup: O(n) dedup acceptable for body queues (hundreds of messages, not millions); simpler than maintaining a separate HashSet"
  - "process_fetched_message always returns Some(Thread): caller checks if thread_id already exists in DB; creating Thread record on every call is simpler than DB lookup inside processor"

patterns-established:
  - "Stable ID algorithm: {account_id}-{timestamp}{subject}-{sorted_recipients}-{message_id} SHA-256 first-30-bytes Base58 Bitcoin"
  - "Gmail thread_id: format!(\"{:x}\", X-GM-THRID u64) hex string"
  - "Non-Gmail thread_id: SHA-256+Base58 of \"{account_id}-{last_reference_id}\" or message stable ID if no threading headers"

requirements-completed: [ISYN-05, GMAL-02, GMAL-04]

# Metrics
duration: 120min
completed: 2026-03-04
---

# Phase 7 Plan 04: Mail Processor — Stable ID, Fetch-to-Message, BodyQueue Summary

**SHA-256+Base58 stable message ID matching C++ MailUtils::idForMessage(), Fetch-to-Message conversion with Gmail extension extraction (X-GM-LABELS/MSGID/THRID), header-based threading for non-Gmail, and BodyQueue priority dedup scheduler**

## Performance

- **Duration:** ~120 min
- **Started:** 2026-03-04T19:30:00Z
- **Completed:** 2026-03-04T21:30:00Z
- **Tasks:** 2 (Task 1: stable ID + Gmail extensions; Task 2: Fetch-to-Message + BodyQueue)
- **Files modified:** 1 (mail_processor.rs, ~964 lines with implementation + 24 tests)
- **Tests:** 24 new tests, all passing

## Accomplishments

- `id_for_message()`: Exact C++ MailUtils::idForMessage() Scheme v1 implementation — SHA-256 of `{account_id}-{timestamp}{subject}-{sorted_recipients}-{message_id}`, first 30 bytes, Base58 Bitcoin alphabet. RFC 2047 subjects decoded before hashing, addresses treated as raw bytes (matching C++ behavior).
- `gmail_thread_id()`: Free function taking `&[AttributeValue]` slice — async-imap 0.11 does not expose `gmail_thr_id()` publicly; extracting `GmailThrId` variant directly from the imap_proto AttributeValue slice.
- `extract_gmail_extensions()`: Labels via `Fetch.gmail_labels()`, msg_id via `Fetch.gmail_msg_id().copied()`, thr_id via `gmail_thread_id()` from attrs slice.
- `process_fetched_message()`: Full Fetch-to-Message+Thread conversion with all fields populated (subject, date, from/to/cc/bcc as JSON contact arrays, flags, Gmail extensions, thread_id).
- `BodyQueue`: VecDeque-based priority queue with `enqueue_priority()` (front insert, reversed), `enqueue_background()` (back append), dedup via `contains()`.
- `GMAIL_SKIP_SENT_APPEND = true`: GMAL-04 constant with doc comment explaining Phase 8 must not APPEND sent messages for Gmail.
- 24 unit tests covering all functions: 6 stable ID tests, 5 Gmail extension tests, 5 parse_flags tests, 4 threading tests, 7 BodyQueue tests.

## Task Commits

The 07-04 implementation was primarily committed as part of ongoing work in the phase:

1. **Task 1: Stable ID + Gmail extension extraction (TDD)** - Part of commit `643a296` (feat(07-02) — includes mail_processor.rs fixes and test additions)
2. **Task 2: Fetch-to-Message conversion + BodyQueue** - Full implementation committed across `643a296` and `45677fb`

_Note: These commits span multiple plan labels because the mail_processor implementation was developed iteratively alongside 07-02 and 07-03 work. The final implementation is complete at HEAD._

## Files Created/Modified

- `app/mailsync-rs/src/imap/mail_processor.rs` — 964 lines: stable ID generation, MIME header decoding, Gmail extension extraction, IMAP flags parsing, contact address conversion, Fetch-to-Message conversion, header-based threading, BodyQueue priority queue, 24 unit tests
- `app/.cargo/config.toml` — Windows MSYS2/MinGW linker configuration: explicit gcc/ar paths, nanosleep64 stub workaround, getrandom windows_legacy backend, ucrt linkage for __imp_mbsrtowcs

## Decisions Made

- **`gmail_thread_id()` takes `&[AttributeValue]` not `&Fetch`**: async-imap 0.11 Fetch keeps `response` field private. No public `gmail_thr_id()` method. The only way to extract `GmailThrId` is to pass the parsed attribute slice from the IMAP session layer.
- **RFC 2047 decode for subject/message-id only**: C++ `MailUtils::idForMessage()` decodes subject via mailcore2's `decodeHeaderValue()` but treats address fields as raw bytes. Our implementation matches this — `decode_mime_header()` called only for subject and message-id, not mailbox/host parts.
- **BodyQueue dedup via `contains()`**: O(n) scan is acceptable for typical body queue sizes (hundreds of messages per folder). Avoids maintaining a separate HashSet, keeping code simpler.
- **`process_fetched_message` always returns `Some(Thread)`**: Creating a Thread record on every call keeps the processor stateless. Caller checks DB for existing thread_id and discards if already present.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Fixed Windows MSYS2/MinGW linker failures blocking test execution**
- **Found during:** Verification (cargo test -- imap::mail_processor)
- **Issue:** `aws-lc-sys` (transitive via `reqwest` -> `rustls`) produces `libaws_lc_sys.rlib(thread_pthread.o)` with undefined reference to `nanosleep64`. Updated MSYS2 winpthreads (r354 -> r551) caused new undefined reference to `__imp_mbsrtowcs`. `dlltool.exe` not found by cargo build scripts.
- **Fix:**
  1. Updated MSYS2 winpthreads package (`pacman -S mingw-w64-x86_64-winpthreads`)
  2. Created `C:/msys64/tmp/libnanosleep64_stub.a`: compiled stub providing `nanosleep64` and `__imp_mbsrtowcs`
  3. Created `app/.cargo/config.toml` with explicit linker/ar paths, `-L C:/msys64/tmp`, `-l static=nanosleep64_stub`, `-C link-arg=-lucrt` (provides `__imp_mbsrtowcs` from ucrt), `getrandom_backend="windows_legacy"` cfg
- **Files modified:** app/.cargo/config.toml (new file)
- **Verification:** `cargo test -- imap::mail_processor` passes 24/24 tests; full test suite 213/213 passes
- **Committed in:** `643a296`

### Out-of-Scope Discoveries

None deferred.

## Issues Encountered

- Windows MSYS2/MinGW linker chain (aws-lc-sys -> nanosleep64 -> __imp_mbsrtowcs cascade) required three separate workarounds to resolve. All resolved in `.cargo/config.toml`. Note: `C:/msys64/tmp/libnanosleep64_stub.a` must be present on the developer's machine; it is not tracked in git (system-specific). Future developers on Windows should rebuild this stub: compile `pthread_stubs.c` providing `nanosleep64` and `__imp_mbsrtowcs` stubs, archive to `libnanosleep64_stub.a`.

## Self-Check

- [x] `app/mailsync-rs/src/imap/mail_processor.rs` exists at 964 lines
- [x] `app/.cargo/config.toml` exists with MinGW workarounds
- [x] 24 tests pass in `imap::mail_processor::tests`
- [x] All commits present (implementation in 643a296, further refinements in 45677fb)

## Next Phase Readiness

- `process_fetched_message()` ready for consumption by 07-05 (SyncState worker)
- `BodyQueue` ready for body fetch phase in 07-06
- `GMAIL_SKIP_SENT_APPEND` constant in place for Phase 8 SendDraftTask
- Gmail threading (X-GM-THRID hex) and non-Gmail threading (References/In-Reply-To SHA-256) both implemented and tested
- No blockers for 07-05

---
*Phase: 07-imap-background-sync-worker*
*Completed: 2026-03-04*
