//! Build SchemaDiffPlan from table schema pairs.

use super::compare::{diff_indexes, diff_table_schemas};
use super::dialects::{mysql, postgres, sqlite};
use super::types::{
    normalize_dialect, resolve_table_for_dialect, ColumnSnapshot, PlanStatement,
    RollbackCompleteness, SchemaDiffPlan, StatementRisk,
};
use crate::db::TableSchema;

/// Optional mapper: (source_type_sql, column_name) → native type for target dialect.
pub type TypeMapper<'a> = dyn Fn(&str, &str) -> Result<String, String> + 'a;

pub struct PlanOptions<'a> {
    pub allow_destructive: bool,
    pub include_indexes: bool,
    /// When set, used instead of raw `column.data_type` (cross-dialect / IR).
    pub type_mapper: Option<&'a TypeMapper<'a>>,
}

impl Default for PlanOptions<'_> {
    fn default() -> Self {
        Self {
            allow_destructive: false,
            include_indexes: true,
            type_mapper: None,
        }
    }
}

fn resolve_type(
    opts: &PlanOptions<'_>,
    col_name: &str,
    source_type: &str,
) -> Result<String, String> {
    if let Some(mapper) = opts.type_mapper {
        mapper(source_type, col_name)
    } else {
        Ok(source_type.to_string())
    }
}

fn is_narrowing_nullability(src_nullable: bool, tgt_nullable: bool) -> bool {
    // Deploy makes target match source: going from nullable→NOT NULL is narrowing.
    !src_nullable && tgt_nullable
}

/// Return a safe zero/empty literal for the given SQL type so that
/// `ADD COLUMN … NOT NULL DEFAULT <zero>` succeeds even when the table has rows.
fn type_zero_literal(_dialect: &str, type_sql: &str) -> Option<&'static str> {
    let ty = type_sql.to_lowercase();
    if ty.contains("serial")
        || ty.contains("int")
        || ty.contains("float")
        || ty.contains("double")
        || ty.contains("real")
        || ty.contains("numeric")
        || ty.contains("decimal")
        || ty.contains("number")
    {
        Some("0")
    } else if ty.contains("bool") {
        Some("false")
    } else if ty.contains("text")
        || ty.contains("char")
        || ty.contains("varchar")
        || ty.contains("citext")
        || ty.contains("clob")
    {
        Some("''")
    } else if ty.contains("timestamp") || ty.contains("datetime") {
        Some("CURRENT_TIMESTAMP")
    } else if ty.contains("date") {
        Some("CURRENT_DATE")
    } else if ty.contains("time") {
        Some("CURRENT_TIME")
    } else if ty.contains("json") {
        Some("'{}'")
    } else if ty.contains("uuid") {
        Some("'00000000-0000-0000-0000-000000000000'")
    } else if ty.contains("bytea") || ty.contains("blob") || ty.contains("binary") {
        Some("''")
    } else {
        None
    }
}

/// Extract the sequence name from a `nextval('seq_name'::regclass)` default expression.
fn extract_sequence_name(default_expr: &str) -> Option<String> {
    let lower = default_expr.to_lowercase();
    if !lower.contains("nextval") {
        return None;
    }
    // Match patterns like: nextval('schema.seq_name'::regclass) or nextval('"schema"."seq_name"'::regclass)
    let start = default_expr.find('\'')? + 1;
    let end = default_expr[start..].find('\'')? + start;
    let raw = &default_expr[start..end];
    if raw.is_empty() {
        None
    } else {
        Some(raw.to_string())
    }
}

