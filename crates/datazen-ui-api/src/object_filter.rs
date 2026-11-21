use serde::Serialize;
use serde_json::Value;

use crate::{ConnectionConfig, TableInfo, TableType};

const SYSTEM_SCHEMAS: [&str; 6] = [
    "information_schema",
    "pg_catalog",
    "pg_toast",
    "mysql",
    "performance_schema",
    "sys",
];

const SYSTEM_DATABASES: [&str; 7] = [
    "information_schema",
    "mysql",
    "performance_schema",
    "sys",
    "postgres",
    "template0",
    "template1",
];

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectFilterPrefs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide_system_schemas: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_name_include: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub table_name_exclude: Option<String>,
}

pub fn get_object_filter(conn: &ConnectionConfig) -> ObjectFilterPrefs {
    let Some(raw) = conn.options.as_ref().and_then(|options| options.get("objectFilter")) else {
        return ObjectFilterPrefs::default();
    };
    let Some(record) = raw.as_object() else {
        return ObjectFilterPrefs::default();
    };
    let flag = record.get("hideSystemSchemas").and_then(Value::as_bool);
    ObjectFilterPrefs {
        hide_system_schemas: flag,
        table_name_include: record.get("tableNameInclude").and_then(Value::as_str).map(str::to_owned),
        table_name_exclude: record.get("tableNameExclude").and_then(Value::as_str).map(str::to_owned),
    }
}

pub fn is_system_schema_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    SYSTEM_SCHEMAS.contains(&lower.as_str()) || lower.starts_with("pg_")
}

pub fn is_system_database_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    SYSTEM_DATABASES.contains(&lower.as_str())
}

