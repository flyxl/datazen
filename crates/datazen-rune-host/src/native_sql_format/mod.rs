//! Native formatting worker provider. Only Rust algorithms and typed SQL
//! options cross this boundary; formatted SQL is never executed.

use datazen_sql_formatter::{Dialect, Indent, KeywordCase, LogicalOperatorNewline, Options};
use datazen_ui_api::{native_sql_format::NativeSqlFormatLanguage, SqlFormatRequest};

pub(crate) use datazen_sql_formatter::Error as NativeSqlFormatError;
pub(crate) mod worker;

const INPUT_LIMIT: usize = datazen_sql_formatter::MAX_INPUT_BYTES;
const OUTPUT_LIMIT: usize = datazen_sql_formatter::MAX_OUTPUT_BYTES;

fn dialect(language: NativeSqlFormatLanguage) -> Dialect {
    match language {
        NativeSqlFormatLanguage::Sql => Dialect::Sql,
        NativeSqlFormatLanguage::Sqlite => Dialect::Sqlite,
        NativeSqlFormatLanguage::PostgreSql => Dialect::PostgreSql,
        NativeSqlFormatLanguage::MySql => Dialect::MySql,
        NativeSqlFormatLanguage::MariaDb => Dialect::MariaDb,
        NativeSqlFormatLanguage::TransactSql => Dialect::TransactSql,
    }
}

pub(crate) fn supports_language(language: NativeSqlFormatLanguage) -> bool {
    datazen_sql_formatter::supports_dialect(dialect(language))
}

pub(crate) fn format_native_sql(
    sql: &str, request: &SqlFormatRequest,
) -> Result<String, NativeSqlFormatError> {
    use NativeSqlFormatError::InvalidOptions;

    if request.indent_style != "standard" {
        return Err(InvalidOptions);
    }
    let language = match request.language.as_str() {
        "sql" => NativeSqlFormatLanguage::Sql,
        "sqlite" => NativeSqlFormatLanguage::Sqlite,
        "postgresql" => NativeSqlFormatLanguage::PostgreSql,
        "mysql" => NativeSqlFormatLanguage::MySql,
        "mariadb" => NativeSqlFormatLanguage::MariaDb,
        "transactsql" => NativeSqlFormatLanguage::TransactSql,
        _ => return Err(InvalidOptions),
    };
    let options = Options {
        dialect: dialect(language),
        keyword_case: match request.keyword_case.as_str() {
            "preserve" => KeywordCase::Preserve,
            "upper" => KeywordCase::Upper,
            "lower" => KeywordCase::Lower,
            _ => return Err(InvalidOptions),
        },
        indent: match (request.tab_width, request.use_tabs) {
            (1, true) => Indent::Tabs,
            (2 | 4, false) => Indent::Spaces(request.tab_width as u8),
            _ => return Err(InvalidOptions),
        },
        logical_operator_newline: match request.logical_operator_newline.as_str() {
            "before" => LogicalOperatorNewline::Before,
            "after" => LogicalOperatorNewline::After,
            _ => return Err(InvalidOptions),
        },
        lines_between_queries: u8::try_from(request.lines_between_queries).map_err(|_| InvalidOptions)?,
    };
    datazen_sql_formatter::format(sql, &options)
}
