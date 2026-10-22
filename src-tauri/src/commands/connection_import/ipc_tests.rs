//! Tests for the connection import / export IPC surface.

use super::super::*;
use super::*;
use datazen_feature_core::connection_import::ImportFormat;
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
    use datazen_driver_api::{ConnectionConfig, SslMode};

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
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use datazen_feature_core::connection_import::{
        decrypt_datazen_fields, derive_argon2_key, encrypt_field,
    };

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
    use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
    use datazen_feature_core::connection_import::{
        decrypt_datazen_fields, derive_argon2_key, encrypt_field,
    };

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
    use datazen_driver_api::{ConnectionConfig, SslMode};

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
    use datazen_feature_core::connection_import::{ImportFormat, ParsedImport};

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
    use crate::testing::app_state::TestAppState;
    use datazen_driver_api::{ConnectionConfig, SslMode};

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

/// G7: a connection that references a saved SSH tunnel must export as
/// `isOverSSH=true` with the jump host materialized, even though it carries no
/// inline `ssh_tunnel` locally.
#[tokio::test]
async fn export_materializes_referenced_ssh_tunnel() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{SavedTunnel, SshTunnelConfig, TunnelKind};

    let test = TestAppState::new().await;
    test.store
        .save_tunnel(SavedTunnel {
            id: "tunnel-ssh".into(),
            name: "Bastion".into(),
            kind: TunnelKind::Ssh,
            ssh: Some(SshTunnelConfig {
                enabled: true,
                host: "bastion.internal".into(),
                port: 2222,
                username: "ubuntu".into(),
                auth_method: "password".into(),
                password: Some("ssh-secret".into()),
                private_key_path: None,
                passphrase: None,
                jump: None,
            }),
            http_proxy: None,
            websocket: None,
        })
        .await
        .unwrap();

    let mut conn = sample_postgres_config("ref-1");
    conn.name = "Internal DB".into();
    conn.tunnel_id = Some("tunnel-ssh".into());
    conn.ssh_tunnel = None;
    test.store.save_connection(conn).await.unwrap();

    let dest = test._temp.path().join("share.datazenconnection");
    assert_eq!(
        write_connections_export(&test.state, "share-secret", dest.clone())
            .await
            .unwrap(),
        1
    );

    let bytes = tokio::fs::read(&dest).await.unwrap();
    let parsed = parse_import_file(
        Path::new("datazen-connections.datazenconnection"),
        &bytes,
        Some("share-secret"),
    )
    .unwrap();
    let exported = &parsed.connections[0];
    let ssh = exported
        .ssh_tunnel
        .as_ref()
        .expect("a referenced SSH tunnel must be materialized into the export");
    assert!(ssh.enabled);
    assert_eq!(ssh.host, "bastion.internal");
    assert_eq!(ssh.port, 2222);
    assert_eq!(ssh.username, "ubuntu");
    assert_eq!(ssh.password.as_deref(), Some("ssh-secret"));
    // The database target stays the database target; only the jump host moved.
    assert_eq!(exported.host.as_deref(), Some("localhost"));
    assert_eq!(exported.port, Some(5432));
}

