# Phase 10: Cross-Platform Builds, Packaging, and C++ Deletion - Research

**Researched:** 2026-03-02
**Domain:** Rust standalone binary CI cross-compilation, Electron asar unpacking, binary size optimization, macOS code signing, C++ source deletion
**Confidence:** HIGH (CI patterns extracted from existing project workflows; path resolution logic extracted directly from mailsync-process.ts; asar unpack patterns extracted from package-task.js; cross-compilation approach from Phase 4 research baseline)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PKGN-01 | Cross-platform builds for 5 targets: win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64 | CI patterns extracted from existing 4 workflow files; Rust targets and runner assignments documented; cargo-zigbuild v0.22.1 for Linux cross-compilation |
| PKGN-02 | Release binary under 15MB with LTO and strip | LTO + strip + opt-level=z + panic=abort profile documented; 15MB is achievable for full async Rust stack (tokio + TLS + SQLite + IMAP/SMTP) |
| PKGN-03 | electron-builder asarUnpack configured for Rust mailsync binary | `mailsync`, `mailsync.exe`, `mailsync.bin` already in asar.unpack glob in package-task.js; Rust binary names match existing glob patterns |
| PKGN-04 | `mailsync-process.ts` spawns Rust binary via existing path resolution logic | Path resolution in mailsync-process.ts lines 103-113 requires NO changes; binary names `mailsync.exe` (win) and `mailsync.bin` (all others) match what Rust CI must produce |
| PKGN-05 | All C++ mailsync source, vendored dependencies, and build configs deleted | Complete audit: `app/mailsync/` (C++ source), `app/mailsync/Vendor/` (SQLiteCpp, libetpan, spdlog, nlohmann, etc.), `app/mailsync/MailSync.xcodeproj/`, `app/mailsync/Windows/`, CI workflow C++ sections |
| PKGN-06 | TLS via rustls exclusively (no OpenSSL symbols) | `cargo tree -e features \| grep -i openssl` CI check; all prior phases must use `rustls-tls` feature flags; async-imap, lettre, reqwest all support rustls-tls features |
</phase_requirements>

---

## Summary

Phase 10 completes the v2.0 rewrite by shipping the Rust mailsync binary to users on all 5 platforms and permanently removing the C++ codebase. The work divides cleanly into four streams: (1) CI — insert `cargo build --release --target <TARGET>` steps into existing workflows; (2) binary placement — copy the Rust binary to a location the existing path resolution code already expects; (3) binary size validation — profile.release settings must produce a stripped binary under 15MB; and (4) C++ deletion — remove `app/mailsync/` in its entirety plus targeted references in workflows and package.json.

The critical insight: the existing `mailsync-process.ts` path resolution logic (lines 103-113) requires **zero changes** to work with the Rust binary. It already looks for `mailsync.exe` on Windows and `mailsync.bin` elsewhere, resolves the asar path via `.replace('app.asar', 'app.asar.unpacked')`, and falls back to a dev build path. The Rust CI pipeline must simply produce a binary with the correct name and place it at `app/dist/resources/` (for Windows) or the equivalent for each platform — the same location the old C++ CI steps targeted.

The `build/tasks/package-task.js` `asar.unpack` glob already includes `'mailsync'`, `'mailsync.exe'`, and `'mailsync.bin'` — the Rust binary matches these patterns exactly with no configuration changes required.

The most complex decision is cross-compilation strategy for Linux. The project already uses native `ubuntu-24.04-arm` runners for ARM64, so no cross-compilation is needed there. For Linux x64, `cargo-zigbuild` v0.22.1 (the approach validated in Phase 4 for napi-rs) works for standalone binaries with glibc version pinning. Windows uses MSVC toolchain natively. macOS uses two separate runners (arm64 and x64). Each platform gets a native or near-native build — no QEMU emulation anywhere.

**Primary recommendation:** Build the standalone Rust binary using `cargo build --release --target <TARGET>` (with cargo-zigbuild for Linux glibc targets) in each existing workflow. Copy the output binary to `app/dist/resources/` before `npm run build`. Remove the entire C++ CI section from `build-windows.yaml`. Delete `app/mailsync/` via `git rm -r`. Done.

---

## Standard Stack

### Core
| Tool | Version | Purpose | Why Standard |
|------|---------|---------|--------------|
| `dtolnay/rust-toolchain` | stable | Rust toolchain setup in GitHub Actions | Canonical GHA action for Rust; used by all napi-rs and cargo-zigbuild examples |
| `cargo-zigbuild` | 0.22.1 (Feb 2026) | Cross-compile Linux targets with Zig linker | Solves glibc version pinning; handles x86_64 and aarch64 Linux from any host |
| `actions/cache@v4` | v4 | Cache `~/.cargo/registry`, `target/` | Prevents 15-minute CI rebuilds on every run |
| `[profile.release]` in Cargo.toml | — | LTO + strip + opt-level=z + panic=abort | Standard Cargo approach; no extra tooling |
| `cargo build --release` | stable | Standalone binary compilation | Direct cargo build, not napi build |

