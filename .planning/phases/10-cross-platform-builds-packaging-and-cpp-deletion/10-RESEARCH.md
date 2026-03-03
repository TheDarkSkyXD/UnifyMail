# Phase 10: Cross-Platform Builds, Packaging, and C++ Deletion - Research

**Researched:** 2026-03-02 (deep-dive update: 2026-03-03)
**Domain:** Rust standalone binary CI cross-compilation, Electron asar unpacking, binary size optimization, macOS code signing, C++ source deletion
**Confidence:** HIGH (CI patterns extracted from existing project workflows; path resolution logic extracted directly from mailsync-process.ts; asar unpack patterns extracted from package-task.js; deep-dive audit resolves all three prior open questions)

---

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|-----------------|
| PKGN-01 | Cross-platform builds for 5 targets: win-x64, mac-arm64, mac-x64, linux-x64, linux-arm64 | CI patterns extracted from existing 4 workflow files; Rust targets and runner assignments documented; cargo-zigbuild v0.22.1 for BOTH Linux targets (x64 AND arm64 — see resolved open question 1) |
| PKGN-02 | Release binary under 15MB with LTO and strip | LTO + strip + opt-level=z + panic=abort profile documented; 15MB is achievable for full async Rust stack (tokio + TLS + SQLite + IMAP/SMTP) |
| PKGN-03 | electron-builder asarUnpack configured for Rust mailsync binary | `mailsync`, `mailsync.exe`, `mailsync.bin` already in asar.unpack glob in package-task.js; Rust binary names match existing glob patterns |
| PKGN-04 | `mailsync-process.ts` spawns Rust binary via existing path resolution logic | Path resolution in mailsync-process.ts lines 103-113 requires ONE small change (Windows dev fallback path, lines 108-113); production path resolution is untouched |
| PKGN-05 | All C++ mailsync source, vendored dependencies, and build configs deleted | Complete audit: `app/mailsync/` (C++ source), `app/mailsync/Vendor/` (SQLiteCpp, libetpan, spdlog, nlohmann, etc.), `app/mailsync/MailSync.xcodeproj/`, `app/mailsync/Windows/`, CI workflow C++ sections; also: C++ system deps removed from apt-get install |
| PKGN-06 | TLS via rustls exclusively (no OpenSSL symbols) | `cargo tree -e features \| grep -i openssl` CI check; all prior phases must use `rustls-tls` feature flags; async-imap, lettre, reqwest all support rustls-tls features |
</phase_requirements>

---

## Summary

Phase 10 completes the v2.0 rewrite by shipping the Rust mailsync binary to users on all 5 platforms and permanently removing the C++ codebase. The work divides cleanly into four streams: (1) CI — insert `cargo build --release --target <TARGET>` steps into existing workflows; (2) binary placement — copy the Rust binary to a location the existing path resolution code already expects; (3) binary size validation — profile.release settings must produce a stripped binary under 15MB; and (4) C++ deletion — remove `app/mailsync/` in its entirety plus targeted references in workflows and package.json.

Deep-dive investigation resolved all three prior open questions. First, the linux-arm64 runner (ubuntu-24.04-arm, glibc ~2.39) DOES need cargo-zigbuild with glibc version pinning to `.2.28` — the native runner's glibc is too new for Ubuntu 22.04 ARM64 systems (glibc 2.35). Second, the existing `build/resources/mac/entitlements.plist` is already correct for the Rust binary — unused entitlements such as `allow-jit` are permission grants that Rust simply ignores, and the provisioning profile issue was already resolved by disabling provisioning profiles entirely. Third, `mailsync-process.ts` requires ONE small change: the Windows dev fallback path (lines 108-113) points to the deleted C++ directory and should be updated to the Rust build output.

Three significant new findings emerged: (1) The C++ Linux binary used a shell script wrapper (`mailsync`) that set `SASL_PATH` and `LD_LIBRARY_PATH` — the Rust binary needs no such wrapper because it statically links everything. (2) Linux and macOS CI workflows never had a C++ build section (C++ was built separately via Travis CI and distributed via S3 tarballs) — Phase 10 adds Rust build steps rather than replacing C++ steps on these platforms. (3) The Windows `*.dll` copy step brings OpenSSL/curl/zlib DLLs that are no longer needed with a statically-linked Rust binary.

**Primary recommendation:** Build the standalone Rust binary using `cargo zigbuild --release --target <TARGET>` (with cargo-zigbuild for BOTH Linux targets with glibc version pinning) in each existing workflow. Copy the output binary to `app/dist/resources/` before `npm run build`. Replace the entire C++ build section in `build-windows.yaml`. Add Rust build steps to Linux and macOS workflows (no C++ section to remove there). Delete `app/mailsync/` via `git rm -r`. Remove C++ system dependencies from `apt-get install`. Done.

---

## Standard Stack

