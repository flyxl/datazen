//! SQL parsing helpers for PostgreSQL driver.

use datazen_driver_api::{sql_dump, QueryResult, Value};

/// Split `schema.table` into `(Some(schema), table)`. Unqualified names use `None`.
pub(crate) fn parse_pg_table_ref(table: &str) -> (Option<&str>, &str) {
    match table.split_once('.') {
        Some((schema, name)) if !schema.is_empty() && !name.is_empty() => (Some(schema), name),
        _ => (None, table),
    }
}

/// Qualified relation name, ready to bind into a `$1::regclass` cast.
///
/// Both components are quoted. An unquoted `schema.table` handed to `regclass`
/// is parsed as a SQL identifier path, so PostgreSQL folds it to lower case:
/// binding `Reporting.orders` failed with `schema "reporting" does not exist`
/// for every schema or table that was not already all lower case. Quoting makes
/// the value mean exactly the name it spells, and is a no-op for names that are
/// lower case anyway.
///
/// Every caller binds this into `$1::regclass` — it is never interpolated into
/// SQL text — so quoting here cannot double-quote at a use site.
pub(crate) fn pg_regclass_name(schema: Option<&str>, table: &str) -> String {
    fn quote(name: &str) -> String {
        format!("\"{}\"", name.replace('"', "\"\""))
    }
    match schema {
        Some(s) => format!("{}.{}", quote(s), quote(table)),
        None => quote(table),
    }
}

/// Effective `(schema, bare_table)` for a single-table call.
///
/// The explicit `schema` argument wins; a schema embedded in the table
/// reference (`"public"."orders"`) is still honoured for callers that pass a
/// pre-qualified name. Blank schemas are treated as absent so an empty string
/// can never silently mean "whatever the session resolves".
pub(crate) fn resolve_pg_table_schema<'a>(
    table: &'a str,
    schema: Option<&'a str>,
) -> (Option<&'a str>, &'a str) {
    let (embedded, bare) = parse_pg_table_ref(table);
    let explicit = schema.map(str::trim).filter(|s| !s.is_empty());
    (explicit.or(embedded), bare)
}

/// The statement that estimates how many rows a table has.
///
/// PostgreSQL keeps the planner's row estimate in `pg_class.reltuples`. It is
/// a catalog statistic rather than a count: it is close to the truth once the
/// planner has looked at the table, and `-1` for a table that was never
/// analysed — a value the host reads as "no estimate available" rather than
/// as a row count.
///
/// The relation is located by its schema and its name, both written as
/// escaped literals. It is deliberately **not** located by a session
/// dependent function (`current_database()`, `DATABASE()`): the session may
/// sit on another database than the one that was asked for. In PostgreSQL the
/// database dimension of a target is the pool the statement runs on, which
/// the host resolves before the statement is sent — so the statement itself
/// only has to name the relation.
///
/// The catalog relations are written fully qualified (`pg_catalog.pg_class`)
/// because the driver qualifies every unqualified relation reference with the
/// target schema, which would turn them into `"public"."pg_class"`.
pub(crate) fn count_estimate_sql(
    table: &str,
    schema: Option<&str>,
    quote_literal: &dyn Fn(&str) -> String,
) -> Option<String> {
    let (schema, table) = resolve_pg_table_schema(table, schema);
    let table_literal = quote_literal(table);
    let schema_clause = match schema {
        Some(schema) => {
            let literal = quote_literal(schema);
            format!(" AND n.nspname = {literal}")
        }
        None => String::new(),
    };
    Some(format!(
        "SELECT c.reltuples::bigint FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         WHERE c.relname = {table_literal}{schema_clause}"
    ))
}

pub(crate) fn is_pg_result_query(sql: &str) -> bool {
    let upper = sql.trim().to_ascii_uppercase();
    upper.starts_with("SELECT")
        || upper.starts_with("WITH")
        || upper.starts_with("SHOW")
        || upper.starts_with("EXPLAIN")
}