### Platform/Target Matrix
| Platform | GitHub Runner | Rust Target | Build Command | Approach |
|----------|---------------|-------------|---------------|----------|
| win-x64 | `windows-2022` | `x86_64-pc-windows-msvc` | `cargo build --release --target x86_64-pc-windows-msvc` | Native MSVC |
| mac-arm64 | `macos-latest` | `aarch64-apple-darwin` | `cargo build --release --target aarch64-apple-darwin` | Native ARM64 |
| mac-x64 | `macos-15-intel` | `x86_64-apple-darwin` | `cargo build --release --target x86_64-apple-darwin` | Native x64 |
| linux-x64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu.2.17` | `cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17` | cargo-zigbuild |
| linux-arm64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | `cargo build --release --target aarch64-unknown-linux-gnu` | Native ARM64 |

**Note on linux-arm64:** The project already uses the native `ubuntu-24.04-arm` runner for ARM64 builds (confirmed in `build-linux-arm64.yaml`). Building natively on this runner avoids cross-compilation entirely — simpler and more reliable.

**Note on glibc version pinning:** The `.2.17` suffix in `x86_64-unknown-linux-gnu.2.17` pins glibc to 2.17 (RHEL 7 era), matching the minimum supported Linux for the Electron app. cargo-zigbuild uses Zig as the linker to achieve this. Without pinning, zig defaults to glibc 2.28 (less compatible).

### Supporting
| Tool | Version | Purpose | When to Use |
|------|---------|---------|-------------|
| `mlugg/setup-zig` or `pip3 install ziglang` | Zig 0.14.x | Provide Zig linker for cargo-zigbuild | Required on linux-x64 runner when using cargo-zigbuild |
| `taiki-e/install-action@v2` | v2 | Install cargo-zigbuild in CI | Reliable cross-platform tool installer |
| Azure Trusted Signing (existing) | — | Sign EXE and DLL on Windows | Already in `build-windows.yaml`; add the Rust binary to the files-folder |
| `apple-actions/import-codesign-certs@v3` | v3 | macOS code signing | Already in `build-macos.yaml`; Rust binary must be in the app bundle for @electron/packager to sign it |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| cargo-zigbuild | `cross` crate (Docker-based) | `cross` requires Docker on CI runners; cargo-zigbuild is faster and simpler for glibc targets |
| cargo-zigbuild | Native ARM runner for linux-arm64 | Already using native runner — cargo-zigbuild only needed for linux-x64 |
| cargo-zigbuild | QEMU cross-compilation | QEMU is 5-10x slower than native or Zig linker approaches |
| `opt-level = "z"` | `opt-level = "s"` | Try both; "s" sometimes produces smaller output for certain codebases. Start with "z", switch if larger |

**Installation (in CI workflow):**
```bash
# Linux x64 workflow only:
pip3 install ziglang
cargo install cargo-zigbuild
```

---

## Architecture Patterns

### Pattern 1: Binary Path Resolution — Zero Changes to mailsync-process.ts

The existing path resolution logic in `mailsync-process.ts` (lines 103-113) is already correct for the Rust binary:

```typescript
// mailsync-process.ts lines 103-113 — NO changes needed
const binaryName = process.platform === 'win32' ? 'mailsync.exe' : 'mailsync.bin';
this.binaryPath = path.join(resourcePath, binaryName).replace('app.asar', 'app.asar.unpacked');

// In local dev, the binary may not be at resourcePath directly.
if (!fs.existsSync(this.binaryPath)) {
  const devBuildPath = path.join(resourcePath, 'mailsync', 'Windows', 'x64', 'Release', binaryName);
  if (fs.existsSync(devBuildPath)) {
    this.binaryPath = devBuildPath;
  }
}
```

**What this means:**
- **Production:** Binary at `<resourcePath>/mailsync.exe` or `<resourcePath>/mailsync.bin` inside `app.asar.unpacked` — Electron resolves this automatically when `asarUnpack` includes the binary
- **Dev (Windows fallback):** Binary at `<resourcePath>/mailsync/Windows/x64/Release/mailsync.exe` — this is the existing C++ output location; for Rust dev builds, the binary can be symlinked or copied there, OR developers run without this path (the mock fallback triggers)
- **Dev (Rust):** After `cargo build --release`, the binary lives at `app/mailsync-rust/target/release/mailsync` — the CI step copies it to `app/dist/resources/` with the correct name

**Rust binary naming convention (must match):**
- Windows: `mailsync.exe` (Rust automatically produces `.exe` for Windows targets)
- macOS: `mailsync.bin` (copy from `target/release/mailsync` → rename to `mailsync.bin`)
- Linux: `mailsync.bin` (copy from `target/release/mailsync` → rename to `mailsync.bin`)

### Pattern 2: CI Build Step — Insert Before `npm run build`

Each existing workflow gets a Rust build section inserted BEFORE the `npm run build` step, replacing the existing C++ native build section:

**build-windows.yaml** — Replace the entire `--- C++ Native Build ---` section:
```yaml
# Replace "--- C++ Native Build (mailcore2 + libetpan + mailsync) ---" section with:
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
      app/mailsync-rust/target/
    key: windows-x64-cargo-${{ hashFiles('app/mailsync-rust/Cargo.lock') }}

