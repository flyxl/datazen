use datazen_driver_api::*;

fn format_mysql_column_def(c: &MigrationColumn, qi: &impl Fn(&str) -> String) -> String {
    let mut def = format!(
        "{} {}{}",
        qi(&c.name),
        c.data_type,
        if c.nullable { "" } else { " NOT NULL" }
    );
    if let Some(default) = &c.default_value {
        def.push_str(&format!(" DEFAULT {default}"));
    }
    if c.is_auto_increment {
        def.push_str(" AUTO_INCREMENT");
    }
    if let Some(comment) = &c.comment {
        if !comment.is_empty() {
            def.push_str(&format!(" COMMENT '{}'", comment.replace('\'', "''")));
        }
    }
    def
}

pub struct MysqlMigrationRenderer;

impl MigrationRenderer for MysqlMigrationRenderer {
    fn render(&self, op: &MigrationOperation) -> Result<MigrationStatement, String> {
        let qi = |s: &str| {
            if s.contains('.') {
                s.split('.')
                    .map(|part| format!("`{}`", part.replace('`', "``")))
                    .collect::<Vec<_>>()
                    .join(".")
            } else {
                format!("`{}`", s.replace('`', "``"))
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
                    .map(|c| format_mysql_column_def(c, &qi))
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
                    format_mysql_column_def(column, &qi)
                ),
                risk: MigrationRisk::Additive,
                rollback_sql: Some(format!(
                    "ALTER TABLE {} DROP COLUMN {}",
                    qi(table),
                    qi(&column.name)
                )),
                summary: format!("ADD COLUMN {}.{}", table, column.name),
            }),
            MigrationOperation::DropColumn { table, column } => Ok(MigrationStatement {
                sql: format!("ALTER TABLE {} DROP COLUMN {}", qi(table), qi(&column.name)),
                risk: MigrationRisk::Destructive,
                rollback_sql: None,
                summary: format!("DROP COLUMN {}.{}", table, column.name),
            }),
            MigrationOperation::SetDefault {
                table,
                column,
                from,
                to,
            } => {
                let sql = match to {
                    Some(v) => format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                        qi(table),
                        qi(column),
                        v
                    ),
                    None => format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT",
                        qi(table),
                        qi(column)
                    ),
                };
                let rollback_sql = match from {
                    Some(v) => Some(format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}",
                        qi(table),
                        qi(column),
                        v
                    )),
                    None => Some(format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT",
                        qi(table),
                        qi(column)
                    )),
                };
                Ok(MigrationStatement {
                    sql,
                    risk: MigrationRisk::Additive,
                    rollback_sql,
                    summary: format!("ALTER DEFAULT {}.{}", table, column),
                })
            }
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
                rollback_sql: Some(format!("DROP INDEX {} ON {}", qi(&index.name), qi(table))),
                summary: format!("CREATE INDEX {}.{}", table, index.name),
            }),
            MigrationOperation::DropIndex { table, index } => Ok(MigrationStatement {
                sql: format!("DROP INDEX {} ON {}", qi(&index.name), qi(table)),
                risk: MigrationRisk::Destructive,
                rollback_sql: None,
                summary: format!("DROP INDEX {}.{}", table, index.name),
            }),
            MigrationOperation::AlterColumnType {
                table, column, to, ..
            } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} MODIFY COLUMN {} {}",
                    qi(table),
                    qi(column),
                    to
                ),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("ALTER TYPE {}.{}", table, column),
            }),
            MigrationOperation::SetNullable { .. } => Err(
                "MySQL nullability change requires the complete original column definition".into(),
            ),
            MigrationOperation::SetComment {
                table, column, to, ..
            } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} MODIFY COLUMN {} COMMENT {}",
                    qi(table),
                    qi(column),
                    to.as_deref()
                        .map(|v| format!("'{}'", v.replace('\'', "''")))
                        .unwrap_or_else(|| "''".into())
                ),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("ALTER COMMENT {}.{}", table, column),
            }),
            MigrationOperation::SetAutoIncrement {
                table, column, to, ..
            } => Ok(MigrationStatement {
                sql: if *to {
                    format!(
                        "ALTER TABLE {} MODIFY COLUMN {} INT AUTO_INCREMENT",
                        qi(table),
                        qi(column)
                    )
                } else {
                    return Err(
                        "MySQL auto-increment removal requires original column definition".into(),
                    );
                },
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("ALTER AUTO_INCREMENT {}.{}", table, column),
            }),
            MigrationOperation::AddPrimaryKey { table, columns } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} ADD PRIMARY KEY ({})",
                    qi(table),
                    columns.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")
                ),
                risk: MigrationRisk::Additive,
                rollback_sql: Some(format!("ALTER TABLE {} DROP PRIMARY KEY", qi(table))),
                summary: format!("ADD PRIMARY KEY {}", table),
            }),
            MigrationOperation::DropPrimaryKey { table, .. } => Ok(MigrationStatement {
                sql: format!("ALTER TABLE {} DROP PRIMARY KEY", qi(table)),
                risk: MigrationRisk::Destructive,
                rollback_sql: None,
                summary: format!("DROP PRIMARY KEY {}", table),
            }),
        }
    }
}

