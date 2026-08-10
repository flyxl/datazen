//! Table structure editor capabilities and DDL planning for SQLite.

use datazen_driver_api::*;

/// Static capability flags for SQLite (P1: additive ALTER + index ops only).
pub fn capabilities(dialect_id: &str) -> StructureCapabilities {
    StructureCapabilities {
        create_table: true,
        add_column: true,
        drop_column: false,
        rename_column: false,
        alter_type: false,
        alter_nullability: false,
        alter_default: false,
        alter_primary_key: false,
        reorder_column: false,
        comment: false,
        create_index: true,
        drop_index: true,
        rebuild_index: false,
        index_type: false,
        index_include: false,
        index_filter: true,
        index_comment: false,
        alter_strategy: AlterStrategy::SqliteRebuild,
        dialect_id: dialect_id.to_string(),
        index_methods: vec!["btree".to_string()],
    }
}

/// Plan DDL statements from a structure change request.
pub fn plan_changes(
    request: &StructureChangeRequest,
    caps: &StructureCapabilities,
) -> Result<StructureChangePlan, DriverError> {
    match request.mode {
        StructureChangeMode::Create => plan_create(request, caps),
        StructureChangeMode::Alter => plan_alter(request, caps),
    }
}

fn plan_create(
    request: &StructureChangeRequest,
    caps: &StructureCapabilities,
) -> Result<StructureChangePlan, DriverError> {
    if !caps.create_table {
        return Err(unsupported("create table is not supported for SQLite"));
    }
    if request.current_columns.is_empty() {
        return Err(unsupported("create table requires at least one column"));
    }

    let table = quote_ident(&request.table);
    let mut parts: Vec<String> = request
        .current_columns
        .iter()
        .map(render_column_def_for_create)
        .collect();

    let pk_cols: Vec<&str> = request
        .current_columns
        .iter()
        .filter(|c| c.is_primary_key && !column_has_inline_primary_key(c))
        .map(|c| c.name.as_str())
        .collect();
    if !pk_cols.is_empty() {
        let pk = pk_cols
            .iter()
            .map(|name| quote_ident(name))
            .collect::<Vec<_>>()
            .join(", ");
        parts.push(format!("PRIMARY KEY ({pk})"));
    }

    let mut statements = vec![PlanStatement {
        sql: format!("CREATE TABLE {table} (\n  {}\n)", parts.join(",\n  ")),
        summary: format!("Create table {}", request.table),
        risk: StatementRisk::Additive,
    }];

    for idx in &request.current_indexes {
        if idx.is_primary {
            continue;
        }
        statements.push(plan_create_index(request, idx, caps)?);
    }

    Ok(StructureChangePlan {
        statements,
        warnings: vec![],
    })
}

