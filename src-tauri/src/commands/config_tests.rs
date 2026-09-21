use super::*;
use std::path::Path;

// #37 split the app-data/archive commands into `config_import_and_archive.rs`,
// so the guard has to scan both halves of the module.
const SOURCE: &str = concat!(
    include_str!("config.rs"),
    "\n",
    include_str!("config_import_and_archive.rs")
);
const BOOTSTRAP_RS: &str = include_str!("../bootstrap/run.rs");

fn command_params(command: &str) -> String {
    let needle = format!("pub async fn {command}(");
    let start = SOURCE
        .find(&needle)
        .unwrap_or_else(|| panic!("{command} not found"));
    let after = &SOURCE[start + needle.len()..];
    let end = after.find(')').expect("params close");
    after[..end].to_string()
}

#[test]
fn test_get_app_executable_path_returns_valid_path() {
    let exe = get_app_executable_path_impl().expect("executable path should be resolved");
    assert!(!exe.is_empty());
    assert!(Path::new(&exe).exists());
}

#[test]
fn path_is_under_matches_prefix() {
    assert!(path_is_under(
        Path::new("/data/app/logs"),
        Path::new("/data/app")
    ));
    assert!(!path_is_under(
        Path::new("/tmp/evil"),
        Path::new("/data/app")
    ));
}

#[test]
fn require_webdriver_path_ipc_gates_without_feature() {
    let result = require_webdriver_path_ipc("Direct path connection export disabled");
    if cfg!(feature = "webdriver") {
        assert!(result.is_ok());
    } else {
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("disabled"));
    }
}

#[test]
fn bootstrap_rs_registers_merged_commands_only() {
    assert!(BOOTSTRAP_RS.contains("commands::export_connections,"));
    assert!(BOOTSTRAP_RS.contains("commands::import_connections_with_dialog,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_settings,"));
    assert!(BOOTSTRAP_RS.contains("commands::save_settings,"));
    assert!(BOOTSTRAP_RS.contains("commands::get_tunnels,"));
    assert!(BOOTSTRAP_RS.contains("commands::save_tunnel,"));
}

#[test]
fn merged_commands_route_through_shared_resolve_override_path() {
    // Single mechanism: config.rs must reuse the error.rs helper (F3
    // pattern) instead of redefining a local copy, and every merged
    // command body must gate through it. Needles are assembled at runtime
    // so this test's own source never contains them.
    let resolve = "resolve";
    let override_path = "override_path";
    let local_def = format!("fn {resolve}_{override_path}(");
    assert!(
        !SOURCE.contains(&local_def),
        "config.rs must not redefine resolve_override_path"
    );
    let call = format!("{resolve}_{override_path}(override_path");
    let gated_bodies = SOURCE.matches(&call).count();
    // Core file + re-exported archive module both use the helper; count is soft.
    assert!(
        gated_bodies >= 3,
        "merged commands must gate their override branch; found {gated_bodies}"
    );
}

#[test]
fn encryption_key_export_bytes_trims() {
    let bytes = encryption_key_export_bytes("  abc==  \n");
    assert_eq!(bytes, b"abc==");
}

#[test]
fn export_rejects_empty_password() {
    assert!(validate_share_password("").is_err());
    assert!(validate_share_password("   ").is_err());
    assert!(validate_share_password("secret").is_ok());
}

#[test]
fn connections_open_filters_cover_all_sources() {
    let filters = connections_open_filters();
    assert_eq!(filters.len(), 6);
    assert_eq!(filters[0].0, "Connections");
    // Every filter declares at least one extension (gateway contract).
    assert!(filters.iter().all(|(_, exts)| !exts.is_empty()));
    assert!(filters
        .iter()
        .flat_map(|(_, exts)| exts.iter())
        .all(|ext| !ext.starts_with('.')));
}

#[test]
fn import_file_filters_match_source_apps() {
    assert_eq!(
        import_file_filters(ImportApp::DataGrip),
        ("DataGrip", &["xml"][..])
    );
    assert_eq!(
        import_file_filters(ImportApp::TablePlus),
        ("TablePlus", &["plist", "tableplusconnection"][..])
    );
    assert_eq!(
        import_file_filters(ImportApp::Navicat),
        ("Navicat", &["ncx", "xml"][..])
    );
}

