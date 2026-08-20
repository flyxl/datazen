//! SQL Server table structure capabilities and DDL planning.
//!
//! T-SQL has several constraints that shape which operations the structure
//! editor can execute in a single plan:
//! - `ALTER TABLE ... ALTER COLUMN` can change type / nullability but **not**
//!   name, default, or identity. Column renames go through `sp_rename`.
//! - Columns cannot be reordered; there is no `FIRST`/`AFTER` positioning.
//! - Default values require a (named) default constraint; changing an existing
//!   default needs the constraint name, so we only support defaults on new
//!   columns / fresh CREATE TABLE.
//! - Indexes are schema-scoped (`DROP INDEX <name> ON <table>`).

use datazen_driver_api::*;
use std::collections::HashMap;

/// SQL Server structure capabilities exposed to the structure editor UI.
pub fn sqlserver_capabilities(dialect_id: &str) -> StructureCapabilities {
    StructureCapabilities {
        create_table: true,
        add_column: true,
        drop_column: true,
        rename_column: true,
        alter_type: true,
        alter_nullability: true,
        alter_default: false,
        alter_primary_key: false,
        reorder_column: false,
        comment: false,
        create_index: true,
        drop_index: true,
        rebuild_index: false,
        index_type: false,
        index_include: false,
        index_filter: false,
        index_comment: false,
        alter_strategy: AlterStrategy::Direct,
        dialect_id: dialect_id.to_string(),
        index_methods: Vec::new(),
    }
}

/// Plan T-SQL DDL for a structure change request.
pub fn plan_structure_changes(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    match request.mode {
        StructureChangeMode::Create => plan_create_table(caps, request),
        StructureChangeMode::Alter => plan_alter_table(caps, request),
    }
}

fn plan_create_table(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    if !caps.create_table {
        return Err(unsupported("create_table is not supported"));
    }
    if request.current_columns.is_empty() {
        return Err(unsupported("create table requires at least one column"));
    }
    validate_column_drafts(caps, &request.current_columns)?;
    validate_index_drafts(caps, &request.current_indexes)?;

    let table = quote_table(request.schema.as_deref(), &request.table);
    let mut defs: Vec<String> = request
        .current_columns
        .iter()
        .map(|c| format_column_definition(c, true))
        .collect();

    if let Some(pk) = primary_key_clause(&request.current_columns) {
        defs.push(pk);
    }

    for idx in &request.current_indexes {
        if idx.is_primary {
            continue;
        }
        defs.push(format_inline_index_definition(idx));
    }

    let sql = format!("CREATE TABLE {} (\n  {}\n)", table, defs.join(",\n  "));

    Ok(StructureChangePlan {
        statements: vec![PlanStatement {
            sql,
            summary: format!("Create table {}", request.table),
            risk: StatementRisk::Additive,
        }],
        warnings: Vec::new(),
    })
}