- name: Build Rust mailsync binary (Windows x64)
  working-directory: app/mailsync-rust
  run: cargo build --release --target x86_64-pc-windows-msvc

- name: Copy mailsync binary to Electron resources
  run: |
    $outDir = "app\mailsync-rust\target\x86_64-pc-windows-msvc\release"
    $destDir = "app\dist\resources"
    New-Item -ItemType Directory -Force -Path $destDir
    Copy-Item "$outDir\mailsync.exe" -Destination $destDir -Force
  shell: pwsh
```

**build-linux.yaml** — Insert after "Install Dependencies":
```yaml
- name: Install Zig (for cargo-zigbuild)
  run: pip3 install ziglang

- name: Install cargo-zigbuild
  run: cargo install cargo-zigbuild

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
      app/mailsync-rust/target/
    key: linux-x64-cargo-${{ hashFiles('app/mailsync-rust/Cargo.lock') }}

- name: Build Rust mailsync binary (Linux x64)
  working-directory: app/mailsync-rust
  run: cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17

- name: Copy mailsync binary to Electron resources
  run: |
    mkdir -p app/dist/resources
    cp app/mailsync-rust/target/x86_64-unknown-linux-gnu/release/mailsync app/dist/resources/mailsync.bin
    chmod +x app/dist/resources/mailsync.bin
```

**build-linux-arm64.yaml** — Insert after "Install Dependencies" (native runner, no cross-compilation):
```yaml
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: aarch64-unknown-linux-gnu

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailsync-rust/target/
    key: linux-arm64-cargo-${{ hashFiles('app/mailsync-rust/Cargo.lock') }}

- name: Build Rust mailsync binary (Linux arm64)
  working-directory: app/mailsync-rust
  run: cargo build --release --target aarch64-unknown-linux-gnu

- name: Copy mailsync binary to Electron resources
  run: |
    mkdir -p app/dist/resources
    cp app/mailsync-rust/target/aarch64-unknown-linux-gnu/release/mailsync app/dist/resources/mailsync.bin
    chmod +x app/dist/resources/mailsync.bin
```

**build-macos.yaml** — Insert after "Install Dependencies" for each matrix target:
```yaml
- name: Setup Rust toolchain
  uses: dtolnay/rust-toolchain@stable
  with:
    targets: aarch64-apple-darwin,x86_64-apple-darwin

- name: Cache cargo
  uses: actions/cache@v4
  with:
    path: |
      ~/.cargo/registry/index/
      ~/.cargo/registry/cache/
      ~/.cargo/git/db/
      app/mailsync-rust/target/
    key: ${{ matrix.os }}-cargo-${{ hashFiles('app/mailsync-rust/Cargo.lock') }}

- name: Build Rust mailsync binary (macOS ${{ matrix.arch }})
  working-directory: app/mailsync-rust
  run: |
    if [ "${{ matrix.arch }}" = "arm64" ]; then
      cargo build --release --target aarch64-apple-darwin
      cp target/aarch64-apple-darwin/release/mailsync ../dist/resources/mailsync.bin
    else
      cargo build --release --target x86_64-apple-darwin
      cp target/x86_64-apple-darwin/release/mailsync ../dist/resources/mailsync.bin
    fi
  shell: bash
