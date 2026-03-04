// Stable loader for the Rust napi-rs addon.
// The napi-generated index.js uses ABI detection that misidentifies
// standard Windows Node.js as needing MSVC when we build with GNU target.
// This loader bypasses that detection and loads the correct binary directly.

import path = require('path');

type NapiModule = typeof import('./index');

let nativeBinding: NapiModule | null = null;

// Determine the correct .node binary for this platform
const platform = process.platform;
const arch = process.arch;

const BINARY_MAP: Record<string, string> = {
  'win32-x64': 'mailcore-napi-rs.win32-x64-gnu.node',
  'darwin-x64': 'mailcore-napi-rs.darwin-x64.node',
  'darwin-arm64': 'mailcore-napi-rs.darwin-arm64.node',
  'linux-x64': 'mailcore-napi-rs.linux-x64-gnu.node',
  'linux-arm64': 'mailcore-napi-rs.linux-arm64-gnu.node',
};

const key = `${platform}-${arch}`;
const binaryName = BINARY_MAP[key];

if (!binaryName) {
  throw new Error(`Unsupported platform: ${platform}-${arch}`);
}

const binaryPath = path.join(__dirname, binaryName);

try {
  nativeBinding = require(binaryPath) as NapiModule;
} catch (err) {
  throw new Error(
    `Failed to load native binding at ${binaryPath}: ${(err as Error).message}`,
    { cause: err }
  );
}

module.exports = nativeBinding;
module.exports.providerForEmail = nativeBinding!.providerForEmail;
module.exports.registerProviders = nativeBinding!.registerProviders;
module.exports.testIMAPConnection = nativeBinding!.testIMAPConnection;
module.exports.testSMTPConnection = nativeBinding!.testSMTPConnection;
module.exports.validateAccount = nativeBinding!.validateAccount;
