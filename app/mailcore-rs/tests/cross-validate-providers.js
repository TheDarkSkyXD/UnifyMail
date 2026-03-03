/**
 * Cross-validation test: compares Rust addon provider results against the
 * expected provider database from providers.json.
 *
 * When the C++ addon is available (built), also compares against C++ results
 * to ensure Rust parity. The test exits 0 on full pass, 1 on any failure.
 *
 * Usage:
 *   node app/mailcore-rs/tests/cross-validate-providers.js
 *   node tests/cross-validate-providers.js  (from app/mailcore-rs/)
 *
 * For C++ comparison (requires built C++ addon):
 *   CPP_ADDON=1 node app/mailcore-rs/tests/cross-validate-providers.js
 */
'use strict';

const path = require('path');
const fs = require('fs');

// ---------------------------------------------------------------------------
// Setup: load Rust addon
// ---------------------------------------------------------------------------

const mailcoreRsDir = path.resolve(__dirname, '..');
const rustAddonPath = path.join(mailcoreRsDir, 'index.js');

let rustAddon;
try {
  rustAddon = require(rustAddonPath);
} catch (e) {
  console.error('FATAL: Cannot load Rust addon from', rustAddonPath);
  console.error(e.message);
  process.exit(1);
}

// ---------------------------------------------------------------------------
// Setup: optionally load C++ addon for cross-comparison
// ---------------------------------------------------------------------------

let cppAddon = null;
const cppAddonPath = path.resolve(mailcoreRsDir, '..', 'mailcore', 'build', 'Release', 'mailcore_napi.node');

if (process.env.CPP_ADDON === '1') {
  try {
    cppAddon = require(cppAddonPath);
    console.log('C++ addon loaded for cross-validation.');
  } catch (e) {
    console.warn('WARNING: CPP_ADDON=1 set but C++ addon failed to load:', e.message);
    console.warn('Continuing with Rust-only validation.');
  }
} else if (fs.existsSync(cppAddonPath)) {
  try {
    cppAddon = require(cppAddonPath);
    console.log('C++ addon found and loaded for cross-validation.');
  } catch (e) {
    console.warn('C++ addon found but failed to load (skipping cross-comparison):', e.message);
  }
}

if (!cppAddon) {
  console.log('Running Rust-only validation (no C++ addon available).');
  console.log('To enable C++ cross-validation, build the C++ addon first.');
  console.log('');
}

// ---------------------------------------------------------------------------
// Build test cases from providers.json
// ---------------------------------------------------------------------------

const providersJsonPath = path.join(mailcoreRsDir, 'resources', 'providers.json');
const providers = JSON.parse(fs.readFileSync(providersJsonPath, 'utf8'));

/**
 * Convert a regex pattern from providers.json to a concrete test domain.
 * Strips anchors, unescapes basic patterns, handles wildcards.
 * Returns null if pattern is too complex to extract a domain from.
 */
function patternToDomain(pattern) {
  // Remove trailing .* wildcard variants — use a literal suffix instead
  let domain = pattern
    .replace(/\\\./g, '.')    // unescape dots
    .replace(/\.\*/g, '.com') // replace wildcard with .com
    .replace(/\\d\+/g, '1')   // replace \d+ with 1
    .replace(/\\d/g, '1')     // replace \d with 1
    .replace(/\^/g, '')       // strip anchors
    .replace(/\$/g, '')
    .trim();

  // If it contains remaining regex metacharacters, skip
  if (/[+?|()\[\]{}^$\\]/.test(domain)) {
    return null;
  }

  return domain;
}

// Generate test cases: one per provider that has domain-match entries
const testCases = [];
const nonMatchingDomains = [
  'unknown-xyz-domain-12345.com',
  'notaprovider.example',
  'test.invalid',
  'random-email-host.net',
  'no-provider-here.org',
];

for (const [identifier, provider] of Object.entries(providers)) {
  const domainMatches = provider['domain-match'] || [];

  if (domainMatches.length === 0) {
    // Provider only uses MX matching — still include in expected-null tests
    // (domain-match returns null for MX-only providers in Phase 1)
    continue;
  }

  // Use the first extractable domain pattern
  for (const pattern of domainMatches) {
    const domain = patternToDomain(pattern);
    if (domain) {
      testCases.push({
        email: `test@${domain}`,
        expectedIdentifier: identifier,
        description: `${identifier} via domain-match "${pattern}"`,
      });
      break; // one test case per provider is sufficient
    }
  }
}