/// Tunnel kinds the export format cannot express, and dangling references, must
/// degrade to "no tunnel" instead of failing the whole export.
#[tokio::test]
async fn export_keeps_connections_whose_tunnel_cannot_be_exported() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{HttpProxyTunnelConfig, SavedTunnel, TunnelKind};

    let test = TestAppState::new().await;
    test.store
        .save_tunnel(SavedTunnel {
            id: "tunnel-proxy".into(),
            name: "Corp proxy".into(),
            kind: TunnelKind::HttpProxy,
            ssh: None,
            http_proxy: Some(HttpProxyTunnelConfig {
                enabled: true,
                host: "proxy.corp".into(),
                port: 8080,
                scheme: "http".into(),
                username: None,
                password: None,
                headers: None,
                connect_timeout_secs: 30,
            }),
            websocket: None,
        })
        .await
        .unwrap();

    let mut proxy = sample_postgres_config("ref-proxy");
    proxy.name = "Via proxy".into();
    proxy.tunnel_id = Some("tunnel-proxy".into());
    test.store.save_connection(proxy).await.unwrap();

    let mut dangling = sample_postgres_config("ref-dangling");
    dangling.name = "Dangling".into();
    dangling.tunnel_id = Some("tunnel-gone".into());
    test.store.save_connection(dangling).await.unwrap();

    let dest = test._temp.path().join("share.datazenconnection");
    assert_eq!(
        write_connections_export(&test.state, "share-secret", dest.clone())
            .await
            .unwrap(),
        2
    );

    let bytes = tokio::fs::read(&dest).await.unwrap();
    let parsed = parse_import_file(
        Path::new("datazen-connections.datazenconnection"),
        &bytes,
        Some("share-secret"),
    )
    .unwrap();
    assert_eq!(parsed.connections.len(), 2);
    for exported in &parsed.connections {
        assert!(
            exported.ssh_tunnel.is_none(),
            "`{}` must not gain an SSH tunnel from a non-SSH reference",
            exported.name
        );
    }
}

/// [tester] `materialize_tunnel_refs` mutates a *clone* of the store snapshot;
/// the live connection cache must stay untouched, otherwise exporting would
/// silently rewrite the running connection configs.
#[tokio::test]
async fn test_tester_materialize_does_not_pollute_the_live_connection_cache() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{SavedTunnel, SshTunnelConfig, TunnelKind};

    let test = TestAppState::new().await;
    test.store
        .save_tunnel(SavedTunnel {
            id: "tunnel-ssh".into(),
            name: "Bastion".into(),
            kind: TunnelKind::Ssh,
            ssh: Some(SshTunnelConfig {
                enabled: true,
                host: "bastion.internal".into(),
                port: 2222,
                username: "ubuntu".into(),
                auth_method: "password".into(),
                password: Some("ssh-secret".into()),
                private_key_path: None,
                passphrase: None,
                jump: None,
            }),
            http_proxy: None,
            websocket: None,
        })
        .await
        .unwrap();

    let mut conn = sample_postgres_config("ref-cache");
    conn.name = "Internal DB".into();
    conn.tunnel_id = Some("tunnel-ssh".into());
    conn.ssh_tunnel = None;
    test.store.save_connection(conn).await.unwrap();

    let dest = test._temp.path().join("share.datazenconnection");
    write_connections_export(&test.state, "share-secret", dest)
        .await
        .unwrap();

    let cached = test
        .store
        .get_connection("ref-cache")
        .await
        .expect("the exported connection is still cached");
    assert!(
        cached.ssh_tunnel.is_none(),
        "the export must not write the materialized tunnel back into the live cache"
    );
    assert_eq!(cached.tunnel_id.as_deref(), Some("tunnel-ssh"));
    assert_eq!(cached.host.as_deref(), Some("localhost"));
    assert_eq!(cached.port, Some(5432));
}

