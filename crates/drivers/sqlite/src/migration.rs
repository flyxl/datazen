use datazen_driver_api::*;

mod rebuild;

fn format_sqlite_column_def(c: &MigrationColumn, qi: &impl Fn(&str) -> String) -> String {
    let mut def = format!(
        "{} {}{}",
        qi(&c.name),
        c.data_type,
        if c.nullable { "" } else { " NOT NULL" }
    );
    if let Some(default) = &c.default_value {
        def.push_str(&format!(" DEFAULT {default}"));
    }
    def
}

fn sqlite_view_ident(view: &MigrationView) -> String {
    let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
    match view.schema.as_deref().filter(|schema| !schema.is_empty()) {
        Some(schema) => format!("{}.{}", quote(schema), quote(&view.name)),
        None => quote(&view.name),
    }
}

pub struct SqliteMigrationRenderer;

impl MigrationRenderer for SqliteMigrationRenderer {
    fn render(&self, op: &MigrationOperation) -> Result<MigrationStatement, String> {
        let qi = |s: &str| {
            if s.contains('.') {
                s.split('.')
                    .map(|part| format!("\"{}\"", part.replace('\"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(".")
            } else {
                format!("\"{}\"", s.replace('\"', "\"\""))
            }
        };
        match op {
            MigrationOperation::CreateTable {
                table,
                columns,
                primary_keys,
            } => {
                let cols = columns
                    .iter()
                    .map(|c| format_sqlite_column_def(c, &qi))
                    .collect::<Vec<_>>();
                let pk = if primary_keys.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", PRIMARY KEY ({})",
                        primary_keys
                            .iter()
                            .map(|c| qi(c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };
                Ok(MigrationStatement {
                    sql: format!("CREATE TABLE {} ({}{})", qi(table), cols.join(", "), pk),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP TABLE {}", qi(table))),
                    summary: format!("CREATE TABLE {}", table),
                })
            }
            MigrationOperation::DropTable { table } => {
                let table = validate_migration_identifier(table)?;
                Ok(MigrationStatement {
                    sql: format!("DROP TABLE {}", qi(table)),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: None,
                    summary: format!("DROP TABLE {}", table),
                })
            }

            MigrationOperation::AddColumn { table, column } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} ADD COLUMN {}",
                    qi(table),
                    format_sqlite_column_def(column, &qi)
                ),
                risk: MigrationRisk::Additive,
                rollback_sql: Some(format!(
                    "ALTER TABLE {} DROP COLUMN {}",
                    qi(table),
                    qi(&column.name)
                )),
                summary: format!("ADD COLUMN {}.{}", table, column.name),
            }),
            MigrationOperation::CreateIndex { table, index } => Ok(MigrationStatement {
                sql: format!(
                    "CREATE {}INDEX {} ON {} ({})",
                    if index.is_unique { "UNIQUE " } else { "" },
                    qi(&index.name),
                    qi(table),
                    index
                        .columns
                        .iter()
                        .map(|c| qi(c))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                risk: MigrationRisk::Additive,
                rollback_sql: Some(format!("DROP INDEX {}", qi(&index.name))),
                summary: format!("CREATE INDEX {}.{}", table, index.name),
            }),
            MigrationOperation::DropIndex { index, .. } => Ok(MigrationStatement {
                sql: format!("DROP INDEX {}", qi(&index.name)),
                risk: MigrationRisk::Destructive,
                rollback_sql: None,
                summary: format!("DROP INDEX {}", index.name),
            }),
            MigrationOperation::DropColumn { .. } => {
                Err("SQLite DROP COLUMN requires version/capability validation".into())
            }
            MigrationOperation::CreateView { view } => {
                validate_view_definition(&view.definition)?;
                let ident = sqlite_view_ident(view);
                Ok(MigrationStatement {
                    sql: format!("CREATE VIEW {ident} AS {}", view.definition.trim()),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP VIEW {ident}")),
                    summary: format!("CREATE VIEW {}", view.name),
                })
            }
            MigrationOperation::DropView { view } => {
                validate_view_definition(&view.definition)?;
                let ident = sqlite_view_ident(view);
                Ok(MigrationStatement {
                    sql: format!("DROP VIEW {ident}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(format!(
                        "CREATE VIEW {ident} AS {}",
                        view.definition.trim()
                    )),
                    summary: format!("DROP VIEW {}", view.name),
                })
            }
            MigrationOperation::ReplaceView { .. } => {
                Err("SQLite view replacement requires an explicit drop/create rebuild".into())
            }
            MigrationOperation::AddCheckConstraint { .. }
            | MigrationOperation::DropCheckConstraint { .. } => Err(
                "SQLite CHECK constraint changes require a table rebuild; refusing an unsafe direct ALTER".into(),
            ),
            MigrationOperation::CreateType { .. }
            | MigrationOperation::ReplaceType { .. }
            | MigrationOperation::DropType { .. } => Err(
                "SQLite user-defined type migration is unsupported".into(),
            ),
            _ => Err(format!(
                "SQLite renderer does not yet support {:?}; table rebuild may be required",
                op
            )),
        }
    }