// Add extra test cases for key providers with multiple domains
testCases.push(
  // Gmail aliases
  { email: 'user@googlemail.com', expectedIdentifier: 'gmail', description: 'gmail via googlemail.com' },
  // Outlook variants
  { email: 'user@hotmail.com', expectedIdentifier: 'outlook', description: 'outlook via hotmail.com' },
  { email: 'user@live.com', expectedIdentifier: 'outlook', description: 'outlook via live.com' },
  // Yahoo — domain-exclude test
  { email: 'user@yahoo.com', expectedIdentifier: 'yahoo', description: 'yahoo.com -> yahoo (not excluded)' },
  { email: 'user@yahoo.co.jp', expectedIdentifier: 'yahoo.co.jp', description: 'yahoo.co.jp -> dedicated provider (excluded from yahoo)' },
  // AOL wildcards
  { email: 'user@aol.com', expectedIdentifier: 'aol', description: 'aol.com -> aol' },
  { email: 'user@aol.de', expectedIdentifier: 'aol', description: 'aol.de -> aol (wildcard aol.*)' },
  // GMX wildcard
  { email: 'user@gmx.de', expectedIdentifier: 'gmx', description: 'gmx.de -> gmx (wildcard gmx.*)' },
  // FastMail variant domains
  { email: 'user@fastmail.fm', expectedIdentifier: 'fastmail', description: 'fastmail.fm -> fastmail (wildcard)' },
  // Apple variants
  { email: 'user@me.com', expectedIdentifier: 'mobileme', description: 'me.com -> mobileme' },
  { email: 'user@icloud.com', expectedIdentifier: 'mobileme', description: 'icloud.com -> mobileme' },
  // Non-matching domains
  ...nonMatchingDomains.map(domain => ({
    email: `test@${domain}`,
    expectedIdentifier: null,
    description: `non-matching: ${domain}`,
  }))
);

// ---------------------------------------------------------------------------
// Run validation
// ---------------------------------------------------------------------------

let passed = 0;
let failed = 0;
const failures = [];

console.log(`Running ${testCases.length} test cases...\n`);

for (const tc of testCases) {
  // Test Rust addon
  let rustResult = null;
  let rustError = null;
  try {
    rustResult = rustAddon.providerForEmail(tc.email);
  } catch (e) {
    rustError = e.message;
  }

  const rustIdentifier = rustResult ? rustResult.identifier : null;
  const rustOk = rustError === null && rustIdentifier === tc.expectedIdentifier;

  // Test C++ addon (if available)
  let cppOk = true;
  let cppIdentifier = null;
  let cppError = null;
  if (cppAddon) {
    try {
      const cppResult = cppAddon.providerForEmail(tc.email);
      cppIdentifier = cppResult ? cppResult.identifier : null;
      cppOk = cppIdentifier === rustIdentifier;
    } catch (e) {
      cppError = e.message;
      cppOk = false;
    }
  }

  const allOk = rustOk && cppOk;

  if (allOk) {
    passed++;
    const cppNote = cppAddon ? ' [C++ match: yes]' : '';
    console.log(`PASS  ${tc.description} => ${rustIdentifier ?? 'null'}${cppNote}`);
  } else {
    failed++;
    const failReasons = [];

    if (!rustOk) {
      if (rustError) {
        failReasons.push(`Rust threw: ${rustError}`);
      } else {
        failReasons.push(`Rust returned "${rustIdentifier}" (expected "${tc.expectedIdentifier}")`);
      }
    }
    if (!cppOk && cppAddon) {
      if (cppError) {
        failReasons.push(`C++ threw: ${cppError}`);
      } else {
        failReasons.push(`C++/Rust mismatch: C++ "${cppIdentifier}" vs Rust "${rustIdentifier}"`);
      }
    }

    const reason = failReasons.join('; ');
    console.log(`FAIL  ${tc.description} => ${reason}`);
    failures.push({ ...tc, reason });
  }
}

// ---------------------------------------------------------------------------
// Server config validation for key providers
// ---------------------------------------------------------------------------

console.log('\n--- Server config spot-checks ---\n');