pub struct MysqlMigrationCapabilities;
impl MigrationCapabilities for MysqlMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        match operation {
            MigrationOperation::SetNullable { .. } => false,
            MigrationOperation::SetAutoIncrement { to, .. } if !*to => false,
            MigrationOperation::CreateTable { .. }
            | MigrationOperation::AddColumn { .. }
            | MigrationOperation::DropColumn { .. }
            | MigrationOperation::AlterColumnType { .. }
            | MigrationOperation::SetDefault { .. }
            | MigrationOperation::SetComment { .. }
            | MigrationOperation::SetAutoIncrement { .. }
            | MigrationOperation::AddPrimaryKey { .. }
            | MigrationOperation::DropPrimaryKey { .. }
            | MigrationOperation::CreateIndex { .. }
            | MigrationOperation::DropIndex { .. } => true,
        }
    }
    fn requires_table_rebuild(&self, operation: &MigrationOperation) -> bool {
        matches!(
            operation,
            MigrationOperation::SetAutoIncrement { .. }
                | MigrationOperation::AlterColumnType { .. }
                | MigrationOperation::SetComment { .. }
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

    #[test]
    fn renderer_quotes_identifiers() {
        let s = MysqlMigrationRenderer
            .render(&MigrationOperation::AddColumn {
                table: "user`s".into(),
                column: col("na`me", "VARCHAR(32)"),
            })
            .unwrap();
        assert!(s.sql.contains("`user``s`"));
        assert!(s.sql.contains("`na``me`"));
    }

    #[test]
    fn renderer_rejects_unsafe_nullable_change() {
        let op = MigrationOperation::SetNullable {
            table: "users".into(),
            column: "name".into(),
            nullable: false,
        };
        assert!(!MysqlMigrationCapabilities.supports(&op));
        assert!(MysqlMigrationRenderer.render(&op).is_err());
    }

    #[test]
    fn create_table_renders_default_comment_and_auto_increment() {
        let op = MigrationOperation::CreateTable {
            table: "users".into(),
            columns: vec![MigrationColumn {
                name: "id".into(),
                data_type: "INT".into(),
                nullable: false,
                default_value: None,
                comment: Some("primary id".into()),
                is_auto_increment: true,
            }],
            primary_keys: vec!["id".into()],
        };
        let stmt = MysqlMigrationRenderer.render(&op).unwrap();
        assert!(stmt.sql.contains("AUTO_INCREMENT"));
        assert!(stmt.sql.contains("COMMENT 'primary id'"));
    }

    #[test]
    fn add_column_renders_metadata() {
        let op = MigrationOperation::AddColumn {
            table: "users".into(),
            column: MigrationColumn {
                name: "score".into(),
                data_type: "INT".into(),
                nullable: false,
                default_value: Some("0".into()),
                comment: Some("score".into()),
                is_auto_increment: false,
            },
        };
        let stmt = MysqlMigrationRenderer.render(&op).unwrap();
        assert!(stmt.sql.contains("DEFAULT 0"));
        assert!(stmt.sql.contains("COMMENT 'score'"));
    }

    #[test]
    fn add_primary_key_provides_rollback() {
        let op = MigrationOperation::AddPrimaryKey {
            table: "users".into(),
            columns: vec!["id".into()],
        };
        let stmt = MysqlMigrationRenderer.render(&op).unwrap();
        assert_eq!(
            stmt.rollback_sql.as_deref(),
            Some("ALTER TABLE `users` DROP PRIMARY KEY")
        );
    }

    #[test]
    fn drop_primary_key_renders() {
        let op = MigrationOperation::DropPrimaryKey {
            table: "users".into(),
            columns: vec!["id".into()],
        };
        let stmt = MysqlMigrationRenderer.render(&op).unwrap();
        assert_eq!(stmt.sql, "ALTER TABLE `users` DROP PRIMARY KEY");
        assert!(MysqlMigrationCapabilities.supports(&op));
    }

    #[test]
    fn capabilities_mark_type_change_as_rewrite() {
        let op = MigrationOperation::AlterColumnType {
            table: "users".into(),
            column: "id".into(),
            from: "INT".into(),
            to: "BIGINT".into(),
        };
        assert!(MysqlMigrationCapabilities.supports(&op));
        assert!(MysqlMigrationCapabilities.requires_table_rebuild(&op));
    }
}
