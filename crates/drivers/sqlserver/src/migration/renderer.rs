use super::sql::*;
use super::validation::*;
use super::views::*;
use super::*;
use std::collections::HashSet;

impl MigrationRenderer for SqlServerMigrationRenderer {
    fn render(&self, operation: &MigrationOperation) -> Result<MigrationStatement, String> {
        match operation {
            MigrationOperation::CreateTable {
                table,
                columns,
                primary_keys,
            } => create_table_sql(table, columns, primary_keys),
            MigrationOperation::DropTable { table } => {
                let table = relation(table)?;
                Ok(MigrationStatement {
                    sql: format!("DROP TABLE {table}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: None,
                    summary: format!("DROP TABLE {table}"),
                })
            }
            MigrationOperation::AddColumn { table, column } => {
                let table_sql = relation(table)?;
                let definition = validate_column(column)?;
                let mut sql = format!("ALTER TABLE {table_sql} ADD {definition}");
                if let Some(comment) = column.comment.as_deref() {
                    sql.push_str("; ");
                    sql.push_str(&comment_property_sql(table, &column.name, Some(comment))?);
                }
                let mut rollback = drop_default_for_column_sql(column, table)?;
                if column.comment.is_some() {
                    rollback.push_str("; ");
                    rollback.push_str(&comment_property_sql(table, &column.name, None)?);
                }
                rollback.push_str(&format!(
                    " ALTER TABLE {table_sql} DROP COLUMN {}",
                    quote_ident(&column.name)?
                ));
                Ok(MigrationStatement {
                    sql,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(rollback),
                    summary: format!("ADD COLUMN {}.{}", table, column.name),
                })
            }
            MigrationOperation::DropColumn { table, column } => {
                let table_sql = relation(table)?;
                let mut sql = drop_default_for_column_sql(column, table)?;
                sql.push_str(&format!(
                    " ALTER TABLE {table_sql} DROP COLUMN {}",
                    quote_ident(&column.name)?
                ));
                Ok(MigrationStatement {
                    sql,
                    risk: MigrationRisk::Destructive,
                    rollback_sql: None,
                    summary: format!("DROP COLUMN {}.{}", table, column.name),
                })
            }
            MigrationOperation::AlterColumnType {
                table, column, to, ..
            } => {
                let to = validate_type(to)?;
                Ok(MigrationStatement {
                    sql: alter_column_type_sql(table, column, &to)?,
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: None,
                    summary: format!("ALTER TYPE {table}.{column}"),
                })
            }
            MigrationOperation::SetNullable {
                table,
                column,
                nullable,
            } => Ok(MigrationStatement {
                sql: catalog_type_sql(table, column, *nullable)?,
                risk: if *nullable {
                    MigrationRisk::Additive
                } else {
                    MigrationRisk::Rewrite
                },
                rollback_sql: None,
                summary: format!("ALTER NULLABILITY {table}.{column}"),
            }),
            MigrationOperation::SetDefault {
                table,
                column,
                from,
                to,
            } => Ok(MigrationStatement {
                sql: set_default_sql(table, column, from.as_deref(), to.as_deref())?,
                risk: MigrationRisk::Rewrite,
                rollback_sql: Some(set_default_sql(
                    table,
                    column,
                    to.as_deref(),
                    from.as_deref(),
                )?),
                summary: format!("ALTER DEFAULT {table}.{column}"),
            }),
            MigrationOperation::SetComment {
                table,
                column,
                from,
                to,
            } => Ok(MigrationStatement {
                sql: comment_property_sql(table, column, to.as_deref())?,
                risk: MigrationRisk::Additive,
                rollback_sql: Some(comment_property_sql(table, column, from.as_deref())?),
                summary: format!("ALTER COMMENT {table}.{column}"),
            }),
            MigrationOperation::SetTableOptions { .. } => {
                Err("SQL Server table options are outside the shared migration model".into())
            }
            MigrationOperation::SetAutoIncrement { .. } => Err(
                "SQL Server IDENTITY changes require a table rebuild and are not supported".into(),
            ),
            MigrationOperation::AddPrimaryKey { table, columns } => {
                validate_unique_columns(columns, "primary key")?;
                let table = relation(table)?;
                let columns_sql = columns
                    .iter()
                    .map(|column| quote_ident(column))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                Ok(MigrationStatement {
                    sql: format!("ALTER TABLE {table} ADD PRIMARY KEY ({columns_sql})"),
                    risk: MigrationRisk::Additive,
                    rollback_sql: None,
                    summary: format!("ADD PRIMARY KEY {table}"),
                })
            }
            MigrationOperation::DropPrimaryKey { table, columns } => {
                let mut sql = primary_key_lookup(table, columns)?;
                let table_sql = relation(table)?;
                sql.push_str(&format!(
                    " EXEC(N'ALTER TABLE {table_sql} DROP CONSTRAINT ' + QUOTENAME(@datazen_pk_name));"
                ));
                Ok(MigrationStatement {
                    sql,
                    risk: MigrationRisk::Destructive,
                    rollback_sql: None,
                    summary: format!("DROP PRIMARY KEY {table}"),
                })
            }
            MigrationOperation::CreateIndex { table, index } => render_index_create(table, index),
            MigrationOperation::DropIndex { table, index } => {
                if index.is_primary {
                    return Err(
                        "SQL Server primary-key indexes must be removed with DropPrimaryKey"
                            .into(),
                    );
                }
                let table = relation(table)?;
                let name = quote_ident(&index.name)?;
                let is_constraint = index.index_type.starts_with("UNIQUE_CONSTRAINT:");
                let rollback = render_index_create(&table, index)?.sql;
                Ok(MigrationStatement {
                    sql: if is_constraint {
                        format!("ALTER TABLE {table} DROP CONSTRAINT {name}")
                    } else {
                        format!("DROP INDEX {name} ON {table}")
                    },
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(rollback),
                    summary: format!("DROP INDEX {} on {table}", index.name),
                })
            }
            MigrationOperation::AddForeignKey { table, foreign_key } => {
                validate_unique_columns(&foreign_key.columns, "foreign key")?;
                validate_unique_columns(&foreign_key.referenced_columns, "referenced key")?;
                if foreign_key.columns.len() != foreign_key.referenced_columns.len() {
                    return Err("SQL Server foreign-key column lists must have equal lengths".into());
                }
                if foreign_key.deferrability != ForeignKeyDeferrability::NotDeferrable {
                    return Err("SQL Server foreign keys must be proven non-deferrable".into());
                }
                let table = relation(table)?;
                let referenced = relation(&foreign_key.referenced_table)?;
                if relation_parts(&foreign_key.referenced_table)?.len() != 2 {
                    return Err("SQL Server foreign keys must reference a schema-qualified table in the same database".into());
                }
                let name = quote_ident(&foreign_key.name)?;
                let columns = foreign_key
                    .columns
                    .iter()
                    .map(|column| quote_ident(column))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                let referenced_columns = foreign_key
                    .referenced_columns
                    .iter()
                    .map(|column| quote_ident(column))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ");
                let action = |value: &str, clause: &str| -> Result<String, String> {
                    let action = value.trim().to_ascii_uppercase();
                    match action.as_str() {
                        "" | "NO ACTION" => Ok(String::new()),
                        "CASCADE" | "SET NULL" | "SET DEFAULT" => {
                            Ok(format!(" ON {clause} {action}"))
                        }
                        other => Err(format!("unsupported SQL Server foreign-key action '{other}'")),
                    }
                };
                let update = action(&foreign_key.on_update, "UPDATE")?;
                let delete = action(&foreign_key.on_delete, "DELETE")?;
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {table} ADD CONSTRAINT {name} FOREIGN KEY ({columns}) REFERENCES {referenced} ({referenced_columns}){delete}{update}"
                    ),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("ALTER TABLE {table} DROP CONSTRAINT {name}")),
                    summary: format!("ADD FOREIGN KEY {}.{}", table, foreign_key.name),
                })
            }
            MigrationOperation::DropForeignKey { table, foreign_key } => {
                let table_sql = relation(table)?;
                let rollback = self
                    .render(&MigrationOperation::AddForeignKey {
                        table: table.clone(),
                        foreign_key: foreign_key.clone(),
                    })?
                    .sql;
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {table_sql} DROP CONSTRAINT {}",
                        quote_ident(&foreign_key.name)?
                    ),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(rollback),
                    summary: format!("DROP FOREIGN KEY {}.{}", table, foreign_key.name),
                })
            }
            MigrationOperation::AddCheckConstraint { table, constraint } => {
                validate_check_expression(&constraint.expression)?;
                let table = relation(table)?;
                let name = quote_ident(&constraint.name)?;
                let expression = literal_expression(&constraint.expression, "CHECK expression")?;
                Ok(MigrationStatement {
                    sql: format!("ALTER TABLE {table} ADD CONSTRAINT {name} CHECK ({expression})"),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("ALTER TABLE {table} DROP CONSTRAINT {name}")),
                    summary: format!("ADD CHECK {}.{}", table, constraint.name),
                })
            }
            MigrationOperation::DropCheckConstraint { table, constraint } => {
                validate_check_expression(&constraint.expression)?;
                let table = relation(table)?;
                let name = quote_ident(&constraint.name)?;
                let rollback = self
                    .render(&MigrationOperation::AddCheckConstraint {
                        table: table.clone(),
                        constraint: constraint.clone(),
                    })?
                    .sql;
                Ok(MigrationStatement {
                    sql: format!("ALTER TABLE {table} DROP CONSTRAINT {name}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(rollback),
                    summary: format!("DROP CHECK {}.{}", table, constraint.name),
                })
            }
            MigrationOperation::CreateView { view } => {
                let (ident, sql) = validate_view(view)?;
                Ok(MigrationStatement {
                    sql,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP VIEW {ident}")),
                    summary: format!("CREATE VIEW {}", view.name),
                })
            }
            MigrationOperation::ReplaceView { current, desired } => {
                if current.schema != desired.schema || current.name != desired.name {
                    return Err("SQL Server view replacement identities must match exactly".into());
                }
                let (_, desired_sql) = validate_view(desired)?;
                let (_, current_sql) = validate_view(current)?;
                Ok(MigrationStatement {
                    sql: to_alter_view(&desired_sql)?,
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: Some(to_alter_view(&current_sql)?),
                    summary: format!("ALTER VIEW {}", desired.name),
                })
            }
            MigrationOperation::DropView { view } => {
                let (ident, definition) = validate_view(view)?;
                Ok(MigrationStatement {
                    sql: format!("DROP VIEW {ident}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(definition),
                    summary: format!("DROP VIEW {}", view.name),
                })
            }
            MigrationOperation::CreateRoutine { .. }
            | MigrationOperation::ReplaceRoutine { .. }
            | MigrationOperation::DropRoutine { .. }
            | MigrationOperation::CreateTrigger { .. }
            | MigrationOperation::ReplaceTrigger { .. }
            | MigrationOperation::DropTrigger { .. } => Err(
                "SQL Server routine and trigger migration is unsupported until exact scope, definition, and dependency rewriting is proven".into(),
            ),
            MigrationOperation::CreateSequence { .. }
            | MigrationOperation::ReplaceSequence { .. }
            | MigrationOperation::DropSequence { .. }
            | MigrationOperation::CreateType { .. }
            | MigrationOperation::ReplaceType { .. }
            | MigrationOperation::DropType { .. } => Err(
                "SQL Server sequence and user-defined type migration is outside the supported subset".into(),
            ),
        }
    }

    fn render_create_table_with_options(
        &self,
        operation: &MigrationOperation,
        table_options: &TableOptions,
    ) -> Result<MigrationStatement, String> {
        if !table_options_supported(table_options) {
            return Err("SQL Server CREATE TABLE cannot preserve the supplied table options or metadata blockers".into());
        }
        self.render(operation)
    }
}

