//! Render dialect-neutral migration operations into executable SQL.

use super::{dialects::{mysql, postgres, sqlite}, operations::MigrationOperation, types::{normalize_dialect, PlanStatement}};

pub fn render_operation(dialect: &str, op: &MigrationOperation) -> Result<PlanStatement, String> {
    let d = normalize_dialect(dialect);
    let risk = op.risk();
    match op {
        MigrationOperation::AddColumn { table, column } => {
            let ty = column.data_type.clone();
            Ok(match d.as_str() {
                "postgresql" => postgres::add_column(table, column, &ty),
                "mysql" => mysql::add_column(table, column, &ty),
                "sqlite" => sqlite::add_column(table, column, &ty),
                _ => return Err(format!("unsupported dialect: {}", d)),
            })
        }
        MigrationOperation::DropColumn { table, column } => {
            let sql = match d.as_str() {
                "postgresql" => format!("ALTER TABLE \\"{}\\" DROP COLUMN \\"{}\\"", table.replace('"', "\\"\\""), column.name.replace('"', "\\"\\"")),
                "mysql" => format!("ALTER TABLE `{}` DROP COLUMN `{}`", table.replace('`', "``"), column.name.replace('`', "``")),
                _ => return Err(format!("DROP COLUMN unsupported for {}", d)),
            };
            Ok(PlanStatement { sql, risk, rollback_sql: None, summary: format!("DROP COLUMN {}.{}", table, column.name) })
        }
        MigrationOperation::SetDefault { table, column, from, to } => Ok(super::dialects::set_default_with_rollback(&d, table, column, to.as_deref(), from.as_deref())),
        MigrationOperation::CreateIndex { table, index } => Ok(match d.as_str() {
            "postgresql" => postgres::create_index(table, &index.name, &index.columns, index.is_unique),
            "mysql" => mysql::create_index(table, &index.name, &index.columns, index.is_unique),
            "sqlite" => sqlite::create_index(table, &index.name, &index.columns, index.is_unique),
            _ => return Err(format!("unsupported dialect: {}", d)),
        }),
        MigrationOperation::DropIndex { table, index } => {
            let sql = match d.as_str() {
                "mysql" => format!("DROP INDEX `{}` ON `{}`", index.name.replace('`', "``"), table.replace('`', "``")),
                "postgresql" | "sqlite" => format!("DROP INDEX \\"{}\\"", index.name.replace('"', "\\"\\"")),
                _ => return Err(format!("unsupported dialect: {}", d)),
            };
            Ok(PlanStatement { sql, risk, rollback_sql: None, summary: format!("DROP INDEX {}.{}", table, index.name) })
        }
        MigrationOperation::AddPrimaryKey { table, columns } => match d.as_str() {
            "postgresql" => Ok(postgres::add_primary_key(table, columns)),
            "mysql" => Ok(mysql::add_primary_key(table, columns)),
            _ => Err(format!("primary-key ADD unsupported for {}", d)),
        },
        MigrationOperation::DropPrimaryKey { table, .. } => match d.as_str() {
            "mysql" => Ok(mysql::drop_primary_key(table)),
            "postgresql" => Err("PostgreSQL PK constraint name is required for safe DROP".into()),
            _ => Err(format!("primary-key DROP unsupported for {}", d)),
        },
        MigrationOperation::CreateTable { table, columns, primary_keys } => {
            let src = crate::db::TableSchema { table_name: table.clone(), columns: columns.iter().map(|c| crate::db::ColumnSchema { name:c.name.clone(), data_type:c.data_type.clone(), nullable:c.nullable, default_value:c.default_value.clone(), comment:c.comment.clone(), is_primary_key:c.is_primary_key, is_auto_increment:c.is_auto_increment }).collect(), primary_keys:primary_keys.clone(), indexes:vec![], foreign_keys:vec![] };
            let resolved = columns.iter().map(|c|(c.name.clone(),c.data_type.clone())).collect();
            Ok(super::dialects::create_table_stmt(&d, table, &src, &resolved))
        }
        MigrationOperation::AlterColumnType { .. } | MigrationOperation::SetNullable { .. } | MigrationOperation::SetComment { .. } | MigrationOperation::SetAutoIncrement { .. } => Err(format!("operation requires dialect-specific renderer for {}", d)),
    }
}