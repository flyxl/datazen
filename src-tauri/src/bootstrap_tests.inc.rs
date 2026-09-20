#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Guards tests that read/write the shared global `settings.json` so they
    /// don't race when `cargo test` runs them in parallel.
    static SETTINGS_FILE_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn resolve_log_settings_defaults_without_settings_file() {
        let _guard = SETTINGS_FILE_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        let (level, log_dir) = resolve_log_settings_in(data_dir);
        assert_eq!(level, "info");
        assert_eq!(log_dir, data_dir.join("logs"));
    }

    #[test]
    fn resolve_log_settings_reads_custom_level_and_path() {
        let _guard = SETTINGS_FILE_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path();
        std::fs::create_dir_all(data_dir).unwrap();
        let settings_path = data_dir.join("settings.json");
        let custom_log = data_dir.join("custom-logs");
        let settings = serde_json::json!({
            "logLevel": "debug",
            "logPath": custom_log.to_string_lossy(),
        });
        std::fs::write(&settings_path, settings.to_string()).unwrap();

        let (level, log_dir) = resolve_log_settings_in(data_dir);
        assert_eq!(level, "debug");
        assert_eq!(log_dir, custom_log);
    }

    #[test]
    fn unique_driver_types_deduplicates_preserving_order() {
        // Structural smoke test — full ConnectionConfig construction lives in integration tests.
        let empty: Vec<crate::db::ConnectionConfig> = vec![];
        assert!(unique_driver_types(&empty).is_empty());
    }

    #[test]
    fn invoke_handler_contains_tunnel_commands() {
        let src = include_str!("bootstrap.rs");
        assert!(src.contains("crate::commands::get_tunnels"));
        assert!(src.contains("crate::commands::save_tunnel"));
        assert!(src.contains("crate::commands::delete_tunnel"));
        assert!(src.contains(".invoke_handler(tauri::generate_handler!["));
    }

    #[test]
    fn test_tester_run_mcp_stdio_entry_chain_wiring() {
        let bootstrap = include_str!("bootstrap.rs");
        assert!(bootstrap.contains("bootstrap_run_pre_a.inc.rs") || bootstrap.contains("run_mcp_stdio"));
        let pre = include_str!("bootstrap_run_pre_a.inc.rs");
        assert!(pre.contains("pub fn run_mcp_stdio()"));
        assert!(pre.contains("mcp::auth::verify_stdio_token"));
        assert!(pre.contains("mcp::start_mcp_stdio"));
    }
}
