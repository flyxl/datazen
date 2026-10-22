//! Tunnel **summary / usage** IPC tests (`get_tunnel_summaries`, `get_tunnel_usage`).
//!
//! Split out of `commands/tunnel.rs` so that file stays under the 800-line
//! guideline (AGENTS.md, "单文件规模与模块拆分"). Pure relocation: every test
//! body, name and assertion is unchanged, and the probe tests stayed behind in
//! `commands/tunnel.rs`.

use super::tunnel::{get_tunnel_summaries_impl, get_tunnel_usage_impl};
use crate::testing::app_state::{sample_postgres_config, TestAppState};
use datazen_driver_api::{SavedTunnel, SshTunnelConfig, TunnelKind};

const SSH_PASSWORD: &str = "ssh-password-secret";
const SSH_PASSPHRASE: &str = "ssh-passphrase-secret";
const SSH_HOST: &str = "bastion.internal";

pub(crate) fn saved_ssh_tunnel(id: &str, name: &str) -> SavedTunnel {
    SavedTunnel {
        id: id.into(),
        name: name.into(),
        kind: TunnelKind::Ssh,
        ssh: Some(SshTunnelConfig {
            enabled: true,
            host: SSH_HOST.into(),
            port: 2222,
            username: "ubuntu".into(),
            auth_method: "password".into(),
            password: Some(SSH_PASSWORD.into()),
            private_key_path: None,
            passphrase: Some(SSH_PASSPHRASE.into()),
            jump: None,
        }),
        http_proxy: None,
        websocket: None,
    }
}

fn assert_no_secret_leak(json: &str) {
    for needle in [
        SSH_PASSWORD,
        SSH_PASSPHRASE,
        SSH_HOST,
        "ubuntu",
        "password",
        "passphrase",
        "authToken",
        "auth_token",
        "privateKeyPath",
    ] {
        assert!(
            !json.contains(needle),
            "IPC payload leaked `{needle}`: {json}"
        );
    }
}

#[tokio::test]
async fn summaries_drop_every_tunnel_secret() {
    let test = TestAppState::new().await;
    test.store
        .save_tunnel(saved_ssh_tunnel("t1", "Prod Bastion"))
        .await
        .unwrap();

    let summaries = get_tunnel_summaries_impl(&test.state).await.unwrap();
    assert_eq!(summaries.len(), 1);
    assert_eq!(summaries[0].id, "t1");
    assert_eq!(summaries[0].name, "Prod Bastion");
    assert_eq!(summaries[0].kind, TunnelKind::Ssh);

    let json = serde_json::to_string(&summaries).unwrap();
    assert_no_secret_leak(&json);
    let value: serde_json::Value = serde_json::to_value(&summaries).unwrap();
    assert_eq!(
        value,
        serde_json::json!([{ "id": "t1", "name": "Prod Bastion", "kind": "ssh" }]),
        "the summary must expose exactly id/name/kind"
    );
}

#[tokio::test]
async fn usage_lists_referencing_connections_in_order() {
    let test = TestAppState::new().await;
    test.store
        .save_tunnel(saved_ssh_tunnel("t1", "Prod Bastion"))
        .await
        .unwrap();
    test.store
        .save_tunnel(saved_ssh_tunnel("t2", "Other Bastion"))
        .await
        .unwrap();

    let mut hit = sample_postgres_config("conn-hit");
    hit.name = "Hit".into();
    hit.tunnel_id = Some("t1".into());
    test.store.save_connection(hit).await.unwrap();

    let mut other = sample_postgres_config("conn-other");
    other.name = "Other".into();
    other.tunnel_id = Some("t2".into());
    test.store.save_connection(other).await.unwrap();

    let usage = get_tunnel_usage_impl(&test.state, "t1".into())
        .await
        .unwrap();
    assert_eq!(usage.connection_ids, vec!["conn-hit".to_string()]);
    assert_eq!(usage.connection_names, vec!["Hit".to_string()]);

    let json = serde_json::to_string(&usage).unwrap();
    assert_no_secret_leak(&json);

    // Miss path: a stored tunnel nobody references returns empty arrays.
    let miss = get_tunnel_usage_impl(&test.state, "t2-missing".into())
        .await
        .unwrap();
    assert!(miss.connection_ids.is_empty());
    assert!(miss.connection_names.is_empty());

    let miss_unknown = get_tunnel_usage_impl(&test.state, "no-such-tunnel".into())
        .await
        .unwrap();
    assert!(miss_unknown.connection_ids.is_empty());
    assert!(miss_unknown.connection_names.is_empty());
}

// ── [tester] independent verification additions ──────────────────

#[tokio::test]
async fn test_tester_summaries_are_empty_for_an_empty_store() {
    let test = TestAppState::new().await;
    let summaries = get_tunnel_summaries_impl(&test.state).await.unwrap();
    assert!(
        summaries.is_empty(),
        "an empty store must project to an empty summary list"
    );
}

