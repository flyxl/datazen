use super::*;
use chrono::Utc;
use crate::db::{ConnectionConfig, SslMode, SshTunnelConfig};
use settings::deserialize_theme;


fn use_file_key_backend() {
        std::env::set_var("DATAZEN_KEYRING", "file");
}

async fn init_store_for_test(dir: &std::path::Path) -> Store {
        use_file_key_backend();
        Store::init_with_path(dir).await.unwrap()
}

#[tokio::test]
async fn file_backend_creates_and_reloads_key() {
        use_file_key_backend();
        let dir = tempfile::tempdir().unwrap();
        let k1 = Store::get_or_create_encryption_key_for_test(dir.path())
            .await
            .unwrap();
        let k2 = Store::get_or_create_encryption_key_for_test(dir.path())
            .await
            .unwrap();
        assert_eq!(k1, k2);
        assert!(key_store::key_file_path(dir.path()).is_file());
}

#[tokio::test]
#[ignore = "requires OS keychain; run with: cargo test migrates_dot_key -- --ignored"]
async fn migrates_dot_key_into_keyring_and_deletes_file() {
        std::env::remove_var("DATAZEN_KEYRING");
        if !key_store::keyring_is_available() {
            eprintln!("skip: OS keychain unavailable");
            return;
        }
        key_store::delete_keyring_entry_for_test();

        let dir = tempfile::tempdir().unwrap();
        let known_key = [7u8; 32];
        std::fs::write(
            key_store::key_file_path(dir.path()),
            BASE64.encode(known_key),
        )
        .unwrap();

        let loaded = key_store::load_or_create_master_key(dir.path()).unwrap();
        assert_eq!(loaded, known_key);
        assert!(!key_store::key_file_path(dir.path()).exists());

        key_store::delete_keyring_entry_for_test();
}

fn sample_connection_with_ssh() -> ConnectionConfig {
        ConnectionConfig {
            id: "test-ssh-1".into(),
            name: "SSH Test".into(),
            database_type: "postgresql".into(),
            host: Some("localhost".into()),
            port: Some(5432),
            database: Some("mydb".into()),
            schema: None,
            username: Some("dbuser".into()),
            password: Some("db-secret".into()),
            ssl_mode: SslMode::Disable,
            connection_timeout: 30,
            ssh_tunnel: Some(SshTunnelConfig {
                enabled: true,
                host: "jump.example.com".into(),
                port: 22,
                username: "sshuser".into(),
                auth_method: "password".into(),
                password: Some("ssh-secret-password".into()),
                private_key_path: None,
                passphrase: Some("key-passphrase".into()),
            }),
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
        }
}

#[tokio::test]
async fn ssh_credentials_encrypted_on_disk_and_decrypted_in_memory() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;

        store
            .save_connection(sample_connection_with_ssh())
            .await
            .unwrap();

        let disk_content =
            tokio::fs::read_to_string(dir.path().join("connections.json"))
                .await
                .unwrap();
        assert!(
            !disk_content.contains("ssh-secret-password"),
            "SSH password must not appear plaintext on disk"
        );
        assert!(
            !disk_content.contains("key-passphrase"),
            "SSH passphrase must not appear plaintext on disk"
        );

        let loaded = store.get_connections().await;
        assert_eq!(loaded.len(), 1);
        let ssh = loaded[0].ssh_tunnel.as_ref().unwrap();
        assert_eq!(ssh.password.as_deref(), Some("ssh-secret-password"));
        assert_eq!(ssh.passphrase.as_deref(), Some("key-passphrase"));
}

#[tokio::test]
async fn connection_options_persist_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;

        let mut conn = sample_connection_with_ssh();
        let mut opts = serde_json::Map::new();
        opts.insert("topology".into(), serde_json::json!("cluster"));
        conn.options = Some(opts);
        store.save_connection(conn).await.unwrap();

        let loaded = store.get_connections().await;
        assert_eq!(loaded.len(), 1);
        assert_eq!(
            loaded[0].options.as_ref().unwrap()["topology"],
            serde_json::json!("cluster")
        );

        let store2 = init_store_for_test(dir.path()).await;
        let reloaded = store2.get_connections().await;
        assert_eq!(
            reloaded[0].options.as_ref().unwrap()["topology"],
            serde_json::json!("cluster")
        );
}

