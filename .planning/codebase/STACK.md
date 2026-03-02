# Technology Stack

**Analysis Date:** 2026-03-01

## Languages

**Primary:**
- TypeScript 5.7.3 - All Electron app code (frontend, backend, internal packages)
- C++11 - Native sync engine (`app/mailsync/MailSync/`)

**Secondary:**
- JavaScript (CommonJS) - Legacy modules, build scripts, bootstrap entry point (`app/unifymail-backend/src/main.js`)
- LESS - Stylesheets within internal packages (`app/internal_packages/*/styles/`)
- CSS (Tailwind) - Modern utility-first styles (`app/static/style/tailwind.src.css`)

## Runtime

**Environment:**
- Electron 39.2.7 (Chromium-based desktop runtime)
- Node.js >= 16.17 (embedded within Electron)

**Package Manager:**
- npm >= 8
- Lockfile: `package-lock.json` present at root and `app/package-lock.json`

**Dual package.json structure:**
- Root `package.json` - Dev tooling, build dependencies, scripts
- `app/package.json` - Runtime application dependencies (productName: "UnifyMail", version: 1.17.4)

## Frameworks

**Core:**
- React 16.9.0 - UI rendering (`app/unifymail-frontend/src/components/`)
- Reflux 0.1.13 - Flux-based state management (`app/unifymail-frontend/src/flux/`)
- RxJS (rx-lite 4.0.8) - Reactive observable queries
- Slate (custom fork) - Rich text editor for email composer (`@bengotow/slate-edit-list`, `slate-react`)

**Testing:**
- Jasmine 2.x - Test runner and assertions (`app/spec/`)
- Enzyme 3.11.0 + enzyme-adapter-react-16 - React component testing
- React Test Renderer 16.9.0 - Snapshot testing

**Build/Dev:**
- Grunt - Build orchestration (`build/Gruntfile.js`, `build/tasks/`)
- TypeScript Compiler - Type checking (`app/tsconfig.json`)
- ESLint 7.x + @typescript-eslint/parser 5.x - Linting (`.eslintrc`)
- Prettier 1.x - Code formatting (`.prettierrc`)
- Tailwind CSS 4.1.18 - Utility CSS generation (`app/package.json` devDependencies)
- @electron/packager 18.3.6 - Electron app packaging

## Key Dependencies

**Critical:**
- `better-sqlite3` ^12.5.0 - Local SQLite database access (read-only in Electron, WAL mode) (`app/unifymail-frontend/src/flux/stores/database-store.ts`)
- `electron-updater` ^6.7.3 - Auto-update via GitHub Releases (`app/unifymail-backend/src/autoupdate-manager.ts`)
- `@electron/remote` ^2.1.2 - Cross-process Electron API access
- `google-auth-library` ^10.5.0 - Google OAuth support
- `dompurify` ^3.3.1 - HTML sanitization for email content
- `mailcore-napi` (optional, file: reference) - Native N-API addon for IMAP/SMTP account validation (`app/mailcore/`)

**UI/Editor:**
- `slate` / `slate-react` (custom GitHub forks) - Rich text email composition
- `react-color` ^2.19.3 - Color picker component
- `mousetrap` ^1.6.5 - Keyboard shortcut handling
- `classnames` ^2.5.1 - Dynamic CSS class composition

**Email Processing:**
- `cheerio` ^1.1.2 - HTML parsing for email body processing
- `juice` ^11.0.3 - CSS inlining for email composition
- `mammoth` ^1.11.0 - DOCX to HTML conversion (attachment preview)
- `chrono-node` ^2.9.0 - Natural language date parsing (send later, snooze)
- `ical-expander` ^3.2.0 / `ical.js` ^2.2.1 - iCalendar event parsing
- `vcf` ^2.0.5 - vCard contact parsing
- `xml2js` ^0.6.2 - XML parsing (Thunderbird autoconfig)

**Infrastructure:**
- `raven` 2.1.2 - Sentry error reporting (`app/unifymail-frontend/src/error-logger-extensions/raven-error-reporter.js`)
- `electron-log` ^5.4.3 - Structured logging
- `lru-cache` ^10.4.3 - In-memory caching for database queries
- `event-kit` ^1.0.2 - Event emitter/disposable pattern
- `underscore` ^1.13.7 - Utility library (used extensively)
- `moment` ^2.30.1 - Date/time manipulation
- `uuid` ^13.0.0 - Unique identifier generation
- `source-map-support` ^0.5.21 - Stack trace source mapping

**Native Sync Engine Dependencies (vcpkg-managed C++):**
- OpenSSL - TLS/SSL
- libcurl - HTTP client
- libxml2 - XML parsing
- SQLite (via SQLiteCpp wrapper) - Database
- ICU - Unicode support
- tidy-html5 - HTML cleaning
- cyrus-sasl - SASL authentication

## Configuration

**Environment:**
- Config stored in `~/.config/UnifyMail/` (production) or `~/.config/UnifyMail-dev/` (dev mode)
- `app/default-config/config.json` - Default configuration template
- `app/default-config/keymap.json` - Default keyboard shortcuts
- Credentials stored via Electron `safeStorage` API, encrypted in config file (`app/unifymail-frontend/src/key-manager.ts`)
- No `.env` files detected in the repository

**Required Environment Variables (for mailsync process):**
- `CONFIG_DIR_PATH` - Path to application config directory
- `GMAIL_CLIENT_ID` - Google OAuth client ID (has default fallback)
- `GMAIL_OAUTH_PROXY_URL` - Cloudflare Worker URL for token exchange
- `IDENTITY_SERVER` - Identity API server URL

**TypeScript Configuration (`app/tsconfig.json`):**
- Target: ES2017
- Module: CommonJS
- JSX: React
- Path aliases: `*` resolves to `node_modules/*` and `unifymail-frontend/src/global/*`
- Includes: `unifymail-frontend/src/**/*`, `unifymail-backend/src/**/*`, `internal_packages/**/*`

**Build:**
- `build/Gruntfile.js` - Main build configuration
- `build/tasks/` - Grunt task definitions
- Production builds: `npm run build` (delegates to Grunt `build-client`)
- Windows installer: `electron-winstaller` 5.4.0
- Linux packaging: Snap (`snap/snapcraft.yaml`, base: core24)

**Linting (`.eslintrc`):**
- Parser: @typescript-eslint/parser
- Extends: eslint:recommended, @typescript-eslint/recommended, prettier
- Many TypeScript strict rules disabled (no-explicit-any, no-unused-vars, etc.)

**Formatting (`.prettierrc`):**
- Print width: 100
- Single quotes: true
- Trailing commas: ES5

## Platform Requirements

**Development:**
- Node.js >= 16.17
- npm >= 8
- For native sync engine: C++ toolchain (MSVC on Windows, GCC/Clang on Linux, Xcode on macOS)
- vcpkg (for Windows C++ dependency management)
- Git

**Production:**
- Windows 10+ (Squirrel installer, `.exe`)
- macOS (`.dmg` / `.app`)
- Linux (Snap package, `.deb`)
- SQLite database at `{configDir}/edgehill.db`

**Target Platforms:**
- Windows (amd64) - Primary
- macOS (amd64, arm64)
- Linux (amd64, arm64 via Snap)

---

*Stack analysis: 2026-03-01*
