//! Tests for the connection import / export IPC surface.

use super::super::*;
use super::*;
use std::path::Path;
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
    use super::super::{decrypt_datazen_fields, derive_argon2_key, encrypt_field};
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
    use super::super::{decrypt_datazen_fields, derive_argon2_key, encrypt_field};
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
        ImportFormat::DataZen,
        ".datazenconnection must be labeled DataZen even though the cipher matches TablePlus"
    );
    assert_eq!(parsed_tableplus.format, ImportFormat::TablePlus);
    assert_eq!(parsed.connections.len(), 1);
    assert_eq!(parsed.connections[0].name, "Demo");
    assert_eq!(parsed.connections[0].password.as_deref(), Some("pw"));
    assert_eq!(parsed.connections[0].group.as_deref(), Some("Prod"));
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

    let groups = crate::commands::config::get_groups_impl(&test.state)
        .await
        .unwrap();
    assert!(groups.contains(&"Alpha".into()));
    assert!(groups.contains(&"Beta".into()));
    assert!(groups.contains(&"Gamma".into()));
    assert_eq!(test.store.get_connections().await.len(), 2);
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