#[tokio::test]
async fn ssh_credentials_roundtrip_after_reload() {
        let dir = tempfile::tempdir().unwrap();

        {
            let store = init_store_for_test(dir.path()).await;
            store
                .save_connection(sample_connection_with_ssh())
                .await
                .unwrap();
        }

        let store = init_store_for_test(dir.path()).await;
        let loaded = store.get_connections().await;
        assert_eq!(loaded.len(), 1);
        let ssh = loaded[0].ssh_tunnel.as_ref().unwrap();
        assert_eq!(ssh.password.as_deref(), Some("ssh-secret-password"));
        assert_eq!(ssh.passphrase.as_deref(), Some("key-passphrase"));
}

#[test]
fn default_app_data_dir_uses_bundle_identifier() {
        let dir = Store::default_app_data_dir().unwrap();
        assert!(
            dir.ends_with(APP_IDENTIFIER),
            "expected path ending with {APP_IDENTIFIER}, got {}",
            dir.display()
        );
}

#[test]
fn default_app_data_dir_matches_resolve_log_settings_path() {
        let store_dir = Store::default_app_data_dir().unwrap();
        let log_dir = dirs::data_dir()
            .map(|d| d.join(APP_IDENTIFIER))
            .expect("data dir");
        assert_eq!(store_dir, log_dir);
}

#[test]
fn theme_deserializes_legacy_string_and_object() {
        #[derive(Deserialize)]
        struct ThemeField {
            #[serde(deserialize_with = "deserialize_theme", default)]
            theme: ThemePreference,
        }

        let legacy: ThemeField = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(
            legacy.theme,
            ThemePreference {
                mode: "dark".into(),
                pack_id: None,
            }
        );

        let nested: ThemeField =
            serde_json::from_str(r#"{"theme":{"mode":"dark","packId":null}}"#).unwrap();
        assert_eq!(
            nested.theme,
            ThemePreference {
                mode: "dark".into(),
                pack_id: None,
            }
        );
}

#[test]
fn default_language_is_english() {
        assert_eq!(AppSettings::default().language, "en");
}

#[test]
fn first_run_language_is_supported() {
        let settings = AppSettings::default_for_first_run();
        const OK: &[&str] = &[
            "en", "zh-CN", "zh-TW", "es", "fr", "de", "ja", "pt-BR", "ru", "ko",
        ];
        assert!(
            OK.contains(&settings.language.as_str()),
            "unexpected {}",
            settings.language
        );
}

#[test]
fn plugin_settings_defaults_when_key_missing() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .remove("pluginSettings");
        let parsed: AppSettings = serde_json::from_value(value).unwrap();
        assert!(parsed.plugin_settings.is_empty());
}

#[test]
fn plugin_settings_roundtrip_opaque() {
        let settings = AppSettings {
            plugin_settings: {
                let mut m = serde_json::Map::new();
                m.insert("redis".into(), serde_json::json!({ "allowFlush": true }));
                m
            },
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("pluginSettings"));
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.plugin_settings.get("redis").unwrap()["allowFlush"], true);
}

#[test]
fn settings_json_roundtrip_preserves_language() {
        let settings = AppSettings {
            language: "de".into(),
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.language, "de");
}

#[test]
fn monitor_settings_json_roundtrip() {
        use crate::dashboard::types::MonitorSettings;

        let settings = AppSettings {
            monitor: MonitorSettings {
                max_concurrent_queries: 4,
                run_retention_count: 100,
                tray_enabled: false,
                ..MonitorSettings::default()
            },
            ..AppSettings::default()
        };
        let json = serde_json::to_string(&settings).unwrap();
        let parsed: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.monitor.max_concurrent_queries, 4);
        assert_eq!(parsed.monitor.run_retention_count, 100);
        assert!(!parsed.monitor.tray_enabled);
        assert!(parsed.monitor.close_to_tray);
}

#[tokio::test]
async fn save_json_file_leaves_no_tmp_artifacts() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let settings = AppSettings {
            language: "fr".into(),
            ..AppSettings::default()
        };
        store.save_settings(settings).await.unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(!entries.iter().any(|n| n.contains(".tmp")));
        let content = tokio::fs::read_to_string(dir.path().join("settings.json"))
            .await
            .unwrap();
        let parsed: AppSettings = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed.language, "fr");
}

