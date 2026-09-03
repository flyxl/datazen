use datazen_driver_api::*;

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
                    .map(|c| {
                        format!(
                            "{} {}{}",
                            qi(&c.name),
                            c.data_type,
                            if c.nullable { "" } else { " NOT NULL" }
                        )
                    })
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
                    "ALTER TABLE {} ADD COLUMN {} {}{}",
                    qi(table),
                    qi(&column.name),
                    column.data_type,
                    if column.nullable { "" } else { " NOT NULL" }
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
                rollback_sql: Some(format!("DROP INDEX {}", qi(&index.name))),
                summary: format!("CREATE INDEX {}.{}", table, index.name),
            }),
            MigrationOperation::DropIndex { index, .. } => Ok(MigrationStatement {
                sql: format!("DROP INDEX {}", qi(&index.name)),
                risk: MigrationRisk::Destructive,
                rollback_sql: None,
                summary: format!("DROP INDEX {}", index.name),
            }),
            MigrationOperation::AddPrimaryKey { table, columns } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} ADD PRIMARY KEY ({})",
                    qi(table),
                    columns.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")
                ),
                risk: MigrationRisk::Additive,
                rollback_sql: None,
                summary: format!("ADD PRIMARY KEY {}", table),
            }),
            MigrationOperation::DropPrimaryKey { .. } => Err(
                "PostgreSQL PK constraint name must be resolved from snapshot before rendering"
                    .into(),
            ),
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
        matches!(
            operation,
            MigrationOperation::CreateTable { .. }
                | MigrationOperation::AddColumn { .. }
                | MigrationOperation::DropColumn { .. }
                | MigrationOperation::AlterColumnType { .. }
                | MigrationOperation::SetNullable { .. }
                | MigrationOperation::SetDefault { .. }
                | MigrationOperation::SetComment { .. }
                | MigrationOperation::SetAutoIncrement { .. }
                | MigrationOperation::AddPrimaryKey { .. }
                | MigrationOperation::DropPrimaryKey { .. }
                | MigrationOperation::CreateIndex { .. }
                | MigrationOperation::DropIndex { .. }
        )
    }
    fn requires_table_rebuild(&self, operation: &MigrationOperation) -> bool {
        matches!(
            operation,
            MigrationOperation::SetAutoIncrement { .. }
                | MigrationOperation::AlterColumnType { .. }
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
}
