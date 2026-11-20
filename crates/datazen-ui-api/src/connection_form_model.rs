use serde_json::{Map, Value};

use crate::build_connection_url::ConnectionMode;
use crate::connection_config::clone_ssh_tunnel;
use crate::connection_types::{
    ConnectionConfig, DatabaseType, HttpProxyTunnelConfig, SshAuthMethod, SshTunnelConfig, SslMode,
    TunnelKind, WebSocketTunnelConfig,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DatabaseFieldType {
    #[default]
    Name,
    Index,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionFormSnapshot {
    pub name: String,
    pub host: String,
    pub port: String,
    pub database: String,
    pub schema: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: SslMode,
    pub group: String,
    pub color_tag: String,
    pub read_only: bool,
    pub connection_options: Map<String, Value>,
    pub show_advanced: bool,
    pub ssh_enabled: bool,
    pub ssh_host: String,
    pub ssh_port: String,
    pub ssh_username: String,
    pub ssh_auth_method: SshAuthMethod,
    pub ssh_password: String,
    pub ssh_key_path: String,
    pub ssh_passphrase: String,
    pub ssh_jump_enabled: bool,
    pub ssh_jump_host: String,
    pub ssh_jump_port: String,
    pub ssh_jump_username: String,
    pub ssh_jump_auth_method: SshAuthMethod,
    pub ssh_jump_password: String,
    pub ssh_jump_key_path: String,
    pub ssh_jump_passphrase: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionFormDriverMeta {
    pub default_ssl_mode: Option<SslMode>,
    pub read_only: bool,
    pub connection_mode: ConnectionMode,
    pub default_host: Option<String>,
    pub default_port: Option<u16>,
    pub database_field_type: DatabaseFieldType,
    pub max_database_index: Option<u32>,
    pub default_user: Option<String>,
    pub requires_username: bool,
    pub has_connection_form: bool,
    pub connection_includes_schema: bool,
}

impl Default for ConnectionFormDriverMeta {
    fn default() -> Self {
        Self {
            default_ssl_mode: None,
            read_only: false,
            connection_mode: ConnectionMode::Server,
            default_host: None,
            default_port: None,
            database_field_type: DatabaseFieldType::Name,
            max_database_index: None,
            default_user: None,
            requires_username: false,
            has_connection_form: false,
            connection_includes_schema: false,
        }
    }
}

pub struct BuildConnectionConfigInput {
    pub edit_id: Option<String>,
    pub new_id: Box<dyn Fn() -> String>,
    pub unnamed_label: String,
    pub name: String,
    pub database_type: DatabaseType,
    pub host: String,
    pub port: String,
    pub database: String,
    pub schema: String,
    pub username: String,
    pub password: String,
    pub ssl_mode: SslMode,
    pub group: String,
    pub color_tag: String,
    pub read_only: bool,
    pub connection_options: Map<String, Value>,
    pub ssh_tunnel: Option<SshTunnelConfig>,
    pub tunnel_kind: Option<TunnelKind>,
    pub tunnel_id: Option<String>,
    pub http_proxy_tunnel: Option<HttpProxyTunnelConfig>,
    pub websocket_tunnel: Option<WebSocketTunnelConfig>,
}

fn has_enabled_tls_option(options: &Map<String, Value>) -> bool {
    match options.get("tls") {
        Some(Value::Object(tls)) => tls.get("enabled") == Some(&Value::Bool(true)),
        _ => false,
    }
}

pub fn normalize_index_database_field(s: &str, max_index: u32) -> String {
    let trimmed = s.trim();
    if trimmed.is_empty() || !trimmed.bytes().all(|byte| byte.is_ascii_digit()) {
        return "0".to_owned();
    }
    let parsed = trimmed.parse::<u128>().unwrap_or(u128::MAX);
    parsed.min(max_index as u128).to_string()
}

pub fn clone_plain_json(value: &Value) -> Value {
    value.clone()
}

pub fn sanitize_connection_options(value: &Map<String, Value>) -> Map<String, Value> {
    value.clone()
}

pub fn coerce_connection_group(value: &Value) -> String {
    value.as_str().unwrap_or("").to_owned()
}

pub fn build_connection_config(
    input: BuildConnectionConfigInput,
    meta: Option<&ConnectionFormDriverMeta>,
) -> ConnectionConfig {
    let effective_ssl_mode = match meta {
        Some(meta)
            if meta.default_ssl_mode == Some(SslMode::Disable)
                && !has_enabled_tls_option(&input.connection_options) =>
        {
            SslMode::Disable
        }
        _ => input.ssl_mode,
    };
    let group = coerce_connection_group(&Value::String(input.group.clone()));
    let mut base = ConnectionConfig {
        id: input.edit_id.clone().unwrap_or_else(|| (input.new_id)()),
        name: if input.name.is_empty() {
            input.unnamed_label.clone()
        } else {
            input.name.clone()
        },
        database_type: input.database_type.clone(),
        host: None,
        port: None,
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: effective_ssl_mode,
        connection_timeout: None,
        max_pool_size: None,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: if input.color_tag.is_empty() {
            None
        } else {
            Some(input.color_tag.clone())
        },
        group: if group.is_empty() { None } else { Some(group) },
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: if meta.is_some_and(|meta| meta.read_only) || input.read_only {
            Some(true)
        } else {
            None
        },
        pinned: None,
    };

    if let Some(tunnel_id) = &input.tunnel_id {
        base.tunnel_id = Some(tunnel_id.clone());
    }
    if input.tunnel_id.is_none() {
        if let Some(tunnel) = &input.ssh_tunnel {
            base.ssh_tunnel = Some(clone_ssh_tunnel(tunnel));
        }
        if let Some(kind) = input.tunnel_kind {
            if kind != TunnelKind::None {
                base.tunnel_kind = Some(kind);
            }
        }
        if let Some(tunnel) = &input.http_proxy_tunnel {
            if tunnel.enabled {
                base.http_proxy_tunnel = Some(tunnel.clone());
            }
        }
        if let Some(tunnel) = &input.websocket_tunnel {
            if tunnel.enabled {
                base.websocket_tunnel = Some(tunnel.clone());
            }
        }
    } else if let Some(kind) = input.tunnel_kind {
        if kind != TunnelKind::None {
            base.tunnel_kind = Some(kind);
        }
    }

    let Some(meta) = meta else {
        base.database = Some(input.database.clone());
        return base;
    };
    if meta.connection_mode == ConnectionMode::File {
        base.database = Some(input.database.clone());
        return base;
    }

    base.host = if input.host.is_empty() {
        meta.default_host.clone()
    } else {
        Some(input.host.clone())
    };
    base.port = input
        .port
        .parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .or(meta.default_port);
    base.database = if meta.database_field_type == DatabaseFieldType::Index {
        Some(normalize_index_database_field(
            &input.database,
            meta.max_database_index.unwrap_or(15),
        ))
    } else if input.database.is_empty() {
        None
    } else {
        Some(input.database.clone())
    };
    base.password = if input.password.is_empty() {
        None
    } else {
        Some(input.password.clone())
    };
    if meta.default_user.is_some() || meta.requires_username || meta.has_connection_form {
        base.username = if input.username.is_empty() {
            meta.default_user.clone()
        } else {
            Some(input.username.clone())
        };
    }
    if meta.connection_includes_schema {
        let schema = input.schema.trim();
        base.schema = Some(if schema.is_empty() {
            "default".to_owned()
        } else {
            schema.to_owned()
        });
    }
    if !input.connection_options.is_empty() {
        if let Value::Object(options) =
            clone_plain_json(&Value::Object(input.connection_options.clone()))
        {
            base.options = Some(options);
        }
    }
    base
}

const FORM_STATE_MARKERS: [&str; 7] = [
    "setName",
    "onTest",
    "onSave",
    "draft",
    "meta",
    "setOptions",
    "handleDatabaseTypeChange",
];

fn walk(current: &Value, path: &str) -> Result<(), String> {
    match current {
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                walk(item, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        Value::Object(map) => {
            for (key, nested) in map {
                let next = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                walk(nested, &next)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub fn assert_plain_connection_config(value: &Value) -> Result<(), String> {
    let Some(root) = value.as_object() else {
        return Err("ConnectionConfig must be a plain object".to_owned());
    };
    for marker in FORM_STATE_MARKERS {
        if root.contains_key(marker) {
            return Err(format!(
                "IPC received ConnectionFormState (found \"{marker}\") — use buildConnectionConfig() instead"
            ));
        }
    }
    walk(value, "")
}

pub fn plain_config_assertion() -> impl Fn(&ConnectionConfig) -> Result<(), String> {
    |config: &ConnectionConfig| {
        let value = serde_json::to_value(config).map_err(|error| {
            format!("ConnectionConfig contains non-serializable value: {error}")
        })?;
        assert_plain_connection_config(&value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn option_map(pairs: &[(&str, Value)]) -> Map<String, Value> {
        let mut map = Map::new();
        for (key, value) in pairs {
            map.insert((*key).to_owned(), value.clone());
        }
        map
    }

    fn input() -> BuildConnectionConfigInput {
        BuildConnectionConfigInput {
            edit_id: None,
            new_id: Box::new(|| "generated-id".to_owned()),
            unnamed_label: "Unnamed".to_owned(),
            name: String::new(),
            database_type: "postgres".to_owned(),
            host: String::new(),
            port: String::new(),
            database: String::new(),
            schema: String::new(),
            username: String::new(),
            password: String::new(),
            ssl_mode: SslMode::Prefer,
            group: String::new(),
            color_tag: String::new(),
            read_only: false,
            connection_options: Map::new(),
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
        }
    }

    fn ssh_tunnel() -> SshTunnelConfig {
        SshTunnelConfig {
            enabled: true,
            host: "bastion".to_owned(),
            port: 22,
            username: "jump".to_owned(),
            auth_method: SshAuthMethod::PrivateKey,
            password: None,
            private_key_path: Some("/keys/id".to_owned()),
            passphrase: None,
            jump: Some(Box::new(SshTunnelConfig {
                enabled: true,
                host: "inner".to_owned(),
                port: 2222,
                username: "inner".to_owned(),
                auth_method: SshAuthMethod::Agent,
                password: None,
                private_key_path: None,
                passphrase: None,
                jump: None,
            })),
        }
    }

    #[test]
    fn normalize_index_field_blanks_non_digits_and_clamps() {
        assert_eq!(normalize_index_database_field("  ", 15), "0");
        assert_eq!(normalize_index_database_field("abc", 15), "0");
        assert_eq!(normalize_index_database_field("1 2", 15), "0");
        assert_eq!(normalize_index_database_field(" 3 ", 15), "3");
        assert_eq!(normalize_index_database_field("99", 15), "15");
        assert_eq!(normalize_index_database_field("0", 15), "0");
        assert_eq!(normalize_index_database_field("7", 3), "3");
        assert_eq!(
            normalize_index_database_field("999999999999999999999", 15),
            "15"
        );
    }

    #[test]
    fn ids_names_groups_and_optional_flags() {
        let config = build_connection_config(input(), None);
        assert_eq!(config.id, "generated-id");
        assert_eq!(config.name, "Unnamed");
        assert_eq!(config.group, None);
        assert_eq!(config.color_tag, None);
        assert_eq!(config.read_only, None);

        let mut source = input();
        source.edit_id = Some("existing".to_owned());
        source.name = "Named".to_owned();
        source.group = "team".to_owned();
        source.color_tag = "#fff".to_owned();
        source.read_only = true;
        let config = build_connection_config(source, None);
        assert_eq!(config.id, "existing");
        assert_eq!(config.name, "Named");
        assert_eq!(config.group.as_deref(), Some("team"));
        assert_eq!(config.color_tag.as_deref(), Some("#fff"));
        assert_eq!(config.read_only, Some(true));
    }

    #[test]
    fn meta_read_only_alone_sets_the_flag() {
        let meta = ConnectionFormDriverMeta {
            read_only: true,
            ..Default::default()
        };
        let config = build_connection_config(input(), Some(&meta));
        assert_eq!(config.read_only, Some(true));
    }

    #[test]
    fn missing_or_file_meta_keeps_database_and_no_host_fields() {
        let mut source = input();
        source.database = "/tmp/a.db".to_owned();
        source.host = "ignored".to_owned();
        let config = build_connection_config(source, None);
        assert_eq!(config.database.as_deref(), Some("/tmp/a.db"));
        assert_eq!(config.host, None);
        assert_eq!(config.port, None);

        let mut source = input();
        source.database = "/tmp/b.db".to_owned();
        let meta = ConnectionFormDriverMeta {
            connection_mode: ConnectionMode::File,
            default_host: Some("ignored".to_owned()),
            ..Default::default()
        };
        let config = build_connection_config(source, Some(&meta));
        assert_eq!(config.database.as_deref(), Some("/tmp/b.db"));
        assert_eq!(config.host, None);
    }

    #[test]
    fn server_mode_falls_back_to_meta_defaults() {
        let mut source = input();
        source.port = "not-a-number".to_owned();
        let meta = ConnectionFormDriverMeta {
            default_host: Some("localhost".to_owned()),
            default_port: Some(5432),
            ..Default::default()
        };
        let config = build_connection_config(source, Some(&meta));
        assert_eq!(config.host.as_deref(), Some("localhost"));
        assert_eq!(config.port, Some(5432));
        assert_eq!(config.database, None);

        let mut source = input();
        source.host = "db.internal".to_owned();
        source.port = "6543".to_owned();
        source.database = "app".to_owned();
        source.password = "secret".to_owned();
        let config = build_connection_config(source, Some(&meta));
        assert_eq!(config.host.as_deref(), Some("db.internal"));
        assert_eq!(config.port, Some(6543));
        assert_eq!(config.database.as_deref(), Some("app"));
        assert_eq!(config.password.as_deref(), Some("secret"));
    }

    #[test]
    fn zero_and_out_of_range_ports_use_the_default() {
        for value in ["0", "70000", ""] {
            let mut source = input();
            source.port = value.to_owned();
            let meta = ConnectionFormDriverMeta {
                default_port: Some(3306),
                ..Default::default()
            };
            assert_eq!(
                build_connection_config(source, Some(&meta)).port,
                Some(3306)
            );
        }
    }

    #[test]
    fn tls_option_overrides_a_disable_default() {
        let meta = ConnectionFormDriverMeta {
            default_ssl_mode: Some(SslMode::Disable),
            ..Default::default()
        };
        assert_eq!(
            build_connection_config(input(), Some(&meta)).ssl_mode,
            SslMode::Disable
        );

        let mut source = input();
        source
            .connection_options
            .insert("tls".to_owned(), json!({ "enabled": true }));
        assert_eq!(
            build_connection_config(source, Some(&meta)).ssl_mode,
            SslMode::Prefer
        );

        let mut source = input();
        source
            .connection_options
            .insert("tls".to_owned(), json!({ "enabled": 1 }));
        assert_eq!(
            build_connection_config(source, Some(&meta)).ssl_mode,
            SslMode::Disable
        );

        let mut source = input();
        source
            .connection_options
            .insert("tls".to_owned(), json!(["enabled"]));
        assert_eq!(
            build_connection_config(source, Some(&meta)).ssl_mode,
            SslMode::Disable
        );
    }

    #[test]
    fn index_database_field_is_normalized() {
        let mut source = input();
        source.database = "42".to_owned();
        let meta = ConnectionFormDriverMeta {
            database_field_type: DatabaseFieldType::Index,
            max_database_index: Some(9),
            ..Default::default()
        };
        assert_eq!(
            build_connection_config(source, Some(&meta))
                .database
                .as_deref(),
            Some("9")
        );

        let mut source = input();
        source.database = "x".to_owned();
        let meta = ConnectionFormDriverMeta {
            database_field_type: DatabaseFieldType::Index,
            ..Default::default()
        };
        assert_eq!(
            build_connection_config(source, Some(&meta))
                .database
                .as_deref(),
            Some("0")
        );
    }

    #[test]
    fn username_rules_and_schema_default() {
        let meta = ConnectionFormDriverMeta {
            default_user: Some("admin".to_owned()),
            connection_includes_schema: true,
            ..Default::default()
        };
        let config = build_connection_config(input(), Some(&meta));
        assert_eq!(config.username.as_deref(), Some("admin"));
        assert_eq!(config.schema.as_deref(), Some("default"));

        let mut source = input();
        source.username = "alice".to_owned();
        source.schema = "  sales  ".to_owned();
        let config = build_connection_config(source, Some(&meta));
        assert_eq!(config.username.as_deref(), Some("alice"));
        assert_eq!(config.schema.as_deref(), Some("sales"));

        let meta = ConnectionFormDriverMeta {
            requires_username: true,
            ..Default::default()
        };
        assert_eq!(build_connection_config(input(), Some(&meta)).username, None);
        let meta = ConnectionFormDriverMeta {
            has_connection_form: true,
            ..Default::default()
        };
        assert_eq!(build_connection_config(input(), Some(&meta)).username, None);
        assert_eq!(build_connection_config(input(), None).username, None);
    }

    #[test]
    fn options_are_cloned_only_when_nonempty() {
        let meta = ConnectionFormDriverMeta::default();
        let config = build_connection_config(input(), Some(&meta));
        assert_eq!(config.options, None);
        let mut source = input();
        source.connection_options.insert("a".to_owned(), json!(1));
        let config = build_connection_config(source, Some(&meta));
        assert_eq!(config.options, Some(option_map(&[("a", json!(1))])));
        let mut source = input();
        source.connection_options.insert("a".to_owned(), json!(1));
        assert_eq!(build_connection_config(source, None).options, None);
    }

    #[test]
    fn tunnel_id_skips_full_configs_but_keeps_a_kind_hint() {
        let mut source = input();
        source.tunnel_id = Some("saved".to_owned());
        source.tunnel_kind = Some(TunnelKind::Ssh);
        source.ssh_tunnel = Some(ssh_tunnel());
        source.http_proxy_tunnel = Some(HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy".to_owned(),
            port: 3128,
            scheme: crate::connection_types::HttpProxyScheme::Http,
            username: None,
            password: None,
            headers: None,
            connect_timeout_secs: None,
        });
        let config = build_connection_config(source, None);
        assert_eq!(config.tunnel_id.as_deref(), Some("saved"));
        assert_eq!(config.tunnel_kind, Some(TunnelKind::Ssh));
        assert_eq!(config.ssh_tunnel, None);
        assert_eq!(config.http_proxy_tunnel, None);
    }

    #[test]
    fn embedded_tunnels_are_fresh_and_disabled_ones_are_skipped() {
        let mut source = input();
        source.ssh_tunnel = Some(ssh_tunnel());
        source.tunnel_kind = Some(TunnelKind::None);
        source.http_proxy_tunnel = Some(HttpProxyTunnelConfig {
            enabled: false,
            host: "proxy".to_owned(),
            port: 3128,
            scheme: crate::connection_types::HttpProxyScheme::Http,
            username: None,
            password: None,
            headers: None,
            connect_timeout_secs: None,
        });
        source.websocket_tunnel = Some(WebSocketTunnelConfig {
            enabled: true,
            url: "wss://relay".to_owned(),
            auth_token: None,
            headers: None,
            connect_timeout_secs: None,
            ping_interval_secs: None,
            mode: Some(crate::connection_types::WebSocketMode::DatazenV1),
        });
        let config = build_connection_config(source, None);
        assert_eq!(config.tunnel_kind, None);
        assert_eq!(config.http_proxy_tunnel, None);
        assert_eq!(
            config
                .websocket_tunnel
                .as_ref()
                .map(|tunnel| tunnel.url.as_str()),
            Some("wss://relay")
        );
        let mut original = ssh_tunnel();
        if let Some(hop) = original.jump.as_mut() {
            hop.host = "changed".to_owned();
        }
        assert_eq!(
            config
                .ssh_tunnel
                .as_ref()
                .and_then(|tunnel| tunnel.jump.as_ref())
                .map(|hop| hop.host.as_str()),
            Some("inner")
        );
    }

    #[test]
    fn json_helpers_sanitize_and_coerce() {
        let original = json!({ "a": { "b": [1, 2] } });
        let cloned = clone_plain_json(&original);
        assert_eq!(cloned, original);
        let mut source = original.clone();
        source["a"]["b"][0] = json!(99);
        assert_eq!(cloned, json!({ "a": { "b": [1, 2] } }));

        let options = option_map(&[("k", json!(true))]);
        assert_eq!(sanitize_connection_options(&options), options);

        assert_eq!(coerce_connection_group(&json!("team")), "team");
        assert_eq!(coerce_connection_group(&json!(5)), "");
        assert_eq!(coerce_connection_group(&json!({ "x": 1 })), "");
        assert_eq!(coerce_connection_group(&Value::Null), "");
    }

    #[test]
    fn plain_assertion_rejects_non_objects_and_form_markers() {
        for value in [json!(null), json!([]), json!("x"), json!(1)] {
            assert_eq!(
                assert_plain_connection_config(&value),
                Err("ConnectionConfig must be a plain object".to_owned())
            );
        }
        for marker in FORM_STATE_MARKERS {
            let value = json!({ marker: true });
            let error = assert_plain_connection_config(&value).unwrap_err();
            assert!(error.contains(marker));
            assert!(error.contains("use buildConnectionConfig() instead"));
        }
    }

    #[test]
    fn plain_assertion_walks_nested_values_and_accepts_a_built_config() {
        let nested = json!({
            "options": { "tls": { "enabled": true }, "list": [1, "x", null] }
        });
        assert_eq!(assert_plain_connection_config(&nested), Ok(()));
        let config = build_connection_config(input(), None);
        assert_eq!(plain_config_assertion()(&config), Ok(()));
    }
}
