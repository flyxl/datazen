//! Owned job boundary for the optional, local Native Query formatter.
use std::collections::BTreeMap;

use serde_json::Value;

use crate::{
    get_action_shortcut, sql_format_indentation, to_code_mirror_key_format, KeymapAction,
    NativeQueryDatabase, SqlFormatOptions, SqlFormatRequest,
};

pub const NATIVE_SQL_FORMAT_MAX_INPUT_BYTES: usize = 256 * 1024;
pub const NATIVE_SQL_FORMAT_MAX_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSqlFormatLanguage {
    Sql,
    PostgreSql,
    MySql,
    MariaDb,
    Sqlite,
    TransactSql,
}

impl NativeSqlFormatLanguage {
    /// Match React's exact registered driver/name-first lookup. MariaDB's
    /// direct language wins over a registered MySQL family fallback.
    pub fn for_driver(driver: &str, registered_dialect: Option<&str>) -> Self {
        fn lookup(key: &str) -> Option<NativeSqlFormatLanguage> {
            use NativeSqlFormatLanguage::*;
            Some(match crate::sql_format::LANGUAGE_MAP.iter()
                .find(|(name, _)| *name == key)?.1
            {
                "postgresql" => PostgreSql,
                "mysql" => MySql,
                "mariadb" => MariaDb,
                "sqlite" => Sqlite,
                "transactsql" => TransactSql,
                _ => return None,
            })
        }
        lookup(driver).or_else(|| registered_dialect.and_then(lookup)).unwrap_or(Self::Sql)
    }