impl MigrationCapabilities for SqlServerMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        match operation {
            MigrationOperation::CreateTable {
                table,
                columns,
                primary_keys,
                ..
            } => {
                relation(table).is_ok()
                    && !columns.is_empty()
                    && columns.iter().all(|column| validate_column(column).is_ok())
                    && columns
                        .iter()
                        .map(|column| column.name.as_str())
                        .collect::<HashSet<_>>()
                        .len()
                        == columns.len()
                    && (primary_keys.is_empty()
                        || (validate_unique_columns(primary_keys, "primary key").is_ok()
                            && primary_keys
                                .iter()
                                .all(|column| columns.iter().any(|c| &c.name == column))))
                    && columns
                        .iter()
                        .filter(|column| column.is_auto_increment)
                        .count()
                        <= 1
            }
            MigrationOperation::DropTable { table } => relation(table).is_ok(),
            MigrationOperation::AddColumn { table, column }
            | MigrationOperation::DropColumn { table, column } => {
                relation(table).is_ok() && validate_column(column).is_ok()
            }
            MigrationOperation::AlterColumnType {
                table, column, to, ..
            } => {
                relation(table).is_ok() && quote_ident(column).is_ok() && validate_type(to).is_ok()
            }
            MigrationOperation::SetNullable { table, column, .. } => {
                relation(table).is_ok() && quote_ident(column).is_ok()
            }
            MigrationOperation::SetDefault {
                table,
                column,
                from,
                to,
            } => set_default_sql(table, column, from.as_deref(), to.as_deref()).is_ok(),
            MigrationOperation::SetComment {
                table,
                column,
                from,
                to,
            } => {
                comment_property_sql(table, column, to.as_deref()).is_ok()
                    && comment_property_sql(table, column, from.as_deref()).is_ok()
            }
            MigrationOperation::SetTableOptions { .. }
            | MigrationOperation::SetAutoIncrement { .. } => false,
            MigrationOperation::AddPrimaryKey { table, columns }
            | MigrationOperation::DropPrimaryKey { table, columns } => {
                relation(table).is_ok() && validate_unique_columns(columns, "primary key").is_ok()
            }
            MigrationOperation::CreateIndex { table, index } => {
                !index.is_primary
                    && relation(table).is_ok()
                    && render_index_create(table, index).is_ok()
            }
            MigrationOperation::DropIndex { table, index } => {
                !index.is_primary
                    && relation(table).is_ok()
                    && quote_ident(&index.name).is_ok()
                    && render_index_create(table, index).is_ok()
            }
            MigrationOperation::AddForeignKey { table, foreign_key } => {
                relation(table).is_ok()
                    && quote_ident(&foreign_key.name).is_ok()
                    && relation(&foreign_key.referenced_table).is_ok_and(|_| {
                        relation_parts(&foreign_key.referenced_table)
                            .is_ok_and(|parts| parts.len() == 2)
                    })
                    && validate_unique_columns(&foreign_key.columns, "foreign key").is_ok()
                    && validate_unique_columns(&foreign_key.referenced_columns, "referenced key")
                        .is_ok()
                    && foreign_key.columns.len() == foreign_key.referenced_columns.len()
                    && foreign_key.deferrability == ForeignKeyDeferrability::NotDeferrable
                    && [
                        foreign_key.on_update.as_str(),
                        foreign_key.on_delete.as_str(),
                    ]
                    .iter()
                    .all(|action| {
                        matches!(
                            action.trim().to_ascii_uppercase().as_str(),
                            "" | "NO ACTION" | "CASCADE" | "SET NULL" | "SET DEFAULT"
                        )
                    })
            }
            MigrationOperation::DropForeignKey { table, foreign_key } => {
                relation(table).is_ok()
                    && quote_ident(&foreign_key.name).is_ok()
                    && SqlServerMigrationRenderer
                        .render(&MigrationOperation::AddForeignKey {
                            table: table.clone(),
                            foreign_key: foreign_key.clone(),
                        })
                        .is_ok()
            }
            MigrationOperation::AddCheckConstraint { table, constraint }
            | MigrationOperation::DropCheckConstraint { table, constraint } => {
                relation(table).is_ok()
                    && quote_ident(&constraint.name).is_ok()
                    && validate_check_expression(&constraint.expression).is_ok()
                    && !has_executable_separator(&constraint.expression)
            }
            MigrationOperation::CreateView { view } | MigrationOperation::DropView { view } => {
                validate_view(view).is_ok()
            }
            MigrationOperation::ReplaceView { current, desired } => {
                current.schema == desired.schema
                    && current.name == desired.name
                    && validate_view(current).is_ok()
                    && validate_view(desired).is_ok()
            }
            MigrationOperation::CreateRoutine { .. }
            | MigrationOperation::ReplaceRoutine { .. }
            | MigrationOperation::DropRoutine { .. }
            | MigrationOperation::CreateTrigger { .. }
            | MigrationOperation::ReplaceTrigger { .. }
            | MigrationOperation::DropTrigger { .. }
            | MigrationOperation::CreateSequence { .. }
            | MigrationOperation::ReplaceSequence { .. }
            | MigrationOperation::DropSequence { .. }
            | MigrationOperation::CreateType { .. }
            | MigrationOperation::ReplaceType { .. }
            | MigrationOperation::DropType { .. } => false,
        }
    }

    fn requires_table_rebuild(&self, _operation: &MigrationOperation) -> bool {
        false
    }
}