    fn render_table_rebuild(
        &self,
        table: &str,
        desired: &TableSchema,
        current: &TableSchema,
    ) -> Result<Vec<MigrationStatement>, String> {
        rebuild::render_table_rebuild(table, desired, current)
    }
}

pub struct SqliteMigrationCapabilities;
impl MigrationCapabilities for SqliteMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        matches!(
            operation,
            MigrationOperation::CreateTable { .. }
                | MigrationOperation::DropTable { .. }
                | MigrationOperation::AddColumn { .. }
                | MigrationOperation::DropColumn { .. }
                | MigrationOperation::AlterColumnType { .. }
                | MigrationOperation::SetNullable { .. }
                | MigrationOperation::SetDefault { .. }
                | MigrationOperation::AddPrimaryKey { .. }
                | MigrationOperation::DropPrimaryKey { .. }
                | MigrationOperation::CreateIndex { .. }
                | MigrationOperation::DropIndex { .. }
                | MigrationOperation::AddForeignKey { .. }
                | MigrationOperation::DropForeignKey { .. }
                | MigrationOperation::AddCheckConstraint { .. }
                | MigrationOperation::DropCheckConstraint { .. }
                | MigrationOperation::CreateView { .. }
                | MigrationOperation::DropView { .. }
        )
    }
    fn requires_table_rebuild(&self, operation: &MigrationOperation) -> bool {
        matches!(
            operation,
            MigrationOperation::DropColumn { .. }
                | MigrationOperation::AlterColumnType { .. }
                | MigrationOperation::SetNullable { .. }
                | MigrationOperation::SetDefault { .. }
                | MigrationOperation::AddPrimaryKey { .. }
                | MigrationOperation::DropPrimaryKey { .. }
                | MigrationOperation::AddForeignKey { .. }
                | MigrationOperation::DropForeignKey { .. }
                | MigrationOperation::AddCheckConstraint { .. }
                | MigrationOperation::DropCheckConstraint { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn col(name: &str, ty: &str) -> MigrationColumn {
        MigrationColumn {
            name: name.into(),
            data_type: ty.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_auto_increment: false,
        }
    }

    fn rebuild_schema() -> TableSchema {
        TableSchema {
            table_name: "users".into(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "INTEGER".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_primary_key: true,
                    is_auto_increment: true,
                },
                ColumnSchema {
                    name: "email".into(),
                    data_type: "TEXT".into(),
                    nullable: false,
                    default_value: Some("'unknown'".into()),
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
                ColumnSchema {
                    name: "parent_id".into(),
                    data_type: "INTEGER".into(),
                    nullable: true,
                    default_value: None,
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
                ColumnSchema {
                    name: "score".into(),
                    data_type: "REAL".into(),
                    nullable: true,
                    default_value: Some("0".into()),
                    comment: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: vec![
                IndexInfo {
                    name: "sqlite_autoindex_users_2".into(),
                    columns: vec!["email".into()],
                    is_unique: true,
                    is_primary: false,
                    index_type: "btree".into(),
                },
                IndexInfo {
                    name: "idx_users_score".into(),
                    columns: vec!["score".into()],
                    is_unique: false,
                    is_primary: false,
                    index_type: "btree".into(),
                },
            ],
            foreign_keys: vec![ForeignKeyInfo {
                name: "fk_users_parent".into(),
                columns: vec!["parent_id".into()],
                referenced_table: "parents".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "CASCADE".into(),
                deferrability: ForeignKeyDeferrability::NotDeferrable,
            }],
            check_constraints: vec![CheckConstraint {
                name: "ck_users_score".into(),
                expression: "score >= 0".into(),
            }],
            table_options: TableOptions::default(),
        }
    }
    #[test]
    fn renders_add_column() {
        let op = MigrationOperation::AddColumn {
            table: "users".into(),
            column: col("name", "TEXT"),
        };
        assert_eq!(
            SqliteMigrationRenderer.render(&op).unwrap().sql,
            "ALTER TABLE \"users\" ADD COLUMN \"name\" TEXT"
        );
    }

    #[test]
    fn add_column_renders_default_and_rollback() {
        let op = MigrationOperation::AddColumn {
            table: "users".into(),
            column: MigrationColumn {
                name: "score".into(),
                data_type: "INTEGER".into(),
                nullable: false,
                default_value: Some("0".into()),
                comment: None,
                is_auto_increment: false,
            },
        };
        let stmt = SqliteMigrationRenderer.render(&op).unwrap();
        assert!(stmt.sql.contains("DEFAULT 0"));
        assert_eq!(
            stmt.rollback_sql.as_deref(),
            Some("ALTER TABLE \"users\" DROP COLUMN \"score\"")
        );
    }

    #[test]
    fn create_table_renders_default() {
        let op = MigrationOperation::CreateTable {
            table: "users".into(),
            columns: vec![MigrationColumn {
                name: "status".into(),
                data_type: "TEXT".into(),
                nullable: true,
                default_value: Some("'active'".into()),
                comment: None,
                is_auto_increment: false,
            }],
            primary_keys: vec![],
        };
        let stmt = SqliteMigrationRenderer.render(&op).unwrap();
        assert!(stmt.sql.contains("DEFAULT 'active'"));
    }

    #[test]
    fn rejects_drop_column_without_capability_validation() {
        let op = MigrationOperation::DropColumn {
            table: "users".into(),
            column: col("name", "TEXT"),
        };
        assert!(SqliteMigrationCapabilities.supports(&op));
        assert!(SqliteMigrationCapabilities.requires_table_rebuild(&op));
        assert!(SqliteMigrationRenderer.render(&op).is_err());
    }

    #[test]
    fn drop_table_is_quoted_destructive_and_has_no_rollback() {
        let stmt = SqliteMigrationRenderer
            .render(&MigrationOperation::DropTable {
                table: "events".into(),
            })
            .unwrap();
        assert_eq!(stmt.sql, "DROP TABLE \"events\"");
        assert_eq!(stmt.risk, MigrationRisk::Destructive);
        assert!(stmt.rollback_sql.is_none());
        assert!(
            SqliteMigrationCapabilities.supports(&MigrationOperation::DropTable {
                table: "events".into(),
            })
        );
    }

    #[test]
    fn test_tester_drop_table_rejects_empty_identifier() {
        assert!(SqliteMigrationRenderer
            .render(&MigrationOperation::DropTable {
                table: String::new()
            })
            .is_err());
    }

    #[test]
    fn drop_table_rejects_blank_control_and_invalid_qualified_identifiers() {
        for table in [" ", "audit\nevents", "audit..events", "audit. events"] {
            assert!(
                SqliteMigrationRenderer
                    .render(&MigrationOperation::DropTable {
                        table: table.into()
                    })
                    .is_err(),
                "{table:?}"
            );
        }
    }

    #[test]
    fn capabilities_only_support_renderer_ops() {
        assert!(
            SqliteMigrationCapabilities.supports(&MigrationOperation::CreateTable {
                table: "users".into(),
                columns: vec![],
                primary_keys: vec![],
            })
        );
        assert!(
            SqliteMigrationCapabilities.supports(&MigrationOperation::AlterColumnType {
                table: "users".into(),
                column: "id".into(),
                from: "INTEGER".into(),
                to: "BIGINT".into(),
            })
        );
        assert!(SqliteMigrationCapabilities.requires_table_rebuild(
            &MigrationOperation::AlterColumnType {
                table: "users".into(),
                column: "id".into(),
                from: "INTEGER".into(),
                to: "BIGINT".into(),
            }
        ));
        assert!(
            SqliteMigrationCapabilities.supports(&MigrationOperation::DropColumn {
                table: "users".into(),
                column: col("name", "TEXT"),
            })
        );
    }

    #[test]
    fn check_constraint_changes_are_routed_to_table_rebuild() {
        let op = MigrationOperation::AddCheckConstraint {
            table: "users".into(),
            constraint: CheckConstraint {
                name: "check_age".into(),
                expression: "age >= 0".into(),
            },
        };
        assert!(SqliteMigrationCapabilities.supports(&op));
        assert!(SqliteMigrationCapabilities.requires_table_rebuild(&op));
        assert!(SqliteMigrationRenderer.render(&op).is_err());
    }

    #[test]
    fn table_rebuild_renders_reviewed_constraints_rows_indexes_and_transaction_guard() {
        let schema = rebuild_schema();
        let statements = SqliteMigrationRenderer
            .render_table_rebuild("users", &schema, &schema)
            .unwrap();
        assert_eq!(statements[0].sql, "PRAGMA defer_foreign_keys = ON");
        let capture = statements
            .iter()
            .position(|statement| statement.summary.starts_with("CAPTURE AUTOINCREMENT"))
            .unwrap();
        let create = statements
            .iter()
            .position(|statement| statement.summary.starts_with("CREATE replacement"))
            .unwrap();
        let copy = statements
            .iter()
            .position(|statement| statement.summary.starts_with("COPY rows"))
            .unwrap();
        let drop_old = statements
            .iter()
            .position(|statement| statement.summary.starts_with("DROP old table"))
            .unwrap();
        let rename = statements
            .iter()
            .position(|statement| statement.summary.starts_with("RENAME replacement"))
            .unwrap();
        let restore = statements
            .iter()
            .position(|statement| statement.summary.starts_with("RESTORE AUTOINCREMENT"))
            .unwrap();
        assert_eq!(capture + 1, create);
        assert!(capture < copy && copy < drop_old && drop_old < rename && rename < restore);
        assert!(statements[create].sql.contains("PRIMARY KEY AUTOINCREMENT"));
        assert!(statements[create].sql.contains("UNIQUE (\"email\")"));
        assert!(statements[create]
            .sql
            .contains("FOREIGN KEY (\"parent_id\")"));
        assert!(statements[create].sql.contains("CHECK (score >= 0)"));
        assert!(statements[copy]
            .sql
            .contains("INSERT INTO \"__datazen_rebuild_users\""));
        assert_eq!(statements[drop_old].sql, "DROP TABLE \"users\"");
        assert_eq!(
            statements[rename].sql,
            "ALTER TABLE \"__datazen_rebuild_users\" RENAME TO \"users\""
        );
        assert!(statements.iter().any(|statement| {
            statement
                .sql
                .contains("UPDATE sqlite_sequence SET seq = MAX(seq")
        }));
        assert!(statements.iter().any(|statement| {
            statement
                .sql
                .contains("INSERT INTO sqlite_sequence (name, seq)")
        }));
    }

    #[test]
    fn table_rebuild_preserves_implicit_rowid_or_refuses_to_change_its_alias() {
        let schema = TableSchema {
            table_name: "rowid_items".into(),
            columns: vec![ColumnSchema {
                name: "value".into(),
                data_type: "TEXT".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            }],
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: TableOptions::default(),
        };
        let statements = SqliteMigrationRenderer
            .render_table_rebuild("rowid_items", &schema, &schema)
            .unwrap();
        assert!(statements.iter().any(|statement| {
            statement.summary.starts_with("COPY rows")
                && statement
                    .sql
                    .contains("(\"value\", \"rowid\") SELECT \"value\", \"rowid\"")
        }));

        let mut with_rowid_alias = schema.clone();
        with_rowid_alias.columns.push(ColumnSchema {
            name: "id".into(),
            data_type: "INTEGER".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        });
        with_rowid_alias.primary_keys.push("id".into());
        let error = SqliteMigrationRenderer
            .render_table_rebuild("rowid_items", &with_rowid_alias, &schema)
            .unwrap_err();
        assert!(error.contains("cannot add an INTEGER PRIMARY KEY rowid alias"));

        let mut shadowed = schema;
        for name in ["rowid", "_rowid_", "oid"] {
            shadowed.columns.push(ColumnSchema {
                name: name.into(),
                data_type: "INTEGER".into(),
                nullable: true,
                default_value: None,
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            });
        }
        let error = SqliteMigrationRenderer
            .render_table_rebuild("rowid_items", &shadowed, &shadowed)
            .unwrap_err();
        assert!(error.contains("all aliases are shadowed"));
    }

    #[test]
    fn table_rebuild_refuses_catalog_semantics_and_expression_injection() {
        let schema = rebuild_schema();
        let mut current = schema.clone();
        current.table_options.migration_blockers.push(
            "View `score_view` may depend on `users` and is outside the table schema snapshot"
                .into(),
        );
        assert!(SqliteMigrationRenderer
            .render_table_rebuild("users", &schema, &current)
            .unwrap_err()
            .contains("score_view"));

        let mut unsafe_schema = schema;
        unsafe_schema.columns[1].default_value = Some("'x'; DROP TABLE secrets".into());
        assert!(SqliteMigrationRenderer
            .render_table_rebuild("users", &unsafe_schema, &rebuild_schema())
            .unwrap_err()
            .contains("one expression"));

        let mut unsafe_type = rebuild_schema();
        unsafe_type.columns[1].data_type = "TEXT, injected TEXT".into();
        assert!(SqliteMigrationRenderer
            .render_table_rebuild("users", &unsafe_type, &rebuild_schema())
            .unwrap_err()
            .contains("type name contains syntax"));

        let mut unsafe_check = rebuild_schema();
        unsafe_check.check_constraints[0].expression = "1) , injected TEXT CHECK (1".into();
        assert!(SqliteMigrationRenderer
            .render_table_rebuild("users", &unsafe_check, &rebuild_schema())
            .unwrap_err()
            .contains("CHECK contains syntax"));
    }

    #[test]
    fn views_support_create_and_drop_but_replacement_fails_closed() {
        let view = MigrationView {
            schema: None,
            name: "active_users".into(),
            definition: "SELECT id FROM users".into(),
        };
        let create = SqliteMigrationRenderer
            .render(&MigrationOperation::CreateView { view: view.clone() })
            .unwrap();
        assert_eq!(
            create.sql,
            "CREATE VIEW \"active_users\" AS SELECT id FROM users"
        );
        assert_eq!(
            create.rollback_sql.as_deref(),
            Some("DROP VIEW \"active_users\"")
        );
        assert!(SqliteMigrationCapabilities
            .supports(&MigrationOperation::CreateView { view: view.clone() }));
        assert!(SqliteMigrationCapabilities
            .supports(&MigrationOperation::DropView { view: view.clone() }));
        assert!(
            !SqliteMigrationCapabilities.supports(&MigrationOperation::ReplaceView {
                current: view.clone(),
                desired: MigrationView {
                    definition: "SELECT id, email FROM users".into(),
                    ..view.clone()
                }
            })
        );
        assert!(SqliteMigrationRenderer
            .render(&MigrationOperation::ReplaceView {
                current: view.clone(),
                desired: view,
            })
            .is_err());
    }
}
