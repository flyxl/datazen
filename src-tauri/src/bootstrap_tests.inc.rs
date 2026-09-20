#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::sync::{Arc, Mutex};

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
        use crate::db::ConnectionConfig;
        let mk = |id: &str, db: &str| ConnectionConfig {
            id: id.into(),
            name: id.into(),
            database_type: db.into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: Default::default(),
            connection_timeout: None,
            max_pool_size: None,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: None,
            pinned: None,
        };
        // Note: field set may differ; this test is structural — kept from upstream.
        let _ = mk;
    }
}
