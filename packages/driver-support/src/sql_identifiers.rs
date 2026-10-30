//! Shared quoting and relation-name construction for SQL consumers.
//!
//! These functions render names only; they never change session context or
//! execute SQL. `qualify_relation_sql` preserves the legacy family-based
//! migration API. Driver-owned SQL rewrites use `DatabaseDriver::qualified_sql`.

pub fn quote_ident_sql(name: &str, quote: char) -> String {
    if quote == '[' {
        return format!("[{}]", name.replace(']', "]]"));
    }
    let doubled = name.replace(quote, &format!("{quote}{quote}"));
    format!("{quote}{doubled}{quote}")
}

/// Qualify `table` as `schema.table` when `schema` is non-empty (PostgreSQL etc.).
pub fn qualify_table_sql(schema: Option<&str>, table: &str, quote: char) -> String {
    match schema.map(str::trim).filter(|s| !s.is_empty()) {
        Some(schema) => format!(
            "{}.{}",
            quote_ident_sql(schema, quote),
            quote_ident_sql(table, quote)
        ),
        None => quote_ident_sql(table, quote),
    }
}

/// Qualify a table reference for DML/SELECT without switching the session catalog.
///
/// - MySQL/MariaDB/ClickHouse: `` `database`.`table` `` when `database` is set.
/// - SQL Server: `[database].[schema].[table]` with either qualifier set.
/// - PostgreSQL and similar: `"schema"."table"` when `schema` is set.
/// - Otherwise: bare `table`.
pub fn qualify_relation_sql(
    family: &str,
    database: Option<&str>,
    schema: Option<&str>,
    table: &str,
    quote: char,
) -> String {
    let family = family.to_ascii_lowercase();
    if matches!(family.as_str(), "mysql" | "mariadb" | "clickhouse") {
        return match database.map(str::trim).filter(|s| !s.is_empty()) {
            Some(db) => format!(
                "{}.{}",
                quote_ident_sql(db, quote),
                quote_ident_sql(table, quote)
            ),
            None => quote_ident_sql(table, quote),
        };
    }
    if family == "sqlserver" {
        return [
            database.map(str::trim).filter(|s| !s.is_empty()),
            schema.map(str::trim).filter(|s| !s.is_empty()),
            Some(table),
        ]
        .into_iter()
        .flatten()
        .map(|part| quote_ident_sql(part, quote))
        .collect::<Vec<_>>()
        .join(".");
    }
    qualify_table_sql(schema, table, quote)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_delimiters_are_escaped_as_one_identifier() {
        assert_eq!(quote_ident_sql("a\"b.c", '"'), "\"a\"\"b.c\"");
        assert_eq!(quote_ident_sql("a`b.c", '`'), "`a``b.c`");
        assert_eq!(quote_ident_sql("a]b.c", '['), "[a]]b.c]");
    }

    #[test]
    fn legacy_relation_qualification_keeps_each_namespace_dimension() {
        assert_eq!(
            qualify_relation_sql("MYSQL", Some(" db`name "), Some("ignored"), "t`x", '`'),
            "`db``name`.`t``x`"
        );
        assert_eq!(
            qualify_relation_sql("postgresql", Some("ignored"), Some(" s\"x "), "t", '"'),
            "\"s\"\"x\".\"t\""
        );
        assert_eq!(
            qualify_relation_sql("sqlserver", Some("db]x"), Some("s"), "t]x", '['),
            "[db]]x].[s].[t]]x]"
        );
        assert_eq!(
            qualify_relation_sql("sqlserver", Some(" "), None, "t", '['),
            "[t]"
        );
        assert_eq!(qualify_table_sql(Some(" "), "a.b", '"'), "\"a.b\"");
    }
}
