//! Build SchemaDiffPlan from table schema pairs.

use super::types::{
    normalize_dialect, resolve_table_for_dialect, ColumnSnapshot, PlanRequirement, PlanStatement,
    RollbackCompleteness, SchemaDiffPlan, StatementRisk,
};
use crate::db::TableSchema;
use std::collections::HashSet;

/// Optional mapper: (source_type_sql, column_name) → native type for target dialect.
pub type TypeMapper<'a> = dyn Fn(&str, &str) -> Result<String, String> + 'a;

pub struct PlanOptions<'a> {
    pub allow_destructive: bool,
    pub include_indexes: bool,
    /// When set, used instead of raw `column.data_type` (cross-dialect / IR).
    pub type_mapper: Option<&'a TypeMapper<'a>>,
    /// Set automatically by build_schema_diff_plan when source ≠ target dialect.
    #[doc(hidden)]
    pub cross_dialect: bool,
}

impl Default for PlanOptions<'_> {
    fn default() -> Self {
        Self {
            allow_destructive: false,
            include_indexes: true,
            type_mapper: None,
            cross_dialect: false,
        }
    }
}

/// Strips source-dialect-specific default expressions that would be invalid
/// in the target dialect (e.g. PostgreSQL `nextval()` in MySQL).
fn strip_dialect_specific_defaults(
    op: &mut super::operations::MigrationOperation,
    warnings: &mut Vec<String>,
) {
    let pg_patterns = ["nextval(", "::regclass", "::"];
    let strip = |col: &mut super::types::ColumnSnapshot, table: &str, w: &mut Vec<String>| {
        if let Some(ref d) = col.default_value {
            if pg_patterns.iter().any(|p| d.contains(p)) {
                w.push(format!(
                    "Stripped dialect-specific default for {}.{}: {}",
                    table, col.name, d
                ));
                col.default_value = None;
            }
        }
    };
    match op {
        super::operations::MigrationOperation::AddColumn { table, column } => {
            strip(column, table, warnings)
        }
        super::operations::MigrationOperation::CreateTable { table, columns, .. } => {
            for c in columns {
                strip(c, table, warnings);
            }
        }
        _ => {}
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

fn integer_rank(ty: &str) -> Option<u8> {
    let base = ty.split('(').next()?.trim();
    match base {
        "bigint" | "int8" => Some(4),
        "int" | "integer" | "int4" | "mediumint" => Some(3),
        "smallint" | "int2" => Some(2),
        "tinyint" | "int1" => Some(1),
        _ => None,
    }
}

fn apply_type_mapping(
    op: &mut super::operations::MigrationOperation,
    opts: &PlanOptions<'_>,
    requirements: &mut Vec<PlanRequirement>,
) -> bool {
    match op {
        super::operations::MigrationOperation::AddColumn { column, .. } => {
            match resolve_type(opts, &column.name, &column.data_type) {
                Ok(ty) => column.data_type = ty,
                Err(reason) => {
                    requirements.push(PlanRequirement::Unsupported {
                        operation: op.key(),
                        reason,
                    });
                    return false;
                }
            }
        }
        super::operations::MigrationOperation::CreateTable { columns, .. } => {
            for column in columns {
                match resolve_type(opts, &column.name, &column.data_type) {
                    Ok(ty) => column.data_type = ty,
                    Err(reason) => {
                        requirements.push(PlanRequirement::Unsupported {
                            operation: op.key(),
                            reason,
                        });
                        return false;
                    }
                }
            }
        }
        super::operations::MigrationOperation::AlterColumnType { column, to, .. } => {
            match resolve_type(opts, column, to) {
                Ok(ty) => *to = ty,
                Err(reason) => {
                    requirements.push(PlanRequirement::Unsupported {
                        operation: op.key(),
                        reason,
                    });
                    return false;
                }
            }
        }
        _ => {}
    }
    true
}

fn effective_risk(
    op: &super::operations::MigrationOperation,
    destructive_narrowing: &HashSet<String>,
) -> StatementRisk {
    if destructive_narrowing.contains(&op.key()) {
        StatementRisk::Destructive
    } else {
        op.risk()
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
    requirements: &mut Vec<super::types::PlanRequirement>,
) {
    let Some(driver) = datazen_driver_api::create_driver(target_dialect) else {
        requirements.push(super::types::PlanRequirement::Unsupported {
            operation: table.to_string(),
            reason: format!("No registered driver for target database: {target_dialect}"),
        });
        return;
    };

    let normalizer = if opts.cross_dialect {
        None
    } else {
        driver.type_normalizer()
    };
    let normalizer_ref = normalizer.as_deref();
    let mut operations = super::ir::diff_to_operations(table, src, tgt, normalizer_ref);

    // Type mapping belongs at the boundary between source snapshot and target driver.
    // The IR remains dialect-neutral; only replace types before rendering.
    if opts.type_mapper.is_some() {
        operations.retain_mut(|op| apply_type_mapping(op, opts, requirements));
    }

    // Cross-dialect: strip source-dialect-specific defaults before rendering.
    if opts.cross_dialect {
        for op in &mut operations {
            strip_dialect_specific_defaults(op, warnings);
        }
    }

    let mut destructive_narrowing = HashSet::new();
    for op in &operations {
        match op {
            super::operations::MigrationOperation::AlterColumnType {
                table,
                column,
                from,
                to,
            } => {
                let desired = ColumnSnapshot {
                    name: column.clone(),
                    data_type: to.clone(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                };
                let current = ColumnSnapshot {
                    name: column.clone(),
                    data_type: from.clone(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                };
                if is_type_narrowing(&desired, &current) {
                    destructive_narrowing.insert(op.key());
                    warnings.push(format!(
                        "Type change on {table}.{column} from {from} to {to} may truncate existing data"
                    ));
                }
            }
            super::operations::MigrationOperation::SetNullable {
                table,
                column,
                nullable,
            } if is_narrowing_nullability(*nullable, !nullable) => {
                destructive_narrowing.insert(op.key());
                warnings.push(format!(
                    "Setting {table}.{column} to NOT NULL may fail if existing rows contain NULL"
                ));
            }
            _ => {}
        }
    }

    // Safety policy is domain-level: destructive operations require explicit approval.
    operations.retain(|op| {
        if !opts.allow_destructive
            && effective_risk(op, &destructive_narrowing) == StatementRisk::Destructive
        {
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
                let mut risk = match stmt.risk {
                    datazen_driver_api::MigrationRisk::Additive => StatementRisk::Additive,
                    datazen_driver_api::MigrationRisk::Rewrite => StatementRisk::Rewrite,
                    datazen_driver_api::MigrationRisk::Destructive => StatementRisk::Destructive,
                };
                if destructive_narrowing.contains(&key) {
                    risk = StatementRisk::Destructive;
                }
                statements.push(PlanStatement {
                    sql: stmt.sql,
                    risk,
                    rollback_sql: stmt.rollback_sql,
                    summary: stmt.summary,
                });
            }
            Err(reason) => requirements.push(PlanRequirement::Unsupported {
                operation: key,
                reason,
            }),
        }
    }
}

fn is_type_narrowing(desired: &ColumnSnapshot, current: &ColumnSnapshot) -> bool {
    let desired_l = desired.data_type.to_ascii_lowercase();
    let current_l = current.data_type.to_ascii_lowercase();
    if desired_l == current_l {
        return false;
    }
    // varchar(n) → varchar(m) with m < n
    if let (Some(dn), Some(cn)) = (extract_len(&desired_l), extract_len(&current_l)) {
        return dn < cn;
    }
    // Integer family narrowing (e.g. INT → SMALLINT).
    if let (Some(dr), Some(cr)) = (integer_rank(&desired_l), integer_rank(&current_l)) {
        return dr < cr;
    }
    false
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

    let opts = PlanOptions {
        cross_dialect: !same_dialect,
        ..opts
    };

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
            cross_dialect: false,
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
                cross_dialect: false,
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
                cross_dialect: false,
            },
        );
        assert!(plan.statements.is_empty());
        assert!(plan
            .warnings
            .iter()
            .any(|w| w.contains("Skipped destructive")));
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
                cross_dialect: false,
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
                cross_dialect: false,
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
    fn mysql_primary_key_change_generates_drop_and_add() {
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
                cross_dialect: false,
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
            .warnings
            .iter()
            .any(|w| w.contains("Cross-dialect plan without IR type mapper")));
    }

    #[test]
    fn type_mapper_failure_becomes_unsupported_not_executable() {
        let src = schema(vec![col("id", "int"), col("payload", "jsonb")]);
        let tgt = schema(vec![col("id", "int")]);
        let mapper = |_ty: &str, name: &str| -> Result<String, String> {
            if name == "payload" {
                Err("no mysql equivalent for jsonb".into())
            } else {
                Ok("INT".into())
            }
        };
        let plan = build_schema_diff_plan(
            &[("users".into(), src, tgt)],
            "postgresql",
            "mysql",
            PlanOptions {
                allow_destructive: true,
                include_indexes: false,
                type_mapper: Some(&mapper),
                cross_dialect: false,
            },
        );
        assert!(plan.statements.is_empty());
        assert!(plan.requirements.iter().any(|r| {
            matches!(
                r,
                super::super::types::PlanRequirement::Unsupported { operation, reason }
                if operation.contains("payload") && reason.contains("jsonb")
            )
        }));
    }

    #[test]
    fn create_table_type_mapper_failure_becomes_unsupported() {
        let src = schema(vec![col("id", "int"), col("meta", "hstore")]);
        let tgt = schema(vec![]);
        let mapper = |_ty: &str, name: &str| -> Result<String, String> {
            if name == "meta" {
                Err("unsupported hstore".into())
            } else {
                Ok("INT".into())
            }
        };
        let plan = build_schema_diff_plan(
            &[("items".into(), src, tgt)],
            "postgresql",
            "mysql",
            PlanOptions {
                allow_destructive: true,
                include_indexes: false,
                type_mapper: Some(&mapper),
                cross_dialect: false,
            },
        );
        assert!(!plan
            .statements
            .iter()
            .any(|s| s.sql.contains("CREATE TABLE")));
        assert!(plan.requirements.iter().any(|r| {
            matches!(
                r,
                super::super::types::PlanRequirement::Unsupported { reason, .. }
                if reason.contains("hstore")
            )
        }));
    }

    #[test]
    fn varchar_narrowing_marks_destructive_risk() {
        let src = schema(vec![col("id", "int"), col("code", "varchar(100)")]);
        let tgt = schema(vec![col("id", "int"), col("code", "varchar(255)")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        let alter = plan
            .statements
            .iter()
            .find(|s| s.summary.contains("ALTER TYPE") || s.sql.contains("TYPE"))
            .expect("ALTER TYPE statement");
        assert_eq!(alter.risk, StatementRisk::Destructive);
        assert!(plan.warnings.iter().any(|w| w.contains("truncate")));
    }

    #[test]
    fn int_to_smallint_narrowing_marks_destructive_risk() {
        let src = schema(vec![col("id", "int"), col("qty", "smallint")]);
        let tgt = schema(vec![col("id", "int"), col("qty", "int")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        let alter = plan
            .statements
            .iter()
            .find(|s| s.summary.contains("ALTER TYPE") || s.sql.contains("TYPE"))
            .expect("ALTER TYPE statement");
        assert_eq!(alter.risk, StatementRisk::Destructive);
    }

    #[test]
    fn unknown_type_change_without_length_is_not_narrowing() {
        let src = schema(vec![col("id", "int"), col("payload", "json")]);
        let tgt = schema(vec![col("id", "int"), col("payload", "text")]);
        let plan = build_column_plan("users", &src, &tgt, "postgresql").unwrap();
        let alter = plan
            .statements
            .iter()
            .find(|s| s.summary.contains("ALTER TYPE"))
            .expect("ALTER TYPE statement");
        assert_eq!(alter.risk, StatementRisk::Rewrite);
        assert!(!plan.warnings.iter().any(|w| w.contains("truncate")));
    }

    #[test]
    fn set_not_null_narrowing_requires_destructive_flag() {
        let mut src_c = col("status", "int");
        src_c.nullable = false;
        let mut tgt_c = col("status", "int");
        tgt_c.nullable = true;
        let src = schema(vec![col("id", "int"), src_c]);
        let tgt = schema(vec![col("id", "int"), tgt_c]);
        let plan = build_schema_diff_plan(
            &[("users".into(), src, tgt)],
            "postgresql",
            "postgresql",
            PlanOptions {
                allow_destructive: false,
                include_indexes: false,
                type_mapper: None,
                cross_dialect: false,
            },
        );
        assert!(plan.statements.is_empty());
        assert!(plan.warnings.iter().any(|w| w.contains("NOT NULL")));
    }

    #[test]
    fn is_type_narrowing_unit_cases() {
        let snap = |ty: &str| ColumnSnapshot {
            name: "c".into(),
            data_type: ty.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        };
        assert!(is_type_narrowing(
            &snap("varchar(100)"),
            &snap("varchar(255)")
        ));
        assert!(!is_type_narrowing(
            &snap("varchar(255)"),
            &snap("varchar(100)")
        ));
        assert!(is_type_narrowing(&snap("smallint"), &snap("int")));
        assert!(!is_type_narrowing(&snap("int"), &snap("smallint")));
        assert!(!is_type_narrowing(&snap("json"), &snap("text")));
        assert!(!is_type_narrowing(
            &snap("varchar(100)"),
            &snap("varchar(255x)")
        ));
    }

    #[test]
    fn unsupported_driver_operation_becomes_requirement() {
        let mut src = schema(vec![col("id", "int")]);
        src.columns[0].is_auto_increment = true;
        let tgt = schema(vec![col("id", "int")]);
        let plan = build_schema_diff_plan(
            &[("users".into(), src, tgt)],
            "postgresql",
            "postgresql",
            PlanOptions::default(),
        );
        assert!(plan
            .requirements
            .iter()
            .any(|r| matches!(r, super::super::types::PlanRequirement::Unsupported { .. })));
    }

    #[test]
    fn sqlite_alter_type_becomes_unsupported() {
        let src = schema(vec![col("id", "int")]);
        let mut target = src.clone();
        target.columns[0].data_type = "text".into();
        let plan = build_schema_diff_plan(
            &[("users".into(), src, target)],
            "sqlite",
            "sqlite",
            PlanOptions::default(),
        );
        assert!(plan
            .requirements
            .iter()
            .any(|r| matches!(r, super::super::types::PlanRequirement::Unsupported { .. })));
    }
}
