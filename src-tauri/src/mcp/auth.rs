//! MCP stdio local authentication — `{appData}/mcp.token` + `DATAZEN_MCP_TOKEN` env.

use std::path::{Path, PathBuf};

pub const MCP_TOKEN_FILE: &str = "mcp.token";
pub const MCP_TOKEN_ENV: &str = "DATAZEN_MCP_TOKEN";

pub fn token_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join(MCP_TOKEN_FILE)
}

fn generate_token() -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use rand::RngCore;

    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Load the persisted token or create `{data_dir}/mcp.token` on first use.
///
/// Returns `(token, created)` where `created` is true when a new file was written.
pub fn load_or_create_token(data_dir: &Path) -> Result<(String, bool), String> {
    let path = token_file_path(data_dir);
    if path.exists() {
        let token = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read MCP token: {e}"))?
            .trim()
            .to_string();
        if token.is_empty() {
            return Err("MCP token file is empty".into());
        }
        return Ok((token, false));
    }

    std::fs::create_dir_all(data_dir).map_err(|e| format!("Failed to create data dir: {e}"))?;
    let token = generate_token();
    std::fs::write(&path, format!("{token}\n"))
        .map_err(|e| format!("Failed to write MCP token: {e}"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok((token, true))
}

/// Gate headless `--mcp-stdio`: client must pass the token via `DATAZEN_MCP_TOKEN`.
pub fn verify_stdio_token(data_dir: &Path) -> Result<(), String> {
    let (expected, created) = load_or_create_token(data_dir)?;
    let path = token_file_path(data_dir);

    if created {
        return Err(format!(
            "Created MCP auth token at {}. Set env {MCP_TOKEN_ENV} to that value and restart.",
            path.display()
        ));
    }

    let provided = std::env::var(MCP_TOKEN_ENV).map_err(|_| {
        format!(
            "MCP stdio auth: set env {MCP_TOKEN_ENV} to the token in {}",
            path.display()
        )
    })?;

    if !constant_time_eq(expected.trim(), provided.trim()) {
        return Err("MCP stdio auth: invalid DATAZEN_MCP_TOKEN".into());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_lock() -> MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn load_or_create_token_persists_and_reloads() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let (first, created) = load_or_create_token(dir.path()).unwrap();
        assert!(created);
        assert!(!first.is_empty());
        assert!(token_file_path(dir.path()).exists());

        let (second, created_again) = load_or_create_token(dir.path()).unwrap();
        assert!(!created_again);
        assert_eq!(first, second);
    }

    #[test]
    fn verify_stdio_token_requires_env_after_bootstrap() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        let (token, _) = load_or_create_token(dir.path()).unwrap();

        std::env::remove_var(MCP_TOKEN_ENV);
        assert!(verify_stdio_token(dir.path()).is_err());

        std::env::set_var(MCP_TOKEN_ENV, "wrong-token");
        assert!(verify_stdio_token(dir.path()).is_err());

        std::env::set_var(MCP_TOKEN_ENV, &token);
        assert!(verify_stdio_token(dir.path()).is_ok());

        std::env::remove_var(MCP_TOKEN_ENV);
    }

    #[test]
    fn verify_stdio_token_rejects_first_run_without_env() {
        let _guard = env_lock();
        let dir = tempfile::tempdir().unwrap();
        std::env::remove_var(MCP_TOKEN_ENV);
        let err = verify_stdio_token(dir.path()).unwrap_err();
        assert!(err.contains("Created MCP auth token"));
        assert!(token_file_path(dir.path()).exists());
    }
}
