// Integration tests for mailsync-rs offline process modes.
// Tests spawn the compiled binary as a child process to verify correct behavior.
// Run with: cargo test --test mode_tests --test-threads=1
//
// NOTE: Tests run with --test-threads=1 because they spawn child processes
// that may compete for the same tempdir paths.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

// ============================================================================
// Test infrastructure helpers
// ============================================================================

/// Returns the path to the compiled mailsync-rs binary.
/// The binary is built automatically by `cargo test` before running integration tests.
/// In a Cargo workspace, the binary is placed in the workspace target directory
/// (app/target/debug/), not the crate's local target directory.
fn binary_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is set by cargo to the mailsync-rs package directory
    // The workspace root is one level up from the crate directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let workspace_root = PathBuf::from(manifest_dir)
        .parent()
        .expect("mailsync-rs must have a parent directory (workspace root)")
        .to_path_buf();

    let mut path = workspace_root;
    path.push("target");
    path.push("debug");
    if cfg!(target_os = "windows") {
        path.push("mailsync-rs.exe");
    } else {
        path.push("mailsync-rs");
    }
    path
}

/// Runs the binary with the given mode, CONFIG_DIR_PATH set to config_dir,
/// optional stdin data, and returns (exit_code, stdout_string, stderr_string).
fn run_mode(mode: &str, config_dir: Option<&str>, stdin_data: Option<&str>) -> (i32, String, String) {
    let bin = binary_path();
    let mut cmd = Command::new(&bin);
    cmd.arg("--mode").arg(mode);

    if let Some(dir) = config_dir {
        cmd.env("CONFIG_DIR_PATH", dir);
    }

    if stdin_data.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to spawn mailsync-rs binary");

    // Write stdin data if provided
    if let (Some(data), Some(mut stdin)) = (stdin_data, child.stdin.take()) {
        stdin.write_all(data.as_bytes()).ok();
        // Drop stdin to signal EOF
    }

    let output = child.wait_with_output().expect("Failed to wait for binary");

    let exit_code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    (exit_code, stdout, stderr)
}

// ============================================================================
// Task 1 Tests: install-check and test modes
// ============================================================================

/// --mode install-check must exit with code 0.
#[test]
fn test_install_check_exits_0() {
    let (exit_code, stdout, _stderr) = run_mode("install-check", None, None);
    assert_eq!(
        exit_code, 0,
        "install-check should exit with code 0, got {exit_code}"
    );
    // Verify stdout contains valid JSON
    let json: serde_json::Value = serde_json::from_str(&stdout.trim())
        .expect("install-check stdout should be valid JSON");
    // Verify expected check keys are present
    assert!(
        json.get("http_check").is_some(),
        "install-check JSON should contain 'http_check'"
    );
    assert!(
        json.get("imap_check").is_some(),
        "install-check JSON should contain 'imap_check'"
    );
}

/// --mode test must exit with code 1 and emit JSON error on stdout.
/// test mode needs account+identity JSON on stdin.
#[test]
fn test_test_mode_exits_1() {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    // test mode reads account+identity JSON from stdin (two-line handshake)
    let stdin_data = "{\"id\":\"acct1\",\"emailAddress\":\"test@example.com\",\"provider\":\"gmail\"}\n{\"id\":\"identity1\"}\n";

    let (exit_code, stdout, _stderr) = run_mode("test", Some(config_dir), Some(stdin_data));

    assert_eq!(
        exit_code, 1,
        "test mode should exit with code 1 (not implemented), got {exit_code}"
    );

    // stdout may contain handshake prompt lines ("Waiting for Account JSON:" etc.)
    // Find the last line that is valid JSON
    let json = parse_last_json_line(&stdout)
        .unwrap_or_else(|| panic!("test mode stdout should contain a JSON line, got: '{stdout}'"));
    assert!(
        json.get("error").is_some(),
        "test mode JSON error should contain 'error' field, got: {json}"
    );
}

