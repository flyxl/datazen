//! PostgreSQL table structure capabilities and DDL planning.

use datazen_driver_api::*;

const DIALECT_ID: &str = "postgresql";

/// Parse the major PostgreSQL version from common `version()` strings.
/// Returns `None` when the major cannot be determined (conservative baseline).
pub fn parse_pg_major_version(version: &str) -> Option<u32> {
    let s = version.trim();
    let digits_start = s.find(|c: char| c.is_ascii_digit())?;
    let tail = &s[digits_start..];
    let major: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
    if major.is_empty() {
        return None;
    }
    major.parse().ok()
}

/// Build structure capabilities for a PostgreSQL server version string.
pub fn caps_for_version(version: &str) -> StructureCapabilities {
    let major = parse_pg_major_version(version);
    let index_include = major.is_some_and(|m| m >= 11);

    let mut index_methods = vec!["btree".to_string(), "hash".to_string()];
    if major.is_none_or(|m| m >= 8) {
        index_methods.push("gist".to_string());
        index_methods.push("gin".to_string());
    }
    if major.is_none_or(|m| m >= 9) {
        index_methods.push("spgist".to_string());
        if major.is_none_or(|m| m >= 9) {
            index_methods.push("brin".to_string());
        }
    }

    StructureCapabilities {
        create_table: true,
        add_column: true,
        drop_column: true,
        rename_column: true,
        alter_type: true,
        alter_nullability: true,
        alter_default: true,
        alter_primary_key: true,
        reorder_column: false,
        comment: true,
        create_index: true,
        drop_index: true,
        rebuild_index: false,
        index_type: true,
        index_include,
        index_filter: true,
        index_comment: true,
        alter_strategy: AlterStrategy::Direct,
        dialect_id: DIALECT_ID.to_string(),
        index_methods,
    }
}

fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn qualified_table(schema: Option<&str>, table: &str) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(table)),
        None => quote_ident(table),
    }
}

fn qualified_index(schema: Option<&str>, index_name: &str) -> String {
    match schema.filter(|s| !s.is_empty()) {
        Some(s) => format!("{}.{}", quote_ident(s), quote_ident(index_name)),
        None => quote_ident(index_name),
    }
}

fn column_def(col: &StructureColumnDraft) -> String {
    let mut parts = vec![quote_ident(&col.name), col.data_type.clone()];
    if !col.nullable {
        parts.push("NOT NULL".to_string());
    }
    if let Some(default) = &col.default_value {
        parts.push(format!("DEFAULT {default}"));
    }
    parts.join(" ")
}

fn primary_key_columns(columns: &[StructureColumnDraft]) -> Vec<String> {
    columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| quote_ident(&c.name))
        .collect()
}

fn columns_by_id(columns: &[StructureColumnDraft]) -> std::collections::HashMap<&str, &StructureColumnDraft> {
    columns.iter().map(|c| (c.id.as_str(), c)).collect()
}

fn indexes_by_id(indexes: &[StructureIndexDraft]) -> std::collections::HashMap<&str, &StructureIndexDraft> {
    indexes.iter().map(|i| (i.id.as_str(), i)).collect()
}

fn require_cap(enabled: bool, feature: &str) -> Result<(), DriverError> {
    if enabled {
        Ok(())
    } else {
        Err(DriverError::Unsupported(format!(
            "PostgreSQL driver does not support {feature} for this server version"
        )))
    }
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
        require_cap(false, "column reordering")?;
    }
    Ok(())
}

fn plan_create_table(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    require_cap(caps.create_table, "CREATE TABLE")?;
    let table = qualified_table(request.schema.as_deref(), &request.table);
    let mut col_defs: Vec<String> = request
        .current_columns
        .iter()
        .map(column_def)
        .collect();
    let pk_cols = primary_key_columns(&request.current_columns);
    if !pk_cols.is_empty() {
        col_defs.push(format!("PRIMARY KEY ({})", pk_cols.join(", ")));
    }
    let sql = format!("CREATE TABLE {table} (\n  {}\n)", col_defs.join(",\n  "));
    Ok(StructureChangePlan {
        statements: vec![PlanStatement {
            sql,
            summary: format!("Create table {}", table),
            risk: StatementRisk::Additive,
        }],
        warnings: vec![],
    })
}