pub(crate) fn collect_named_ddl_column(
    result: &QueryResult,
    col_name: &str,
    kind_label: &str,
) -> String {
    let ddl_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case(col_name));
    let name_idx = result
        .columns
        .iter()
        .position(|c| c.name.eq_ignore_ascii_case("name"));
    let Some(ddl_idx) = ddl_idx else {
        return String::new();
    };
    let mut out = String::new();
    for row in &result.rows {
        let Some(Value::String(ddl)) = row.get(ddl_idx).and_then(|v| v.as_ref()) else {
            continue;
        };
        if ddl.trim().is_empty() {
            continue;
        }
        if let Some(name_idx) = name_idx {
            if let Some(Value::String(name)) = row.get(name_idx).and_then(|v| v.as_ref()) {
                out.push_str(&format!("-- {kind_label}: {name}\n"));
            }
        }
        let trimmed = ddl.trim_end();
        out.push_str(trimmed);
        if !trimmed.ends_with(';') {
            out.push(';');
        }
        out.push_str("\n\n");
    }
    out
}

/// If the statement is a SELECT without an existing LIMIT clause, returns a
/// modified SQL with `LIMIT limit+1` appended (the extra row lets us detect
/// truncation).  If the statement already has a LIMIT, the SQL is unchanged
/// but the cap is still returned so the caller can truncate over-limit results.
pub(crate) fn apply_select_limit(stmt: &str, limit: Option<u32>) -> (String, Option<u32>) {
    let Some(lim) = limit else {
        return (stmt.to_string(), None);
    };

    let trimmed = stmt.trim();
    let upper = trimmed.to_ascii_uppercase();
    let is_select = upper.starts_with("SELECT") || upper.starts_with("WITH");
    if !is_select {
        return (stmt.to_string(), None);
    }

    if has_top_level_limit(trimmed) {
        return (stmt.to_string(), Some(lim));
    }

    let effective = format!("{} LIMIT {}", trimmed, lim + 1);
    (effective, Some(lim))
}

