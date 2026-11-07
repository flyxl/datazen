use datazen_driver_api::*;

fn quote_sqlite_name(name: &str) -> Result<String, String> {
    validate_migration_identifier(name)?;
    if name.contains('.') {
        return Err("SQLite table rebuild accepts a single identifier at this boundary".into());
    }
    Ok(format!("\"{}\"", name.replace('"', "\"\"")))
}

fn quote_sqlite_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn sqlite_rowid_alias(schema: &TableSchema) -> Option<&str> {
    let primary_keys = schema.effective_primary_keys();
    if primary_keys.len() != 1 || schema.indexes.iter().any(|index| index.is_primary) {
        return None;
    }
    let primary_key = primary_keys.first()?;
    schema
        .columns
        .iter()
        .find(|column| {
            column.name.eq_ignore_ascii_case(primary_key)
                && column.data_type.trim().eq_ignore_ascii_case("INTEGER")
        })
        .map(|column| column.name.as_str())
}

fn available_sqlite_rowid_alias(columns: &[ColumnSchema]) -> Option<&'static str> {
    ["rowid", "_rowid_", "oid"].into_iter().find(|alias| {
        !columns
            .iter()
            .any(|column| column.name.eq_ignore_ascii_case(alias))
    })
}

fn qualify_sqlite_table(name: &str) -> Result<(String, String, String), String> {
    let name = validate_migration_identifier(name)?;
    let parts = name.split('.').collect::<Vec<_>>();
    if parts.len() > 2 {
        return Err("SQLite table rebuild supports only an optional database qualifier".into());
    }
    let table = parts.last().copied().unwrap_or_default();
    let table_sql = quote_sqlite_name(table)?;
    let schema_sql = parts
        .first()
        .filter(|_| parts.len() == 2)
        .map(|schema| quote_sqlite_name(schema))
        .transpose()?;
    let qualified = schema_sql
        .as_ref()
        .map(|schema| format!("{schema}.{table_sql}"))
        .unwrap_or_else(|| table_sql.clone());
    Ok((qualified, table_sql, schema_sql.unwrap_or_default()))
}

fn validate_sqlite_expression(expression: &str, label: &str) -> Result<(), String> {
    if expression.trim().is_empty()
        || expression
            .chars()
            .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')))
    {
        return Err(format!(
            "SQLite {label} is empty or contains control characters"
        ));
    }
    let tokens =
        sqlparser::tokenizer::Tokenizer::new(&sqlparser::dialect::SQLiteDialect {}, expression)
            .tokenize()
            .map_err(|error| format!("could not validate SQLite {label}: {error}"))?;
    if tokens
        .iter()
        .any(|token| matches!(token, sqlparser::tokenizer::Token::SemiColon))
    {
        return Err(format!("SQLite {label} must be one expression"));
    }
    Ok(())
}

fn validate_sqlite_type_name(data_type: &str) -> Result<(), String> {
    let sql = format!("CREATE TABLE __datazen_type_check (\"value\" {data_type})");
    let statements =
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::SQLiteDialect {}, &sql)
            .map_err(|error| format!("could not validate SQLite type name: {error}"))?;
    let Some(sqlparser::ast::Statement::CreateTable(table)) = statements.first() else {
        return Err("SQLite type name did not parse as a column definition".into());
    };
    if statements.len() != 1
        || table.columns.len() != 1
        || !table.constraints.is_empty()
        || !table.columns[0].options.is_empty()
    {
        return Err("SQLite type name contains syntax outside a declared type".into());
    }
    Ok(())
}

fn validate_sqlite_default(expression: &str) -> Result<(), String> {
    validate_sqlite_expression(expression, "default")?;
    let sql = format!(
        "CREATE TABLE __datazen_default_check (\"value\" TEXT DEFAULT {})",
        expression.trim()
    );
    let statements =
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::SQLiteDialect {}, &sql)
            .map_err(|error| format!("could not validate SQLite default expression: {error}"))?;
    let Some(sqlparser::ast::Statement::CreateTable(table)) = statements.first() else {
        return Err("SQLite default did not parse as a column definition".into());
    };
    let valid_default = table.columns.len() == 1
        && table.constraints.is_empty()
        && table.columns[0].options.len() == 1
        && matches!(
            table.columns[0]
                .options
                .first()
                .map(|option| &option.option),
            Some(sqlparser::ast::ColumnOption::Default(_))
        );
    if statements.len() != 1 || !valid_default {
        return Err("SQLite default contains syntax outside a single expression".into());
    }
    Ok(())
}