fn plan_alter(
    request: &StructureChangeRequest,
    caps: &StructureCapabilities,
) -> Result<StructureChangePlan, DriverError> {
    let mut statements = Vec::new();
    let mut warnings = Vec::new();

    let original_by_id: std::collections::HashMap<&str, &StructureColumnDraft> = request
        .original_columns
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();
    let current_by_id: std::collections::HashMap<&str, &StructureColumnDraft> = request
        .current_columns
        .iter()
        .map(|c| (c.id.as_str(), c))
        .collect();

    for col in &request.current_columns {
        if !original_by_id.contains_key(col.id.as_str()) {
            if !caps.add_column {
                return Err(unsupported("add column is not supported for SQLite"));
            }
            statements.push(plan_add_column(&request.table, col));
        }
    }

    for orig in &request.original_columns {
        if !current_by_id.contains_key(orig.id.as_str()) {
            require_cap(caps.drop_column, "drop column")?;
            statements.push(plan_drop_column(&request.table, orig));
        }
    }

    for col in &request.current_columns {
        let Some(orig) = original_by_id.get(col.id.as_str()) else {
            continue;
        };
        if orig.name != col.name {
            require_cap(caps.rename_column, "rename column")?;
            statements.push(plan_rename_column(&request.table, orig, col));
        }
        if orig.data_type != col.data_type {
            require_cap(caps.alter_type, "alter column type")?;
            statements.push(plan_alter_column_type(&request.table, col));
        }
        if orig.nullable != col.nullable {
            require_cap(caps.alter_nullability, "alter column nullability")?;
            statements.push(plan_alter_nullability(&request.table, col));
        }
        if orig.default_value != col.default_value {
            require_cap(caps.alter_default, "alter column default")?;
            statements.push(plan_alter_default(&request.table, col));
        }
        if orig.is_primary_key != col.is_primary_key {
            require_cap(caps.alter_primary_key, "alter primary key")?;
            statements.push(plan_alter_primary_key(&request.table, request, col));
        }
        if orig.comment != col.comment {
            require_cap(caps.comment, "column comment")?;
        }
    }

    if caps.reorder_column {
        if column_order_changed(&request.original_columns, &request.current_columns) {
            statements.push(plan_reorder_columns(&request.table, request)?);
        }
    } else if column_order_changed(&request.original_columns, &request.current_columns) {
        return Err(unsupported(
            "SQLite does not support reordering columns without a table rebuild",
        ));
    }

    let orig_idx: std::collections::HashMap<&str, &StructureIndexDraft> = request
        .original_indexes
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect();
    let curr_idx: std::collections::HashMap<&str, &StructureIndexDraft> = request
        .current_indexes
        .iter()
        .map(|i| (i.id.as_str(), i))
        .collect();

    for idx in &request.current_indexes {
        if idx.is_primary {
            continue;
        }
        match orig_idx.get(idx.id.as_str()) {
            None => statements.push(plan_create_index(request, idx, caps)?),
            Some(orig) if index_changed(orig, idx) => {
                require_cap(caps.drop_index, "drop index")?;
                statements.push(plan_drop_index(idx));
                statements.push(plan_create_index(request, idx, caps)?);
            }
            _ => {}
        }
    }

    for orig in &request.original_indexes {
        if orig.is_primary {
            continue;
        }
        if !curr_idx.contains_key(orig.id.as_str()) {
            require_cap(caps.drop_index, "drop index")?;
            statements.push(plan_drop_index(orig));
        }
    }

    if statements.is_empty() && warnings.is_empty() {
        warnings.push("No structural changes detected".into());
    }

    Ok(StructureChangePlan {
        statements,
        warnings,
    })
}

fn require_cap(enabled: bool, operation: &str) -> Result<(), DriverError> {
    if enabled {
        Ok(())
    } else {
        Err(unsupported(&format!(
            "{operation} is not supported for SQLite without rebuilding the table"
        )))
    }
}

