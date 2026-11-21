use std::collections::HashMap;
use std::rc::Rc;

use serde_json::Value;

use crate::{ConnectionMode, SslMode};

pub const DB_TYPE_POPULARITY_ORDER: [&str; 26] = [
    "mysql",
    "postgresql",
    "mariadb",
    "sqlite",
    "redis",
    "mongodb",
    "sqlserver",
    "clickhouse",
    "doris",
    "starrocks",
    "ob_oracle",
    "elasticsearch",
    "duckdb",
    "questdb",
    "cloudberry",
    "turso",
    "rqlite",
    "influxdb",
    "victoriametrics",
    "hbase",
    "manticore",
    "vector",
    "kiwi",
    "presto",
    "trino",
    "superset",
];

pub const DEFAULT_MAX_DATABASE_INDEX: u32 = 15;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DatabaseTypeMeta {
    pub label: String,
    pub short_label: String,
    pub icon_bg: String,
    pub quote_char: String,
    pub connection_mode: ConnectionMode,
    pub default_ssl_mode: Option<SslMode>,
    pub default_host: String,
    pub default_port: u32,
}

impl Default for DatabaseTypeMeta {
    fn default() -> Self {
        Self {
            label: String::new(),
            short_label: String::new(),
            icon_bg: String::new(),
            quote_char: "\"".to_owned(),
            connection_mode: ConnectionMode::Server,
            default_ssl_mode: None,
            default_host: String::new(),
            default_port: 0,
        }
    }
}

pub type SqlValueEscaper = dyn Fn(&Value) -> String;

pub struct DatabaseRegistry {
    entries: HashMap<String, DatabaseTypeMeta>,
    icons: HashMap<String, String>,
    icon_parents: HashMap<String, String>,
    escapers: HashMap<String, Rc<SqlValueEscaper>>,
}

impl DatabaseRegistry {
    pub fn new(
        entries: Vec<(String, DatabaseTypeMeta)>,
        icons: Vec<(String, String)>,
        icon_parents: Vec<(String, String)>,
        escapers: Vec<(String, Rc<SqlValueEscaper>)>,
    ) -> Self {
        Self {
            entries: entries.into_iter().collect(),
            icons: icons.into_iter().collect(),
            icon_parents: icon_parents.into_iter().collect(),
            escapers: escapers.into_iter().collect(),
        }
    }

    pub fn register_metas(&mut self, metas: Vec<(String, DatabaseTypeMeta)>) {
        for (key, meta) in metas {
            self.entries.insert(key, meta);
        }
    }

    pub fn get(&self, db_type: &str) -> Option<&DatabaseTypeMeta> {
        self.entries.get(db_type)
    }

    pub fn driver_icons(&self) -> HashMap<String, String> {
        self.icons.clone()
    }