fn build_create_index_sql(
    schema: Option<&str>,
    table: &str,
    idx: &StructureIndexDraft,
    caps: &StructureCapabilities,
) -> Result<String, DriverError> {
    if idx.is_primary {
        return Err(DriverError::Unsupported(
            "primary key indexes must be managed via column primary key flags".into(),
        ));
    }
    require_cap(caps.create_index, "CREATE INDEX")?;
    if !idx.index_type.is_empty() && !caps.index_type {
        require_cap(false, "index type selection")?;
    }
    if !idx.include_columns.is_empty() {
        require_cap(caps.index_include, "index INCLUDE columns")?;
    }
    if idx.filter.is_some() {
        require_cap(caps.index_filter, "partial index filters")?;
    }

    let table_ref = qualified_table(schema, table);
    let unique = if idx.is_unique { "UNIQUE " } else { "" };
    let method = if idx.index_type.is_empty() {
        "btree".to_string()
    } else {
        idx.index_type.clone()
    };
    let cols = idx
        .columns
        .iter()
        .map(|c| quote_ident(c))
        .collect::<Vec<_>>()
        .join(", ");
    let mut sql = format!(
        "CREATE {unique}INDEX {} ON {table_ref} USING {method} ({cols})",
        quote_ident(&idx.name)
    );
    if !idx.include_columns.is_empty() {
        let include = idx
            .include_columns
            .iter()
            .map(|c| quote_ident(c))
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(&format!(" INCLUDE ({include})"));
    }
    if let Some(filter) = &idx.filter {
        sql.push_str(&format!(" WHERE {filter}"));
    }
    Ok(sql)
}

