<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# services

## Purpose
Application services that transform, analyze, and process email content and search queries. Includes HTML sanitization, inline style transformation, quoted text detection, and autolink utilities.

## Key Files

| File | Description |
|------|-------------|
| `autolinker.ts` | Automatically converts URLs, emails, and phone numbers in text to clickable links |
| `inline-style-transformer.ts` | Transforms LESS/CSS styles for inline rendering in email HTML |
| `quote-string-detector.ts` | Detects and identifies quoted reply text in email messages |
| `quoted-html-transformer.ts` | Processes quoted HTML content: collapse, expand, strip quotes for replies |
| `sanitize-transformer.ts` | HTML sanitization pipeline for rendering email content safely (XSS prevention) |
| `unwrapped-signature-detector.ts` | Detects email signatures that aren't in standard wrapper elements |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `search/` | Search query parsing and transformation |

## For AI Agents

### Working In This Directory
- These services handle **untrusted email HTML** — security is paramount
- The sanitize transformer strips dangerous HTML/JS while preserving email formatting
- Quoted text detection is complex (varies by email client) — test with many real-world emails
- Changes to HTML transformation affect how every email is displayed

### Testing Requirements
- Test with diverse real-world email HTML (different clients: Gmail, Outlook, Apple Mail)
- Security-critical: test for XSS vectors in sanitization
- Test quoted text detection with various reply/forward formats

### Common Patterns
- Services are pure functions or stateless transformers (no Flux store dependency)
- HTML parsing uses DOMParser or regex-based approaches
- Pipeline pattern: input HTML → transform 1 → transform 2 → sanitized output

## Dependencies

### Internal
- `app/src/flux/stores/message-body-processor.ts` — Orchestrates transformation pipeline
- `app/src/flux/stores/message-store.ts` — Triggers body processing for display

### External
- DOMParser — HTML parsing
- DOMPurify — XSS sanitization (if used)

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