### Core
| Tool | Version | Purpose | Why Standard |
|------|---------|---------|--------------|
| `dtolnay/rust-toolchain` | stable | Rust toolchain setup in GitHub Actions | Canonical GHA action for Rust; used by all napi-rs and cargo-zigbuild examples |
| `cargo-zigbuild` | 0.22.1 (Feb 2026) | Cross-compile Linux targets with Zig linker + glibc pinning | Solves glibc version pinning; handles x86_64 AND aarch64 Linux from any host |
| `actions/cache@v4` | v4 | Cache `~/.cargo/registry`, `target/` | Prevents 15-minute CI rebuilds on every run |
| `[profile.release]` in Cargo.toml | — | LTO + strip + opt-level=z + panic=abort | Standard Cargo approach; no extra tooling |
| `cargo build --release` | stable | Standalone binary compilation | Direct cargo build, not napi build |

### Platform/Target Matrix

| Platform | GitHub Runner | Rust Target | Build Command | Approach | glibc Note |
|----------|---------------|-------------|---------------|----------|------------|
| win-x64 | `windows-2022` | `x86_64-pc-windows-msvc` | `cargo build --release --target x86_64-pc-windows-msvc` | Native MSVC | N/A |
| mac-arm64 | `macos-latest` | `aarch64-apple-darwin` | `cargo build --release --target aarch64-apple-darwin` | Native ARM64 | N/A |
| mac-x64 | `macos-15-intel` | `x86_64-apple-darwin` | `cargo build --release --target x86_64-apple-darwin` | Native x64 | N/A |
| linux-x64 | `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17` | cargo-zigbuild | .2.17 suffix: RHEL 7 era minimum |
| linux-arm64 | `ubuntu-24.04-arm` | `aarch64-unknown-linux-gnu` | `cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.28` | cargo-zigbuild | .2.28 suffix: needed because runner glibc ~2.39 is too new for Ubuntu 22.04 (glibc 2.35) |

**RESOLVED — linux-arm64 glibc:** The native `ubuntu-24.04-arm` runner links against glibc ~2.39. Ubuntu 22.04 ARM64 has glibc 2.35. A binary requiring glibc 2.39 will FAIL with `GLIBC_2.38 not found` on Ubuntu 22.04 (glibc is forward-compatible only). The Docker test matrix in `build-linux-arm64.yaml` (`matrix.ubuntu_version: ['22.04', '24.04', '25.04']`) will catch this failure at test time, but the fix is to use `cargo zigbuild --target aarch64-unknown-linux-gnu.2.28` on the ARM64 runner so the binary is compatible with Ubuntu 22.04. glibc 2.28 is the Debian Buster baseline (widely supported ARM64 systems).

**Note on linux-x64 glibc:** The `.2.17` suffix targets glibc 2.17 (RHEL 7 era), matching the minimum supported Linux for the Electron app. cargo-zigbuild uses Zig as the linker to achieve this.

### Supporting
| Tool | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `pip3 install ziglang` | Zig 0.14.x | Provide Zig linker for cargo-zigbuild | Required on BOTH Linux runners when using cargo-zigbuild |
| `taiki-e/install-action@v2` | v2 | Install cargo-zigbuild in CI | Reliable cross-platform tool installer (alternative to `cargo install cargo-zigbuild`) |
| Azure Trusted Signing (existing) | — | Sign EXE on Windows | Already in `build-windows.yaml`; `files-folder-recurse: true` with `exe` filter already covers `mailsync.exe` inside `app.asar.unpacked` |
| `apple-actions/import-codesign-certs@v3` | v3 | macOS code signing | Already in `build-macos.yaml`; Rust binary signed automatically via `osxSign` `optionsForFile` callback |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| cargo-zigbuild (linux-arm64) | Native `cargo build` without glibc pinning | Fails on Ubuntu 22.04 ARM64 (glibc 2.35) because native runner is glibc ~2.39 |
| cargo-zigbuild (linux-x64) | `cross` crate (Docker-based) | `cross` requires Docker on CI runners; cargo-zigbuild is faster and simpler for glibc targets |
| cargo-zigbuild | QEMU cross-compilation | QEMU is 5-10x slower than native or Zig linker approaches |
| `opt-level = "z"` | `opt-level = "s"` | Try both; "s" sometimes produces smaller output for certain codebases. Start with "z", switch if larger |
| Separate entitlements.plist for mailsync.bin | Single entitlements.plist for all files | Not needed: unused entitlements are permission grants, not requirements. Rust binary ignores allow-jit. |

**Installation (in CI workflow, both Linux runners):**
```bash
pip3 install ziglang
cargo install cargo-zigbuild
```

---

## Architecture Patterns

### Pattern 1: Binary Path Resolution — One Small Change to mailsync-process.ts

The existing path resolution logic in `mailsync-process.ts` is almost entirely correct for the Rust binary. The PRODUCTION path (lines 103-107) requires zero changes. The WINDOWS DEV FALLBACK (lines 108-113) points to the deleted C++ directory and should be updated.