fn plan_single_table(
    table: &str,
    src: &TableSchema,
    tgt: &TableSchema,
    target_dialect: &str,
    opts: &PlanOptions<'_>,
    statements: &mut Vec<super::types::PlanStatement>,
    warnings: &mut Vec<String>,
) {
    let dialect = normalize_dialect(target_dialect);

    if tgt.columns.is_empty() {
        if src.columns.is_empty() {
            warnings.push(format!(
                "Table {table} is missing on target and source has no columns; skipped"
            ));
            return;
        }
        let mut resolved = Vec::new();
        for col in &src.columns {
            match resolve_type(opts, &col.name, &col.data_type) {
                Ok(ty) => resolved.push((col.name.clone(), ty)),
                Err(e) => {
                    warnings.push(format!("Skip CREATE TABLE {table}: {e}"));
                    return;
                }
            }
        }
        // Create sequences referenced by column defaults before CREATE TABLE
        // (PostgreSQL only — MySQL uses AUTO_INCREMENT, SQLite uses AUTOINCREMENT)
        if dialect == "postgresql" {
            for col in &src.columns {
                if let Some(default_val) = &col.default_value {
                    if let Some(seq) = extract_sequence_name(default_val) {
                        let q_seq = super::dialects::quote_ident(&dialect, &seq);
                        statements.push(PlanStatement {
                            sql: format!("CREATE SEQUENCE IF NOT EXISTS {q_seq}"),
                            risk: StatementRisk::Additive,
                            rollback_sql: Some(format!("DROP SEQUENCE IF EXISTS {q_seq}")),
                            summary: format!("CREATE SEQUENCE {seq}"),
                        });
                    }
                }
            }
        }
        statements.push(super::dialects::create_table_stmt(
            &dialect, table, src, &resolved,
        ));
        // Set column defaults and sequence ownership after CREATE TABLE
        // (PostgreSQL only for sequences; other dialects skip nextval defaults)
        for col in &src.columns {
            if let Some(default_val) = &col.default_value {
                let has_nextval = extract_sequence_name(default_val).is_some();
                if has_nextval && dialect != "postgresql" {
                    warnings.push(format!(
                        "Skipped nextval default for {}.{} (not supported on {}; configure auto-increment manually)",
                        table, col.name, dialect
                    ));
                    continue;
                }
                let q_table = super::dialects::quote_ident(&dialect, table);
                let q_col = super::dialects::quote_column(&dialect, &col.name);
                statements.push(PlanStatement {
                    sql: format!(
                        "ALTER TABLE {q_table} ALTER COLUMN {q_col} SET DEFAULT {default_val}"
                    ),
                    risk: StatementRisk::Additive,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {q_table} ALTER COLUMN {q_col} DROP DEFAULT"
                    )),
                    summary: format!("SET DEFAULT {}.{}", table, col.name),
                });
                if let Some(seq) = extract_sequence_name(default_val) {
                    let q_seq = super::dialects::quote_ident(&dialect, &seq);
                    statements.push(PlanStatement {
                        sql: format!("ALTER SEQUENCE {q_seq} OWNED BY {q_table}.{q_col}"),
                        risk: StatementRisk::Additive,
                        rollback_sql: None,
                        summary: format!("SET SEQUENCE OWNER {seq} → {}.{}", table, col.name),
                    });
                }
            }
        }
        if opts.include_indexes {
            for i in &src.indexes {
                if i.is_primary {
                    continue;
                }
                let stmt = match dialect.as_str() {
                    "postgresql" => postgres::create_index(table, &i.name, &i.columns, i.is_unique),
                    "mysql" => mysql::create_index(table, &i.name, &i.columns, i.is_unique),
                    "sqlite" => sqlite::create_index(table, &i.name, &i.columns, i.is_unique),
                    _ => continue,
                };
                statements.push(stmt);
            }
        }
        return;
    }

    let diff = diff_table_schemas(table, src, tgt);

    for col in &diff.missing_on_target {
        match resolve_type(opts, &col.name, &col.data_type) {
            Ok(ty) => {
                if dialect == "sqlite" && !col.nullable {
                    warnings.push(format!(
                        "SQLite ADD COLUMN {}.{} will be nullable (NOT NULL without DEFAULT unsupported)",
                        table, col.name
                    ));
                }
                // For non-SQLite dialects, adding a NOT NULL column to a table
                // that already has rows fails (PostgreSQL: "contains null values").
                // Strategy:
                // 1) If source has a nextval() default → CREATE SEQUENCE + ADD COLUMN with original default
                // 2) If source has another default → ADD COLUMN with that default
                // 3) If no default → ADD COLUMN with type zero literal, then DROP DEFAULT
                // 4) If no zero literal known → fall back to nullable + SET NOT NULL
                let needs_default_hack = !col.nullable && dialect != "sqlite";
                if needs_default_hack {
                    let src_col = src.columns.iter().find(|c| c.name == col.name);
                    let src_default = src_col.and_then(|c| c.default_value.as_deref());

                    // Check if the default references a sequence (nextval)
                    let seq_name = src_default.and_then(extract_sequence_name);
                    let has_nextval = seq_name.is_some();

                    // Sequences are PostgreSQL-only; for MySQL/SQLite, skip nextval defaults
                    if has_nextval && dialect == "postgresql" {
                        if let Some(ref seq) = seq_name {
                            let q_seq = super::dialects::quote_ident(&dialect, seq);
                            statements.push(PlanStatement {
                                sql: format!("CREATE SEQUENCE IF NOT EXISTS {q_seq}"),
                                risk: StatementRisk::Additive,
                                rollback_sql: Some(format!("DROP SEQUENCE IF EXISTS {q_seq}")),
                                summary: format!("CREATE SEQUENCE {seq}"),
                            });
                        }
                    }

                    // Determine the default expression for ADD COLUMN
                    let default_expr = if has_nextval && dialect != "postgresql" {
                        // nextval on non-PG target: use zero literal instead
                        warnings.push(format!(
                            "Skipped nextval default for {}.{} (not supported on {}; configure auto-increment manually)",
                            table, col.name, dialect
                        ));
                        type_zero_literal(&dialect, &ty).map(|z| z.to_string())
                    } else if src_default.is_some() {
                        // Use the original source default (including nextval for PG targets)
                        src_default.map(|d| d.to_string())
                    } else {
                        // No source default → use a zero literal
                        type_zero_literal(&dialect, &ty).map(|z| z.to_string())
                    };

                    if let Some(def) = &default_expr {
                        let q_table = super::dialects::quote_ident(&dialect, table);
                        let q_col = super::dialects::quote_column(&dialect, &col.name);
                        let nulls = super::dialects::nullability_sql(col.nullable);
                        let sql = format!(
                            "ALTER TABLE {q_table} ADD COLUMN {q_col} {ty}{nulls} DEFAULT {def}"
                        );
                        let rollback = format!("ALTER TABLE {q_table} DROP COLUMN {q_col}");
                        statements.push(PlanStatement {
                            sql,
                            risk: StatementRisk::Additive,
                            rollback_sql: Some(rollback),
                            summary: format!("ADD COLUMN {}.{}", table, col.name),
                        });

                        // If we used a synthetic zero literal (no source default,
                        // or nextval on non-PG target), drop the default so target
                        // matches source expectations.
                        let used_synthetic =
                            src_default.is_none() || (has_nextval && dialect != "postgresql");
                        if used_synthetic {
                            let q_table = super::dialects::quote_ident(&dialect, table);
                            let q_col = super::dialects::quote_column(&dialect, &col.name);
                            let drop_def =
                                format!("ALTER TABLE {q_table} ALTER COLUMN {q_col} DROP DEFAULT");
                            statements.push(PlanStatement {
                                sql: drop_def,
                                risk: StatementRisk::Additive,
                                rollback_sql: None,
                                summary: format!("DROP DEFAULT {}.{}", table, col.name),
                            });
                        }

                        // Set sequence ownership so it's dropped with the column (PG only)
                        if dialect == "postgresql" {
                            if let Some(ref seq) = seq_name {
                                let q_table = super::dialects::quote_ident(&dialect, table);
                                let q_col = super::dialects::quote_column(&dialect, &col.name);
                                let q_seq = super::dialects::quote_ident(&dialect, seq);
                                statements.push(PlanStatement {
                                    sql: format!(
                                        "ALTER SEQUENCE {q_seq} OWNED BY {q_table}.{q_col}"
                                    ),
                                    risk: StatementRisk::Additive,
                                    rollback_sql: None,
                                    summary: format!(
                                        "SET SEQUENCE OWNER {seq} → {}.{}",
                                        table, col.name
                                    ),
                                });
                            }
                        }
                    } else {
                        // Cannot determine a safe default → fall back to nullable add + SET NOT NULL
                        let nullable_col = ColumnSnapshot {
                            nullable: true,
                            ..col.clone()
                        };
                        let add_stmt = match dialect.as_str() {
                            "postgresql" => postgres::add_column(table, &nullable_col, &ty),
                            "mysql" => mysql::add_column(table, &nullable_col, &ty),
                            other => {
                                warnings
                                    .push(format!("Unsupported dialect for ADD COLUMN: {other}"));
                                continue;
                            }
                        };
                        statements.push(add_stmt);
                        let q_table = super::dialects::quote_ident(&dialect, table);
                        let q_col = super::dialects::quote_column(&dialect, &col.name);
                        let set_nn = match dialect.as_str() {
                            "postgresql" => PlanStatement {
                                sql: format!(
                                    "ALTER TABLE {q_table} ALTER COLUMN {q_col} SET NOT NULL"
                                ),
                                risk: StatementRisk::Rewrite,
                                rollback_sql: Some(format!(
                                    "ALTER TABLE {q_table} ALTER COLUMN {q_col} DROP NOT NULL"
                                )),
                                summary: format!("SET NOT NULL {}.{}", table, col.name),
                            },
                            "mysql" => PlanStatement {
                                sql: format!(
                                    "ALTER TABLE {q_table} MODIFY COLUMN {q_col} {ty} NOT NULL"
                                ),
                                risk: StatementRisk::Rewrite,
                                rollback_sql: Some(format!(
                                    "ALTER TABLE {q_table} MODIFY COLUMN {q_col} {ty} NULL"
                                )),
                                summary: format!("SET NOT NULL {}.{}", table, col.name),
                            },
                            _ => continue,
                        };
                        statements.push(set_nn);
                        warnings.push(format!(
                            "ADD COLUMN {}.{} is NOT NULL but no safe default could be determined; populate existing rows before SET NOT NULL",
                            table, col.name
                        ));
                    }
                } else {
                    let stmt = match dialect.as_str() {
                        "postgresql" => postgres::add_column(table, col, &ty),
                        "mysql" => mysql::add_column(table, col, &ty),
                        "sqlite" => sqlite::add_column(table, col, &ty),
                        other => {
                            warnings.push(format!("Unsupported dialect for ADD COLUMN: {other}"));
                            continue;
                        }
                    };
                    statements.push(stmt);
                }
            }
            Err(e) => warnings.push(format!("Skip ADD {}.{}: {e}", table, col.name)),
        }
    }

    for col in &diff.extra_on_target {
        if !opts.allow_destructive {
            warnings.push(format!(
                "Skipped DROP COLUMN {}.{} (enable allowDestructive)",
                table, col.name
            ));
            continue;
        }
        match dialect.as_str() {
            "postgresql" => statements.push(postgres::drop_column(table, col)),
            "mysql" => statements.push(mysql::drop_column(table, col)),
            "sqlite" => warnings.push(sqlite::unsupported_drop(table, &col.name)),
            other => warnings.push(format!("Unsupported dialect for DROP COLUMN: {other}")),
        }
    }

    for change in &diff.changed {
        let wants_type = change.changes.iter().any(|c| c == "dataType");
        let wants_null = change.changes.iter().any(|c| c == "nullable");

        if dialect == "sqlite" && (wants_type || wants_null) {
            warnings.push(sqlite::unsupported_modify(table, &change.name));
            continue;
        }

        if wants_type {
            let narrowing = is_type_narrowing(&change.source, &change.target);
            if narrowing && !opts.allow_destructive {
                warnings.push(format!(
                    "Skipped type ALTER {}.{} (possible narrowing; enable allowDestructive)",
                    table, change.name
                ));
            } else {
                match resolve_type(opts, &change.name, &change.source.data_type) {
                    Ok(ty) => {
                        let stmt = match dialect.as_str() {
                            "postgresql" => postgres::alter_type(table, change, &ty),
                            "mysql" => mysql::modify_column(table, change, &ty),
                            _ => continue,
                        };
                        if stmt.risk == StatementRisk::Rewrite && !opts.allow_destructive {
                            warnings.push(format!(
                                "Skipped rewrite ALTER {}.{} (enable allowDestructive)",
                                table, change.name
                            ));
                        } else {
                            statements.push(stmt);
                        }
                    }
                    Err(e) => {
                        warnings.push(format!("Skip ALTER type {}.{}: {e}", table, change.name))
                    }
                }
            }
        } else if wants_null {
            let narrowing =
                is_narrowing_nullability(change.source.nullable, change.target.nullable);
            if narrowing && !opts.allow_destructive {
                warnings.push(format!(
                    "Skipped SET NOT NULL {}.{} (enable allowDestructive)",
                    table, change.name
                ));
                continue;
            }
            match dialect.as_str() {
                "postgresql" => statements.push(postgres::set_nullability(table, change)),
                "mysql" => match resolve_type(opts, &change.name, &change.source.data_type) {
                    Ok(ty) => statements.push(mysql::modify_column(table, change, &ty)),
                    Err(e) => {
                        warnings.push(format!("Skip nullability {}.{}: {e}", table, change.name))
                    }
                },
                _ => {}
            }
        }

        if change.changes.iter().any(|c| c == "isPrimaryKey") {
            warnings.push(format!(
                "Primary key change on {}.{} is not auto-planned; apply manually",
                table, change.name
            ));
        }
    }

    if opts.include_indexes {
        let idx = diff_indexes(src, tgt);
        for i in &idx.missing_on_target {
            let stmt = match dialect.as_str() {
                "postgresql" => postgres::create_index(table, &i.name, &i.columns, i.is_unique),
                "mysql" => mysql::create_index(table, &i.name, &i.columns, i.is_unique),
                "sqlite" => sqlite::create_index(table, &i.name, &i.columns, i.is_unique),
                _ => continue,
            };
            statements.push(stmt);
        }
        for i in &idx.extra_on_target {
            if !opts.allow_destructive {
                warnings.push(format!(
                    "Skipped DROP INDEX {} (enable allowDestructive)",
                    i.name
                ));
                continue;
            }
            let stmt = match dialect.as_str() {
                "postgresql" => postgres::drop_index(&i.name),
                "mysql" => mysql::drop_index(table, &i.name),
                "sqlite" => sqlite::drop_index(&i.name),
                _ => continue,
            };
            statements.push(stmt);
        }
    }
}