#[tokio::test]
async fn save_ai_config_uses_atomic_encrypted_write() {
        use datazen_ai_api::{AiProviderConfig, AiProviderType};

        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let config = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: Some("sk-test".into()),
            endpoint: None,
            model: "gpt-4o".into(),
            max_tokens: 200_000,
            extra: serde_json::Value::Null,
        };
        store.save_ai_config(&config).await.unwrap();

        let entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        assert!(entries.contains(&"ai_config.enc".to_string()));
        assert!(!entries.iter().any(|n| n.contains(".tmp")));
        let loaded = store.get_ai_config().await.unwrap();
        assert_eq!(loaded.model, "gpt-4o");
}

fn sample_history_entry(sql: &str) -> QueryHistoryEntry {
        QueryHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            connection_id: "conn-1".into(),
            database: "db".into(),
            sql: sql.into(),
            executed_at: Utc::now(),
            execution_time_ms: 10,
            rows_affected: Some(1),
            success: true,
            error_message: None,
        }
}

#[tokio::test]
async fn store_data_dir_and_encryption_key_b64() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        assert_eq!(store.data_dir(), &dir.path().to_path_buf());
        let b64 = store.encryption_key_b64();
        assert!(!b64.is_empty());
        let roundtrip = store.decrypt_password(&store.encrypt("secret").unwrap()).unwrap();
        assert_eq!(roundtrip, "secret");
}

#[tokio::test]
async fn connection_crud_and_groups() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;

        let mut conn = sample_connection_with_ssh();
        conn.id = "c1".into();
        conn.group = Some("prod".into());
        store.save_connection(conn.clone()).await.unwrap();
        assert_eq!(store.get_connections().await.len(), 1);
        assert_eq!(store.get_connection("c1").await.unwrap().name, "SSH Test");
        assert!(store.get_connection("missing").await.is_none());

        conn.name = "Updated".into();
        store.save_connection(conn).await.unwrap();
        assert_eq!(store.get_connection("c1").await.unwrap().name, "Updated");

        store.save_groups(vec!["dev".into()]).await.unwrap();
        let groups = store.get_groups().await;
        assert!(groups.contains(&"dev".to_string()));
        assert!(groups.contains(&"prod".to_string()));

        store.delete_connection("c1").await.unwrap();
        assert!(store.get_connections().await.is_empty());
}

#[tokio::test]
async fn groups_merge_connection_groups_without_groups_file() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let mut conn = sample_connection_with_ssh();
        conn.id = "g1".into();
        conn.group = Some("from-conn".into());
        store.save_connection(conn).await.unwrap();
        let groups = store.get_groups().await;
        assert_eq!(groups, vec!["from-conn".to_string()]);
}

#[tokio::test]
async fn query_history_dedup_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;

        store
            .add_query_history(sample_history_entry("SELECT 1"))
            .await
            .unwrap();
        store
            .add_query_history(sample_history_entry("SELECT 2"))
            .await
            .unwrap();
        assert_eq!(store.get_query_history(10).await.len(), 2);

        // Dedup only applies when SQL matches the most recent entry.
        let mut dup = sample_history_entry("SELECT 2");
        dup.execution_time_ms = 99;
        store.add_query_history(dup).await.unwrap();
        let history = store.get_query_history(10).await;
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].execution_time_ms, 99);

        store.clear_query_history().await.unwrap();
        assert!(store.get_query_history(10).await.is_empty());
}

#[tokio::test]
async fn favorite_queries_crud() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;

        let fav = FavoriteQuery {
            id: "f1".into(),
            title: "Users".into(),
            sql: "SELECT * FROM users".into(),
            created_at: Utc::now(),
        };
        store.add_favorite_query(fav).await.unwrap();
        assert_eq!(store.get_favorite_queries().await.len(), 1);

        store.delete_favorite_query("f1").await.unwrap();
        assert!(store.get_favorite_queries().await.is_empty());
}