```typescript
// mailsync-process.ts lines 103-113 — ANNOTATED with what changes

// Lines 103-104: PRIMARY PATH — NO changes needed
// Works in production; resolves to app.asar.unpacked correctly
const binaryName = process.platform === 'win32' ? 'mailsync.exe' : 'mailsync.bin';
this.binaryPath = path.join(resourcePath, binaryName).replace('app.asar', 'app.asar.unpacked');

// Lines 108-113: WINDOWS DEV FALLBACK — UPDATE this path
// OLD (C++ output directory — will no longer exist after deletion):
if (!fs.existsSync(this.binaryPath)) {
  const devBuildPath = path.join(resourcePath, 'mailsync', 'Windows', 'x64', 'Release', binaryName);
  if (fs.existsSync(devBuildPath)) {
    this.binaryPath = devBuildPath;
  }
}

// NEW (Rust output directory):
if (!fs.existsSync(this.binaryPath)) {
  const devBuildPath = path.join(resourcePath, '..', 'mailsync-rust', 'target', 'release', binaryName);
  if (fs.existsSync(devBuildPath)) {
    this.binaryPath = devBuildPath;
  }
}

// Lines 184-198: MOCK FALLBACK — NO changes needed
// This continues to work for all developers who don't build the Rust binary locally
const mockPath = path.resolve(this.resourcePath, '..', 'scripts', 'mock-mailsync.js');
if (fs.existsSync(this.binaryPath)) {
  this._proc = spawn(this.binaryPath, args, { env });
} else if (fs.existsSync(mockPath)) {
  // Falls back to mock-mailsync.js — always available
}
```

**Summary of changes needed in mailsync-process.ts:**
- Production path resolution (lines 103-107): **no change**
- asar path resolution (`.replace('app.asar', 'app.asar.unpacked')`): **no change**
- Binary naming (`mailsync.exe` / `mailsync.bin`): **no change**
- Spawn logic (lines 184-198): **no change**
- IPC protocol: **no change**
- Windows dev fallback path (lines 109): **ONE LINE CHANGE** — update directory path

**Rust binary naming convention (must match):**
- Windows: `mailsync.exe` (Rust automatically produces `.exe` for Windows targets)
- macOS: `mailsync.bin` (copy from `target/release/mailsync` → rename to `mailsync.bin`)
- Linux: `mailsync.bin` (copy from `target/release/mailsync` → rename to `mailsync.bin`)

### Pattern 2: CI Build Step — Insert Before `npm run build`

Each existing workflow gets a Rust build section inserted BEFORE the `npm run build` step.

**IMPORTANT platform difference:** Windows has an existing C++ build section to REPLACE. Linux and macOS workflows have NO C++ build section — the C++ binary was built separately via Travis CI and distributed via S3 tarballs. Linux and macOS workflows only need new Rust build steps ADDED.

**build-windows.yaml** — REPLACE the entire `--- C++ Native Build ---` section AND remove the `*.dll` copy:
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
    # DO NOT copy *.dll — Rust binary is fully statically linked; no DLLs needed
    if (-not (Test-Path "$destDir\mailsync.exe")) {
      Write-Error "mailsync.exe not found in $destDir"; exit 1
    }
    Write-Host "mailsync.exe: $(Get-Item "$destDir\mailsync.exe" | Select-Object -ExpandProperty Length) bytes"
  shell: pwsh
```

**build-linux.yaml** — ADD Rust build steps (no C++ section to remove); also REMOVE C++ system deps from apt-get install:
```yaml
# Remove from apt-get install (C++ only, no longer needed):
#   cmake libcurl4-openssl-dev libssl-dev libsasl2-dev libsasl2-modules
#   libsasl2-modules-gssapi-mit libc-ares-dev libctemplate-dev libtidy-dev
#   libxml2-dev libicu-dev autoconf automake libtool clang uuid-dev

# Keep in apt-get install (Electron/system still needed):
#   build-essential fakeroot rpm git libsecret-1-dev libnss3 libnss3-dev
#   libxext-dev libxkbfile-dev libxtst-dev pkg-config xvfb software-properties-common

# Add these new steps after "Install Dependencies":
- name: Install Zig (for cargo-zigbuild glibc pinning)
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
    ls -lh app/dist/resources/mailsync.bin
```

**build-linux-arm64.yaml** — ADD Rust build steps (no C++ section to remove); also REMOVE C++ system deps; also NEEDS cargo-zigbuild for glibc pinning:
```yaml
# Same apt-get cleanup as linux-x64 (remove C++ deps, keep Electron/system deps)

# Add these new steps:
- name: Install Zig (for cargo-zigbuild glibc pinning)
  run: pip3 install ziglang