    pub fn wire(self) -> &'static str {
        match self {
            Self::Sql => "sql",
            Self::PostgreSql => "postgresql",
            Self::MySql => "mysql",
            Self::MariaDb => "mariadb",
            Self::Sqlite => "sqlite",
            Self::TransactSql => "transactsql",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeSqlFormatContext {
    pub session_generation: u32,
    pub default_target: NativeQueryDatabase,
    pub language: NativeSqlFormatLanguage,
    pub options: SqlFormatOptions,
    /// Effective persisted CodeMirror-format chord; None is toolbar-only.
    pub shortcut: Option<String>,
}

impl NativeSqlFormatContext {
    pub fn engine_options(&self) -> SqlFormatRequest {
        let indent = sql_format_indentation(self.options.indent_style.as_str());
        SqlFormatRequest {
            language: self.language.wire().to_owned(),
            keyword_case: self.options.keyword_case.as_str().to_owned(),
            indent_style: "standard".to_owned(),
            tab_width: indent.tab_width,
            use_tabs: indent.use_tabs,
            logical_operator_newline: if self.options.break_before_boolean_operators {
                "before".to_owned()
            } else {
                "after".to_owned()
            },
            lines_between_queries: self.options.lines_between_queries,
        }
    }
}

/// Contains private draft text: deliberately not Debug. The host validates
/// the accepted session/target and rechecks budgets before enqueue/delivery.
/// Never hand this job to a database driver, external process or network.
pub struct NativeSqlFormatRequest {
    pub generation: u64,
    pub session_generation: u32,
    pub target: NativeQueryDatabase,
    /// LF-projected, trimmed UTF-8, already checked nonblank by the editor.
    pub sql: String,
    pub options: SqlFormatRequest,
}

pub struct NativeSqlFormatDelivery {
    pub generation: u64,
    pub result: Result<String, NativeSqlFormatError>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeSqlFormatError {
    Rejected,
    Limit,
    Timeout,
    Cancelled,
    Unavailable,
}

/// Read the same persisted root object as React without changing it. Merge
/// missing options individually. An invalid present type is not an excuse
/// to silently apply a different formatter configuration.
pub fn options_from_settings(document: &Value) -> Option<SqlFormatOptions> {
    let defaults = serde_json::json!({
        "keywordCase": crate::DEFAULT_KEYWORD_CASE,
        "indentStyle": crate::DEFAULT_INDENT_STYLE,
        "breakBeforeBooleanOperators": crate::DEFAULT_BREAK_BEFORE_BOOLEAN_OPERATORS,
        "linesBetweenQueries": crate::DEFAULT_LINES_BETWEEN_QUERIES,
    });
    let raw = match document.get("sqlFormatOptions").filter(|value| !value.is_null()) {
        Some(value) => Some(value.as_object()?),
        None => None,
    };
    let mut merged = defaults.as_object()?.clone();
    if let Some(raw) = raw {
        for key in merged.keys().cloned().collect::<Vec<_>>() {
            if let Some(value) = raw.get(&key) {
                let value = if key == "linesBetweenQueries" && value.as_i64().is_some_and(|n| n < 0) {
                    Value::from(0)
                } else {
                    value.clone()
                };
                merged.insert(key, value);
            }
        }
    }
    serde_json::from_value(Value::Object(merged)).ok()
}

pub fn shortcut_from_settings(document: &Value) -> String {
    let preset = document.get("keymapPreset").and_then(Value::as_str).unwrap_or("default");
    let mut custom = BTreeMap::new();
    if let Some(binding) = document.get("customKeymap").and_then(|map| map.get("formatSql"))
        .and_then(Value::as_str)
    {
        custom.insert("formatSql".to_owned(), binding.to_owned());
    }
    to_code_mirror_key_format(&get_action_shortcut(KeymapAction::FormatSql, preset, &custom))
}

/// ECMAScript String.trim: differs from Rust trim on BOM, NEL and a few
/// controls. The original formatter receives trimmed CodeMirror Text.
pub fn trim_like_react(sql: &str) -> &str {
    sql.trim_matches(|c: char| matches!(c as u32,
        0x0009..=0x000d | 0x0020 | 0x00a0 | 0x1680 | 0x2000..=0x200a
        | 0x2028 | 0x2029 | 0x202f | 0x205f | 0x3000 | 0xfeff))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_formatter_settings_and_provider_request_match_the_wire() {
        use NativeSqlFormatLanguage::*;
        assert_eq!(NativeSqlFormatLanguage::for_driver("mariadb", Some("mysql")), MariaDb);
        assert_eq!(NativeSqlFormatLanguage::for_driver("cloudberry", Some("postgresql")), PostgreSql);
        assert_eq!(NativeSqlFormatLanguage::for_driver("tsql", None), TransactSql);
        assert_eq!(NativeSqlFormatLanguage::for_driver("unknown", None), Sql);
        let mut settings = json!({
            "keymapPreset": "dbeaver",
            "sqlFormatOptions": {"indentStyle": "tab", "breakBeforeBooleanOperators": false},
        });
        assert_eq!(shortcut_from_settings(&settings), "Ctrl-Shift-f");
        settings["customKeymap"] = json!({"formatSql": "Mod+Alt+F8"});
        assert_eq!(shortcut_from_settings(&settings), "Mod-Alt-F8");
        let ctx = NativeSqlFormatContext {
            session_generation: 9,
            default_target: NativeQueryDatabase { connection_id: "test".into(), database: "temporary".into() },
            language: Sqlite,
            options: options_from_settings(&settings).unwrap(),
            shortcut: None,
        };
        assert_eq!(serde_json::to_value(ctx.engine_options()).unwrap(), json!({
            "language": "sqlite", "keywordCase": "upper", "indentStyle": "standard",
            "tabWidth": 1, "useTabs": true, "logicalOperatorNewline": "after", "linesBetweenQueries": 1,
        }));
        settings["sqlFormatOptions"]["linesBetweenQueries"] = json!(-1);
        assert_eq!(options_from_settings(&settings).unwrap().lines_between_queries, 0);
        settings["sqlFormatOptions"]["keywordCase"] = json!("invalid");
        assert!(options_from_settings(&settings).is_none());
        assert_eq!(trim_like_react("\u{feff}\u{a0}select\n"), "select");
        assert_eq!(trim_like_react("\u{85}"), "\u{85}");
    }
}
