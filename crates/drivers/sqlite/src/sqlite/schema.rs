use super::SqliteDriver;
use datazen_driver_api::*;
use sqlparser::ast::{ColumnOption, Statement as SqlStatement, TableConstraint};
use sqlparser::dialect::SQLiteDialect;
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer};
use sqlx::Row;
use std::collections::BTreeMap;

fn sqlite_on_conflict_policy_blockers(sql: &str) -> Result<Vec<String>, String> {
    // sqlparser does not consistently expose table-level conflict clauses in
    // its AST. Inspect SQLite's stored DDL tokens so neither column nor table
    // policies can disappear during a rebuild. Tokenizing avoids matching
    // text inside string literals or quoted identifiers.
    let tokens = Tokenizer::new(&SQLiteDialect {}, sql)
        .tokenize()
        .map_err(|error| format!("could not inspect SQLite conflict policies: {error}"))?;
    let significant_tokens = tokens
        .into_iter()
        .filter(|token| !matches!(token, Token::Whitespace(_)))
        .collect::<Vec<_>>();
    let mut blockers = Vec::new();

    for index in 0..significant_tokens.len().saturating_sub(1) {
        if !sqlite_is_unquoted_word(&significant_tokens[index], "ON")
            || !sqlite_is_unquoted_word(&significant_tokens[index + 1], "CONFLICT")
        {
            continue;
        }

        let action = significant_tokens
            .get(index + 2)
            .and_then(|token| match token {
                Token::Word(word) if word.quote_style.is_none() => {
                    Some(word.value.to_ascii_uppercase())
                }
                _ => None,
            });
        let blocker = match action.as_deref() {
            Some(action)
                if matches!(action, "ROLLBACK" | "ABORT" | "FAIL" | "IGNORE" | "REPLACE") =>
            {
                format!("SQLite ON CONFLICT {action} behavior is not represented by Schema Diff")
            }
            _ => "SQLite ON CONFLICT behavior is not represented by Schema Diff".into(),
        };
        blockers.push(blocker);
    }

    blockers.sort();
    blockers.dedup();
    Ok(blockers)
}

fn sqlite_is_unquoted_word(token: &Token, expected: &str) -> bool {
    matches!(token, Token::Word(word) if word.quote_style.is_none() && word.value.eq_ignore_ascii_case(expected))
}

fn parse_sqlite_table_ddl(
    sql: &str,
) -> Result<(Vec<CheckConstraint>, Vec<String>, Vec<String>), String> {
    let conflict_policy_blockers = sqlite_on_conflict_policy_blockers(sql)?;
    let statements = Parser::parse_sql(&SQLiteDialect {}, sql).map_err(|error| {
        let parse_error = format!("could not inspect SQLite table DDL: {error}");
        if conflict_policy_blockers.is_empty() {
            parse_error
        } else {
            format!("{parse_error}; {}", conflict_policy_blockers.join("; "))
        }
    })?;
    if statements.len() != 1 {
        return Err("SQLite table catalog returned an ambiguous DDL statement".into());
    }
    let SqlStatement::CreateTable(table) = &statements[0] else {
        return Err("SQLite table catalog returned non-table DDL".into());
    };

    let mut checks = Vec::new();
    let mut blockers = conflict_policy_blockers;
    let mut auto_increment_columns = Vec::new();
    let mut check_number = 0usize;
    let mut add_check = |name: Option<String>, expression: String| {
        check_number += 1;
        checks.push(CheckConstraint {
            name: name.unwrap_or_else(|| format!("sqlite_check_{check_number}")),
            expression,
        });
    };

    if table.strict {
        blockers.push("STRICT table typing is not represented by Schema Diff".into());
    }
    if table.without_rowid {
        blockers.push("WITHOUT ROWID table storage is not represented by Schema Diff".into());
    }
    for column in &table.columns {
        for option in &column.options {
            match &option.option {
                ColumnOption::Check(expression) => {
                    add_check(
                        option.name.as_ref().map(|name| name.value.clone()),
                        expression.to_string(),
                    );
                }
                ColumnOption::Collation(_) => blockers.push(format!(
                    "Column collation on `{}` is not represented by Schema Diff",
                    column.name.value
                )),
                ColumnOption::Generated { .. } => blockers.push(format!(
                    "Generated column `{}` is not represented by Schema Diff",
                    column.name.value
                )),
                ColumnOption::DialectSpecific(tokens)
                    if tokens
                        .iter()
                        .any(|token| token.to_string().eq_ignore_ascii_case("AUTOINCREMENT")) =>
                {
                    auto_increment_columns.push(column.name.value.clone());
                }
                ColumnOption::ForeignKey {
                    characteristics: Some(characteristics),
                    ..
                } if characteristics.deferrable == Some(true)
                    || characteristics.initially.is_some() =>
                {
                    blockers.push("Deferrable foreign-key behavior is not represented by SQLite schema snapshots".into());
                }
                _ => {}
            }
        }
    }
    for constraint in &table.constraints {
        match constraint {
            TableConstraint::Check { name, expr, .. } => {
                add_check(
                    name.as_ref().map(|name| name.value.clone()),
                    expr.to_string(),
                );
            }
            TableConstraint::ForeignKey {
                characteristics: Some(characteristics),
                ..
            } if characteristics.deferrable == Some(true)
                || characteristics.initially.is_some() =>
            {
                blockers.push(
                    "Deferrable foreign-key behavior is not represented by SQLite schema snapshots"
                        .into(),
                );
            }
            _ => {}
        }
    }
    checks.sort_by(|a, b| a.name.cmp(&b.name));
    blockers.sort();
    blockers.dedup();
    Ok((checks, blockers, auto_increment_columns))
}

