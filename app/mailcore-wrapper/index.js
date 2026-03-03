// Wrapper module: routes each function to the correct addon
// Provider functions (Phase 1+): Rust addon (app/mailcore-rs/)
// Network functions (Phase 2-3): C++ addon (app/mailcore/) until replaced
'use strict';

let rustAddon = null;
let cppAddon = null;

function getRust() {
  if (!rustAddon) {
    rustAddon = require('../mailcore-rs/index.js');
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

// Phases 2-3: Network functions still routed to C++ until replaced
exports.validateAccount = function validateAccount(opts) {
  return getCpp().validateAccount(opts);
};
exports.testIMAPConnection = function testIMAPConnection(opts) {
  return getCpp().testIMAPConnection(opts);
};
exports.testSMTPConnection = function testSMTPConnection(opts) {
  return getCpp().testSMTPConnection(opts);
};
