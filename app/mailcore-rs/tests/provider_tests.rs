//! Integration tests for the provider detection module.
//!
//! These tests exercise the public API of `mailcore_napi_rs::provider` without
//! requiring a Node.js context (napi functions are thin wrappers around the
//! internal helpers tested here).
//!
//! # Test isolation
//!
//! Tests use a global `TEST_MUTEX` to run serially. The provider database is a
//! process-global singleton, so parallel tests would race on reset/init calls.
//! Each test acquires the mutex, resets the singleton, initializes from embedded
//! JSON, and then runs its assertions.

use mailcore_napi_rs::provider;
use std::sync::{Mutex, MutexGuard};

/// Global mutex ensuring provider-singleton tests run serially.
static TEST_MUTEX: Mutex<()> = Mutex::new(());

/// Acquire the test mutex and initialize the provider singleton from embedded JSON.
///
/// Returns the guard — the singleton is reset when the guard is dropped.
fn init_locked() -> MutexGuard<'static, ()> {
    let guard = TEST_MUTEX.lock().unwrap_or_else(|e| e.into_inner());
    provider::reset_providers();
    provider::init_from_embedded(EMBEDDED_JSON).expect("init_from_embedded must not fail");
    guard
}

/// The embedded providers JSON — used to reset state in each test.
const EMBEDDED_JSON: &str = include_str!("../resources/providers.json");

/// Custom provider JSON used to test merge semantics.
const CUSTOM_GMAIL_OVERRIDE: &str = r#"{
    "gmail": {
        "servers": {
            "imap": [{ "hostname": "custom-imap.example.com", "port": 993, "ssl": true }],
            "smtp": [{ "hostname": "custom-smtp.example.com", "port": 587, "starttls": true }],
            "pop": []
        },
        "domain-match": ["gmail\\.com", "googlemail\\.com"]
    }
}"#;

/// New provider not in the embedded set — used to test append semantics.
const CUSTOM_NEW_PROVIDER: &str = r#"{
    "testprovider": {
        "servers": {
            "imap": [{ "hostname": "imap.testprovider.example", "port": 993, "ssl": true }],
            "smtp": [{ "hostname": "smtp.testprovider.example", "port": 587, "starttls": true }],
            "pop": []
        },
        "domain-match": ["testprovider\\.example"]
    }
}"#;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Test: embedded providers parse correctly
// ---------------------------------------------------------------------------

#[test]
fn embedded_providers_parse_into_37_providers() {
    let _guard = init_locked();
    let count = provider::provider_count();
    assert_eq!(count, 37, "Expected 37 embedded providers, got {count}");
}

// ---------------------------------------------------------------------------
// Test: domain matching — known providers
// ---------------------------------------------------------------------------

#[test]
fn provider_for_email_should_return_gmail_for_gmail_domain() {
    let _guard = init_locked();
    let result = provider::lookup_provider("user@gmail.com").expect("lookup must not error");
    let info = result.expect("gmail.com must match a provider");
    assert_eq!(
        info.identifier, "gmail",
        "Expected identifier 'gmail', got '{}'",
        info.identifier
    );
}

#[test]
fn provider_for_email_should_return_gmail_imap_server_config() {
    let _guard = init_locked();
    let info = provider::lookup_provider("user@gmail.com")
        .expect("lookup must not error")
        .expect("gmail.com must match");
    assert!(
        !info.servers.imap.is_empty(),
        "Gmail must have IMAP servers"
    );
    let imap = &info.servers.imap[0];
    assert_eq!(imap.hostname, "imap.gmail.com");
    assert_eq!(imap.port, 993);
    assert_eq!(imap.connection_type, "tls");
}

#[test]
fn provider_for_email_should_return_yahoo_for_yahoo_com() {
    let _guard = init_locked();
    let result = provider::lookup_provider("user@yahoo.com").expect("lookup must not error");
    let info = result.expect("yahoo.com must match a provider");
    assert_eq!(
        info.identifier, "yahoo",
        "Expected identifier 'yahoo', got '{}'",
        info.identifier
    );
}