fn is_type_narrowing(src: &ColumnSnapshot, tgt: &ColumnSnapshot) -> bool {
    // Heuristic: treat any type change as rewrite/narrowing candidate when lengths shrink.
    let src_l = src.data_type.to_ascii_lowercase();
    let tgt_l = tgt.data_type.to_ascii_lowercase();
    if src_l == tgt_l {
        return false;
    }
    // varchar(n) → varchar(m) with m < n
    if let (Some(sn), Some(tn)) = (extract_len(&src_l), extract_len(&tgt_l)) {
        return sn < tn; // source desired is smaller than target current → narrowing deploy
    }
    true // unknown type change → treat as potentially narrowing
}

fn extract_len(ty: &str) -> Option<usize> {
    let start = ty.find('(')?;
    let end = ty.find(')')?;
    ty[start + 1..end].parse().ok()
}

fn rollback_completeness(statements: &[super::types::PlanStatement]) -> RollbackCompleteness {
    let missing: Vec<String> = statements
        .iter()
        .filter(|s| s.rollback_sql.is_none())
        .map(|s| s.summary.clone())
        .collect();
    RollbackCompleteness {
        complete: missing.is_empty(),
        missing,
    }
}

/// Build a plan for one or more tables. `pairs` is (table_name, source_schema, target_schema).
pub fn build_schema_diff_plan(
    pairs: &[(String, TableSchema, TableSchema)],
    source_dialect: &str,
    target_dialect: &str,
    opts: PlanOptions<'_>,
) -> SchemaDiffPlan {
    let mut statements = Vec::new();
    let mut warnings = Vec::new();
    let src_d = normalize_dialect(source_dialect);
    let tgt_d = normalize_dialect(target_dialect);
    let same_dialect = src_d == tgt_d;

    if !same_dialect && opts.type_mapper.is_none() {
        warnings.push(
            "Cross-dialect plan without IR type mapper: using source native types as-is (may fail)"
                .into(),
        );
    }

    let mut tables = Vec::new();
    for (table, src, tgt) in pairs {
        tables.push(table.clone());
        let deploy_table = resolve_table_for_dialect(&tgt_d, table);
        plan_single_table(
            &deploy_table,
            src,
            tgt,
            &tgt_d,
            &opts,
            &mut statements,
            &mut warnings,
        );
    }

    let primary = tables.first().cloned().unwrap_or_default();
    let completeness = rollback_completeness(&statements);

    SchemaDiffPlan {
        table: primary,
        tables,
        source_dialect: src_d,
        target_dialect: tgt_d,
        same_dialect,
        statements,
        warnings,
        rollback_completeness: completeness,
    }
}