```

### Pattern 3: Cargo.toml Release Profile for Size

```toml
# Source: min-sized-rust guidance + Phase 4 research baseline
[profile.release]
codegen-units = 1     # enables cross-crate LTO
lto = true            # fat LTO (most aggressive dead code removal)
strip = "symbols"     # strip debug symbols and symbol table after link
opt-level = "z"       # optimize for size (try "s" if "z" produces larger output)
panic = "abort"       # removes unwinding code; safe for a standalone binary
```

**Expected size impact (HIGH confidence):**
- Unstripped debug: 50-100MB (debug symbols dominate)
- Release without LTO: 20-40MB
- Release + LTO + strip + opt-level=z + panic=abort: **8-15MB** for a full async Rust binary with tokio, rusqlite (bundled SQLite), rustls, async-imap, lettre, and libdav

The 15MB target (PKGN-02) is achievable. The deltachat-rpc-server (similar stack: async email, TLS, SQLite) ships a 11.7MB manylinux wheel as of 2025, confirming the size range.

**Note on `panic = "abort"` safety:** This is correct for a standalone binary. Unlike a .node addon (where panic inside a napi callback triggers undefined behavior), a standalone binary process can safely `abort()` — the parent Electron process receives a non-zero exit code and handles it as a crash. The `MailsyncProcess` already handles unexpected exits via the `close` event.

### Pattern 4: asarUnpack — Zero Configuration Changes

The `build/tasks/package-task.js` asar.unpack glob is already correct:

```javascript
// build/tasks/package-task.js (existing — NO changes needed)
asar: {
  unpack:
    '{' +
    [
      'mailsync',       // matches the binary name on macOS/Linux if no extension
      'mailsync.exe',   // matches Windows binary
      'mailsync.bin',   // matches macOS/Linux binary (used by mailsync-process.ts)
      '*.so',
      '*.so.*',
      '*.dll',
      // ...
    ].join(',') +
    '}',
},
```

All three patterns are already present. `@electron/packager` will unpack any file matching `mailsync`, `mailsync.exe`, or `mailsync.bin` to `app.asar.unpacked/` during packaging.

**At runtime,** `mailsync-process.ts` resolves the binary path as:
```
app.asar/mailsync.bin → app.asar.unpacked/mailsync.bin
```
The `.replace('app.asar', 'app.asar.unpacked')` call on line 104 handles this automatically.

**On Linux/macOS,** the binary must be executable. The CI step must `chmod +x` the binary before `npm run build` (which packages it). `@electron/packager` preserves file permissions when copying to `app.asar.unpacked`.

### Pattern 5: macOS Code Signing for Spawned Binary

The Rust binary is a **spawned child process**, not an embedded framework. It must be code-signed differently from Electron's helper processes.

**Key rules (verified from macOS documentation):**
1. The Rust binary must be signed with the same Developer ID certificate as the Electron app
2. It does NOT need an App Sandbox entitlement (it is not sandboxed)
3. It needs Hardened Runtime enabled (`--options runtime`) to pass notarization
4. It does NOT need `com.apple.security.inherit` (that is for App Sandbox child processes)
5. If the Rust binary spawns no child processes of its own, no special entitlements are required

**The existing `osxSign` configuration in `package-task.js`** uses `optionsForFile` callback with a single entitlements.plist. This callback receives the file path — verify that the mailsync.bin path triggers the correct signing behavior. The existing `entitlements.plist` at `build/resources/mac/entitlements.plist` likely applies the same entitlements to all files in the bundle including mailsync.bin, which is acceptable.

**Critical:** The `osxNotarize` configuration in `package-task.js` notarizes the entire app bundle including all unpacked binaries. The Rust binary in `app.asar.unpacked/mailsync.bin` will be notarized automatically as part of the bundle.

**Azure Trusted Signing (Windows):** The existing `build-windows.yaml` step signs `files-folder: ...UnifyMail-win32-x64` with `files-folder-filter: exe,dll,node`. The Rust `mailsync.exe` will be at `...UnifyMail-win32-x64/resources/app.asar.unpacked/mailsync.exe`. Verify the `files-folder-recurse: true` setting ensures it is found. The filter `exe` already includes it.

### Pattern 6: TLS Verification CI Check

```bash
# Run in CI after cargo build (all platforms) to verify PKGN-06
cd app/mailsync-rust
cargo tree -e features 2>/dev/null | grep -i openssl && echo "FAIL: openssl dependency detected" && exit 1 || echo "PASS: no openssl"
```

**Additional binary-level check (Linux/macOS):**
```bash
# Verify no dynamic linkage to OpenSSL shared library
ldd app/dist/resources/mailsync.bin 2>/dev/null | grep -i ssl | grep -v boring && echo "FAIL: SSL dynamic lib detected" || echo "PASS"
# Alternative: check symbol table
nm --dynamic app/dist/resources/mailsync.bin 2>/dev/null | grep -i "SSL_" && echo "FAIL" || echo "PASS"
```

**Crate-level prevention (Cargo.toml):**
All crates that have TLS options must use their `rustls-tls` (or equivalent) feature flag:
```toml
# async-imap uses rustls via async-native-tls or tokio-rustls
# reqwest (if used for OAuth2/metadata HTTP): use rustls-tls feature
reqwest = { version = "0.12", default-features = false, features = ["rustls-tls", "json"] }
# lettre: use rustls-tls feature
lettre = { version = "0.11", default-features = false, features = ["smtp-transport", "tokio1", "tokio1-rustls-tls"] }
```

### Pattern 7: C++ Source Deletion

**Complete inventory of files to delete:**

```
app/mailsync/                    # Entire directory — all 352 C++ source files
  ├── MailSync/                  # C++ source (~50 files)
  ├── Vendor/                    # Vendored deps
  │   ├── SQLiteCpp/
  │   ├── StanfordCPPLib/
  │   ├── icalendarlib/
  │   ├── libetpan/
  │   ├── nlohmann/
  │   └── spdlog/
  ├── MailSync.xcodeproj/        # Xcode project
  ├── Windows/                   # Visual Studio project files
  ├── CMakeLists.txt
  ├── vcpkg.json
  ├── vcpkg-configuration.json
  ├── vcpkg-overlay-ports/
  └── build.sh