/// Plan DDL statements using precomputed capabilities (pure, testable).
pub fn plan_structure_changes_with_caps(
    caps: &StructureCapabilities,
    request: &StructureChangeRequest,
) -> Result<StructureChangePlan, DriverError> {
    if request.mode == StructureChangeMode::Create {
        return plan_create_table(caps, request);
    }

    check_reorder(caps, &request.original_columns, &request.current_columns)?;

    let schema = request.schema.as_deref();
    let table_ref = qualified_table(schema, &request.table);
    let orig_cols = columns_by_id(&request.original_columns);
    let curr_cols = columns_by_id(&request.current_columns);
    let orig_idx = indexes_by_id(&request.original_indexes);
    let curr_idx = indexes_by_id(&request.current_indexes);

    let mut statements: Vec<PlanStatement> = Vec::new();
    let mut warnings: Vec<String> = Vec::new();

    // Drop removed indexes first (before dropping columns they may reference).
    for (id, idx) in &orig_idx {
        if !curr_idx.contains_key(id) {
            require_cap(caps.drop_index, "DROP INDEX")?;
            let index_ref = qualified_index(schema, &idx.name);
            statements.push(PlanStatement {
                sql: format!("DROP INDEX {index_ref}"),
                summary: format!("Drop index {}", idx.name),
                risk: StatementRisk::Destructive,
            });
        }
    }

    // Drop removed columns.
    for (id, col) in &orig_cols {
        if !curr_cols.contains_key(id) {
            require_cap(caps.drop_column, "DROP COLUMN")?;
            statements.push(PlanStatement {
                sql: format!(
                    "ALTER TABLE {table_ref} DROP COLUMN {}",
                    quote_ident(&col.name)
                ),
                summary: format!("Drop column {}", col.name),
                risk: StatementRisk::Destructive,
            });
        }
    }

    // Add new columns.
    for (id, col) in &curr_cols {
        if !orig_cols.contains_key(id) {
            require_cap(caps.add_column, "ADD COLUMN")?;
            statements.push(PlanStatement {
                sql: format!(
                    "ALTER TABLE {table_ref} ADD COLUMN {}",
                    column_def(col)
                ),
                summary: format!("Add column {}", col.name),
                risk: StatementRisk::Additive,
            });
        }
    }

    // Alter existing columns (rename before type/null/default changes).
    for (id, curr) in &curr_cols {
        let Some(orig) = orig_cols.get(id) else {
            continue;
        };

        if orig.name != curr.name {
            require_cap(caps.rename_column, "RENAME COLUMN")?;
            statements.push(PlanStatement {
                sql: format!(
                    "ALTER TABLE {table_ref} RENAME COLUMN {} TO {}",
                    quote_ident(&orig.name),
                    quote_ident(&curr.name)
                ),
                summary: format!("Rename column {} to {}", orig.name, curr.name),
                risk: StatementRisk::Rewrite,
            });
        }

        let col_name = quote_ident(&curr.name);

        if orig.data_type != curr.data_type {
            require_cap(caps.alter_type, "ALTER COLUMN TYPE")?;
            statements.push(PlanStatement {
                sql: format!(
                    "ALTER TABLE {table_ref} ALTER COLUMN {col_name} TYPE {}",
                    curr.data_type
                ),
                summary: format!("Change column {} type to {}", curr.name, curr.data_type),
                risk: StatementRisk::Rewrite,
            });
        }

        if orig.nullable != curr.nullable {
            require_cap(caps.alter_nullability, "ALTER COLUMN nullability")?;
            let action = if curr.nullable {
                "DROP NOT NULL"
            } else {
                "SET NOT NULL"
            };
            statements.push(PlanStatement {
                sql: format!("ALTER TABLE {table_ref} ALTER COLUMN {col_name} {action}"),
                summary: format!("Set column {} nullable={}", curr.name, curr.nullable),
                risk: StatementRisk::Rewrite,
            });
        }

        if orig.default_value != curr.default_value {
            require_cap(caps.alter_default, "ALTER COLUMN DEFAULT")?;
            let sql = match &curr.default_value {
                Some(val) => format!(
                    "ALTER TABLE {table_ref} ALTER COLUMN {col_name} SET DEFAULT {val}"
                ),
                None => format!("ALTER TABLE {table_ref} ALTER COLUMN {col_name} DROP DEFAULT"),
            };
            statements.push(PlanStatement {
                sql,
                summary: format!("Change default on column {}", curr.name),
                risk: StatementRisk::Rewrite,
            });
        }

        if orig.is_primary_key != curr.is_primary_key {
            require_cap(caps.alter_primary_key, "ALTER PRIMARY KEY")?;
            warnings.push(format!(
                "Primary key change on column {} may require manual constraint naming",
                curr.name
            ));
            if curr.is_primary_key {
                statements.push(PlanStatement {
                    sql: format!(
                        "ALTER TABLE {table_ref} ADD PRIMARY KEY ({col_name})"
                    ),
                    summary: format!("Add primary key on column {}", curr.name),
                    risk: StatementRisk::Rewrite,
                });
            } else {
                statements.push(PlanStatement {
                    sql: format!("ALTER TABLE {table_ref} DROP CONSTRAINT IF EXISTS {}_pkey", request.table),
                    summary: format!("Drop primary key involving column {}", curr.name),
                    risk: StatementRisk::Destructive,
                });
            }
        }

        if orig.comment != curr.comment {
            require_cap(caps.comment, "COMMENT ON COLUMN")?;
            let comment = curr.comment.as_deref().unwrap_or("");
            statements.push(PlanStatement {
                sql: format!(
                    "COMMENT ON COLUMN {table_ref}.{col_name} IS '{}'",
                    comment.replace('\'', "''")
                ),
                summary: format!("Set comment on column {}", curr.name),
                risk: StatementRisk::Additive,
            });
        }
    }

    // Create new indexes.
    for (id, idx) in &curr_idx {
        if !orig_idx.contains_key(id) {
            let sql = build_create_index_sql(schema, &request.table, idx, caps)?;
            statements.push(PlanStatement {
                sql,
                summary: format!("Create index {}", idx.name),
                risk: StatementRisk::Additive,
            });
        }
    }

    // Modified indexes: drop + recreate when definition changed.
    for (id, curr) in &curr_idx {
        let Some(orig) = orig_idx.get(id) else {
            continue;
        };
        if **orig == **curr {
            continue;
        }
        require_cap(caps.drop_index, "DROP INDEX")?;
        require_cap(caps.create_index, "CREATE INDEX")?;
        let index_ref = qualified_index(schema, &orig.name);
        statements.push(PlanStatement {
            sql: format!("DROP INDEX {index_ref}"),
            summary: format!("Drop index {} for rebuild", orig.name),
            risk: StatementRisk::Destructive,
        });
        let sql = build_create_index_sql(schema, &request.table, curr, caps)?;
        statements.push(PlanStatement {
            sql,
            summary: format!("Recreate index {}", curr.name),
            risk: StatementRisk::Rewrite,
        });
    }

    Ok(StructureChangePlan {
        statements,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alter_request(
        original_columns: Vec<StructureColumnDraft>,
        current_columns: Vec<StructureColumnDraft>,
        original_indexes: Vec<StructureIndexDraft>,
        current_indexes: Vec<StructureIndexDraft>,
    ) -> StructureChangeRequest {
        StructureChangeRequest {
            mode: StructureChangeMode::Alter,
            schema: Some("public".into()),
            table: "users".into(),
            original_columns,
            current_columns,
            original_indexes,
            current_indexes,
        }
    }

    fn sample_column(id: &str, name: &str) -> StructureColumnDraft {
        StructureColumnDraft {
            id: id.into(),
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
            is_unique: false,
        }
    }

    #[test]
    fn caps_index_include_false_on_pg10() {
        let caps = caps_for_version("10.23");
        assert!(!caps.index_include);
    }

    #[test]
    fn caps_index_include_true_on_pg14() {
        let caps = caps_for_version("14.5");
        assert!(caps.index_include);
    }

    #[test]
    fn caps_index_include_false_on_unparseable_version() {
        let caps = caps_for_version("unknown server");
        assert!(!caps.index_include);
    }

    #[test]
    fn caps_parses_postgresql_version_string() {
        let caps = caps_for_version("PostgreSQL 14.5 on aarch64-unknown-linux-gnu, compiled by gcc");
        assert!(caps.index_include);
        assert_eq!(caps.alter_strategy, AlterStrategy::Direct);
        assert!(!caps.reorder_column);
    }

    #[test]
    fn plan_add_column() {
        let caps = caps_for_version("14.5");
        let request = alter_request(
            vec![],
            vec![sample_column("c1", "email")],
            vec![],
            vec![],
        );
        let plan = plan_structure_changes_with_caps(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            r#"ALTER TABLE "public"."users" ADD COLUMN "email" text"#
        );
        assert_eq!(plan.statements[0].risk, StatementRisk::Additive);
    }

    #[test]
    fn plan_drop_column() {
        let caps = caps_for_version("14.5");
        let request = alter_request(
            vec![sample_column("c1", "email")],
            vec![],
            vec![],
            vec![],
        );
        let plan = plan_structure_changes_with_caps(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert_eq!(
            plan.statements[0].sql,
            r#"ALTER TABLE "public"."users" DROP COLUMN "email""#
        );
        assert_eq!(plan.statements[0].risk, StatementRisk::Destructive);
    }

    #[test]
    fn plan_rename_column() {
        let caps = caps_for_version("14.5");
        let request = alter_request(
            vec![sample_column("c1", "email")],
            vec![StructureColumnDraft {
                name: "email_address".into(),
                ..sample_column("c1", "email_address")
            }],
            vec![],
            vec![],
        );
        let plan = plan_structure_changes_with_caps(&caps, &request).unwrap();
        assert!(plan
            .statements
            .iter()
            .any(|s| s.sql == r#"ALTER TABLE "public"."users" RENAME COLUMN "email" TO "email_address""#));
    }

    #[test]
    fn plan_create_index() {
        let caps = caps_for_version("14.5");
        let request = alter_request(
            vec![sample_column("c1", "email")],
            vec![sample_column("c1", "email")],
            vec![],
            vec![StructureIndexDraft {
                id: "i1".into(),
                name: "users_email_idx".into(),
                columns: vec!["email".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
                include_columns: vec![],
                filter: None,
                comment: None,
            }],
        );
        let plan = plan_structure_changes_with_caps(&caps, &request).unwrap();
        assert!(plan.statements.iter().any(|s| {
            s.sql == r#"CREATE INDEX "users_email_idx" ON "public"."users" USING btree ("email")"#
        }));
    }

    #[test]
    fn plan_drop_index() {
        let caps = caps_for_version("14.5");
        let request = alter_request(
            vec![sample_column("c1", "email")],
            vec![sample_column("c1", "email")],
            vec![StructureIndexDraft {
                id: "i1".into(),
                name: "users_email_idx".into(),
                columns: vec!["email".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
                include_columns: vec![],
                filter: None,
                comment: None,
            }],
            vec![],
        );
        let plan = plan_structure_changes_with_caps(&caps, &request).unwrap();
        assert!(plan.statements.iter().any(|s| {
            s.sql == r#"DROP INDEX "public"."users_email_idx""#
        }));
    }

    #[test]
    fn plan_create_table_mode() {
        let caps = caps_for_version("14.5");
        let request = StructureChangeRequest {
            mode: StructureChangeMode::Create,
            schema: Some("public".into()),
            table: "users".into(),
            original_columns: vec![],
            current_columns: vec![StructureColumnDraft {
                id: "c1".into(),
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
                is_unique: false,
            }],
            original_indexes: vec![],
            current_indexes: vec![],
        };
        let plan = plan_structure_changes_with_caps(&caps, &request).unwrap();
        assert_eq!(plan.statements.len(), 1);
        assert!(plan.statements[0].sql.starts_with(r#"CREATE TABLE "public"."users" ("#));
        assert!(plan.statements[0].sql.contains(r#""id" integer NOT NULL"#));
        assert!(plan.statements[0].sql.contains(r#"PRIMARY KEY ("id")"#));
    }

    #[test]
    fn plan_rejects_column_reorder_when_cap_disabled() {
        let caps = caps_for_version("14.5");
        let request = alter_request(
            vec![
                sample_column("c1", "a"),
                sample_column("c2", "b"),
            ],
            vec![
                sample_column("c2", "b"),
                sample_column("c1", "a"),
            ],
            vec![],
            vec![],
        );
        let err = plan_structure_changes_with_caps(&caps, &request).unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(msg) if msg.contains("column reordering")));
    }

    #[test]
    fn plan_rejects_index_include_on_pg10() {
        let caps = caps_for_version("10.0");
        let request = alter_request(
            vec![sample_column("c1", "email")],
            vec![sample_column("c1", "email")],
            vec![],
            vec![StructureIndexDraft {
                id: "i1".into(),
                name: "users_email_idx".into(),
                columns: vec!["email".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
                include_columns: vec!["id".into()],
                filter: None,
                comment: None,
            }],
        );
        let err = plan_structure_changes_with_caps(&caps, &request).unwrap_err();
        assert!(matches!(err, DriverError::Unsupported(msg) if msg.contains("INCLUDE")));
    }
}
