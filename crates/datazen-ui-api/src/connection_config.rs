use crate::connection_types::{
    ConnectionConfig, DatabaseType, HttpProxyScheme, HttpProxyTunnelConfig, SshTunnelConfig,
    SslMode, TunnelKind, WebSocketMode, WebSocketTunnelConfig,
};
use serde_json::{Map, Value};

pub type PlainConfigAssertion = dyn Fn(&ConnectionConfig) -> Result<(), String>;

pub(crate) fn clone_ssh_tunnel(tunnel: &SshTunnelConfig) -> SshTunnelConfig {
    SshTunnelConfig {
        enabled: tunnel.enabled,
        host: tunnel.host.clone(),
        port: tunnel.port,
        username: tunnel.username.clone(),
        auth_method: tunnel.auth_method.clone(),
        password: tunnel.password.clone(),
        private_key_path: tunnel.private_key_path.clone(),
        passphrase: tunnel.passphrase.clone(),
        jump: tunnel.jump.as_ref().map(|hop| Box::new(clone_ssh_tunnel(hop))),
    }
}

fn clone_http_proxy_tunnel(tunnel: &HttpProxyTunnelConfig) -> HttpProxyTunnelConfig {
    HttpProxyTunnelConfig {
        enabled: tunnel.enabled,
        host: tunnel.host.clone(),
        port: tunnel.port,
        scheme: tunnel.scheme.clone(),
        username: tunnel.username.clone(),
        password: tunnel.password.clone(),
        headers: tunnel.headers.clone(),
        connect_timeout_secs: tunnel.connect_timeout_secs,
    }
}

fn clone_websocket_tunnel(tunnel: &WebSocketTunnelConfig) -> WebSocketTunnelConfig {
    WebSocketTunnelConfig {
        enabled: tunnel.enabled,
        url: tunnel.url.clone(),
        auth_token: tunnel.auth_token.clone(),
        headers: tunnel.headers.clone(),
        connect_timeout_secs: tunnel.connect_timeout_secs,
        ping_interval_secs: tunnel.ping_interval_secs,
        mode: tunnel.mode.clone(),
    }
}

fn clone_options(options: &Map<String, Value>) -> Map<String, Value> {
    options.clone()
}

pub fn clone_connection_config_for_ipc(
    config: &ConnectionConfig,
    assert_plain: &PlainConfigAssertion,
) -> Result<ConnectionConfig, String> {
    assert_plain(config)?;
    let out = ConnectionConfig {
        id: config.id.clone(),
        name: config.name.clone(),
        database_type: config.database_type.clone(),
        host: config.host.clone(),
        port: config.port,
        database: config.database.clone(),
        schema: config.schema.clone(),
        username: config.username.clone(),
        password: config.password.clone(),
        ssl_mode: config.ssl_mode,
        connection_timeout: config.connection_timeout,
        max_pool_size: config.max_pool_size,
        ssh_tunnel: config.ssh_tunnel.as_ref().map(clone_ssh_tunnel),
        tunnel_kind: config.tunnel_kind,
        tunnel_id: config.tunnel_id.clone(),
        http_proxy_tunnel: config.http_proxy_tunnel.as_ref().map(clone_http_proxy_tunnel),
        websocket_tunnel: config.websocket_tunnel.as_ref().map(clone_websocket_tunnel),
        color_tag: config.color_tag.clone(),
        group: config.group.clone(),
        last_connected_at: config.last_connected_at.clone(),
        server_version: config.server_version.clone(),
        options: config.options.as_ref().filter(|options| !options.is_empty()).map(clone_options),
        read_only: config.read_only,
        pinned: config.pinned,
    };
    Ok(out)
}