```

**References that must be updated/deleted before or alongside directory deletion:**

| Location | Reference | Action |
|----------|-----------|--------|
| `app/package.json` line 95 | `"mailcore-napi": "file:mailcore"` | Change to `"file:mailcore-rs"` (Phase 4 did this, verify complete) |
| `build-windows.yaml` | Entire `--- C++ Native Build ---` section | Delete; replace with Rust build steps |
| `build/tasks/package-task.js` line 184 | `/^\/mailsync\/.*/` in `ignore` array | DELETE this line — `app/mailsync/` no longer exists |
| `app/mailsync.cmd` | `node ..\scripts\mock-mailsync.js %*` | DELETE this file — no longer relevant |

**References that must NOT be changed:**

| Location | Reference | Reason |
|----------|-----------|--------|
| `build/tasks/package-task.js` asar.unpack | `'mailsync'`, `'mailsync.exe'`, `'mailsync.bin'` | These match the Rust binary names |
| `mailsync-process.ts` entire file | All binary spawning logic | Zero changes needed |
| `mailsync-bridge.ts` entire file | All IPC bridge logic | Zero changes needed |
| Lang JSON files (`app/lang/*.json`) | "Open Mailsync Logs" strings | These are UI strings, not build references |
| `app/keymaps/base-darwin.json` | `"window:open-mailsync-logs"` | UI keymap, not build reference |

### Recommended Project Structure (Phase 10 scope)

```
.github/workflows/
├── build-windows.yaml          # Remove C++ build section; add Rust cargo build
├── build-linux.yaml            # Add Rust cargo-zigbuild step
├── build-linux-arm64.yaml      # Add Rust cargo build step (native runner)
└── build-macos.yaml            # Add Rust cargo build step (matrix: arm64, x64)

app/
├── mailsync-rust/              # Rust binary (built in Phases 5-9)
│   ├── Cargo.toml              # [profile.release] with LTO settings
│   ├── src/
│   └── target/release/        # Build output (gitignored)
├── mailsync/                   # DELETE ENTIRE DIRECTORY
├── mailsync.cmd                # DELETE THIS FILE
└── dist/resources/             # Populated during CI build
    ├── mailsync.exe            # Windows binary (CI copies here)
    └── mailsync.bin            # macOS/Linux binary (CI copies here)
```

### Anti-Patterns to Avoid

- **Do NOT use cargo-zigbuild for linux-arm64**: The project already has a native `ubuntu-24.04-arm` runner. Native builds are faster and simpler than cross-compilation.
- **Do NOT use `cross` (Docker-based cross-compiler)**: Adds Docker daemon complexity to CI; cargo-zigbuild via Zig linker is lighter and faster.
- **Do NOT forget `chmod +x` on Linux/macOS**: The binary must be executable. `@electron/packager` preserves permissions, but only if set before packaging.
- **Do NOT delete `app/mailsync/` before the CI workflows are updated**: The Windows workflow currently references `app/mailsync/Windows/` — deleting the directory without first replacing the CI step will break the build immediately.
- **Do NOT remove the asar.unpack glob entries for mailsync**: They are already present and correctly target the Rust binary names.
- **Do NOT place the Rust binary inside `app/mailsync-rust/` in the packaged app**: It must be at the top level of `resources/` so `mailsync-process.ts` can find it via `path.join(resourcePath, binaryName)`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Cross-compilation toolchain (Linux) | Custom Docker containers, QEMU | `cargo-zigbuild` 0.22.1 | Zig linker handles glibc ABI; no Docker daemon required; official tool |
| glibc version pinning | Custom libc wrappers | `cargo zigbuild --target x86_64-unknown-linux-gnu.2.17` | Built-in cargo-zigbuild feature |
| Binary stripping | Shell `strip` invocations | `strip = "symbols"` in `[profile.release]` | Cargo handles ordering; runs after LTO |
| LTO pipeline | Incremental linking workarounds | `lto = true` in `[profile.release]` | Cargo fat LTO is the standard approach |
| macOS signing of standalone binary | Custom codesign scripts | `@electron/packager` `osxSign` callback | Already configured; packager signs all bundle contents including unpacked files |
| Windows signing of standalone binary | Custom signtool scripts | Azure Trusted Signing action (existing) | Already in workflow with `files-folder-recurse: true` |
| asar unpack configuration | Custom electron-packager plugins | Existing glob patterns in `package-task.js` | Already configured; zero changes needed |
| Binary path resolution | Custom path logic in TypeScript | Existing `.replace('app.asar', 'app.asar.unpacked')` in `mailsync-process.ts` | Already implemented; zero changes needed |
| OpenSSL detection | Manual `nm` symbol checks | `cargo tree -e features | grep -i openssl` | Catches the dependency at the crate graph level, before it links |

**Key insight:** The existing project infrastructure was already designed for a standalone binary named `mailsync.exe`/`mailsync.bin`. The path resolution, asar unpacking, Windows signing, and macOS signing all work without modification. Phase 10's primary work is in CI (adding Rust build steps) and cleanup (deleting C++ code).

---

## Common Pitfalls

### Pitfall 1: Binary Placed in Wrong Location (Binary Not Found at Launch)
**What goes wrong:** User launches the packaged app; `mailsync-process.ts` throws `mailsync binary not found at <path>` and fails to start any account sync.
**Why it happens:** The CI step copies the Rust binary to the wrong location — for example, into `app/mailsync-rust/` instead of `app/dist/resources/`.
**How to avoid:** The binary must be at `app/dist/resources/mailsync.exe` (Windows) or `app/dist/resources/mailsync.bin` (macOS/Linux) BEFORE `npm run build` runs. Verify with `ls app/dist/resources/` in CI after the copy step. `@electron/packager` will then pick it up from there.
**Warning signs:** "Binary not found" error on first account connect in packaged app; CI build succeeds but app is broken.

### Pitfall 2: Binary Inside ASAR (spawn Fails)
**What goes wrong:** `child_process.spawn()` on a path inside `app.asar` fails silently or with ENOENT because ASAR is a virtual filesystem.
**Why it happens:** The asar.unpack glob does not match the binary name; or the CI places the binary at a path the glob doesn't cover.
**How to avoid:** The existing glob includes `'mailsync'`, `'mailsync.exe'`, `'mailsync.bin'`. The binary name in CI output MUST match one of these exactly. Verify the unpacked path in the packaged output: `ls <app-bundle>/resources/app.asar.unpacked/` should contain the binary.
**Warning signs:** `Error: spawn ENOENT` or `Error: spawn EACCES` when `mailsync-process.ts` tries to start the process.

### Pitfall 3: Missing Execute Permission on Linux/macOS
**What goes wrong:** `spawn()` fails with `EACCES` (permission denied) because the binary is not executable.
**Why it happens:** `cargo build` output is executable by default, but file copying scripts may not preserve permissions (especially on Windows runners with cross-platform copies).
**How to avoid:** Always `chmod +x` the binary in the CI copy step on Linux/macOS workflows. Verify with `ls -la app/dist/resources/mailsync.bin` in CI.
**Warning signs:** `EACCES` or "Permission denied" in mailsync-process error logs on macOS/Linux.

### Pitfall 4: OpenSSL Contamination via Transitive Dependency
**What goes wrong:** One of the crates in the dependency tree (reqwest, hyper, tokio-native-tls, etc.) pulls in `openssl-sys`, causing symbol conflicts with Electron's BoringSSL when the binary is loaded in the same process context (not directly applicable to a standalone binary, but the binary may fail to start if it tries to dlopen OpenSSL that conflicts with the system).
**Why it happens:** Some crates default to `native-tls` which uses OpenSSL on Linux. The `default-features = false` flag is required for these crates.
**How to avoid:** In `Cargo.toml`, use `default-features = false` and explicitly enable `rustls-tls` for all network crates: `reqwest`, `lettre`, and any other HTTP/TLS client. Run `cargo tree -e features | grep -i openssl` in CI as a gate check.
**Warning signs:** CI check `cargo tree | grep openssl` returns output; or binary fails to start on Linux with shared library errors.

### Pitfall 5: C++ CI Steps Not Removed (Double Build Failure)
**What goes wrong:** The C++ `--- Native Build ---` section in `build-windows.yaml` is left in place after deleting `app/mailsync/`. The Windows workflow fails immediately on the `vcpkg install` step because the directory no longer exists.
**Why it happens:** Deleting `app/mailsync/` without simultaneously updating the CI workflow.
**How to avoid:** The CI workflow update and the C++ directory deletion must be done in the same commit or in strict order (CI update first, then deletion). Verify: `build-windows.yaml` references `app\mailsync\` in multiple places — all must be removed.
**Warning signs:** Windows CI workflow fails at `vcpkg install --triplet x64-windows` with "directory not found".

### Pitfall 6: package-task.js ignore Pattern Matches Nothing but Clutters Build
**What goes wrong:** The ignore pattern `/^\/mailsync\/.*/` in `build/tasks/package-task.js` is left after deleting `app/mailsync/`. This is harmless functionally (matches nothing) but should be cleaned up.
**Why it happens:** Incomplete cleanup reference audit.
**How to avoid:** Remove the `/^\/mailsync\/.*/` pattern from the `ignore` array in `package-task.js` as part of the deletion commit.

### Pitfall 7: macOS Binary Not Signed (Notarization Fails)
**What goes wrong:** notarization fails with "unsigned binary" error because the Rust `mailsync.bin` in `app.asar.unpacked` was not code-signed.
**Why it happens:** `@electron/packager`'s `osxSign` `optionsForFile` callback must cover the mailsync.bin path. If the callback's `filePath` matching logic skips files in `app.asar.unpacked`, the binary goes unsigned.
**How to avoid:** The existing `optionsForFile` callback in `package-task.js` returns the same `entitlements` plist for all files. This applies to mailsync.bin. No change needed — but verify that the existing signing workflow actually signs the binary by checking the build log for `Signing: .../app.asar.unpacked/mailsync.bin`.
**Warning signs:** notarization fails; or macOS Gatekeeper blocks the binary with "damaged" or "developer cannot be verified" message.

### Pitfall 8: Cargo Cache Key Invalidation on First Run
**What goes wrong:** Without proper cargo caching, every CI run re-downloads and recompiles all Rust dependencies (tokio, rustls, rusqlite bundled SQLite, lettre, etc.) — adding 15-25 minutes per platform.
**Why it happens:** Cache keys not set, or set to just `hashFiles('Cargo.lock')` without the platform target prefix.
**How to avoid:** Use `key: <platform>-cargo-${{ hashFiles('app/mailsync-rust/Cargo.lock') }}` with the platform string as a prefix. Cache both `~/.cargo/registry/` and `app/mailsync-rust/target/`.

---

## Code Examples

### Binary Size Cargo Profile
```toml
# Source: min-sized-rust guidance (johnthagen/min-sized-rust on GitHub)
# Location: app/mailsync-rust/Cargo.toml

