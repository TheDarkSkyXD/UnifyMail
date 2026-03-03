// mailcore-rs — platform-aware binary loader
// Hand-written to bypass napi-rs ABI detection limitations on Windows:
// The auto-generated loader uses process.config.variables.shlib_suffix to
// distinguish GNU vs MSVC Node.js, but standard Windows Node.js distributions
// are MSVC-built and always resolve to the MSVC binary path — even when the
// GNU binary is present and compatible. We load by explicit platform path.
'use strict';

const path = require('path');
const os = require('os');

function loadBinding() {
  const platform = os.platform();
  const arch = os.arch();

  if (platform === 'win32' && arch === 'x64') {
    // GNU .node built with x86_64-pc-windows-gnu target.
    // Loads in both GNU and MSVC Node.js processes because napi-rs uses the
    // stable N-API ABI, not the C++ ABI. The GNU runtime (libgcc / libwinpthread)
    // is statically linked into the .node, so no external MinGW DLLs are needed.
    return require('./mailcore-napi-rs.win32-x64-gnu.node');
  } else if (platform === 'darwin') {
    try {
      return require('./mailcore-napi-rs.darwin-universal.node');
    } catch (_) {
      if (arch === 'arm64') {
        return require('./mailcore-napi-rs.darwin-arm64.node');
      }
      return require('./mailcore-napi-rs.darwin-x64.node');
    }
  } else if (platform === 'linux') {
    if (arch === 'x64') {
      return require('./mailcore-napi-rs.linux-x64-gnu.node');
    } else if (arch === 'arm64') {
      return require('./mailcore-napi-rs.linux-arm64-gnu.node');
    }
  }
  throw new Error(
    `mailcore-rs: unsupported platform ${platform}/${arch}. ` +
    'Build the Rust addon for this target and update app/mailcore-rs/index.js.'
  );
}

module.exports = loadBinding();