pub fn to_ipc_connection_config(
    config: &ConnectionConfig,
    assert_plain: &PlainConfigAssertion,
) -> Result<ConnectionConfig, String> {
    assert_plain(config)?;
    clone_connection_config_for_ipc(config, assert_plain)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use crate::connection_types::SshAuthMethod;

    use super::*;

    fn ok_assertion() -> impl Fn(&ConnectionConfig) -> Result<(), String> {
        |_| Ok(())
    }

    fn count_assertion(calls: Rc<RefCell<Vec<String>>>) -> impl Fn(&ConnectionConfig) -> Result<(), String> {
        move |config| {
            calls.borrow_mut().push(config.id.clone());
            Ok(())
        }
    }

    fn full_config() -> ConnectionConfig {
        let mut options = Map::new();
        options.insert("tls".to_owned(), Value::Bool(true));
        options.insert("retries".to_owned(), Value::from(3));
        ConnectionConfig {
            id: "c1".to_owned(),
            name: "Local".to_owned(),
            database_type: DatabaseType::from("postgres"),
            host: Some("db.local".to_owned()),
            port: Some(5432),
            database: Some("app".to_owned()),
            schema: Some("public".to_owned()),
            username: Some("alice".to_owned()),
            password: Some("secret".to_owned()),
            ssl_mode: SslMode::Require,
            connection_timeout: Some(15),
            max_pool_size: Some(10),
            ssh_tunnel: Some(SshTunnelConfig {
                enabled: true,
                host: "bastion".to_owned(),
                port: 22,
                username: "jump".to_owned(),
                auth_method: SshAuthMethod::PrivateKey,
                password: None,
                private_key_path: Some("/keys/id".to_owned()),
                passphrase: Some("phrase".to_owned()),
                jump: Some(Box::new(SshTunnelConfig {
                    enabled: true,
                    host: "inner".to_owned(),
                    port: 2222,
                    username: "inner".to_owned(),
                    auth_method: SshAuthMethod::Agent,
                    password: Some("inner-pw".to_owned()),
                    private_key_path: None,
                    passphrase: None,
                    jump: None,
                })),
            }),
            tunnel_kind: Some(TunnelKind::Ssh),
            tunnel_id: Some("saved-1".to_owned()),
            http_proxy_tunnel: Some(HttpProxyTunnelConfig {
                enabled: true,
                host: "proxy.local".to_owned(),
                port: 3128,
                scheme: HttpProxyScheme::Http,
                username: Some("proxy-user".to_owned()),
                password: None,
                headers: None,
                connect_timeout_secs: Some(5),
            }),
            websocket_tunnel: Some(WebSocketTunnelConfig {
                enabled: true,
                url: "wss://relay.local".to_owned(),
                auth_token: Some("token".to_owned()),
                headers: None,
                connect_timeout_secs: None,
                ping_interval_secs: Some(30),
                mode: Some(WebSocketMode::DatazenV1),
            }),
            color_tag: Some("#fff".to_owned()),
            group: Some("group-a".to_owned()),
            last_connected_at: Some("2026-01-01".to_owned()),
            server_version: Some("16.1".to_owned()),
            options: Some(options),
            read_only: Some(true),
            pinned: Some(false),
        }
    }

    #[test]
    fn required_fields_are_copied_and_all_optionals_survive() {
        let config = full_config();
        let cloned = clone_connection_config_for_ipc(&config, &ok_assertion()).expect("ok");
        assert_eq!(cloned, config);
        assert_eq!(cloned.id, "c1");
        assert_eq!(cloned.ssl_mode, SslMode::Require);
        assert_eq!(cloned.database_type, DatabaseType::from("postgres"));
    }

    #[test]
    fn no_optional_fields_stay_absent() {
        let config = ConnectionConfig {
            id: "bare".to_owned(),
            name: "Bare".to_owned(),
            database_type: DatabaseType::from("sqlite"),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
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
        let cloned = clone_connection_config_for_ipc(&config, &ok_assertion()).expect("ok");
        assert_eq!(cloned, config);
        assert!(cloned.ssh_tunnel.is_none());
        assert!(cloned.options.is_none());
    }

    #[test]
    fn empty_options_are_dropped_but_nonempty_options_are_kept() {
        let mut config = full_config();
        config.options = Some(Map::new());
        let cloned = clone_connection_config_for_ipc(&config, &ok_assertion()).expect("ok");
        assert!(cloned.options.is_none());

        let mut config = full_config();
        let options = config.options.clone().expect("present");
        let cloned = clone_connection_config_for_ipc(&config, &ok_assertion()).expect("ok");
        config.options.as_mut().expect("present").insert("extra".to_owned(), Value::Bool(true));
        assert_eq!(cloned.options, Some(options));
        assert_ne!(cloned.options, config.options);
    }

    #[test]
    fn ssh_tunnel_and_nested_jump_are_deeply_fresh() {
        let mut config = full_config();
        let cloned = clone_connection_config_for_ipc(&config, &ok_assertion()).expect("ok");
        let original_jump = config.ssh_tunnel.as_mut().expect("present").jump.as_mut().expect("present");
        original_jump.host = "changed".to_owned();
        original_jump.password = None;
        let cloned_jump = cloned.ssh_tunnel.expect("present").jump.expect("present");
        assert_eq!(cloned_jump.host, "inner");
        assert_eq!(cloned_jump.password.as_deref(), Some("inner-pw"));
        assert_eq!(cloned_jump.auth_method, SshAuthMethod::Agent);
    }

    #[test]
    fn tunnel_variants_are_fresh_copies() {
        let mut config = full_config();
        let cloned = clone_connection_config_for_ipc(&config, &ok_assertion()).expect("ok");
        config.http_proxy_tunnel.as_mut().expect("present").host = "changed".to_owned();
        config.websocket_tunnel.as_mut().expect("present").url = "changed".to_owned();
        let http = cloned.http_proxy_tunnel.expect("present");
        let websocket = cloned.websocket_tunnel.expect("present");
        assert_eq!(http.host, "proxy.local");
        assert_eq!(websocket.url, "wss://relay.local");
    }

    #[test]
    fn clone_for_ipc_asserts_once_before_copying() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let assertion = count_assertion(calls.clone());
        let config = full_config();
        let cloned = clone_connection_config_for_ipc(&config, &assertion).expect("ok");
        assert_eq!(cloned, config);
        assert_eq!(calls.borrow().as_slice(), &["c1".to_owned()]);
    }

    #[test]
    fn to_ipc_asserts_twice_and_matches_the_clone() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let assertion = count_assertion(calls.clone());
        let config = full_config();
        let via_to = to_ipc_connection_config(&config, &assertion).expect("ok");
        assert_eq!(calls.borrow().as_slice(), &["c1".to_owned(), "c1".to_owned()]);
        let via_clone = clone_connection_config_for_ipc(&config, &ok_assertion()).expect("ok");
        assert_eq!(via_to, via_clone);
    }

    #[test]
    fn assertion_failure_propagates_and_stops_before_copying() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let seen = calls.clone();
        let rejection = move |config: &ConnectionConfig| {
            seen.borrow_mut().push(config.id.clone());
            Err("IPC received ConnectionFormState".to_owned())
        };
        let config = full_config();
        assert_eq!(
            clone_connection_config_for_ipc(&config, &rejection).unwrap_err(),
            "IPC received ConnectionFormState"
        );
        assert_eq!(calls.borrow().len(), 1);
        calls.borrow_mut().clear();
        assert_eq!(
            to_ipc_connection_config(&config, &rejection).unwrap_err(),
            "IPC received ConnectionFormState"
        );
        assert_eq!(calls.borrow().len(), 1, "the second assertion is never reached on failure");
    }
}