- name: Install cargo-zigbuild
  run: cargo install cargo-zigbuild

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
  # NOTE: cargo zigbuild required — NOT plain cargo build
  # Runner glibc ~2.39 would produce a binary incompatible with Ubuntu 22.04 (glibc 2.35)
  # .2.28 pins to Debian Buster baseline — compatible with all supported ARM64 systems
  run: cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.28

- name: Copy mailsync binary to Electron resources
  run: |
    mkdir -p app/dist/resources
    cp app/mailsync-rust/target/aarch64-unknown-linux-gnu/release/mailsync app/dist/resources/mailsync.bin
    chmod +x app/dist/resources/mailsync.bin
    ls -lh app/dist/resources/mailsync.bin
```

**build-macos.yaml** — ADD Rust build steps (no C++ section to remove):
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
    chmod +x ../dist/resources/mailsync.bin
    ls -lh ../dist/resources/mailsync.bin
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

**Note on *.dll glob:** The existing asar.unpack glob includes `*.dll`. With the Rust statically-linked binary, there are no DLLs to unpack. This glob entry becomes a no-op for the mailsync DLLs but may still be needed for other native modules — leave it in place.

### Pattern 5: macOS Code Signing — Existing Configuration is Correct

**RESOLVED — macOS entitlements plist:** The existing `build/resources/mac/entitlements.plist` applies to all files including `mailsync.bin`. No separate helper entitlements plist is needed.

The existing entitlements contain:
```xml
com.apple.security.automation.apple-events  <!-- harmless for Rust binary -->
com.apple.security.cs.allow-jit             <!-- Electron V8 JIT; Rust binary simply ignores it -->
com.apple.security.device.print             <!-- harmless -->
com.apple.security.network.client           <!-- NEEDED by mailsync for IMAP/SMTP connections -->
com.apple.security.network.server           <!-- harmless -->
```

Unused entitlements are permission grants, not requirements. A Rust binary that does not use JIT simply ignores `allow-jit`. Apple notarization does NOT reject a binary for having extra entitlements it doesn't use.

**RESOLVED — provisioning profile:** The `build-macos.yaml` provisioning profile section is already COMMENTED OUT with this explicit note:
```yaml
# This is disabled because we need to codesign UnifyMail with this profile, but its
# presence makes Apple use it for the mailsync executable too. We need more nuanced
# provisioning profile configuration.
```
The project already encountered and resolved the "provisioning profile applies to all binaries" issue. The Rust binary will be signed with Developer ID + Hardened Runtime only (no provisioning profile), which is correct for a helper tool distribution via direct download.

**Windows signing (confirmed working):** The existing `build-windows.yaml` Azure Trusted Signing step uses:
```yaml
files-folder-filter: exe,dll,node
files-folder-recurse: true
```
The `exe` filter with `files-folder-recurse: true` will find `mailsync.exe` inside `app.asar.unpacked/` automatically. No changes needed.

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
| `build-linux.yaml` | C++ system deps in `apt-get install` | Remove C++ deps (cmake, libssl-dev, libsasl2-dev, etc.); keep Electron deps |
| `build-linux-arm64.yaml` | C++ system deps in `apt-get install` | Same cleanup as linux.yaml |
| `build/tasks/package-task.js` line 184 | `/^\/mailsync\/.*/` in `ignore` array | DELETE this line — `app/mailsync/` no longer exists |
| `app/mailsync.cmd` | `node ..\scripts\mock-mailsync.js %*` | DELETE this file — no longer relevant |
| `mailsync-process.ts` lines 108-113 | Windows dev fallback path points to C++ dir | UPDATE path to Rust build output |

**References that must NOT be changed:**

| Location | Reference | Reason |
|----------|-----------|--------|
| `build/tasks/package-task.js` asar.unpack | `'mailsync'`, `'mailsync.exe'`, `'mailsync.bin'` | These match the Rust binary names |
| `mailsync-process.ts` lines 103-107 | Primary binary path resolution | Zero changes needed |
| `mailsync-process.ts` lines 184-198 | Mock fallback spawn logic | Zero changes needed |
| `mailsync-bridge.ts` entire file | All IPC bridge logic | Zero changes needed |
| Lang JSON files (`app/lang/*.json`) | "Open Mailsync Logs" strings | These are UI strings, not build references |
| `app/keymaps/base-darwin.json` | `"window:open-mailsync-logs"` | UI keymap, not build reference |

### Pattern 8: Linux System Dependencies Cleanup

The Linux workflows (`build-linux.yaml` and `build-linux-arm64.yaml`) install many C++ build dependencies that are not needed after Rust replacement. These should be removed from the `apt-get install` step.

**Remove from apt-get install (C++ only — not needed for Rust or Electron):**
```
cmake                          # C++ cmake build system
libcurl4-openssl-dev           # C++ curl dependency
libssl-dev                     # C++ OpenSSL headers
libsasl2-dev                   # C++ SASL library
libsasl2-modules               # SASL plugin modules
libsasl2-modules-gssapi-mit    # SASL GSSAPI module
libc-ares-dev                  # C++ c-ares (async DNS)
libctemplate-dev               # C++ template library
libtidy-dev                    # C++ tidy-html5
libxml2-dev                    # C++ libxml2
libicu-dev                     # C++ International Components for Unicode
autoconf                       # autotools
automake                       # autotools
libtool                        # autotools
clang                          # C/C++ compiler
uuid-dev                       # C++ UUID library
```

**Keep in apt-get install (Electron/system — still required):**
```
build-essential                # gcc/make for native npm modules
fakeroot                       # needed for DEB package creation
rpm                            # needed for RPM package creation
git                            # needed for checkout
libsecret-1-dev                # needed by Electron for keyring access
libnss3                        # needed by Electron
libnss3-dev                    # needed by Electron
libxext-dev                    # needed by Electron
libxkbfile-dev                 # needed by Electron
libxtst-dev                    # needed by Electron
pkg-config                     # may be needed by native npm modules
xvfb                           # headless display for testing
software-properties-common     # needed for apt-add-repository
```

### Pattern 9: Linux Shell Script Wrapper — NOT Needed for Rust

The C++ Linux build (`app/mailsync/build.sh`) produced THREE artifacts:
1. `mailsync.bin` — the actual C++ binary
2. `mailsync` — a SHELL SCRIPT WRAPPER that set `SASL_PATH` and `LD_LIBRARY_PATH`
3. `libsasl2.so*` and SASL modules — copied shared libraries

The wrapper script:
```bash
#!/bin/bash
set -e; set -o pipefail
SCRIPTPATH="$( cd "$(dirname "$0")" >/dev/null 2>&1 ; pwd -P )"
SASL_PATH="$SCRIPTPATH" LD_LIBRARY_PATH="$SCRIPTPATH:$LD_LIBRARY_PATH" "$SCRIPTPATH/mailsync.bin" "$@"
```

**For the Rust binary, this wrapper is NOT NEEDED** because Rust statically links everything (rustls, rusqlite with bundled SQLite, pure-Rust SASL). There are no dynamic library path manipulations required. The Rust binary is just `mailsync.bin` — single file, no wrapper script, no `.so` files alongside it.

**Impact on packaging:** The `*.so` and `*.so.*` entries in the `asar.unpack` glob were needed for the C++ build's shared libraries. After Rust replacement, these globs will match nothing for mailsync but may still match other native modules. Leave them in place.

### Pattern 10: macOS Universal Binary — NOT Used for Rust

The C++ macOS build produced a **single universal binary** (fat binary, both arm64 and x86_64 in one file):
```bash
# From app/mailsync/build.sh:
xcodebuild ... ARCHS="arm64 x86_64" | xcpretty
```

The Rust build uses **separate per-architecture binaries** — one arm64 binary from the `macos-latest` runner, one x64 binary from the `macos-15-intel` runner. This is correct because the existing CI matrix already creates two separate macOS builds (one per architecture). No universal binary step is needed.

### Recommended Project Structure (Phase 10 scope)

```
.github/workflows/
├── build-windows.yaml          # Remove C++ build section; add Rust cargo build; remove *.dll copy
├── build-linux.yaml            # Remove C++ apt-get deps; add Rust cargo-zigbuild step
├── build-linux-arm64.yaml      # Remove C++ apt-get deps; add Rust cargo-zigbuild step (with .2.28 glibc)
└── build-macos.yaml            # Add Rust cargo build step (matrix: arm64, x64); no C++ changes

app/
├── mailsync-rust/              # Rust binary (built in Phases 5-9)
│   ├── Cargo.toml              # [profile.release] with LTO settings
│   ├── src/
│   └── target/release/        # Build output (gitignored)
├── mailsync/                   # DELETE ENTIRE DIRECTORY
├── mailsync.cmd                # DELETE THIS FILE
├── src/mailsync-process.ts     # UPDATE lines 108-113 (Windows dev fallback path only)
└── dist/resources/             # Populated during CI build
    ├── mailsync.exe            # Windows binary (CI copies here)
    └── mailsync.bin            # macOS/Linux binary (CI copies here)
```

### Anti-Patterns to Avoid

- **Do NOT use plain `cargo build` for linux-arm64**: The `ubuntu-24.04-arm` runner has glibc ~2.39; this produces a binary incompatible with Ubuntu 22.04 ARM64 (glibc 2.35). Always use `cargo zigbuild --target aarch64-unknown-linux-gnu.2.28`.
- **Do NOT use `cross` (Docker-based cross-compiler)**: Adds Docker daemon complexity to CI; cargo-zigbuild via Zig linker is lighter and faster.
- **Do NOT forget `chmod +x` on Linux/macOS**: The binary must be executable. `@electron/packager` preserves permissions, but only if set before packaging.
- **Do NOT delete `app/mailsync/` before the CI workflows are updated**: The Windows workflow currently references `app/mailsync/Windows/` — deleting the directory without first replacing the CI step will break the build immediately.
- **Do NOT remove the asar.unpack glob entries for mailsync**: They are already present and correctly target the Rust binary names.
- **Do NOT place the Rust binary inside `app/mailsync-rust/` in the packaged app**: It must be at the top level of `resources/` so `mailsync-process.ts` can find it via `path.join(resourcePath, binaryName)`.
- **Do NOT copy *.dll files for the Rust binary on Windows**: The Rust binary is statically linked. The old `Copy-Item "$outDir\*.dll"` step should be removed; only `mailsync.exe` needs to be copied.
- **Do NOT add a shell script wrapper for Linux**: The C++ build needed one for `SASL_PATH`/`LD_LIBRARY_PATH`. The Rust binary is fully self-contained.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Cross-compilation toolchain (Linux x64) | Custom Docker containers, QEMU | `cargo-zigbuild` 0.22.1 | Zig linker handles glibc ABI; no Docker daemon required; official tool |
| Cross-compilation toolchain (Linux arm64) | Plain `cargo build` on native runner | `cargo zigbuild` with `.2.28` glibc suffix | Native runner glibc ~2.39 is too new; zigbuild pins to 2.28 |
| glibc version pinning | Custom libc wrappers | `cargo zigbuild --target x86_64-unknown-linux-gnu.2.17` / `aarch64-unknown-linux-gnu.2.28` | Built-in cargo-zigbuild feature |
| Binary stripping | Shell `strip` invocations | `strip = "symbols"` in `[profile.release]` | Cargo handles ordering; runs after LTO |
| LTO pipeline | Incremental linking workarounds | `lto = true` in `[profile.release]` | Cargo fat LTO is the standard approach |
| macOS signing of standalone binary | Custom codesign scripts | `@electron/packager` `osxSign` callback | Already configured; packager signs all bundle contents including unpacked files |
| Windows signing of standalone binary | Custom signtool scripts | Azure Trusted Signing action (existing) | Already in workflow with `files-folder-recurse: true` and `exe` filter |
| asar unpack configuration | Custom electron-packager plugins | Existing glob patterns in `package-task.js` | Already configured; zero changes needed |
| Binary path resolution | Custom path logic in TypeScript | Existing `.replace('app.asar', 'app.asar.unpacked')` in `mailsync-process.ts` | Production logic already implemented; zero changes needed |
| OpenSSL detection | Manual `nm` symbol checks | `cargo tree -e features \| grep -i openssl` | Catches the dependency at the crate graph level, before it links |
| Linux SASL shared library path setup | Shell wrapper script | Pure Rust SASL implementation (built-in to async-imap) | Rust handles SASL authentication internally; no libsasl2 needed |
| macOS helper entitlements plist | Separate `entitlements-helper.plist` | Existing `entitlements.plist` (all files) | Unused entitlements are permission grants; Rust binary ignores allow-jit |

**Key insight:** The existing project infrastructure was already designed for a standalone binary named `mailsync.exe`/`mailsync.bin`. The path resolution, asar unpacking, Windows signing, and macOS signing all work without modification. Phase 10's primary work is in CI (adding/replacing Rust build steps), one-line fix in mailsync-process.ts (Windows dev path), system dep cleanup on Linux, and C++ deletion.

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

### Pitfall 9: linux-arm64 glibc Too New (Binary Incompatible with Ubuntu 22.04 ARM)
**What goes wrong:** The arm64 binary built on `ubuntu-24.04-arm` (glibc ~2.39) fails to start on Ubuntu 22.04 ARM64 (glibc 2.35) with `GLIBC_2.36 not found` or similar.
**Why it happens:** Using plain `cargo build` on the native runner links against the runner's glibc ~2.39. glibc is forward-compatible only — a binary compiled for 2.39 will not run on 2.35.
**How to avoid:** Use `cargo zigbuild --target aarch64-unknown-linux-gnu.2.28` on the ARM64 runner. The `.2.28` suffix pins to Debian Buster baseline (2018), compatible with Ubuntu 22.04 (glibc 2.35). The CI Docker test matrix (`ubuntu:22.04`) will catch this if zigbuild is omitted.
**Warning signs:** Docker integration test step fails on `ubuntu:22.04` with `GLIBC_*` not found; binary works on `ubuntu:24.04` but fails on `22.04`.

### Pitfall 10: Old Windows *.dll Copy Step Left In (Dead Code)
**What goes wrong:** The old `Copy-Item "$outDir\*.dll"` step is left in the Windows CI workflow after switching to Rust. This is harmless (copies nothing if no DLLs exist) but represents dead code that may confuse future maintainers.
**Why it happens:** Incomplete cleanup of the old C++ Windows build section.
**How to avoid:** When replacing the C++ build section in `build-windows.yaml`, remove the `Copy-Item "$outDir\*.dll"` line entirely. The Rust binary is statically linked and requires no DLLs.

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

### Windows Binary Copy (PowerShell — Rust version, no DLLs)
```powershell
# Source: adapted from existing build-windows.yaml pattern
# NOTE: No *.dll copy — Rust binary is statically linked
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

# Build with glibc 2.17 minimum (RHEL 7 era):
cd app/mailsync-rust
cargo zigbuild --release --target x86_64-unknown-linux-gnu.2.17

# Output at:
ls -lh target/x86_64-unknown-linux-gnu/release/mailsync
```

### cargo-zigbuild Linux arm64 Build (glibc pinned to 2.28)
```bash
# Source: cargo-zigbuild v0.22.1 README; resolved open question 1
# Install (same as x64):
pip3 install ziglang
cargo install cargo-zigbuild

# Build with glibc 2.28 minimum (Debian Buster baseline; compatible with Ubuntu 22.04 glibc 2.35):
cd app/mailsync-rust
cargo zigbuild --release --target aarch64-unknown-linux-gnu.2.28
# NOTE: NOT plain cargo build — ubuntu-24.04-arm runner has glibc ~2.39
#       which would produce a binary INCOMPATIBLE with Ubuntu 22.04 ARM64

# Output at:
ls -lh target/aarch64-unknown-linux-gnu/release/mailsync
```

### Verify glibc requirement of built binary (Linux)
```bash
# Use objdump to verify the minimum glibc version required by the binary
# Should show GLIBC_2.17 or lower for x64, GLIBC_2.28 or lower for arm64
objdump -p app/dist/resources/mailsync.bin | grep GLIBC
# Or with readelf:
readelf -d app/dist/resources/mailsync.bin | grep NEEDED
readelf --dyn-syms app/dist/resources/mailsync.bin | grep GLIBC
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

### mailsync-process.ts Dev Fallback Path Change
```typescript
// Source: mailsync-process.ts lines 108-113 — THE ONE CHANGE NEEDED

// BEFORE (C++ directory — will not exist after deletion):
if (!fs.existsSync(this.binaryPath)) {
  const devBuildPath = path.join(resourcePath, 'mailsync', 'Windows', 'x64', 'Release', binaryName);
  if (fs.existsSync(devBuildPath)) {
    this.binaryPath = devBuildPath;
  }
}

// AFTER (Rust build output directory):
if (!fs.existsSync(this.binaryPath)) {
  const devBuildPath = path.join(resourcePath, '..', 'mailsync-rust', 'target', 'release', binaryName);
  if (fs.existsSync(devBuildPath)) {
    this.binaryPath = devBuildPath;
  }
}
// If devBuildPath also doesn't exist, the mock fallback at lines 184-198 takes over
// (mock-mailsync.js) — always available for development without building the binary
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
| macOS universal binary (arm64 + x64 fat binary) | Separate per-architecture binaries via CI matrix | Phase 10 | Per-arch CI matrix already exists; separate binaries are simpler than lipo |
| Linux shell script wrapper (sets SASL_PATH, LD_LIBRARY_PATH) | No wrapper; single statically-linked Rust binary | Phase 10 | Rust has no dynamic library dependencies; mailsync.bin is self-contained |
| Windows DLL copy (OpenSSL, curl, zlib DLLs) | No DLLs; statically-linked Rust binary | Phase 10 | Eliminates DLL distribution complexity; binary is fully self-contained |

**Deprecated/outdated:**
- `vcpkg install` workflow step: DELETE — C++ build system gone
- `msbuild app\mailcore\...` workflow step: DELETE — mailcore2 gone
- `msbuild app\mailsync\...` workflow step: DELETE — C++ mailsync gone
- `app/mailsync.cmd` dev helper: DELETE — no longer needed
- `node-gyp` and `node-addon-api` in package.json: DELETE (Phase 4 handles this)
- `Copy-Item "$outDir\*.dll"` in build-windows.yaml: DELETE — Rust binary is statically linked
- Linux `mailsync` shell script wrapper: NOT CREATED — Rust binary needs no wrapper

---

## Open Questions

All three prior open questions have been resolved. No open questions remain.

### RESOLVED: Question 1 — Linux arm64 glibc minimum version
**Answer:** cargo-zigbuild IS needed for arm64 too. Use `cargo zigbuild --target aarch64-unknown-linux-gnu.2.28`.

Evidence: The `ubuntu-24.04-arm` runner has glibc ~2.39. The Docker test matrix in `build-linux-arm64.yaml` tests against Ubuntu 22.04 (glibc 2.35). A binary linked against glibc 2.39 will FAIL on Ubuntu 22.04. The `.2.28` suffix (Debian Buster baseline) is compatible with glibc 2.35.

**Standard Stack table updated:** linux-arm64 now uses `cargo zigbuild` with `.2.28` suffix, not plain `cargo build`.

### RESOLVED: Question 2 — macOS entitlements plist
**Answer:** No separate entitlements plist needed. The existing `build/resources/mac/entitlements.plist` is correct for the Rust binary.

Evidence: Unused entitlements are permission grants, not requirements. The `allow-jit` entitlement is ignored by a binary that does not use JIT. The provisioning profile issue was already resolved — the profile section in `build-macos.yaml` is commented out. The Rust binary will be signed with Developer ID + Hardened Runtime (no provisioning profile), which is correct.

### RESOLVED: Question 3 — Dev mode binary path
**Answer:** Update the Windows dev fallback path in `mailsync-process.ts` lines 108-113 to point to the Rust build output. The mock fallback continues to work for all developers.

Evidence: Lines 108-113 currently point to `app/mailsync/Windows/x64/Release/mailsync.exe` (C++ output directory that will be deleted). Updating to `app/mailsync-rust/target/release/mailsync` lets developers who build the Rust binary locally run it without packaging. The mock fallback at lines 184-198 remains untouched and continues to work when neither the production binary nor the dev binary exists.

---

## Sources

### Primary (HIGH confidence)
- `app/frontend/mailsync-process.ts` — Binary path resolution logic (lines 103-113); spawning logic (lines 185-198); IPC protocol; dev fallback path analysis
- `build/tasks/package-task.js` — `asar.unpack` glob patterns; `osxSign` configuration; `ignore` patterns; `optionsForFile` callback for entitlements
- `build/resources/mac/entitlements.plist` — Exact entitlements content; confirmed network.client present; allow-jit present but harmless for Rust
- `.github/workflows/build-windows.yaml` — Existing C++ build section structure; Azure signing workflow; `files-folder-filter: exe,dll,node` confirmed; `files-folder-recurse: true` confirmed; `*.dll` copy step location
- `.github/workflows/build-linux.yaml` — Confirmed NO C++ build steps (C++ built separately via Travis CI/S3); apt-get install package list for cleanup audit
- `.github/workflows/build-linux-arm64.yaml` — Confirmed `ubuntu-24.04-arm` runner; Docker test matrix `ubuntu_version: ['22.04', '24.04', '25.04']`; confirmed NO C++ build steps; apt-get install package list
- `.github/workflows/build-macos.yaml` — Matrix build pattern (arm64/x64); provisioning profile section confirmed COMMENTED OUT with explanatory note; signing workflow
- `app/mailsync/build.sh` — Linux shell script wrapper confirmed; macOS universal binary with `ARCHS="arm64 x86_64"` confirmed; C++ build artifact inventory
- Phase 4 RESEARCH.md — CI patterns for Rust builds, cargo-zigbuild for Linux, napi-rs asar unpack patterns; binary size profile settings
- Phase 5 RESEARCH.md — Binary structure and binary directory location (`app/mailsync-rust/`)

### Secondary (MEDIUM confidence)
- cargo-zigbuild v0.22.1 README (github.com/rust-cross/cargo-zigbuild) — Supported platforms and glibc pinning syntax; `.2.28` and `.2.17` suffix usage
- min-sized-rust (github.com/johnthagen/min-sized-rust) — Cargo.toml profile settings for binary size
- Electron ASAR Archives documentation (electronjs.org) — spawn/child_process limitations; app.asar.unpacked behavior

### Tertiary (LOW confidence — extrapolation)
- deltachat-rpc-server wheel size (~11.7MB, manylinux, 2025) — Used as analog for achievable Rust email binary size; similar stack (async email + TLS + SQLite)
- glibc compatibility table — glibc 2.28 is Debian Buster (2018); glibc 2.17 is RHEL 7 (2013); glibc 2.35 is Ubuntu 22.04; glibc 2.39 is Ubuntu 24.04 — cross-referenced from multiple sources

---

## Metadata

**Confidence breakdown:**
- Standard stack (CI workflow structure): HIGH — extracted directly from existing workflow files; all three open questions resolved
- Architecture (path resolution, asar unpack): HIGH — extracted directly from mailsync-process.ts and package-task.js; one-line change identified precisely
- linux-arm64 glibc version: HIGH (was LOW) — resolved by reading actual workflow file and Docker test matrix; cargo-zigbuild with .2.28 required
- macOS entitlements: HIGH (was MEDIUM) — resolved by reading actual entitlements.plist and provisioning profile comments in workflow
- Dev fallback path change: HIGH (was MEDIUM) — resolved by reading exact lines 103-198 of mailsync-process.ts
- Binary size target (15MB achievable): MEDIUM — based on deltachat analog and Cargo profile guidance; actual size depends on final dependency set after Phase 9
- Linux apt-get cleanup: HIGH — packages audited from actual workflow files against Electron/Rust requirements

**Research date:** 2026-03-02 (initial), 2026-03-03 (deep-dive update — all open questions resolved)
**Valid until:** 2026-06-01 (stable ecosystem; cargo-zigbuild and packager APIs are stable; GitHub Actions runner updates are the main risk)