/// SyncError variants must serialize to exact C++ error key strings.
/// This is a library test (tests the error module directly).
#[test]
fn test_sync_error_keys_match_cpp() {
    // This test verifies the error key strings match C++ baseline.
    // We check by running --mode test and verifying the error key in the output.
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let stdin_data = "{\"id\":\"acct1\",\"emailAddress\":\"test@example.com\",\"provider\":\"gmail\"}\n{\"id\":\"identity1\"}\n";
    let (exit_code, stdout, _stderr) = run_mode("test", Some(config_dir), Some(stdin_data));

    assert_eq!(exit_code, 1);

    let json = parse_last_json_line(&stdout)
        .unwrap_or_else(|| panic!("Expected JSON line in stdout, got: '{stdout}'"));

    // The error key must be "ErrorNotImplemented" — the exact C++ string
    let error_key = json["error"].as_str().unwrap_or("");
    assert_eq!(
        error_key, "ErrorNotImplemented",
        "test mode should emit error key 'ErrorNotImplemented'"
    );
}

/// Parses the last line from stdout that contains valid JSON.
/// Used to handle multi-line stdout (handshake prompts followed by JSON).
fn parse_last_json_line(stdout: &str) -> Option<serde_json::Value> {
    stdout
        .lines()
        .rev()
        .filter(|line| !line.is_empty())
        .find_map(|line| serde_json::from_str(line.trim()).ok())
}

// ============================================================================
// Task 2 Tests: migrate and reset modes
// ============================================================================

/// --mode migrate must create edgehill.db with user_version = 9.
#[test]
fn test_migrate_creates_schema() {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let (exit_code, _stdout, stderr) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(
        exit_code, 0,
        "migrate should exit with code 0, got {exit_code}. stderr: {stderr}"
    );

    // Open the created database and check user_version
    let db_path = tempdir.path().join("edgehill.db");
    assert!(db_path.exists(), "edgehill.db should exist after migrate");

    let conn = rusqlite::Connection::open(&db_path).expect("Failed to open edgehill.db");
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("Failed to query user_version");
    assert_eq!(version, 9, "Schema version should be 9 after full migration");
}

/// --mode migrate must create all 22+ expected tables.
#[test]
fn test_migrate_creates_all_tables() {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let (exit_code, _stdout, stderr) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(exit_code, 0, "migrate failed: {stderr}");

    let db_path = tempdir.path().join("edgehill.db");
    let conn = rusqlite::Connection::open(&db_path).expect("Failed to open edgehill.db");

    // Query all table names
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("Failed to prepare table query");
    let tables: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .expect("Failed to query tables")
        .filter_map(|r| r.ok())
        .collect();

    // All expected tables from the C++ constants.h schema
    let expected_tables = [
        "_State",
        "Account",
        "Calendar",
        "Contact",
        "ContactBook",
        "ContactContactGroup",
        "ContactGroup",
        "ContactSearch",
        "DetatchedPluginMetadata",
        "Event",
        "EventSearch",
        "File",
        "Folder",
        "Label",
        "Message",
        "MessageBody",
        "ModelPluginMetadata",
        "Task",
        "Thread",
        "ThreadCategory",
        "ThreadCounts",
        "ThreadReference",
        "ThreadSearch",
    ];

    for expected in &expected_tables {
        assert!(
            tables.iter().any(|t| t == expected),
            "Expected table '{expected}' not found. Tables present: {tables:?}"
        );
    }

    assert!(
        tables.len() >= 22,
        "Expected at least 22 tables, found {}: {tables:?}",
        tables.len()
    );
}

/// --mode migrate must create FTS5 virtual tables.
#[test]
fn test_migrate_creates_fts5_tables() {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let (exit_code, _stdout, _stderr) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(exit_code, 0, "migrate should succeed");

    let db_path = tempdir.path().join("edgehill.db");
    let conn = rusqlite::Connection::open(&db_path).expect("Failed to open edgehill.db");

    // Check FTS5 virtual tables exist (they appear as tables in sqlite_master)
    let fts5_tables = ["ThreadSearch", "EventSearch", "ContactSearch"];
    for table in &fts5_tables {
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = ? AND type = 'table'",
                [table],
                |row| row.get(0),
            )
            .expect("Failed to query FTS5 table existence");
        assert_eq!(
            count, 1,
            "FTS5 virtual table '{table}' should exist after migrate"
        );

        // Verify it's actually an FTS5 table (has _data, _idx, etc. shadow tables)
        let shadow_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE ? AND type = 'table'",
                [&format!("{table}_%")],
                |row| row.get(0),
            )
            .expect("Failed to query FTS5 shadow tables");
        assert!(
            shadow_count > 0,
            "FTS5 table '{table}' should have shadow tables (name_data, name_idx, etc.)"
        );
    }
}

