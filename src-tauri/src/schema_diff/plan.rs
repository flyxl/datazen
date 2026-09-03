//! Build SchemaDiffPlan from table schema pairs.

use super::compare::diff_table_schemas;
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
    statements: &mut Vec<PlanStatement>,
    warnings: &mut Vec<String>,
    requirements: &mut Vec<super::types::PlanRequirement>,
) {
    let mut operations = super::ir::diff_to_operations(table, src, tgt);

    // Type mapping belongs at the boundary between source snapshot and target driver.
    // The IR remains dialect-neutral; only replace types before rendering.
    if let Some(mapper) = opts.type_mapper {
        for op in &mut operations {
            match op {
                super::operations::MigrationOperation::AddColumn { column, .. } => {
                    if let Ok(ty) = mapper(&column.data_type, &column.name) {
                        column.data_type = ty;
                    }
                }
                super::operations::MigrationOperation::CreateTable { columns, .. } => {
                    for column in columns {
                        if let Ok(ty) = mapper(&column.data_type, &column.name) {
                            column.data_type = ty;
                        }
                    }
                }
                super::operations::MigrationOperation::AlterColumnType { column, to, .. } => {
                    if let Ok(ty) = mapper(to, column) {
                        *to = ty;
                    }
                }
                _ => {}
            }
        }
    }

    // Safety policy is domain-level: destructive operations require explicit approval.
    operations.retain(|op| {
        if !opts.allow_destructive && op.risk() == StatementRisk::Destructive {
            warnings.push(format!("Skipped destructive operation {}", op.key()));
            false
        } else {
            true
        }
    });

    // Adding a NOT NULL column without a source default would fail for existing rows.
    // Do not invent a business value. Add it nullable and require explicit backfill.
    for op in &mut operations {
        if let super::operations::MigrationOperation::AddColumn { table, column } = op {
            if !column.nullable && column.default_value.is_none() {
                column.nullable = true;
                requirements.push(super::types::PlanRequirement::Backfill {
                    table: table.clone(),
                    column: column.name.clone(),
                    reason: "Populate existing rows before enforcing NOT NULL.".into(),
                });
            }
        }
    }

    if !opts.include_indexes {
        operations.retain(|op| {
            !matches!(
                op,
                super::operations::MigrationOperation::CreateIndex { .. }
                    | super::operations::MigrationOperation::DropIndex { .. }
            )
        });
    }

    let Some(driver) = datazen_driver_api::create_driver(target_dialect) else {
        requirements.push(super::types::PlanRequirement::Unsupported {
            operation: table.to_string(),
            reason: format!("No registered driver for target database: {target_dialect}"),
        });
        return;
    };
    let Some(capabilities) = driver.migration_capabilities() else {
        requirements.push(super::types::PlanRequirement::Unsupported {
            operation: table.to_string(),
            reason: format!(
                "Driver {} does not expose schema migration capabilities",
                target_dialect
            ),
        });
        return;
    };

    operations.retain(|op| {
        let driver_op = op.to_driver_api();
        if !capabilities.supports(&driver_op) {
            requirements.push(super::types::PlanRequirement::Unsupported {
                operation: op.key(),
                reason: format!("Operation is not supported by {}", target_dialect),
            });
            false
        } else {
            if capabilities.requires_table_rebuild(&driver_op) {
                warnings.push(format!(
                    "{} may require a table rewrite on {}",
                    op.key(),
                    target_dialect
                ));
            }
            true
        }
    });

    let operations = super::dependencies::resolve_dependencies(operations);
    let Some(renderer) = driver.migration_renderer() else {
        requirements.push(super::types::PlanRequirement::Unsupported {
            operation: table.to_string(),
            reason: format!(
                "Driver {} does not expose schema migration rendering",
                target_dialect
            ),
        });
        return;
    };

    for op in operations {
        let key = op.key();
        let driver_op = op.to_driver_api();
        match renderer.render(&driver_op) {
            Ok(stmt) => {
                statements.push(PlanStatement {
                    sql: stmt.sql,
                    risk: match stmt.risk {
                        datazen_driver_api::MigrationRisk::Additive => StatementRisk::Additive,
                        datazen_driver_api::MigrationRisk::Rewrite => StatementRisk::Rewrite,
                        datazen_driver_api::MigrationRisk::Destructive => {
                            StatementRisk::Destructive
                        }
                    },
                    rollback_sql: stmt.rollback_sql,
                    summary: stmt.summary,
                });
            }
            Err(reason) => requirements.push(super::types::PlanRequirement::Unsupported {
                operation: key,
                reason,
            }),
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
        let stmt = plan
            .statements
            .iter()
            .find(|s| s.summary.starts_with("ALTER DEFAULT"))
            .unwrap();
        assert!(stmt.sql.contains("SET DEFAULT 0"));
        assert!(stmt
            .rollback_sql
            .as_deref()
            .unwrap()
            .contains("SET DEFAULT 1"));
    }

    #[test]
    fn mysql_primary_key_change_is_planned() {
        let mut src = schema(vec![col("id", "int"), col("user_id", "int")]);
        src.primary_keys = vec!["user_id".into()];
        let mut tgt = schema(vec![col("id", "int"), col("user_id", "int")]);
        tgt.primary_keys = vec!["id".into()];
        let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
        assert!(plan
            .statements
            .iter()
            .any(|s| s.sql.contains("DROP PRIMARY KEY")));
        assert!(plan
            .statements
            .iter()
            .any(|s| s.sql.contains("ADD PRIMARY KEY")));
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
        let add = plan
            .statements
            .iter()
            .find(|s| s.summary.starts_with("ADD COLUMN"))
            .unwrap();
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
        let add = plan
            .statements
            .iter()
            .find(|s| s.summary.starts_with("ADD COLUMN"))
            .unwrap();
        assert!(add.sql.contains("NOT NULL"));
        assert!(add.sql.contains("DEFAULT 0"));
        assert!(!plan
            .statements
            .iter()
            .any(|s| s.summary.starts_with("DROP DEFAULT")));
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
        assert!(plan
            .requirements
            .iter()
            .any(|r| matches!(r, super::super::types::PlanRequirement::Unsupported { .. })));
    }

    #[test]
    fn unsupported_driver_operation_becomes_requirement() {
        let src = schema(vec![col("id", "int")]);
        let mut target = src.clone();
        target.columns[0].data_type = "json".into();
        let plan = build_schema_diff_plan(
            &[("users".into(), src, target)],
            "postgresql",
            "postgresql",
            PlanOptions::default(),
        );
        assert!(plan
            .requirements
            .iter()
            .any(|r| matches!(r, super::super::types::PlanRequirement::Unsupported { .. })));
    }
}
