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

fn format_mysql_index_col(s: &str, qi: &impl Fn(&str) -> String) -> String {
    let trimmed = s.trim();
    if let Some(paren_pos) = trimmed.find('(') {
        if trimmed.ends_with(')') {
            let col = trimmed[..paren_pos].trim().trim_matches('`');
            let len_part = &trimmed[paren_pos..];
            return format!("{}{}", qi(col), len_part);
        }
    }
    qi(trimmed)
}

fn mysql_index_kind(index: &IndexInfo) -> Result<(&'static str, &'static str), String> {
    match index.index_type.trim().to_ascii_uppercase().as_str() {
        "BTREE" => Ok(("", " USING BTREE")),
        "HASH" => Ok(("", " USING HASH")),
        "FULLTEXT" if !index.is_unique => Ok(("FULLTEXT ", "")),
        "RTREE" if !index.is_unique => Ok(("SPATIAL ", "")),
        "FULLTEXT" | "RTREE" => Err(format!(
            "MySQL cannot create a unique {} index",
            index.index_type
        )),
        "" => Err("MySQL index type is missing; cannot safely recreate the index".into()),
        _ => Err(format!(
            "MySQL index type '{}' cannot be safely recreated",
            index.index_type
        )),
    }
}

fn mysql_fk_action(raw: &str, clause: &str) -> Result<String, String> {
    let action = raw.trim().to_ascii_uppercase();
    if action.is_empty() || action == "NO ACTION" {
        return Ok(String::new());
    }
    if matches!(
        action.as_str(),
        "CASCADE" | "RESTRICT" | "SET NULL" | "SET DEFAULT"
    ) {
        return Ok(format!(" {clause} {action}"));
    }
    Err(format!("MySQL cannot represent foreign-key action '{raw}'"))
}

fn mysql_view_ident(view: &MigrationView) -> String {
    let quote = |value: &str| format!("`{}`", value.replace('`', "``"));
    match view.schema.as_deref().filter(|schema| !schema.is_empty()) {
        Some(schema) => format!("{}.{}", quote(schema), quote(&view.name)),
        None => quote(&view.name),
    }
}

fn mysql_routine_ident(routine: &MigrationRoutine) -> Result<String, String> {
    if !matches!(routine.kind, ObjectKind::Function | ObjectKind::Procedure) {
        return Err("MySQL routine operation requires a function or procedure".into());
    }
    validate_migration_identifier(&routine.name)?;
    let quote = |value: &str| format!("`{}`", value.replace('`', "``"));
    Ok(
        match routine
            .schema
            .as_deref()
            .filter(|schema| !schema.is_empty())
        {
            Some(schema) => format!("{}.{}", quote(schema), quote(&routine.name)),
            None => quote(&routine.name),
        },
    )
}

fn mysql_trigger_ident(trigger: &MigrationTrigger) -> Result<String, String> {
    validate_migration_identifier(&trigger.name)?;
    validate_migration_identifier(&trigger.target_name)?;
    let quote = |value: &str| format!("`{}`", value.replace('`', "``"));
    Ok(
        match trigger
            .schema
            .as_deref()
            .filter(|schema| !schema.is_empty())
        {
            Some(schema) => format!("{}.{}", quote(schema), quote(&trigger.name)),
            None => quote(&trigger.name),
        },
    )
}

fn mysql_validate_object_ddl(
    definition: &str,
    kind: ObjectKind,
    name: &str,
    signature: Option<&str>,
) -> Result<String, String> {
    validate_object_definition_with_identity(definition, kind, name, signature)?;
    Ok(definition.trim().to_owned())
}

