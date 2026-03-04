# Phase 4: Cross-Platform Packaging and Cleanup - Research

**Researched:** 2026-03-03
**Domain:** napi-rs GitHub Actions CI, Electron asar packaging, Rust binary size optimization, C++ cleanup
**Confidence:** HIGH (primary findings from official napi-rs docs, direct CI workflow inspection, and existing project structure)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

**Phase boundary:**
GitHub Actions CI produces .node binaries for all 5 platform targets (win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64), the stripped Linux x64 release binary is under 8MB with LTO, consumer code imports via `require('mailcore-napi')` without modification, the mailcore-wrapper module is removed, and all C++ N-API addon artifacts (app/mailcore/, app/mailcore-wrapper/, node-gyp, node-addon-api) are deleted from the repository. The C++ mailsync engine (app/mailsync/) is NOT touched — it stays until v2.0 Phase 10.

**Binary size enforcement:**
- 8MB hard fail in CI on Linux x64 — CI step fails the build if the stripped .node binary exceeds 8MB
- Size check runs on Linux x64 only — other platforms may vary and aren't gated
- If 8MB is exceeded after Phase 3 code is complete: investigate with cargo-bloat first, identify largest contributors, trim unnecessary features or adjust opt-level before considering relaxing the limit
- Full Cargo release profile: `lto = true`, `codegen-units = 1`, `strip = "symbols"`, `panic = "abort"`

**Wrapper module removal:**
- Remove mailcore-wrapper entirely — delete `app/mailcore-wrapper/` directory and all references
- Point `app/package.json` directly at `"mailcore-napi": "file:mailcore-rs"` in `optionalDependencies`
- Keep as optionalDependencies (not regular dependencies) — both consumer files have try/catch fallbacks
- The require chain after cleanup: `require('mailcore-napi')` → `app/mailcore-rs/index.js` → `mailcore-rs.<platform>.node`

**C++ deletion scope:**
- Delete `app/mailcore/` — entire C++ N-API addon directory (src/, build-windows/, Externals/, binding.gyp, etc.)
- Delete `app/mailcore-wrapper/` — no longer needed after wrapper removal
- Leave `app/mailsync/` untouched — C++ sync engine binary stays until v2.0 Phase 10
- Completely remove Windows C++ CI steps — vcpkg setup, msbuild steps 6-12 in build-windows.yaml. No comments, clean delete.
- Aggressive trim of Linux CI system packages — remove all C++-only packages: autoconf, automake, clang, cmake, libctemplate-dev, libcurl4-openssl-dev, libicu-dev, libsasl2-dev, libsasl2-modules, libsasl2-modules-gssapi-mit, libssl-dev, libtidy-dev, libtool, libxml2-dev, execstack. Keep Electron-needed packages.
- Remove `node-addon-api` and `node-gyp` from root package.json
- Remove any remaining references to `file:mailcore` in package.json files

**CI workflow structure:**
- Insert Rust build steps into existing 4 workflows — no new shared/reusable workflow
- Each workflow gets: Rust toolchain setup (dtolnay/rust-toolchain@stable), cargo cache (actions/cache@v4), napi build step with platform-specific target
- Add CI verification step — explicit `npm ci` after cleanup changes to catch broken references
- Add smoke test — quick Electron headless test verifying `require('mailcore-napi')` loads and `providerForEmail('test@gmail.com')` returns a provider object

**Dev workflow:**
- No changes to npm start — Phase 1's Rust build integration stays as-is

### Claude's Discretion

- opt-level choice: try `"z"` (size) first, fall back to `"s"` (balanced) if performance issues arise
- Custom index.js vs napi-rs generated loader: check if napi-rs v3 fixed the shlib_suffix detection issue for GNU .node in MSVC Node.js; use standard loader if fixed, keep custom if not
- Windows setup documentation updates after C++ removal
- Exact smoke test implementation (inline in workflow vs separate script)
- Cargo dependency feature flag tuning for minimum binary size

### Deferred Ideas (OUT OF SCOPE)

None — discussion stayed within phase scope.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SCAF-03 | GitHub Actions CI builds for all 5 targets (win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64) | All 4 existing workflows inspected; exact step insertion points documented per workflow; Windows uses GNU target (x86_64-pc-windows-gnu) matching existing dev toolchain |
| SCAF-04 | Release binary < 8MB on Linux x64 with LTO + strip | Full Cargo release profile documented; size verification step defined; 8MB achievable with opt-level="z" + LTO + panic=abort + strip; cargo-bloat identified as the investigation tool |
| INTG-01 | `onboarding-helpers.ts` works with Rust addon via existing `require('mailcore-napi')` | No TypeScript changes needed; app/package.json pointer change from `file:mailcore-wrapper` to `file:mailcore-rs` is the only change; loader.js already exports all 5 functions; try/catch fallback confirmed |
| INTG-02 | `mailsync-process.ts` works with Rust addon via existing require path | Same pointer change covers this; require('mailcore-napi') call at mailsync-process.ts line 439 traces through to loader.js which exports validateAccount; fallback to mailsync child process confirmed |
| INTG-03 | All C++ source files, node-gyp configs, and vendored mailcore2 removed | Deletion scope: app/mailcore/ and app/mailcore-wrapper/ — both confirmed present; app/mailsync/ explicitly left intact; all reference locations catalogued |
| INTG-04 | `node-addon-api` and `node-gyp` dependencies removed from package.json | Root package.json has node-gyp: ^12.1.0; app/mailcore/package.json has node-addon-api: ^7.1.0; after deleting app/mailcore/, only root package.json needs editing |
</phase_requirements>

## Summary

Phase 4 has four distinct work streams: (1) Cargo.toml release profile additions to hit the 8MB size target, (2) GitHub Actions CI that builds the napi-rs .node addon for all 5 targets, (3) removing the mailcore-wrapper and pointing `app/package.json` directly at `file:mailcore-rs`, and (4) cleanly deleting all C++ artifacts from the repository.