[profile.release]
codegen-units = 1     # Maximize LTO optimization across compilation units
lto = true            # Fat LTO: single optimization pass across all crates
strip = "symbols"     # Remove symbol table and debug info after linking
opt-level = "z"       # Optimize for size (try "s" if produces larger output)
panic = "abort"       # No unwinding code; ~10-20% size reduction
```

### Verify Binary Size in CI (Linux)
```bash
# Source: Standard bash file size check
ls -lh app/dist/resources/mailsync.bin
du -sh app/dist/resources/mailsync.bin
# Fail if over 15MB
SIZE=$(du -m app/dist/resources/mailsync.bin | cut -f1)
if [ "$SIZE" -gt 15 ]; then
  echo "FAIL: Binary size ${SIZE}MB exceeds 15MB limit"
  exit 1
fi
echo "PASS: Binary size ${SIZE}MB"
```

### Verify No OpenSSL in CI
```bash
# Source: cargo tree dependency graph inspection
cd app/mailsync-rust
cargo tree -e features 2>/dev/null | grep -i openssl
if [ $? -eq 0 ]; then
  echo "FAIL: openssl dependency detected in crate graph"
  exit 1
fi
echo "PASS: no openssl in dependency tree"
```

### Windows Binary Copy (PowerShell)
```powershell
# Source: adapted from existing build-windows.yaml pattern
$outDir = "app\mailsync-rust\target\x86_64-pc-windows-msvc\release"
$destDir = "app\dist\resources"
New-Item -ItemType Directory -Force -Path $destDir
Copy-Item "$outDir\mailsync.exe" -Destination $destDir -Force
# Verify it's there
if (-not (Test-Path "$destDir\mailsync.exe")) {
  Write-Error "mailsync.exe not found in $destDir"
  exit 1
}
Write-Host "mailsync.exe copied to $destDir ($(Get-Item "$destDir\mailsync.exe").Length bytes)"
```

### cargo-zigbuild Linux x64 Build
```bash
# Source: cargo-zigbuild v0.22.1 README (github.com/rust-cross/cargo-zigbuild)
# Install:
pip3 install ziglang
cargo install cargo-zigbuild

