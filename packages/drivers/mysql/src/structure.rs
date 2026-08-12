//! MySQL / MariaDB table structure capabilities and DDL planning.

use datazen_driver_api::*;
use std::collections::HashMap;

/// Fixed P1 capability baseline for MySQL-compatible drivers.
pub fn baseline_capabilities(dialect_id: &str) -> StructureCapabilities {
    StructureCapabilities {
        create_table: true,
        add_column: true,
        drop_column: true,
        rename_column: true,
        alter_type: true,
        alter_nullability: true,
        alter_default: true,
        alter_primary_key: false,
        reorder_column: true,
        comment: true,
        create_index: true,
        drop_index: true,
        rebuild_index: false,
        index_type: true,
        index_include: false,
        index_filter: false,
        index_comment: true,
        alter_strategy: AlterStrategy::Direct,
        dialect_id: dialect_id.to_string(),
        index_methods: vec!["BTREE".into(), "HASH".into()],
    }
}

/// Plan dialect DDL for a structure change request.
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
    let mut col_defs: Vec<String> = request
        .current_columns
        .iter()
        .map(|col| format_column_definition(col, true, false, caps))
        .collect();

    let pk_cols: Vec<String> = request
        .current_columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| quote_ident(&c.name))
        .collect();
    if !pk_cols.is_empty() {
        col_defs.push(format!("PRIMARY KEY ({})", pk_cols.join(", ")));
    }

    for idx in &request.current_indexes {
        if idx.is_primary {
            continue;
        }
        col_defs.push(format_index_definition(idx, caps));
    }

    let sql = format!("CREATE TABLE {} (\n  {}\n)", table, col_defs.join(",\n  "));

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
    check_reorder(caps, &request.original_columns, &request.current_columns)?;

    let original_cols = index_columns_by_id(&request.original_columns);
    let current_cols = index_columns_by_id(&request.current_columns);
    let original_idx = index_indexes_by_id(&request.original_indexes);
    let current_idx = index_indexes_by_id(&request.current_indexes);

    let mut alter_clauses: Vec<String> = Vec::new();
    let mut create_index_stmts: Vec<PlanStatement> = Vec::new();

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
        alter_clauses.push(format!("DROP INDEX {}", quote_ident(&idx.name)));
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

    // ── column modifications & additions ──
    let original_order: Vec<&str> = request
        .original_columns
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    let current_order: Vec<&str> = request
        .current_columns
        .iter()
        .map(|c| c.id.as_str())
        .collect();

    for col in &request.current_columns {
        match original_cols.get(col.id.as_str()) {
            None => {
                if !caps.add_column {
                    return Err(unsupported("add_column is not supported"));
                }
                validate_column_draft(caps, col)?;
                let mut clause = format!(
                    "ADD COLUMN {}",
                    format_column_definition(col, true, false, caps)
                );
                if caps.reorder_column {
                    if let Some(after) =
                        after_column_name(&current_order, col.id.as_str(), &current_cols)
                    {
                        clause.push_str(&format!(" AFTER {}", quote_ident(&after)));
                    } else if is_first_column(&current_order, col.id.as_str()) {
                        clause.push_str(" FIRST");
                    }
                }
                alter_clauses.push(clause);
            }
            Some(orig) => {
                let renamed = orig.name != col.name;
                let type_changed = orig.data_type != col.data_type;
                let null_changed = orig.nullable != col.nullable;
                let default_changed = orig.default_value != col.default_value;
                let comment_changed = orig.comment != col.comment;
                let auto_inc_changed = orig.is_auto_increment != col.is_auto_increment;
                let pk_changed = orig.is_primary_key != col.is_primary_key;

                if pk_changed {
                    return Err(unsupported("alter_primary_key is not supported"));
                }

                let reordered = caps.reorder_column
                    && column_order_changed(&original_order, &current_order, col.id.as_str());

                if renamed && !caps.rename_column {
                    return Err(unsupported("rename_column is not supported"));
                }
                if type_changed && !caps.alter_type {
                    return Err(unsupported("alter_type is not supported"));
                }
                if null_changed && !caps.alter_nullability {
                    return Err(unsupported("alter_nullability is not supported"));
                }
                if default_changed && !caps.alter_default {
                    return Err(unsupported("alter_default is not supported"));
                }
                if comment_changed && !caps.comment {
                    return Err(unsupported("comment is not supported"));
                }
                if auto_inc_changed {
                    return Err(unsupported("changing AUTO_INCREMENT is not supported"));
                }

                let def_changed = renamed
                    || type_changed
                    || null_changed
                    || default_changed
                    || comment_changed
                    || reordered;

                if def_changed {
                    let positioning = if reordered && caps.reorder_column {
                        column_position_suffix(&current_order, col.id.as_str(), &current_cols)
                    } else {
                        String::new()
                    };

                    if renamed {
                        alter_clauses.push(format!(
                            "CHANGE COLUMN {} {}{}",
                            quote_ident(&orig.name),
                            format_column_definition(col, true, false, caps),
                            positioning
                        ));
                    } else {
                        alter_clauses.push(format!(
                            "MODIFY COLUMN {}{}",
                            format_column_definition(col, true, false, caps),
                            positioning
                        ));
                    }
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

        let table = quote_table(request.schema.as_deref(), &request.table);
        let unique = if idx.is_unique { "UNIQUE " } else { "" };
        let method = normalize_index_method(&idx.index_type);

        let cols = idx
            .columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        let mut sql = format!(
            "CREATE {}INDEX {} ON {} ({})",
            unique,
            quote_ident(&idx.name),
            table,
            cols
        );
        if caps.index_type && !idx.index_type.trim().is_empty() {
            sql.push_str(&format!(" USING {method}"));
        }
        if let Some(comment) = &idx.comment {
            if !comment.is_empty() {
                sql.push_str(&format!(" COMMENT {}", quote_string_literal(comment)));
            }
        }

        create_index_stmts.push(PlanStatement {
            sql,
            summary: format!("Create index {}", idx.name),
            risk: StatementRisk::Additive,
        });
    }

    // ── modified indexes (unsupported in P1 — require drop + create) ──
    for (id, cur) in &current_idx {
        if let Some(orig) = original_idx.get(*id) {
            if index_definition_changed(orig, cur) {
                return Err(unsupported(
                    "modifying an existing index is not supported; drop and recreate it",
                ));
            }
        }
    }

    validate_index_drafts(caps, &request.current_indexes)?;

    let table = quote_table(request.schema.as_deref(), &request.table);
    let mut statements: Vec<PlanStatement> = Vec::new();

    if !alter_clauses.is_empty() {
        statements.push(PlanStatement {
            sql: format!("ALTER TABLE {} {}", table, alter_clauses.join(", ")),
            summary: format!("Alter table {}", request.table),
            risk: classify_alter_risk(&alter_clauses),
        });
    }

    statements.extend(create_index_stmts);

    if statements.is_empty() {
        return Err(unsupported("no structure changes detected"));
    }

    Ok(StructureChangePlan {
        statements,
        warnings: Vec::new(),
    })
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
    if col.comment.as_ref().is_some_and(|c| !c.is_empty()) && !caps.comment {
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
    caps: &StructureCapabilities,
    idx: &StructureIndexDraft,
) -> Result<(), DriverError> {
    if !idx.include_columns.is_empty() && !caps.index_include {
        return Err(unsupported("index_include is not supported"));
    }
    if idx.filter.is_some() && !caps.index_filter {
        return Err(unsupported("index_filter is not supported"));
    }
    if !idx.index_type.trim().is_empty() && !caps.index_type {
        return Err(unsupported("index_type is not supported"));
    }
    if idx.comment.as_ref().is_some_and(|c| !c.is_empty()) && !caps.index_comment {
        return Err(unsupported("index_comment is not supported"));
    }

    let method = normalize_index_method(&idx.index_type);
    if !caps.index_methods.is_empty() {
        if !caps
            .index_methods
            .iter()
            .any(|m| m.eq_ignore_ascii_case(&method))
        {
            return Err(unsupported(format!(
                "index method `{method}` is not supported"
            )));
        }
    } else if !idx.index_type.trim().is_empty() {
        return Err(unsupported(format!(
            "index method `{method}` is not supported"
        )));
    }

    Ok(())
}

fn check_reorder(
    caps: &StructureCapabilities,
    original: &[StructureColumnDraft],
    current: &[StructureColumnDraft],
) -> Result<(), DriverError> {
    if caps.reorder_column {
        return Ok(());
    }
    let orig_ids: Vec<&str> = original.iter().map(|c| c.id.as_str()).collect();
    let curr_ids: Vec<&str> = current.iter().map(|c| c.id.as_str()).collect();
    let orig_set: std::collections::HashSet<&str> = orig_ids.iter().copied().collect();
    let shared_orig: Vec<&str> = orig_ids
        .iter()
        .copied()
        .filter(|id| current.iter().any(|c| c.id.as_str() == *id))
        .collect();
    let shared_curr: Vec<&str> = curr_ids
        .iter()
        .copied()
        .filter(|id| orig_set.contains(id))
        .collect();
    if shared_orig != shared_curr {
        return Err(unsupported("reorder_column is not supported"));
    }
    Ok(())
}

fn index_definition_changed(a: &StructureIndexDraft, b: &StructureIndexDraft) -> bool {
    a.name != b.name
        || a.columns != b.columns
        || a.is_unique != b.is_unique
        || a.is_primary != b.is_primary
        || !a.index_type.eq_ignore_ascii_case(&b.index_type)
        || a.include_columns != b.include_columns
        || a.filter != b.filter
        || a.comment != b.comment
}

fn classify_alter_risk(clauses: &[String]) -> StatementRisk {
    if clauses.iter().any(|c| c.starts_with("DROP ")) {
        StatementRisk::Destructive
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

fn column_order_changed(original: &[&str], current: &[&str], id: &str) -> bool {
    let orig_pos = original.iter().position(|&x| x == id);
    let cur_pos = current.iter().position(|&x| x == id);
    match (orig_pos, cur_pos) {
        (Some(a), Some(b)) => a != b,
        _ => false,
    }
}

fn is_first_column(order: &[&str], id: &str) -> bool {
    order.first() == Some(&id)
}

fn after_column_name(
    order: &[&str],
    id: &str,
    cols: &HashMap<&str, &StructureColumnDraft>,
) -> Option<String> {
    let pos = order.iter().position(|&x| x == id)?;
    if pos == 0 {
        return None;
    }
    cols.get(order.get(pos - 1)?).map(|c| c.name.clone())
}

fn column_position_suffix(
    order: &[&str],
    id: &str,
    cols: &HashMap<&str, &StructureColumnDraft>,
) -> String {
    if is_first_column(order, id) {
        " FIRST".to_string()
    } else if let Some(after) = after_column_name(order, id, cols) {
        format!(" AFTER {}", quote_ident(&after))
    } else {
        String::new()
    }
}

fn format_index_definition(idx: &StructureIndexDraft, caps: &StructureCapabilities) -> String {
    let cols = idx
        .columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let unique = if idx.is_unique { "UNIQUE " } else { "" };
    let method = normalize_index_method(&idx.index_type);
    let mut sql = if caps.index_type && !idx.index_type.trim().is_empty() {
        format!(
            "{}KEY {} ({}) USING {}",
            unique,
            quote_ident(&idx.name),
            cols,
            method
        )
    } else {
        format!("{}KEY {} ({})", unique, quote_ident(&idx.name), cols)
    };
    if let Some(comment) = &idx.comment {
        if !comment.is_empty() {
            sql.push_str(&format!(" COMMENT {}", quote_string_literal(comment)));
        }
    }
    sql
}

fn format_column_definition(
    col: &StructureColumnDraft,
    include_name: bool,
    inline_primary: bool,
    caps: &StructureCapabilities,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if include_name {
        parts.push(quote_ident(&col.name));
    }
    parts.push(col.data_type.clone());

    if !col.nullable {
        parts.push("NOT NULL".into());
    } else {
        parts.push("NULL".into());
    }

    match &col.default_value {
        None => {}
        Some(v) if v.is_empty() => {}
        Some(v) if v.eq_ignore_ascii_case("NULL") => parts.push("DEFAULT NULL".into()),
        Some(v) => parts.push(format!("DEFAULT {v}")),
    }

    if col.is_auto_increment {
        parts.push("AUTO_INCREMENT".into());
    }

    if inline_primary && col.is_primary_key {
        parts.push("PRIMARY KEY".into());
    }

    if col.is_unique && !col.is_primary_key {
        parts.push("UNIQUE".into());
    }

    if caps.comment {
        if let Some(comment) = &col.comment {
            if !comment.is_empty() {
                parts.push(format!("COMMENT {}", quote_string_literal(comment)));
            }
        }
    }

    parts.join(" ")
}

fn normalize_index_method(index_type: &str) -> String {
    let trimmed = index_type.trim();
    if trimmed.is_empty() {
        "BTREE".to_string()
    } else {
        trimmed.to_ascii_uppercase()
    }
}

fn quote_ident(name: &str) -> String {
    format!("`{}`", name.replace('`', "``"))
}

fn quote_table(schema: Option<&str>, table: &str) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
}

fn quote_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

fn unsupported(message: impl Into<String>) -> DriverError {
    DriverError::Unsupported(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_col(id: &str, name: &str, data_type: &str) -> StructureColumnDraft {
        StructureColumnDraft {
            id: id.into(),
            name: name.into(),
            data_type: data_type.into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
            is_unique: false,
        }
    }

    fn sample_idx(id: &str, name: &str, columns: Vec<&str>) -> StructureIndexDraft {
        StructureIndexDraft {
            id: id.into(),
            name: name.into(),
            columns: columns.into_iter().map(str::to_string).collect(),
            is_unique: false,
            is_primary: false,
            index_type: String::new(),
            include_columns: Vec::new(),
            filter: None,
            comment: None,
        }
    }

    #[test]
    fn caps_mysql_baseline() {
        let caps = baseline_capabilities("mysql");
        assert_eq!(caps.dialect_id, "mysql");
        assert_eq!(caps.alter_strategy, AlterStrategy::Direct);
        assert!(caps.create_table);
        assert!(caps.add_column);
        assert!(caps.drop_column);
        assert!(caps.rename_column);
        assert!(caps.alter_type);
        assert!(caps.alter_nullability);
        assert!(caps.alter_default);
        assert!(!caps.alter_primary_key);
        assert!(caps.reorder_column);
        assert!(caps.comment);
        assert!(caps.create_index);
        assert!(caps.drop_index);
        assert!(!caps.rebuild_index);
        assert!(caps.index_type);
        assert!(!caps.index_include);
        assert!(!caps.index_filter);
        assert!(caps.index_comment);
        assert_eq!(caps.index_methods, vec!["BTREE", "HASH"]);
    }

    #[test]
    fn caps_mariadb_uses_dialect_id() {
        let caps = baseline_capabilities("mariadb");
        assert_eq!(caps.dialect_id, "mariadb");
    }

    #[test]
    fn plan_add_column_snapshot() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: Some("app".into()),
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![
                sample_col("1", "id", "INT"),
                StructureColumnDraft {
                    id: "2".into(),
                    name: "email".into(),
                    data_type: "VARCHAR(255)".into(),
                    nullable: false,
                    default_value: Some("''".into()),
                    comment: Some("contact email".into()),
                    ..sample_col("2", "email", "VARCHAR(255)")
                },
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE `app`.`users` ADD COLUMN `email` VARCHAR(255) NOT NULL DEFAULT '' COMMENT 'contact email' AFTER `id`"
        );
        assert_eq!(plan.statements[0].risk, StatementRisk::Additive);
    }

    #[test]
    fn plan_create_table_column_unique() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: Some("app".into()),
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![
                sample_col("1", "id", "INT"),
                StructureColumnDraft {
                    is_unique: true,
                    ..sample_col("2", "email", "VARCHAR(255)")
                },
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0]
            .sql
            .contains("`email` VARCHAR(255) NOT NULL UNIQUE"));
    }

    #[test]
    fn plan_drop_index_snapshot() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![sample_col("1", "id", "INT")],
            original_indexes: vec![sample_idx("i1", "idx_email", vec!["email"])],
            current_indexes: vec![],
        };

        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE `users` DROP INDEX `idx_email`"
        );
        assert_eq!(plan.statements[0].risk, StatementRisk::Destructive);
    }

    #[test]
    fn plan_modify_column_snapshot() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: Some("shop".into()),
            table: "items".into(),
            original_columns: vec![StructureColumnDraft {
                nullable: true,
                default_value: Some("NULL".into()),
                ..sample_col("1", "qty", "INT")
            }],
            current_columns: vec![StructureColumnDraft {
                data_type: "BIGINT".into(),
                nullable: false,
                default_value: Some("0".into()),
                comment: Some("stock count".into()),
                ..sample_col("1", "qty", "INT")
            }],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE `shop`.`items` MODIFY COLUMN `qty` BIGINT NOT NULL DEFAULT 0 COMMENT 'stock count'"
        );
    }

    #[test]
    fn plan_rename_column_uses_change() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "name", "VARCHAR(100)")],
            current_columns: vec![StructureColumnDraft {
                name: "full_name".into(),
                ..sample_col("1", "full_name", "VARCHAR(100)")
            }],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert_eq!(
            plan.statements[0].sql,
            "ALTER TABLE `users` CHANGE COLUMN `name` `full_name` VARCHAR(100) NOT NULL"
        );
    }

    #[test]
    fn plan_create_index_snapshot() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: Some("app".into()),
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![sample_col("1", "id", "INT")],
            original_indexes: vec![],
            current_indexes: vec![StructureIndexDraft {
                is_unique: true,
                index_type: "hash".into(),
                ..sample_idx("i1", "idx_email", vec!["email"])
            }],
        };

        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            "CREATE UNIQUE INDEX `idx_email` ON `app`.`users` (`email`) USING HASH"
        );
    }

    #[test]
    fn plan_rejects_index_include() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![sample_col("1", "id", "INT")],
            original_indexes: vec![],
            current_indexes: vec![StructureIndexDraft {
                include_columns: vec!["extra".into()],
                ..sample_idx("i1", "idx_email", vec!["email"])
            }],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "index_include is not supported")
        );
    }

    #[test]
    fn plan_rejects_add_column_when_cap_disabled() {
        let mut caps = baseline_capabilities("mysql");
        caps.add_column = false;
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![
                sample_col("1", "id", "INT"),
                sample_col("2", "email", "VARCHAR(255)"),
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "add_column is not supported")
        );
    }

    #[test]
    fn plan_reorder_column_emits_after() {
        let caps = baseline_capabilities("mysql");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![
                sample_col("1", "id", "INT"),
                sample_col("2", "name", "VARCHAR(100)"),
                sample_col("3", "email", "VARCHAR(255)"),
            ],
            current_columns: vec![
                sample_col("1", "id", "INT"),
                StructureColumnDraft {
                    data_type: "VARCHAR(255)".into(),
                    ..sample_col("3", "email", "VARCHAR(255)")
                },
                StructureColumnDraft {
                    data_type: "VARCHAR(100)".into(),
                    ..sample_col("2", "name", "VARCHAR(100)")
                },
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let plan = plan_structure_changes(&caps, &request).unwrap();
        assert!(plan.statements[0]
            .sql
            .contains("MODIFY COLUMN `email` VARCHAR(255) NOT NULL AFTER `id`"));
        assert!(plan.statements[0]
            .sql
            .contains("MODIFY COLUMN `name` VARCHAR(100) NOT NULL AFTER `email`"));
    }

    #[test]
    fn plan_rejects_column_reorder_when_cap_disabled() {
        let mut caps = baseline_capabilities("mysql");
        caps.reorder_column = false;
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![
                sample_col("1", "id", "INT"),
                sample_col("2", "name", "VARCHAR(100)"),
            ],
            current_columns: vec![
                sample_col("2", "name", "VARCHAR(100)"),
                sample_col("1", "id", "INT"),
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "reorder_column is not supported")
        );
    }

    #[test]
    fn plan_rejects_column_comment_on_add_when_cap_disabled() {
        let mut caps = baseline_capabilities("mysql");
        caps.comment = false;
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![
                sample_col("1", "id", "INT"),
                StructureColumnDraft {
                    comment: Some("note".into()),
                    ..sample_col("2", "email", "VARCHAR(255)")
                },
            ],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(msg) if msg == "comment is not supported"));
    }

    #[test]
    fn plan_rejects_column_comment_on_create_when_cap_disabled() {
        let mut caps = baseline_capabilities("mysql");
        caps.comment = false;
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: None,
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![StructureColumnDraft {
                comment: Some("identifier".into()),
                ..sample_col("1", "id", "INT")
            }],
            original_indexes: vec![],
            current_indexes: vec![],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(msg) if msg == "comment is not supported"));
    }

    #[test]
    fn plan_rejects_index_type_when_cap_disabled() {
        let mut caps = baseline_capabilities("mysql");
        caps.index_type = false;
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![sample_col("1", "id", "INT")],
            original_indexes: vec![],
            current_indexes: vec![StructureIndexDraft {
                index_type: "hash".into(),
                ..sample_idx("i1", "idx_email", vec!["email"])
            }],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "index_type is not supported")
        );
    }

    #[test]
    fn plan_rejects_index_comment_when_cap_disabled() {
        let mut caps = baseline_capabilities("mysql");
        caps.index_comment = false;
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![sample_col("1", "id", "INT")],
            original_indexes: vec![],
            current_indexes: vec![StructureIndexDraft {
                comment: Some("lookup".into()),
                ..sample_idx("i1", "idx_email", vec!["email"])
            }],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "index_comment is not supported")
        );
    }

    #[test]
    fn plan_rejects_index_method_when_not_allowed() {
        let mut caps = baseline_capabilities("mysql");
        caps.index_methods = vec!["BTREE".into()];
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: None,
            table: "users".into(),
            original_columns: vec![sample_col("1", "id", "INT")],
            current_columns: vec![sample_col("1", "id", "INT")],
            original_indexes: vec![],
            current_indexes: vec![StructureIndexDraft {
                index_type: "hash".into(),
                ..sample_idx("i1", "idx_email", vec!["email"])
            }],
        };

        let err = plan_structure_changes(&caps, &request).unwrap_err();
        assert!(
            matches!(err, DriverError::Unsupported(msg) if msg == "index method `HASH` is not supported")
        );
    }
}
