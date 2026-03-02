# External Integrations

**Analysis Date:** 2026-03-01

## APIs & External Services

**Google (Gmail):**
- OAuth 2.0 authentication for Gmail accounts
  - Client ID: `400141604862-ceirca79mb14lt7vu06v7ascpo6rj0fr.apps.googleusercontent.com` (default, overridable via `MS_GMAIL_CLIENT_ID` env var)
  - Scopes: `https://mail.google.com/`, `userinfo.email`, `userinfo.profile`, `contacts`
  - Auth URL: `https://accounts.google.com/o/oauth2/auth`
  - Token exchange: Proxied through Cloudflare Worker at `https://unifymail-site.leveluptogetherbiz.workers.dev/auth/gmail/token`
  - Token refresh: Proxied through Cloudflare Worker at `https://unifymail-site.leveluptogetherbiz.workers.dev/auth/gmail/refresh`
  - User profile: `https://www.googleapis.com/oauth2/v1/userinfo?alt=json`
  - Implementation: `app/internal_packages/onboarding/lib/onboarding-helpers.ts` (functions `buildGmailAuthURL`, `buildGmailAccountFromAuthResponse`)
  - Constants: `app/internal_packages/onboarding/lib/onboarding-constants.ts`

**Microsoft (Office 365 / Outlook.com):**
- OAuth 2.0 authentication for Office 365 and Outlook accounts
  - Client ID: `8787a430-6eee-41e1-b914-681d90d35625` (default, overridable via `MS_O365_CLIENT_ID` env var)
  - Scopes: `user.read`, `offline_access`, `Contacts.ReadWrite`, `Contacts.ReadWrite.Shared`, `Calendars.ReadWrite`, `Calendars.ReadWrite.Shared`, `IMAP.AccessAsUser.All`, `SMTP.Send`
  - Auth URL: `https://login.microsoftonline.com/common/oauth2/v2.0/authorize` (PKCE flow with S256 code challenge)
  - Token URL: `https://login.microsoftonline.com/common/oauth2/v2.0/token` (direct exchange, no proxy)
  - User profile: `https://graph.microsoft.com/v1.0/me`
  - Implementation: `app/internal_packages/onboarding/lib/onboarding-helpers.ts` (functions `buildO365AuthURL`, `buildMicrosoftAccountFromAuthResponse`)
  - O365 page: `app/internal_packages/onboarding/lib/page-account-settings-o365.tsx`
  - Outlook page: `app/internal_packages/onboarding/lib/page-account-settings-outlook.tsx`
  - Note: Origin header stripped for Microsoft OAuth requests in main process to avoid AADSTS90023 errors (`app/backend/main.js`, lines 375-391)

**Thunderbird ISPDB Autoconfig:**
- Automatic IMAP/SMTP server discovery for generic email providers
  - URLs: `https://autoconfig.{domain}/mail/config-v1.1.xml` and `https://{domain}/.well-known/autoconfig/mail/config-v1.1.xml`
  - Implementation: `app/internal_packages/onboarding/lib/onboarding-helpers.ts` (function `TryThunderbirdAutoconfig`)
  - Fallback chain: N-API mailcore provider lookup -> Static JSON (`mailcore-provider-settings.json`) -> Thunderbird autoconfig -> UnifyMail provider settings JSON -> Domain-based fallback

**Cloudflare Workers (Auth Proxy):**
- OAuth token exchange proxy that keeps client secrets server-side
  - Worker name: `unifymail-site`
  - Source: `workers/auth-proxy/src/index.ts`
  - Config: `workers/auth-proxy/wrangler.jsonc`
  - URL: `https://unifymail-site.leveluptogetherbiz.workers.dev`
  - Endpoints:
    - `POST /auth/gmail/token` - Exchange authorization code for Gmail tokens
    - `POST /auth/gmail/refresh` - Refresh expired Gmail access token
    - `GET /` - Homepage (Google OAuth compliance)
    - `GET /privacy` - Privacy policy page (Google OAuth compliance)
    - `GET /terms` - Terms of service page (Google OAuth compliance)
  - Environment secrets: `GMAIL_CLIENT_ID`, `GMAIL_CLIENT_SECRET`

