//! Build SchemaDiffPlan from table schema pairs.

use super::compare::diff_table_schemas;
use super::types::{
    ColumnSnapshot, PlanRequirement, PlanStatement,
    RollbackCompleteness, SchemaDiffPlan, StatementRisk,
    normalize_dialect, resolve_table_for_dialect,
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
        Self { allow_destructive: false, include_indexes: true, type_mapper: None }
    }
}

fn plan_single_table(
    table: &str,
    src: &TableSchema,
    tgt: &TableSchema,
    target_dialect: &str,
    opts: &PlanOptions<'_>,
    statements: &mut Vec<PlanStatement>,
    warnings: &mut Vec<String>,
    requirements: &mut Vec<PlanRequirement>,
) {
    let mut operations = super::ir::diff_to_operations(table, src, tgt);

    if let Some(mapper) = opts.type_mapper {
        for op in &mut operations {
            match op {
                super::operations::MigrationOperation::AddColumn { column, .. } => {
                    if let Ok(ty) = mapper(&column.data_type, &column.name) { column.data_type = ty; }
                }
                super::operations::MigrationOperation::CreateTable { columns, .. } => {
                    for column in columns {
                        if let Ok(ty) = mapper(&column.data_type, &column.name) { column.data_type = ty; }
                    }
                }
                super::operations::MigrationOperation::AlterColumnType { column, to, .. } => {
                    if let Ok(ty) = mapper(to, column) { *to = ty; }
                }
                _ => {}
            }
        }
    }

    operations.retain(|op| {
        if !opts.allow_destructive && op.risk() == StatementRisk::Destructive {
            warnings.push(format!("Skipped destructive operation {}", op.key()));
            false
        } else { true }
    });

    for op in &mut operations {
        if let super::operations::MigrationOperation::AddColumn { table, column } = op {
            if !column.nullable && column.default_value.is_none() {
                column.nullable = true;
                requirements.push(PlanRequirement::Backfill {
                    table: table.clone(), column: column.name.clone(),
                    reason: "Populate existing rows before enforcing NOT NULL.".into(),
                });
            }
        }
    }

    if !opts.include_indexes {
        operations.retain(|op| !matches!(op,
            super::operations::MigrationOperation::CreateIndex { .. } |
            super::operations::MigrationOperation::DropIndex { .. }
        ));
    }

    let Some(driver) = datazen_driver_api::create_driver(target_dialect) else {
        requirements.push(PlanRequirement::Unsupported {
            operation: table.to_string(), reason: format!("No registered driver for target database: {target_dialect}"),
        });
        return;
    };
    let Some(capabilities) = driver.migration_capabilities() else {
        requirements.push(PlanRequirement::Unsupported {
            operation: table.to_string(), reason: format!("Driver {target_dialect} does not expose schema migration capabilities"),
        });
        return;
    };

    operations.retain(|op| {
        let driver_op = op.to_driver_api();
        if !capabilities.supports(&driver_op) {
            requirements.push(PlanRequirement::Unsupported {
                operation: op.key(), reason: format!("Operation is not supported by {target_dialect}"),
            });
            false
        } else {
            if capabilities.requires_table_rebuild(&driver_op) {
                warnings.push(format!("{} may require a table rewrite on {}", op.key(), target_dialect));
            }
            true
        }
    });

    let operations = super::dependencies::resolve_dependencies(operations);
    let Some(renderer) = driver.migration_renderer() else {
        requirements.push(PlanRequirement::Unsupported {
            operation: table.to_string(), reason: format!("Driver {target_dialect} does not expose schema migration rendering"),
        });
        return;
    };

    for op in operations {
        let key = op.key();
        match renderer.render(&op.to_driver_api()) {
            Ok(stmt) => statements.push(PlanStatement {
                sql: stmt.sql,
                risk: match stmt.risk {
                    datazen_driver_api::MigrationRisk::Additive => StatementRisk::Additive,
                    datazen_driver_api::MigrationRisk::Rewrite => StatementRisk::Rewrite,
                    datazen_driver_api::MigrationRisk::Destructive => StatementRisk::Destructive,
                },
                rollback_sql: stmt.rollback_sql,
                summary: stmt.summary,
            }),
            Err(reason) => requirements.push(PlanRequirement::Unsupported { operation: key, reason }),
        }
    }
}

fn rollback_completeness(statements: &[PlanStatement]) -> RollbackCompleteness {
    let missing = statements.iter().filter(|s| s.rollback_sql.is_none()).map(|s| s.summary.clone()).collect::<Vec<_>>();
    RollbackCompleteness { complete: missing.is_empty(), missing }
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
        warnings.push("Cross-dialect plan without IR type mapper: using source native types as-is (may fail)".into());
    }

    let mut tables = Vec::new();
    for (table, src, tgt) in pairs {
        tables.push(table.clone());
        let deploy_table = resolve_table_for_dialect(&tgt_d, table);
        plan_single_table(&deploy_table, src, tgt, &tgt_d, &opts, &mut statements, &mut warnings, &mut requirements);
    }

    let primary = tables.first().cloned().unwrap_or_default();
    SchemaDiffPlan {
        table: primary, tables, source_dialect: src_d, target_dialect: tgt_d, same_dialect,
        statements: statements.clone(), warnings, rollback_completeness: rollback_completeness(&statements), requirements,
    }
}

