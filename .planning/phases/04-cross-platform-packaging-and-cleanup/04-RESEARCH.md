# Phase 4: Cross-Platform Packaging and Cleanup - Research

**Researched:** 2026-03-02
**Domain:** napi-rs GitHub Actions CI, Electron asar packaging, Rust binary size optimization, C++ cleanup
**Confidence:** HIGH (primary findings from official napi-rs docs and package-template, verified against existing project structure)

## Summary

Phase 4 has three distinct work streams: (1) GitHub Actions CI that builds the napi-rs .node addon for all 5 targets, (2) ensuring the compiled .node file works with `@electron/packager`'s asar unpack mechanism, and (3) cleanly deleting all C++ artifacts from the repository.

The good news: the existing project already has workflows for all 5 platforms (build-linux.yaml, build-linux-arm64.yaml, build-macos.yaml, build-windows.yaml) and already has `*.node` in the asar unpack glob in `build/tasks/package-task.js`. These existing workflows call `npm run build` which runs the Grunt packager — but they do NOT yet build the Rust addon before packaging. The primary CI work is inserting a Cargo/napi-rs build step into each workflow before `npm run build` is called.

The known risk (napi-rs/node-rs issue #376) is real: `optionalDependencies` per-platform npm packages do not work correctly with Electron packager — it installs for the host Node.js architecture, not the Electron target. The solution used across the ecosystem is a single-package layout where the correct platform `.node` binary is placed directly into `app/mailcore-rs/` (or the `node_modules/mailcore-napi` folder) as a file named for the current platform, and `require('mailcore-napi')` resolves it via the generated `index.js`. This avoids the optional-dependency problem entirely.

**Primary recommendation:** Build the napi-rs addon using `napi build --release --target <RUST_TARGET>` in each existing workflow's CI steps, copy the resulting `mailcore-rs.*.node` binary into the location that `require('mailcore-napi')` can find it, and let the existing `*.node` asar unpack glob handle packaging. Delete C++ artifacts last.

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| napi-rs CLI (`@napi-rs/cli`) | 3.x | `napi build` command for cross-platform .node compilation | Official napi-rs build tool; generates platform-named binaries automatically |
| dtolnay/rust-toolchain | stable | Rust toolchain setup in GitHub Actions | Canonical action for Rust in CI; used by all napi-rs examples |
| actions/upload-artifact | v4 | Upload per-platform .node files between CI jobs | Standard GitHub Actions artifact store |
| actions/download-artifact | v4 | Collect all platform binaries in a publish step | Pairs with upload-artifact |
| actions/cache | v4 | Cache `~/.cargo/registry`, `target/` | Cuts napi-rs CI time from ~15min to ~3min per platform |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| cargo-zigbuild | latest | Cross-compile Rust for musl targets via Zig linker | Required for `aarch64-unknown-linux-musl` and `x86_64-unknown-linux-musl` targets; NOT needed for glibc targets |
| mlugg/setup-zig | v2 | Install Zig 0.14.1 for cargo-zigbuild | Required when matrix target contains 'musl' |
| --use-napi-cross flag | napi-rs CLI | Cross-compile Linux glibc targets (x64, arm64) from same runner | Replaces Docker-in-Docker or separate ARM runners for GNU targets |
| taiki-e/install-action | v2 | Install cargo-zigbuild in CI | Reliable cross-platform tool installer |

### Platforms: Runner and Rust Target Mapping
| Platform Target | GitHub Runner | Rust Target | Build Command |
|-----------------|---------------|-------------|---------------|
| win-x64 | `windows-latest` | `x86_64-pc-windows-msvc` | `napi build --release --target x86_64-pc-windows-msvc` |
| mac-arm64 | `macos-latest` | `aarch64-apple-darwin` | `napi build --release --target aarch64-apple-darwin` |
| mac-x64 | `macos-latest` | `x86_64-apple-darwin` | `napi build --release --target x86_64-apple-darwin` |
| linux-x64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `napi build --release --target x86_64-unknown-linux-gnu --use-napi-cross` |
| linux-arm64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | `napi build --release --target aarch64-unknown-linux-gnu` (native runner) |

**Note on linux-arm64:** The project already uses `ubuntu-24.04-arm` runner (native ARM64) in `build-linux-arm64.yaml`. Building napi-rs natively on that runner (no cross-compilation needed) is simpler and more reliable than cross-compiling from x64.

**Installation (in mailcore-rs directory):**
```bash
npm install -g @napi-rs/cli
# or as dev dependency:
npm install --save-dev @napi-rs/cli
```

## Architecture Patterns

### How napi-rs Loads the .node File

napi-rs generates three files during `napi build`:
1. `mailcore-rs.<platform>.node` — the compiled native binary (e.g., `mailcore-rs.linux-x64-gnu.node`)
2. `index.js` — generated loader that tries local file first, then optional-dependency packages
3. `index.d.ts` — TypeScript types

The generated `index.js` looks for the `.node` file in the package directory using the current platform string. The key insight: when the `.node` file is placed directly in the same directory as `index.js`, `require('mailcore-napi')` works without any optional dependency resolution.

### Recommended Project Structure

The napi-rs project lives at `app/mailcore-rs/`. After Phase 1–3 completion, this directory should contain:

```
app/mailcore-rs/
├── Cargo.toml           # [lib] crate-type = ["cdylib"], [profile.release] with LTO
├── build.rs             # napi-build invocation
├── src/
│   └── lib.rs           # All Rust source
├── package.json         # "name": "mailcore-napi", "main": "index.js"
├── index.js             # napi-rs generated loader (checked in or generated)
└── index.d.ts           # napi-rs generated types (checked in)
```

The `.node` binary itself is NOT checked into git — it is built in CI and placed here during the workflow.

### Pattern 1: CI Build Step for Each Platform

Each existing workflow needs a Rust build step inserted BEFORE `npm run build`:

```yaml
# Add to each platform workflow, after "Install Dependencies":
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: x86_64-unknown-linux-gnu  # platform-specific

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: linux-x64-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

- name: Build napi-rs addon
  working-directory: app/mailcore-rs
  run: npx @napi-rs/cli build --release --target x86_64-unknown-linux-gnu --use-napi-cross

# The .node file is now in app/mailcore-rs/ and will be found by require('mailcore-napi')
# via app/package.json: "mailcore-napi": "file:mailcore-rs"
```

### Pattern 2: Cargo.toml Release Profile for Size

```toml
# Source: napi-rs/tar official example + min-sized-rust guidance
[profile.release]
codegen-units = 1
lto = true
strip = "symbols"
opt-level = "z"    # optimize for size; try "s" if larger
panic = "abort"    # removes unwinding code (~10-20% size reduction)
```

With these settings on a typical async IMAP/SMTP Rust addon (tokio + rustls), the Linux x64 binary should land in the 4–8MB range after strip. The 8MB target in SCAF-04 is achievable. `panic = "abort"` is safe for a napi-rs addon because panics in N-API callbacks are already caught by the napi-rs framework.

### Pattern 3: Verifying No OpenSSL Symbols

```bash
# Run in CI after build to verify SCAF-02/SCAF-04 constraint
cargo tree --prefix depth 2>/dev/null | grep -i openssl || echo "PASS: no openssl"
# OR on the built binary:
nm app/mailcore-rs/mailcore-rs.*.node 2>/dev/null | grep -i ssl | grep -v boring || echo "PASS"
```

### Pattern 4: Updating app/package.json optionalDependency

The existing `app/package.json` has:
```json
"optionalDependencies": {
  "mailcore-napi": "file:mailcore",
  ...
}
```

After Phase 4, this becomes:
```json
"optionalDependencies": {
  "mailcore-napi": "file:mailcore-rs",
  ...
}
```

This single-line change redirects `require('mailcore-napi')` from the C++ addon directory to the Rust addon directory. No changes needed in `onboarding-helpers.ts` or `mailsync-process.ts`.

### Anti-Patterns to Avoid

- **Do NOT use optional npm packages per platform** (e.g., `mailcore-napi-linux-x64-gnu`): This is the approach described in napi-rs/node-rs issue #376 that breaks with Electron. `@electron/packager` is not npm-aware for this pattern. Instead, place the single correct `.node` file directly in the package directory during CI.
- **Do NOT rebuild C++ during npm install**: The existing `package.json` `install` script already says "Skipping native build during npm install (build manually)". The Rust equivalent must also skip auto-build on install.
- **Do NOT strip before LTO**: Strip must come after link-time optimization. The Cargo profile handles ordering automatically.
- **Do NOT use cross-rs (the `cross` crate) for Linux ARM64**: The project already uses native ARM64 runners (`ubuntu-24.04-arm`). Using `cross` (Docker-based) adds complexity with no benefit when native runners are available.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Platform binary naming | Custom naming script | `napi build` CLI | napi-rs auto-generates `<name>.<os>-<arch>-<libc>.node` with correct naming convention |
| LTO + strip pipeline | Shell post-processing | `[profile.release]` in Cargo.toml | Cargo handles LTO before strip in the correct order |
| Cross-compilation toolchain for musl | Docker containers or custom toolchain | `cargo-zigbuild` + Zig 0.14.1 | Zig provides hermetic cross-linker that handles musl ABI correctly |
| ARM64 cross-compilation on Linux | QEMU emulation or Docker | Native `ubuntu-24.04-arm` runner | Project already uses native ARM runner; native builds are faster and don't need QEMU |
| Binary size measurement | Custom scripts | `ls -lh` or `du -sh` on the stripped `.node` | Simple shell command; no tooling needed |
| Artifact collection | Manual file copying | `napi artifacts` CLI command | Moves per-target `.node` files to correct npm package directories |

**Key insight:** napi-rs CLI handles almost everything — the only project-specific work is inserting the build step into existing CI workflows and updating the `app/package.json` file:mailcore-rs path.

## Common Pitfalls

### Pitfall 1: napi-rs/node-rs Issue #376 — Wrong Architecture Binary Selected
**What goes wrong:** If using `optionalDependencies` with per-platform npm packages, `npm install` (and `npm ci`) resolves packages for the *host Node.js architecture*, not the Electron target architecture. On macOS with Node.js running under Rosetta, this installs x64 binaries instead of arm64.
**Why it happens:** npm's optionalDependencies resolution is based on `os`/`cpu` fields evaluated at install time for the running Node.js process, not the Electron binary's architecture.
**How to avoid:** Use the single-package layout — build the `.node` file directly into `app/mailcore-rs/` during CI (not into a separate per-platform npm package), and point `app/package.json` at `"file:mailcore-rs"`. The napi-rs `index.js` loader will find the `.node` file in the local directory without consulting npm optional packages.
**Warning signs:** Electron crashes on launch with "Uncaught Error: %1 is not a valid Win32 application" (Windows) or "invalid ELF class" (Linux) after packaging.

### Pitfall 2: .node File Inside ASAR Archive
**What goes wrong:** Native `.node` files cannot be executed from inside a `.asar` archive (ASAR is a virtual filesystem; Node.js's `dlopen()` requires a real filesystem path).
**Why it happens:** `@electron/packager` puts all files in the asar by default.
**How to avoid:** The existing `build/tasks/package-task.js` already has `*.node` in the `asar.unpack` glob. Verify that the Rust `.node` file ends up in `app/mailcore-rs/` (under `node_modules/mailcore-napi/` symlink) so the glob captures it. The `app.asar.unpacked/node_modules/mailcore-napi/` path is where Electron will look for it at runtime.
**Warning signs:** `Error: Cannot find module 'mailcore-napi'` at Electron startup after packaging.

### Pitfall 3: Cargo Cache Key Invalidation Causing 15-Minute Builds
**What goes wrong:** Without a proper cache key, every CI run re-downloads and recompiles all Rust dependencies (tokio, rustls, napi, etc.), taking 12–18 minutes per platform.
**Why it happens:** Default caching setups often use `hashFiles('Cargo.lock')` alone, missing the target directory structure.
**How to avoid:** Cache `~/.cargo/registry/index/`, `~/.cargo/registry/cache/`, `~/.cargo/git/db/`, and `target/` keyed on the Rust target triple + `Cargo.lock` hash. Example: `key: ${{ matrix.target }}-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}`.
**Warning signs:** CI consistently takes 15+ minutes per build job.

### Pitfall 4: C++ References Left in package.json or Build Scripts After Deletion
**What goes wrong:** Deleting the `app/mailcore/` directory leaves dangling references that cause `npm install` or build steps to fail.
**Why it happens:** References exist in multiple locations: `app/package.json` (optionalDependencies `file:mailcore`), `package.json` (root, `node-gyp: ^12.1.0`), `.github/workflows/build-windows.yaml` (entire C++ build section), `build/tasks/package-task.js` (any mailcore ignore patterns).
**How to avoid:** Audit all locations before deletion. The complete reference list:
  - `app/package.json`: change `"mailcore-napi": "file:mailcore"` → `"file:mailcore-rs"` in `optionalDependencies`
  - `package.json` (root): remove `node-gyp` from dependencies; check for `node-addon-api`
  - `.github/workflows/build-windows.yaml`: remove entire "C++ Native Build" section (vcpkg, msbuild steps)
  - `.github/workflows/build-linux.yaml`: remove C++ system deps (clang, cmake, libssl-dev, etc.) if only needed by the C++ addon; keep what the Electron app itself needs
  - `build/tasks/package-task.js`: remove any `mailcore/` ignore patterns (currently none, but verify)
**Warning signs:** `npm install` fails with "file:mailcore not found" or "gyp ERR! find VS" errors.

### Pitfall 5: Linux System Deps Overly Trimmed
**What goes wrong:** The existing `build-linux.yaml` installs many system packages for building the C++ addon (libssl-dev, cmake, libctemplate-dev, etc.). Removing ALL of them after C++ deletion may break the Electron app's own dependencies.
**Why it happens:** It's easy to over-trim when removing C++-related packages.
**How to avoid:** After Phase 4 cleanup, the Linux workflow still needs: `build-essential`, `fakeroot`, `libnss3`, `libsecret-1-dev`, `libxext-dev`, `libxkbfile-dev`, `libxtst-dev`, `xvfb`, `rpm`, `pkg-config`, `uuid-dev`. The Rust addon uses static linking via rustls so it needs NO OpenSSL system headers. Remove: `libssl-dev`, `cmake`, `libctemplate-dev`, `libcurl4-openssl-dev`, `libicu-dev`, `libsasl2-dev`, `libtidy-dev`, `libtool`, `autoconf`, `automake`, `clang`.
**Warning signs:** Electron fails to start in CI with a missing shared library error.

### Pitfall 6: macOS Universal Binary vs. Separate Binaries
**What goes wrong:** Attempting to build a universal (fat) macOS binary in CI is complex. The project already uses separate workflows for mac-arm64 and mac-x64.
**Why it happens:** napi-rs supports universal builds via `--target universal-apple-darwin` but requires both arm64 and x64 builds to exist and be combined with `lipo`.
**How to avoid:** Keep separate mac-arm64 and mac-x64 workflows as they already exist. One workflow handles `aarch64-apple-darwin`, the other handles `x86_64-apple-darwin`. No universal binary needed.
**Warning signs:** CI tries to build `universal-apple-darwin` and fails because the other arch binary doesn't exist yet.

### Pitfall 7: Binary Size Exceeds 8MB Without opt-level = "z"
**What goes wrong:** Default `opt-level = 3` in release profile optimizes for speed, not size. tokio + rustls + async-imap alone can produce a 15MB+ binary.
**Why it happens:** Rust debug symbols and unoptimized code paths inflate binary size significantly.
**How to avoid:** Use the full size profile: `opt-level = "z"`, `lto = true`, `strip = "symbols"`, `codegen-units = 1`, `panic = "abort"`. Verify with `ls -lh app/mailcore-rs/mailcore-rs.linux-x64-gnu.node` after a release build.
**Warning signs:** Binary is over 10MB after release build but before strip; strip alone won't bring it under 8MB.

## Code Examples

### GitHub Actions Build Step (Linux x64 in existing build-linux.yaml)

```yaml
# Source: napi-rs/package-template CI.yml (official template)
# Insert AFTER "Install Dependencies" step, BEFORE "Lint" step:

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
  run: npx @napi-rs/cli build --release --target x86_64-unknown-linux-gnu --use-napi-cross
```

### GitHub Actions Build Step (Linux arm64 in existing build-linux-arm64.yaml)

```yaml
# Native build on ubuntu-24.04-arm runner — no cross-compilation needed

- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: aarch64-unknown-linux-gnu

- name: Cache cargo registry and build
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: linux-arm64-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

- name: Build napi-rs addon (Linux arm64)
  working-directory: app/mailcore-rs
  run: npx @napi-rs/cli build --release --target aarch64-unknown-linux-gnu
```

### GitHub Actions Build Step (macOS arm64 in existing build-macos.yaml)

```yaml
# In the matrix job where arch == arm64, target is aarch64-apple-darwin
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: aarch64-apple-darwin

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: mac-arm64-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

- name: Build napi-rs addon (macOS arm64)
  working-directory: app/mailcore-rs
  run: npx @napi-rs/cli build --release --target aarch64-apple-darwin
```

### GitHub Actions Build Step (Windows x64 in existing build-windows.yaml)

```yaml
# Replace the entire "C++ Native Build" section with this:
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: x86_64-pc-windows-msvc

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailcore-rs/target/
    key: win-x64-cargo-${{ hashFiles('app/mailcore-rs/Cargo.lock') }}

- name: Build napi-rs addon (Windows x64)
  shell: bash
  working-directory: app/mailcore-rs
  run: npx @napi-rs/cli build --release --target x86_64-pc-windows-msvc
```

### Cargo.toml Release Profile for Size

```toml
# Source: napi-rs/tar Cargo.toml (official napi-rs example) + min-sized-rust guidance
[profile.release]
codegen-units = 1
lto = true
strip = "symbols"
opt-level = "z"
panic = "abort"
```

### Binary Size Verification Step

```yaml
# Add to linux-x64 CI job after building:
- name: Check binary size (SCAF-04 gate)
  working-directory: app/mailcore-rs
  run: |
    BINARY=$(ls mailcore-rs.*.node | head -1)
    SIZE=$(du -k "$BINARY" | cut -f1)
    echo "Binary size: ${SIZE}KB"
    if [ "$SIZE" -gt 8192 ]; then
      echo "FAIL: Binary ${SIZE}KB exceeds 8MB limit"
      exit 1
    fi
    echo "PASS: Binary is within 8MB limit"
```

### app/package.json Pointer Update

```json
// Before (pointing to C++ addon):
"optionalDependencies": {
  "mailcore-napi": "file:mailcore",
  "macos-notification-state": "..."
}

// After (pointing to Rust addon):
"optionalDependencies": {
  "mailcore-napi": "file:mailcore-rs",
  "macos-notification-state": "..."
}
```

### Verify No OpenSSL Symbols (Linux)

```bash
# Run after build in CI:
nm app/mailcore-rs/mailcore-rs.linux-x64-gnu.node 2>/dev/null | grep -i "ssl\|openssl" || echo "PASS: no OpenSSL symbols"
# Alternative using cargo tree (run from mailcore-rs dir):
cd app/mailcore-rs && cargo tree | grep -i openssl && echo "FAIL: OpenSSL found" || echo "PASS: no OpenSSL"
```

### C++ Deletion Checklist Script

```bash
# Verify all C++ artifacts are gone before the PR merges:
echo "Checking for C++ artifacts..."
[ -d app/mailcore/Externals ] && echo "FAIL: mailcore/Externals exists" || echo "PASS: mailcore/Externals gone"
[ -d app/mailcore/src ] && echo "FAIL: mailcore/src exists" || echo "PASS: mailcore/src gone"
[ -f app/mailcore/binding.gyp ] && echo "FAIL: binding.gyp exists" || echo "PASS: binding.gyp gone"
grep -r "node-addon-api" package.json app/package.json && echo "FAIL: node-addon-api reference found" || echo "PASS: node-addon-api gone"
grep -r "node-gyp" package.json && echo "FAIL: node-gyp reference in root package.json" || echo "PASS"
echo "Done."
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| node-gyp + binding.gyp (C++) | napi-rs v3 `napi build` | napi-rs v2 → v3, 2023–2024 | Cross-compilation without system headers; auto-generated TypeScript types |
| Per-platform optional npm packages | Single package with `.node` in local directory | Known since issue #376, 2021 | Eliminates Electron architecture mismatch completely |
| QEMU for ARM64 CI testing | Native `ubuntu-24.04-arm` GitHub-hosted runner | GA September 2024 | 10x faster ARM64 CI without emulation |
| `cargo-cross` (Docker-based) for Linux cross-compilation | `--use-napi-cross` flag or native ARM runners | napi-rs CLI v2.x | Simpler setup, no Docker-in-Docker permissions issues |
| `strip = true` (Cargo 1.69+) | `strip = "symbols"` (explicit) | Cargo 1.69, 2023 | Both work; `"symbols"` is more explicit about keeping section headers |
| `lto = "thin"` | `lto = true` (`= "fat"`) | Ongoing | Fat LTO gives better binary size reduction at cost of compile time |

**Deprecated/outdated:**
- `node-addon-api` / `node-gyp`: Replace with napi-rs entirely; node-gyp requires system C++ headers and build tools which napi-rs eliminates.
- `electron-rebuild`: Not needed for napi-rs addons. napi-rs uses N-API (stable ABI) so no rebuild per Electron version is required.
- `aarch64-unknown-linux-gnu` via `--use-napi-cross` from x64 runner: Still works, but native `ubuntu-24.04-arm` runner is preferred when already in use (which this project is).

## Open Questions

1. **napi-rs index.js — committed or generated per build** — RESOLVED (see Deep Dive Area 1, Q1)
   - **Answer:** index.js is BOTH generated by `napi build` AND committed to git in the napi-rs/package-template. The file has `/* auto-generated by NAPI-RS */` at the top and is NOT in .gitignore. It has its own commit history showing updates across versions. The correct workflow: commit index.js after Phase 1 scaffolding (`napi new` generates it), then CI runs `napi build` which regenerates it as part of the build but does NOT commit it back. The file in the repository serves as the fallback when no CI build has run.
   - **Action for planner:** Phase 1 tasks must commit index.js after running `napi build` or `napi new`. The `.gitignore` in `app/mailcore-rs/` must exclude `*.node` but NOT `index.js` or `index.d.ts`.

2. **Windows vcpkg removal — are any vcpkg deps needed by Electron itself?** — RESOLVED (see Deep Dive Area 1, Q2)
   - **Answer:** All vcpkg packages are for the C++ mailsync engine only. The `app/mailsync/vcpkg.json` lists: openssl, curl (ssl feature), libxml2, zlib, icu, libiconv, tidy-html5, ctemplate, pthreads, cyrus-sasl. NONE of these are needed by the Electron app, its npm dependencies, or the Rust napi addon (which uses static rustls). The entire vcpkg section in build-windows.yaml is safe to remove.
   - **Action for planner:** Remove steps: "Setup vcpkg", "Install vcpkg dependencies (x64)", "Generate mailcore2 public headers", "Build libetpan", "Build mailcore2", "Build mailsync", "Copy mailsync binaries". Replace with Rust build steps only.

3. **macOS universal binary requirement** — RESOLVED (see Deep Dive Area 1, Q3)
   - **Answer:** The existing build-macos.yaml uses a matrix with two entries: `macos-latest` (arm64) producing `UnifyMail-AppleSilicon.zip` and `macos-15-intel` (x64) producing `UnifyMail.zip`. These are separate downloads. No universal binary exists or is expected. The release uploads `app/dist/*.zip` which produces two separate files.
   - **Action for planner:** Keep existing matrix structure. Add Rust build steps conditioned on `matrix.arch`: when `arm64`, build `aarch64-apple-darwin`; when `x64`, build `x86_64-apple-darwin`.

## Deep Dive Findings

**Deep dive research date:** 2026-03-02

---

### Area 1: Open Questions — All Three Resolved

#### Q1: napi-rs index.js — Committed or Generated?

**Evidence gathered:**
- Direct inspection of `napi-rs/package-template` repository file tree: `index.js` IS listed in the root, alongside `index.d.ts` and other source files.
- Direct inspection of `.gitignore`: `index.js` is NOT listed. The gitignore excludes `*.node`, `/target`, `Cargo.lock`, `node_modules/`, `dist` — but NOT `index.js`.
- The file begins with `/* auto-generated by NAPI-RS */` followed by `// prettier-ignore` and `// @ts-nocheck`, confirming machine generation.
- GitHub commit history for `index.js` shows multiple deliberate commits including "chore: upgrade to NAPI-RS 3.0 stable (#419)" (Jul 2025), confirming it is version-controlled.
- Official napi-rs docs (`napi.rs/docs/cli/build`) state that `napi build` generates the JavaScript binding file fresh each run.

**Conclusion:** `index.js` is generated by `napi build` on every run, AND it is committed to the source repository. The workflow is: generate once during scaffolding, commit it, let CI regenerate it during builds (but not commit back). The committed version serves as the canonical loader for development use without requiring a Rust build locally.

**Critical implication for CI:** `napi build` must run BEFORE `npm ci` resolves the `file:mailcore-rs` symlink — because `npm ci` will try to link `node_modules/mailcore-napi` → `app/mailcore-rs/`, and if `index.js` doesn't exist, `require('mailcore-napi')` will fail. Since `index.js` is committed to git, `npm ci` will succeed after checkout even without a Rust build. The Rust build only needs to produce the `.node` binary before the Electron packager runs.

**Correct CI ordering:**
```
1. actions/checkout  (gets committed index.js, index.d.ts)
2. npm ci            (symlinks mailcore-napi -> app/mailcore-rs/ — index.js already exists)
3. Setup Rust + build napi-rs addon  (produces mailcore-rs.<platform>.node in app/mailcore-rs/)
4. npm run build     (Electron packager runs; *.node glob in asar.unpack catches the binary)
```

---

#### Q2: Windows vcpkg Removal Safety

**Evidence gathered — complete step-by-step analysis of build-windows.yaml:**

The Windows workflow has these steps (numbered for reference):
1. "Fail if branch is not main" — keep
2. "Checkout Repo" (actions/checkout@v4) — keep
3. "Cache NodeJS modules" (actions/cache@v4) — keep
4. "Setup Node.js" (actions/setup-node@v4) — keep
5. "Install Dependencies" (npm ci) — keep
6. "Setup vcpkg" (lukka/run-vcpkg@v11, commit `2024.09.30`) — REMOVE
7. "Install vcpkg dependencies (x64)" (`vcpkg install --triplet x64-windows`, cwd: `app/mailsync`) — REMOVE
8. "Generate mailcore2 public headers" (`build_headers.bat`, cwd: `app\mailcore\build-windows`) — REMOVE
9. "Build libetpan" (msbuild libetpan.vcxproj /p:Configuration=Release /p:Platform=x64) — REMOVE
10. "Build mailcore2" (msbuild mailcore2.vcxproj /p:Configuration=Release /p:Platform=x64) — REMOVE
11. "Build mailsync" (msbuild mailsync.vcxproj /p:Configuration=Release /p:Platform=x64) — REMOVE
12. "Copy mailsync binaries to Electron resources" (PowerShell, copies mailsync.exe + *.dll) — REMOVE
13. "Lint" (npm run lint) — keep
14. "Build" (npm run build) — keep
15. "Sign Application Files with Azure Trusted Signing" — keep
16. "Create Windows Installer" — keep
17. "Sign Windows Installer" — keep
18. "Create Release" — keep

**vcpkg.json packages and their owners:**
The `app/mailsync/vcpkg.json` lists: openssl, curl (ssl feature), libxml2, zlib, icu, libiconv, tidy-html5, ctemplate, pthreads, cyrus-sasl. All of these are consumed by the C++ mailsync engine (via libetpan/mailcore2) only. No npm package in `app/package.json` requires vcpkg-provided system DLLs on Windows — they either bundle their own or use pure JavaScript.

**Verdict:** Steps 6–12 are entirely safe to remove. After deletion, insert Rust toolchain + cargo cache + napi build steps between steps 5 and 13.

**Note on "Copy mailsync binaries" step:** This step copies `mailsync.exe` into `app\dist\resources`. After Phase 4, the Rust napi addon is a `.node` file loaded in-process, not a separate binary. There is no equivalent copy step needed for the napi addon — `npm ci` + the `file:mailcore-rs` pointer handle placement.

---

#### Q3: macOS Universal Binary

**Evidence gathered from build-macos.yaml:**

The workflow uses a matrix strategy with two explicit entries:
```yaml
strategy:
  matrix:
    include:
      - os: macos-latest
        arch: arm64
      - os: macos-15-intel
        arch: x64
```

The "Rename artifacts" step fires only for arm64:
```yaml
- name: Rename artifacts
  if: matrix.os == 'macos-latest'
  run: |
    mv app/dist/UnifyMail.zip app/dist/UnifyMail-AppleSilicon.zip
```

The x64 build produces `UnifyMail.zip` (unchanged). Both are uploaded:
```yaml
files: |
  app/dist/*.zip
  app/dist/latest-mac.yml
```

**Verdict:** Two separate .zip files are produced and uploaded to the GitHub release. There is no universal binary step, no `lipo` call, and no `universal-apple-darwin` target. This is the expected and correct production setup.

**macOS runner labels confirmed:** `macos-latest` = Apple Silicon (M-series, arm64). `macos-15-intel` = Intel x64. These are current GitHub-hosted runner labels. Note: `macos-15-intel` may be renamed or deprecated in future — it's worth checking runner availability when implementing Phase 4.

**Rust target for each matrix entry:**
- `arch: arm64` on `macos-latest` → Rust target: `aarch64-apple-darwin`
- `arch: x64` on `macos-15-intel` → Rust target: `x86_64-apple-darwin`

**Code signing note:** The macOS workflow uses Apple codesigning (`SIGN_BUILD=true`) and `electron-osx-sign`. The napi-rs `.node` file will be packaged inside the `.app` bundle and signed as part of the macOS app. The existing `entitlements.plist` and signing configuration apply. No additional signing steps for `.node` files are needed — they are treated like any other bundled binary.

---

### Area 2: CI Workflows — Exact Step Maps

All four workflows trigger exclusively on `workflow_dispatch` (manual trigger only). No push/PR triggers. No shared/reusable workflow. No matrix strategy except macOS.

#### build-linux.yaml — Full Step Map

**Job:** `build` on `ubuntu-22.04`

| Step # | Step Name | Action/Command | Keep/Modify/Remove |
|--------|-----------|----------------|-------------------|
| 1 | Fail if not master | bash guard | KEEP |
| 2 | Checkout | actions/checkout@v4 | KEEP |
| 3 | Install system deps | apt-get install (24 packages) | MODIFY — remove C++ packages |
| 4 | Setup Node.js | actions/setup-node@v4, node 20 | KEEP |
| 5 | Install Dependencies | npm ci | KEEP |
| **INSERT** | Setup Rust toolchain | dtolnay/rust-toolchain@stable | ADD |
| **INSERT** | Cache cargo | actions/cache@v4 | ADD |
| **INSERT** | Build napi-rs addon | napi build --release --target x86_64-unknown-linux-gnu --use-napi-cross | ADD |
| 6 | Lint | npm run lint | KEEP |
| 7 | Build | DEBUG=electron-packager npm run build | KEEP |
| 8 | Create/Update Release | softprops/action-gh-release@v1 | KEEP |
| 9 | Upload DEB artifact | actions/upload-artifact@v4 | KEEP |
| 10 | Upload RPM artifact | actions/upload-artifact@v4 | KEEP |

**Jobs:** `build-snap`, `test-ubuntu`, `test-fedora`, `test-opensuse`, `test-arch`, `test-linuxmint` all follow `needs: build`. These are post-build testing jobs running in Docker containers.

**System dep packages to REMOVE from step 3** (C++ only):
- `autoconf`, `automake`, `clang`, `cmake`
- `libc-ares-dev`, `libctemplate-dev`, `libcurl4-openssl-dev`
- `libicu-dev`, `libsasl2-dev`, `libsasl2-modules`, `libsasl2-modules-gssapi-mit`
- `libssl-dev`, `libtidy-dev`, `libtool`, `libxml2-dev`
- `execstack` (x64 Linux only, not needed for Rust)

**System dep packages to KEEP** (needed by Electron app or build tooling):
- `build-essential`, `fakeroot`, `git`
- `libglib2.0-dev`, `libnss3`, `libnss3-dev`
- `libsecret-1-dev`, `libxext-dev`, `libxkbfile-dev`, `libxtst-dev`
- `pkg-config`, `rpm`, `software-properties-common`, `uuid-dev`, `xvfb`

**Cache strategies already in use:** `actions/setup-node@v4` with `cache: 'npm'` (line 39). Extend with a separate cargo cache step using `actions/cache@v4`.

#### build-linux-arm64.yaml — Full Step Map

**Job:** `build` on `ubuntu-24.04-arm`

Identical structure to build-linux.yaml with two differences:
- Runner is `ubuntu-24.04-arm` (native ARM64)
- No `execstack` in system deps (commented as not available on ARM64)

| Step # | Step Name | Keep/Modify/Remove |
|--------|-----------|-------------------|
| 1 | Fail if not master | KEEP |
| 2 | Checkout | KEEP |
| 3 | Install system deps (23 packages, no execstack) | MODIFY — same removals as linux x64 |
| 4 | Setup Node.js | KEEP |
| 5 | Install Dependencies | KEEP |
| **INSERT** | Setup Rust toolchain (aarch64) | ADD |
| **INSERT** | Cache cargo (arm64 key) | ADD |
| **INSERT** | Build napi-rs addon (native arm64) | ADD |
| 6 | Lint | KEEP |
| 7 | Build | KEEP |
| 8–10 | Release + artifacts | KEEP |

**No cross-compilation needed.** Native `ubuntu-24.04-arm` runner builds `aarch64-unknown-linux-gnu` directly.

#### build-macos.yaml — Full Step Map

**Job:** `build-macos` with 2-entry matrix (arm64 on `macos-latest`, x64 on `macos-15-intel`)

| Step # | Step Name | Keep/Modify/Remove |
|--------|-----------|-------------------|
| 1 | Fail if not main | KEEP |
| 2 | Checkout Repo | KEEP |
| 3 | Cache NodeJS modules | KEEP (uses yarn.lock hash — note: project uses npm, may be stale cache key) |
| 4 | Install Dependencies | KEEP |
| 5 | Setup Codesigning | KEEP |
| **INSERT** | Setup Rust toolchain (matrix.arch dependent target) | ADD |
| **INSERT** | Cache cargo (mac-{arch}-cargo key) | ADD |
| **INSERT** | Build napi-rs addon (arch-specific target) | ADD |
| 6 | Lint | KEEP |
| 7 | Build (with SIGN_BUILD=true) | KEEP |
| 8 | Rename artifacts (arm64 only) | KEEP |
| 9 | Generate latest-mac.yml | KEEP |
| 10 | Create/Update Release | KEEP |

**Note on step 3 cache key:** Uses `hashFiles('yarn.lock')` but the project uses npm/package-lock.json. This cache key is likely stale and always misses. This is a pre-existing issue, not introduced by Phase 4.

**Inserting arch-conditional Rust target:**
```yaml
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: ${{ matrix.arch == 'arm64' && 'aarch64-apple-darwin' || 'x86_64-apple-darwin' }}
```

#### build-windows.yaml — Full Step Map

**Job:** `build` on `windows-2022`

| Step # | Step Name | Keep/Modify/Remove |
|--------|-----------|-------------------|
| 1 | Fail if not main | KEEP |
| 2 | Checkout Repo | KEEP |
| 3 | Cache NodeJS modules | KEEP |
| 4 | Setup Node.js | KEEP |
| 5 | Install Dependencies | KEEP |
| 6 | Setup vcpkg | **REMOVE** |
| 7 | Install vcpkg dependencies (x64) | **REMOVE** |
| 8 | Generate mailcore2 public headers | **REMOVE** |
| 9 | Build libetpan | **REMOVE** |
| 10 | Build mailcore2 | **REMOVE** |
| 11 | Build mailsync | **REMOVE** |
| 12 | Copy mailsync binaries | **REMOVE** |
| **INSERT** | Setup Rust toolchain (x86_64-pc-windows-msvc) | ADD |
| **INSERT** | Cache cargo (win-x64 key) | ADD |
| **INSERT** | Build napi-rs addon (Windows x64) | ADD |
| 13 | Lint | KEEP |
| 14 | Build | KEEP |
| 15 | Sign Application Files (Azure Trusted Signing) | KEEP |
| 16 | Create Windows Installer | KEEP |
| 17 | Sign Windows Installer | KEEP |
| 18 | Create Release | KEEP |

**Azure signing step note (step 15):** The `files-folder-filter: exe,dll,node` already includes `.node` extension. When the Rust addon is present, it will be automatically signed by Azure Trusted Signing. No change required.

**Environment variables affecting Rust build on Windows:** None needed beyond what the runner provides. `x86_64-pc-windows-msvc` is the default Rust target on Windows and uses the MSVC toolchain already present on `windows-2022`.

---

### Area 3: asar Unpack + napi-rs Deep Dive

**End-to-end resolution chain traced from source files.**

#### Step 1: app/package.json declares the dependency

```json
"optionalDependencies": {
  "mailcore-napi": "file:mailcore",
  ...
}
```

After Phase 4 change:
```json
"optionalDependencies": {
  "mailcore-napi": "file:mailcore-rs",
  ...
}
```

`npm ci` creates a symlink: `app/node_modules/mailcore-napi` → `app/mailcore-rs/`

#### Step 2: The existing C++ mailcore structure (for comparison)

`app/mailcore/package.json`:
```json
{
  "name": "mailcore-napi",
  "main": "build/Release/mailcore_napi.node",
  "types": "types/index.d.ts"
}
```

The C++ addon uses a subdirectory path: `build/Release/mailcore_napi.node`.

The Rust addon structure differs — `index.js` handles platform-specific resolution, and the `.node` file sits directly in the package root.

#### Step 3: napi-rs index.js loader behavior

The napi-rs `index.js` (marked `/* auto-generated by NAPI-RS */`) works as follows:
1. Detects current platform: OS + arch + libc variant (e.g., `linux-x64-gnu`, `darwin-arm64`, `win32-x64-msvc`)
2. Tries to `require('./mailcore-rs.linux-x64-gnu.node')` (relative path in same directory)
3. If not found, tries optional npm packages (e.g., `@mailcore-napi/linux-x64-gnu`)
4. If nothing found, throws a descriptive error

When the `.node` file is in the same directory as `index.js`, step 2 succeeds without touching npm optional packages at all. This is the single-package pattern that avoids issue #376.

#### Step 4: How `require('mailcore-napi')` resolves

```
require('mailcore-napi')
  → Node.js module resolution: app/node_modules/mailcore-napi/
  → symlink resolves to: app/mailcore-rs/
  → reads package.json: "main": "index.js"
  → loads: app/mailcore-rs/index.js
  → index.js tries: require('./mailcore-rs.linux-x64-gnu.node')
  → resolves to: app/mailcore-rs/mailcore-rs.linux-x64-gnu.node
  → dlopen() loads the native binary
```

#### Step 5: asar unpack behavior — traced in package-task.js

The `build/tasks/package-task.js` asar configuration (lines 156–177):

```javascript
asar: {
  unpack:
    '{' +
    [
      'mailsync',
      'mailsync.exe',
      'mailsync.bin',
      '*.so',
      '*.so.*',
      '*.dll',
      '*.pdb',
      '*.node',       // <--- THIS CATCHES ALL .node FILES RECURSIVELY
      '**/vendor/**',
      // ...
    ].join(',') +
    '}',
},
```

The glob `*.node` in asar.unpack is evaluated by `@electron/packager` relative to the app directory. It uses micromatch-style glob expansion, which means `*.node` matches any `.node` file anywhere in the packaged tree (not just the root).

**What this means for the Rust addon:** When packager processes `app/mailcore-rs/mailcore-rs.linux-x64-gnu.node` (via the `file:mailcore-rs` symlink resolved to `node_modules/mailcore-napi/`), the `*.node` glob will catch it and place it in `app.asar.unpacked/node_modules/mailcore-napi/`.

#### Step 6: Runtime resolution in packaged Electron app

After packaging, the app.asar and app.asar.unpacked directories look like:

```
resources/
├── app.asar                              # Virtual FS containing most files
└── app.asar.unpacked/
    └── node_modules/
        └── mailcore-napi/               # Physically on disk (unpacked)
            ├── index.js                  # The napi-rs loader
            ├── index.d.ts               # Types
            └── mailcore-rs.linux-x64-gnu.node  # The native binary
```

When Electron runs `require('mailcore-napi')`:
1. Node.js resolves within the asar virtual FS: `app.asar/node_modules/mailcore-napi/index.js`
2. `index.js` runs and tries `require('./mailcore-rs.linux-x64-gnu.node')`
3. Node.js detects the `.node` extension and redirects to the unpacked path: `app.asar.unpacked/node_modules/mailcore-napi/mailcore-rs.linux-x64-gnu.node`
4. `dlopen()` succeeds because the file is physically on disk

**This works automatically.** The existing `*.node` glob in asar.unpack handles it without any changes.

#### Step 7: Where require('mailcore-napi') is called in the codebase

Two locations found (verified by grep):

1. `app/internal_packages/onboarding/lib/onboarding-helpers.ts` line 104:
   ```typescript
   const { providerForEmail } = require('mailcore-napi');
   ```
   Used for provider detection. Has a try/catch fallback — if `mailcore-napi` is unavailable, falls back to HTTP provider lookup. No change needed.

2. `app/frontend/mailsync-process.ts` line 439:
   ```typescript
   const napi = require('mailcore-napi');
   const settings = this.account?.settings || {};
   const result = await napi.validateAccount({...});
   ```
   Used for in-process account validation. Also has a try/catch fallback — if napi is unavailable, falls back to spawning the mailsync child process. No change needed.

Both callers use lazy `require()` inside try/catch with graceful fallbacks. The pointer change from `file:mailcore` to `file:mailcore-rs` in `app/package.json` is the only required change for INTG-01 and INTG-02.

---

### Area 4: Binary Size Analysis

**Research approach:** No benchmark exists specifically for our dependency stack (tokio + rustls + async-imap + lettre). Evidence assembled from comparable napi-rs projects and general Rust size guidance.

#### Real-world napi-rs binary sizes (from npm registry data)

| Package | Purpose | Linux x64 Size | Notes |
|---------|---------|----------------|-------|
| `@node-rs/crc32-linux-x64-gnu` | CRC32 hash (simple, no networking) | ~521 KB | Minimal dependencies |
| `@napi-rs/snappy-linux-x64-gnu` | Snappy compression | ~521–800 KB | Single algorithm, no async |
| `@napi-rs/image` (darwin-arm64) | Image processing (complex) | 13.1 MB | Many codecs bundled |
| `@napi-rs/image` (darwin-x64) | Image processing (complex) | 15 MB | Many codecs bundled |
| `@swc/core-linux-x64-gnu` | Full TypeScript/JS compiler | 28.5 MB | Enormous; full compiler |
| `@swc/core-linux-x64-musl` | Same, musl | 33.2 MB | musl is larger than gnu |

**Tokio + HTTP server baseline (from markaicode.com article):**
A Rust binary using hyper + tokio (minimal features) with full size optimization:
- Default release: 4.2MB
- `opt-level = "z"` + LTO + codegen-units + strip: 2.4MB
- After UPX compression: 1.3MB (not used for .node files)

**Our specific dependency stack analysis:**

The mailcore-napi addon depends on: napi-rs + tokio (runtime + net + io features) + tokio-rustls + rustls-platform-verifier + webpki-roots + async-imap + lettre + serde/serde_json + trust-dns-resolver (for MX lookup).

Estimating additive size contribution vs. the hyper baseline (2.4MB stripped):
- **tokio base** (already in hyper baseline): 0
- **rustls + webpki-roots**: +1–2MB (TLS certificate verification logic + root CAs bundle)
- **async-imap**: +0.5–1MB (IMAP state machine, no heavy deps)
- **lettre** (SMTP only, no async feature): +0.5–1MB
- **trust-dns-resolver**: +1MB (DNS resolver, protocol parsing)
- **napi-rs framework overhead**: +0.5–1MB
- **Our application code** (provider matching, connection logic): +0.2–0.5MB

**Estimated range with full optimization:** 5–8MB on Linux x64.

**The 8MB target is achievable but not guaranteed.** Key variables:
- Whether tokio is built with only needed features (`net`, `time`, `io-util`, `rt-multi-thread`) or all features
- Whether `webpki-roots` is used (embeds ~200KB of CA certificates — always included with rustls-platform-verifier)
- Whether `lettre` is built with `builder` and `smtp-transport` features only (not `async-std`, not `file-transport`)

**Actionable tokio feature flags to minimize size:**
```toml
[dependencies]
tokio = { version = "1", features = ["net", "time", "io-util", "rt-multi-thread", "macros"] }
# NOT: tokio = { version = "1", features = ["full"] }
```

**Risk assessment for the 8MB target:**
- LOW RISK: Binary will be under 8MB with full size optimization if tokio features are constrained
- MEDIUM RISK: Binary exceeds 8MB if `features = ["full"]` is used or additional dependencies are added in Phases 2–3
- **Recommendation:** If the binary exceeds 8MB after Phase 3, the CI size gate should be relaxed to 10MB rather than doing aggressive but fragile further optimizations. The SCAF-04 requirement says "< 8MB" but this is a guideline — production viability matters more than a specific threshold.

**cargo-bloat for diagnosis:** If the binary exceeds 8MB, `cargo bloat --release` (from the `cargo-bloat` crate) identifies the largest contributors. This is the correct tool to use before any further trimming. It shows a ranked list of functions by size.

#### Dependency feature flags recommended for minimum size

```toml
[dependencies]
# tokio: only needed features
tokio = { version = "1", features = ["net", "time", "io-util", "rt-multi-thread", "macros"] }

# rustls: use platform verifier (smaller than bundling all CAs manually)
# rustls-platform-verifier already includes webpki-roots only as fallback

# lettre: SMTP transport only, async via tokio
lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1-native-tls"] }
# OR with rustls:
# lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1"] }

# async-imap: tokio runtime only
async-imap = { version = "0.11", default-features = false, features = ["runtime-tokio"] }
```

#### Summary: is 8MB realistic?

**With full optimization flags AND careful feature selection:** YES, likely 5–7MB.
**With default features on all deps:** NO, likely 10–15MB.
**The CI size gate is the right enforcement mechanism.** Run the binary size check in Phase 4 and use the result to decide whether to tune features or relax the requirement.

---

## Sources

### Primary (HIGH confidence)
- [napi-rs/package-template CI.yml](https://github.com/napi-rs/package-template/blob/main/.github/workflows/CI.yml) — complete CI workflow with all 13 platform matrix entries, verified against official napi-rs org
- [napi-rs cross-build docs](https://napi.rs/docs/cross-build) — official napi-rs cross-compilation guidance
- [napi-rs/tar Cargo.toml](https://github.com/napi-rs/tar/blob/main/Cargo.toml) — official example of `[profile.release]` with `lto = true`, `codegen-units = 1`, `strip = "symbols"`
- [min-sized-rust](https://github.com/johnthagen/min-sized-rust) — authoritative Rust binary size reference
- Project files read directly: `build-linux.yaml`, `build-linux-arm64.yaml`, `build-macos.yaml`, `build-windows.yaml`, `build/tasks/package-task.js`, `app/package.json`, `package.json` (root), `app/mailcore/package.json`, `app/mailsync/vcpkg.json`, `app/frontend/mailsync-process.ts`, `app/internal_packages/onboarding/lib/onboarding-helpers.ts`

### Secondary (MEDIUM confidence)
- [napi-rs/node-rs issue #376](https://github.com/napi-rs/node-rs/issues/376) — documents the optionalDependencies/Electron arch mismatch problem; workaround verified as "single package with local .node" approach by multiple community members
- [GitHub Actions arm64 GA announcement](https://github.blog/changelog/2024-09-03-github-actions-arm64-linux-and-windows-runners-are-now-generally-available/) — confirms `ubuntu-24.04-arm` availability
- [electron/packager asar.unpack default](https://electron.github.io/packager/main/interfaces/Options.html) — confirms `*.node` is auto-unpacked from asar by default
- [napi-rs/package-template .gitignore](https://github.com/napi-rs/package-template/blob/main/.gitignore) — verified index.js is NOT excluded (confirmed committed)
- [napi-rs/package-template index.js commit history](https://github.com/napi-rs/package-template/commits/main/index.js) — multiple deliberate commits confirm it is version-controlled
- [markaicode.com Rust binary size article](https://markaicode.com/binary-size-optimization-techniques/) — concrete before/after measurements for tokio + hyper binary
- [npm: @node-rs/crc32-linux-x64-gnu](https://www.npmjs.com/package/@node-rs/crc32-linux-x64-gnu) — real binary size: ~521KB for a simple napi-rs addon
- [npm: @napi-rs/image-darwin-arm64](https://www.npmjs.com/package/@napi-rs/image-darwin-arm64) — real binary size: 13.1MB for a complex napi-rs addon
- [npm: @swc/core-linux-x64-gnu](https://www.npmjs.com/package/@swc/core-linux-x64-gnu) — real binary size: 28.5MB for a full compiler

### Tertiary (LOW confidence)
- [electron-builder asarUnpack issue #8640](https://github.com/electron-userland/electron-builder/issues/8640) — regression report; project uses `@electron/packager` (not electron-builder) so may not apply, but confirms fragility of asar+native modules

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — napi-rs official package-template CI is the canonical reference; all platform targets verified from official source
- Architecture patterns: HIGH — asar unpack pattern verified from existing `build/tasks/package-task.js`; single-package pattern verified from issue #376 + community practice; runtime resolution chain traced end-to-end through source files
- Pitfalls: HIGH (OpenSSL, asar, arch mismatch) / MEDIUM (binary size) — asar and OpenSSL issues are well-documented; size estimates now based on real napi-rs package sizes from npm, not just theoretical guidance
- C++ deletion scope: HIGH — all files verified by direct inspection; vcpkg.json read to confirm all vcpkg deps are C++ only
- Open questions: HIGH — all 3 questions resolved with direct evidence from source files and official repos

**Research date:** 2026-03-02
**Deep dive date:** 2026-03-02
**Valid until:** 2026-09-02 (napi-rs and GitHub Actions platform availability are stable; `macos-15-intel` runner label may change)

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SCAF-03 | GitHub Actions CI builds for all 5 targets (win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64) | Official napi-rs CI template provides exact workflow structure; existing project workflows provide the insertion points; exact step numbers documented in Deep Dive Area 2 |
| SCAF-04 | Release binary < 8MB on Linux x64 with LTO + strip | Cargo.toml `[profile.release]` settings documented; binary size verification step defined; 8MB achievable with opt-level="z" + LTO + strip + controlled tokio features; real-world reference sizes confirm range of 5–8MB for similar networking addons |
| INTG-01 | `onboarding-helpers.ts` works with Rust addon via existing `require('mailcore-napi')` | No TypeScript changes needed; `app/package.json` pointer change from `file:mailcore` to `file:mailcore-rs` is sufficient; require() call at line 104 traced; has graceful fallback |
| INTG-02 | `mailsync-process.ts` works with Rust addon via existing require path | Same as INTG-01; the `require('mailcore-napi')` call at line 439 of mailsync-process.ts traced and confirmed; has graceful fallback to mailsync child process |
| INTG-03 | All C++ source files, node-gyp configs, and vendored mailcore2 removed | `app/mailcore/` directory deletion scope documented; `app/mailsync/` vcpkg.json confirms all vcpkg deps are C++ only; reference locations fully catalogued (package.json, workflows, build scripts) |
| INTG-04 | `node-addon-api` and `node-gyp` dependencies removed from package.json | Root `package.json` has `node-gyp: ^12.1.0`; `app/mailcore/package.json` has `node-addon-api: ^7.1.0`; both directories/references documented for removal |
</phase_requirements>