**Sentry (Error Reporting):**
- Error tracking via Raven SDK
  - Client: `raven` 2.1.2
  - Implementation: `app/frontend/error-logger-extensions/raven-error-reporter.js`
  - Device identification via hashed MAC address (using `getmac` package)
  - Disabled in dev mode and spec mode

**GitHub Releases (Auto-Update):**
- Application auto-update distribution
  - Client: `electron-updater` ^6.7.3
  - Feed: GitHub releases from `TheDarkSkyXD/UnifyMail`
  - Implementation: `app/backend/autoupdate-manager.ts`
  - Windows: Squirrel-based updates (`app/backend/autoupdate-impl-win32.ts`, `app/backend/windows-updater.js`)

## Supported Email Providers

**OAuth-based (built-in flows):**
- Gmail / G Suite - OAuth 2.0 via Google
- Office 365 - OAuth 2.0 via Microsoft (PKCE)
- Outlook.com / Hotmail - OAuth 2.0 via Microsoft (PKCE)

**App-password / IMAP-based (manual credential entry):**
- Yahoo
- iCloud
- FastMail
- GMX
- Yandex
- Any generic IMAP/SMTP provider

**Provider definitions:** `app/internal_packages/onboarding/lib/account-providers.tsx`
**Provider settings:** `app/internal_packages/onboarding/lib/unifymail-provider-settings.json`, `app/internal_packages/onboarding/lib/mailcore-provider-settings.json`

## Data Storage

**Databases:**
- SQLite (via `better-sqlite3` ^12.5.0 in Electron, direct SQLite in C++ sync engine)
  - Database file: `{configDir}/edgehill.db`
  - WAL mode enabled for concurrent read access
  - Connection: Read-only in Electron renderer (`app/frontend/flux/stores/database-store.ts`)
  - Write access: Exclusively via C++ mailsync process (`app/mailsync/MailSync/MailStore.hpp`)
  - Page size: 8192, Cache size: 20000
  - Agent process for background queries: `app/frontend/flux/stores/database-agent.js`

**File Storage:**
- Local filesystem only
- Config directory: `~/.config/UnifyMail/` (Linux), `%APPDATA%/UnifyMail/` (Windows), `~/Library/Application Support/UnifyMail/` (macOS)
- Dev config: `UnifyMail-dev/` suffix
- Spec config: `~/.UnifyMail-spec/`

**Caching:**
- LRU cache for database query results (`lru-cache` ^10.4.3 in `database-store.ts`)
- `FileListCache` for file listing optimization (`app/backend/file-list-cache.ts`)

## Authentication & Identity

**Email Account Auth:**
- OAuth 2.0 (Gmail, O365, Outlook) - Tokens stored locally via `KeyManager`
- IMAP/SMTP credentials (all other providers) - Passwords stored locally via `KeyManager`
- Implementation: `app/frontend/key-manager.ts`
- Storage: Encrypted using Electron `safeStorage` API, stored in config file under `credentials` key
- Per-account secrets stored as: `{email}-imap`, `{email}-smtp`, `{email}-refresh-token`

**Local OAuth Callback Server:**
- Local HTTP server on port 12141 for receiving OAuth callback redirects
- Gmail redirect: `http://127.0.0.1:12141`
- Microsoft redirect: `http://localhost:12141/desktop`
- Constant: `LOCAL_SERVER_PORT` in `app/internal_packages/onboarding/lib/onboarding-constants.ts`

**UnifyMail Identity (Legacy):**
- Identity management for feature gating (Pro vs Basic)
  - Store: `app/frontend/flux/stores/identity-store.ts`
  - Identity server: `http://localhost:5101` (all environments - appears to be a placeholder/disabled)
  - API endpoints: `/api/me`, `/api/login-link`, `/api/save-public-asset`, `/api/share-static-page`
  - Token stored in keychain under name "UnifyMail Account"
  - Polls identity every 10 minutes in main window
  - Implementation: `app/frontend/flux/unifymail-api-request.ts`

## Communication Protocols

**Email (via C++ mailsync process):**
- IMAP - Email retrieval and folder sync
- SMTP - Email sending
- Gmail X-GM-LABELS extension - Label handling for Gmail accounts
- CONDSTORE/XYZRESYNC - Incremental IMAP sync optimization

