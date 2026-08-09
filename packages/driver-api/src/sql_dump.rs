//! Generic SQL dump helpers used by [`crate::DatabaseDriver`] defaults.
//!
//! Does **not** emit `CREATE DATABASE` — drivers that support that option
//! prepend their dialect preamble in `dump_database` before calling
//! [`dump_sql_database`].

use crate::traits::DatabaseDriver;
use crate::types::*;

/// Build `CREATE TABLE IF NOT EXISTS` DDL from a table schema (host-compatible).
pub fn build_create_table_sql(
    quote_ident: &dyn Fn(&str) -> String,
    schema: &TableSchema,
) -> String {
    let tname = &schema.table_name;
    let cols_sql: Vec<String> = schema
        .columns
        .iter()
        .map(|c| {
            let mut def = format!("  {} {}", quote_ident(&c.name), c.data_type);
            if !c.nullable {
                def.push_str(" NOT NULL");
            }
            if let Some(ref dv) = c.default_value {
                def.push_str(&format!(" DEFAULT {}", dv));
            }
            def
        })
        .collect();

    let mut create = format!(
        "CREATE TABLE IF NOT EXISTS {} (\n{}",
        quote_ident(tname),
        cols_sql.join(",\n")
    );
    if !schema.primary_keys.is_empty() {
        let pks: Vec<String> = schema.primary_keys.iter().map(|k| quote_ident(k)).collect();
        create.push_str(&format!(",\n  PRIMARY KEY ({})", pks.join(", ")));
    }
    create.push_str("\n);\n");
    create
}

/// Default `dump_table_ddl`: load schema then build CREATE TABLE.
pub async fn dump_table_ddl_from_schema<D>(
    driver: &D,
    handle: &ConnectionHandle,
    table: &str,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let schema = driver.get_table_schema(handle, table).await?;
    Ok(build_create_table_sql(&|n| driver.quote_ident(n), &schema))
}

/// Dump a database to SQL (header, optional DROP, DDL via `dump_table_ddl`, INSERTs).
///
/// Does not emit `CREATE DATABASE`.
pub async fn dump_sql_database<D>(
    driver: &D,
    handle: &ConnectionHandle,
    database: &str,
    opts: &BackupDumpOptions,
) -> Result<String, DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let tables = driver.get_tables(handle, database).await?;

    let mut out = String::new();
    out.push_str(&format!("-- DataZen backup: {}\n", database));
    out.push_str(&format!("-- Date: {}\n", chrono::Utc::now().to_rfc3339()));
    let mut opt_flags = Vec::new();
    if opts.schema_only {
        opt_flags.push("schema-only");
    }
    if opts.data_only {
        opt_flags.push("data-only");
    }
    if opts.clean {
        opt_flags.push("clean");
    }
    if opts.create_database {
        opt_flags.push("create");
    }
    if !opt_flags.is_empty() {
        out.push_str(&format!("-- Options: {}\n", opt_flags.join(", ")));
    }
    out.push('\n');

    for table in &tables {
        let tname = &table.name;
        out.push_str(&format!("-- Table: {}\n", tname));

        if opts.clean {
            out.push_str(&format!(
                "DROP TABLE IF EXISTS {};\n",
                driver.quote_ident(tname)
            ));
        }

        if !opts.data_only {
            let ddl = driver.dump_table_ddl(handle, tname).await?;
            out.push_str(&ddl);
            if !ddl.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }

        if !opts.schema_only {
            let schema = driver.get_table_schema(handle, tname).await?;
            let col_names: Vec<String> = schema
                .columns
                .iter()
                .map(|c| driver.quote_ident(&c.name))
                .collect();
            let select_sql = format!(
                "SELECT {} FROM {}",
                col_names.join(", "),
                driver.quote_ident(tname)
            );

            match driver.query(handle, &select_sql).await {
                Ok(result) => {
                    for row in &result.rows {
                        let vals: Vec<String> = row
                            .iter()
                            .map(|v| driver.format_sql_literal(v))
                            .collect();
                        out.push_str(&format!(
                            "INSERT INTO {} ({}) VALUES ({});\n",
                            driver.quote_ident(tname),
                            col_names.join(", "),
                            vals.join(", ")
                        ));
                    }
                    out.push('\n');
                }
                Err(e) => {
                    out.push_str(&format!("-- Error dumping data for {tname}: {e}\n\n"));
                }
            }
        }
    }

    Ok(out)
}

/// Default restore: split on `;`, execute each non-empty / non-comment statement.
pub async fn restore_sql_statements<D>(
    driver: &D,
    handle: &ConnectionHandle,
    sql: &str,
) -> Result<(), DriverError>
where
    D: DatabaseDriver + ?Sized,
{
    let statements: Vec<&str> = sql
        .split(';')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.starts_with("--"))
        .collect();

    let mut errors = Vec::new();
    for stmt in &statements {
        let full = format!("{};", stmt);
        if let Err(e) = driver.execute(handle, &full).await {
            let max = 80;
            let end = if stmt.len() <= max {
                stmt.len()
            } else {
                let mut e = max;
                while e > 0 && !stmt.is_char_boundary(e) {
                    e -= 1;
                }
                e
            };
            errors.push(format!("Error executing: {}... -> {e}", &stmt[..end]));
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(DriverError::QueryFailed(format!(
            "Partial restore failure ({}/{} statements failed):\n{}",
            errors.len(),
            statements.len(),
            errors.join("\n")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_create_table_sql_includes_pk_and_not_null() {
        let schema = TableSchema {
            table_name: "users".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                },
                ColumnSchema {
                    name: "name".into(),
                    data_type: "text".into(),
                    nullable: true,
                    default_value: Some("'anon'".into()),
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
        };
        let sql = build_create_table_sql(&|n| format!("\"{}\"", n), &schema);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS \"users\""));
        assert!(sql.contains("\"id\" integer NOT NULL"));
        assert!(sql.contains("DEFAULT 'anon'"));
        assert!(sql.contains("PRIMARY KEY (\"id\")"));
        assert!(sql.ends_with(";\n"));
    }
}
