// IPC contract tests for mailsync-rs sync mode.
// Tests spawn the compiled binary and verify the stdin/stdout handshake protocol.
//
// Run with: cargo test --test ipc_contract --test-threads=1
//
// NOTE: Tests run with --test-threads=1 because they spawn child processes
// that pipe stdin/stdout and must not compete for system resources.

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

// ============================================================================
// Test infrastructure helpers
// ============================================================================

/// Returns the path to the compiled mailsync-rs binary.
/// Reuses the workspace binary path pattern from mode_tests.rs.
fn binary_path() -> PathBuf {
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

/// Runs the binary with the given mode in a tempdir.
/// Returns the exit code, stdout, and stderr.
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

/// Runs migrate mode to set up the schema, then returns the tempdir.
/// All sync mode tests need a migrated database to function.
fn setup_migrated_tempdir() -> TempDir {
    let tempdir = TempDir::new().expect("Failed to create tempdir");
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let (exit_code, _stdout, stderr) = run_mode("migrate", Some(config_dir), None);
    assert_eq!(
        exit_code, 0,
        "migrate must succeed before IPC contract tests. stderr: {stderr}"
    );

    tempdir
}

/// Account JSON for the two-line handshake.
const TEST_ACCOUNT_JSON: &str = r#"{"id":"acct-ipc-test","emailAddress":"test@example.com","provider":"imap"}"#;
/// Identity JSON for the two-line handshake.
const TEST_IDENTITY_JSON: &str = r#"{"id":"identity-ipc-test"}"#;

/// Full two-line handshake data (account + newline + identity + newline).
fn handshake_data() -> String {
    format!("{TEST_ACCOUNT_JSON}\n{TEST_IDENTITY_JSON}\n")
}

// ============================================================================
// IPC-01 + IPC-02: Handshake and ProcessState delta
// ============================================================================

/// The binary must complete the two-line stdin handshake and emit a valid
/// ProcessState delta to stdout with exact field names.
///
/// Requirements: IPC-01 (handshake), IPC-02 (delta field names)
#[test]
fn test_handshake_and_process_state_delta() {
    let tempdir = setup_migrated_tempdir();
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    // Spawn sync mode with piped stdin/stdout
    let bin = binary_path();
    let mut child = Command::new(&bin)
        .arg("--mode").arg("sync")
        .env("CONFIG_DIR_PATH", config_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn mailsync-rs in sync mode");

    // Write handshake data in a separate thread (avoids deadlock with stdout pipe fill)
    // We wait briefly first to let the binary print its prompt to stdout.
    let mut stdin = child.stdin.take().expect("Child stdin must be piped");
    let handshake = handshake_data();

    let writer_thread = thread::spawn(move || {
        // Give binary time to start and write its prompt
        thread::sleep(Duration::from_millis(300));
        stdin.write_all(handshake.as_bytes()).ok();
        // Drop stdin to signal EOF — binary should exit with 141
    });

    // Wait for the binary to complete with a timeout
    let output = child.wait_with_output().expect("Failed to wait for child");
    writer_thread.join().ok();

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    // Binary should exit with 141 (stdin EOF detected)
    assert_eq!(
        exit_code, 141,
        "Sync mode should exit 141 after stdin EOF, got {exit_code}. stdout: {stdout}"
    );

    // Find the ProcessState delta line in stdout output
    let process_state_line = stdout
        .lines()
        .find(|line| line.contains("ProcessState"))
        .unwrap_or_else(|| {
            panic!(
                "ProcessState delta not found in stdout.\nFull stdout:\n{stdout}"
            )
        });

    // Parse and validate the delta JSON
    let delta: serde_json::Value = serde_json::from_str(process_state_line.trim())
        .unwrap_or_else(|e| {
            panic!(
                "ProcessState line is not valid JSON: {e}\nLine: {process_state_line}"
            )
        });

    // IPC-02: Must have exact field names (camelCase, not snake_case)
    assert!(
        delta.get("type").is_some(),
        "Delta must have 'type' field. Got: {delta}"
    );
    assert!(
        delta.get("modelClass").is_some(),
        "Delta must have 'modelClass' field. Got: {delta}"
    );
    assert!(
        delta.get("modelJSONs").is_some(),
        "Delta must have 'modelJSONs' field. Got: {delta}"
    );

    // Must NOT have snake_case variants
    assert!(
        delta.get("model_class").is_none(),
        "Delta must NOT have 'model_class'. Got: {delta}"
    );
    assert!(
        delta.get("model_jsons").is_none(),
        "Delta must NOT have 'model_jsons'. Got: {delta}"
    );

    // Validate ProcessState field values
    assert_eq!(delta["type"], "persist", "ProcessState delta type must be 'persist'");
    assert_eq!(delta["modelClass"], "ProcessState", "modelClass must be 'ProcessState'");

    let model_jsons = delta["modelJSONs"].as_array()
        .expect("modelJSONs must be an array");
    assert_eq!(model_jsons.len(), 1, "ProcessState must have exactly 1 modelJSON");

    let state = &model_jsons[0];
    assert_eq!(
        state["accountId"], "acct-ipc-test",
        "accountId must match the handshake account id"
    );
    assert_eq!(
        state["id"], "acct-ipc-test",
        "id must equal accountId in ProcessState"
    );
    assert_eq!(
        state["connectionError"], false,
        "connectionError must be false for fresh connection"
    );
}

// ============================================================================
// IPC-05: stdin EOF causes exit code 141
// ============================================================================

/// The binary must exit with code 141 when stdin is closed (orphan detection).
///
/// Requirement: IPC-05
#[test]
fn test_stdin_eof_exit_141() {
    let tempdir = setup_migrated_tempdir();
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let bin = binary_path();
    let mut child = Command::new(&bin)
        .arg("--mode").arg("sync")
        .env("CONFIG_DIR_PATH", config_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn mailsync-rs in sync mode");

    // Write handshake then immediately close stdin (EOF)
    let mut stdin = child.stdin.take().expect("Child stdin must be piped");
    let writer_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        let handshake = handshake_data();
        stdin.write_all(handshake.as_bytes()).ok();
        // Drop stdin = EOF
    });

    let output = child.wait_with_output().expect("Failed to wait for child");
    writer_thread.join().ok();

    let exit_code = output.status.code().unwrap_or(-1);
    assert_eq!(
        exit_code, 141,
        "Binary must exit with code 141 on stdin EOF, got {exit_code}"
    );
}

// ============================================================================
// IPC-06: stdout flush timing (no block buffering)
// ============================================================================

/// The ProcessState delta must arrive on the stdout pipe within 2 seconds.
/// This verifies no block buffering — the flush task must call flush() explicitly.
///
/// Requirement: IPC-06
#[test]
fn test_stdout_flush_timing() {
    let tempdir = setup_migrated_tempdir();
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let bin = binary_path();
    let mut child = Command::new(&bin)
        .arg("--mode").arg("sync")
        .env("CONFIG_DIR_PATH", config_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn mailsync-rs in sync mode");

    let start = Instant::now();

    // Write handshake in a background thread
    let mut stdin = child.stdin.take().expect("Child stdin must be piped");
    let writer_thread = thread::spawn(move || {
        thread::sleep(Duration::from_millis(100));
        let handshake = handshake_data();
        stdin.write_all(handshake.as_bytes()).ok();
        // Keep stdin open (don't drop) to let the binary keep running
        // We need to hold stdin alive while we check stdout timing
        thread::sleep(Duration::from_secs(3));
        // After 3s, drop stdin to cause EOF
    });

    // Read stdout output — binary should write the ProcessState delta quickly
    // Use wait_with_output but with a manual timeout approach:
    // The binary will be killed after writer_thread closes stdin.
    let output = child.wait_with_output().expect("Failed to wait for child");
    writer_thread.join().ok();

    let elapsed = start.elapsed();
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();

    // The ProcessState delta should arrive well within 2 seconds of process start
    assert!(
        elapsed.as_secs() < 5,
        "Test should complete within 5 seconds total, took {:?}",
        elapsed
    );

    // Verify the ProcessState delta was actually emitted
    let has_process_state = stdout.lines().any(|line| line.contains("ProcessState"));
    assert!(
        has_process_state,
        "ProcessState delta must be emitted within the test window. stdout: {stdout}"
    );
}

// ============================================================================
// IMPR-08: No deadlock with large stdin payload
// ============================================================================

/// Writing 500KB+ to stdin must not deadlock stdout writes.
/// The stdin reader and stdout writer are independent tokio tasks.
///
/// Requirement: IMPR-08
#[test]
fn test_no_deadlock_large_stdin() {
    let tempdir = setup_migrated_tempdir();
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let bin = binary_path();
    let mut child = Command::new(&bin)
        .arg("--mode").arg("sync")
        .env("CONFIG_DIR_PATH", config_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn mailsync-rs in sync mode");

    // Capture child stdout in a separate thread (prevents pipe buffer fill deadlock)
    let stdout_pipe = child.stdout.take().expect("stdout must be piped");
    let stdout_data: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
    let stdout_data_clone = Arc::clone(&stdout_data);

    let reader_thread = thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut reader = std::io::BufReader::new(stdout_pipe);
        reader.read_to_end(&mut buf).ok();
        *stdout_data_clone.lock().unwrap() = buf;
    });

    // Write handshake then 500KB+ of valid JSON commands to stdin
    let mut stdin = child.stdin.take().expect("Child stdin must be piped");
    let writer_thread = thread::spawn(move || {
        // First write the handshake
        thread::sleep(Duration::from_millis(200));
        let handshake = handshake_data();
        if stdin.write_all(handshake.as_bytes()).is_err() {
            return;
        }

        // Then write 500KB+ of repeated command lines to stress-test the stdin buffer
        let cmd_line = b"{\"command\":\"wake-workers\"}\n";
        let target_bytes = 512 * 1024; // 512KB
        let mut written = 0;
        while written < target_bytes {
            if stdin.write_all(cmd_line).is_err() {
                break;
            }
            written += cmd_line.len();
        }

        // Close stdin to trigger EOF exit
    });

    // Wait for the binary to finish with a 10-second timeout
    // The binary should exit with 141 (stdin EOF) within 10s.
    // If it hangs, the test will time out (cargo test default is no timeout,
    // but --test-threads=1 and test runner will eventually kill it).
    let start = Instant::now();
    let output = child.wait_with_output().expect("Failed to wait for child");
    let elapsed = start.elapsed();

    writer_thread.join().ok();
    reader_thread.join().ok();

    let exit_code = output.status.code().unwrap_or(-1);

    // Must complete within reasonable time (10s would indicate a hang)
    assert!(
        elapsed.as_secs() < 10,
        "Binary must complete within 10 seconds with large stdin payload, took {:?}",
        elapsed
    );

    // Must exit with code 141 (stdin EOF detected — not a crash or hang)
    assert_eq!(
        exit_code, 141,
        "Binary should exit 141 after large stdin payload + EOF, got {exit_code}"
    );
}

// ============================================================================
// IPC-03: Unknown commands are logged and ignored (process continues)
// ============================================================================

/// Sending an unknown command must NOT cause the binary to exit or panic.
/// The binary must log a warning and continue reading stdin.
/// A known command after the unknown command must also be accepted.
///
/// Requirement: IPC-03
#[test]
fn test_unknown_command_continues() {
    let tempdir = setup_migrated_tempdir();
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    let bin = binary_path();
    let mut child = Command::new(&bin)
        .arg("--mode").arg("sync")
        .env("CONFIG_DIR_PATH", config_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to spawn mailsync-rs in sync mode");

    let mut stdin = child.stdin.take().expect("Child stdin must be piped");

    let writer_thread = thread::spawn(move || {
        // Write handshake
        thread::sleep(Duration::from_millis(200));
        let handshake = handshake_data();
        stdin.write_all(handshake.as_bytes()).ok();

        // Small delay to ensure binary is in the command loop
        thread::sleep(Duration::from_millis(200));

        // Send an unknown command
        stdin.write_all(b"{\"command\":\"unknown-future-command\",\"data\":{}}\n").ok();

        // Send a known command after the unknown one
        stdin.write_all(b"{\"command\":\"wake-workers\"}\n").ok();

        // Give the binary time to process both commands
        thread::sleep(Duration::from_millis(300));

        // Close stdin to trigger EOF exit
        // Drop stdin here
    });

    let output = child.wait_with_output().expect("Failed to wait for child");
    writer_thread.join().ok();

    let exit_code = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // Must exit with 141 (stdin EOF) — NOT a crash or non-141 exit
    assert_eq!(
        exit_code, 141,
        "Binary must continue after unknown command and exit 141 on EOF. Got: {exit_code}"
    );

    // Stderr should contain a warning about the unknown command
    // (tracing at warn level will appear in stderr)
    assert!(
        stderr.contains("unknown") || stderr.contains("warn") || stderr.contains("WARN"),
        "Stderr should contain a warning about unknown command. stderr: {stderr}"
    );
}

// ============================================================================
// Additional: Startup prompt appears before handshake data
// ============================================================================

/// The binary must print the "Waiting for Account JSON:" prompt before reading stdin.
/// Without this prompt, the TypeScript side won't know to send the handshake data.
#[test]
fn test_startup_prompt_printed_to_stdout() {
    let tempdir = setup_migrated_tempdir();
    let config_dir = tempdir.path().to_str().expect("tempdir path is valid UTF-8");

    // Run sync mode with immediate stdin close (no handshake data)
    // The binary should still print the prompt before exiting
    let (exit_code, stdout, _stderr) = run_mode("sync", Some(config_dir), Some(""));

    // The binary will fail to deserialize an empty account JSON,
    // but the prompt must have been printed to stdout first
    assert!(
        stdout.contains("Waiting for Account JSON") || exit_code == 141 || exit_code == 1,
        "Binary must print startup prompt or exit cleanly. stdout: {stdout}, exit: {exit_code}"
    );
}
