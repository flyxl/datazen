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

fn plan_single_table(
    table: &str,
    src: &TableSchema,
    tgt: &TableSchema,
    target_dialect: &str,
    opts: &PlanOptions<'_>,
    statements: &mut Vec<super::types::PlanStatement>,
    warnings: &mut Vec<String>,
    requirements: &mut Vec<super::types::PlanRequirement>,
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
        statements.push(super::dialects::create_table_stmt(
            &dialect, table, src, &resolved,
        ));
        // Defaults are rendered as part of CREATE TABLE. Dialect-specific sequence
        // handling is intentionally deferred until sequence metadata is available from
        // introspection; never infer object names by parsing a SQL expression here.
        // Primary key is a table-level object; do not model it as a column-only warning.
    if src.primary_keys != tgt.primary_keys {
        if !tgt.primary_keys.is_empty() {
            if !opts.allow_destructive {
                warnings.push(format!("Skipped DROP PRIMARY KEY on {table} (enable allowDestructive)"));
            } else {
                match dialect.as_str() {
                    "postgresql" => {
                        requirements.push(super::types::PlanRequirement::Unsupported {
                            operation: format!("DROP PRIMARY KEY {table}"),
                            reason: "PostgreSQL primary-key constraint names are not exposed by the current schema snapshot; refusing to guess the constraint name.".into(),
                        });
                    },
                    "mysql" => statements.push(mysql::drop_primary_key(table)),
                    "sqlite" => warnings.push(format!("SQLite primary-key changes require table rebuild: {table}")),
                    _ => warnings.push(format!("Unsupported dialect for DROP PRIMARY KEY: {dialect}")),
                }
            }
        }
        if !src.primary_keys.is_empty() && (tgt.primary_keys.is_empty() || dialect == "mysql") {
            match dialect.as_str() {
                "postgresql" => statements.push(postgres::add_primary_key(table, &src.primary_keys)),
                "mysql" => statements.push(mysql::add_primary_key(table, &src.primary_keys)),
                "sqlite" => warnings.push(format!("SQLite primary-key changes require table rebuild: {table}")),
                _ => warnings.push(format!("Unsupported dialect for ADD PRIMARY KEY: {dialect}")),
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
                let src_col = src.columns.iter().find(|c| c.name == col.name);
                let src_default = src_col.and_then(|c| c.default_value.as_deref());

                // Never invent values for existing target rows. A NOT NULL column
                // without a source default is added nullable and requires backfill.
                let unsafe_sequence_default = src_default
                    .map(|d| d.to_ascii_lowercase().contains("nextval("))
                    .unwrap_or(false);

                if !col.nullable && src_default.is_none() {
                    let nullable_col = ColumnSnapshot { nullable: true, ..col.clone() };
                    let add_stmt = match dialect.as_str() {
                        "postgresql" => postgres::add_column(table, &nullable_col, &ty),
                        "mysql" => mysql::add_column(table, &nullable_col, &ty),
                        "sqlite" => sqlite::add_column(table, &nullable_col, &ty),
                        other => {
                            warnings.push(format!("Unsupported dialect for ADD COLUMN: {other}"));
                            continue;
                        }
                    };
                    statements.push(add_stmt);
                    warnings.push(format!(
                        "ADD COLUMN {}.{} requires backfill before SET NOT NULL; no default was provided by source",
                        table, col.name
                    ));
                    requirements.push(super::types::PlanRequirement::Backfill {
                        table: table.to_string(),
                        column: col.name.clone(),
                        reason: "Source column is NOT NULL but has no default; populate existing target rows before enforcing NOT NULL.".into(),
                    });
                    continue;
                }

                if unsafe_sequence_default && dialect != "postgresql" {
                    let nullable_col = ColumnSnapshot { nullable: true, ..col.clone() };
                    let add_stmt = match dialect.as_str() {
                        "mysql" => mysql::add_column(table, &nullable_col, &ty),
                        "sqlite" => sqlite::add_column(table, &nullable_col, &ty),
                        _ => continue,
                    };
                    statements.push(add_stmt);
                    warnings.push(format!(
                        "Skipped PostgreSQL nextval default for {}.{} on {}; explicit identity/auto-increment mapping is required",
                        table, col.name, dialect
                    ));
                    requirements.push(super::types::PlanRequirement::Unsupported {
                        operation: format!("ADD COLUMN {}.{}", table, col.name),
                        reason: "PostgreSQL sequence defaults cannot be translated safely without sequence metadata.".into(),
                    });
                    continue;
                }

                if let Some(def) = src_default {
                    let stmt = match dialect.as_str() {
                        "postgresql" => postgres::add_column_with_default(table, col, &ty, def),
                        "mysql" => mysql::add_column_with_default(table, col, &ty, def),
                        _ => {
                            warnings.push(format!(
                                "Default expression for {}.{} is not supported on {}",
                                table, col.name, dialect
                            ));
                            requirements.push(super::types::PlanRequirement::Unsupported {
                                operation: format!("ADD COLUMN {}.{}", table, col.name),
                                reason: "Target dialect cannot render this default expression.".into(),
                            });
                            continue;
                        }
                    };
                    statements.push(stmt);
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
        let wants_type = change.changes.contains(&super::types::ColumnChange::DataType);
        let wants_null = change.changes.contains(&super::types::ColumnChange::Nullable);

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

        if change.changes.contains(&super::types::ColumnChange::Default) {
            let stmt = super::dialects::set_default_with_rollback(
                &dialect,
                table,
                &change.name,
                change.source.default_value.as_deref(),
                change.target.default_value.as_deref(),
            );
            statements.push(stmt);
        }

        if change.changes.contains(&super::types::ColumnChange::Comment)
            || change.changes.contains(&super::types::ColumnChange::AutoIncrement)
        {
            requirements.push(super::types::PlanRequirement::Unsupported {
                operation: format!("ALTER COLUMN {}.{}", table, change.name),
                reason: "Column comment/auto-increment changes are detected but require dialect-specific migration support.".into(),
            });
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
    let mut requirements = Vec::new();
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
            &mut requirements,
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
        requirements,
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
    fn changed_default_generates_target_dialect_ddl() {
        let mut src_c = col("status", "int");
        src_c.default_value = Some("0".into());
        let mut tgt_c = col("status", "int");
        tgt_c.default_value = Some("1".into());
        let src = schema(vec![src_c]);
        let tgt = schema(vec![tgt_c]);
        let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
        let stmt = plan.statements.iter().find(|s| s.summary.starts_with("ALTER DEFAULT")).unwrap();
        assert!(stmt.sql.contains("SET DEFAULT 0"));
        assert!(stmt.rollback_sql.as_deref().unwrap().contains("SET DEFAULT 1"));
    }

    #[test]
    fn mysql_primary_key_change_is_planned() {
        let mut src = schema(vec![col("id", "int"), col("user_id", "int")]);
        src.primary_keys = vec!["user_id".into()];
        let mut tgt = schema(vec![col("id", "int"), col("user_id", "int")]);
        tgt.primary_keys = vec!["id".into()];
        let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
        assert!(plan.statements.iter().any(|s| s.sql.contains("DROP PRIMARY KEY")));
        assert!(plan.statements.iter().any(|s| s.sql.contains("ADD PRIMARY KEY")));
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
    fn add_not_null_column_without_default_requires_backfill() {
        let mut c = col("status", "int");
        c.nullable = false;
        let src = schema(vec![col("id", "int"), c]);
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        let add = plan.statements.iter().find(|s| s.summary.starts_with("ADD COLUMN")).unwrap();
        assert!(add.sql.contains("status"));
        assert!(!add.sql.contains("NOT NULL"));
        assert!(plan.requirements.iter().any(|r| matches!(r, super::super::types::PlanRequirement::Backfill { column, .. } if column == "status")));
    }

    #[test]
    fn add_not_null_column_with_default_preserves_default() {
        let mut c = col("status", "int");
        c.nullable = false;
        c.default_value = Some("0".into());
        let src = schema(vec![col("id", "int"), c]);
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        let add = plan.statements.iter().find(|s| s.summary.starts_with("ADD COLUMN")).unwrap();
        assert!(add.sql.contains("NOT NULL"));
        assert!(add.sql.contains("DEFAULT 0"));
        assert!(!plan.statements.iter().any(|s| s.summary.starts_with("DROP DEFAULT")));
    }

    #[test]
    fn cross_dialect_pg_to_mysql_does_not_translate_nextval() {
        let mut id_col = col("id", "integer");
        id_col.nullable = false;
        id_col.default_value = Some("nextval('orders_id_seq'::regclass)".into());
        let src = schema(vec![id_col, col("name", "text")]);
        let tgt = schema(vec![col("name", "text")]);
        let plan = build_schema_diff_plan(
            &[("orders".into(), src, tgt)],
            "postgresql",
            "mysql",
            PlanOptions::default(),
        );
        assert!(!plan.statements.iter().any(|s| s.sql.contains("nextval")));
        assert!(plan.requirements.iter().any(|r| matches!(r, super::super::types::PlanRequirement::Unsupported { .. })));
    }
}