The good news: the existing project already has workflows for all 5 platforms (build-linux.yaml, build-linux-arm64.yaml, build-macos.yaml, build-windows.yaml), already has `*.node` in the asar unpack glob in `build/tasks/package-task.js`, and the `app/mailcore-rs/loader.js` already exports all 5 functions (providerForEmail, registerProviders, testIMAPConnection, testSMTPConnection, validateAccount). The primary CI work is inserting a Cargo/napi-rs build step into each workflow before `npm run build` is called.

The existing `app/mailcore-rs/index.js` is a hand-written custom loader (not napi-rs generated) that bypasses the shlib_suffix detection issue and loads the correct binary for all 5 platforms. The current `app/mailcore-wrapper/` intermediary will be removed — after which `require('mailcore-napi')` resolves directly to `app/mailcore-rs/index.js` → loader.js chain.

**Critical Windows note:** The project uses the **GNU target** (`x86_64-pc-windows-gnu`) not MSVC. The CI must use MSYS2 (`msys2/setup-msys2@v2`) to provide `dlltool.exe` and set `LIBNODE_PATH`. This is already established by the local dev workflow and README.md.

**Primary recommendation:** Insert Rust build steps (dtolnay/rust-toolchain@stable + actions/cache@v4 + napi build --release --target) into each existing workflow, add the full Cargo release profile to Cargo.toml, update app/package.json to point at `file:mailcore-rs`, delete mailcore-wrapper and mailcore directories, then remove C++ package dependencies.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| @napi-rs/cli | ^3.0.0 (devDep in mailcore-rs) | `napi build` command | Official napi-rs build tool; already in mailcore-rs/package.json |
| dtolnay/rust-toolchain | stable | Rust toolchain in GitHub Actions | Canonical action; used by all napi-rs examples |
| msys2/setup-msys2 | v2 | MSYS2 environment for Windows GNU target | Required to provide dlltool.exe for x86_64-pc-windows-gnu |
| actions/cache | v4 | Cargo dependency cache | Reduces build time from 15min to 3min; already used in project for npm cache |

### Platform-to-Target Mapping (LOCKED — from project toolchain decisions)
| Platform | GitHub Runner | Rust Target | Build Command | Notes |
|----------|---------------|-------------|---------------|-------|
| win-x64 | `windows-2022` | `x86_64-pc-windows-gnu` | `napi build --release --target x86_64-pc-windows-gnu` | MSYS2 provides dlltool.exe; LIBNODE_PATH required |
| mac-arm64 | `macos-latest` | `aarch64-apple-darwin` | `napi build --release --target aarch64-apple-darwin` | matrix.arch == 'arm64' |
| mac-x64 | `macos-15-intel` | `x86_64-apple-darwin` | `napi build --release --target x86_64-apple-darwin` | matrix.arch == 'x64' |
| linux-x64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `napi build --release --target x86_64-unknown-linux-gnu` | Native runner; no cross-compilation |
| linux-arm64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | `napi build --release --target aarch64-unknown-linux-gnu` | Native runner; already in use |

**Note on linux-x64:** The `--use-napi-cross` flag (cargo-zigbuild) is NOT needed when building natively on ubuntu-22.04. It's only needed when cross-compiling from a different arch. The project already uses a native ubuntu-22.04 runner.

### Cargo Release Profile (LOCKED — from CONTEXT.md decisions)
```toml
[profile.release]
lto = true           # link-time optimization (already present)
strip = "symbols"    # strip debug symbols (already present)
codegen-units = 1    # ADD: single codegen unit enables better inlining across modules
panic = "abort"      # ADD: removes unwinding code (~10-20% size reduction)
opt-level = "z"      # ADD: optimize for size (Claude's discretion: try "z" first, fall back to "s")
```

**Current state of Cargo.toml `[profile.release]`:**
```toml
[profile.release]
lto = true
strip = "symbols"
```
Two entries need to be added: `codegen-units = 1` and `panic = "abort"`. `opt-level = "z"` is also added per Claude's discretion.

## Architecture Patterns

### Require Chain Before and After Phase 4

**Before Phase 4:**
```
require('mailcore-napi')
  → app/node_modules/mailcore-napi (symlink: file:mailcore-wrapper)
  → app/mailcore-wrapper/index.js  (wrapper routing layer)
  → require('../mailcore-rs/loader.js')
  → app/mailcore-rs/loader.js  (platform binary map)
  → app/mailcore-rs/mailcore-napi-rs.win32-x64-gnu.node
```

**After Phase 4:**
```
require('mailcore-napi')
  → app/node_modules/mailcore-napi (symlink: file:mailcore-rs)
  → app/mailcore-rs/index.js  (hand-written platform loader — already handles all 5 platforms)
  → app/mailcore-rs/mailcore-napi-rs.<platform>.node
```

**Key insight:** `app/mailcore-rs/index.js` is the hand-written loader (not the napi-generated one that ships with `napi build`). This loader already correctly handles GNU .node files in MSVC Node.js via explicit BINARY_MAP lookup. The `mailcore-wrapper` was a routing layer that added indirection but no unique logic — it simply delegated to `loader.js`. After removing the wrapper, `app/package.json` must point directly at `file:mailcore-rs`, and `mailcore-rs/package.json` (`"name": "mailcore-napi"`) ensures the module resolution works correctly.

**Note:** `app/mailcore-rs/index.js` is the auto-generated NAPI-RS file (large, 578 lines). `app/mailcore-rs/loader.js` is the hand-written custom loader. The `mailcore-rs/package.json` `"main"` field currently says `"index.js"` — which is the large generated file. After removing the wrapper, we should verify whether `package.json` should point to `loader.js` (the custom one) or stay pointing to `index.js`. Given that `loader.js` was written precisely to fix the shlib_suffix detection problem, and `index.js` has the flawed detection logic, the `package.json` `"main"` should be updated to `"loader.js"`.