pub fn build_column_plan(table: &str, src: &TableSchema, tgt: &TableSchema, dialect: &str) -> Result<SchemaDiffPlan, String> {
    Ok(build_schema_diff_plan(&[(table.to_string(), src.clone(), tgt.clone())], dialect, dialect,
        PlanOptions { allow_destructive: true, include_indexes: false, type_mapper: None }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ColumnSchema, IndexInfo};

    fn col(name: &str, ty: &str) -> ColumnSchema {
        ColumnSchema { name: name.into(), data_type: ty.into(), nullable: true, default_value: None,
            comment: None, is_primary_key: false, is_auto_increment: false }
    }
    fn schema(cols: Vec<ColumnSchema>) -> TableSchema {
        TableSchema { table_name: "users".into(), columns: cols, primary_keys: vec![], indexes: vec![], foreign_keys: vec![] }
    }

    #[test]
    fn pg_to_mysql_strips_schema_prefix_in_ddl() {
        let src = schema(vec![col("id", "int"), col("email", "text")]);
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_schema_diff_plan(&[("public.users".into(), src, tgt)], "postgresql", "mysql", PlanOptions::default());
        let add = plan.statements.iter().find(|s| s.sql.contains("email")).expect("ADD COLUMN for email");
        assert!(add.sql.contains("`users`")); assert!(!add.sql.contains("public."));
    }
    #[test]
    fn missing_target_table_plans_create_not_add_column() {
        let src = schema(vec![col("id", "int"), col("email", "varchar(255)")]);
        let tgt = schema(vec![]);
        let plan = build_column_plan("public.users", &src, &tgt, "postgresql").unwrap();
        assert!(plan.statements.iter().any(|s| s.sql.contains("CREATE TABLE") && s.sql.contains("email")));
        assert!(!plan.statements.iter().any(|s| s.sql.contains("ADD COLUMN")));
    }
    #[test]
    fn postgres_add_varchar_column() {
        let src = schema(vec![col("id", "int"), col("email", "varchar(255)")]);
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        assert!(plan.statements.iter().any(|s| s.sql.contains("ADD COLUMN") && s.sql.contains("email")));
    }
    #[test]
    fn mysql_drop_requires_destructive_flag_in_metadata() {
        let src = schema(vec![col("id", "int")]); let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
        let plan = build_column_plan("users", &src, &tgt, "mysql").unwrap();
        let drop = plan.statements.iter().find(|s| s.sql.contains("DROP COLUMN")).unwrap();
        assert_eq!(drop.risk, StatementRisk::Destructive);
    }
    #[test]
    fn additive_default_skips_drop() {
        let src = schema(vec![col("id", "int")]); let tgt = schema(vec![col("id", "int"), col("legacy", "text")]);
        let plan = build_schema_diff_plan(&[("users".into(), src, tgt)], "postgresql", "postgresql", PlanOptions::default());
        assert!(plan.statements.is_empty()); assert!(plan.warnings.iter().any(|w| w.contains("DROP COLUMN")));
    }
    #[test]
    fn multi_table_concatenates() {
        let src_a = schema(vec![col("id", "int"), col("a", "text")]); let tgt_a = schema(vec![col("id", "int")]);
        let src_b = schema(vec![col("id", "int"), col("b", "text")]); let tgt_b = schema(vec![col("id", "int")]);
        let plan = build_schema_diff_plan(&[("t_a".into(), src_a, tgt_a), ("t_b".into(), src_b, tgt_b)], "postgresql", "postgresql", PlanOptions::default());
        assert_eq!(plan.tables.len(), 2); assert!(plan.statements.len() >= 2);
    }
    #[test]
    fn index_create_planned() {
        let mut src = schema(vec![col("id", "int"), col("email", "text")]);
        src.indexes.push(IndexInfo { name: "idx_email".into(), columns: vec!["email".into()], is_unique: true, is_primary: false, index_type: "btree".into() });
        let tgt = schema(vec![col("id", "int"), col("email", "text")]);
        let plan = build_schema_diff_plan(&[("users".into(), src, tgt)], "postgresql", "postgresql", PlanOptions::default());
        assert!(plan.statements.iter().any(|s| s.sql.contains("CREATE") && s.sql.contains("idx_email")));
    }
    #[test]
    fn cross_dialect_type_mapper() {
        let src = schema(vec![col("id", "int4"), col("email", "character varying")]); let tgt = schema(vec![col("id", "int")]);
        let mapper = |ty: &str, _name: &str| -> Result<String, String> { if ty.contains("varying") || ty == "text" { Ok("VARCHAR(255)".into()) } else if ty.starts_with("int") { Ok("INT".into()) } else { Err(format!("unsupported type: {ty}")) } };
        let plan = build_schema_diff_plan(&[("users".into(), src, tgt)], "postgresql", "mysql", PlanOptions { allow_destructive: true, include_indexes: false, type_mapper: Some(&mapper) });
        assert!(plan.statements.iter().any(|s| s.sql.contains("ADD COLUMN") && s.sql.contains("VARCHAR(255)")));
    }
    #[test]
    fn cross_dialect_pg_to_mysql_does_not_translate_nextval() {
        let mut id_col = col("id", "integer"); id_col.nullable = false; id_col.default_value = Some("nextval('orders_id_seq'::regclass)".into());
        let src = schema(vec![id_col, col("name", "text")]); let tgt = schema(vec![col("name", "text")]);
        let plan = build_schema_diff_plan(&[("orders".into(), src, tgt)], "postgresql", "mysql", PlanOptions::default());
        assert!(!plan.statements.iter().any(|s| s.sql.contains("nextval")));
        assert!(plan.requirements.iter().any(|r| matches!(r, PlanRequirement::Unsupported { .. })));
    }
}