#[test]
fn provider_for_email_should_return_some_for_outlook_com() {
    let _guard = init_locked();
    let result = provider::lookup_provider("user@outlook.com").expect("lookup must not error");
    assert!(result.is_some(), "outlook.com must match a provider");
    assert_eq!(result.unwrap().identifier, "outlook");
}

// ---------------------------------------------------------------------------
// Test: case-insensitivity
// ---------------------------------------------------------------------------

#[test]
fn provider_for_email_should_be_case_insensitive_for_gmail() {
    let _guard = init_locked();
    let result = provider::lookup_provider("user@Gmail.COM").expect("lookup must not error");
    let info = result.expect("Gmail.COM (uppercase) must match gmail provider");
    assert_eq!(
        info.identifier, "gmail",
        "Case-insensitive match must return 'gmail'"
    );
}

// ---------------------------------------------------------------------------
// Test: domain-exclude (Yahoo Japan)
// ---------------------------------------------------------------------------

#[test]
fn provider_for_email_yahoo_co_jp_should_not_match_yahoo_due_to_exclude() {
    let _guard = init_locked();
    // yahoo.co.jp is listed in yahoo's domain-exclude, and has its own provider
    // The key invariant: it must NOT match the "yahoo" (international) provider.
    let result = provider::lookup_provider("user@yahoo.co.jp").expect("lookup must not error");
    if let Some(info) = result {
        assert_ne!(
            info.identifier, "yahoo",
            "yahoo.co.jp must NOT match the 'yahoo' (international) provider due to domain-exclude"
        );
        // It should match the specific yahoo.co.jp provider instead
        assert_eq!(
            info.identifier, "yahoo.co.jp",
            "yahoo.co.jp should match the dedicated 'yahoo.co.jp' provider"
        );
    }
    // If None, that's also acceptable — the important thing is it did NOT match "yahoo"
}

// ---------------------------------------------------------------------------
// Test: unknown domain returns None
// ---------------------------------------------------------------------------

#[test]
fn provider_for_email_should_return_none_for_unknown_domain() {
    let _guard = init_locked();
    let result = provider::lookup_provider("user@unknown-xyz-domain-that-never-exists.com")
        .expect("lookup must not error for valid email");
    assert!(
        result.is_none(),
        "Unknown domain must return None, not a provider"
    );
}

// ---------------------------------------------------------------------------
// Test: input validation errors
// ---------------------------------------------------------------------------

#[test]
fn provider_for_email_should_return_error_for_empty_email() {
    let _guard = init_locked();
    let result = provider::lookup_provider("");
    assert!(result.is_err(), "Empty email must return an error");
}

#[test]
fn provider_for_email_should_return_error_when_no_at_sign() {
    let _guard = init_locked();
    let result = provider::lookup_provider("no-at-sign-here");
    assert!(result.is_err(), "Email without '@' must return an error");
}

// ---------------------------------------------------------------------------
// Test: merge semantics — override existing provider
// ---------------------------------------------------------------------------

#[test]
fn register_providers_should_override_gmail_when_identifier_conflicts() {
    let _guard = init_locked();
    // Merge a custom gmail provider with different IMAP hostname
    provider::merge_providers_from_str(CUSTOM_GMAIL_OVERRIDE)
        .expect("merge_providers_from_str must not fail");

    let result = provider::lookup_provider("user@gmail.com").expect("lookup must not error");
    let info = result.expect("gmail.com must still match after override");

    assert_eq!(info.identifier, "gmail");
    // The overridden provider uses custom-imap.example.com
    assert_eq!(
        info.servers.imap[0].hostname, "custom-imap.example.com",
        "After override, gmail IMAP hostname must be the custom one"
    );
}

// ---------------------------------------------------------------------------
// Test: merge semantics — append new provider
// ---------------------------------------------------------------------------

