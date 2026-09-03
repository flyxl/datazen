use datazen_driver_api::*;

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

pub struct SqliteMigrationRenderer;

impl MigrationRenderer for SqliteMigrationRenderer {
    fn render(&self, op: &MigrationOperation) -> Result<MigrationStatement, String> {
        let qi = |s: &str| format!("\"{}\"", s.replace('\"', "\"\""));
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
            _ => Err(format!(
                "SQLite renderer does not yet support {:?}; table rebuild may be required",
                op
            )),
        }
    }
}

pub struct SqliteMigrationCapabilities;
impl MigrationCapabilities for SqliteMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        matches!(
            operation,
            MigrationOperation::CreateTable { .. }
                | MigrationOperation::AddColumn { .. }
                | MigrationOperation::CreateIndex { .. }
                | MigrationOperation::DropIndex { .. }
        )
    }
    fn requires_table_rebuild(&self, _operation: &MigrationOperation) -> bool {
        false
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
        assert!(!SqliteMigrationCapabilities.supports(&op));
        assert!(SqliteMigrationRenderer.render(&op).is_err());
    }

    #[test]
    fn capabilities_only_support_renderer_ops() {
        assert!(SqliteMigrationCapabilities.supports(&MigrationOperation::CreateTable {
            table: "users".into(),
            columns: vec![],
            primary_keys: vec![],
        }));
        assert!(!SqliteMigrationCapabilities.supports(&MigrationOperation::AlterColumnType {
            table: "users".into(),
            column: "id".into(),
            from: "INTEGER".into(),
            to: "BIGINT".into(),
        }));
        assert!(!SqliteMigrationCapabilities.supports(&MigrationOperation::DropColumn {
            table: "users".into(),
            column: col("name", "TEXT"),
        }));
    }
}