/// [tester] Every degradation branch of `materialize_tunnel_refs` must leave the
/// connection without a tunnel, without panicking and without touching the
/// connection's own database target.
#[tokio::test]
async fn test_tester_materialize_degrades_every_unsupported_branch() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{SavedTunnel, SshTunnelConfig, TunnelKind, WebSocketTunnelConfig};

    let test = TestAppState::new().await;
    for tunnel in [
        SavedTunnel {
            id: "t-none".into(),
            name: "Disabled".into(),
            kind: TunnelKind::None,
            ssh: None,
            http_proxy: None,
            websocket: None,
        },
        SavedTunnel {
            id: "t-ws".into(),
            name: "Relay".into(),
            kind: TunnelKind::WebSocket,
            ssh: None,
            http_proxy: None,
            websocket: Some(WebSocketTunnelConfig {
                enabled: true,
                url: "wss://relay.corp/v1".into(),
                auth_token: Some("ws-secret".into()),
                headers: None,
                connect_timeout_secs: 30,
                ping_interval_secs: 30,
                mode: "datazen_v1".into(),
            }),
        },
        SavedTunnel {
            id: "t-ssh-empty".into(),
            name: "SSH without config".into(),
            kind: TunnelKind::Ssh,
            ssh: None,
            http_proxy: None,
            websocket: None,
        },
    ] {
        test.store.save_tunnel(tunnel).await.unwrap();
    }

    let mut empty_ref = sample_postgres_config("c-empty-ref");
    empty_ref.tunnel_id = Some(String::new());
    let mut dangling = sample_postgres_config("c-dangling");
    dangling.tunnel_id = Some("t-gone".into());
    let mut none_kind = sample_postgres_config("c-none-kind");
    none_kind.tunnel_id = Some("t-none".into());
    let mut ws_kind = sample_postgres_config("c-ws-kind");
    ws_kind.tunnel_id = Some("t-ws".into());
    let mut ssh_no_config = sample_postgres_config("c-ssh-empty");
    ssh_no_config.tunnel_id = Some("t-ssh-empty".into());
    // No reference at all: an inline tunnel must survive untouched.
    let mut inline_only = sample_postgres_config("c-inline");
    inline_only.ssh_tunnel = Some(SshTunnelConfig {
        enabled: true,
        host: "inline.bastion".into(),
        port: 22,
        username: "inline-user".into(),
        auth_method: "password".into(),
        password: Some("inline-secret".into()),
        private_key_path: None,
        passphrase: None,
        jump: None,
    });

    let mut connections = vec![
        empty_ref,
        dangling,
        none_kind,
        ws_kind,
        ssh_no_config,
        inline_only,
    ];
    materialize_tunnel_refs(&test.state, &mut connections).await;

    for conn in connections.iter().take(5) {
        assert!(
            conn.ssh_tunnel.is_none(),
            "`{}` must degrade to no tunnel",
            conn.id
        );
        assert_eq!(
            conn.host.as_deref(),
            Some("localhost"),
            "`{}` must keep its own database host",
            conn.id
        );
        assert_eq!(
            conn.port,
            Some(5432),
            "`{}` must keep its own port",
            conn.id
        );
    }
    let inline = connections.last().expect("inline connection");
    assert_eq!(
        inline.ssh_tunnel.as_ref().map(|s| s.host.as_str()),
        Some("inline.bastion"),
        "a connection without a tunnel reference keeps its inline tunnel"
    );
}

/// [tester] The degradations are observable, not silent: the dangling-reference
/// and non-expressible-kind branches both emit a warning naming the connection
/// and the tunnel involved.
#[tokio::test]
async fn test_tester_materialize_warns_on_dangling_and_unsupported_references() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{SavedTunnel, TunnelKind, WebSocketTunnelConfig};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("log buffer lock")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let test = TestAppState::new().await;
    test.store
        .save_tunnel(SavedTunnel {
            id: "tunnel-ws".into(),
            name: "Relay".into(),
            kind: TunnelKind::WebSocket,
            ssh: None,
            http_proxy: None,
            websocket: Some(WebSocketTunnelConfig {
                enabled: true,
                url: "wss://relay.corp/v1".into(),
                auth_token: None,
                headers: None,
                connect_timeout_secs: 30,
                ping_interval_secs: 30,
                mode: "datazen_v1".into(),
            }),
        })
        .await
        .unwrap();

    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer_buf = buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || SharedBuf(writer_buf.clone()))
        .with_ansi(false)
        .finish();
    // Thread-local default: `#[tokio::test]` is a current-thread runtime, so the
    // awaited export stays on this thread and its warnings land in `buf`.
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut dangling = sample_postgres_config("c-dangling-log");
    dangling.name = "Dangling".into();
    dangling.tunnel_id = Some("tunnel-gone".into());
    let mut unsupported = sample_postgres_config("c-ws-log");
    unsupported.name = "Relay ref".into();
    unsupported.tunnel_id = Some("tunnel-ws".into());

    let mut connections = vec![dangling, unsupported];
    materialize_tunnel_refs(&test.state, &mut connections).await;

    let logged = String::from_utf8(buf.lock().expect("log buffer lock").clone()).unwrap();
    assert!(
        logged.contains("referenced tunnel not found"),
        "dangling reference must warn: {logged}"
    );
    assert!(
        logged.contains("tunnel-gone") && logged.contains("c-dangling-log"),
        "the dangling warning must name the tunnel and the connection: {logged}"
    );
    assert!(
        logged.contains("cannot be expressed"),
        "a non-expressible kind must warn: {logged}"
    );
    assert!(
        logged.contains("c-ws-log"),
        "the kind warning must name the connection: {logged}"
    );
}

