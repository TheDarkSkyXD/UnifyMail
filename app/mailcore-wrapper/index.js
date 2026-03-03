// Wrapper module: routes each function to the correct addon
// Provider functions (Phase 1+): Rust addon (app/mailcore-rs/)
// testIMAPConnection (Phase 2): Rust addon (app/mailcore-rs/)
// testSMTPConnection, validateAccount (Phase 3): C++ addon (app/mailcore/) until replaced
'use strict';

let rustAddon = null;
let cppAddon = null;

function getRust() {
  if (!rustAddon) {
    rustAddon = require('../mailcore-rs/loader.js');
  }
  return rustAddon;
}

function getCpp() {
  if (!cppAddon) {
    cppAddon = require('../mailcore/build/Release/mailcore_napi.node');
  }
  return cppAddon;
}

// Phase 1: Provider functions routed to Rust
exports.providerForEmail = function providerForEmail(email) {
  return getRust().providerForEmail(email);
};
exports.registerProviders = function registerProviders(jsonPath) {
  return getRust().registerProviders(jsonPath);
};

// Phase 2: testIMAPConnection now routed to Rust
exports.testIMAPConnection = function testIMAPConnection(opts) {
  if (process.env.MAILCORE_DEBUG === '1') {
    console.log('testIMAPConnection -> Rust');
  }
  return getRust().testIMAPConnection(opts);
};

// Phases 3+: SMTP and account validation still routed to C++ until replaced
exports.validateAccount = function validateAccount(opts) {
  return getCpp().validateAccount(opts);
};
exports.testSMTPConnection = function testSMTPConnection(opts) {
  return getCpp().testSMTPConnection(opts);
};