### Pattern 1: Correct CI Step Order

**Critical for correctness:**
```
1. actions/checkout  (gets committed index.js, loader.js, index.d.ts from git)
2. npm ci            (symlinks node_modules/mailcore-napi → app/mailcore-rs/ via file:mailcore-rs)
3. Setup Rust + napi build  (produces mailcore-napi-rs.<platform>.node in app/mailcore-rs/)
4. Binary size check  (linux-x64 only: fail if > 8MB)
5. npm run build     (Electron packager runs; *.node glob in asar.unpack catches the binary)
```

`npm ci` must run BEFORE the Rust build because it creates the `node_modules/mailcore-napi` symlink. The Rust build only needs to produce the `.node` binary before the Electron packager runs in step 5.

### Pattern 2: Windows CI Steps (MSYS2 + GNU Target)

```yaml
# Step A: MSYS2 environment for dlltool.exe
- name: Setup MSYS2 (for GNU toolchain dlltool.exe)
  uses: msys2/setup-msys2@v2
  with:
    msystem: MINGW64
    install: mingw-w64-x86_64-toolchain

# Step B: Generate libnode.dll import library (one-time per Node.js version)
- name: Generate libnode.dll import library
  shell: msys2 {0}
  run: |
    NODE_EXE=$(cygpath "${{ runner.tool_cache }}/node/20.*/x64/node.exe" | head -1)
    gendef "$NODE_EXE"
    dlltool --dllname node.exe --def node.def --output-lib /tmp/libnode.dll
    echo "LIBNODE_PATH=/tmp" >> $GITHUB_ENV

# Step C: Rust toolchain with GNU target
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: x86_64-pc-windows-gnu

# Step D: Cargo cache
- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: win-x64-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

# Step E: Build the addon
- name: Build napi-rs addon (Windows x64 GNU)
  shell: msys2 {0}
  working-directory: app/mailcore-rs
  env:
    LIBNODE_PATH: ${{ env.LIBNODE_PATH }}
    PATH: /mingw64/bin:${{ env.PATH }}
  run: |
    npx @napi-rs/cli build --release --target x86_64-pc-windows-gnu
```

**Why this complexity:** `napi-build` (the Rust build script) requires `dlltool.exe` (from MSYS2 MinGW64) and a `libnode.dll` import library at `LIBNODE_PATH`. Windows GitHub runners (`windows-2022`) do not have dlltool.exe by default. The `start-dev.js` script handles this locally by prepending `C:\msys64\mingw64\bin` to PATH.

### Pattern 3: Linux x64 CI Steps (Native — Simplest Case)

```yaml
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: x86_64-unknown-linux-gnu

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: linux-x64-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

- name: Build napi-rs addon (Linux x64)
  working-directory: app/mailcore-rs
  run: npx @napi-rs/cli build --release --target x86_64-unknown-linux-gnu

- name: Check binary size (SCAF-04 gate — Linux x64 only)
  working-directory: app/mailcore-rs
  run: |
    BINARY=$(ls mailcore-napi-rs.linux-x64-gnu.node 2>/dev/null || ls *.node | head -1)
    SIZE_KB=$(du -k "$BINARY" | cut -f1)
    SIZE_MB=$((SIZE_KB / 1024))
    echo "Binary size: ${SIZE_KB}KB (${SIZE_MB}MB)"
    if [ "$SIZE_KB" -gt 8192 ]; then
      echo "FAIL: Binary ${SIZE_KB}KB exceeds 8192KB (8MB) limit"
      echo "Run cargo-bloat to identify largest contributors:"
      echo "  cargo install cargo-bloat && cargo bloat --release -n 10"
      exit 1
    fi
    echo "PASS: Binary is within 8MB limit"
```

### Pattern 4: macOS CI — Arch-Conditional Rust Target

```yaml
# In existing build-macos.yaml matrix job:
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: ${{ matrix.arch == 'arm64' && 'aarch64-apple-darwin' || 'x86_64-apple-darwin' }}

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: mac-${{ matrix.arch }}-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

- name: Build napi-rs addon (macOS ${{ matrix.arch }})
  working-directory: app/mailcore-rs
  run: |
    TARGET=${{ matrix.arch == 'arm64' && 'aarch64-apple-darwin' || 'x86_64-apple-darwin' }}
    npx @napi-rs/cli build --release --target "$TARGET"
```

### Anti-Patterns to Avoid

- **Do NOT use x86_64-pc-windows-msvc on Windows:** The project's established toolchain is GNU (documented in README.md and start-dev.js). Using MSVC would require Visual Studio Build Tools and would produce a binary incompatible with the custom `loader.js` BINARY_MAP entry which expects the `-gnu.node` suffix.
- **Do NOT use `--use-napi-cross` or cargo-zigbuild for these targets:** The 5 targets all have native runners available. Cross-compilation is unnecessary complexity.
- **Do NOT put the napi-rs `.node` file inside the asar archive:** `*.node` files require real filesystem paths for `dlopen()`. The existing `*.node` asar unpack glob already handles this.
- **Do NOT delete `app/mailsync/` or `app/mailsync.exe`/`app/mailsync.bin`:** The C++ sync engine is NOT being replaced in Phase 4. Only the N-API addon (`app/mailcore/`) is removed.
- **Do NOT update `mailcore-rs/package.json` "main" to point at the generated `index.js`:** The generated `index.js` has the flawed shlib_suffix detection. The `"main"` should point at `loader.js` (the custom hand-written loader). Alternatively, if the generated `index.js` is overwritten by the CI build anyway, the custom `loader.js` should replace it — but verify this.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Platform binary naming | Custom naming script | `napi build --platform` | napi-rs auto-generates `<name>.<os>-<arch>-<libc>.node` |
| LTO + strip pipeline | Shell post-processing | `[profile.release]` in Cargo.toml | Cargo handles LTO before strip in correct order |
| Binary size check | Custom reporting | `du -k` + shell arithmetic | Simple; no tooling needed |
| Rust toolchain installation | curl \| sh rustup | `dtolnay/rust-toolchain@stable` | CI-safe, cached, reproducible |
| dlltool.exe on Windows | Bundle it | `msys2/setup-msys2@v2` with MINGW64 | Standard MSYS2 action provides the full MinGW64 toolchain |
| Binary size investigation | Manual nm/readelf | `cargo-bloat --release -n 10` | Shows ranked function-level contributors in < 2 minutes |