fn validate_sqlite_check(expression: &str) -> Result<(), String> {
    validate_sqlite_expression(expression, "CHECK expression")?;
    let sql = format!(
        "CREATE TABLE __datazen_check_expression (CHECK ({}))",
        expression.trim()
    );
    let statements =
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::SQLiteDialect {}, &sql)
            .map_err(|error| format!("could not validate SQLite CHECK expression: {error}"))?;
    let valid_check = matches!(
        statements.first(),
        Some(sqlparser::ast::Statement::CreateTable(table))
            if table.columns.is_empty()
                && table.constraints.len() == 1
                && matches!(
                    table.constraints.first(),
                    Some(sqlparser::ast::TableConstraint::Check { .. })
                )
    );
    if statements.len() != 1 || !valid_check {
        return Err("SQLite CHECK contains syntax outside a single expression".into());
    }
    Ok(())
}

fn sqlite_table_definition(
    table: &str,
    desired: &TableSchema,
) -> Result<(String, Vec<IndexInfo>), String> {
    let (table_sql, table_name_sql, schema_sql) = qualify_sqlite_table(table)?;
    if desired.columns.is_empty() {
        return Err(
            "SQLite table rebuild cannot create a table without represented columns".into(),
        );
    }

    let columns_by_name = desired
        .columns
        .iter()
        .map(|column| (column.name.to_ascii_lowercase(), column))
        .collect::<std::collections::HashMap<_, _>>();
    if columns_by_name.len() != desired.columns.len() {
        return Err("SQLite table snapshot contains case-insensitive duplicate columns".into());
    }

    let primary_keys = desired.effective_primary_keys();
    for primary_key in &primary_keys {
        if !columns_by_name.contains_key(&primary_key.to_ascii_lowercase()) {
            return Err(format!(
                "SQLite primary key references missing column `{primary_key}`"
            ));
        }
    }

    let mut definitions = Vec::new();
    for column in &desired.columns {
        let quoted_name = quote_sqlite_name(&column.name)?;
        let data_type = column.data_type.trim();
        if data_type.is_empty()
            || data_type.chars().any(|ch| {
                ch == ';' || ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
            })
        {
            return Err(format!(
                "SQLite type for `{}` is unsafe or empty",
                column.name
            ));
        }
        if column.comment.is_some() {
            return Err(format!(
                "SQLite column comment on `{}` is not represented",
                column.name
            ));
        }
        let mut definition = format!("{quoted_name} {data_type}");
        validate_sqlite_type_name(data_type)
            .map_err(|error| format!("Column {}: {error}", column.name))?;
        if !column.nullable {
            definition.push_str(" NOT NULL");
        }
        if let Some(default) = &column.default_value {
            validate_sqlite_default(default)
                .map_err(|error| format!("Column `{}` {error}", column.name))?;
            definition.push_str(" DEFAULT ");
            definition.push_str(default.trim());
        }
        if column.is_auto_increment {
            if primary_keys.len() != 1
                || !primary_keys[0].eq_ignore_ascii_case(&column.name)
                || !data_type.eq_ignore_ascii_case("INTEGER")
            {
                return Err(format!(
                    "SQLite AUTOINCREMENT column `{}` must be the single INTEGER primary key",
                    column.name
                ));
            }
            definition.push_str(" PRIMARY KEY AUTOINCREMENT");
        }
        definitions.push(definition);
    }

    if !primary_keys.is_empty()
        && !desired
            .columns
            .iter()
            .any(|column| column.is_auto_increment)
    {
        definitions.push(format!(
            "PRIMARY KEY ({})",
            primary_keys
                .iter()
                .map(|name| quote_sqlite_name(name))
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        ));
    }

    let mut indexes_to_recreate = Vec::new();
    for index in &desired.indexes {
        if index.is_primary {
            continue;
        }
        if index.columns.is_empty() {
            return Err(format!(
                "SQLite index `{}` has no represented columns",
                index.name
            ));
        }
        for column in &index.columns {
            if !columns_by_name.contains_key(&column.to_ascii_lowercase()) {
                return Err(format!(
                    "SQLite index `{}` references missing column `{column}`",
                    index.name
                ));
            }
        }
        if index.is_unique && index.name.starts_with("sqlite_autoindex_") {
            definitions.push(format!(
                "UNIQUE ({})",
                index
                    .columns
                    .iter()
                    .map(|name| quote_sqlite_name(name))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(", ")
            ));
        } else {
            indexes_to_recreate.push(index.clone());
        }
    }

    for foreign_key in &desired.foreign_keys {
        if foreign_key.deferrability != ForeignKeyDeferrability::NotDeferrable {
            return Err(format!(
                "SQLite foreign key `{}` has unknown or deferrable timing semantics",
                foreign_key.name
            ));
        }
        if foreign_key.columns.is_empty()
            || foreign_key.columns.len() != foreign_key.referenced_columns.len()
        {
            return Err(format!(
                "SQLite foreign key `{}` has an invalid column mapping",
                foreign_key.name
            ));
        }
        for column in &foreign_key.columns {
            if !columns_by_name.contains_key(&column.to_ascii_lowercase()) {
                return Err(format!(
                    "SQLite foreign key `{}` references missing column `{column}`",
                    foreign_key.name
                ));
            }
        }
        let referenced_table = quote_sqlite_name(&foreign_key.referenced_table)?;
        let referenced_columns = foreign_key
            .referenced_columns
            .iter()
            .map(|name| quote_sqlite_name(name))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        let columns = foreign_key
            .columns
            .iter()
            .map(|name| quote_sqlite_name(name))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        let action = |raw: &str| -> Result<String, String> {
            let action = raw.trim().to_ascii_uppercase();
            match action.as_str() {
                "" | "NO ACTION" => Ok("NO ACTION".into()),
                "RESTRICT" | "CASCADE" | "SET NULL" | "SET DEFAULT" => Ok(action),
                _ => Err(format!("Unsupported SQLite foreign-key action `{raw}`")),
            }
        };
        definitions.push(format!(
            "FOREIGN KEY ({columns}) REFERENCES {referenced_table} ({referenced_columns}) ON UPDATE {} ON DELETE {}",
            action(&foreign_key.on_update)?,
            action(&foreign_key.on_delete)?,
        ));
    }

    for constraint in &desired.check_constraints {
        let name = quote_sqlite_name(&constraint.name)?;
        validate_sqlite_check(&constraint.expression)?;
        definitions.push(format!(
            "CONSTRAINT {name} CHECK ({})",
            constraint.expression.trim()
        ));
    }

    let create_sql = format!("CREATE TABLE {table_sql} ({})", definitions.join(", "));
    let parsed =
        sqlparser::parser::Parser::parse_sql(&sqlparser::dialect::SQLiteDialect {}, &create_sql)
            .map_err(|error| format!("generated SQLite rebuild DDL is invalid: {error}"))?;
    let Some(sqlparser::ast::Statement::CreateTable(parsed_table)) = parsed.first() else {
        return Err("generated SQLite rebuild DDL is not a single CREATE TABLE statement".into());
    };
    let expected_table_constraints = usize::from(
        !primary_keys.is_empty()
            && !desired
                .columns
                .iter()
                .any(|column| column.is_auto_increment),
    ) + desired
        .indexes
        .iter()
        .filter(|index| {
            index.is_unique && !index.is_primary && index.name.starts_with("sqlite_autoindex_")
        })
        .count()
        + desired.foreign_keys.len()
        + desired.check_constraints.len();
    if parsed.len() != 1
        || parsed_table.columns.len() != desired.columns.len()
        || parsed_table.constraints.len() != expected_table_constraints
        || parsed_table
            .columns
            .iter()
            .zip(&desired.columns)
            .any(|(parsed, expected)| !parsed.name.value.eq_ignore_ascii_case(&expected.name))
    {
        return Err(
            "generated SQLite rebuild DDL does not match the reviewed table structure".into(),
        );
    }

    let _ = (table_name_sql, schema_sql);
    Ok((create_sql, indexes_to_recreate))
}