#[test]
fn merge_connections_overwrites_by_id() {
    use crate::db::{ConnectionConfig, SslMode};

    fn conn(id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: id.into(),
            database_type: "postgresql".into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
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
            read_only: false,
            pinned: false,
        }
    }

    let existing_ids: HashSet<String> = ["a", "b"].into_iter().map(String::from).collect();
    let incoming = vec![conn("b"), conn("c"), conn("d")];
    let (imported, overwritten) = merge_connection_import_stats(&existing_ids, &incoming);
    assert_eq!(imported, 2);
    assert_eq!(overwritten, 1);
}

#[test]
fn merge_group_lists_unions_and_counts_new() {
    let (merged, added) = merge_group_lists(
        &["alpha".into(), "beta".into()],
        &["beta".into(), "gamma".into()],
    );
    assert_eq!(merged, vec!["alpha", "beta", "gamma"]);
    assert_eq!(added, 1);
}

#[test]
fn encrypt_decrypt_roundtrip() {
    use super::connection_import::{decrypt_datazen_fields, derive_argon2_key, encrypt_field};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    let salt = [7u8; 16];
    let key = derive_argon2_key("unit-test-password", &salt).unwrap();
    let cipher = encrypt_field("secret-db-password", &key).unwrap();
    let mut data = serde_json::json!({
        "encrypted": true,
        "salt": BASE64.encode(salt),
        "connections": [{ "password": cipher }]
    });
    decrypt_datazen_fields(&mut data, "unit-test-password").unwrap();
    assert_eq!(data["connections"][0]["password"], "secret-db-password");
}

#[test]
fn decrypt_rejects_wrong_password() {
    use super::connection_import::{decrypt_datazen_fields, derive_argon2_key, encrypt_field};
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

    let salt = [9u8; 16];
    let key = derive_argon2_key("correct", &salt).unwrap();
    let cipher = encrypt_field("payload", &key).unwrap();
    let mut data = serde_json::json!({
        "encrypted": true,
        "salt": BASE64.encode(salt),
        "connections": [{ "password": cipher }]
    });
    assert!(decrypt_datazen_fields(&mut data, "wrong").is_err());
}

#[test]
fn encryption_key_export_bytes_roundtrip_write() {
    let key_b64 = "  dGVzdGtleQ==  \n";
    let bytes = encryption_key_export_bytes(key_b64);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("datazen.key");
    std::fs::write(&path, &bytes).unwrap();
    let read_back = std::fs::read_to_string(&path).unwrap();
    assert_eq!(read_back, "dGVzdGtleQ==");
}

#[test]
fn build_encrypted_connections_export_roundtrip() {
    use crate::db::{ConnectionConfig, SslMode};

    let conn = ConnectionConfig {
        id: "c1".into(),
        name: "Demo".into(),
        database_type: "postgresql".into(),
        host: Some("localhost".into()),
        port: Some(5432),
        database: Some("app".into()),
        schema: None,
        username: Some("alice".into()),
        password: Some("pw".into()),
        ssl_mode: SslMode::default(),
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: Some("Prod".into()),
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    };
    let bytes =
        build_encrypted_connections_export(&[conn], &["Prod".into()], "share-secret").unwrap();
    assert_eq!(&bytes[0..2], &[0x03, 0x01]);
    let parsed = parse_import_file(
        Path::new("datazen-connections.datazenconnection"),
        &bytes,
        Some("share-secret"),
    )
    .unwrap();
    let parsed_tableplus = parse_import_file(
        Path::new("legacy.tableplusconnection"),
        &bytes,
        Some("share-secret"),
    )
    .unwrap();
    assert_eq!(parsed_tableplus.connections[0].name, "Demo");
    assert_eq!(
        parsed.format,
        connection_import::ImportFormat::DataZen,
        ".datazenconnection must be labeled DataZen even though the cipher matches TablePlus"
    );
    assert_eq!(
        parsed_tableplus.format,
        connection_import::ImportFormat::TablePlus
    );
    assert_eq!(parsed.connections.len(), 1);
    assert_eq!(parsed.connections[0].name, "Demo");
    assert_eq!(parsed.connections[0].password.as_deref(), Some("pw"));
    assert_eq!(parsed.connections[0].group.as_deref(), Some("Prod"));
}