## Common Pitfalls

### Pitfall 1: Windows Builds Fail — Wrong Target or Missing dlltool
**What goes wrong:** Using `x86_64-pc-windows-msvc` (the napi-rs default) instead of `x86_64-pc-windows-gnu` causes a completely different build setup. Or the GNU target is used but `dlltool.exe` is not in PATH, causing `napi-build` to fail during the Rust compile step.
**Why it happens:** napi-rs documentation defaults show MSVC. The project explicitly uses GNU (established in Phase 1 due to missing Visual Studio in the dev environment). The `napi-build` crate calls `dlltool.exe` to create import libraries — this tool only exists in MSYS2 MinGW64.
**How to avoid:** Use `msys2/setup-msys2@v2` with `msystem: MINGW64`, set PATH to include `/mingw64/bin`, generate `libnode.dll` via `gendef + dlltool`, set `LIBNODE_PATH`. Follow the pattern in `app/mailcore-rs/README.md` Windows CI section exactly.
**Warning signs:** CI error `dlltool.exe: program not found` or `LIBNODE_PATH not set`.

### Pitfall 2: mailcore-rs/package.json "main" Points at Wrong Loader
**What goes wrong:** After the mailcore-wrapper is removed, `require('mailcore-napi')` resolves to `app/mailcore-rs/` and reads `package.json` `"main": "index.js"`. The `index.js` is the large auto-generated napi-rs loader with broken shlib_suffix detection. On MSVC Node.js (standard Windows), it tries to load `-msvc.node` instead of the `-gnu.node` that was built.
**Why it happens:** The `app/mailcore-rs/package.json` currently says `"main": "index.js"`. The hand-written custom loader is in `loader.js`, not `index.js`. The wrapper previously bypassed this by calling `require('../mailcore-rs/loader.js')` directly.
**How to avoid:** Update `app/mailcore-rs/package.json` `"main"` from `"index.js"` to `"loader.js"`. Alternatively, overwrite `index.js` with the content of `loader.js` and delete `loader.js`. The planner should pick one approach.
**Warning signs:** Windows app fails with "Cannot find module mailcore-napi" or loads wrong .node binary.

### Pitfall 3: postinstall.js Still Checks app/mailcore/ Path
**What goes wrong:** `scripts/postinstall.js` checks for `app/mailcore/build/Release/mailcore_napi.node` at line 129 and emits a "Note: mailcore-napi addon not built (optional)" message. After deleting `app/mailcore/`, this check must be updated to avoid noise (or removed, since Phase 4 removes all C++ addon references).
**Why it happens:** The postinstall script was written for the old C++ addon workflow.
**How to avoid:** Update `scripts/postinstall.js` to check for `app/mailcore-rs/loader.js` (or any `.node` file in `app/mailcore-rs/`) instead. The Rust addon is always built automatically by `npm start`, so the check may become unnecessary.
**Warning signs:** npm install always shows "Note: mailcore-napi addon not built" even when the Rust addon is present.

### Pitfall 4: C++ References Left in package.json Causing npm ci Failure
**What goes wrong:** Deleting `app/mailcore/` while `app/package.json` still has `"mailcore-napi": "file:mailcore-wrapper"` causes `npm ci` to fail (or vice versa).
**Why it happens:** The deletion and package.json update must happen atomically (in the same commit).
**How to avoid:** In a single commit: (1) update `app/package.json` optionalDependencies from `file:mailcore-wrapper` to `file:mailcore-rs`, (2) delete `app/mailcore-wrapper/`, (3) delete `app/mailcore/`, (4) remove `node-gyp` from root `package.json`. Then run `npm ci` to verify.
**Warning signs:** `npm ERR! enoent Could not read package.json: app/mailcore-wrapper/package.json` or `npm ERR! No such file or directory: app/mailcore`.

### Pitfall 5: .node File Inside ASAR Archive
**What goes wrong:** Native `.node` files cannot be executed from inside a `.asar` archive (ASAR is a virtual filesystem; `dlopen()` requires a real filesystem path).
**Why it happens:** `@electron/packager` puts all files in the asar by default.
**How to avoid:** The existing `build/tasks/package-task.js` already has `*.node` in the `asar.unpack` glob (verified at line 167). The Rust `.node` binary will be placed in `app.asar.unpacked/node_modules/mailcore-napi/` automatically.
**Warning signs:** `Error: Cannot find module 'mailcore-napi'` at Electron startup after packaging.

### Pitfall 6: Linux C++ Packages Over-Trimmed
**What goes wrong:** Removing ALL C++ packages from the `apt-get install` list breaks Electron's own dependencies.
**Why it happens:** It's easy to trim too aggressively. The Electron app itself needs some system libraries.
**How to avoid:** Remove only the packages exclusively needed by the C++ addon and build tooling. See the Keep/Remove list in the CI Step Maps section.
**Warning signs:** Electron fails to start in CI with `libXXX.so: No such file or directory`.