fn plan_alter_table(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    let original_cols = index_columns_by_id(&request.original_columns);
    let current_cols = index_columns_by_id(&request.current_columns);
    let original_idx = index_indexes_by_id(&request.original_indexes);
    let current_idx = index_indexes_by_id(&request.current_indexes);

    let mut alter_clauses: Vec<String> = Vec::new();
    let mut extra_stmts: Vec<PlanStatement> = Vec::new();

    // ── dropped indexes ──
    for (id, idx) in &original_idx {
        if current_idx.contains_key(id) {
            continue;
        }
        if !caps.drop_index {
            return Err(unsupported("drop_index is not supported"));
        }
        if idx.is_primary {
            return Err(unsupported("dropping the primary key is not supported"));
        }
        alter_clauses.push(format!(
            "DROP INDEX {} ON {}",
            quote_ident(&idx.name),
            quote_table(request.schema.as_deref(), &request.table)
        ));
    }

    // ── dropped columns ──
    for (id, col) in &original_cols {
        if current_cols.contains_key(id) {
            continue;
        }
        if !caps.drop_column {
            return Err(unsupported("drop_column is not supported"));
        }
        alter_clauses.push(format!("DROP COLUMN {}", quote_ident(&col.name)));
    }

    // ── column adds, renames, and definition changes ──
    for col in &request.current_columns {
        match original_cols.get(col.id.as_str()) {
            None => {
                if !caps.add_column {
                    return Err(unsupported("add_column is not supported"));
                }
                validate_column_draft(caps, col)?;
                let def = format_column_definition(col, true);
                alter_clauses.push(format!("ADD {def}"));
            }
            Some(orig) => {
                let renamed = orig.name != col.name;
                if renamed {
                    if !caps.rename_column {
                        return Err(unsupported("rename_column is not supported"));
                    }
                    let old =
                        quote_rename_target(request.schema.as_deref(), &request.table, &orig.name);
                    extra_stmts.push(PlanStatement {
                        sql: format!(
                            "EXEC sp_rename '{}', '{}', 'COLUMN'",
                            old,
                            escape_single(&col.name)
                        ),
                        summary: format!("Rename column {} to {}", orig.name, col.name),
                        risk: StatementRisk::Rewrite,
                    });
                }

                let type_changed = orig.data_type != col.data_type;
                let null_changed = orig.nullable != col.nullable;
                let auto_inc_changed = orig.is_auto_increment != col.is_auto_increment;
                if type_changed || null_changed {
                    if type_changed && !caps.alter_type {
                        return Err(unsupported("alter_type is not supported"));
                    }
                    if null_changed && !caps.alter_nullability {
                        return Err(unsupported("alter_nullability is not supported"));
                    }
                    let null_kw = if col.nullable { "NULL" } else { "NOT NULL" };
                    alter_clauses.push(format!(
                        "ALTER COLUMN {} {} {}",
                        quote_ident(&col.name),
                        col.data_type,
                        null_kw
                    ));
                }
                if auto_inc_changed {
                    return Err(unsupported(
                        "changing IDENTITY is not supported; drop and recreate the column",
                    ));
                }
                // Default / comment / primary key changes are intentionally
                // unsupported (caps disabled) — T-SQL constraint plumbing.
                let def_changed = orig.default_value != col.default_value;
                let pk_changed = orig.is_primary_key != col.is_primary_key;
                if def_changed {
                    return Err(unsupported("alter_default is not supported"));
                }
                if pk_changed {
                    return Err(unsupported("alter_primary_key is not supported"));
                }
            }
        }
    }

    // ── new indexes ──
    for (id, idx) in &current_idx {
        if original_idx.contains_key(id) {
            continue;
        }
        if !caps.create_index {
            return Err(unsupported("create_index is not supported"));
        }
        if idx.is_primary {
            return Err(unsupported("adding a primary key index is not supported"));
        }
        validate_index_draft(caps, idx)?;

        let unique = if idx.is_unique { "UNIQUE " } else { "" };
        let cols = idx
            .columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let table = quote_table(request.schema.as_deref(), &request.table);
        extra_stmts.push(PlanStatement {
            sql: format!(
                "CREATE {}INDEX {} ON {} ({})",
                unique,
                quote_ident(&idx.name),
                table,
                cols
            ),
            summary: format!("Create index {}", idx.name),
            risk: StatementRisk::Additive,
        });
    }

    // ── modified indexes (unsupported; drop + recreate) ──
    for (id, cur) in &current_idx {
        if let Some(orig) = original_idx.get(id) {
            if index_definition_changed(orig, cur) {
                return Err(unsupported(
                    "modifying an existing index is not supported; drop and recreate it",
                ));
            }
        }
    }

    let table = quote_table(request.schema.as_deref(), &request.table);
    let mut statements: Vec<PlanStatement> = Vec::new();

    if !alter_clauses.is_empty() {
        statements.push(PlanStatement {
            sql: format!("ALTER TABLE {} {}", table, alter_clauses.join(", ")),
            summary: format!("Alter table {}", request.table),
            risk: classify_alter_risk(&alter_clauses),
        });
    }

    statements.extend(extra_stmts);

    if statements.is_empty() {
        return Err(unsupported("no structure changes detected"));
    }

    Ok(StructureChangePlan {
        statements,
        warnings: Vec::new(),
    })
}

// ░░ helpers ░░

fn format_column_definition(col: &StructureColumnDraft, include_name: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    if include_name {
        parts.push(quote_ident(&col.name));
    }
    parts.push(col.data_type.clone());

    if col.is_auto_increment {
        parts.push("IDENTITY(1,1)".into());
    }

    if !col.nullable {
        parts.push("NOT NULL".into());
    } else {
        parts.push("NULL".into());
    }

    match &col.default_value {
        Some(v) if !v.is_empty() && !v.eq_ignore_ascii_case("NULL") => {
            parts.push(format!("DEFAULT {v}"));
        }
        _ => {}
    }

    parts.join(" ")
}