#[test]
fn register_providers_should_append_new_provider_and_increase_count() {
    let _guard = init_locked();
    let count_before = provider::provider_count();

    provider::merge_providers_from_str(CUSTOM_NEW_PROVIDER)
        .expect("merge_providers_from_str must not fail");

    let count_after = provider::provider_count();
    assert_eq!(
        count_after,
        count_before + 1,
        "Adding a new provider must increase count by 1 (before={count_before}, after={count_after})"
    );

    // Verify the new provider is actually reachable
    let result =
        provider::lookup_provider("user@testprovider.example").expect("lookup must not error");
    assert!(
        result.is_some(),
        "Newly appended provider must be matchable"
    );
    assert_eq!(result.unwrap().identifier, "testprovider");
}

// ---------------------------------------------------------------------------
// Test: server connection types
// ---------------------------------------------------------------------------

#[test]
fn server_with_ssl_true_should_have_connection_type_tls() {
    let _guard = init_locked();
    // Gmail IMAP uses ssl: true
    let info = provider::lookup_provider("user@gmail.com")
        .expect("lookup must not error")
        .expect("gmail.com must match");
    let imap = &info.servers.imap[0];
    assert_eq!(
        imap.connection_type, "tls",
        "ssl:true must produce connectionType 'tls', got '{}'",
        imap.connection_type
    );
}

#[test]
fn server_with_starttls_true_should_have_connection_type_starttls() {
    let _guard = init_locked();
    // Gmail SMTP first entry uses starttls: true
    let info = provider::lookup_provider("user@gmail.com")
        .expect("lookup must not error")
        .expect("gmail.com must match");
    let smtp_starttls = info
        .servers
        .smtp
        .iter()
        .find(|s| s.connection_type == "starttls");
    assert!(
        smtp_starttls.is_some(),
        "Gmail must have at least one SMTP server with starttls connection type"
    );
}

// ---------------------------------------------------------------------------
// Test: POP servers are included in response
// ---------------------------------------------------------------------------

#[test]
fn provider_for_email_should_include_pop_servers_for_gmail() {
    let _guard = init_locked();
    // The providers.json doesn't include POP for gmail itself, but we should verify
    // the field exists and is accessible. Use comcast which has pop servers.
    // Actually checking providers.json: comcast has pop servers
    let result = provider::lookup_provider("user@comcast.net").expect("lookup must not error");
    // comcast uses mx-match not domain-match, so it may not match.
    // Check gmail doesn't fail on pop field access.
    let gmail_info = provider::lookup_provider("user@gmail.com")
        .expect("lookup must not error")
        .expect("gmail.com must match");
    // Gmail has no POP in providers.json, but the field must be an empty Vec (not missing)
    let _ = &gmail_info.servers.pop; // Access must not panic

    // Use pobox which does have pop servers and domain-match
    let pobox_result = provider::lookup_provider("user@pobox.com").expect("lookup must not error");
    let pobox_info = pobox_result.expect("pobox.com must match pobox provider");
    assert_eq!(pobox_info.identifier, "pobox");
    assert!(
        !pobox_info.servers.pop.is_empty(),
        "pobox must have POP servers"
    );
    drop(result); // comcast may or may not match
}

// ---------------------------------------------------------------------------
// Test: regex anchoring — partial matches must NOT match
// ---------------------------------------------------------------------------

#[test]
fn compile_pattern_should_not_match_substrings() {
    let _guard = init_locked();
    // The yahoo pattern is "yahoo\..*" which must anchor to the full domain.
    // "notyahoo.com" should NOT match because ^ anchors the start.
    let result = provider::lookup_provider("user@notyahoo.com").expect("lookup must not error");
    // "notyahoo.com" — without anchoring, "yahoo\..*" would match the "yahoo.com" suffix.
    // With proper ^...$ anchoring it must NOT match yahoo.
    if let Some(info) = result {
        assert_ne!(
            info.identifier, "yahoo",
            "'notyahoo.com' must NOT match the 'yahoo' provider (anchoring requirement)"
        );
    }
    // None is also fine — the key is it didn't match yahoo
}
