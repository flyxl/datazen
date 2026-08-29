//! Integration tests for MCP stdio local token authentication.
//!
//! Spawns the `datazen --mcp-stdio` binary as a subprocess with an isolated
//! `DATAZEN_DATA_DIR` so headless auth behavior is exercised end-to-end.
//!
//! Run with:
//! `CARGO_TARGET_DIR=target cargo test -p datazen --test mcp_stdio_auth -- --nocapture`

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn datazen_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_datazen"))
}

fn token_file(data_dir: &Path) -> PathBuf {
    data_dir.join("mcp.token")
}

fn write_token_file(data_dir: &Path, token: &str) {
    std::fs::create_dir_all(data_dir).unwrap();
    std::fs::write(token_file(data_dir), format!("{token}\n")).unwrap();
}

fn run_mcp_stdio(data_dir: &Path, token: Option<&str>) -> Output {
    let mut cmd = Command::new(datazen_bin());
    cmd.arg("--mcp-stdio")
        .env("DATAZEN_DATA_DIR", data_dir)
        .env("DATAZEN_KEYRING", "file")
        .env_remove("DATAZEN_MCP_TOKEN");
    if let Some(t) = token {
        cmd.env("DATAZEN_MCP_TOKEN", t);
    }
    cmd.output().expect("failed to run datazen --mcp-stdio")
}

fn stderr_text(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// MCP-AUTH-3: first bootstrap creates `mcp.token` and exits with guidance.
#[test]
fn mcp_stdio_bootstrap_creates_token_and_exits() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!token_file(dir.path()).exists());

    let output = run_mcp_stdio(dir.path(), None);

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        stderr_text(&output)
    );
    assert!(token_file(dir.path()).exists());

    let token = std::fs::read_to_string(token_file(dir.path())).unwrap();
    assert!(!token.trim().is_empty());

    let err = stderr_text(&output);
    assert!(
        err.contains("Created MCP auth token"),
        "expected bootstrap message, got: {err}"
    );
    assert!(err.contains("DATAZEN_MCP_TOKEN"));
}

/// MCP-AUTH-1: token file exists but env is unset → reject.
#[test]
fn mcp_stdio_rejects_missing_env() {
    let dir = tempfile::tempdir().unwrap();
    write_token_file(dir.path(), "test-token-value");

    let output = run_mcp_stdio(dir.path(), None);

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        stderr_text(&output)
    );
    let err = stderr_text(&output);
    assert!(
        err.contains("DATAZEN_MCP_TOKEN"),
        "expected env hint, got: {err}"
    );
    assert!(
        !err.contains("invalid DATAZEN_MCP_TOKEN"),
        "wrong-token path should not trigger on missing env: {err}"
    );
}

/// MCP-AUTH-2: wrong token → reject with explicit invalid message.
#[test]
fn mcp_stdio_rejects_invalid_token() {
    let dir = tempfile::tempdir().unwrap();
    write_token_file(dir.path(), "expected-secret-token");

    let output = run_mcp_stdio(dir.path(), Some("wrong-token"));

    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        stderr_text(&output)
    );
    let err = stderr_text(&output);
    assert!(
        err.contains("invalid DATAZEN_MCP_TOKEN"),
        "expected invalid token message, got: {err}"
    );
}

/// Valid token passes auth and MCP server starts (process stays alive briefly).
#[test]
fn mcp_stdio_accepts_valid_token_and_starts_server() {
    let dir = tempfile::tempdir().unwrap();
    let token = "integration-test-token-abc123";
    write_token_file(dir.path(), token);

    let mut child = Command::new(datazen_bin())
        .arg("--mcp-stdio")
        .env("DATAZEN_DATA_DIR", dir.path())
        .env("DATAZEN_KEYRING", "file")
        .env("DATAZEN_MCP_TOKEN", token)
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn datazen --mcp-stdio");

    // Hold stdin open so MCP transport does not see EOF before we observe startup.
    let _stdin_guard = child.stdin.take();

    let stderr = child.stderr.take().expect("stderr pipe");
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut handle = stderr;
        let _ = handle.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    });

    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().expect("try_wait failed") {
            let logs = reader.join().unwrap_or_default();
            panic!("datazen exited early with {status}; stderr:\n{logs}");
        }
        if Instant::now() >= deadline {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    child.kill().ok();
    child.wait().ok();

    let logs = reader.join().unwrap_or_default();
    assert!(
        logs.contains("starting MCP Server") || logs.contains("[mcp]"),
        "expected MCP server startup log, got:\n{logs}"
    );
}
