use datazen_driver_api::*;

fn format_pg_column_def(c: &MigrationColumn, qi: &impl Fn(&str) -> String) -> String {
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

fn pg_comment_on_column(table: &str, column: &str, comment: &str, qi: &impl Fn(&str) -> String) -> String {
    format!(
        "COMMENT ON COLUMN {}.{} IS '{}'",
        qi(table),
        qi(column),
        comment.replace('\'', "''")
    )
}

fn append_pg_column_comments(
    mut sql: String,
    table: &str,
    columns: &[MigrationColumn],
    qi: &impl Fn(&str) -> String,
) -> String {
    for column in columns {
        if let Some(comment) = &column.comment {
            if !comment.is_empty() {
                sql.push_str("; ");
                sql.push_str(&pg_comment_on_column(table, &column.name, comment, qi));
            }
        }
    }
    sql
}

fn pg_pk_constraint_name(table: &str) -> String {
    format!("{table}_pkey")
}

pub struct PostgresMigrationRenderer;

impl MigrationRenderer for PostgresMigrationRenderer {
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
                    .map(|c| format_pg_column_def(c, &qi))
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
                let sql = append_pg_column_comments(
                    format!("CREATE TABLE {} ({}{})", qi(table), cols.join(", "), pk),
                    table,
                    columns,
                    &qi,
                );
                Ok(MigrationStatement {
                    sql,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP TABLE {}", qi(table))),
                    summary: format!("CREATE TABLE {}", table),
                })
            }

            MigrationOperation::AddColumn { table, column } => {
                let mut sql = format!(
                    "ALTER TABLE {} ADD COLUMN {}",
                    qi(table),
                    format_pg_column_def(column, &qi)
                );
                if let Some(comment) = &column.comment {
                    if !comment.is_empty() {
                        sql.push_str("; ");
                        sql.push_str(&pg_comment_on_column(table, &column.name, comment, &qi));
                    }
                }
                Ok(MigrationStatement {
                    sql,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {} DROP COLUMN {}",
                        qi(table),
                        qi(&column.name)
                    )),
                    summary: format!("ADD COLUMN {}.{}", table, column.name),
                })
            }
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
                rollback_sql: Some(format!("DROP INDEX {}", qi(&index.name))),
                summary: format!("CREATE INDEX {}.{}", table, index.name),
            }),
            MigrationOperation::DropIndex { index, .. } => Ok(MigrationStatement {
                sql: format!("DROP INDEX {}", qi(&index.name)),
                risk: MigrationRisk::Destructive,
                rollback_sql: None,
                summary: format!("DROP INDEX {}", index.name),
            }),
            MigrationOperation::AddPrimaryKey { table, columns } => {
                let pk_name = pg_pk_constraint_name(table);
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} PRIMARY KEY ({})",
                        qi(table),
                        qi(&pk_name),
                        columns.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")
                    ),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {} DROP CONSTRAINT {}",
                        qi(table),
                        qi(&pk_name)
                    )),
                    summary: format!("ADD PRIMARY KEY {}", table),
                })
            }
            MigrationOperation::DropPrimaryKey { table, .. } => {
                let pk_name = pg_pk_constraint_name(table);
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {} DROP CONSTRAINT {}",
                        qi(table),
                        qi(&pk_name)
                    ),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: None,
                    summary: format!("DROP PRIMARY KEY {}", table),
                })
            }
            MigrationOperation::AlterColumnType {
                table, column, to, ..
            } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} ALTER COLUMN {} TYPE {}",
                    qi(table),
                    qi(column),
                    to
                ),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("ALTER TYPE {}.{}", table, column),
            }),
            MigrationOperation::SetNullable {
                table,
                column,
                nullable,
            } => Ok(MigrationStatement {
                sql: if *nullable {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} DROP NOT NULL",
                        qi(table),
                        qi(column)
                    )
                } else {
                    format!(
                        "ALTER TABLE {} ALTER COLUMN {} SET NOT NULL",
                        qi(table),
                        qi(column)
                    )
                },
                risk: if *nullable {
                    MigrationRisk::Additive
                } else {
                    MigrationRisk::Rewrite
                },
                rollback_sql: None,
                summary: format!("ALTER NULLABILITY {}.{}", table, column),
            }),
            MigrationOperation::SetComment {
                table, column, to, ..
            } => Ok(MigrationStatement {
                sql: format!(
                    "COMMENT ON COLUMN {}.{} IS {}",
                    qi(table),
                    qi(column),
                    to.as_deref()
                        .map(|v| format!("'{}'", v.replace('\'', "''")))
                        .unwrap_or_else(|| "NULL".into())
                ),
                risk: MigrationRisk::Additive,
                rollback_sql: None,
                summary: format!("ALTER COMMENT {}.{}", table, column),
            }),
            MigrationOperation::SetAutoIncrement { .. } => {
                Err("PostgreSQL auto-increment changes require identity/sequence metadata".into())
            }
            _ => Err(format!("PostgreSQL renderer does not yet support {:?}", op)),
        }
    }
}

