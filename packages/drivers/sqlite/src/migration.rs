use datazen_driver_api::*;

pub struct SqliteMigrationRenderer;

impl MigrationRenderer for SqliteMigrationRenderer {
    fn render(&self, op: &MigrationOperation) -> Result<MigrationStatement, String> {
        let qi = |s: &str| format!("\"{}\"", s.replace('\"', "\"\""));
        match op {
            MigrationOperation::AddColumn { table, column } => Ok(MigrationStatement { sql: format!("ALTER TABLE {} ADD COLUMN {} {}{}", qi(table), qi(&column.name), column.data_type, if column.nullable { "" } else { " NOT NULL" }), risk: MigrationRisk::Additive, rollback_sql: None, summary: format!("ADD COLUMN {}.{}", table, column.name) }),
            MigrationOperation::CreateIndex { table, index } => Ok(MigrationStatement { sql: format!("CREATE {}INDEX {} ON {} ({})", if index.is_unique { "UNIQUE " } else { "" }, qi(&index.name), qi(table), index.columns.iter().map(|c| qi(c)).collect::<Vec<_>>().join(", ")), risk: MigrationRisk::Additive, rollback_sql: Some(format!("DROP INDEX {}", qi(&index.name))), summary: format!("CREATE INDEX {}.{}", table, index.name) }),
            MigrationOperation::DropIndex { index, .. } => Ok(MigrationStatement { sql: format!("DROP INDEX {}", qi(&index.name)), risk: MigrationRisk::Destructive, rollback_sql: None, summary: format!("DROP INDEX {}", index.name) }),
            MigrationOperation::DropColumn { .. } => Err("SQLite DROP COLUMN requires version/capability validation".into()),
            _ => Err(format!("SQLite renderer does not yet support {:?}; table rebuild may be required", op)),
        }
    }
}