/// --mode migrate must be idempotent — running twice produces no errors.
#[test]
fn test_migrate_idempotent() {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    // Run migrate twice
    let (exit_code1, _stdout1, stderr1) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(exit_code1, 0, "First migrate should succeed. stderr: {stderr1}");

    let (exit_code2, _stdout2, stderr2) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(
        exit_code2, 0,
        "Second migrate (idempotent) should succeed. stderr: {stderr2}"
    );

    // Verify schema version is still 9 after second run
    let db_path = tempdir.path().join("edgehill.db");
    let conn = rusqlite::Connection::open(&db_path).expect("Failed to open edgehill.db");
    let version: i32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("Failed to query user_version");
    assert_eq!(version, 9, "Schema version should remain 9 after second migrate");
}

/// --mode reset must delete rows for specified account only (other account data preserved).
#[test]
fn test_reset_isolates_account_data() {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    // First, migrate to create the schema
    let (exit_code, _stdout, stderr) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(exit_code, 0, "migrate should succeed: {stderr}");

    let db_path = tempdir.path().join("edgehill.db");

    // Insert test data for two accounts
    {
        let conn = rusqlite::Connection::open(&db_path).expect("Failed to open edgehill.db");
        conn.execute(
            "INSERT INTO Thread (id, accountId, version, data, subject, unread, starred, hasAttachments, lastMessageTimestamp, firstMessageTimestamp, lastMessageReceivedTimestamp, lastMessageSentTimestamp, inAllMail, isSearchIndexed, participants) VALUES (?, ?, 1, '{}', 'Test Thread 1', 0, 0, 0, 0, 0, 0, 0, 0, 0, '')",
            ["thread-acct1", "acct1"],
        ).expect("Failed to insert thread for acct1");
        conn.execute(
            "INSERT INTO Thread (id, accountId, version, data, subject, unread, starred, hasAttachments, lastMessageTimestamp, firstMessageTimestamp, lastMessageReceivedTimestamp, lastMessageSentTimestamp, inAllMail, isSearchIndexed, participants) VALUES (?, ?, 1, '{}', 'Test Thread 2', 0, 0, 0, 0, 0, 0, 0, 0, 0, '')",
            ["thread-acct2", "acct2"],
        ).expect("Failed to insert thread for acct2");
    }

    // Verify both rows exist
    {
        let conn = rusqlite::Connection::open(&db_path).expect("Failed to open edgehill.db");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM Thread", [], |row| row.get(0))
            .expect("Failed to count threads");
        assert_eq!(count, 2, "Should have 2 threads before reset");
    }

    // Run --mode reset for acct1 only
    let account_json = "{\"id\":\"acct1\",\"emailAddress\":\"acct1@example.com\",\"provider\":\"gmail\"}\n";
    let (exit_code, _stdout, stderr) = run_mode("reset", Some(config_dir), Some(account_json));
    assert_eq!(exit_code, 0, "reset should exit with code 0. stderr: {stderr}");

    // Verify acct1 data is deleted but acct2 data is preserved
    {
        let conn = rusqlite::Connection::open(&db_path).expect("Failed to open edgehill.db");
        let acct1_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM Thread WHERE accountId = 'acct1'",
                [],
                |row| row.get(0),
            )
            .expect("Failed to count acct1 threads");
        assert_eq!(acct1_count, 0, "acct1 threads should be deleted after reset");

        let acct2_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM Thread WHERE accountId = 'acct2'",
                [],
                |row| row.get(0),
            )
            .expect("Failed to count acct2 threads");
        assert_eq!(acct2_count, 1, "acct2 threads should be preserved after reset of acct1");
    }
}

/// --mode reset must exit with code 0 on success.
#[test]
fn test_reset_exits_0() {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    // Migrate first so tables exist
    let (exit_code, _stdout, stderr) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(exit_code, 0, "migrate should succeed: {stderr}");

    // Run reset with a valid account JSON
    let account_json = "{\"id\":\"test-account\",\"emailAddress\":\"test@example.com\",\"provider\":\"gmail\"}\n";
    let (exit_code, _stdout, stderr) = run_mode("reset", Some(config_dir), Some(account_json));
    assert_eq!(
        exit_code, 0,
        "reset should exit with code 0. stderr: {stderr}"
    );
}