/// [tester] A failing write must surface as an error and must not be mistaken
/// for a successful export (covers the `cmd_err("export_connections")` branch
/// the tunnel materialization now sits in front of).
#[tokio::test]
async fn test_tester_export_fails_when_the_destination_cannot_be_written() {
    use crate::testing::app_state::TestAppState;

    let test = TestAppState::new().await;
    test.save_connection("exp-unwritable").await;

    let dest = test
        ._temp
        .path()
        .join("no-such-directory")
        .join("share.datazenconnection");
    let err = write_connections_export(&test.state, "share-secret", dest)
        .await
        .expect_err("writing into a missing directory must fail the export");
    assert!(
        !err.to_string().is_empty(),
        "the export error must carry a message"
    );
    assert_eq!(test.store.get_connections().await.len(), 1);
}

/// A `kind = ssh` reference whose `ssh.enabled` is false is a silent degradation
/// in the payload (`connection_to_tableplus_json` filters disabled SSH blocks
/// out), so it must be treated like every other non-exportable branch: warned
/// about, and not materialized.
#[tokio::test]
async fn export_warns_and_skips_a_disabled_ssh_tunnel() {
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use datazen_driver_api::{SavedTunnel, SshTunnelConfig, TunnelKind};
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct SharedBuf(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for SharedBuf {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0
                .lock()
                .expect("log buffer lock")
                .extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    let test = TestAppState::new().await;
    test.store
        .save_tunnel(SavedTunnel {
            id: "tunnel-ssh-off".into(),
            name: "Disabled bastion".into(),
            kind: TunnelKind::Ssh,
            ssh: Some(SshTunnelConfig {
                enabled: false,
                host: "bastion.internal".into(),
                port: 2222,
                username: "ubuntu".into(),
                auth_method: "password".into(),
                password: Some("ssh-secret".into()),
                private_key_path: None,
                passphrase: None,
                jump: None,
            }),
            http_proxy: None,
            websocket: None,
        })
        .await
        .unwrap();

    let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
    let writer_buf = buf.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_writer(move || SharedBuf(writer_buf.clone()))
        .with_ansi(false)
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);

    let mut conn = sample_postgres_config("c-ssh-off");
    conn.name = "Disabled ref".into();
    conn.tunnel_id = Some("tunnel-ssh-off".into());
    let mut connections = vec![conn];
    materialize_tunnel_refs(&test.state, &mut connections).await;

    assert!(
        connections[0].ssh_tunnel.is_none(),
        "a disabled SSH tunnel must not be materialized into the export"
    );
    let logged = String::from_utf8(buf.lock().expect("log buffer lock").clone()).unwrap();
    assert!(
        logged.contains("SSH tunnel is disabled"),
        "a disabled SSH tunnel must warn: {logged}"
    );
    assert!(
        logged.contains("tunnel-ssh-off") && logged.contains("c-ssh-off"),
        "the warning must name the tunnel and the connection: {logged}"
    );
}
