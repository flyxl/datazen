use datazen_driver_api::*;

pub struct MysqlMigrationRenderer;

impl MigrationRenderer for MysqlMigrationRenderer {
    fn render(&self, op: &MigrationOperation) -> Result<MigrationStatement, String> {
        let qi = |s: &str| format!("`{}`", s.replace('`', "``"));
        match op {
            MigrationOperation::CreateTable { table, columns, primary_keys } => { let cols = columns.iter().map(|c| format!("{} {}{}", qi(&c.name), c.data_type, if c.nullable { "" } else { " NOT NULL" })).collect::<Vec<_>>(); let pk = if primary_keys.is_empty() { String::new() } else { format!(", PRIMARY KEY ({})", primary_keys.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")) }; Ok(MigrationStatement { sql: format!("CREATE TABLE {} ({}{})", qi(table), cols.join(", "), pk), risk: MigrationRisk::Additive, rollback_sql: Some(format!("DROP TABLE {}", qi(table))), summary: format!("CREATE TABLE {}", table) }) },

            MigrationOperation::AddColumn { table, column } => Ok(MigrationStatement { sql: format!("ALTER TABLE {} ADD COLUMN {} {}{}", qi(table), qi(&column.name), column.data_type, if column.nullable { "" } else { " NOT NULL" }), risk: MigrationRisk::Additive, rollback_sql: Some(format!("ALTER TABLE {} DROP COLUMN {}", qi(table), qi(&column.name))), summary: format!("ADD COLUMN {}.{}", table, column.name) }),
            MigrationOperation::DropColumn { table, column } => Ok(MigrationStatement { sql: format!("ALTER TABLE {} DROP COLUMN {}", qi(table), qi(&column.name)), risk: MigrationRisk::Destructive, rollback_sql: None, summary: format!("DROP COLUMN {}.{}", table, column.name) }),
            MigrationOperation::SetDefault { table, column, from, to } => {
                let sql = match to { Some(v) => format!("ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}", qi(table), qi(column), v), None => format!("ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT", qi(table), qi(column)) };
                let rollback_sql = match from { Some(v) => Some(format!("ALTER TABLE {} ALTER COLUMN {} SET DEFAULT {}", qi(table), qi(column), v)), None => Some(format!("ALTER TABLE {} ALTER COLUMN {} DROP DEFAULT", qi(table), qi(column))) };
                Ok(MigrationStatement { sql, risk: MigrationRisk::Additive, rollback_sql, summary: format!("ALTER DEFAULT {}.{}", table, column) })
            }
            MigrationOperation::CreateIndex { table, index } => Ok(MigrationStatement { sql: format!("CREATE {}INDEX {} ON {} ({})", if index.is_unique { "UNIQUE " } else { "" }, qi(&index.name), qi(table), index.columns.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")), risk: MigrationRisk::Additive, rollback_sql: Some(format!("DROP INDEX {} ON {}", qi(&index.name), qi(table))), summary: format!("CREATE INDEX {}.{}", table, index.name) }),
            MigrationOperation::DropIndex { table, index } => Ok(MigrationStatement { sql: format!("DROP INDEX {} ON {}", qi(&index.name), qi(table)), risk: MigrationRisk::Destructive, rollback_sql: None, summary: format!("DROP INDEX {}.{}", table, index.name) }),
            MigrationOperation::AlterColumnType { table, column, to, .. } => Ok(MigrationStatement { sql: format!("ALTER TABLE {} MODIFY COLUMN {} {}", qi(table), qi(column), to), risk: MigrationRisk::Rewrite, rollback_sql: None, summary: format!("ALTER TYPE {}.{}", table, column) }),
            MigrationOperation::SetNullable { .. } => Err("MySQL nullability change requires the complete original column definition".into()),
            MigrationOperation::SetComment { table, column, to, .. } => Ok(MigrationStatement { sql: format!("ALTER TABLE {} MODIFY COLUMN {} COMMENT {}", qi(table), qi(column), to.as_deref().map(|v| format!("'{}'", v.replace('\'', "''"))).unwrap_or_else(|| "''".into())), risk: MigrationRisk::Rewrite, rollback_sql: None, summary: format!("ALTER COMMENT {}.{}", table, column) }),
            MigrationOperation::SetAutoIncrement { table, column, to, .. } => Ok(MigrationStatement { sql: if *to { format!("ALTER TABLE {} MODIFY COLUMN {} INT AUTO_INCREMENT", qi(table), qi(column)) } else { return Err("MySQL auto-increment removal requires original column definition".into()) }, risk: MigrationRisk::Rewrite, rollback_sql: None, summary: format!("ALTER AUTO_INCREMENT {}.{}", table, column) }),
            _ => Err(format!("MySQL renderer does not yet support {:?}", op)),
        }
    }
}

pub struct MysqlMigrationCapabilities;
impl MigrationCapabilities for MysqlMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        matches!(operation,
            MigrationOperation::CreateTable { .. } | MigrationOperation::AddColumn { .. } |
            MigrationOperation::DropColumn { .. } | MigrationOperation::AlterColumnType { .. } |
            MigrationOperation::SetNullable { .. } | MigrationOperation::SetDefault { .. } |
            MigrationOperation::SetComment { .. } | MigrationOperation::SetAutoIncrement { .. } |
            MigrationOperation::AddPrimaryKey { .. } | MigrationOperation::DropPrimaryKey { .. } |
            MigrationOperation::CreateIndex { .. } | MigrationOperation::DropIndex { .. })
    }
    fn requires_table_rebuild(&self, operation: &MigrationOperation) -> bool {
        matches!(operation, MigrationOperation::SetAutoIncrement { .. } | MigrationOperation::AlterColumnType { .. } | MigrationOperation::SetNullable { .. } | MigrationOperation::SetComment { .. })
    }
}