/// Rough heuristic: scan the SQL outside of string literals, dollar-quotes,
/// and parenthesised sub-expressions for the keyword `LIMIT`.
pub(crate) fn has_top_level_limit(sql: &str) -> bool {
    let bytes = sql.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut depth: i32 = 0; // parenthesis nesting

    while i < len {
        match bytes[i] {
            b'\'' => {
                i += 1;
                while i < len {
                    if bytes[i] == b'\'' {
                        i += 1;
                        if i < len && bytes[i] == b'\'' {
                            i += 1; // escaped quote
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            b'"' => {
                i += 1;
                while i < len && bytes[i] != b'"' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
            }
            b'$' => {
                if let Some(tag_end) = sql_dump::find_dollar_tag(bytes, i) {
                    let tag = &sql[i..tag_end];
                    i = tag_end;
                    loop {
                        if i >= len {
                            break;
                        }
                        if bytes[i] == b'$' {
                            if sql[i..].starts_with(tag) {
                                i += tag.len();
                                break;
                            }
                        }
                        i += 1;
                    }
                } else {
                    i += 1;
                }
            }
            b'-' if i + 1 < len && bytes[i + 1] == b'-' => {
                i += 2;
                while i < len && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if i + 1 < len && bytes[i + 1] == b'*' => {
                i += 2;
                let mut cd = 1i32;
                while i + 1 < len && cd > 0 {
                    if bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        cd += 1;
                        i += 2;
                    } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        cd -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
            }
            b'(' => {
                depth += 1;
                i += 1;
            }
            b')' => {
                depth -= 1;
                i += 1;
            }
            b'L' | b'l' if depth == 0 => {
                if i + 5 <= len
                    && sql[i..i + 5].eq_ignore_ascii_case("LIMIT")
                    && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric())
                    && (i + 5 >= len || !bytes[i + 5].is_ascii_alphanumeric())
                {
                    return true;
                }
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regclass_name_quotes_both_components() {
        // Lower case: the shape the driver has always produced, unchanged in
        // meaning. Quoting is invisible to PostgreSQL here.
        assert_eq!(
            pg_regclass_name(Some("public"), "orders"),
            "\"public\".\"orders\""
        );
    }

    #[test]
    fn regclass_name_preserves_case() {
        // The bug: an unquoted path is case-folded, so this used to reach
        // PostgreSQL as `reporting.orders` and resolve to the wrong schema — or
        // fail outright with `schema "reporting" does not exist`.
        assert_eq!(
            pg_regclass_name(Some("Reporting"), "Orders"),
            "\"Reporting\".\"Orders\""
        );
    }

    #[test]
    fn regclass_name_without_schema_still_quotes() {
        assert_eq!(pg_regclass_name(None, "MixedCase"), "\"MixedCase\"");
    }

    #[test]
    fn regclass_name_doubles_embedded_quote() {
        // A literal double quote would otherwise terminate the identifier early.
        assert_eq!(
            pg_regclass_name(Some("we\"ird"), "ta\"ble"),
            "\"we\"\"ird\".\"ta\"\"ble\""
        );
    }

    #[test]
    fn explicit_schema_beats_one_embedded_in_the_table_name() {
        assert_eq!(
            resolve_pg_table_schema("other.orders", Some("public")),
            (Some("public"), "orders")
        );
    }

    #[test]
    fn a_blank_explicit_schema_never_means_whatever_the_session_resolves() {
        assert_eq!(
            resolve_pg_table_schema("orders", Some("   ")),
            (None, "orders")
        );
    }

    /// The literal formatter of the driver, so the tests exercise the same
    /// escaping the statement really uses.
    fn quote_literal(value: &str) -> String {
        match &datazen_driver_api::Value::String(value.to_string()) {
            datazen_driver_api::Value::String(text) => format!("'{}'", text.replace('\'', "''")),
            _ => unreachable!("a string stays a string"),
        }
    }

    #[test]
    fn count_estimate_sql_names_the_relation_it_estimates() {
        let sql = count_estimate_sql("orders", Some("public"), &quote_literal)
            .expect("the driver has an estimate");
        assert!(sql.contains("pg_catalog.pg_class"), "{sql}");
        assert!(sql.contains("c.relname = 'orders'"), "{sql}");
        assert!(sql.contains("n.nspname = 'public'"), "{sql}");
        assert!(sql.contains("reltuples"), "{sql}");
    }

    #[test]
    fn count_estimate_sql_escapes_the_names_it_writes() {
        let sql = count_estimate_sql("or'ders", Some("pub'lic"), &quote_literal)
            .expect("the driver has an estimate");
        assert!(sql.contains("c.relname = 'or''ders'"), "{sql}");
        assert!(sql.contains("n.nspname = 'pub''lic'"), "{sql}");
        // No bare quote survives: the statement cannot be broken out of.
        assert!(!sql.contains("or'ders"), "{sql}");
    }

    #[test]
    fn count_estimate_sql_does_not_ask_the_session_which_database_it_is_on() {
        let sql = count_estimate_sql("orders", Some("public"), &quote_literal)
            .expect("the driver has an estimate");
        // The database dimension of a PostgreSQL target is the pool the
        // statement runs on; a session-dependent function would answer for
        // whatever database the session happens to sit on.
        for probe in ["current_database()", "DATABASE()", "DB_NAME()"] {
            assert!(!sql.contains(probe), "{probe} must not appear in: {sql}");
        }
    }

    #[test]
    fn count_estimate_sql_keeps_the_catalog_relations_qualified() {
        // The driver qualifies every unqualified relation reference with the
        // target schema, so a bare `pg_class` would become
        // `"public"."pg_class"`.
        let sql = count_estimate_sql("orders", Some("public"), &quote_literal)
            .expect("the driver has an estimate");
        assert!(sql.contains("pg_catalog.pg_class"), "{sql}");
        assert!(sql.contains("pg_catalog.pg_namespace"), "{sql}");
    }

    #[test]
    fn count_estimate_sql_survives_the_schema_qualification() {
        let sql = count_estimate_sql("orders", Some("public"), &quote_literal)
            .expect("the driver has an estimate");
        let qualified = crate::sql_target::qualify_sql(&sql, Some("app"), Some("public"));
        // The rewrite may restate the statement, but it must not move the
        // catalog relations into the target schema: that would name a table
        // that does not exist.
        assert!(
            qualified.contains("pg_catalog.pg_class"),
            "the catalog relation was moved: {qualified}"
        );
        assert!(
            qualified.contains("pg_catalog.pg_namespace"),
            "the catalog relation was moved: {qualified}"
        );
        assert!(
            !qualified.contains("\"public\".\"pg_class\""),
            "the catalog relation was moved: {qualified}"
        );
        assert!(
            qualified.contains("c.relname = 'orders'"),
            "the relation was lost: {qualified}"
        );
    }

    #[test]
    fn count_estimate_sql_without_a_schema_names_only_the_table() {
        let sql =
            count_estimate_sql("orders", None, &quote_literal).expect("the driver has an estimate");
        assert!(sql.contains("c.relname = 'orders'"), "{sql}");
        assert!(!sql.contains("nspname"), "{sql}");
    }
}
