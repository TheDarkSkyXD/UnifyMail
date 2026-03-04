// Wrapper module: all functions routed to Rust addon (app/mailcore-rs/)
// Phase 1: providerForEmail, registerProviders
// Phase 2: testIMAPConnection
// Phase 3: testSMTPConnection, validateAccount
'use strict';

let rustAddon = null;

function getRust() {
  if (!rustAddon) {
    rustAddon = require('../mailcore-rs/loader.js');
  }
  return rustAddon;
}

// Phase 1: Provider functions routed to Rust
exports.providerForEmail = function providerForEmail(email) {
  return getRust().providerForEmail(email);
};
exports.registerProviders = function registerProviders(jsonPath) {
  return getRust().registerProviders(jsonPath);
};

// Phase 2: IMAP connection testing routed to Rust
exports.testIMAPConnection = function testIMAPConnection(opts) {
  if (process.env.MAILCORE_DEBUG === '1') {
    console.log('testIMAPConnection -> Rust');
  }
  return getRust().testIMAPConnection(opts);
};

// Phase 3: SMTP connection testing routed to Rust
exports.testSMTPConnection = function testSMTPConnection(opts) {
  if (process.env.MAILCORE_DEBUG === '1') {
    console.log('testSMTPConnection -> Rust');
  }
  return getRust().testSMTPConnection(opts);
};

// Phase 3: Account validation routed to Rust
exports.validateAccount = function validateAccount(opts) {
  if (process.env.MAILCORE_DEBUG === '1') {
    console.log('validateAccount -> Rust');
  }
  return getRust().validateAccount(opts);
};