#[tokio::test]
async fn test_tester_summaries_keep_store_order_and_drop_secrets_for_every_kind() {
    use datazen_driver_api::{HttpProxyTunnelConfig, WebSocketTunnelConfig};

    let test = TestAppState::new().await;
    test.store
        .save_tunnel(saved_ssh_tunnel("t1", "Bastion"))
        .await
        .unwrap();
    test.store
        .save_tunnel(SavedTunnel {
            id: "t2".into(),
            name: "Corp proxy".into(),
            kind: TunnelKind::HttpProxy,
            ssh: None,
            http_proxy: Some(HttpProxyTunnelConfig {
                enabled: true,
                host: "proxy.corp".into(),
                port: 8080,
                scheme: "http".into(),
                username: Some("proxy-user".into()),
                password: Some("proxy-password-secret".into()),
                headers: None,
                connect_timeout_secs: 30,
            }),
            websocket: None,
        })
        .await
        .unwrap();
    test.store
        .save_tunnel(SavedTunnel {
            id: "t3".into(),
            name: "Relay".into(),
            kind: TunnelKind::WebSocket,
            ssh: None,
            http_proxy: None,
            websocket: Some(WebSocketTunnelConfig {
                enabled: true,
                url: "wss://relay.corp/v1".into(),
                auth_token: Some("ws-auth-token-secret".into()),
                headers: None,
                connect_timeout_secs: 30,
                ping_interval_secs: 30,
                mode: "datazen_v1".into(),
            }),
        })
        .await
        .unwrap();

    let summaries = get_tunnel_summaries_impl(&test.state).await.unwrap();
    assert_eq!(
        summaries.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(),
        vec!["t1", "t2", "t3"],
        "summaries must preserve store order"
    );
    assert_eq!(
        summaries.iter().map(|s| s.kind).collect::<Vec<_>>(),
        vec![
            TunnelKind::Ssh,
            TunnelKind::HttpProxy,
            TunnelKind::WebSocket
        ]
    );

    // Every kind's secrets must be gone, not just SSH's.
    let json = serde_json::to_string(&summaries).unwrap();
    for needle in [
        SSH_PASSWORD,
        SSH_PASSPHRASE,
        SSH_HOST,
        "proxy-password-secret",
        "proxy-user",
        "proxy.corp",
        "ws-auth-token-secret",
        "relay.corp",
    ] {
        assert!(!json.contains(needle), "summary leaked `{needle}`: {json}");
    }
}

#[tokio::test]
async fn test_tester_usage_is_positionally_aligned_across_multiple_hits() {
    let test = TestAppState::new().await;
    test.store
        .save_tunnel(saved_ssh_tunnel("t1", "Bastion"))
        .await
        .unwrap();

    // Deliberately insert in an order where the ids and the names do not
    // sort alike, so a positional mix-up cannot cancel out.
    for (id, name) in [("c-b", "Bravo"), ("c-a", "Alpha"), ("c-c", "Charlie")] {
        let mut conn = sample_postgres_config(id);
        conn.name = name.into();
        conn.tunnel_id = Some("t1".into());
        test.store.save_connection(conn).await.unwrap();
    }

    // Non-matching neighbours: one direct connection (`None`) and one on a
    // different tunnel.
    let mut direct = sample_postgres_config("c-direct");
    direct.name = "Direct".into();
    direct.tunnel_id = None;
    test.store.save_connection(direct).await.unwrap();

    let mut other = sample_postgres_config("c-other");
    other.name = "Other".into();
    other.tunnel_id = Some("t-other".into());
    test.store.save_connection(other).await.unwrap();

    let usage = get_tunnel_usage_impl(&test.state, "t1".into())
        .await
        .unwrap();
    assert_eq!(
        usage.connection_ids,
        vec!["c-b".to_string(), "c-a".to_string(), "c-c".to_string()],
        "ids must come back in store order"
    );
    assert_eq!(
        usage.connection_names,
        vec![
            "Bravo".to_string(),
            "Alpha".to_string(),
            "Charlie".to_string()
        ],
        "each name must stay aligned with its own id"
    );
    assert_eq!(usage.connection_ids.len(), usage.connection_names.len());
}

#[tokio::test]
async fn test_tester_usage_with_empty_id_matches_no_real_reference() {
    let test = TestAppState::new().await;
    test.store
        .save_tunnel(saved_ssh_tunnel("t1", "Bastion"))
        .await
        .unwrap();

    let mut referenced = sample_postgres_config("c-ref");
    referenced.tunnel_id = Some("t1".into());
    test.store.save_connection(referenced).await.unwrap();

    let mut direct = sample_postgres_config("c-direct");
    direct.tunnel_id = None;
    test.store.save_connection(direct).await.unwrap();

    // `None` must never match; an empty query must not sweep in every
    // direct connection (none of them carries an empty-string reference).
    let usage = get_tunnel_usage_impl(&test.state, String::new())
        .await
        .unwrap();
    assert!(usage.connection_ids.is_empty(), "{usage:?}");
    assert!(usage.connection_names.is_empty(), "{usage:?}");
}
