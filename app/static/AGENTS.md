<!-- Parent: ../AGENTS.md -->
<!-- Generated: 2026-02-15 | Updated: 2026-02-15 -->

# static

## Purpose
Static assets served to the Electron renderer process. Contains the HTML entry points, base CSS/LESS stylesheets, fonts, images, sounds, animations, and third-party extensions.

## Key Files

| File | Description |
|------|-------------|
| `index.html` | Main renderer window HTML entry point |
| `index.js` | Renderer process JavaScript bootstrap (loads compile-cache, app-env) |
| `db-migration.html` | Database migration progress UI (shown during schema upgrades) |
| `db-vacuum.html` | Database vacuum/optimization progress UI |
| `font-awesome.min.css` | Font Awesome icon library CSS |
| `all_licenses.html` | Generated open-source license attributions page |
| `all_licenses_preamble.html` | Preamble HTML for the license attributions |

## Subdirectories

| Directory | Purpose |
|-----------|---------|
| `style/` | Base LESS stylesheets: variables, mixins, component styles, layout |
| `images/` | Application images: icons, illustrations, onboarding graphics |
| `fonts/` | Bundled fonts (Nylas symbols, FiraMono, SourceSansPro) |
| `sounds/` | Notification and UI sound effects |
| `animations/` | CSS/SVG animation assets |
| `extensions/` | Third-party extensions bundled with the app |

## For AI Agents

### Working In This Directory
- `index.html` and `index.js` are the renderer entry points — modify with extreme care
- Base styles in `style/` define the design system: color variables, typography, spacing
- Theme packages in `internal_packages/ui-*` override variables defined here
- Add new images to `images/` with appropriate naming (use @2x suffix for Retina)
- Sound files should be short WAV or MP3 clips

### Common Patterns
- LESS stylesheets use variables for colors, font sizes, and spacing
- Image assets use `@2x` suffix convention for Retina/HiDPI displays
- The base style directory establishes the design tokens that themes customize

## Dependencies

### Internal
- `app/src/window-bootstrap.ts` — Invoked by `index.js`
- `app/src/compile-cache.js` — Loaded by `index.js` for dev mode

### External
- Font Awesome — Icon library
- Custom fonts — Nylas Mail symbols, FiraMono, Source Sans Pro

<!-- MANUAL: Any manually added notes below this line are preserved on regeneration -->