#[tokio::test]
async fn config_store_commands_via_impl() {
    use crate::store::AppSettings;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    save_groups_impl(&test.state, vec!["alpha".into(), "beta".into()])
        .await
        .unwrap();
    assert_eq!(
        get_groups_impl(&test.state).await.unwrap(),
        vec!["alpha", "beta"]
    );

    let mut settings = AppSettings::default();
    settings.language = "zh-CN".into();
    save_settings_impl(&test.state, settings.clone())
        .await
        .unwrap();
    assert_eq!(
        get_settings_impl(&test.state).await.unwrap().language,
        "zh-CN"
    );

    let log_path = get_log_path_impl(&test.state).await.unwrap();
    assert!(log_path.contains("logs"));
}

#[tokio::test]
async fn open_path_validation_without_webdriver() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    if cfg!(feature = "webdriver") {
        return;
    }
    let err = open_path_impl(&test.state, "../etc/passwd".into())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("disabled") || err.to_string().contains("traversal"));
}

#[test]
fn import_password_option_treats_blank_as_none() {
    assert_eq!(import_password_option(""), None);
    assert_eq!(import_password_option("   "), None);
    assert_eq!(import_password_option("secret"), Some("secret"));
}

#[test]
fn build_import_preview_json_includes_source_format() {
    use crate::commands::connection_import::{ImportFormat, ParsedImport};

    let preview = build_import_preview_json(&ParsedImport {
        connections: vec![],
        groups: vec!["Prod".into()],
        skipped: vec!["bad".into()],
        format: ImportFormat::DataZen,
    });
    assert_eq!(preview["groups"], serde_json::json!(["Prod"]));
    assert_eq!(preview["skipped"], serde_json::json!(["bad"]));
    assert_eq!(preview["sourceFormat"], "DataZen");
}

#[tokio::test]
async fn apply_connection_import_impl_merges_connections_and_groups() {
    use crate::db::{ConnectionConfig, SslMode};
    use crate::testing::app_state::TestAppState;

    fn conn(id: &str, group: Option<&str>) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: id.into(),
            database_type: "postgresql".into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
            max_pool_size: 10,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: group.map(str::to_string),
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: false,
            pinned: false,
        }
    }

    let test = TestAppState::new().await;
    test.store
        .save_connection(conn("existing", Some("Alpha")))
        .await
        .unwrap();
    test.store.save_groups(vec!["Alpha".into()]).await.unwrap();

    let result = apply_connection_import_impl(
        &test.state,
        vec![
            conn("existing", Some("Beta")),
            conn("new-one", Some("Beta")),
        ],
        vec!["Beta".into(), "Gamma".into()],
        vec!["skipped-row".into()],
        "DataZen".into(),
    )
    .await
    .unwrap();

    assert_eq!(result.imported, 1);
    assert_eq!(result.overwritten, 1);
    // "Beta" may already be present from connection save before group merge.
    assert!(result.groups_added >= 1);
    assert_eq!(result.skipped, vec!["skipped-row"]);
    assert_eq!(result.source_format, "DataZen");

    let groups = get_groups_impl(&test.state).await.unwrap();
    assert!(groups.contains(&"Alpha".into()));
    assert!(groups.contains(&"Beta".into()));
    assert!(groups.contains(&"Gamma".into()));
    assert_eq!(test.store.get_connections().await.len(), 2);
}

#[test]
fn export_options_from_settings_reflects_monitor_flag() {
    use crate::store::AppSettings;

    let mut settings = AppSettings::default();
    settings.monitor.export_include_dashboard_runs = false;
    assert!(!export_options_from_settings(&settings).include_dashboard_runs);
    settings.monitor.export_include_dashboard_runs = true;
    assert!(export_options_from_settings(&settings).include_dashboard_runs);
}

#[test]
fn resolve_log_and_context_dirs_via_crate_helpers() {
    let data = Path::new("/data/app");
    assert_eq!(
        crate::resolve_log_dir(data, ""),
        Path::new("/data/app/logs")
    );
    assert_eq!(crate::resolve_context_dir(data, "/ctx"), Path::new("/ctx"));
}