/// Convenience for single-table same-dialect plans (P1 tests).
pub fn build_column_plan(
    table: &str,
    src: &TableSchema,
    tgt: &TableSchema,
    dialect: &str,
) -> Result<SchemaDiffPlan, String> {
    Ok(build_schema_diff_plan(
        &[(table.to_string(), src.clone(), tgt.clone())],
        dialect,
        dialect,
        PlanOptions {
            allow_destructive: true,
            include_indexes: false,
            type_mapper: None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnSchema, IndexInfo};

    fn col(name: &str, ty: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: ty.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    fn schema(cols: Vec<ColumnSchema>) -> TableSchema {
        TableSchema {
            table_name: "users".into(),
            columns: cols,
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        }
    }

    #[test]
    fn pg_to_mysql_strips_schema_prefix_in_ddl() {
        let src = schema(vec![col("id", "int"), col("email", "text")]);
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_schema_diff_plan(
            &[("public.users".into(), src, tgt)],
            "postgresql",
            "mysql",
            PlanOptions {
                allow_destructive: false,
                include_indexes: false,
                type_mapper: None,
            },
        );
        let add = plan
            .statements
            .iter()
            .find(|s| s.sql.contains("email"))
            .expect("ADD COLUMN for email");
        assert!(add.sql.contains("`users`"));
        assert!(!add.sql.contains("public."));
    }

    #[test]
    fn missing_target_table_plans_create_not_add_column() {
        let src = schema(vec![col("id", "int"), col("email", "varchar(255)")]);
        let tgt = schema(vec![]);
        let plan = build_column_plan("public.users", &src, &tgt, "postgresql").unwrap();
        assert!(plan
            .statements
            .iter()
            .any(|s| s.sql.contains("CREATE TABLE") && s.sql.contains("email")));
        assert!(!plan.statements.iter().any(|s| s.sql.contains("ADD COLUMN")));
    }

    #[test]
    fn postgres_add_varchar_column() {
        let src = schema(vec![col("id", "int"), col("email", "varchar(255)")]);
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        assert!(plan
            .statements
            .iter()
            .any(|s| { s.sql.contains("ADD COLUMN") && s.sql.contains("email") }));
    }

    #[test]
    fn mysql_drop_requires_destructive_flag_in_metadata() {
        let src = schema(vec![col("id", "int")]);
        let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
        let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
        let drop = plan
            .statements
            .iter()
            .find(|s| s.sql.contains("DROP COLUMN"))
            .unwrap();
        assert_eq!(drop.risk, StatementRisk::Destructive);
    }

    #[test]
    fn additive_default_skips_drop() {
        let src = schema(vec![col("id", "int")]);
        let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
        let plan = build_schema_diff_plan(
            &[("users".into(), src, tgt)],
            "postgresql",
            "postgresql",
            PlanOptions {
                allow_destructive: false,
                include_indexes: false,
                type_mapper: None,
            },
        );
        assert!(plan.statements.is_empty());
        assert!(plan.warnings.iter().any(|w| w.contains("DROP COLUMN")));
    }

    #[test]
    fn multi_table_concatenates() {
        let src_a = schema(vec![col("id", "int"), col("a", "text")]);
        let tgt_a = schema(vec![col("id", "int")]);
        let src_b = schema(vec![col("id", "int"), col("b", "text")]);
        let tgt_b = schema(vec![col("id", "int")]);
        let plan = build_schema_diff_plan(
            &[("t_a".into(), src_a, tgt_a), ("t_b".into(), src_b, tgt_b)],
            "postgresql",
            "postgresql",
            PlanOptions::default(),
        );
        assert_eq!(plan.tables.len(), 2);
        assert!(plan.statements.len() >= 2);
    }

    #[test]
    fn index_create_planned() {
        let mut src = schema(vec![col("id", "int"), col("email", "text")]);
        src.indexes.push(IndexInfo {
            name: "idx_email".into(),
            columns: vec!["email".into()],
            is_unique: true,
            is_primary: false,
            index_type: "btree".into(),
        });
        let tgt = schema(vec![col("id", "int"), col("email", "text")]);
        let plan = build_schema_diff_plan(
            &[("users".into(), src, tgt)],
            "postgresql",
            "postgresql",
            PlanOptions {
                allow_destructive: false,
                include_indexes: true,
                type_mapper: None,
            },
        );
        assert!(plan
            .statements
            .iter()
            .any(|s| s.sql.contains("CREATE") && s.sql.contains("idx_email")));
    }

    #[test]
    fn cross_dialect_type_mapper() {
        let src = schema(vec![col("id", "int4"), col("email", "character varying")]);
        let tgt = schema(vec![col("id", "int")]);
        let mapper = |ty: &str, _name: &str| -> Result<String, String> {
            if ty.contains("varying") || ty == "text" {
                Ok("VARCHAR(255)".into())
            } else if ty.starts_with("int") {
                Ok("INT".into())
            } else {
                Err(format!("unsupported type {ty}"))
            }
        };
        let plan = build_schema_diff_plan(
            &[("users".into(), src, tgt)],
            "postgresql",
            "mysql",
            PlanOptions {
                allow_destructive: false,
                include_indexes: false,
                type_mapper: Some(&mapper),
            },
        );
        assert!(!plan.same_dialect);
        let add = plan
            .statements
            .iter()
            .find(|s| s.sql.contains("email"))
            .unwrap();
        assert!(add.sql.contains("VARCHAR(255)"));
    }

    #[test]
    fn rollback_completeness_false_when_drop_index() {
        let src = schema(vec![col("id", "int")]);
        let mut tgt = schema(vec![col("id", "int")]);
        tgt.indexes.push(IndexInfo {
            name: "idx_old".into(),
            columns: vec!["id".into()],
            is_unique: false,
            is_primary: false,
            index_type: "btree".into(),
        });
        let plan = build_schema_diff_plan(
            &[("users".into(), src, tgt)],
            "postgresql",
            "postgresql",
            PlanOptions {
                allow_destructive: true,
                include_indexes: true,
                type_mapper: None,
            },
        );
        assert!(!plan.rollback_completeness.complete);
        assert!(!plan.rollback_completeness.missing.is_empty());
    }

    #[test]
    fn add_not_null_column_splits_into_two_steps() {
        let src = {
            let mut c = col("email", "varchar(255)");
            c.nullable = false;
            schema(vec![col("id", "int"), c])
        };
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        // ADD COLUMN should include NOT NULL DEFAULT ''
        let add = plan
            .statements
            .iter()
            .find(|s| s.summary.starts_with("ADD COLUMN"))
            .expect("ADD COLUMN statement");
        assert!(
            add.sql.contains("NOT NULL"),
            "ADD COLUMN should include NOT NULL: {}",
            add.sql
        );
        assert!(
            add.sql.contains("DEFAULT"),
            "ADD COLUMN should include DEFAULT: {}",
            add.sql
        );
        // Second step: DROP DEFAULT (since source has no default)
        let drop_def = plan
            .statements
            .iter()
            .find(|s| s.summary.starts_with("DROP DEFAULT"))
            .expect("DROP DEFAULT statement");
        assert!(
            drop_def.sql.contains("DROP DEFAULT"),
            "should contain DROP DEFAULT: {}",
            drop_def.sql
        );
    }

    #[test]
    fn add_not_null_column_creates_sequence_for_nextval() {
        let mut id_col = col("id", "integer");
        id_col.nullable = false;
        id_col.default_value = Some("nextval('test_orders_id_seq'::regclass)".into());
        let src = schema(vec![id_col, col("name", "text")]);
        let tgt = schema(vec![col("name", "text")]);
        let plan = build_column_plan("test_orders", &src, &tgt, "postgresql").unwrap();

        // Should create the sequence first
        let create_seq = plan
            .statements
            .iter()
            .find(|s| s.summary.contains("CREATE SEQUENCE"))
            .expect("CREATE SEQUENCE statement");
        assert!(
            create_seq.sql.contains("test_orders_id_seq"),
            "should create the sequence: {}",
            create_seq.sql
        );

        // ADD COLUMN should use the original nextval default
        let add = plan
            .statements
            .iter()
            .find(|s| s.summary.starts_with("ADD COLUMN"))
            .expect("ADD COLUMN statement");
        assert!(
            add.sql.contains("nextval"),
            "should use nextval default: {}",
            add.sql
        );
        assert!(
            add.sql.contains("NOT NULL"),
            "should be NOT NULL: {}",
            add.sql
        );

        // Should set sequence ownership
        let owned_by = plan
            .statements
            .iter()
            .find(|s| s.summary.contains("SET SEQUENCE OWNER"))
            .expect("OWNED BY statement");
        assert!(
            owned_by.sql.contains("OWNED BY"),
            "should set ownership: {}",
            owned_by.sql
        );
    }

    #[test]
    fn cross_dialect_pg_to_mysql_skips_sequence_uses_zero_literal() {
        let mut id_col = col("id", "integer");
        id_col.nullable = false;
        id_col.default_value = Some("nextval('orders_id_seq'::regclass)".into());
        let src = schema(vec![id_col, col("name", "text")]);
        let tgt = schema(vec![col("name", "text")]);
        let plan = build_schema_diff_plan(
            &[("orders".into(), src, tgt)],
            "postgresql",
            "mysql",
            PlanOptions {
                allow_destructive: false,
                include_indexes: false,
                type_mapper: None,
            },
        );
        // Should NOT have CREATE SEQUENCE (MySQL doesn't support it)
        assert!(
            !plan
                .statements
                .iter()
                .any(|s| s.sql.contains("CREATE SEQUENCE")),
            "MySQL plan should not contain CREATE SEQUENCE"
        );
        // Should NOT have OWNED BY
        assert!(
            !plan.statements.iter().any(|s| s.sql.contains("OWNED BY")),
            "MySQL plan should not contain OWNED BY"
        );
        // ADD COLUMN should use zero literal DEFAULT, not nextval
        let add = plan
            .statements
            .iter()
            .find(|s| s.summary.starts_with("ADD COLUMN"))
            .expect("ADD COLUMN statement");
        assert!(
            !add.sql.contains("nextval"),
            "MySQL ADD COLUMN should not reference nextval: {}",
            add.sql
        );
        assert!(
            add.sql.contains("DEFAULT 0"),
            "MySQL ADD COLUMN should use DEFAULT 0: {}",
            add.sql
        );
        // Should have a warning about skipped nextval
        assert!(
            plan.warnings
                .iter()
                .any(|w| w.contains("nextval") && w.contains("auto-increment")),
            "should warn about skipped nextval"
        );
    }
}