### Pitfall 7: Binary Exceeds 8MB
**What goes wrong:** CI fails the size gate. tokio + rustls + async-imap + lettre + hickory-resolver is a substantial dependency tree.
**Why it happens:** Default release profile optimizes for speed; size requires explicit configuration.
**How to avoid:** Add all four Cargo.toml entries: `codegen-units = 1`, `panic = "abort"`, `opt-level = "z"`, `lto = true`. These are additive to the existing `lto = true` + `strip = "symbols"` already in Cargo.toml.
**Investigation with cargo-bloat:** If still over 8MB: `cargo install cargo-bloat && cargo bloat --release -n 10` — shows largest contributing functions. Common culprits: regex (prune features), rustls cert store, unused protocol implementations.
**Warning signs:** CI size check fails; binary is 9–12MB unstripped.

## Code Examples

### Full Cargo.toml Release Profile (with all required additions)

```toml
# Source: CONTEXT.md locked decisions + official napi-rs/tar Cargo.toml
[profile.release]
lto = true           # already present — link-time optimization
strip = "symbols"    # already present — strip debug symbols
codegen-units = 1    # ADD — enables cross-module inlining, reduces size
panic = "abort"      # ADD — removes unwinding code (~10-20% size reduction)
opt-level = "z"      # ADD — optimize for size (try "z", fall back to "s")
```

### build-linux.yaml: Rust Steps to Insert (After "Install Dependencies", Before "Lint")

```yaml
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: x86_64-unknown-linux-gnu

- name: Cache cargo registry and build
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: linux-x64-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}
    restore-keys: |
      linux-x64-cargo-

- name: Build napi-rs addon (Linux x64)
  working-directory: app/mailcore-rs
  run: npx @napi-rs/cli build --release --target x86_64-unknown-linux-gnu

- name: Check binary size (SCAF-04 — Linux x64 only)
  working-directory: app/mailcore-rs
  run: |
    BINARY=$(ls *.node | head -1)
    SIZE_KB=$(du -k "$BINARY" | cut -f1)
    echo "Binary: $BINARY — size: ${SIZE_KB}KB"
    if [ "$SIZE_KB" -gt 8192 ]; then
      echo "FAIL: ${SIZE_KB}KB exceeds 8192KB (8MB) limit. Investigate with cargo-bloat."
      exit 1
    fi
    echo "PASS: Within 8MB limit"
```

### build-linux.yaml: System Deps to Remove

**Remove these packages** (C++ addon only, not needed by Electron or Rust):
```
autoconf automake clang cmake execstack
libctemplate-dev libcurl4-openssl-dev libicu-dev
libsasl2-dev libsasl2-modules libsasl2-modules-gssapi-mit
libssl-dev libtidy-dev libtool libxml2-dev
```

**Keep these packages** (needed by Electron build or app):
```
build-essential fakeroot git libc-ares-dev
libglib2.0-dev libsecret-1-dev libnss3 libnss3-dev
libxext-dev libxkbfile-dev libxtst-dev pkg-config
rpm software-properties-common uuid-dev xvfb
```

### build-windows.yaml: Steps to Remove and Replace

**Remove steps 6–12** (C++ build section, between "Install Dependencies" and "Lint"):
- Setup vcpkg (lukka/run-vcpkg@v11)
- Install vcpkg dependencies (x64)
- Generate mailcore2 public headers
- Build libetpan (Release|x64)
- Build mailcore2 (Release|x64)
- Build mailsync (Release|x64)
- Copy mailsync binaries to Electron resources

**Replace with Rust steps** (insert between "Install Dependencies" and "Lint"):
```yaml
- name: Setup MSYS2 (provides dlltool.exe for GNU target)
  uses: msys2/setup-msys2@v2
  with:
    msystem: MINGW64
    install: mingw-w64-x86_64-toolchain

- name: Generate libnode.dll import library
  shell: msys2 {0}
  run: |
    # Find node.exe from actions/setup-node
    NODE_EXE=$(which node | xargs cygpath -w)
    gendef "$NODE_EXE"
    dlltool --dllname node.exe --def node.def --output-lib /tmp/libnode.dll
    echo "LIBNODE_PATH=$(cygpath -w /tmp)" >> $GITHUB_ENV

- name: Setup Rust toolchain (Windows GNU)
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: x86_64-pc-windows-gnu

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: win-x64-gnu-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

- name: Build napi-rs addon (Windows x64 GNU)
  shell: msys2 {0}
  working-directory: app/mailcore-rs
  env:
    LIBNODE_PATH: ${{ env.LIBNODE_PATH }}
  run: |
    export PATH="/mingw64/bin:$PATH"
    npx @napi-rs/cli build --release --target x86_64-pc-windows-gnu
```

### app/package.json optionalDependency Change

```json
// Before: pointing to mailcore-wrapper intermediary
"optionalDependencies": {
  "mailcore-napi": "file:mailcore-wrapper",
  "macos-notification-state": "github:bengotow/macos-notification-state#77e9e825"
}

// After: pointing directly to Rust addon
"optionalDependencies": {
  "mailcore-napi": "file:mailcore-rs",
  "macos-notification-state": "github:bengotow/macos-notification-state#77e9e825"
}
```

### mailcore-rs/package.json "main" Field Fix

```json
// Before: points to auto-generated loader with broken shlib_suffix detection
{
  "name": "mailcore-napi-rs",
  "main": "index.js",
  ...
}

// After: points to hand-written loader that correctly handles GNU .node in MSVC Node.js
{
  "name": "mailcore-napi-rs",
  "main": "loader.js",
  ...
}
```

**Or alternatively:** Rename `loader.js` to `index.js` and delete the auto-generated `index.js`. Both approaches achieve the same result. The planner should choose one and document it.

