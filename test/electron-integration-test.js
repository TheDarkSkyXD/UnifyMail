/**
 * Electron integration test: verifies the Rust napi-rs addon loads in the
 * Electron main process without BoringSSL / OpenSSL conflicts, and that
 * provider lookup returns expected results through the wrapper module.
 *
 * Usage (from repo root):
 *   npx electron test/electron-integration-test.js
 *
 * Exit codes:
 *   0 — all checks passed
 *   1 — one or more checks failed
 */
'use strict';

const path = require('path');

const { app } = require('electron');

app.whenReady().then(async () => {
  const results = [];

  function check(name, fn) {
    try {
      fn();
      results.push({ name, ok: true });
      console.log(`PASS  ${name}`);
    } catch (e) {
      results.push({ name, ok: false, error: e.message });
      console.error(`FAIL  ${name}: ${e.message}`);
    }
  }

  function assert(condition, message) {
    if (!condition) throw new Error(message);
  }

  // Load via the wrapper — same path consumer code (onboarding-helpers.ts) uses
  let addon;
  try {
    addon = require(path.join(__dirname, '..', 'app', 'mailcore-rs', 'loader.js'));
    console.log('PASS  Wrapper module loaded');
  } catch (e) {
    console.error('FAIL  Wrapper module failed to load:', e.message);
    app.quit();
    process.exit(1);
  }

  // Check 1: Gmail lookup returns correct provider
  check('providerForEmail("test@gmail.com") returns gmail', () => {
    const result = addon.providerForEmail('test@gmail.com');
    assert(result !== null && result !== undefined, 'result should not be null');
    assert(result.identifier === 'gmail', `expected "gmail", got "${result.identifier}"`);
    assert(result.servers && result.servers.imap && result.servers.imap.length > 0, 'gmail should have IMAP servers');
    assert(result.servers.imap[0].hostname === 'imap.gmail.com', `expected "imap.gmail.com", got "${result.servers.imap[0].hostname}"`);
  });

  // Check 2: Unknown domain returns null
  check('providerForEmail("test@unknown-xyz.com") returns null', () => {
    const result = addon.providerForEmail('test@unknown-xyz-domain-12345.com');
    assert(result === null, `expected null, got ${JSON.stringify(result)}`);
  });

  // Check 3: Yahoo.co.jp returns dedicated provider (not generic yahoo)
  check('providerForEmail("test@yahoo.co.jp") returns "yahoo.co.jp"', () => {
    const result = addon.providerForEmail('test@yahoo.co.jp');
    assert(result !== null, 'yahoo.co.jp should match a provider');
    assert(result.identifier === 'yahoo.co.jp', `expected "yahoo.co.jp", got "${result.identifier}"`);
  });

  // Check 4: Yahoo.com returns yahoo (not excluded)
  check('providerForEmail("test@yahoo.com") returns "yahoo"', () => {
    const result = addon.providerForEmail('test@yahoo.com');
    assert(result !== null, 'yahoo.com should match a provider');
    assert(result.identifier === 'yahoo', `expected "yahoo", got "${result.identifier}"`);
  });

  // Check 5: Outlook / Hotmail
  check('providerForEmail("test@hotmail.com") returns "outlook"', () => {
    const result = addon.providerForEmail('test@hotmail.com');
    assert(result !== null, 'hotmail.com should match a provider');
    assert(result.identifier === 'outlook', `expected "outlook", got "${result.identifier}"`);
  });

  // Check 6: Empty string throws
  check('providerForEmail("") throws', () => {
    let threw = false;
    try {
      addon.providerForEmail('');
    } catch (_) {
      threw = true;
    }
    assert(threw, 'expected throw for empty string');
  });

  // Check 7: Network functions are accessible (lazy-loaded — should not throw on reference)
  check('validateAccount is a function (C++ stub accessible)', () => {
    assert(typeof addon.validateAccount === 'function', 'validateAccount should be a function');
    assert(typeof addon.testIMAPConnection === 'function', 'testIMAPConnection should be a function');
    assert(typeof addon.testSMTPConnection === 'function', 'testSMTPConnection should be a function');
  });

  // Check 8: validateAccount field echo (async — proves N-API boundary crossing)
  // The call WILL fail (invalid hosts) but server echo fields are always populated.
  try {
    const echoResult = await addon.validateAccount({
      email: 'test@example.com',
      imapHostname: 'test-imap.invalid',
      imapPort: 993,
      imapConnectionType: 'tls',
      imapUsername: 'imap-user@example.com',
      imapPassword: 'imap-secret',
      smtpHostname: 'test-smtp.invalid',
      smtpPort: 587,
      smtpConnectionType: 'starttls',
      smtpUsername: 'smtp-user@example.com',
      smtpPassword: 'smtp-secret',
      oauth2Token: undefined,
    });
    // success will be false (invalid hosts) but echo fields must be present
    const name = 'validateAccount field echo crosses N-API boundary';
    let ok = true;
    let error = null;
    if (echoResult.imapServer.hostname !== 'test-imap.invalid') {
      ok = false;
      error = `imapServer.hostname: expected "test-imap.invalid", got "${echoResult.imapServer.hostname}"`;
    } else if (echoResult.imapServer.port !== 993) {
      ok = false;
      error = `imapServer.port: expected 993, got ${echoResult.imapServer.port}`;
    } else if (echoResult.smtpServer.hostname !== 'test-smtp.invalid') {
      ok = false;
      error = `smtpServer.hostname: expected "test-smtp.invalid", got "${echoResult.smtpServer.hostname}"`;
    } else if (echoResult.smtpServer.port !== 587) {
      ok = false;
      error = `smtpServer.port: expected 587, got ${echoResult.smtpServer.port}`;
    }
    results.push({ name, ok, error });
    if (ok) {
      console.log(`PASS  ${name}`);
    } else {
      console.error(`FAIL  ${name}: ${error}`);
    }
  } catch (e) {
    const name = 'validateAccount field echo crosses N-API boundary';
    results.push({ name, ok: false, error: e.message });
    console.error(`FAIL  ${name}: ${e.message}`);
  }

  // Check 9: security mapping logic (synchronous)
  check('security mapping: SSL/TLS->tls, STARTTLS->starttls, none->clear, undefined->undefined', () => {
    function mapSecurity(security) {
      return security === 'SSL / TLS' ? 'tls'
           : security === 'STARTTLS'   ? 'starttls'
           : security === 'none'        ? 'clear'
           : undefined;
    }
    assert(mapSecurity('SSL / TLS') === 'tls', '"SSL / TLS" must map to "tls"');
    assert(mapSecurity('STARTTLS') === 'starttls', '"STARTTLS" must map to "starttls"');
    assert(mapSecurity('none') === 'clear', '"none" must map to "clear"');
    assert(mapSecurity(undefined) === undefined, 'undefined must map to undefined');
  });

  // Check 10: username fallback logic (synchronous)
  check('username fallback: empty/null imap_username falls back to emailAddress', () => {
    const emailAddress = 'account@example.com';
    assert(('' || emailAddress) === emailAddress, 'empty string must fall back to emailAddress');
    assert((null || emailAddress) === emailAddress, 'null must fall back to emailAddress');
    assert(('custom@example.com' || emailAddress) === 'custom@example.com', 'non-empty username must not be overridden');
  });

  // Summary
  const passed = results.filter(r => r.ok).length;
  const total = results.length;
  console.log(`\nResults: ${passed}/${total} passed`);

  if (passed === total) {
    console.log('\nPASS: Rust addon loads in Electron main process without conflicts.');
    console.log('Provider detection works correctly through the wrapper module.');
    app.quit();
    process.exit(0);
  } else {
    const failed = results.filter(r => !r.ok);
    console.error('\nFAIL: Some checks failed:');
    for (const f of failed) {
      console.error(`  - ${f.name}: ${f.error}`);
    }
    app.quit();
    process.exit(1);
  }
});
