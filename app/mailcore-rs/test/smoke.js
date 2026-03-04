'use strict';

// Shared smoke test for mailcore-napi exports.
// Run from the app/ working directory: node mailcore-rs/test/smoke.js
// The require('mailcore-napi') resolves via npm symlink at app/node_modules/mailcore-napi -> app/mailcore-rs

const m = require('mailcore-napi');

let failed = false;

// providerForEmail: functional check — call with a real email, verify an object is returned
const result = m.providerForEmail('test@gmail.com');
if (result !== null && typeof result === 'object') {
  console.log('PASS: providerForEmail returned result:', result.displayName || JSON.stringify(result));
} else {
  console.error('FAIL: providerForEmail did not return an object');
  failed = true;
}

// registerProviders: type-check only
if (typeof m.registerProviders !== 'function') {
  console.error('FAIL: registerProviders is not a function');
  failed = true;
} else {
  console.log('PASS: registerProviders is function');
}

// testIMAPConnection: type-check only
if (typeof m.testIMAPConnection !== 'function') {
  console.error('FAIL: testIMAPConnection is not a function');
  failed = true;
} else {
  console.log('PASS: testIMAPConnection is function');
}

// testSMTPConnection: type-check only
if (typeof m.testSMTPConnection !== 'function') {
  console.error('FAIL: testSMTPConnection is not a function');
  failed = true;
} else {
  console.log('PASS: testSMTPConnection is function');
}

// validateAccount: type-check only
if (typeof m.validateAccount !== 'function') {
  console.error('FAIL: validateAccount is not a function');
  failed = true;
} else {
  console.log('PASS: validateAccount is function');
}

if (failed) {
  process.exit(1);
}

console.log('All smoke tests passed.');
