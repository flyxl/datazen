//! Stand-in for React's `lib/databaseTypes.ts` registry, `connectionGroups.ts`
//! and the option/group sanitizers. There is no Rust `DB_REGISTRY` in this crate
//! yet; these few entries are the ones the hook's React tests pin.
//!
//! Leftover L1 (removal stage): when the driver meta registry lands in Rune,
//! replace `db_meta` with that lookup and delete the stand-in table.

use std::collections::BTreeMap;

/// React `DatabaseTypeMeta.connectionMode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectionMode {
    Server,
    File,
}

/// React `DatabaseTypeMeta.databaseFieldType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DatabaseFieldType {
    Name,
    Path,
    Index,
}

/// The subset of `DatabaseTypeMeta` the form hook reads.
#[derive(Clone, Debug)]
pub(crate) struct DatabaseTypeMeta {
    pub(crate) default_host: &'static str,
    pub(crate) default_port: u16,
    pub(crate) default_user: &'static str,
    pub(crate) default_database: &'static str,
    pub(crate) default_ssl_mode: &'static str,
    pub(crate) connection_form: &'static str,
    pub(crate) connection_mode: ConnectionMode,
    pub(crate) database_field_type: DatabaseFieldType,
    pub(crate) supports_ssl: bool,
    pub(crate) supports_ssh: bool,
    pub(crate) read_only: bool,
    pub(crate) connection_includes_schema: bool,
    pub(crate) default_options: &'static [(&'static str, &'static str)],
}

/// React `PRESET_GROUPS.development`.
pub(crate) const PRESET_DEVELOPMENT: &str = "preset:development";

const META: &[(&str, DatabaseTypeMeta)] = &[
    (
        "postgresql",
        DatabaseTypeMeta {
            default_host: "127.0.0.1",
            default_port: 5432,
            default_user: "postgres",
            default_database: "postgres",
            default_ssl_mode: "prefer",
            connection_form: "standard",
            connection_mode: ConnectionMode::Server,
            database_field_type: DatabaseFieldType::Name,
            supports_ssl: true,
            supports_ssh: true,
            read_only: false,
            connection_includes_schema: true,
            default_options: &[],
        },
    ),
    (
        "mysql",
        DatabaseTypeMeta {
            default_host: "127.0.0.1",
            default_port: 3306,
            default_user: "root",
            default_database: "",
            default_ssl_mode: "prefer",
            connection_form: "standard",
            connection_mode: ConnectionMode::Server,
            database_field_type: DatabaseFieldType::Name,
            supports_ssl: true,
            supports_ssh: true,
            read_only: false,
            connection_includes_schema: false,
            default_options: &[],
        },
    ),
    (
        "sqlite",
        DatabaseTypeMeta {
            default_host: "",
            default_port: 0,
            default_user: "",
            default_database: "",
            default_ssl_mode: "prefer",
            connection_form: "file",
            connection_mode: ConnectionMode::File,
            database_field_type: DatabaseFieldType::Path,
            supports_ssl: false,
            supports_ssh: false,
            read_only: false,
            connection_includes_schema: false,
            default_options: &[],
        },
    ),
    (
        "redis",
        DatabaseTypeMeta {
            default_host: "127.0.0.1",
            default_port: 6379,
            default_user: "",
            default_database: "0",
            default_ssl_mode: "disable",
            connection_form: "redis",
            connection_mode: ConnectionMode::Server,
            database_field_type: DatabaseFieldType::Index,
            supports_ssl: true,
            supports_ssh: true,
            read_only: false,
            connection_includes_schema: false,
            default_options: &[("topology", "standalone")],
        },
    ),
    (
        "kiwi",
        DatabaseTypeMeta {
            default_host: "127.0.0.1",
            default_port: 8080,
            default_user: "",
            default_database: "",
            default_ssl_mode: "prefer",
            connection_form: "kiwi",
            connection_mode: ConnectionMode::Server,
            database_field_type: DatabaseFieldType::Name,
            supports_ssl: true,
            supports_ssh: true,
            read_only: false,
            connection_includes_schema: false,
            default_options: &[],
        },
    ),
];

/// `DB_REGISTRY[id]` stand-in. Unknown types are absent, matching React.
pub(crate) fn db_meta(id: &str) -> Option<&'static DatabaseTypeMeta> {
    META.iter().find(|(key, _)| *key == id).map(|(_, meta)| meta)
}

/// `coerceConnectionGroup`: only a plain string survives; a mistaken DOM event
/// becomes the empty group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GroupInput {
    Text(String),
    Other,
}

pub(crate) fn coerce_group(value: GroupInput) -> String {
    match value {
        GroupInput::Text(text) => text,
        GroupInput::Other => String::new(),
    }
}

/// `sanitizeConnectionOptions` — scalar JSON-compatible options only.
pub(crate) fn sanitize_options(
    value: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    value
        .iter()
        .filter(|(key, entry)| !key.is_empty() && !entry.is_empty())
        .map(|(key, entry)| (key.clone(), entry.clone()))
        .collect()
}

/// `hasEnabledTlsOption` from `connectionFormModel.ts`.
pub(crate) fn has_enabled_tls_option(options: &BTreeMap<String, String>) -> bool {
    options
        .iter()
        .any(|(key, value)| {
            let key = key.to_ascii_lowercase();
            let value = value.to_ascii_lowercase();
            (key == "sslmode" || key == "ssl" || key == "tls")
                && value != "disable"
                && value != "false"
                && value != "0"
        })
}

// ---- Existing connection (stand-in for `ConnectionConfig`) ------------------

#[derive(Clone, Debug, Default)]
pub(crate) struct ExistingSsh {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub password: String,
    pub private_key_path: String,
    pub passphrase: String,
    pub jump_enabled: bool,
    pub jump_host: String,
    pub jump_port: u16,
    pub jump_username: String,
    pub jump_auth_method: String,
    pub jump_password: String,
    pub jump_private_key_path: String,
    pub jump_passphrase: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ExistingHttpProxy {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub scheme: String,
    pub username: String,
    pub password: String,
    pub connect_timeout_secs: u16,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ExistingWebSocket {
    pub enabled: bool,
    pub url: String,
    pub mode: String,
    pub auth_token: String,
    pub connect_timeout_secs: u16,
}

/// The fields of React `ConnectionConfig` the form hydrates from.
#[derive(Clone, Debug)]
pub(crate) struct ExistingConnection {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub ssl_mode: String,
    pub group: Option<String>,
    pub color_tag: Option<String>,
    pub read_only: bool,
    pub options: BTreeMap<String, String>,
    pub tunnel_id: Option<String>,
    pub tunnel_kind: Option<super::TunnelKind>,
    pub ssh: Option<ExistingSsh>,
    pub http_proxy: Option<ExistingHttpProxy>,
    pub websocket: Option<ExistingWebSocket>,
}

impl ExistingConnection {
    pub(crate) fn new(id: &str, database_type: &str, name: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            database_type: database_type.to_owned(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: "prefer".to_owned(),
            group: None,
            color_tag: None,
            read_only: false,
            options: BTreeMap::new(),
            tunnel_id: None,
            tunnel_kind: None,
            ssh: None,
            http_proxy: None,
            websocket: None,
        }
    }
}
