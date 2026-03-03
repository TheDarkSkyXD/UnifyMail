# Phase 2: IMAP Connection Testing — Tracking

**Status:** In Progress
**Branch:** `feat/phase2-imap-connection-testing`
**Started:** 2026-03-03

## Plans

| Plan | Wave | Status | Description |
|------|------|--------|-------------|
| 02-01 | 1 | Pending | Core `testIMAPConnection` Rust implementation |
| 02-02 | 2 | Pending | Mock IMAP test suite + wrapper switchover |

## Requirements

- IMAP-01: TLS connection (port 993)
- IMAP-02: STARTTLS upgrade
- IMAP-03: Clear/unencrypted connection
- IMAP-04: Password + XOAUTH2 auth
- IMAP-05: 7 capability detections
- IMAP-06: 15-second timeout