fn unsupported(message: &str) -> DriverError {
    DriverError::Unsupported(message.to_string())
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn column_has_inline_primary_key(col: &StructureColumnDraft) -> bool {
    col.is_auto_increment
}

fn render_column_def_for_create(col: &StructureColumnDraft) -> String {
    let mut s = format!("{} {}", quote_ident(&col.name), col.data_type);
    if col.is_auto_increment {
        s.push_str(" PRIMARY KEY AUTOINCREMENT");
    } else if !col.nullable {
        s.push_str(" NOT NULL");
    }
    if col.is_unique && !col.is_primary_key {
        s.push_str(" UNIQUE");
    }
    if let Some(ref default) = col.default_value {
        s.push_str(&format!(" DEFAULT {default}"));
    }
    s
}

fn render_column_def_for_add(col: &StructureColumnDraft) -> String {
    let mut s = format!("{} {}", quote_ident(&col.name), col.data_type);
    if !col.nullable {
        s.push_str(" NOT NULL");
    }
    if let Some(ref default) = col.default_value {
        s.push_str(&format!(" DEFAULT {default}"));
    }
    s
}

fn plan_add_column(table: &str, col: &StructureColumnDraft) -> PlanStatement {
    PlanStatement {
        sql: format!(
            "ALTER TABLE {} ADD COLUMN {}",
            quote_ident(table),
            render_column_def_for_add(col)
        ),
        summary: format!("Add column {}", col.name),
        risk: StatementRisk::Additive,
    }
}

fn plan_drop_column(table: &str, col: &StructureColumnDraft) -> PlanStatement {
    PlanStatement {
        sql: format!(
            "ALTER TABLE {} DROP COLUMN {}",
            quote_ident(table),
            quote_ident(&col.name)
        ),
        summary: format!("Drop column {}", col.name),
        risk: StatementRisk::Destructive,
    }
}

fn plan_rename_column(
    table: &str,
    orig: &StructureColumnDraft,
    col: &StructureColumnDraft,
) -> PlanStatement {
    PlanStatement {
        sql: format!(
            "ALTER TABLE {} RENAME COLUMN {} TO {}",
            quote_ident(table),
            quote_ident(&orig.name),
            quote_ident(&col.name)
        ),
        summary: format!("Rename column {} to {}", orig.name, col.name),
        risk: StatementRisk::Rewrite,
    }
}

fn plan_alter_column_type(_table: &str, col: &StructureColumnDraft) -> PlanStatement {
    PlanStatement {
        sql: format!(
            "-- table rebuild required to change type of {}",
            quote_ident(&col.name)
        ),
        summary: format!("Change column {} type to {}", col.name, col.data_type),
        risk: StatementRisk::Rewrite,
    }
}

fn plan_alter_nullability(_table: &str, col: &StructureColumnDraft) -> PlanStatement {
    PlanStatement {
        sql: format!(
            "-- table rebuild required to change nullability of {}",
            quote_ident(&col.name)
        ),
        summary: format!(
            "Change column {} nullability to {}",
            col.name,
            if col.nullable { "NULL" } else { "NOT NULL" }
        ),
        risk: StatementRisk::Rewrite,
    }
}

fn plan_alter_default(_table: &str, col: &StructureColumnDraft) -> PlanStatement {
    PlanStatement {
        sql: format!(
            "-- table rebuild required to change default of {}",
            quote_ident(&col.name)
        ),
        summary: format!("Change column {} default", col.name),
        risk: StatementRisk::Rewrite,
    }
}

fn plan_alter_primary_key(
    _table: &str,
    _request: &StructureChangeRequest,
    col: &StructureColumnDraft,
) -> PlanStatement {
    PlanStatement {
        sql: format!(
            "-- table rebuild required to change primary key on {}",
            quote_ident(&col.name)
        ),
        summary: format!("Change primary key on column {}", col.name),
        risk: StatementRisk::Rewrite,
    }
}

fn plan_reorder_columns(
    table: &str,
    request: &StructureChangeRequest,
) -> Result<PlanStatement, DriverError> {
    Ok(PlanStatement {
        sql: format!(
            "-- table rebuild required to reorder columns on {}",
            quote_ident(table)
        ),
        summary: format!("Reorder columns on table {}", request.table),
        risk: StatementRisk::Rewrite,
    })
}

fn plan_create_index(
    request: &StructureChangeRequest,
    idx: &StructureIndexDraft,
    caps: &StructureCapabilities,
) -> Result<PlanStatement, DriverError> {
    if !caps.create_index {
        return Err(unsupported("create index is not supported for SQLite"));
    }
    if !idx.include_columns.is_empty() {
        require_cap(caps.index_include, "index include columns")?;
    }
    if !idx.index_type.is_empty() && idx.index_type != "btree" {
        require_cap(caps.index_type, "index type")?;
    }
    if idx.comment.is_some() {
        require_cap(caps.index_comment, "index comment")?;
    }

    let unique = if idx.is_unique { "UNIQUE " } else { "" };
    let cols = idx
        .columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "CREATE {unique}INDEX {} ON {} ({cols})",
        quote_ident(&idx.name),
        quote_ident(&request.table),
    );
    if let Some(ref filter) = idx.filter {
        if !filter.is_empty() {
            require_cap(caps.index_filter, "partial index filter")?;
            sql.push_str(&format!(" WHERE {filter}"));
        }
    }

    Ok(PlanStatement {
        sql,
        summary: format!("Create index {}", idx.name),
        risk: StatementRisk::Additive,
    })
}