fn mysql_option_token(value: &str, field: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty()
        || value
            .chars()
            .any(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    {
        return Err(format!("MySQL table {field} is not a safe identifier"));
    }
    Ok(value.to_string())
}

fn mysql_table_comment(value: Option<&String>) -> String {
    value
        .map(|value| format!("'{}'", value.replace('\'', "''")))
        .unwrap_or_else(|| "''".into())
}

fn render_mysql_create_table(
    table: &str,
    columns: &[MigrationColumn],
    primary_keys: &[String],
    options: &TableOptions,
) -> Result<MigrationStatement, String> {
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
    let cols = columns
        .iter()
        .map(|column| format_mysql_column_def(column, &qi))
        .collect::<Vec<_>>();
    let pk = if primary_keys.is_empty() {
        String::new()
    } else {
        format!(
            ", PRIMARY KEY ({})",
            primary_keys
                .iter()
                .map(|column| qi(column))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    let mut suffixes = Vec::new();
    if let Some(engine) = options.engine.as_deref() {
        suffixes.push(format!(
            "ENGINE = {}",
            mysql_option_token(engine, "engine")?
        ));
    }
    if let Some(charset) = options.charset.as_deref() {
        suffixes.push(format!(
            "DEFAULT CHARACTER SET = {}",
            mysql_option_token(charset, "charset")?
        ));
    }
    if let Some(collation) = options.collation.as_deref() {
        suffixes.push(format!(
            "COLLATE = {}",
            mysql_option_token(collation, "collation")?
        ));
    }
    if options.comment.is_some() {
        suffixes.push(format!(
            "COMMENT = {}",
            mysql_table_comment(options.comment.as_ref())
        ));
    }
    let suffix = if suffixes.is_empty() {
        String::new()
    } else {
        format!(" {}", suffixes.join(" "))
    };
    Ok(MigrationStatement {
        sql: format!(
            "CREATE TABLE {} ({}{}){}",
            qi(table),
            cols.join(", "),
            pk,
            suffix
        ),
        risk: MigrationRisk::Additive,
        rollback_sql: Some(format!("DROP TABLE {}", qi(table))),
        summary: format!("CREATE TABLE {}", table),
    })
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
            } => render_mysql_create_table(table, columns, primary_keys, &TableOptions::default()),
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
            MigrationOperation::CreateIndex { table, index } => {
                let (kind, method) = mysql_index_kind(index)?;
                let uniqueness = if index.is_unique { "UNIQUE " } else { kind };
                Ok(MigrationStatement {
                    sql: format!(
                        "CREATE {}INDEX {}{} ON {} ({})",
                        uniqueness,
                        qi(&index.name),
                        method,
                        qi(table),
                        index
                            .columns
                            .iter()
                            .map(|c| format_mysql_index_col(c, &qi))
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP INDEX {} ON {}", qi(&index.name), qi(table))),
                    summary: format!("CREATE INDEX {}.{}", table, index.name),
                })
            }
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
            MigrationOperation::SetTableOptions { table, from, to } => {
                validate_migration_identifier(table)?;
                let mut clauses = Vec::new();
                if from.engine != to.engine {
                    let Some(engine) = to.engine.as_deref() else {
                        return Err("MySQL table engine removal is not representable".into());
                    };
                    clauses.push(format!(
                        "ENGINE = {}",
                        mysql_option_token(engine, "engine")?
                    ));
                }
                if from.charset != to.charset {
                    let Some(charset) = to.charset.as_deref() else {
                        return Err("MySQL table charset removal is not representable".into());
                    };
                    clauses.push(format!(
                        "DEFAULT CHARACTER SET = {}",
                        mysql_option_token(charset, "charset")?
                    ));
                }
                if from.comment != to.comment {
                    clauses.push(format!(
                        "COMMENT = {}",
                        mysql_table_comment(to.comment.as_ref())
                    ));
                }
                if clauses.is_empty() {
                    return Err("table options are unchanged".into());
                }
                let mut rollback_clauses = Vec::new();
                if from.engine != to.engine {
                    let Some(engine) = from.engine.as_deref() else {
                        return Err("MySQL table engine rollback is unavailable".into());
                    };
                    rollback_clauses.push(format!(
                        "ENGINE = {}",
                        mysql_option_token(engine, "engine")?
                    ));
                }
                if from.charset != to.charset {
                    let Some(charset) = from.charset.as_deref() else {
                        return Err("MySQL table charset rollback is unavailable".into());
                    };
                    rollback_clauses.push(format!(
                        "DEFAULT CHARACTER SET = {}",
                        mysql_option_token(charset, "charset")?
                    ));
                }
                if from.comment != to.comment {
                    rollback_clauses.push(format!(
                        "COMMENT = {}",
                        mysql_table_comment(from.comment.as_ref())
                    ));
                }
                Ok(MigrationStatement {
                    sql: format!("ALTER TABLE {} {}", qi(table), clauses.join(", ")),
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {} {}",
                        qi(table),
                        rollback_clauses.join(", ")
                    )),
                    summary: format!("ALTER TABLE OPTIONS {}", table),
                })
            }
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
            MigrationOperation::AddForeignKey { table, foreign_key } => {
                if foreign_key.name.trim().is_empty()
                    || foreign_key.columns.is_empty()
                    || foreign_key.columns.len() != foreign_key.referenced_columns.len()
                {
                    return Err("foreign key must have a name and matching column lists".into());
                }
                if foreign_key.deferrability != ForeignKeyDeferrability::NotDeferrable {
                    return Err(
                        "MySQL cannot represent unknown or deferrable foreign key semantics".into(),
                    );
                }
                let on_update = mysql_fk_action(&foreign_key.on_update, "ON UPDATE")?;
                let on_delete = mysql_fk_action(&foreign_key.on_delete, "ON DELETE")?;
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}){}{}",
                        qi(table),
                        qi(&foreign_key.name),
                        foreign_key
                            .columns
                            .iter()
                            .map(|column| qi(column))
                            .collect::<Vec<_>>()
                            .join(", "),
                        qi(&foreign_key.referenced_table),
                        foreign_key
                            .referenced_columns
                            .iter()
                            .map(|column| qi(column))
                            .collect::<Vec<_>>()
                            .join(", "),
                        on_update,
                        on_delete,
                    ),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {} DROP FOREIGN KEY {}",
                        qi(table),
                        qi(&foreign_key.name)
                    )),
                    summary: format!("ADD FOREIGN KEY {}.{}", table, foreign_key.name),
                })
            }
            MigrationOperation::DropForeignKey { table, foreign_key } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} DROP FOREIGN KEY {}",
                    qi(table),
                    qi(&foreign_key.name)
                ),
                risk: MigrationRisk::Destructive,
                rollback_sql: None,
                summary: format!("DROP FOREIGN KEY {}.{}", table, foreign_key.name),
            }),
            MigrationOperation::AddCheckConstraint { table, constraint } => {
                validate_migration_identifier(table)?;
                validate_migration_identifier(&constraint.name)?;
                validate_check_expression(&constraint.expression)?;
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({})",
                        qi(table),
                        qi(&constraint.name),
                        constraint.expression.trim()
                    ),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {} DROP CHECK {}",
                        qi(table),
                        qi(&constraint.name)
                    )),
                    summary: format!("ADD CHECK {}.{}", table, constraint.name),
                })
            }
            MigrationOperation::DropCheckConstraint { table, constraint } => {
                validate_migration_identifier(table)?;
                validate_migration_identifier(&constraint.name)?;
                validate_check_expression(&constraint.expression)?;
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {} DROP CHECK {}",
                        qi(table),
                        qi(&constraint.name)
                    ),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} CHECK ({})",
                        qi(table),
                        qi(&constraint.name),
                        constraint.expression.trim()
                    )),
                    summary: format!("DROP CHECK {}.{}", table, constraint.name),
                })
            }
            MigrationOperation::CreateView { view } => {
                validate_view_definition(&view.definition)?;
                let ident = mysql_view_ident(view);
                Ok(MigrationStatement {
                    sql: format!("CREATE VIEW {ident} AS {}", view.definition.trim()),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP VIEW {ident}")),
                    summary: format!("CREATE VIEW {}", view.name),
                })
            }
            MigrationOperation::ReplaceView { current, desired } => {
                validate_view_definition(&desired.definition)?;
                validate_view_definition(&current.definition)?;
                let ident = mysql_view_ident(desired);
                Ok(MigrationStatement {
                    sql: format!(
                        "CREATE OR REPLACE VIEW {ident} AS {}",
                        desired.definition.trim()
                    ),
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: Some(format!(
                        "CREATE OR REPLACE VIEW {ident} AS {}",
                        current.definition.trim()
                    )),
                    summary: format!("REPLACE VIEW {}", desired.name),
                })
            }
            MigrationOperation::DropView { view } => {
                validate_view_definition(&view.definition)?;
                let ident = mysql_view_ident(view);
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
            MigrationOperation::CreateRoutine { routine } => {
                let definition = mysql_validate_object_ddl(
                    &routine.definition,
                    routine.kind,
                    &routine.name,
                    routine.signature.as_deref(),
                )?;
                let ident = mysql_routine_ident(routine)?;
                Ok(MigrationStatement {
                    sql: definition,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "DROP {} {ident}",
                        routine.kind.as_str().to_ascii_uppercase()
                    )),
                    summary: format!(
                        "CREATE {} {}",
                        routine.kind.as_str().to_ascii_uppercase(),
                        routine.name
                    ),
                })
            }
            MigrationOperation::ReplaceRoutine { current, desired } => {
                if current.kind != desired.kind
                    || current.schema != desired.schema
                    || current.name != desired.name
                {
                    return Err("routine replacement identities must match".into());
                }
                let desired_definition = mysql_validate_object_ddl(
                    &desired.definition,
                    desired.kind,
                    &desired.name,
                    desired.signature.as_deref(),
                )?;
                let current_definition = mysql_validate_object_ddl(
                    &current.definition,
                    current.kind,
                    &current.name,
                    current.signature.as_deref(),
                )?;
                let ident = mysql_routine_ident(desired)?;
                Ok(MigrationStatement {
                    sql: format!(
                        "DROP {} {ident}; {desired_definition}",
                        desired.kind.as_str().to_ascii_uppercase()
                    ),
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: Some(format!(
                        "DROP {} {ident}; {current_definition}",
                        current.kind.as_str().to_ascii_uppercase()
                    )),
                    summary: format!(
                        "REPLACE {} {}",
                        desired.kind.as_str().to_ascii_uppercase(),
                        desired.name
                    ),
                })
            }
            MigrationOperation::DropRoutine { routine } => {
                let definition = mysql_validate_object_ddl(
                    &routine.definition,
                    routine.kind,
                    &routine.name,
                    routine.signature.as_deref(),
                )?;
                let ident = mysql_routine_ident(routine)?;
                Ok(MigrationStatement {
                    sql: format!(
                        "DROP {} {ident}",
                        routine.kind.as_str().to_ascii_uppercase()
                    ),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(definition),
                    summary: format!(
                        "DROP {} {}",
                        routine.kind.as_str().to_ascii_uppercase(),
                        routine.name
                    ),
                })
            }
            MigrationOperation::CreateTrigger { trigger } => {
                let definition = mysql_validate_object_ddl(
                    &trigger.definition,
                    ObjectKind::Trigger,
                    &trigger.name,
                    None,
                )?;
                let ident = mysql_trigger_ident(trigger)?;
                Ok(MigrationStatement {
                    sql: definition,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP TRIGGER {ident}")),
                    summary: format!("CREATE TRIGGER {}", trigger.name),
                })
            }
            MigrationOperation::ReplaceTrigger { current, desired } => {
                if current.schema != desired.schema
                    || current.name != desired.name
                    || current.target_schema != desired.target_schema
                    || current.target_name != desired.target_name
                {
                    return Err("trigger replacement identities must match".into());
                }
                let desired_definition = mysql_validate_object_ddl(
                    &desired.definition,
                    ObjectKind::Trigger,
                    &desired.name,
                    None,
                )?;
                let current_definition = mysql_validate_object_ddl(
                    &current.definition,
                    ObjectKind::Trigger,
                    &current.name,
                    None,
                )?;
                let ident = mysql_trigger_ident(desired)?;
                Ok(MigrationStatement {
                    sql: format!("DROP TRIGGER {ident}; {desired_definition}"),
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: Some(format!("DROP TRIGGER {ident}; {current_definition}")),
                    summary: format!("REPLACE TRIGGER {}", desired.name),
                })
            }
            MigrationOperation::DropTrigger { trigger } => {
                let definition = mysql_validate_object_ddl(
                    &trigger.definition,
                    ObjectKind::Trigger,
                    &trigger.name,
                    None,
                )?;
                let ident = mysql_trigger_ident(trigger)?;
                Ok(MigrationStatement {
                    sql: format!("DROP TRIGGER {ident}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(definition),
                    summary: format!("DROP TRIGGER {}", trigger.name),
                })
            }
            MigrationOperation::CreateSequence { .. }
            | MigrationOperation::ReplaceSequence { .. }
            | MigrationOperation::DropSequence { .. } => {
                Err("MySQL sequence migration is unsupported".into())
            }
            MigrationOperation::CreateType { .. }
            | MigrationOperation::ReplaceType { .. }
            | MigrationOperation::DropType { .. } => {
                Err("MySQL user-defined type migration is unsupported".into())
            }
        }
    }

    fn map_schema_object_scope(
        &self,
        kind: ObjectKind,
        source_scope: &str,
        target_scope: &str,
        definition: &str,
        dependencies: &[SchemaObjectScopeDependency],
    ) -> Result<Option<SchemaObjectScopeMapping>, String> {
        crate::schema_scope_mapping::map_view_scope(
            kind,
            source_scope,
            target_scope,
            definition,
            dependencies,
        )
    }

    fn render_create_table_with_options(
        &self,
        operation: &MigrationOperation,
        table_options: &TableOptions,
    ) -> Result<MigrationStatement, String> {
        match operation {
            MigrationOperation::CreateTable {
                table,
                columns,
                primary_keys,
            } => render_mysql_create_table(table, columns, primary_keys, table_options),
            _ => self.render(operation),
        }
    }
}