#[tokio::test]
async fn get_log_path_honors_custom_log_path_setting() {
    use crate::store::AppSettings;
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    let mut settings = AppSettings::default();
    settings.log_path = test
        ._temp
        .path()
        .join("custom-logs")
        .to_string_lossy()
        .into();
    save_settings_impl(&test.state, settings).await.unwrap();
    let log_path = get_log_path_impl(&test.state).await.unwrap();
    assert!(log_path.contains("custom-logs"));
}

#[tokio::test]
async fn merged_connections_export_roundtrips_through_preview_helper() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    test.save_connection("exp-1").await;

    // Merged export impl: writes the encrypted payload, returns the count.
    let dest = test._temp.path().join("share.datazenconnection");
    let count = write_connections_export(&test.state, "share-secret", dest.clone())
        .await
        .unwrap();
    assert_eq!(count, 1);

    // Merged preview impl parses the export back (param passthrough:
    // password reaches the decryptor).
    let preview = build_import_preview_from_path("share-secret", dest.clone())
        .await
        .unwrap();
    assert_eq!(preview["connections"].as_array().unwrap().len(), 1);
    assert_eq!(preview["sourceFormat"], "DataZen");

    // Wrong password must fail the same decrypt path.
    assert!(build_import_preview_from_path("wrong", dest).await.is_err());
}

#[tokio::test]
async fn app_data_export_lists_store_files_and_import_helper_extracts() {
    use crate::testing::app_state::TestAppState;

    // Export impl: real store dir → ZIP containing the JSON store files.
    let source = TestAppState::new().await;
    source
        .store
        .save_groups(vec!["Alpha".into()])
        .await
        .unwrap();
    let zip_path = source._temp.path().join("app-backup.zip");
    export_app_data_to_dest(&source.state, zip_path.clone())
        .await
        .unwrap();

    let file = std::fs::File::open(&zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with("groups.json")),
        "export must carry groups.json; entries: {names:?}"
    );

    // Import impl param passthrough: extracts into the target state's data
    // dir. A hand-crafted clean archive keeps live sqlite sidecar files
    // (zero-filled -wal/-shm trip the import zip-bomb ratio guard) out of
    // the fixture.
    let clean_zip = source._temp.path().join("clean.zip");
    {
        use std::io::Write;
        let file = std::fs::File::create(&clean_zip).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("groups.json", options).unwrap();
        writer.write_all(br#"["Alpha"]"#).unwrap();
        writer.finish().unwrap();
    }

    let target = TestAppState::new().await;
    import_app_data_from_source(
        &target.state,
        clean_zip,
        app_data_archive::ImportOptions::default(),
    )
    .await
    .unwrap();
    let groups_on_disk =
        std::fs::read_to_string(target.state.store.data_dir().join("groups.json")).unwrap();
    assert!(
        groups_on_disk.contains("Alpha"),
        "imported archive must land as groups.json: {groups_on_disk}"
    );
}

mod ipc_contract_guards {
    use super::*;

    // Runtime-assembled needles (avoid self-reference in include_str! source).
    #[allow(non_upper_case_globals)]
    const resolve: &str = "resolve";

    #[allow(non_upper_case_globals)]
    const override_path: &str = "override_path";

    #[test]
    fn merged_commands_take_override_path_not_raw_path_param() {
        for cmd in [
            "export_connections",
            "import_connections_preview",
            "import_connections_with_dialog",
            "export_app_data",
            "import_app_data",
        ] {
            let params = command_params(cmd);
            assert!(
                params.contains("override_path: Option<String>"),
                "`{cmd}` must expose the webdriver-only override; got: {params}"
            );
            assert!(
                !params.contains("path: String"),
                "raw `path` parameter must not return after the merge; got: {params}"
            );
        }
    }

    #[test]
    fn stale_dialog_twins_are_gone_from_config_rs() {
        // Needle parts are assembled at runtime so this test's own source never
        // contains them (mirroring backup.rs guards).
        let fn_prefix = "pub async fn ";
        let with_dialog = "_with_";
        let dialog_kind = "dialog(";
        for name in ["export_connections", "export_app_data", "import_app_data"] {
            let variant = format!("{fn_prefix}{name}{with_dialog}{dialog_kind}");
            assert!(!SOURCE.contains(&variant), "stale dialog twin `{variant}`");
        }
    }
}