**Note on package name:** `app/mailcore-rs/package.json` currently has `"name": "mailcore-napi-rs"`. The `mailcore-wrapper/package.json` has `"name": "mailcore-napi"`. After the switch, `require('mailcore-napi')` resolves to the mailcore-rs directory via npm's `file:` protocol and the `package.json` "name" field is irrelevant for module resolution — it's the directory path that matters. However, the `name` field should ideally be updated to `"mailcore-napi"` for clarity.

### Smoke Test (Node.js script — inline in workflow or separate file)

```javascript
// smoke-test.js — verify mailcore-napi loads and providerForEmail works
'use strict';
const addon = require('mailcore-napi');
if (typeof addon.providerForEmail !== 'function') {
  console.error('FAIL: providerForEmail is not a function');
  process.exit(1);
}
const result = addon.providerForEmail('test@gmail.com');
if (!result || typeof result !== 'object') {
  console.error('FAIL: providerForEmail("test@gmail.com") returned null or non-object');
  process.exit(1);
}
console.log('PASS: mailcore-napi loaded, providerForEmail returned:', JSON.stringify(result.label || result));
```

**In workflow:**
```yaml
- name: Smoke test — verify mailcore-napi loads
  working-directory: app
  run: node -e "
    const addon = require('mailcore-napi');
    const result = addon.providerForEmail('test@gmail.com');
    if (!result) { console.error('FAIL: no result'); process.exit(1); }
    console.log('PASS:', result.label || JSON.stringify(result));
  "
```

### C++ Deletion Verification Script (run after deletion, before PR)

```bash
echo "=== Verifying C++ artifact deletion ==="
[ -d app/mailcore ] && echo "FAIL: app/mailcore/ still exists" || echo "PASS: app/mailcore/ deleted"
[ -d app/mailcore-wrapper ] && echo "FAIL: app/mailcore-wrapper/ still exists" || echo "PASS: app/mailcore-wrapper/ deleted"
grep -q '"mailcore-napi": "file:mailcore-wrapper"' app/package.json && echo "FAIL: file:mailcore-wrapper reference" || echo "PASS: wrapper reference removed"
grep -q 'node-gyp' package.json && echo "FAIL: node-gyp in root package.json" || echo "PASS: node-gyp removed"
grep -qr 'node-addon-api' app/package.json package.json && echo "FAIL: node-addon-api reference" || echo "PASS: node-addon-api removed"
grep -q '"mailcore-napi": "file:mailcore-rs"' app/package.json && echo "PASS: app/package.json points to file:mailcore-rs" || echo "FAIL: app/package.json not updated"
echo "=== Done ==="
```

## CI Workflow Step Maps (Complete Reference)

### build-linux.yaml — Full Step Map

**Job:** `build` on `ubuntu-22.04`

| Step | Name | Action | Keep/Modify/Remove |
|------|------|--------|--------------------|
| 1 | Fail if not master | bash guard | KEEP |
| 2 | Checkout | actions/checkout@v4 | KEEP |
| 3 | Install system deps | apt-get install 24 packages | MODIFY — remove C++ packages listed above |
| 4 | Setup Node.js | actions/setup-node@v4, node 20 | KEEP |
| 5 | Install Dependencies | npm ci | KEEP |
| **INSERT** | Setup Rust toolchain | dtolnay/rust-toolchain@stable | ADD |
| **INSERT** | Cache cargo | actions/cache@v4 | ADD |
| **INSERT** | Build napi-rs addon | napi build linux-x64 | ADD |
| **INSERT** | Check binary size | du -k + size gate | ADD (Linux x64 only) |
| 6 | Lint | npm run lint | KEEP |
| 7 | Build | DEBUG=electron-packager npm run build | KEEP |
| 8 | Create/Update Release | softprops/action-gh-release@v1 | KEEP |
| 9 | Upload DEB artifact | actions/upload-artifact@v4 | KEEP |
| 10 | Upload RPM artifact | actions/upload-artifact@v4 | KEEP |

**Post-build jobs** (`build-snap`, `test-ubuntu`, `test-fedora`, `test-opensuse`, `test-arch`, `test-linuxmint`) follow `needs: build` — no changes needed.

### build-linux-arm64.yaml — Full Step Map

**Job:** `build` on `ubuntu-24.04-arm`

Identical to build-linux.yaml with two differences:
- Runner is `ubuntu-24.04-arm` (native ARM64) — no cross-compilation
- No `execstack` in system deps (not available on ARM64, not needed)
- Rust target: `aarch64-unknown-linux-gnu` (no `--use-napi-cross`)
- No size gate step (size check is Linux x64 only)

| Step | Name | Keep/Modify/Remove |
|------|------|-------------------|
| 1 | Fail if not master | KEEP |
| 2 | Checkout | KEEP |
| 3 | Install system deps (23 packages, no execstack) | MODIFY — same C++ removals |
| 4 | Setup Node.js | KEEP |
| 5 | Install Dependencies | KEEP |
| **INSERT** | Setup Rust toolchain (aarch64) | ADD |
| **INSERT** | Cache cargo (arm64 key) | ADD |
| **INSERT** | Build napi-rs addon (native arm64) | ADD |
| 6 | Lint | KEEP |
| 7 | Build | KEEP |
| 8–10 | Release + artifacts | KEEP |

### build-macos.yaml — Full Step Map

**Job:** `build-macos` with 2-entry matrix (`macos-latest` arm64, `macos-15-intel` x64)