fn format_inline_index_definition(idx: &StructureIndexDraft) -> String {
    let cols = idx
        .columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let unique = if idx.is_unique { "UNIQUE " } else { "" };
    format!("{}INDEX {} ({})", unique, quote_ident(&idx.name), cols)
}

fn primary_key_clause(columns: &[StructureColumnDraft]) -> Option<String> {
    let pks: Vec<String> = columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| quote_ident(&c.name))
        .collect();
    if pks.is_empty() {
        None
    } else {
        Some(format!("PRIMARY KEY ({})", pks.join(", ")))
    }
}

fn validate_column_drafts(
    caps: &StructureCapabilities,
    columns: &[StructureColumnDraft],
) -> Result<(), DriverError> {
    for col in columns {
        validate_column_draft(caps, col)?;
    }
    Ok(())
}

fn validate_column_draft(
    caps: &StructureCapabilities,
    col: &StructureColumnDraft,
) -> Result<(), DriverError> {
    if caps.comment && col.comment.as_ref().is_some_and(|c| !c.is_empty()) {
        return Err(unsupported("comment is not supported"));
    }
    Ok(())
}

fn validate_index_drafts(
    caps: &StructureCapabilities,
    indexes: &[StructureIndexDraft],
) -> Result<(), DriverError> {
    for idx in indexes {
        validate_index_draft(caps, idx)?;
    }
    Ok(())
}

fn validate_index_draft(
    _caps: &StructureCapabilities,
    idx: &StructureIndexDraft,
) -> Result<(), DriverError> {
    if idx.columns.is_empty() {
        return Err(unsupported("index requires at least one column"));
    }
    Ok(())
}

fn index_definition_changed(a: &StructureIndexDraft, b: &StructureIndexDraft) -> bool {
    a.is_unique != b.is_unique
        || a.columns != b.columns
        || a.is_primary != b.is_primary
        || a.index_type != b.index_type
        || a.include_columns != b.include_columns
        || a.filter != b.filter
        || a.comment != b.comment
}

fn classify_alter_risk(_clauses: &[String]) -> StatementRisk {
    // Dropping columns or indexes carries real risk; anything else is additive/MOD.
    if _clauses.iter().any(|c| c.contains("DROP ")) {
        StatementRisk::Destructive
    } else if _clauses.iter().any(|c| c.contains("ALTER COLUMN")) {
        StatementRisk::Rewrite
    } else {
        StatementRisk::Additive
    }
}

fn index_columns_by_id(columns: &[StructureColumnDraft]) -> HashMap<&str, &StructureColumnDraft> {
    columns.iter().map(|c| (c.id.as_str(), c)).collect()
}

fn index_indexes_by_id(indexes: &[StructureIndexDraft]) -> HashMap<&str, &StructureIndexDraft> {
    indexes.iter().map(|i| (i.id.as_str(), i)).collect()
}

fn quote_ident(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

fn quote_table(schema: Option<&str>, table: &str) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
}

/// sp_rename target uses a 3-part-ish literal: [schema].[table].[oldcol]
fn quote_rename_target(schema: Option<&str>, table: &str, column: &str) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(s) => format!(
            "{}.{}.{}",
            quote_ident(s),
            quote_ident(table),
            quote_ident(column)
        ),
        None => format!("{}.{}", quote_ident(table), quote_ident(column)),
    }
}

fn escape_single(value: &str) -> String {
    value.replace('\'', "''")
}