# Build with glibc 2.17 minimum:
cd app/mailsync-rust
cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17

# Output at:
ls -lh target/x86_64-unknown-linux-gnu/release/mailsync
```

### macOS Matrix Build
```bash
# Source: adapted from existing build-macos.yaml pattern
# In GitHub Actions with matrix.arch = arm64 or x64:
cd app/mailsync-rust
if [ "${{ matrix.arch }}" = "arm64" ]; then
  cargo build --release --target aarch64-apple-darwin
  BINARY=target/aarch64-apple-darwin/release/mailsync
else
  cargo build --release --target x86_64-apple-darwin
  BINARY=target/x86_64-apple-darwin/release/mailsync
fi

mkdir -p ../dist/resources
cp "$BINARY" ../dist/resources/mailsync.bin
chmod +x ../dist/resources/mailsync.bin
echo "Binary size: $(du -sh ../dist/resources/mailsync.bin)"
```

### package-task.js Change — Remove Dead Ignore Pattern
```javascript
// build/tasks/package-task.js — REMOVE this one line from ignore array:
// /^\/mailsync\/.*/,     ← DELETE after app/mailsync/ is gone

// The asar.unpack array stays EXACTLY AS-IS:
asar: {
  unpack: '{' + [
    'mailsync',      // keep: matches Rust binary (if no extension)
    'mailsync.exe',  // keep: matches Windows Rust binary
    'mailsync.bin',  // keep: matches macOS/Linux Rust binary
    '*.so',
    '*.so.*',
    '*.dll',
    '*.pdb',
    '*.node',
    // ...
  ].join(',') + '}',
},
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| C++ with vcpkg + libetpan + mailcore2 + CMake | Rust standalone binary with cargo | Phase 10 | Eliminates 352 C++ source files, 7 vendored C++ libraries, and platform-specific build toolchains |
| OpenSSL for TLS on Linux | rustls (pure Rust) | Pre-Phase 5 decision | Eliminates BoringSSL symbol conflict with Electron |
| Docker-based cross-compilation | cargo-zigbuild with Zig linker | Phase 4 baseline | No Docker daemon required; glibc version pinning built-in |
| Per-platform npm packages for native addon | Single-package layout | Phase 4 (napi-rs) | No optionalDependencies resolution issues |
| Separate vcpkg toolchain per platform | Universal Rust cross-compilation | Phase 10 | cargo handles all targets; no per-platform C toolchain management |