pub struct PostgresMigrationCapabilities;
impl MigrationCapabilities for PostgresMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        match operation {
            MigrationOperation::SetAutoIncrement { .. } => false,
            MigrationOperation::CreateTable { .. }
            | MigrationOperation::AddColumn { .. }
            | MigrationOperation::DropColumn { .. }
            | MigrationOperation::AlterColumnType { .. }
            | MigrationOperation::SetNullable { .. }
            | MigrationOperation::SetDefault { .. }
            | MigrationOperation::SetComment { .. }
            | MigrationOperation::AddPrimaryKey { .. }
            | MigrationOperation::DropPrimaryKey { .. }
            | MigrationOperation::CreateIndex { .. }
            | MigrationOperation::DropIndex { .. } => true,
        }
    }
    fn requires_table_rebuild(&self, operation: &MigrationOperation) -> bool {
        matches!(
            operation,
            MigrationOperation::AlterColumnType { .. }
                | MigrationOperation::SetNullable { .. }
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
    fn renders_nullable_change() {
        let op = MigrationOperation::SetNullable {
            table: "users".into(),
            column: "name".into(),
            nullable: true,
        };
        assert_eq!(
            PostgresMigrationRenderer.render(&op).unwrap().sql,
            "ALTER TABLE \"users\" ALTER COLUMN \"name\" DROP NOT NULL"
        );
    }
    #[test]
    fn escapes_comment_quotes() {
        let op = MigrationOperation::SetComment {
            table: "users".into(),
            column: "name".into(),
            from: None,
            to: Some("Bob's name".into()),
        };
        assert!(PostgresMigrationRenderer
            .render(&op)
            .unwrap()
            .sql
            .contains("Bob''s name"));
    }
    #[test]
    fn capabilities_mark_type_change_as_rewrite() {
        let op = MigrationOperation::AlterColumnType {
            table: "users".into(),
            column: "id".into(),
            from: "integer".into(),
            to: "bigint".into(),
        };
        assert!(PostgresMigrationCapabilities.supports(&op));
        assert!(PostgresMigrationCapabilities.requires_table_rebuild(&op));
    }

    #[test]
    fn create_table_renders_default_and_comment() {
        let op = MigrationOperation::CreateTable {
            table: "users".into(),
            columns: vec![
                MigrationColumn {
                    name: "id".into(),
                    data_type: "integer".into(),
                    nullable: false,
                    default_value: None,
                    comment: None,
                    is_auto_increment: false,
                },
                MigrationColumn {
                    name: "status".into(),
                    data_type: "text".into(),
                    nullable: true,
                    default_value: Some("'active'".into()),
                    comment: Some("user status".into()),
                    is_auto_increment: false,
                },
            ],
            primary_keys: vec!["id".into()],
        };
        let stmt = PostgresMigrationRenderer.render(&op).unwrap();
        assert!(stmt.sql.contains("DEFAULT 'active'"));
        assert!(stmt.sql.contains("COMMENT ON COLUMN \"users\".\"status\" IS 'user status'"));
    }

    #[test]
    fn add_column_renders_default_and_comment() {
        let op = MigrationOperation::AddColumn {
            table: "users".into(),
            column: MigrationColumn {
                name: "score".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: Some("0".into()),
                comment: Some("score".into()),
                is_auto_increment: false,
            },
        };
        let stmt = PostgresMigrationRenderer.render(&op).unwrap();
        assert!(stmt.sql.starts_with(
            "ALTER TABLE \"users\" ADD COLUMN \"score\" integer NOT NULL DEFAULT 0"
        ));
        assert!(stmt.sql.contains("COMMENT ON COLUMN \"users\".\"score\" IS 'score'"));
        assert_eq!(
            stmt.rollback_sql.as_deref(),
            Some("ALTER TABLE \"users\" DROP COLUMN \"score\"")
        );
    }

    #[test]
    fn drop_primary_key_uses_table_pkey_convention() {
        let op = MigrationOperation::DropPrimaryKey {
            table: "users".into(),
            columns: vec!["id".into()],
        };
        let stmt = PostgresMigrationRenderer.render(&op).unwrap();
        assert_eq!(
            stmt.sql,
            "ALTER TABLE \"users\" DROP CONSTRAINT \"users_pkey\""
        );
        assert!(PostgresMigrationCapabilities.supports(&op));
    }

    #[test]
    fn add_primary_key_provides_rollback() {
        let op = MigrationOperation::AddPrimaryKey {
            table: "users".into(),
            columns: vec!["id".into()],
        };
        let stmt = PostgresMigrationRenderer.render(&op).unwrap();
        assert_eq!(
            stmt.rollback_sql.as_deref(),
            Some("ALTER TABLE \"users\" DROP CONSTRAINT \"users_pkey\"")
        );
    }

    #[test]
    fn capabilities_reject_set_auto_increment() {
        let op = MigrationOperation::SetAutoIncrement {
            table: "users".into(),
            column: "id".into(),
            from: false,
            to: true,
        };
        assert!(!PostgresMigrationCapabilities.supports(&op));
        assert!(PostgresMigrationRenderer.render(&op).is_err());
    }
}