pub struct MysqlMigrationCapabilities;
impl MigrationCapabilities for MysqlMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        match operation {
            MigrationOperation::SetNullable { .. } => false,
            MigrationOperation::SetAutoIncrement { to, .. } if !*to => false,
            MigrationOperation::SetTableOptions { .. } => true,
            MigrationOperation::CreateTable { .. }
            | MigrationOperation::DropTable { .. }
            | MigrationOperation::AddColumn { .. }
            | MigrationOperation::DropColumn { .. }
            | MigrationOperation::AlterColumnType { .. }
            | MigrationOperation::SetDefault { .. }
            | MigrationOperation::SetComment { .. }
            | MigrationOperation::SetAutoIncrement { .. }
            | MigrationOperation::AddPrimaryKey { .. }
            | MigrationOperation::DropPrimaryKey { .. }
            | MigrationOperation::CreateIndex { .. }
            | MigrationOperation::DropIndex { .. }
            | MigrationOperation::AddForeignKey { .. }
            | MigrationOperation::DropForeignKey { .. }
            | MigrationOperation::AddCheckConstraint { .. }
            | MigrationOperation::DropCheckConstraint { .. }
            | MigrationOperation::CreateView { .. }
            | MigrationOperation::ReplaceView { .. }
            | MigrationOperation::DropView { .. } => true,
            MigrationOperation::CreateRoutine { routine }
            | MigrationOperation::ReplaceRoutine {
                desired: routine, ..
            }
            | MigrationOperation::DropRoutine { routine } => {
                matches!(routine.kind, ObjectKind::Function | ObjectKind::Procedure)
            }
            MigrationOperation::CreateTrigger { .. }
            | MigrationOperation::ReplaceTrigger { .. }
            | MigrationOperation::DropTrigger { .. } => true,
            MigrationOperation::CreateSequence { .. }
            | MigrationOperation::ReplaceSequence { .. }
            | MigrationOperation::DropSequence { .. }
            | MigrationOperation::CreateType { .. }
            | MigrationOperation::ReplaceType { .. }
            | MigrationOperation::DropType { .. } => false,
        }
    }
    fn requires_table_rebuild(&self, operation: &MigrationOperation) -> bool {
        matches!(
            operation,
            MigrationOperation::SetAutoIncrement { .. }
                | MigrationOperation::AlterColumnType { .. }
                | MigrationOperation::SetComment { .. }
                | MigrationOperation::SetTableOptions { .. }
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
    fn create_table_preserves_reviewed_source_table_options() {
        let operation = MigrationOperation::CreateTable {
            table: "items".into(),
            columns: vec![col("id", "INT")],
            primary_keys: vec!["id".into()],
        };
        let statement = MysqlMigrationRenderer
            .render_create_table_with_options(
                &operation,
                &TableOptions {
                    engine: Some("InnoDB".into()),
                    charset: Some("utf8mb4".into()),
                    collation: Some("utf8mb4_0900_ai_ci".into()),
                    comment: Some("catalog items".into()),
                    ..TableOptions::default()
                },
            )
            .unwrap();

        assert!(statement.sql.contains("ENGINE = InnoDB"));
        assert!(statement.sql.contains("DEFAULT CHARACTER SET = utf8mb4"));
        assert!(statement.sql.contains("COLLATE = utf8mb4_0900_ai_ci"));
        assert!(statement.sql.contains("COMMENT = 'catalog items'"));
        assert_eq!(statement.risk, MigrationRisk::Additive);
    }

    #[test]
    fn table_options_render_with_safe_tokens_and_rollback() {
        let statement = MysqlMigrationRenderer
            .render(&MigrationOperation::SetTableOptions {
                table: "orders".into(),
                from: TableOptions {
                    engine: Some("InnoDB".into()),
                    charset: Some("latin1".into()),
                    comment: Some("old note".into()),
                    ..TableOptions::default()
                },
                to: TableOptions {
                    engine: Some("InnoDB".into()),
                    charset: Some("utf8mb4".into()),
                    comment: Some("owner's orders".into()),
                    ..TableOptions::default()
                },
            })
            .unwrap();
        assert_eq!(
            statement.sql,
            "ALTER TABLE `orders` DEFAULT CHARACTER SET = utf8mb4, COMMENT = 'owner''s orders'"
        );
        assert_eq!(
            statement.rollback_sql.as_deref(),
            Some("ALTER TABLE `orders` DEFAULT CHARACTER SET = latin1, COMMENT = 'old note'")
        );
        assert!(
            MysqlMigrationCapabilities.supports(&MigrationOperation::SetTableOptions {
                table: "orders".into(),
                from: TableOptions::default(),
                to: TableOptions {
                    engine: Some("InnoDB".into()),
                    charset: None,
                    comment: None,
                    ..TableOptions::default()
                },
            })
        );
    }

    #[test]
    fn table_options_fail_closed_for_unsafe_engine_or_unknown_charset() {
        let unsafe_engine = MigrationOperation::SetTableOptions {
            table: "orders".into(),
            from: TableOptions::default(),
            to: TableOptions {
                engine: Some("InnoDB; DROP TABLE users".into()),
                ..TableOptions::default()
            },
        };
        assert!(MysqlMigrationRenderer.render(&unsafe_engine).is_err());

        let unknown_charset = MigrationOperation::SetTableOptions {
            table: "orders".into(),
            from: TableOptions {
                charset: Some("utf8mb4".into()),
                ..TableOptions::default()
            },
            to: TableOptions::default(),
        };
        assert!(MysqlMigrationRenderer.render(&unknown_charset).is_err());
    }

    #[test]
    fn renders_named_check_constraint_and_rollback() {
        let statement = MysqlMigrationRenderer
            .render(&MigrationOperation::AddCheckConstraint {
                table: "users".into(),
                constraint: CheckConstraint {
                    name: "users_age_check".into(),
                    expression: "age >= 0".into(),
                },
            })
            .unwrap();
        assert_eq!(
            statement.sql,
            "ALTER TABLE `users` ADD CONSTRAINT `users_age_check` CHECK (age >= 0)"
        );
        assert_eq!(
            statement.rollback_sql.as_deref(),
            Some("ALTER TABLE `users` DROP CHECK `users_age_check`")
        );
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
    fn drop_table_is_quoted_destructive_and_has_no_rollback() {
        let stmt = MysqlMigrationRenderer
            .render(&MigrationOperation::DropTable {
                table: "archive.events".into(),
            })
            .unwrap();
        assert_eq!(stmt.sql, "DROP TABLE `archive`.`events`");
        assert_eq!(stmt.risk, MigrationRisk::Destructive);
        assert!(stmt.rollback_sql.is_none());
        assert!(
            MysqlMigrationCapabilities.supports(&MigrationOperation::DropTable {
                table: "archive.events".into(),
            })
        );
        assert!(!stmt.sql.contains("CASCADE"));
    }

    #[test]
    fn test_tester_drop_table_rejects_empty_identifier() {
        assert!(MysqlMigrationRenderer
            .render(&MigrationOperation::DropTable {
                table: String::new()
            })
            .is_err());
    }

    #[test]
    fn drop_table_rejects_blank_control_and_invalid_qualified_identifiers() {
        for table in [" ", "audit\nevents", "audit..events", "audit. events"] {
            assert!(
                MysqlMigrationRenderer
                    .render(&MigrationOperation::DropTable {
                        table: table.into()
                    })
                    .is_err(),
                "{table:?}"
            );
        }
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

    #[test]
    fn create_index_renders_prefix_length_properly() {
        let op = MigrationOperation::CreateIndex {
            table: "demo_customers".into(),
            index: IndexInfo {
                name: "idx_demo_customers_region".into(),
                columns: vec!["region(255)".into()],
                is_unique: false,
                is_primary: false,
                index_type: "BTREE".into(),
            },
        };
        let stmt = MysqlMigrationRenderer.render(&op).unwrap();
        assert_eq!(
            stmt.sql,
            "CREATE INDEX `idx_demo_customers_region` USING BTREE ON `demo_customers` (`region`(255))"
        );

        let unique_op = MigrationOperation::CreateIndex {
            table: "demo_customers".into(),
            index: IndexInfo {
                name: "uq_demo_customers_name".into(),
                columns: vec!["name(255)".into()],
                is_unique: true,
                is_primary: false,
                index_type: "BTREE".into(),
            },
        };
        let unique_stmt = MysqlMigrationRenderer.render(&unique_op).unwrap();
        assert_eq!(
            unique_stmt.sql,
            "CREATE UNIQUE INDEX `uq_demo_customers_name` USING BTREE ON `demo_customers` (`name`(255))"
        );
    }

    #[test]
    fn create_index_preserves_hash_fulltext_and_spatial_kinds() {
        let render = |index_type: &str, is_unique| {
            MysqlMigrationRenderer.render(&MigrationOperation::CreateIndex {
                table: "articles".into(),
                index: IndexInfo {
                    name: "idx_articles_body".into(),
                    columns: vec!["body".into()],
                    is_unique,
                    is_primary: false,
                    index_type: index_type.into(),
                },
            })
        };

        assert_eq!(
            render("hash", false).unwrap().sql,
            "CREATE INDEX `idx_articles_body` USING HASH ON `articles` (`body`)"
        );
        assert_eq!(
            render("FULLTEXT", false).unwrap().sql,
            "CREATE FULLTEXT INDEX `idx_articles_body` ON `articles` (`body`)"
        );
        assert_eq!(
            render("RTREE", false).unwrap().sql,
            "CREATE SPATIAL INDEX `idx_articles_body` ON `articles` (`body`)"
        );
        assert!(render("FULLTEXT", true).is_err());
        assert!(render("RTREE", true).is_err());
        assert!(render("unknown", false).is_err());
        assert!(render("", false).is_err());
    }

    #[test]
    fn renders_foreign_key_with_actions_and_rollback() {
        let op = MigrationOperation::AddForeignKey {
            table: "orders".into(),
            foreign_key: ForeignKeyInfo {
                name: "orders_user_id_fk".into(),
                columns: vec!["user_id".into()],
                referenced_table: "users".into(),
                referenced_columns: vec!["id".into()],
                on_update: "CASCADE".into(),
                on_delete: "RESTRICT".into(),
                deferrability: ForeignKeyDeferrability::NotDeferrable,
            },
        };
        let stmt = MysqlMigrationRenderer.render(&op).unwrap();
        assert_eq!(
            stmt.sql,
            "ALTER TABLE `orders` ADD CONSTRAINT `orders_user_id_fk` FOREIGN KEY (`user_id`) REFERENCES `users` (`id`) ON UPDATE CASCADE ON DELETE RESTRICT"
        );
        assert_eq!(
            stmt.rollback_sql.as_deref(),
            Some("ALTER TABLE `orders` DROP FOREIGN KEY `orders_user_id_fk`")
        );
        assert!(MysqlMigrationCapabilities.supports(&op));
    }

    #[test]
    fn rejects_unknown_or_deferrable_foreign_key_semantics() {
        for deferrability in [
            ForeignKeyDeferrability::Unknown,
            ForeignKeyDeferrability::DeferrableInitiallyImmediate,
            ForeignKeyDeferrability::DeferrableInitiallyDeferred,
        ] {
            let op = MigrationOperation::AddForeignKey {
                table: "orders".into(),
                foreign_key: ForeignKeyInfo {
                    name: "orders_user_id_fk".into(),
                    columns: vec!["user_id".into()],
                    referenced_table: "users".into(),
                    referenced_columns: vec!["id".into()],
                    on_update: "NO ACTION".into(),
                    on_delete: "NO ACTION".into(),
                    deferrability,
                },
            };
            assert!(MysqlMigrationRenderer.render(&op).is_err());
        }
    }

    #[test]
    fn renders_qualified_view_with_mysql_identifier_quoting() {
        let view = MigrationView {
            schema: Some("reporting`archive".into()),
            name: "active`users".into(),
            definition: "SELECT id FROM users".into(),
        };
        let stmt = MysqlMigrationRenderer
            .render(&MigrationOperation::CreateView { view: view.clone() })
            .unwrap();
        assert_eq!(
            stmt.sql,
            "CREATE VIEW `reporting``archive`.`active``users` AS SELECT id FROM users"
        );
        assert_eq!(
            stmt.rollback_sql.as_deref(),
            Some("DROP VIEW `reporting``archive`.`active``users`")
        );
        let replacement = MysqlMigrationRenderer
            .render(&MigrationOperation::ReplaceView {
                current: view.clone(),
                desired: MigrationView {
                    definition: "SELECT id, email FROM users".into(),
                    ..view
                },
            })
            .unwrap();
        assert!(replacement.sql.starts_with("CREATE OR REPLACE VIEW"));
        assert!(replacement.rollback_sql.is_some());
    }

    fn mysql_procedure(definition: &str) -> MigrationRoutine {
        MigrationRoutine {
            kind: ObjectKind::Procedure,
            schema: Some("app".into()),
            name: "rebuild_cache".into(),
            signature: None,
            definition: definition.into(),
        }
    }

    #[test]
    fn renders_mysql_routine_create_replace_drop() {
        let current = mysql_procedure(
            "CREATE DEFINER=`root`@`localhost` PROCEDURE `app`.`rebuild_cache`() BEGIN SELECT 1; END",
        );
        let desired = mysql_procedure(
            "CREATE DEFINER=`root`@`localhost` PROCEDURE `app`.`rebuild_cache`() BEGIN SELECT 2; END",
        );
        let create = MysqlMigrationRenderer
            .render(&MigrationOperation::CreateRoutine {
                routine: desired.clone(),
            })
            .unwrap();
        assert!(create.sql.starts_with("CREATE DEFINER"));
        assert_eq!(
            create.rollback_sql.as_deref(),
            Some("DROP PROCEDURE `app`.`rebuild_cache`")
        );
        let replace = MysqlMigrationRenderer
            .render(&MigrationOperation::ReplaceRoutine {
                current: current.clone(),
                desired: desired.clone(),
            })
            .unwrap();
        assert!(replace
            .sql
            .contains("DROP PROCEDURE `app`.`rebuild_cache`; CREATE"));
        assert!(replace
            .rollback_sql
            .is_some_and(|sql| sql.contains("SELECT 1")));
        let drop = MysqlMigrationRenderer
            .render(&MigrationOperation::DropRoutine { routine: current })
            .unwrap();
        assert_eq!(drop.risk, MigrationRisk::Destructive);
        assert!(drop.rollback_sql.is_some());
    }

    #[test]
    fn renders_mysql_trigger_and_rejects_bad_ddl() {
        let trigger = MigrationTrigger {
            schema: Some("app".into()),
            name: "audit_insert".into(),
            target_schema: Some("app".into()),
            target_name: "orders".into(),
            definition: "CREATE DEFINER=`root`@`localhost` TRIGGER `app`.`audit_insert` AFTER INSERT ON `app`.`orders` FOR EACH ROW SET @audit = 1".into(),
        };
        let create = MysqlMigrationRenderer
            .render(&MigrationOperation::CreateTrigger {
                trigger: trigger.clone(),
            })
            .unwrap();
        assert!(create.sql.starts_with("CREATE DEFINER"));
        assert_eq!(
            create.rollback_sql.as_deref(),
            Some("DROP TRIGGER `app`.`audit_insert`")
        );
        let bad = MigrationRoutine {
            definition: "CREATE VIEW wrong AS SELECT 1".into(),
            ..mysql_procedure("CREATE PROCEDURE rebuild_cache() BEGIN SELECT 1; END")
        };
        assert!(MysqlMigrationRenderer
            .render(&MigrationOperation::CreateRoutine { routine: bad })
            .is_err());
    }
}