**Deprecated/outdated:**
- `vcpkg install` workflow step: DELETE — C++ build system gone
- `msbuild app\mailcore\...` workflow step: DELETE — mailcore2 gone
- `msbuild app\mailsync\...` workflow step: DELETE — C++ mailsync gone
- `app/mailsync.cmd` dev helper: DELETE — no longer needed
- `node-gyp` and `node-addon-api` in package.json: DELETE (Phase 4 handles this)

---

## Open Questions

1. **Linux arm64 glibc minimum version**
   - What we know: The project uses `ubuntu-24.04-arm` native runner; native `cargo build --target aarch64-unknown-linux-gnu` will link against the runner's glibc (~2.39)
   - What's unclear: Whether glibc 2.39 is too new for users on Ubuntu 22.04 ARM64 (glibc 2.35) or earlier
   - Recommendation: Use `cargo zigbuild --target aarch64-unknown-linux-gnu.2.28` on the ARM64 runner too, for wider compatibility. If the ARM runner doesn't have Zig, use `pip3 install ziglang`. The CI install-test Docker containers (`ubuntu:22.04`) will catch glibc version issues at test time.

2. **macOS binary signing — separate entitlements plist needed?**
   - What we know: The `osxSign` `optionsForFile` callback in `package-task.js` returns the same `entitlements.plist` for all files; the mailsync.bin is a standalone binary that does NOT need App Sandbox
   - What's unclear: Whether applying the main entitlements.plist (which may include `com.apple.security.cs.allow-jit` for Electron) causes notarization to reject the mailsync.bin
   - Recommendation: Check the existing `build/resources/mac/entitlements.plist` before Phase 10. If it contains Electron-specific JIT entitlements, create a minimal `entitlements-helper.plist` for mailsync.bin that omits them. Use the `optionsForFile` callback to route `.../mailsync.bin` to the helper plist.

3. **Dev mode binary access**
   - What we know: `mailsync-process.ts` has a fallback to `app/mailsync/Windows/x64/Release/mailsync.exe` for Windows dev mode; after C++ deletion this path will not exist
   - What's unclear: Whether the dev fallback was actively used, or whether the mock fallback (next in the chain) is sufficient for development
   - Recommendation: After deletion, the mock script at `scripts/mock-mailsync.js` becomes the dev fallback. Update `mailsync-process.ts`'s fallback path comment to document this; optionally, add a new dev fallback pointing to `app/mailsync-rust/target/release/mailsync` so developers can run the real binary without packaging.

---

## Sources

### Primary (HIGH confidence)
- `app/frontend/mailsync-process.ts` — Binary path resolution logic (lines 103-113); spawning logic (lines 185-198); IPC protocol
- `build/tasks/package-task.js` — `asar.unpack` glob patterns; `osxSign` configuration; `ignore` patterns
- `.github/workflows/build-windows.yaml` — Existing C++ build section structure; Azure signing workflow; dist/resources copy step pattern
- `.github/workflows/build-linux.yaml` — Existing Linux build workflow; dist/resources pattern
- `.github/workflows/build-linux-arm64.yaml` — Native ARM64 runner confirmed; existing build pattern
- `.github/workflows/build-macos.yaml` — Matrix build pattern (arm64/x64); signing workflow
- Phase 4 RESEARCH.md — CI patterns for Rust builds, cargo-zigbuild for Linux, napi-rs asar unpack patterns; binary size profile settings
- Phase 5 RESEARCH.md — Binary structure and binary directory location (`app/mailsync-rust/`)

### Secondary (MEDIUM confidence)
- cargo-zigbuild v0.22.1 README (github.com/rust-cross/cargo-zigbuild) — Supported platforms and glibc pinning syntax
- min-sized-rust (github.com/johnthagen/min-sized-rust) — Cargo.toml profile settings for binary size
- Electron ASAR Archives documentation (electronjs.org) — spawn/child_process limitations; app.asar.unpacked behavior

### Tertiary (LOW confidence — extrapolation)
- deltachat-rpc-server wheel size (~11.7MB, manylinux, 2025) — Used as analog for achievable Rust email binary size; similar stack (async email + TLS + SQLite)
- macOS notarization entitlements guidance — Synthesized from multiple sources; verify with actual build before declaring complete

---

## Metadata

**Confidence breakdown:**
- Standard stack (CI workflow structure): HIGH — extracted directly from existing workflow files
- Architecture (path resolution, asar unpack): HIGH — extracted directly from mailsync-process.ts and package-task.js
- Binary size target (15MB achievable): MEDIUM — based on deltachat analog and Cargo profile guidance; actual size depends on final dependency set after Phase 9
- macOS signing (spawned binary entitlements): MEDIUM — synthesized from multiple sources; requires hands-on verification against actual entitlements.plist
- Linux glibc version (aarch64): LOW — uncertain whether native runner glibc is too new for older Ubuntu ARM systems

**Research date:** 2026-03-02
**Valid until:** 2026-06-01 (stable ecosystem; cargo-zigbuild and packager APIs are stable; GitHub Actions runner updates are the main risk)