    pub fn driver_icon_parents(&self) -> HashMap<String, String> {
        self.icon_parents.clone()
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Default for DatabaseRegistry {
    fn default() -> Self {
        Self::new(Vec::new(), Vec::new(), Vec::new(), Vec::new())
    }
}

pub trait PopularityValue {
    fn value(&self) -> &str;
}

impl PopularityValue for String {
    fn value(&self) -> &str {
        self.as_str()
    }
}

fn popularity_rank(value: &str) -> usize {
    DB_TYPE_POPULARITY_ORDER.iter().position(|entry| *entry == value).unwrap_or(usize::MAX)
}

pub fn sort_db_types_by_popularity<T: PopularityValue + Clone>(items: &[T]) -> Vec<T> {
    let mut sorted = items.to_vec();
    sorted.sort_by(|a, b| {
        popularity_rank(a.value())
            .cmp(&popularity_rank(b.value()))
            .then_with(|| a.value().to_lowercase().cmp(&b.value().to_lowercase()))
    });
    sorted
}

pub fn escape_ident(name: &str, db_type: Option<&str>, registry: &DatabaseRegistry) -> String {
    let quote = match db_type.filter(|value| !value.is_empty()) {
        Some(db_type) => registry.get(db_type).map_or("\"", |meta| meta.quote_char.as_str()),
        None => "\"",
    };
    match quote {
        "`" => format!("`{}`", name.replace('`', "``")),
        "\"" => format!("\"{}\"", name.replace('"', "\"\"")),
        "[" => format!("[{}]", name.replace(']', "]]")),
        _ => name.to_owned(),
    }
}

fn quote_sql(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

pub fn default_escape_sql_value(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_owned(),
        Value::Number(number) => match number.as_f64() {
            Some(number) if number.fract() == 0.0 => format!("{number:.0}"),
            Some(number) => number.to_string(),
            None => number.to_string(),
        },
        Value::Bool(flag) => if *flag { "TRUE" } else { "FALSE" }.to_owned(),
        Value::String(text) => quote_sql(text),
        Value::Array(_) | Value::Object(_) => {
            let text = serde_json::to_string(value).unwrap_or_else(|_| "null".to_owned());
            quote_sql(&text)
        }
    }
}

pub enum EscapeSqlValue {
    Default,
    Custom(Rc<SqlValueEscaper>),
}

impl EscapeSqlValue {
    pub fn escape(&self, value: &Value) -> String {
        match self {
            EscapeSqlValue::Default => default_escape_sql_value(value),
            EscapeSqlValue::Custom(escaper) => escaper(value),
        }
    }
}

pub fn get_escape_sql_value(db_type: Option<&str>, registry: &DatabaseRegistry) -> EscapeSqlValue {
    if let Some(db_type) = db_type.filter(|value| !value.is_empty()) {
        if let Some(escaper) = registry.escapers.get(db_type) {
            return EscapeSqlValue::Custom(escaper.clone());
        }
    }
    EscapeSqlValue::Default
}

pub fn get_db_label(db_type: &str, registry: &DatabaseRegistry) -> String {
    registry.get(db_type).map_or_else(|| db_type.to_owned(), |meta| meta.label.clone())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbIconInfo {
    pub label: String,
    pub bg: String,
}

pub fn get_db_icon(db_type: &str, registry: &DatabaseRegistry) -> DbIconInfo {
    match registry.get(db_type) {
        Some(meta) => DbIconInfo {
            label: meta.short_label.clone(),
            bg: meta.icon_bg.clone(),
        },
        None => DbIconInfo {
            label: "DB".to_owned(),
            bg: "bg-gray-500".to_owned(),
        },
    }
}

pub fn get_driver_icon_map(registry: &DatabaseRegistry) -> HashMap<String, String> {
    registry.driver_icons()
}

pub fn get_driver_icon_parents(registry: &DatabaseRegistry) -> HashMap<String, String> {
    registry.driver_icon_parents()
}

pub fn normalize_index_database_field_default(s: &str) -> String {
    crate::normalize_index_database_field(s, DEFAULT_MAX_DATABASE_INDEX)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SshTunnelAddr {
    pub enabled: bool,
    pub host: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionAddrInput {
    pub database_type: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub database: Option<String>,
    pub ssh_tunnel: Option<SshTunnelAddr>,
}

pub fn format_connection_addr(conn: &ConnectionAddrInput, registry: &DatabaseRegistry) -> String {
    if let Some(meta) = registry.get(&conn.database_type) {
        match meta.connection_mode {
            ConnectionMode::File => {
                return conn.database.clone().unwrap_or_else(|| meta.label.clone());
            }
            ConnectionMode::Url => {
                return conn.host.clone().unwrap_or_else(|| meta.label.clone());
            }
            ConnectionMode::Server => (),
        }
    }
    let has_ssh = conn.ssh_tunnel.as_ref().is_some_and(|tunnel| tunnel.enabled);
    let host = conn.host.as_deref().unwrap_or("");
    let database = conn.database.as_deref().unwrap_or("");
    if has_ssh {
        let tunnel_host = conn.ssh_tunnel.as_ref().map_or("", |tunnel| tunnel.host.as_str());
        return format!("{tunnel_host} → {host} : {database}");
    }
    format!("{host} : {database}")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn meta(label: &str, short: &str, bg: &str) -> DatabaseTypeMeta {
        DatabaseTypeMeta {
            label: label.to_owned(),
            short_label: short.to_owned(),
            icon_bg: bg.to_owned(),
            quote_char: "\"".to_owned(),
            ..Default::default()
        }
    }

    fn registry() -> DatabaseRegistry {
        DatabaseRegistry::new(
            vec![
                ("mysql".to_owned(), DatabaseTypeMeta {
                    label: "MySQL".to_owned(),
                    short_label: "My".to_owned(),
                    icon_bg: "bg-orange".to_owned(),
                    quote_char: "`".to_owned(),
                    ..Default::default()
                }),
                ("sqlserver".to_owned(), DatabaseTypeMeta {
                    label: "SQL Server".to_owned(),
                    short_label: "Ms".to_owned(),
                    icon_bg: "bg-red".to_owned(),
                    quote_char: "[".to_owned(),
                    ..Default::default()
                }),
                ("sqlite".to_owned(), DatabaseTypeMeta {
                    connection_mode: ConnectionMode::File,
                    ..meta("SQLite", "Sq", "bg-blue")
                }),
                ("redis".to_owned(), DatabaseTypeMeta {
                    connection_mode: ConnectionMode::Server,
                    ..meta("Redis", "Re", "bg-red")
                }),
            ],
            vec![("db.mysql".to_owned(), "https://x/my.svg".to_owned())],
            vec![("mariadb".to_owned(), "mysql".to_owned())],
            vec![("custom".to_owned(), Rc::new(|value: &Value| format!("CUSTOM({value})")) as Rc<SqlValueEscaper>)],
        )
    }

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct OptionItem {
        value: String,
        extra: u32,
    }

    impl PopularityValue for OptionItem {
        fn value(&self) -> &str {
            &self.value
        }
    }

    #[test]
    fn popularity_order_is_pinned() {
        assert_eq!(DB_TYPE_POPULARITY_ORDER.len(), 26);
        assert_eq!(DB_TYPE_POPULARITY_ORDER[0], "mysql");
        assert_eq!(DB_TYPE_POPULARITY_ORDER[1], "postgresql");
        assert_eq!(DB_TYPE_POPULARITY_ORDER[25], "superset");
    }

    #[test]
    fn popularity_sort_ranks_known_then_unknown_alphabetically() {
        let items = vec![
            OptionItem { value: "zzz".to_owned(), extra: 1 },
            OptionItem { value: "sqlite".to_owned(), extra: 2 },
            OptionItem { value: "aaa".to_owned(), extra: 3 },
            OptionItem { value: "mysql".to_owned(), extra: 4 },
        ];
        let sorted = sort_db_types_by_popularity(&items);
        assert_eq!(sorted.iter().map(|item| item.value.as_str()).collect::<Vec<_>>(), vec!["mysql", "sqlite", "aaa", "zzz"]);
        assert_eq!(items[0].value, "zzz", "the input is not mutated");
    }

    #[test]
    fn escape_ident_uses_the_driver_quote_char_and_doubles_closers() {
        let registry = registry();
        assert_eq!(escape_ident("col", Some("mysql"), &registry), "`col`");
        assert_eq!(escape_ident("a`b", Some("mysql"), &registry), "`a``b`");
        assert_eq!(escape_ident("a\"b", Some("postgres"), &registry), "\"a\"\"b\"");
        assert_eq!(escape_ident("a]b", Some("sqlserver"), &registry), "[a]]b]");
        assert_eq!(escape_ident("col", None, &registry), "\"col\"");
        assert_eq!(escape_ident("col", Some(""), &registry), "\"col\"");
        assert_eq!(escape_ident("col", Some("unknown"), &registry), "\"col\"");
    }

    #[test]
    fn default_escape_sql_value_covers_null_numbers_bools_strings_and_objects() {
        assert_eq!(default_escape_sql_value(&Value::Null), "NULL");
        assert_eq!(default_escape_sql_value(&json!(42)), "42");
        assert_eq!(default_escape_sql_value(&json!(1.5)), "1.5");
        assert_eq!(default_escape_sql_value(&json!(true)), "TRUE");
        assert_eq!(default_escape_sql_value(&json!(false)), "FALSE");
        assert_eq!(default_escape_sql_value(&json!("O'Brien")), "'O''Brien'");
        assert_eq!(default_escape_sql_value(&json!({"a": 1})), "'{\"a\":1}'");
        assert_eq!(default_escape_sql_value(&json!([1, 2])), "'[1,2]'");
    }

    #[test]
    fn get_escape_sql_value_prefers_a_registered_custom_escaper() {
        let registry = registry();
        assert_eq!(get_escape_sql_value(Some("custom"), &registry).escape(&json!(1)), "CUSTOM(1)");
        assert_eq!(get_escape_sql_value(Some("mysql"), &registry).escape(&json!("x")), "'x'");
        assert_eq!(get_escape_sql_value(None, &registry).escape(&Value::Null), "NULL");
        assert_eq!(get_escape_sql_value(Some(""), &registry).escape(&json!(true)), "TRUE");
    }

    #[test]
    fn labels_and_icons_fall_back_for_unknown_types() {
        let registry = registry();
        assert_eq!(get_db_label("mysql", &registry), "MySQL");
        assert_eq!(get_db_label("ghost", &registry), "ghost");
        assert_eq!(get_db_icon("mysql", &registry), DbIconInfo { label: "My".to_owned(), bg: "bg-orange".to_owned() });
        assert_eq!(get_db_icon("ghost", &registry), DbIconInfo { label: "DB".to_owned(), bg: "bg-gray-500".to_owned() });
    }

    #[test]
    fn icon_maps_are_returned_as_copies() {
        let registry = registry();
        let mut icons = get_driver_icon_map(&registry);
        icons.insert("new".to_owned(), "url".to_owned());
        assert_eq!(icons.len(), 2);
        assert_eq!(get_driver_icon_map(&registry).len(), 1);
        let mut parents = get_driver_icon_parents(&registry);
        parents.insert("other".to_owned(), "mysql".to_owned());
        assert_eq!(parents.len(), 2);
        assert_eq!(get_driver_icon_parents(&registry).len(), 1);
    }

    #[test]
    fn index_field_clamps_and_rejects_dirty_input() {
        assert_eq!(normalize_index_database_field_default(""), "0");
        assert_eq!(normalize_index_database_field_default("   "), "0");
        assert_eq!(normalize_index_database_field_default("abc"), "0");
        assert_eq!(normalize_index_database_field_default("-1"), "0");
        assert_eq!(normalize_index_database_field_default(" 3 "), "3");
        assert_eq!(normalize_index_database_field_default("99"), "15");
        assert_eq!(crate::normalize_index_database_field("12", 9), "9");
        assert_eq!(crate::normalize_index_database_field("999999999999999999999999", 15), "15");
        assert_eq!(crate::normalize_index_database_field("0", 15), "0");
    }

    #[test]
    fn address_formatting_branches_on_mode_and_ssh() {
        let registry = registry();
        let file = ConnectionAddrInput {
            database_type: "sqlite".to_owned(),
            database: Some("/data/app.db".to_owned()),
            ..Default::default()
        };
        assert_eq!(format_connection_addr(&file, &registry), "/data/app.db");
        assert_eq!(
            format_connection_addr(&ConnectionAddrInput { database: None, ..file }, &registry),
            "SQLite"
        );

        let host = ConnectionAddrInput {
            database_type: "mysql".to_owned(),
            host: Some("db.local".to_owned()),
            database: Some("app".to_owned()),
            ..Default::default()
        };
        assert_eq!(format_connection_addr(&host, &registry), "db.local : app");
        assert_eq!(
            format_connection_addr(&ConnectionAddrInput { host: None, ..host.clone() }, &registry),
            " : app"
        );
        let ssh = ConnectionAddrInput {
            ssh_tunnel: Some(SshTunnelAddr { enabled: true, host: "bastion".to_owned() }),
            ..host
        };
        assert_eq!(format_connection_addr(&ssh, &registry), "bastion → db.local : app");
        let disabled = ConnectionAddrInput {
            ssh_tunnel: Some(SshTunnelAddr { enabled: false, host: "bastion".to_owned() }),
            ..ssh.clone()
        };
        assert_eq!(format_connection_addr(&disabled, &registry), "db.local : app");
    }

    #[test]
    fn register_metas_merges_with_overwrite_semantics() {
        let mut registry = registry();
        assert_eq!(registry.len(), 4);
        registry.register_metas(vec![
            ("mysql".to_owned(), meta("MariaDB", "Ma", "bg-pink")),
            ("new".to_owned(), meta("New", "Ne", "bg-green")),
        ]);
        assert_eq!(registry.len(), 5);
        assert_eq!(get_db_label("mysql", &registry), "MariaDB");
        assert_eq!(get_db_icon("new", &registry).label, "Ne");
        assert_eq!(escape_ident("x", Some("mysql"), &registry), "\"x\"", "the replacement meta is used");
        assert_eq!(get_driver_icon_map(&registry).len(), 1, "icons are unaffected");
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::sort_db_types_by_popularity::<String>(&[]);
        let _ = crate::escape_ident;
        let _ = crate::default_escape_sql_value;
        let _ = crate::get_escape_sql_value;
        let _ = crate::get_db_label;
        let _ = crate::get_db_icon;
        let _ = crate::get_driver_icon_map;
        let _ = crate::get_driver_icon_parents;
        let _ = crate::normalize_index_database_field;
        let _ = crate::normalize_index_database_field_default;
        let _ = crate::format_connection_addr;
        let _ = crate::DB_TYPE_POPULARITY_ORDER;
        assert_eq!(crate::DEFAULT_MAX_DATABASE_INDEX, 15);
        let _: Option<crate::DatabaseRegistry> = None;
        let _: Option<crate::DatabaseTypeMeta> = None;
        let _: Option<crate::DbIconInfo> = None;
        let _: Option<crate::EscapeSqlValue> = None;
        let _: Option<crate::ConnectionAddrInput> = None;
        let _: Option<crate::SshTunnelAddr> = None;
    }
}