#[tokio::test]
async fn sync_tasks_crud() {
        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let now = Utc::now();
        let task = SyncTask {
            id: "t1".into(),
            source_connection_id: "s".into(),
            target_connection_id: "t".into(),
            source_config_id: "sc".into(),
            target_config_id: "tc".into(),
            tables: vec!["users".into()],
            completed_tables: vec![],
            current_table: None,
            current_table_offset: 0,
            source_row_counts: Default::default(),
            strategy: "full".into(),
            status: "running".into(),
            error_message: None,
            created_at: now,
            updated_at: now,
        };
        store.save_sync_task(task.clone()).await.unwrap();
        assert_eq!(store.get_sync_tasks().await.len(), 1);

        let mut updated = task;
        updated.status = "completed".into();
        store.save_sync_task(updated).await.unwrap();
        assert_eq!(store.get_sync_tasks().await[0].status, "completed");

        store.delete_sync_task("t1").await.unwrap();
        assert!(store.get_sync_tasks().await.is_empty());
}

#[tokio::test]
async fn delete_ai_config_removes_file() {
        use datazen_ai_api::{AiProviderConfig, AiProviderType};

        let dir = tempfile::tempdir().unwrap();
        let store = init_store_for_test(dir.path()).await;
        let config = AiProviderConfig {
            provider_type: AiProviderType::OpenAi,
            api_key: Some("sk-x".into()),
            endpoint: None,
            model: "gpt-4o".into(),
            max_tokens: 1000,
            extra: serde_json::Value::Null,
        };
        store.save_ai_config(&config).await.unwrap();
        assert!(store.get_ai_config().await.is_some());

        store.delete_ai_config().await.unwrap();
        assert!(store.get_ai_config().await.is_none());
        assert!(!dir.path().join("ai_config.enc").exists());
}

#[tokio::test]
async fn reload_clears_bad_encrypted_password() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = init_store_for_test(dir.path()).await;
            store.save_connection(sample_connection_with_ssh()).await.unwrap();
        }

        let mut raw: serde_json::Value =
            serde_json::from_str(&tokio::fs::read_to_string(dir.path().join("connections.json")).await.unwrap()).unwrap();
        raw[0]["password"] = serde_json::json!("not-valid-ciphertext");
        tokio::fs::write(
            dir.path().join("connections.json"),
            serde_json::to_string_pretty(&raw).unwrap(),
        )
        .await
        .unwrap();

        let store = init_store_for_test(dir.path()).await;
        let conn = store.get_connections().await.into_iter().next().unwrap();
        assert!(conn.password.is_none());
}

#[tokio::test]
async fn reload_clears_bad_encrypted_ssh_password() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = init_store_for_test(dir.path()).await;
            store.save_connection(sample_connection_with_ssh()).await.unwrap();
        }

        let mut raw: serde_json::Value =
            serde_json::from_str(&tokio::fs::read_to_string(dir.path().join("connections.json")).await.unwrap()).unwrap();
        raw[0]["sshTunnel"]["password"] = serde_json::json!("not-valid-ciphertext");
        tokio::fs::write(
            dir.path().join("connections.json"),
            serde_json::to_string_pretty(&raw).unwrap(),
        )
        .await
        .unwrap();

        let store = init_store_for_test(dir.path()).await;
        let conn = store.get_connections().await.into_iter().next().unwrap();
        let ssh = conn.ssh_tunnel.as_ref().unwrap();
        assert!(ssh.password.is_none());
}

#[test]
fn theme_deserializes_light_and_system_strings() {
        #[derive(Deserialize)]
        struct ThemeField {
            #[serde(deserialize_with = "deserialize_theme", default)]
            theme: ThemePreference,
        }

        for mode in ["light", "system"] {
            let legacy: ThemeField =
                serde_json::from_str(&format!(r#"{{"theme":"{mode}"}}"#)).unwrap();
            assert_eq!(legacy.theme.mode, mode);
            assert!(legacy.theme.pack_id.is_none());
        }
}

#[test]
fn decrypt_rejects_short_payload() {
        use_file_key_backend();
        let dir = tempfile::tempdir().unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        let store = rt.block_on(Store::init_with_path(dir.path())).unwrap();
        let err = store.decrypt_password("AAAA").unwrap_err();
        assert!(matches!(err, StoreError::EncryptionError(_)));
}