pub(super) fn render_table_rebuild(
    table: &str,
    desired: &TableSchema,
    current: &TableSchema,
) -> Result<Vec<MigrationStatement>, String> {
    if desired.table_options.migration_blockers.len() > 0
        || current.table_options.migration_blockers.len() > 0
    {
        let blockers = desired
            .table_options
            .migration_blockers
            .iter()
            .chain(&current.table_options.migration_blockers)
            .cloned()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        return Err(format!(
            "SQLite table rebuild is blocked by catalog semantics: {}",
            blockers.join("; ")
        ));
    }

    let (current_sql, current_name_sql, schema_sql) = qualify_sqlite_table(table)?;
    let raw_table_name = validate_migration_identifier(table)?
        .rsplit('.')
        .next()
        .unwrap_or_default();
    if !current
        .table_name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .eq_ignore_ascii_case(raw_table_name)
    {
        return Err("SQLite target table identity changed while preparing the rebuild".into());
    }
    let temporary_name = format!("__datazen_rebuild_{raw_table_name}");
    let temporary_name_sql = quote_sqlite_name(&temporary_name)?;
    let temporary_sql = if schema_sql.is_empty() {
        temporary_name_sql.clone()
    } else {
        format!("{schema_sql}.{temporary_name_sql}")
    };
    let temporary_identity = validate_migration_identifier(table)?
        .rsplit_once('.')
        .map(|(schema, _)| format!("{schema}.{temporary_name}"))
        .unwrap_or_else(|| temporary_name.clone());
    let (create_sql, indexes) = sqlite_table_definition(&temporary_identity, desired)?;

    let current_columns = current
        .columns
        .iter()
        .map(|column| (column.name.to_ascii_lowercase(), column.name.as_str()))
        .collect::<std::collections::HashMap<_, _>>();
    let copy_columns = desired
        .columns
        .iter()
        .filter_map(|column| {
            current_columns
                .get(&column.name.to_ascii_lowercase())
                .map(|current_name| (column.name.as_str(), *current_name))
        })
        .collect::<Vec<_>>();
    if copy_columns.is_empty() {
        return Err(
            "SQLite table rebuild has no shared columns to preserve existing row values".into(),
        );
    }
    let mut destination_columns = copy_columns
        .iter()
        .map(|(desired_name, _)| quote_sqlite_name(desired_name))
        .collect::<Result<Vec<_>, _>>()?;
    let mut source_columns = copy_columns
        .iter()
        .map(|(_, current_name)| quote_sqlite_name(current_name))
        .collect::<Result<Vec<_>, _>>()?;

    let current_rowid_alias = sqlite_rowid_alias(current);
    let desired_rowid_alias = sqlite_rowid_alias(desired);
    let rowid_alias_is_preserved = match (current_rowid_alias, desired_rowid_alias) {
        (Some(current_alias), Some(desired_alias)) => {
            if !current_alias.eq_ignore_ascii_case(desired_alias)
                || !copy_columns.iter().any(|(destination, source)| {
                    destination.eq_ignore_ascii_case(desired_alias)
                        && source.eq_ignore_ascii_case(current_alias)
                })
            {
                return Err(
                        "SQLite table rebuild cannot prove that the INTEGER PRIMARY KEY rowid alias is preserved".into(),
                    );
            }
            true
        }
        (None, Some(_)) => {
            return Err(
                    "SQLite table rebuild cannot add an INTEGER PRIMARY KEY rowid alias without proving the existing rowids match".into(),
                );
        }
        _ => false,
    };

    if !rowid_alias_is_preserved {
        let source_rowid = available_sqlite_rowid_alias(&current.columns).ok_or_else(|| {
                "SQLite table rebuild cannot read the implicit rowid because all aliases are shadowed by columns".to_string()
            })?;
        let destination_rowid =
                available_sqlite_rowid_alias(&desired.columns).ok_or_else(|| {
                    "SQLite table rebuild cannot preserve the implicit rowid because all aliases are shadowed by columns".to_string()
                })?;
        destination_columns.push(quote_sqlite_name(destination_rowid)?);
        source_columns.push(quote_sqlite_name(source_rowid)?);
    }

    let destination_columns = destination_columns.join(", ");
    let source_columns = source_columns.join(", ");

    let rename = if schema_sql.is_empty() {
        format!("ALTER TABLE {temporary_name_sql} RENAME TO {current_name_sql}")
    } else {
        format!("ALTER TABLE {temporary_sql} RENAME TO {current_name_sql}")
    };
    let mut statements = vec![
            MigrationStatement {
                sql: "PRAGMA defer_foreign_keys = ON".into(),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("DEFER SQLite foreign-key checks for {table} rebuild"),
            },
            MigrationStatement {
                sql: create_sql,
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("CREATE replacement table for {table}"),
            },
            MigrationStatement {
                sql: format!(
                    "INSERT INTO {temporary_sql} ({destination_columns}) SELECT {source_columns} FROM {current_sql}"
                ),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("COPY rows from {table} to its replacement"),
            },
            MigrationStatement {
                sql: format!("DROP TABLE {current_sql}"),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("DROP old table {table} inside the rebuild transaction"),
            },
            MigrationStatement {
                sql: rename,
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("RENAME replacement table to {table}"),
            },
        ];

    let has_current_autoincrement = current
        .columns
        .iter()
        .any(|column| column.is_auto_increment);
    let has_desired_autoincrement = desired
        .columns
        .iter()
        .any(|column| column.is_auto_increment);
    let sequence_snapshot = if has_current_autoincrement && has_desired_autoincrement {
        let snapshot_name = format!(
            "__datazen_rebuild_sequence_{}",
            uuid::Uuid::new_v4().simple()
        );
        let snapshot_sql = format!("temp.{}", quote_sqlite_name(&snapshot_name)?);
        let sequence_table = if schema_sql.is_empty() {
            "sqlite_sequence".to_string()
        } else {
            format!("{schema_sql}.sqlite_sequence")
        };
        statements.insert(
            1,
            MigrationStatement {
                sql: format!(
                    "CREATE TEMP TABLE {} AS SELECT seq FROM {} WHERE name = {}",
                    quote_sqlite_name(&snapshot_name)?,
                    sequence_table,
                    quote_sqlite_literal(raw_table_name)
                ),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("CAPTURE AUTOINCREMENT high-water mark for {table}"),
            },
        );
        Some((snapshot_sql, sequence_table))
    } else {
        None
    };

    if let Some((snapshot_sql, sequence_table)) = sequence_snapshot.as_ref() {
        let table_name = quote_sqlite_literal(raw_table_name);
        statements.push(MigrationStatement {
                sql: format!(
                    "UPDATE {sequence_table} SET seq = MAX(seq, (SELECT MAX(seq) FROM {snapshot_sql})) WHERE name = {table_name} AND EXISTS (SELECT 1 FROM {snapshot_sql})"
                ),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("RESTORE AUTOINCREMENT high-water mark for {table}"),
            });
        statements.push(MigrationStatement {
                sql: format!(
                    "INSERT INTO {sequence_table} (name, seq) SELECT {table_name}, MAX(seq) FROM {snapshot_sql} WHERE NOT EXISTS (SELECT 1 FROM {sequence_table} WHERE name = {table_name}) HAVING MAX(seq) IS NOT NULL"
                ),
                risk: MigrationRisk::Rewrite,
                rollback_sql: None,
                summary: format!("RESTORE empty-table AUTOINCREMENT high-water mark for {table}"),
            });
        statements.push(MigrationStatement {
            sql: format!("DROP TABLE {snapshot_sql}"),
            risk: MigrationRisk::Rewrite,
            rollback_sql: None,
            summary: format!("DROP temporary AUTOINCREMENT state for {table}"),
        });
    }

    for index in indexes {
        let index_name = quote_sqlite_name(&index.name)?;
        let index_name = if schema_sql.is_empty() {
            index_name
        } else {
            format!("{schema_sql}.{index_name}")
        };
        let index_columns = index
            .columns
            .iter()
            .map(|column| quote_sqlite_name(column))
            .collect::<Result<Vec<_>, _>>()?
            .join(", ");
        statements.push(MigrationStatement {
            sql: format!(
                "CREATE {}INDEX {index_name} ON {current_name_sql} ({index_columns})",
                if index.is_unique { "UNIQUE " } else { "" },
            ),
            risk: MigrationRisk::Rewrite,
            rollback_sql: None,
            summary: format!("RECREATE index {} on {table}", index.name),
        });
    }

    Ok(statements)
}