| Step | Name | Keep/Modify/Remove |
|------|------|-------------------|
| 1 | Fail if not main | KEEP |
| 2 | Checkout Repo | KEEP |
| 3 | Cache NodeJS modules | KEEP (stale yarn.lock key — pre-existing, not Phase 4 scope) |
| 4 | Install Dependencies | KEEP |
| 5 | Setup Codesigning | KEEP |
| **INSERT** | Setup Rust toolchain (arch-conditional) | ADD |
| **INSERT** | Cache cargo (mac-{arch} key) | ADD |
| **INSERT** | Build napi-rs addon (arch-specific target) | ADD |
| 6 | Lint | KEEP |
| 7 | Build (SIGN_BUILD=true) | KEEP |
| 8 | Rename artifacts (arm64 only) | KEEP |
| 9 | Generate latest-mac.yml | KEEP |
| 10 | Create/Update Release | KEEP |

**macOS code signing note:** The existing `SIGN_BUILD=true` flow signs the full `.app` bundle including the `.node` file inside it. No additional signing steps needed for the Rust addon. Azure Trusted Signing is Windows-only.

### build-windows.yaml — Full Step Map

**Job:** `build` on `windows-2022`

| Step | Name | Keep/Modify/Remove |
|------|------|-------------------|
| 1 | Fail if not main | KEEP |
| 2 | Checkout Repo | KEEP |
| 3 | Cache NodeJS modules | KEEP |
| 4 | Setup Node.js | KEEP |
| 5 | Install Dependencies | KEEP |
| 6 | Setup vcpkg | **REMOVE** |
| 7 | Install vcpkg dependencies (x64) | **REMOVE** |
| 8 | Generate mailcore2 public headers | **REMOVE** |
| 9 | Build libetpan (Release\|x64) | **REMOVE** |
| 10 | Build mailcore2 (Release\|x64) | **REMOVE** |
| 11 | Build mailsync (Release\|x64) | **REMOVE** |
| 12 | Copy mailsync binaries | **REMOVE** |
| **INSERT** | Setup MSYS2 (MINGW64) | ADD |
| **INSERT** | Generate libnode.dll | ADD |
| **INSERT** | Setup Rust toolchain (x86_64-pc-windows-gnu) | ADD |
| **INSERT** | Cache cargo (win-x64-gnu key) | ADD |
| **INSERT** | Build napi-rs addon (Windows GNU) | ADD |
| 13 | Lint | KEEP |
| 14 | Build | KEEP |
| 15 | Sign Application Files (Azure Trusted Signing) | KEEP — `files-folder-filter: exe,dll,node` already includes .node |
| 16 | Create Windows Installer | KEEP |
| 17 | Sign Windows Installer | KEEP |
| 18 | Create Release | KEEP |

**Azure signing note (step 15):** The `files-folder-filter: exe,dll,node` already includes `.node` extension. The Rust addon binary will be automatically signed without any workflow change.

## asar Packaging — Verified Chain

**Step 1: app/package.json declares file:mailcore-rs dependency**
```json
"optionalDependencies": {
  "mailcore-napi": "file:mailcore-rs"
}
```
`npm ci` creates symlink: `app/node_modules/mailcore-napi` → `app/mailcore-rs/`

**Step 2: napi-rs produces platform-named binary**
`napi build --release --target x86_64-unknown-linux-gnu` produces:
`app/mailcore-rs/mailcore-napi-rs.linux-x64-gnu.node`

**Step 3: asar unpack glob catches it (verified in package-task.js line 167)**
```javascript
asar: {
  unpack: '{mailsync,mailsync.exe,mailsync.bin,*.so,*.so.*,*.dll,*.pdb,*.node,...}',
}
```
`*.node` is evaluated recursively across the app directory. The `.node` binary under `node_modules/mailcore-napi/` is caught and placed in `app.asar.unpacked/`.

**Step 4: Runtime resolution in packaged Electron app**
```
require('mailcore-napi')
  → asar FS: app.asar/node_modules/mailcore-napi/loader.js (after package.json "main" fix)
  → loader.js: BINARY_MAP['linux-x64'] = 'mailcore-napi-rs.linux-x64-gnu.node'
  → Node.js: .node extension → redirect to app.asar.unpacked/node_modules/mailcore-napi/mailcore-napi-rs.linux-x64-gnu.node
  → dlopen() succeeds (physically on disk)
```

**This works automatically** once: (1) package.json points at `file:mailcore-rs`, (2) package.json "main" is `loader.js`, (3) the `.node` binary is built before packaging.

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| node-gyp + binding.gyp (C++) | napi-rs v3 `napi build` | napi-rs v2→v3, 2023-2024 | Cross-compilation without system headers; auto-generated TS types |
| Per-platform optional npm packages | Single package with .node in local dir | Known since node-rs issue #376, 2021 | Eliminates Electron architecture mismatch |
| QEMU for ARM64 CI | Native `ubuntu-24.04-arm` runner | GA September 2024 | 10x faster ARM64 CI without emulation |
| `lto = "thin"` | `lto = true` (= "fat") | Ongoing | Fat LTO gives better size reduction; tradeoff is compile time |
| `strip = true` (Cargo 1.69+) | `strip = "symbols"` (explicit) | Cargo 1.69, 2023 | Both work; "symbols" is more explicit |

**Deprecated/outdated:**
- `node-addon-api` / `node-gyp`: Replaced by napi-rs; requires no system C++ headers or build tools
- `electron-rebuild`: Not needed for napi-rs addons (stable N-API ABI = no rebuild per Electron version)
- `x86_64-pc-windows-msvc` for this project: Project uses GNU toolchain per Phase 1 decision; MSVC would require Visual Studio Build Tools not present in this dev environment

## Open Questions

1. **Should mailcore-rs/package.json "main" be "loader.js" or "index.js"?**
   - What we know: `index.js` is auto-generated with broken shlib_suffix detection; `loader.js` is the hand-written fix
   - What's unclear: Whether CI's `napi build` overwrites `index.js` after each build (if so, the committed `index.js` is irrelevant at runtime)
   - Recommendation: Change `"main"` to `"loader.js"` in `package.json`. This is explicit and safe regardless of whether `napi build` regenerates `index.js`. Then `index.js` can optionally be removed to avoid confusion.