fn unsupported(message: impl Into<String>) -> DriverError {
    DriverError::Unsupported(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(
        id: &str,
        name: &str,
        data_type: &str,
        nullable: bool,
        default: Option<&str>,
    ) -> StructureColumnDraft {
        StructureColumnDraft {
            id: id.into(),
            name: name.into(),
            data_type: data_type.into(),
            nullable,
            default_value: default.map(Into::into),
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
            is_unique: false,
        }
    }

    fn idx(id: &str, name: &str, columns: &[&str], unique: bool) -> StructureIndexDraft {
        StructureIndexDraft {
            id: id.into(),
            name: name.into(),
            columns: columns.iter().map(|c| c.to_string()).collect(),
            is_unique: unique,
            is_primary: false,
            index_type: String::new(),
            include_columns: Vec::new(),
            filter: None,
            comment: None,
        }
    }

    #[test]
    fn capabilities_reflect_tsql_limits() {
        let caps = sqlserver_capabilities("sqlserver");
        assert!(caps.create_table);
        assert!(caps.add_column);
        assert!(caps.rename_column);
        assert!(!caps.reorder_column, "T-SQL cannot reorder columns");
        assert!(!caps.alter_default, "default requires constraint plumbing");
        assert!(!caps.alter_primary_key);
        assert_eq!(caps.alter_strategy, AlterStrategy::Direct);
    }

    #[test]
    fn create_table_emits_tsql() {
        let caps = sqlserver_capabilities("sqlserver");
        let req = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: Some("dbo".into()),
            table: "users".into(),
            original_columns: Vec::new(),
            current_columns: vec![
                {
                    let mut c = col("1", "id", "int", false, None);
                    c.is_auto_increment = true;
                    c.is_primary_key = true;
                    c
                },
                col("2", "name", "nvarchar(100)", true, Some("N'anon'")),
            ],
            original_indexes: Vec::new(),
            current_indexes: vec![idx("i", "ix_users_name", &["name"], false)],
        };
        let plan = plan_structure_changes(&caps, &req).unwrap();
        let sql = &plan.statements[0].sql;
        assert!(sql.contains("CREATE TABLE [dbo].[users]"));
        assert!(sql.contains("[id] int IDENTITY(1,1) NOT NULL"));
        assert!(sql.contains("PRIMARY KEY ([id])"));
        assert!(sql.contains("INDEX [ix_users_name] ([name])"));
    }

    #[test]
    fn add_and_drop_column() {
        let caps = sqlserver_capabilities("sqlserver");
        let orig = vec![
            col("1", "a", "int", false, None),
            col("2", "b", "int", true, None),
        ];
        let curr = vec![
            col("1", "a", "int", false, None),
            col("3", "c", "decimal(10,2)", true, None),
        ];
        let req = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: orig.clone(),
            current_columns: curr,
            original_indexes: Vec::new(),
            current_indexes: Vec::new(),
        };
        let plan = plan_structure_changes(&caps, &req).unwrap();
        let sql = &plan.statements[0].sql;
        assert!(sql.contains("DROP COLUMN [b]"));
        assert!(sql.contains("ADD [c] decimal(10,2) NULL"));
    }

    #[test]
    fn alter_type_rewrites_definition() {
        let caps = sqlserver_capabilities("sqlserver");
        let orig = col("a", "a", "int", false, None);
        let mut next = col("a", "a", "bigint", false, None);
        next.nullable = true;
        let req = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: vec![orig],
            current_columns: vec![next],
            original_indexes: Vec::new(),
            current_indexes: Vec::new(),
        };
        let plan = plan_structure_changes(&caps, &req).unwrap();
        let sql = &plan.statements[0].sql;
        assert!(sql.contains("ALTER COLUMN [a] bigint NULL"));
    }

    #[test]
    fn rename_uses_sp_rename() {
        let caps = sqlserver_capabilities("sqlserver");
        let orig = col("a", "old", "int", true, None);
        let next = col("a", "new", "int", true, None);
        let req = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: Some("dbo".into()),
            table: "t".into(),
            original_columns: vec![orig],
            current_columns: vec![next],
            original_indexes: Vec::new(),
            current_indexes: Vec::new(),
        };
        let plan = plan_structure_changes(&caps, &req).unwrap();
        let sql = &plan.statements[0].sql;
        assert!(sql.contains("EXEC sp_rename '[dbo].[t].[old]', 'new', 'COLUMN'"));
    }

    #[test]
    fn create_index_emits_tsql() {
        let caps = sqlserver_capabilities("sqlserver");
        let req = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "t".into(),
            original_columns: vec![col("a", "a", "int", true, None)],
            current_columns: vec![col("a", "a", "int", true, None)],
            original_indexes: Vec::new(),
            current_indexes: vec![idx("i", "ix_t_a", &["a"], true)],
        };
        let plan = plan_structure_changes(&caps, &req).unwrap();
        let sql = &plan.statements[0].sql;
        assert!(sql.contains("CREATE UNIQUE INDEX [ix_t_a] ON [t] ([a])"));
    }
}