fn sqlite_sql_mentions_identifier(sql: &str, identifier: &str) -> bool {
    let Ok(tokens) = sqlparser::tokenizer::Tokenizer::new(&SQLiteDialect {}, sql).tokenize() else {
        // Unknown SQL lexical structure must block the rebuild.
        return true;
    };
    tokens.iter().any(|token| match token {
        sqlparser::tokenizer::Token::Word(word) => word.value.eq_ignore_ascii_case(identifier),
        _ => false,
    })
}

pub(super) async fn get_table_schema(
    driver: &SqliteDriver,
    handle: &ConnectionHandle,
    table: &str,
    database: &str,
    schema: Option<&str>,
) -> Result<TableSchema, DriverError> {
    // SQLite has no schema level: a single-table read must pin no schema.
    validate_schema_target(driver, database, schema, SchemaScope::ExactSchema)?;
    // Resolve against the caller's attached database, never the session's
    // implicit `main`; blank keeps the old `main` fallback.
    let catalog = SqliteDriver::quote_schema(SqliteDriver::effective_database(database));
    let pools = driver.pools.read().await;
    let pool = SqliteDriver::get_pool(&pools, handle)?;

    let table_ddl = sqlx::query_scalar::<_, String>(&format!(
        "SELECT sql FROM {catalog}.sqlite_master WHERE type = 'table' AND name = ?"
    ))
    .bind(table)
    .fetch_optional(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let (check_constraints, mut migration_blockers, auto_increment_columns) = match table_ddl {
        Some(ddl) => parse_sqlite_table_ddl(&ddl)
            .unwrap_or_else(|reason| (Vec::new(), vec![reason], Vec::new())),
        None => (
            Vec::new(),
            vec!["SQLite did not provide the table's original DDL".into()],
            Vec::new(),
        ),
    };

    let col_rows = sqlx::query(&format!(
        "PRAGMA {catalog}.table_xinfo({})",
        driver.quote_ident(table)
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut columns = Vec::new();
    let mut ordered_primary_keys = Vec::new();

    for row in &col_rows {
        let name: String = row.get("name");
        let hidden: i32 = row.try_get("hidden").unwrap_or(0);
        if hidden != 0 {
            migration_blockers.push(format!(
                "Hidden or generated column `{name}` is not represented by Schema Diff"
            ));
            continue;
        }
        let data_type: String = row.get("type");
        let notnull: bool = row.get::<i32, _>("notnull") != 0;
        let default: Option<String> = row.try_get("dflt_value").unwrap_or(None);
        let pk_order: i32 = row.get("pk");
        let pk = pk_order != 0;

        if pk {
            ordered_primary_keys.push((pk_order, name.clone()));
        }

        columns.push(ColumnSchema {
            is_auto_increment: auto_increment_columns
                .iter()
                .any(|column| column.eq_ignore_ascii_case(&name)),
            name,
            data_type,
            nullable: !notnull,
            default_value: default,
            is_primary_key: pk,
            comment: None,
        });
    }
    ordered_primary_keys.sort_by_key(|(order, _)| *order);
    let primary_keys = ordered_primary_keys
        .into_iter()
        .map(|(_, name)| name)
        .collect::<Vec<_>>();

    // Indexes
    let idx_rows = sqlx::query(&format!(
        "PRAGMA {catalog}.index_list({})",
        driver.quote_ident(table)
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut indexes = Vec::new();
    for idx_row in &idx_rows {
        let idx_name: String = idx_row.get("name");
        let is_unique: bool = idx_row.get::<i32, _>("unique") != 0;
        let origin: String = idx_row.try_get("origin").unwrap_or_default();
        let is_partial: bool = idx_row.try_get::<i32, _>("partial").unwrap_or(0) != 0;
        if is_partial {
            migration_blockers.push(format!(
                "Partial index `{idx_name}` is not represented by Schema Diff"
            ));
        }

        let info_rows = sqlx::query(&format!(
            "PRAGMA {catalog}.index_info({})",
            driver.quote_ident(&idx_name)
        ))
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let xinfo_rows = sqlx::query(&format!(
            "PRAGMA {catalog}.index_xinfo({})",
            driver.quote_ident(&idx_name)
        ))
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let idx_columns: Vec<String> = info_rows
            .iter()
            .filter_map(|row| row.try_get::<Option<String>, _>("name").ok().flatten())
            .collect();

        for row in &xinfo_rows {
            let is_key: bool = row.try_get::<i32, _>("key").unwrap_or(1) != 0;
            if !is_key {
                continue;
            }
            let column_id: i32 = row.try_get("cid").unwrap_or(-1);
            let column_name: Option<String> = row.try_get("name").ok().flatten();
            let descending: bool = row.try_get::<i32, _>("desc").unwrap_or(0) != 0;
            let collation: Option<String> = row.try_get("coll").ok().flatten();
            if column_id < 0 || column_name.is_none() {
                migration_blockers.push(format!(
                    "Expression index `{idx_name}` is not represented by Schema Diff"
                ));
            }
            if descending
                || collation
                    .as_deref()
                    .is_some_and(|value| !value.eq_ignore_ascii_case("BINARY"))
            {
                migration_blockers.push(format!(
                        "Index sort or collation semantics on `{idx_name}` are not represented by Schema Diff"
                    ));
            }
        }

        let is_primary = origin == "pk";
        indexes.push(IndexInfo {
            name: idx_name,
            columns: idx_columns,
            is_unique,
            is_primary,
            index_type: "btree".into(),
        });
    }
    indexes.sort_by(|a, b| a.name.cmp(&b.name));

    // Foreign keys
    let fk_rows = sqlx::query(&format!(
        "PRAGMA {catalog}.foreign_key_list({})",
        driver.quote_ident(table)
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

    let mut fk_map: BTreeMap<i64, ForeignKeyInfo> = BTreeMap::new();
    for fk_row in &fk_rows {
        let id: i64 = fk_row.get::<i32, _>("id") as i64;
        let from_col: String = fk_row.get("from");
        let to_table: String = fk_row.get("table");
        let to_col: String = fk_row
            .try_get::<Option<String>, _>("to")
            .ok()
            .flatten()
            .unwrap_or_default();
        let on_update: String = fk_row.try_get("on_update").unwrap_or_default();
        let on_delete: String = fk_row.try_get("on_delete").unwrap_or_default();

        fk_map
            .entry(id)
            .and_modify(|fk| {
                fk.columns.push(from_col.clone());
                fk.referenced_columns.push(to_col.clone());
            })
            .or_insert_with(|| ForeignKeyInfo {
                name: format!("fk_{}_{}_{}", table, to_table, id),
                columns: vec![from_col],
                referenced_table: to_table,
                referenced_columns: vec![to_col],
                on_update,
                on_delete,
                deferrability: ForeignKeyDeferrability::NotDeferrable,
            });
    }

    let foreign_keys: Vec<ForeignKeyInfo> = fk_map.into_values().collect();

    let trigger_rows = sqlx::query(&format!(
        "SELECT name FROM {catalog}.sqlite_master WHERE type = 'trigger' AND tbl_name = ?"
    ))
    .bind(table)
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    for row in trigger_rows {
        let name: String = row.get("name");
        migration_blockers.push(format!(
            "Trigger `{name}` attached to `{table}` is outside the table schema snapshot"
        ));
    }

    let view_rows = sqlx::query(&format!(
        "SELECT name, sql FROM {catalog}.sqlite_master WHERE type = 'view'"
    ))
    .fetch_all(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    for row in view_rows {
        let name: String = row.get("name");
        let definition: Option<String> = row.try_get("sql").ok();
        if definition
            .as_deref()
            .is_none_or(|sql| sqlite_sql_mentions_identifier(sql, table))
        {
            migration_blockers.push(format!(
                "View `{name}` may depend on `{table}` and is outside the table schema snapshot"
            ));
        }
    }

    let table_rows = sqlx::query(&format!(
            "SELECT name FROM {catalog}.sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'"
        ))
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    for row in table_rows {
        let candidate: String = row.get("name");
        let incoming_rows = sqlx::query(&format!(
            "PRAGMA {catalog}.foreign_key_list({})",
            driver.quote_ident(&candidate)
        ))
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        if incoming_rows.iter().any(|fk| {
            fk.try_get::<String, _>("table")
                .is_ok_and(|referenced| referenced.eq_ignore_ascii_case(table))
        }) {
            migration_blockers.push(format!(
                "Table `{candidate}` has a foreign key referencing `{table}`"
            ));
        }
    }

    let temp_name = format!("__datazen_rebuild_{table}");
    let temp_exists: Option<i32> = sqlx::query_scalar(&format!(
        "SELECT 1 FROM {catalog}.sqlite_master WHERE type = 'table' AND name = ?"
    ))
    .bind(&temp_name)
    .fetch_optional(pool)
    .await
    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    if temp_exists.is_some() {
        migration_blockers.push(format!(
            "Temporary rebuild relation `{temp_name}` already exists"
        ));
    }

    migration_blockers.sort();
    migration_blockers.dedup();

    Ok(TableSchema {
        table_name: table.to_string(),
        columns,
        primary_keys,
        indexes,
        foreign_keys,
        check_constraints,
        table_options: TableOptions {
            migration_blockers,
            ..TableOptions::default()
        },
    })
}
