---
title: Monorepo Migration Plan
status: ✅ Completed
---

# Monorepo Migration — Completed

## What Changed

The UnifyMail project has been restructured into a cleaner monorepo layout inside the `app/` directory, similar to how [streamwall](https://github.com/streamwall/streamwall/tree/main/packages) organizes its packages.

### New Structure

```
app/
├── mailsync/                 # C++ email sync engine (inlined source)
│   └── Vendor/mailcore2/     # IMAP/SMTP library (stays as vendor dependency)
├── backend/        # Electron main process (was: app/src/browser/)
│   └── src/
│       ├── application.ts
│       ├── main.js
│       ├── window-manager.ts
│       └── ...
├── frontend/       # React UI / Flux architecture (was: app/src/)
│   └── src/
│       ├── app-env.ts
│       ├── components/
│       ├── flux/
│       ├── services/
│       └── ...
├── internal_packages/        # Plugin packages (unchanged)
├── spec/                     # Test specs (unchanged location, updated imports)
├── static/                   # Static assets (unchanged)
├── build/                    # Build tooling (updated globs)
├── package.json              # Main entry updated to backend/main.js
└── tsconfig.json             # Includes updated for new paths
```

## Files Modified

### Config & Build
- `app/package.json` — `main` entry → `./backend/main.js`
- `app/tsconfig.json` — `include` and `paths` updated for new dirs
- `build/Gruntfile.js` — TypeScript source globs updated
- `build/tasks/package-task.js` — Symlink resolution dirs + asar unpack patterns

### Backend (12 files)
All `../` imports in `backend/` that pointed to the old `app/src/` were rewritten to `../../frontend/`.

### Frontend (2 fixes)
- `app-env.ts` — `src/global` module path → `frontend/global`
- `app-env.ts` — `../src/flux/stores/workspace-store` → `./flux/stores/workspace-store`

### Specs (45 files)
All `../../src/` imports rewritten to `../../frontend/`.

### Internal Packages (4 files)
All `../../../src/` imports rewritten to `../../../frontend/`.

### Dynamic Path (critical)
- `main.js` line 411 — `path.join(resourcePath, 'src', 'browser', 'application')` → `path.join(resourcePath, 'backend', 'src', 'application')`