fn glob_match(text: &str, pattern: &str) -> bool {
    let text: Vec<char> = text.chars().collect();
    let pattern: Vec<char> = pattern.chars().collect();
    let (mut text_index, mut pattern_index) = (0usize, 0usize);
    let mut star: Option<usize> = None;
    let mut retry = 0usize;
    while text_index < text.len() {
        if pattern_index < pattern.len() && pattern[pattern_index] == text[text_index] {
            text_index += 1;
            pattern_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some(pattern_index);
            pattern_index += 1;
            retry = text_index;
        } else if let Some(star_index) = star {
            pattern_index = star_index + 1;
            retry += 1;
            text_index = retry;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

pub fn match_name_pattern(name: &str, pattern: &str) -> bool {
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return true;
    }
    let name = name.to_lowercase();
    let lowered = pattern.to_lowercase();
    if lowered.contains('*') {
        glob_match(&name, &lowered)
    } else {
        name.contains(&lowered)
    }
}

pub fn matches_table_name_filter(name: &str, filter: &ObjectFilterPrefs) -> bool {
    let include = filter.table_name_include.as_deref().map(str::trim).filter(|value| !value.is_empty());
    let exclude = filter.table_name_exclude.as_deref().map(str::trim).filter(|value| !value.is_empty());
    if let Some(include) = include {
        if !match_name_pattern(name, include) {
            return false;
        }
    }
    if let Some(exclude) = exclude {
        if match_name_pattern(name, exclude) {
            return false;
        }
    }
    true
}

pub fn should_show_schema(schema_name: &str, filter: &ObjectFilterPrefs) -> bool {
    !(filter.hide_system_schemas == Some(true) && is_system_schema_name(schema_name))
}

pub fn should_show_database(db_name: &str, filter: &ObjectFilterPrefs) -> bool {
    !(filter.hide_system_schemas == Some(true) && is_system_database_name(db_name))
}

pub fn filter_table_items(items: &[TableInfo], filter: &ObjectFilterPrefs) -> Vec<TableInfo> {
    let hide = filter.hide_system_schemas == Some(true);
    items
        .iter()
        .filter(|item| {
            if hide && item.table_type == TableType::SystemTable {
                return false;
            }
            if hide && item.schema.as_deref().is_some_and(is_system_schema_name) {
                return false;
            }
            matches_table_name_filter(&item.name, filter)
        })
        .cloned()
        .collect()
}

pub fn with_object_filter_options(conn: &ConnectionConfig, filter: &ObjectFilterPrefs) -> ConnectionConfig {
    let mut out = conn.clone();
    let options = out.options.get_or_insert_with(serde_json::Map::new);
    if let Ok(value) = serde_json::to_value(filter) {
        options.insert("objectFilter".to_owned(), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn conn_with_options(options: Option<Value>) -> ConnectionConfig {
        ConnectionConfig {
            id: "c1".to_owned(),
            name: "Local".to_owned(),
            database_type: "postgres".to_owned(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: crate::SslMode::Disable,
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
            options: options.and_then(|value| value.as_object().cloned()),
            read_only: None,
            pinned: None,
        }
    }

    fn table(name: &str, schema: Option<&str>, table_type: TableType) -> TableInfo {
        TableInfo {
            name: name.to_owned(),
            schema: schema.map(str::to_owned),
            table_type,
            row_count: None,
        }
    }

    #[test]
    fn prefs_read_only_the_expected_field_shapes() {
        let conn = conn_with_options(Some(json!({"objectFilter": {
            "hideSystemSchemas": true,
            "tableNameInclude": "user*",
            "tableNameExclude": 42
        }})));
        assert_eq!(get_object_filter(&conn), ObjectFilterPrefs {
            hide_system_schemas: Some(true),
            table_name_include: Some("user*".to_owned()),
            table_name_exclude: None,
        });

        let conn = conn_with_options(Some(json!({"objectFilter": []})));
        assert_eq!(get_object_filter(&conn), ObjectFilterPrefs::default());
        let conn = conn_with_options(Some(json!({"objectFilter": "nope"})));
        assert_eq!(get_object_filter(&conn), ObjectFilterPrefs::default());
    }

    #[test]
    fn system_names_are_recognized() {
        for name in ["information_schema", "PG_CATALOG", "pg_toast", "pg_extra", "sys"] {
            assert!(is_system_schema_name(name), "{name}");
        }
        assert!(!is_system_schema_name("public"));
        for name in ["Postgres", "template1", "MYSQL"] {
            assert!(is_system_database_name(name), "{name}");
        }
        assert!(!is_system_database_name("app"));
    }

    #[test]
    fn glob_and_substring_patterns() {
        assert!(match_name_pattern("anything", ""));
        assert!(match_name_pattern("anything", "   "));
        assert!(match_name_pattern("Users", "user"));
        assert!(match_name_pattern("users_table", "users*"));
        assert!(match_name_pattern("users", "*ers"));
        assert!(match_name_pattern("users", "u*s"));
        assert!(!match_name_pattern("users", "u*r"));
        assert!(match_name_pattern("a.b", "a.b"));
        assert!(!match_name_pattern("axb", "a.b"), "only * is a wildcard");
        assert!(match_name_pattern("abc", "***"));
    }

    #[test]
    fn include_then_exclude_decide_table_matching() {
        let filter = ObjectFilterPrefs {
            table_name_include: Some("user*".to_owned()),
            table_name_exclude: Some("*archive*".to_owned()),
            ..Default::default()
        };
        assert!(matches_table_name_filter("users", &filter));
        assert!(!matches_table_name_filter("orders", &filter));
        assert!(!matches_table_name_filter("users_archive", &filter));
        let empty = ObjectFilterPrefs::default();
        assert!(matches_table_name_filter("orders", &empty));
    }

    #[test]
    fn schema_database_and_table_visibility_follow_hide_flag() {
        let hidden = ObjectFilterPrefs { hide_system_schemas: Some(true), ..Default::default() };
        assert!(!should_show_schema("pg_catalog", &hidden));
        assert!(!should_show_database("postgres", &hidden));
        assert!(should_show_schema("public", &hidden));
        assert!(should_show_database("app", &hidden));
        let shown = ObjectFilterPrefs::default();
        assert!(should_show_schema("pg_catalog", &shown));
    }

    #[test]
    fn table_items_filter_on_type_schema_and_name() {
        let items = vec![
            table("users", Some("public"), TableType::Table),
            table("pg_stat", Some("pg_catalog"), TableType::Table),
            table("internal", None, TableType::SystemTable),
            table("orders", Some("public"), TableType::View),
        ];
        let filter = ObjectFilterPrefs {
            hide_system_schemas: Some(true),
            table_name_exclude: Some("orders".to_owned()),
            ..Default::default()
        };
        let kept = filter_table_items(&items, &filter);
        assert_eq!(kept.iter().map(|item| item.name.as_str()).collect::<Vec<_>>(), vec!["users"]);
    }

    #[test]
    fn writing_prefs_preserves_other_options_and_drops_absent_fields() {
        let conn = conn_with_options(Some(json!({"other": 1})));
        let filter = ObjectFilterPrefs { hide_system_schemas: Some(false), ..Default::default() };
        let updated = with_object_filter_options(&conn, &filter);
        let options = updated.options.expect("options");
        assert_eq!(options.get("other"), Some(&json!(1)));
        assert_eq!(options.get("objectFilter"), Some(&json!({"hideSystemSchemas": false})));
        assert_eq!(conn.options.as_ref().and_then(|o| o.get("objectFilter")), None);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::get_object_filter;
        let _ = crate::is_system_schema_name;
        let _ = crate::is_system_database_name;
        let _ = crate::match_name_pattern;
        let _ = crate::matches_table_name_filter;
        let _ = crate::should_show_schema;
        let _ = crate::should_show_database;
        let _ = crate::filter_table_items;
        let _ = crate::with_object_filter_options;
        let _: Option<crate::ObjectFilterPrefs> = None;
    }
}