**Calendar/Contacts (via C++ mailsync process):**
- CalDAV - Calendar sync
- CardDAV - Contact sync
- Google Contacts API - Contact sync for Gmail accounts

## Inter-Process Communication

**Electron <-> Mailsync Process:**
- stdin: JSON messages from Electron to mailsync (task requests, commands)
- stdout: Newline-delimited JSON from mailsync to Electron (database change deltas)
- Implementation: `app/frontend/mailsync-process.ts`, `app/frontend/flux/mailsync-bridge.ts`
- One mailsync process spawned per email account

**Electron Main <-> Renderer:**
- `ipcMain` / `ipcRenderer` for cross-process messaging
- `@electron/remote` for synchronous API access from renderer
- Action bridge: `app/frontend/flux/action-bridge.ts`
- Mailsync bridge: `app/frontend/flux/mailsync-bridge.ts`

## Monitoring & Observability

**Error Tracking:**
- Sentry via Raven SDK (production only)
- Device hashing via MAC address for anonymous identification
- Implementation: `app/frontend/error-logger-extensions/raven-error-reporter.js`

**Logs:**
- `electron-log` for structured logging (auto-updater)
- `debug` npm package for debug-level logging (prefixed `app:RxDB`, etc.)
- `console.log/warn/error` used throughout codebase
- Dev tools console: `$m` exposes `unifymail-exports` for debugging

## CI/CD & Deployment

**Hosting:**
- GitHub Releases for distributing packaged application
- Cloudflare Workers for OAuth proxy (`workers/auth-proxy/`)

**CI Pipeline:**
- GitHub Actions (referenced in mailsync CLAUDE.md for Windows builds)
- Build targets: Windows (Squirrel/NSIS installer), macOS (DMG), Linux (Snap, Deb)

**Packaging:**
- `@electron/packager` 18.3.6 - Electron app packaging
- `electron-winstaller` 5.4.0 - Windows installer creation
- Snap: `snap/snapcraft.yaml` (base: core24, platforms: amd64/arm64)
- Grunt tasks: `build/tasks/` orchestrate the full build pipeline

## Environment Configuration

**Required env vars (runtime - set internally by the app):**
- `CONFIG_DIR_PATH` - Passed to mailsync process
- `GMAIL_CLIENT_ID` - Passed to mailsync process (has default)
- `GMAIL_OAUTH_PROXY_URL` - Passed to mailsync process (has default)
- `IDENTITY_SERVER` - Passed to mailsync process

**Optional override env vars:**
- `MS_GMAIL_CLIENT_ID` - Override default Gmail OAuth client ID
- `MS_O365_CLIENT_ID` - Override default O365 OAuth client ID

**Cloudflare Worker secrets (server-side only):**
- `GMAIL_CLIENT_ID` - Google OAuth client ID
- `GMAIL_CLIENT_SECRET` - Google OAuth client secret (never exposed to client)

**Secrets location:**
- User credentials: Encrypted via Electron `safeStorage` in `{configDir}/config.json` under `credentials` key
- OAuth tokens: Stored alongside credentials in same encrypted blob
- Identity token: Stored in OS keychain under "UnifyMail Account"

## Webhooks & Callbacks

**Incoming:**
- Local OAuth callback server on `http://127.0.0.1:12141` (Gmail) and `http://localhost:12141/desktop` (Microsoft)
- Custom protocol handler: `UnifyMail://` for deep linking (notifications, plugin management, thread opening)
- `mailto:` protocol handler for composing emails from external apps

**Outgoing:**
- None detected (no webhook dispatch)

## Custom Protocol Handlers

**`UnifyMail://` protocol:**
- `UnifyMail://open-inbox` - Show main window
- `UnifyMail://open-preferences` - Open preferences
- `UnifyMail://plugins` - Change plugin state
- `UnifyMail://notification-*` - Handle Windows toast notification actions
- Implementation: `app/backend/application.ts` (method `openUrl`)
- Registration: `app/backend/unifymail-protocol-handler.ts`

**`mailto:` protocol:**
- Opens compose window with pre-filled recipients and attachments
- Handles `?attach=file:///path` for file attachments
- Implementation: `app/backend/main.js` (function `extractMailtoLink`)

---

*Integration audit: 2026-03-01*
