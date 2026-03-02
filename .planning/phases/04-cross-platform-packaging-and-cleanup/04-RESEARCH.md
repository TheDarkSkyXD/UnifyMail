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

1. **napi-rs package.json structure for mailcore-rs**
   - What we know: Phase 1 research documented that `app/mailcore-rs/package.json` will have `"name": "mailcore-napi"` and `"main": "index.js"`, and napi-rs generates `index.js` during `napi build`
   - What's unclear: Whether `index.js` is generated fresh on every `napi build` (meaning it must NOT be checked in) or is generated once and committed (as the canonical loader)
   - Recommendation: Check the Phase 1 output — if `index.js` was generated by `napi new` during scaffolding, it should be committed. The binary `.node` file is always generated at CI time and never committed.

2. **Windows vcpkg removal — are any vcpkg deps needed by Electron itself?**
   - What we know: The Windows workflow installs vcpkg for the C++ addon (libssl via vcpkg for mailcore2)
   - What's unclear: Whether the main Electron app or any of its npm deps require vcpkg-installed system libraries
   - Recommendation: Remove the entire vcpkg section from build-windows.yaml since the Rust addon uses static rustls (no system OpenSSL). If the Electron build fails, add back only the specific deps needed.

3. **macOS universal binary requirement**
   - What we know: The current build-macos.yaml builds separate arm64 and x64 binaries using a matrix strategy
   - What's unclear: Whether the release packaging expects a universal `.dmg` or separate downloads for each architecture
   - Recommendation: Keep separate binaries. The existing workflow already produces two separate `.zip` files (arm64 and x64). Universal binary is a nice-to-have but out of scope for Phase 4.

## Sources

### Primary (HIGH confidence)
- [napi-rs/package-template CI.yml](https://github.com/napi-rs/package-template/blob/main/.github/workflows/CI.yml) — complete CI workflow with all 13 platform matrix entries, verified against official napi-rs org
- [napi-rs cross-build docs](https://napi.rs/docs/cross-build) — official napi-rs cross-compilation guidance
- [napi-rs/tar Cargo.toml](https://github.com/napi-rs/tar/blob/main/Cargo.toml) — official example of `[profile.release]` with `lto = true`, `codegen-units = 1`, `strip = "symbols"`
- [min-sized-rust](https://github.com/johnthagen/min-sized-rust) — authoritative Rust binary size reference
- Project files read directly: `build-linux.yaml`, `build-linux-arm64.yaml`, `build-macos.yaml`, `build-windows.yaml`, `build/tasks/package-task.js`, `app/package.json`, `package.json` (root)

### Secondary (MEDIUM confidence)
- [napi-rs/node-rs issue #376](https://github.com/napi-rs/node-rs/issues/376) — documents the optionalDependencies/Electron arch mismatch problem; workaround verified as "single package with local .node" approach by multiple community members
- [GitHub Actions arm64 GA announcement](https://github.blog/changelog/2024-09-03-github-actions-arm64-linux-and-windows-runners-are-now-generally-available/) — confirms `ubuntu-24.04-arm` availability
- [electron/packager asar.unpack default](https://electron.github.io/packager/main/interfaces/Options.html) — confirms `*.node` is auto-unpacked from asar by default

### Tertiary (LOW confidence)
- [electron-builder asarUnpack issue #8640](https://github.com/electron-userland/electron-builder/issues/8640) — regression report; project uses `@electron/packager` (not electron-builder) so may not apply, but confirms fragility of asar+native modules

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — napi-rs official package-template CI is the canonical reference; all platform targets verified from official source
- Architecture patterns: HIGH — asar unpack pattern verified from existing `build/tasks/package-task.js`; single-package pattern verified from issue #376 + community practice
- Pitfalls: HIGH (OpenSSL, asar, arch mismatch) / MEDIUM (binary size, system dep trimming) — asar and OpenSSL issues are well-documented; size estimates based on similar napi-rs addons, not measured on this specific codebase
- C++ deletion scope: HIGH — all files verified by direct inspection of `app/mailcore/` directory and `package.json` files

**Research date:** 2026-03-02
**Valid until:** 2026-09-02 (napi-rs and GitHub Actions platform availability are stable; runner labels may change with notice)

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| SCAF-03 | GitHub Actions CI builds for all 5 targets (win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64) | Official napi-rs CI template provides exact workflow structure; existing project workflows provide the insertion points |
| SCAF-04 | Release binary < 8MB on Linux x64 with LTO + strip | Cargo.toml `[profile.release]` settings documented; binary size verification step defined; 8MB achievable with opt-level="z" + LTO + strip |
| INTG-01 | onboarding-helpers.ts works with Rust addon via existing `require('mailcore-napi')` | No TypeScript changes needed; `app/package.json` pointer change from `file:mailcore` to `file:mailcore-rs` is sufficient |
| INTG-02 | mailsync-process.ts works with Rust addon via existing require path | Same as INTG-01; the `require('mailcore-napi')` call at line 439 of mailsync-process.ts resolves unchanged |
| INTG-03 | All C++ source files, node-gyp configs, and vendored mailcore2 removed | `app/mailcore/` directory deletion scope documented; reference locations catalogued (package.json, workflows, build scripts) |
| INTG-04 | node-addon-api and node-gyp dependencies removed from package.json | Root `package.json` has `node-gyp: ^12.1.0`; `app/mailcore/package.json` has `node-addon-api: ^7.1.0`; both directories/references documented for removal |
</phase_requirements>
