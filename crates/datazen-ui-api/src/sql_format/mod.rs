#[cfg(test)]
mod tests;

use std::sync::OnceLock;

pub const LANGUAGE_MAP: [(&str, &str); 6] = [
    ("postgresql", "postgresql"),
    ("mysql", "mysql"),
    ("mariadb", "mariadb"),
    ("sqlite", "sqlite"),
    ("sqlserver", "transactsql"),
    ("tsql", "transactsql"),
];

pub const DEFAULT_KEYWORD_CASE: &str = "upper";
pub const DEFAULT_INDENT_STYLE: &str = "2spaces";
pub const DEFAULT_BREAK_BEFORE_BOOLEAN_OPERATORS: bool = true;
pub const DEFAULT_LINES_BETWEEN_QUERIES: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlFormatConfig {
    pub keyword_case: String,
    pub indent_style: String,
    pub break_before_boolean_operators: bool,
    pub lines_between_queries: u32,
}

impl SqlFormatConfig {
    pub fn from_wire(options: &crate::SqlFormatOptions) -> Option<Self> {
        serde_json::to_value(options)
            .ok()
            .and_then(|value| serde_json::from_value(value).ok())
    }
}

pub fn default_sql_format_options() -> &'static SqlFormatConfig {
    static DEFAULT: OnceLock<SqlFormatConfig> = OnceLock::new();
    DEFAULT.get_or_init(|| SqlFormatConfig {
        keyword_case: DEFAULT_KEYWORD_CASE.to_owned(),
        indent_style: DEFAULT_INDENT_STYLE.to_owned(),
        break_before_boolean_operators: DEFAULT_BREAK_BEFORE_BOOLEAN_OPERATORS,
        lines_between_queries: DEFAULT_LINES_BETWEEN_QUERIES,
    })
}

fn map_language(key: &str) -> Option<&'static str> {
    LANGUAGE_MAP
        .iter()
        .find(|(candidate, _)| *candidate == key)
        .map(|(_, language)| *language)
}

pub type SqlDialectLookup = dyn Fn(&str) -> Option<String>;

pub fn sql_format_language(
    database_type: Option<&str>,
    dialect_lookup: Option<&SqlDialectLookup>,
) -> String {
    let database_type = match database_type {
        Some(value) if !value.is_empty() => value,
        _ => return "sql".to_owned(),
    };
    if let Some(language) = map_language(database_type) {
        return language.to_owned();
    }
    if let Some(lookup) = dialect_lookup {
        if let Some(dialect) = lookup(database_type) {
            if let Some(language) = map_language(&dialect) {
                return language.to_owned();
            }
        }
    }
    "sql".to_owned()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlIndent {
    pub tab_width: u32,
    pub use_tabs: bool,
}

pub fn sql_format_indentation(style: &str) -> SqlIndent {
    match style {
        "tab" => SqlIndent {
            tab_width: 1,
            use_tabs: true,
        },
        "4spaces" => SqlIndent {
            tab_width: 4,
            use_tabs: false,
        },
        _ => SqlIndent {
            tab_width: 2,
            use_tabs: false,
        },
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SqlFormatRequest {
    pub language: String,
    pub keyword_case: String,
    pub indent_style: String,
    pub tab_width: u32,
    pub use_tabs: bool,
    pub logical_operator_newline: String,
    pub lines_between_queries: u32,
}

pub type SqlFormatter = dyn Fn(&str, &SqlFormatRequest) -> String;

pub fn format_sql(
    sql: &str,
    database_type: Option<&str>,
    options: Option<&SqlFormatConfig>,
    dialect_lookup: Option<&SqlDialectLookup>,
    formatter: &SqlFormatter,
) -> String {
    let trimmed = sql.trim();
    if trimmed.is_empty() {
        return sql.to_owned();
    }
    let defaults = default_sql_format_options();
    let resolved = options.unwrap_or(defaults);
    let indent = sql_format_indentation(&resolved.indent_style);
    let request = SqlFormatRequest {
        language: sql_format_language(database_type, dialect_lookup),
        keyword_case: resolved.keyword_case.clone(),
        indent_style: "standard".to_owned(),
        tab_width: indent.tab_width,
        use_tabs: indent.use_tabs,
        logical_operator_newline: if resolved.break_before_boolean_operators {
            "before".to_owned()
        } else {
            "after".to_owned()
        },
        lines_between_queries: resolved.lines_between_queries,
    };
    formatter(trimmed, &request)
}