2. **Node.js path for gendef on windows-2022 runner**
   - What we know: Node.js is installed by `actions/setup-node@v4` at some path under `${{ runner.tool_cache }}/node/`
   - What's unclear: Exact path format when accessed from MSYS2 shell (POSIX vs Windows path conversion needed)
   - Recommendation: In the workflow, use `$(which node)` from the MSYS2 shell context after ensuring Node.js is in PATH. Test this step in isolation first.

3. **Does `npm ci` after C++ deletion need to be run with `--ignore-scripts`?**
   - What we know: `postinstall.js` checks for `app/mailcore/build/Release/mailcore_napi.node` and emits a warning if missing; this is harmless but noisy
   - What's unclear: Whether postinstall.js has any hard failures that block `npm ci` after deletion
   - Recommendation: Update `postinstall.js` to remove the C++ addon check (or update it to check for the Rust addon) as part of Phase 4 cleanup. This prevents any postinstall.js noise from being misread as an error.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Jasmine 2.x (existing) + cargo test (Rust) |
| Config file | None for CI smoke test (inline script) |
| Quick run command | `node -e "require('mailcore-napi').providerForEmail('test@gmail.com')"` |
| Full suite command | `cd app/mailcore-rs && cargo test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| SCAF-03 | CI produces .node binaries for all 5 targets | CI pass/fail | All 4 workflows complete successfully | ❌ — determined by CI execution |
| SCAF-04 | Linux x64 binary < 8MB | automated size gate | `du -k *.node \| awk '$1>8192{exit 1}'` | ❌ Wave 0 — added to workflow |
| INTG-01 | onboarding-helpers.ts loads via require('mailcore-napi') | smoke | `node -e "require('mailcore-napi').providerForEmail('test@gmail.com')"` | ❌ Wave 0 |
| INTG-02 | mailsync-process.ts loads via require path | smoke | same as INTG-01 (same require path) | ❌ Wave 0 |
| INTG-03 | C++ files deleted | manual verification | C++ deletion check script | ❌ Wave 0 |
| INTG-04 | node-addon-api + node-gyp removed | automated | `grep -v node-gyp package.json && grep -v node-addon-api app/package.json` | ❌ Wave 0 |

### Wave 0 Gaps
- [ ] Size gate step in `build-linux.yaml` — covers SCAF-04
- [ ] Smoke test step in each workflow — covers INTG-01, INTG-02
- [ ] C++ deletion verification script — covers INTG-03, INTG-04

## Sources

### Primary (HIGH confidence)
- Project files read directly: `build-linux.yaml`, `build-linux-arm64.yaml`, `build-macos.yaml`, `build-windows.yaml`, `build/tasks/package-task.js`, `app/package.json`, `package.json` (root), `app/mailcore/package.json`, `app/mailcore-wrapper/index.js`, `app/mailcore-wrapper/package.json`, `app/mailcore-rs/Cargo.toml`, `app/mailcore-rs/package.json`, `app/mailcore-rs/index.js`, `app/mailcore-rs/loader.js`, `app/mailcore-rs/README.md`, `scripts/postinstall.js`, `scripts/start-dev.js`
- `04-CONTEXT.md` — locked decisions for this phase (Phase 3 complete as of 2026-03-04)
- [napi-rs build CLI docs](https://napi.rs/docs/cli/build) — `--target`, `--platform`, `--release` flags confirmed
- [dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain) — `targets:` input for cross-compilation targets confirmed
- [min-sized-rust](https://github.com/johnthagen/min-sized-rust) — authoritative Rust binary size reference; all Cargo profile options verified

### Secondary (MEDIUM confidence)
- [napi-rs cross-build docs](https://napi.rs/docs/cross-build) — confirms `--use-napi-cross` is for cross-compilation; not needed for native runners
- [GitHub Actions ARM64 GA](https://github.blog/changelog/2024-09-03-github-actions-arm64-linux-and-windows-runners-are-now-generally-available/) — confirms `ubuntu-24.04-arm` availability
- [actions/cache v4 documentation](https://github.com/actions/cache/tree/v4) — cargo cache key pattern with `hashFiles('**/Cargo.lock')`
- [msys2/setup-msys2](https://github.com/msys2/setup-msys2) — setup action for MSYS2/MINGW64 in GitHub Actions
- [napi-rs support x86_64-pc-windows-gnu issue #2001](https://github.com/napi-rs/napi-rs/issues/2001) — documents GNU target support and dlltool requirement
- [electron/packager asar.unpack documentation](https://electron.github.io/packager/main/interfaces/Options.html) — `*.node` glob behavior confirmed

### Tertiary (LOW confidence)
- Binary size estimates (5–8MB range) — assembled from comparable napi-rs package sizes on npm; no direct measurement of this specific dependency stack available without building

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — all platform targets match existing project toolchain decisions; verified from source files
- Windows GNU CI steps: MEDIUM — MSYS2 step structure is well-documented; exact node.exe path in runner.tool_cache needs verification during implementation
- Architecture patterns: HIGH — asar unpack chain traced end-to-end through source files; require chain before/after documented from actual code
- Binary size: MEDIUM — estimated from comparable packages; actual size only known after Phase 3 deps are compiled with full release profile
- C++ deletion scope: HIGH — all files verified by direct inspection; nothing in app/mailsync/ needed by Phase 4

**Research date:** 2026-03-03
**Updated from:** 2026-03-02 original research (corrected Windows target from MSVC to GNU; added user_constraints; updated wrapper removal details to reflect Phase 3 completion state)
**Valid until:** 2026-09-03 (napi-rs and GitHub Actions platform availability are stable; `macos-15-intel` runner label may change)