fn plan_drop_index(idx: &StructureIndexDraft) -> PlanStatement {
    PlanStatement {
        sql: format!("DROP INDEX {}", quote_ident(&idx.name)),
        summary: format!("Drop index {}", idx.name),
        risk: StatementRisk::Destructive,
    }
}

fn column_order_changed(
    original: &[StructureColumnDraft],
    current: &[StructureColumnDraft],
) -> bool {
    let orig_ids: Vec<&str> = original.iter().map(|c| c.id.as_str()).collect();
    let shared_order: Vec<&str> = current
        .iter()
        .map(|c| c.id.as_str())
        .filter(|id| orig_ids.contains(id))
        .collect();
    shared_order != orig_ids
}

fn index_changed(orig: &StructureIndexDraft, curr: &StructureIndexDraft) -> bool {
    orig.name != curr.name
        || orig.columns != curr.columns
        || orig.is_unique != curr.is_unique
        || orig.index_type != curr.index_type
        || orig.include_columns != curr.include_columns
        || orig.filter != curr.filter
        || orig.comment != curr.comment
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sqlite_caps() -> StructureCapabilities {
        capabilities("sqlite")
    }

    fn col(id: &str, name: &str, data_type: &str) -> StructureColumnDraft {
        StructureColumnDraft {
            id: id.into(),
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
            is_unique: false,
        }
    }

    fn idx(id: &str, name: &str, columns: &[&str]) -> StructureIndexDraft {
        StructureIndexDraft {
            id: id.into(),
            name: name.into(),
            columns: columns.iter().map(|c| (*c).to_string()).collect(),
            is_unique: false,
            is_primary: false,
            index_type: String::new(),
            include_columns: vec![],
            filter: None,
            comment: None,
        }
    }

    #[test]
    fn structure_capabilities_use_sqlite_rebuild_strategy() {
        let caps = sqlite_caps();
        assert_eq!(caps.alter_strategy, AlterStrategy::SqliteRebuild);
        assert_eq!(caps.dialect_id, "sqlite");
    }

    #[test]
    fn structure_capabilities_enable_additive_ops_only() {
        let caps = sqlite_caps();
        assert!(caps.create_table);
        assert!(caps.add_column);
        assert!(caps.create_index);
        assert!(caps.drop_index);
        assert!(caps.index_filter);
        assert!(!caps.drop_column);
        assert!(!caps.rename_column);
        assert!(!caps.alter_type);
        assert!(!caps.alter_nullability);
        assert!(!caps.alter_default);
        assert!(!caps.alter_primary_key);
        assert!(!caps.reorder_column);
        assert!(!caps.comment);
        assert!(!caps.rebuild_index);
        assert!(!caps.index_type);
        assert!(!caps.index_include);
        assert!(!caps.index_comment);
        assert_eq!(caps.index_methods, vec!["btree"]);
    }

    #[test]
    fn plan_add_column_generates_alter_table_add_column() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![col("c1", "id", "INTEGER")],
            current_columns: vec![
                col("c1", "id", "INTEGER"),
                col("c2", "email", "TEXT"),
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_changes(&request, &sqlite_caps()).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE \"users\" ADD COLUMN \"email\" TEXT"
        );
        assert_eq!(plan.statements[0].risk, StatementRisk::Additive);
        assert_eq!(plan.statements[0].summary, "Add column email");
    }

    #[test]
    fn plan_alter_type_returns_unsupported_when_cap_disabled() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![col("c1", "id", "INTEGER")],
            current_columns: vec![col("c1", "id", "TEXT")],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_changes(&request, &sqlite_caps()).unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(msg) if msg.contains("alter column type")));
    }

    #[test]
    fn plan_drop_column_returns_unsupported_when_cap_disabled() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![
                col("c1", "id", "INTEGER"),
                col("c2", "email", "TEXT"),
            ],
            current_columns: vec![col("c1", "id", "INTEGER")],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_changes(&request, &sqlite_caps()).unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(msg) if msg.contains("drop column")));
    }

    #[test]
    fn plan_create_index_generates_sql() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![col("c1", "id", "INTEGER")],
            current_columns: vec![col("c1", "id", "INTEGER")],
            original_indexes: vec![],
            current_indexes: vec![idx("i1", "idx_users_email", &["email"])],
        };

        let plan = plan_changes(&request, &sqlite_caps()).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            "CREATE INDEX \"idx_users_email\" ON \"users\" (\"email\")"
        );
        assert_eq!(plan.statements[0].risk, StatementRisk::Additive);
    }

    #[test]
    fn plan_drop_index_generates_sql() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![col("c1", "id", "INTEGER")],
            current_columns: vec![col("c1", "id", "INTEGER")],
            original_indexes: vec![idx("i1", "idx_users_email", &["email"])],
            current_indexes: vec![],
        };

        let plan = plan_changes(&request, &sqlite_caps()).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(plan.statements[0].sql, "DROP INDEX \"idx_users_email\"");
        assert_eq!(plan.statements[0].risk, StatementRisk::Destructive);
    }

    #[test]
    fn plan_create_table_generates_sql() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: None,
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![
                StructureColumnDraft {
                    id: "c1".into(),
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: true,
                    is_unique: false,
                },
                col("c2", "name", "TEXT"),
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_changes(&request, &sqlite_caps()).unwrap();
        assert_eq!(plan.statements.len(), 1);
        let sql = &plan.statements[0].sql;
        assert_eq!(
            sql,
            "CREATE TABLE \"users\" (\n  \"id\" INTEGER PRIMARY KEY AUTOINCREMENT,\n  \"name\" TEXT\n)"
        );
        assert_eq!(sql.matches("PRIMARY KEY").count(), 1);
    }

    #[test]
    fn plan_create_table_column_unique() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: None,
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![
                col("c1", "id", "INTEGER"),
                StructureColumnDraft {
                    is_unique: true,
                    ..col("c2", "email", "TEXT")
                },
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_changes(&request, &sqlite_caps()).unwrap();
        assert!(plan.statements[0].sql.contains(r#""email" TEXT UNIQUE"#));
    }

    #[test]
    fn plan_create_table_composite_primary_key_uses_table_level_constraint() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: None,
            table: "pairs".into(),
            original_columns: vec![],
            current_columns: vec![
                StructureColumnDraft {
                    id: "c1".into(),
                    name: "a".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                    is_unique: false,
                },
                StructureColumnDraft {
                    id: "c2".into(),
                    name: "b".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: false,
                    is_unique: false,
                },
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_changes(&request, &sqlite_caps()).unwrap();
        assert_eq!(
            plan.statements[0].sql,
            "CREATE TABLE \"pairs\" (\n  \"a\" INTEGER NOT NULL,\n  \"b\" INTEGER NOT NULL,\n  PRIMARY KEY (\"a\", \"b\")\n)"
        );
    }

    #[test]
    fn disabled_cap_consistency_returns_unsupported_on_request() {
        let caps = sqlite_caps();
        assert!(!caps.rename_column);

        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![col("c1", "id", "INTEGER")],
            current_columns: vec![col("c1", "user_id", "INTEGER")],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_changes(&request, &caps).unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(msg) if msg.contains("rename column")));
    }

    #[test]
    fn plan_reorder_columns_returns_unsupported_when_cap_disabled() {
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![col("c1", "id", "INTEGER"), col("c2", "name", "TEXT")],
            current_columns: vec![col("c2", "name", "TEXT"), col("c1", "id", "INTEGER")],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_changes(&request, &sqlite_caps()).unwrap_err();
        assert!(matches!(
            err,
            DriverError::Unsupported(msg) if msg.contains("reordering columns")
        ));
    }
}