const serverChecks = [
  { email: 'test@gmail.com', identifier: 'gmail', imapHost: 'imap.gmail.com', imapPort: 993, smtpHost: 'smtp.gmail.com' },
  { email: 'test@outlook.com', identifier: 'outlook', imapHost: 'imap-mail.outlook.com', imapPort: 993, smtpHost: 'smtp-mail.outlook.com' },
  { email: 'test@yahoo.com', identifier: 'yahoo', imapHost: 'imap.mail.yahoo.com', imapPort: 993 },
  { email: 'test@fastmail.com', identifier: 'fastmail', imapHost: 'mail.messagingengine.com', imapPort: 993 },
  { email: 'test@hushmail.com', identifier: 'hushmail', imapHost: 'imap.hushmail.com', imapPort: 993 },
];

for (const check of serverChecks) {
  let result = null;
  try {
    result = rustAddon.providerForEmail(check.email);
  } catch (e) {
    console.log(`FAIL  server-config ${check.identifier}: threw ${e.message}`);
    failed++;
    failures.push({ email: check.email, expectedIdentifier: check.identifier, reason: `threw: ${e.message}` });
    continue;
  }

  if (!result) {
    console.log(`FAIL  server-config ${check.identifier}: no result returned`);
    failed++;
    failures.push({ email: check.email, expectedIdentifier: check.identifier, reason: 'returned null' });
    continue;
  }

  const imap = result.servers && result.servers.imap && result.servers.imap[0];
  let ok = true;
  const errors = [];

  if (result.identifier !== check.identifier) {
    errors.push(`identifier: expected "${check.identifier}", got "${result.identifier}"`);
    ok = false;
  }
  if (check.imapHost && (!imap || imap.hostname !== check.imapHost)) {
    errors.push(`IMAP hostname: expected "${check.imapHost}", got "${imap ? imap.hostname : 'undefined'}"`);
    ok = false;
  }
  if (check.imapPort && (!imap || imap.port !== check.imapPort)) {
    errors.push(`IMAP port: expected ${check.imapPort}, got ${imap ? imap.port : 'undefined'}`);
    ok = false;
  }
  if (check.smtpHost) {
    const smtp = result.servers && result.servers.smtp && result.servers.smtp[0];
    if (!smtp || smtp.hostname !== check.smtpHost) {
      errors.push(`SMTP hostname: expected "${check.smtpHost}", got "${smtp ? smtp.hostname : 'undefined'}"`);
      ok = false;
    }
  }

  if (ok) {
    passed++;
    console.log(`PASS  server-config ${check.identifier}: ${check.imapHost}:${check.imapPort} [${imap ? imap.connectionType : 'N/A'}]`);
  } else {
    failed++;
    console.log(`FAIL  server-config ${check.identifier}: ${errors.join('; ')}`);
    failures.push({ email: check.email, expectedIdentifier: check.identifier, reason: errors.join('; ') });
  }
}

// ---------------------------------------------------------------------------
// Error input validation
// ---------------------------------------------------------------------------

console.log('\n--- Error input tests ---\n');

const errorCases = [
  { email: '', description: 'empty string should throw' },
  { email: 'notanemail', description: 'missing @ should throw' },
  { email: 'test@', description: 'empty domain should throw' },
];

for (const ec of errorCases) {
  try {
    rustAddon.providerForEmail(ec.email);
    console.log(`FAIL  ${ec.description} (expected throw, but returned normally)`);
    failed++;
    failures.push({ email: ec.email, expectedIdentifier: 'throw', reason: 'did not throw as expected' });
  } catch (e) {
    console.log(`PASS  ${ec.description}: threw "${e.message}"`);
    passed++;
  }
}

// ---------------------------------------------------------------------------
// Summary
// ---------------------------------------------------------------------------

const total = passed + failed;
console.log(`\n${'='.repeat(60)}`);
console.log(`Results: ${passed}/${total} passed, ${failed} failed`);

if (failed > 0) {
  console.log(`\nFailed cases:`);
  for (const f of failures) {
    console.log(`  - ${f.email}: ${f.reason}`);
  }
  console.log('\nCross-validation FAILED');
  process.exit(1);
} else {
  console.log('\nAll cross-validation tests PASSED');
  if (cppAddon) {
    console.log('Rust and C++ provider results are identical for all tested cases.');
  }
  process.exit(0);
}
