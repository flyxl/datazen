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

fn pg_comment_on_column(
    table: &str,
    column: &str,
    comment: &str,
    qi: &impl Fn(&str) -> String,
) -> String {
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

fn pg_fk_action(raw: &str, clause: &str) -> Result<String, String> {
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
    Err(format!(
        "PostgreSQL cannot represent foreign-key action '{raw}'"
    ))
}

fn pg_fk_deferrability(value: ForeignKeyDeferrability) -> Result<&'static str, String> {
    match value {
        ForeignKeyDeferrability::NotDeferrable => Ok(" NOT DEFERRABLE"),
        ForeignKeyDeferrability::DeferrableInitiallyImmediate => {
            Ok(" DEFERRABLE INITIALLY IMMEDIATE")
        }
        ForeignKeyDeferrability::DeferrableInitiallyDeferred => {
            Ok(" DEFERRABLE INITIALLY DEFERRED")
        }
        ForeignKeyDeferrability::Unknown => {
            Err("foreign key deferrability is unknown; cannot safely create the constraint".into())
        }
    }
}

fn pg_view_ident(view: &MigrationView) -> String {
    let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
    match view.schema.as_deref().filter(|schema| !schema.is_empty()) {
        Some(schema) => format!("{}.{}", quote(schema), quote(&view.name)),
        None => quote(&view.name),
    }
}

fn pg_object_schema(schema: Option<&str>) -> Result<Option<String>, String> {
    schema
        .filter(|value| !value.is_empty())
        .map(|value| {
            validate_migration_identifier(value)?;
            Ok(format!("\"{}\"", value.replace('"', "\"\"")))
        })
        .transpose()
}

fn pg_routine_ident(routine: &MigrationRoutine) -> Result<String, String> {
    if !matches!(routine.kind, ObjectKind::Function | ObjectKind::Procedure) {
        return Err("PostgreSQL routine operation requires a function or procedure".into());
    }
    validate_migration_identifier(&routine.name)?;
    let schema = pg_object_schema(routine.schema.as_deref())?;
    let name = format!("\"{}\"", routine.name.replace('"', "\"\""));
    let signature = routine
        .signature
        .as_deref()
        .ok_or("PostgreSQL routine identity arguments are required")?;
    if signature
        .chars()
        .any(|ch| ch == '\0' || (ch.is_control() && !matches!(ch, '\n' | '\r' | '\t')) || ch == ';')
    {
        return Err("PostgreSQL routine signature contains unsafe characters".into());
    }
    Ok(format!(
        "{}{}({})",
        schema.map(|value| format!("{value}.")).unwrap_or_default(),
        name,
        signature
    ))
}

fn pg_trigger_ident(trigger: &MigrationTrigger) -> Result<(String, String), String> {
    validate_migration_identifier(&trigger.name)?;
    validate_migration_identifier(&trigger.target_name)?;
    let trigger_schema = pg_object_schema(trigger.schema.as_deref())?;
    let target_schema = pg_object_schema(
        trigger
            .target_schema
            .as_deref()
            .or(trigger.schema.as_deref()),
    )?;
    let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
    let trigger_ident = format!(
        "{}{}",
        trigger_schema
            .map(|value| format!("{value}."))
            .unwrap_or_default(),
        quote(&trigger.name)
    );
    let target_ident = format!(
        "{}{}",
        target_schema
            .map(|value| format!("{value}."))
            .unwrap_or_default(),
        quote(&trigger.target_name)
    );
    Ok((trigger_ident, target_ident))
}

fn pg_sequence_ident(sequence: &MigrationSequence) -> Result<String, String> {
    validate_migration_identifier(&sequence.name)?;
    let schema = sequence
        .schema
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or("PostgreSQL sequence schema is required")?;
    validate_migration_identifier(schema)?;
    let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
    Ok(format!("{}.{}", quote(schema), quote(&sequence.name)))
}

fn pg_validate_sequence_ddl(sequence: &MigrationSequence) -> Result<String, String> {
    validate_sequence_definition_with_identity(
        &sequence.definition,
        sequence.schema.as_deref(),
        &sequence.name,
    )?;
    Ok(sequence.definition.trim().to_owned())
}

fn pg_type_ident(type_definition: &MigrationType) -> Result<String, String> {
    validate_migration_identifier(&type_definition.name)?;
    let schema = pg_object_schema(type_definition.schema.as_deref())?;
    let quote = |value: &str| format!("\"{}\"", value.replace('"', "\"\""));
    Ok(format!(
        "{}{}",
        schema.map(|value| format!("{value}.")).unwrap_or_default(),
        quote(&type_definition.name)
    ))
}

fn pg_type_keyword(definition: &str) -> Result<&'static str, String> {
    let tokens = definition.split_whitespace().collect::<Vec<_>>();
    match tokens
        .get(0..2)
        .map(|pair| (pair[0].to_ascii_uppercase(), pair[1].to_ascii_uppercase()))
    {
        Some((create, kind)) if create == "CREATE" && kind == "DOMAIN" => Ok("DOMAIN"),
        Some((create, kind)) if create == "CREATE" && kind == "TYPE" => Ok("TYPE"),
        _ => Err("PostgreSQL type definition must declare CREATE TYPE or CREATE DOMAIN".into()),
    }
}

fn pg_validate_type_ddl(type_definition: &MigrationType) -> Result<String, String> {
    validate_type_definition_with_identity(
        &type_definition.definition,
        type_definition.schema.as_deref(),
        &type_definition.name,
    )?;
    Ok(type_definition.definition.trim().to_owned())
}

fn pg_validate_object_ddl(
    definition: &str,
    kind: ObjectKind,
    name: &str,
    signature: Option<&str>,
) -> Result<String, String> {
    validate_object_definition_with_identity(definition, kind, name, signature)?;
    Ok(definition.trim().to_owned())
}

pub struct PostgresMigrationRenderer;

impl MigrationRenderer for PostgresMigrationRenderer {
    fn render(&self, op: &MigrationOperation) -> Result<MigrationStatement, String> {
        let qi = |s: &str| {
            if s.contains('.') {
                s.split('.')
                    .map(|part| format!("\"{}\"", part.replace('\"', "\"\"")))
                    .collect::<Vec<_>>()
                    .join(".")
            } else {
                format!("\"{}\"", s.replace('\"', "\"\""))
            }
        };
        let qi_identifier = |s: &str| format!("\"{}\"", s.replace('"', "\"\""));
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
            MigrationOperation::DropTable { table } => {
                let table = validate_migration_identifier(table)?;
                Ok(MigrationStatement {
                    sql: format!("DROP TABLE {}", qi(table)),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: None,
                    summary: format!("DROP TABLE {}", table),
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
            MigrationOperation::CreateIndex { table, index } => {
                let index_type = index.index_type.trim();
                if index_type.is_empty() {
                    return Err(
                        "PostgreSQL index type is missing; cannot safely recreate the index".into(),
                    );
                }
                let index_type = match index_type.to_ascii_uppercase().as_str() {
                    // MySQL reports these access methods in uppercase, while
                    // PostgreSQL's built-in catalog names are lowercase.
                    "BTREE" => "btree",
                    "HASH" => "hash",
                    "FULLTEXT" | "RTREE" | "SPATIAL" => {
                        return Err(format!(
                            "PostgreSQL cannot recreate MySQL {index_type} indexes directly"
                        ));
                    }
                    // Preserve PostgreSQL extension access-method names from
                    // pg_am exactly; quoted custom names may be case-sensitive.
                    _ => index_type,
                };
                Ok(MigrationStatement {
                    sql: format!(
                        "CREATE {}INDEX {} ON {} USING {} ({})",
                        if index.is_unique { "UNIQUE " } else { "" },
                        qi(&index.name),
                        qi(table),
                        qi_identifier(index_type),
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
                })
            }
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
                    sql: format!("ALTER TABLE {} DROP CONSTRAINT {}", qi(table), qi(&pk_name)),
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
            MigrationOperation::SetTableOptions { .. } => Err(
                "PostgreSQL table engine/charset options are outside the portable migration contract".into(),
            ),
            MigrationOperation::SetAutoIncrement { .. } => {
                Err("PostgreSQL auto-increment changes require identity/sequence metadata".into())
            }
            MigrationOperation::AddForeignKey { table, foreign_key } => {
                if foreign_key.name.trim().is_empty()
                    || foreign_key.columns.is_empty()
                    || foreign_key.columns.len() != foreign_key.referenced_columns.len()
                {
                    return Err("foreign key must have a name and matching column lists".into());
                }
                let on_update = pg_fk_action(&foreign_key.on_update, "ON UPDATE")?;
                let on_delete = pg_fk_action(&foreign_key.on_delete, "ON DELETE")?;
                let deferrability = pg_fk_deferrability(foreign_key.deferrability)?;
                Ok(MigrationStatement {
                    sql: format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({}){}{}{}",
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
                        deferrability,
                    ),
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "ALTER TABLE {} DROP CONSTRAINT {}",
                        qi(table),
                        qi(&foreign_key.name)
                    )),
                    summary: format!("ADD FOREIGN KEY {}.{}", table, foreign_key.name),
                })
            }
            MigrationOperation::DropForeignKey { table, foreign_key } => Ok(MigrationStatement {
                sql: format!(
                    "ALTER TABLE {} DROP CONSTRAINT {}",
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
                        "ALTER TABLE {} DROP CONSTRAINT {}",
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
                        "ALTER TABLE {} DROP CONSTRAINT {}",
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
                let ident = pg_view_ident(view);
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
                let ident = pg_view_ident(desired);
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
                let ident = pg_view_ident(view);
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
                let definition = pg_validate_object_ddl(
                    &routine.definition,
                    routine.kind,
                    &routine.name,
                    routine.signature.as_deref(),
                )?;
                let _ = pg_routine_ident(routine)?;
                Ok(MigrationStatement {
                    sql: definition,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "DROP {} {}",
                        routine.kind.as_str().to_ascii_uppercase(),
                        pg_routine_ident(routine)?
                    )),
                    summary: format!("CREATE {} {}", routine.kind.as_str().to_ascii_uppercase(), routine.name),
                })
            }
            MigrationOperation::ReplaceRoutine { current, desired } => {
                if current.kind != desired.kind || current.schema != desired.schema || current.name != desired.name || current.signature != desired.signature {
                    return Err("routine replacement identities must match".into());
                }
                let desired_definition = pg_validate_object_ddl(
                    &desired.definition,
                    desired.kind,
                    &desired.name,
                    desired.signature.as_deref(),
                )?;
                let current_definition = pg_validate_object_ddl(
                    &current.definition,
                    current.kind,
                    &current.name,
                    current.signature.as_deref(),
                )?;
                Ok(MigrationStatement {
                    sql: desired_definition,
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: Some(current_definition),
                    summary: format!("REPLACE {} {}", desired.kind.as_str().to_ascii_uppercase(), desired.name),
                })
            }
            MigrationOperation::DropRoutine { routine } => {
                let definition = pg_validate_object_ddl(
                    &routine.definition,
                    routine.kind,
                    &routine.name,
                    routine.signature.as_deref(),
                )?;
                let ident = pg_routine_ident(routine)?;
                Ok(MigrationStatement {
                    sql: format!("DROP {} {ident}", routine.kind.as_str().to_ascii_uppercase()),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(definition),
                    summary: format!("DROP {} {}", routine.kind.as_str().to_ascii_uppercase(), routine.name),
                })
            }
            MigrationOperation::CreateTrigger { trigger } => {
                let definition = pg_validate_object_ddl(
                    &trigger.definition,
                    ObjectKind::Trigger,
                    &trigger.name,
                    None,
                )?;
                let _ = pg_trigger_ident(trigger)?;
                Ok(MigrationStatement {
                    sql: definition,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!(
                        "DROP TRIGGER {} ON {}",
                        pg_trigger_ident(trigger)?.0,
                        pg_trigger_ident(trigger)?.1
                    )),
                    summary: format!("CREATE TRIGGER {}", trigger.name),
                })
            }
            MigrationOperation::ReplaceTrigger { current, desired } => {
                if current.schema != desired.schema || current.name != desired.name || current.target_schema != desired.target_schema || current.target_name != desired.target_name {
                    return Err("trigger replacement identities must match".into());
                }
                let desired_definition = pg_validate_object_ddl(
                    &desired.definition,
                    ObjectKind::Trigger,
                    &desired.name,
                    None,
                )?;
                let current_definition = pg_validate_object_ddl(
                    &current.definition,
                    ObjectKind::Trigger,
                    &current.name,
                    None,
                )?;
                let (trigger_ident, target_ident) = pg_trigger_ident(desired)?;
                Ok(MigrationStatement {
                    sql: format!("DROP TRIGGER {trigger_ident} ON {target_ident}; {desired_definition}"),
                    risk: MigrationRisk::Rewrite,
                    rollback_sql: Some(format!("DROP TRIGGER {trigger_ident} ON {target_ident}; {current_definition}")),
                    summary: format!("REPLACE TRIGGER {}", desired.name),
                })
            }
            MigrationOperation::DropTrigger { trigger } => {
                let definition = pg_validate_object_ddl(
                    &trigger.definition,
                    ObjectKind::Trigger,
                    &trigger.name,
                    None,
                )?;
                let (trigger_ident, target_ident) = pg_trigger_ident(trigger)?;
                Ok(MigrationStatement {
                    sql: format!("DROP TRIGGER {trigger_ident} ON {target_ident}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(definition),
                    summary: format!("DROP TRIGGER {}", trigger.name),
                })
            }
            MigrationOperation::CreateSequence { sequence } => {
                let definition = pg_validate_sequence_ddl(sequence)?;
                let ident = pg_sequence_ident(sequence)?;
                Ok(MigrationStatement {
                    sql: definition,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP SEQUENCE {ident}")),
                    summary: format!("CREATE SEQUENCE {}", sequence.name),
                })
            }
            MigrationOperation::ReplaceSequence { current, desired } => {
                if current.schema != desired.schema || current.name != desired.name {
                    return Err("sequence replacement identities must match".into());
                }
                pg_validate_sequence_ddl(current)?;
                let desired_definition = pg_validate_sequence_ddl(desired)?;
                let ident = pg_sequence_ident(desired)?;
                Ok(MigrationStatement {
                    sql: format!("DROP SEQUENCE {ident}; {desired_definition}"),
                    risk: MigrationRisk::Destructive,
                    // Recreating a sequence from CREATE DDL cannot restore its
                    // mutable last_value counter, so this operation must not
                    // claim complete rollback support.
                    rollback_sql: None,
                    summary: format!("REPLACE SEQUENCE {}", desired.name),
                })
            }
            MigrationOperation::DropSequence { sequence } => {
                let definition = pg_validate_sequence_ddl(sequence)?;
                let ident = pg_sequence_ident(sequence)?;
                Ok(MigrationStatement {
                    sql: format!("DROP SEQUENCE {ident}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(definition),
                    summary: format!("DROP SEQUENCE {}", sequence.name),
                })
            }
            MigrationOperation::CreateType { type_definition } => {
                let definition = pg_validate_type_ddl(type_definition)?;
                let ident = pg_type_ident(type_definition)?;
                let keyword = pg_type_keyword(&definition)?;
                Ok(MigrationStatement {
                    sql: definition,
                    risk: MigrationRisk::Additive,
                    rollback_sql: Some(format!("DROP {keyword} {ident}")),
                    summary: format!("CREATE TYPE {}", type_definition.name),
                })
            }
            MigrationOperation::ReplaceType { current, desired } => {
                if current.schema != desired.schema || current.name != desired.name {
                    return Err("type replacement identities must match".into());
                }
                let current_definition = pg_validate_type_ddl(current)?;
                let desired_definition = pg_validate_type_ddl(desired)?;
                let ident = pg_type_ident(desired)?;
                let current_keyword = pg_type_keyword(&current_definition)?;
                let desired_keyword = pg_type_keyword(&desired_definition)?;
                if current_keyword != desired_keyword {
                    return Err("type replacement cannot change TYPE/DOMAIN kind".into());
                }
                Ok(MigrationStatement {
                    sql: format!("DROP {desired_keyword} {ident}; {desired_definition}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(format!(
                        "DROP {current_keyword} {ident}; {current_definition}"
                    )),
                    summary: format!("REPLACE TYPE {}", desired.name),
                })
            }
            MigrationOperation::DropType { type_definition } => {
                let definition = pg_validate_type_ddl(type_definition)?;
                let ident = pg_type_ident(type_definition)?;
                let keyword = pg_type_keyword(&definition)?;
                Ok(MigrationStatement {
                    sql: format!("DROP {keyword} {ident}"),
                    risk: MigrationRisk::Destructive,
                    rollback_sql: Some(definition),
                    summary: format!("DROP TYPE {}", type_definition.name),
                })
            }
        }
    }
}

pub struct PostgresMigrationCapabilities;
impl MigrationCapabilities for PostgresMigrationCapabilities {
    fn supports(&self, operation: &MigrationOperation) -> bool {
        match operation {
            MigrationOperation::SetAutoIncrement { .. } => false,
            MigrationOperation::SetTableOptions { .. } => false,
            MigrationOperation::CreateTable { .. }
            | MigrationOperation::DropTable { .. }
            | MigrationOperation::AddColumn { .. }
            | MigrationOperation::DropColumn { .. }
            | MigrationOperation::AlterColumnType { .. }
            | MigrationOperation::SetNullable { .. }
            | MigrationOperation::SetDefault { .. }
            | MigrationOperation::SetComment { .. }
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
            | MigrationOperation::DropTrigger { .. }
            | MigrationOperation::CreateSequence { .. }
            | MigrationOperation::ReplaceSequence { .. }
            | MigrationOperation::DropSequence { .. }
            | MigrationOperation::CreateType { .. }
            | MigrationOperation::ReplaceType { .. }
            | MigrationOperation::DropType { .. } => true,
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
    #[allow(dead_code)]
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
    fn create_index_preserves_and_quotes_access_method() {
        let statement = PostgresMigrationRenderer
            .render(&MigrationOperation::CreateIndex {
                table: "public.documents".into(),
                index: IndexInfo {
                    name: "documents_search_idx".into(),
                    columns: vec!["search_vector".into()],
                    is_unique: false,
                    is_primary: false,
                    index_type: "gin".into(),
                },
            })
            .unwrap();
        assert_eq!(
            statement.sql,
            "CREATE INDEX \"documents_search_idx\" ON \"public\".\"documents\" USING \"gin\" (\"search_vector\")"
        );

        let extension_method = PostgresMigrationRenderer
            .render(&MigrationOperation::CreateIndex {
                table: "documents".into(),
                index: IndexInfo {
                    name: "documents_rum_idx".into(),
                    columns: vec!["search_vector".into()],
                    is_unique: false,
                    is_primary: false,
                    index_type: "rum".into(),
                },
            })
            .unwrap();
        assert!(extension_method.sql.contains(" USING \"rum\" "));

        for (method, quoted) in [
            ("custom.method", "\"custom.method\""),
            ("odd\"name", "\"odd\"\"name\""),
        ] {
            let statement = PostgresMigrationRenderer
                .render(&MigrationOperation::CreateIndex {
                    table: "documents".into(),
                    index: IndexInfo {
                        name: "documents_search_idx".into(),
                        columns: vec!["search_vector".into()],
                        is_unique: false,
                        is_primary: false,
                        index_type: method.into(),
                    },
                })
                .unwrap();
            assert!(statement.sql.contains(&format!(" USING {quoted} (")));
        }

        for (mysql_method, pg_method) in [("BTREE", "btree"), ("HASH", "hash")] {
            let statement = PostgresMigrationRenderer
                .render(&MigrationOperation::CreateIndex {
                    table: "documents".into(),
                    index: IndexInfo {
                        name: "documents_search_idx".into(),
                        columns: vec!["search_vector".into()],
                        is_unique: false,
                        is_primary: false,
                        index_type: mysql_method.into(),
                    },
                })
                .unwrap();
            assert!(statement.sql.contains(&format!(" USING \"{pg_method}\" (")));
        }
    }

    #[test]
    fn create_index_rejects_mysql_only_access_methods() {
        for index_type in ["FULLTEXT", "RTREE", "SPATIAL"] {
            let result = PostgresMigrationRenderer.render(&MigrationOperation::CreateIndex {
                table: "documents".into(),
                index: IndexInfo {
                    name: "documents_search_idx".into(),
                    columns: vec!["search_vector".into()],
                    is_unique: false,
                    is_primary: false,
                    index_type: index_type.into(),
                },
            });
            assert!(
                result.is_err(),
                "{index_type} must fail closed for PostgreSQL"
            );
        }
    }

    #[test]
    fn create_index_rejects_missing_access_method() {
        let result = PostgresMigrationRenderer.render(&MigrationOperation::CreateIndex {
            table: "documents".into(),
            index: IndexInfo {
                name: "documents_idx".into(),
                columns: vec!["id".into()],
                is_unique: false,
                is_primary: false,
                index_type: " ".into(),
            },
        });
        assert!(result.is_err());
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
    fn renders_named_check_constraint_and_rollback() {
        let statement = PostgresMigrationRenderer
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
            "ALTER TABLE \"users\" ADD CONSTRAINT \"users_age_check\" CHECK (age >= 0)"
        );
        assert_eq!(
            statement.rollback_sql.as_deref(),
            Some("ALTER TABLE \"users\" DROP CONSTRAINT \"users_age_check\"")
        );
    }

    #[test]
    fn rejects_check_expression_with_statement_terminator() {
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::AddCheckConstraint {
                table: "users".into(),
                constraint: CheckConstraint {
                    name: "bad".into(),
                    expression: "age >= 0); DROP TABLE users; --".into(),
                },
            })
            .is_err());
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
        assert!(stmt
            .sql
            .contains("COMMENT ON COLUMN \"users\".\"status\" IS 'user status'"));
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
        assert!(stmt
            .sql
            .starts_with("ALTER TABLE \"users\" ADD COLUMN \"score\" integer NOT NULL DEFAULT 0"));
        assert!(stmt
            .sql
            .contains("COMMENT ON COLUMN \"users\".\"score\" IS 'score'"));
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
    fn drop_table_is_quoted_destructive_and_has_no_rollback() {
        let stmt = PostgresMigrationRenderer
            .render(&MigrationOperation::DropTable {
                table: "audit.events".into(),
            })
            .unwrap();
        assert_eq!(stmt.sql, "DROP TABLE \"audit\".\"events\"");
        assert_eq!(stmt.risk, MigrationRisk::Destructive);
        assert!(stmt.rollback_sql.is_none());
        assert!(
            PostgresMigrationCapabilities.supports(&MigrationOperation::DropTable {
                table: "audit.events".into(),
            })
        );
        assert!(!stmt.sql.contains("CASCADE"));
    }

    #[test]
    fn test_tester_drop_table_rejects_empty_identifier() {
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::DropTable {
                table: String::new()
            })
            .is_err());
    }

    #[test]
    fn drop_table_rejects_blank_control_and_invalid_qualified_identifiers() {
        for table in [" ", "audit\nevents", "audit..events", "audit. events"] {
            assert!(
                PostgresMigrationRenderer
                    .render(&MigrationOperation::DropTable {
                        table: table.into()
                    })
                    .is_err(),
                "{table:?}"
            );
        }
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

    #[test]
    fn renders_foreign_key_with_actions_and_rollback() {
        let op = MigrationOperation::AddForeignKey {
            table: "orders".into(),
            foreign_key: ForeignKeyInfo {
                name: "orders_user_id_fkey".into(),
                columns: vec!["user_id".into()],
                referenced_table: "public.users".into(),
                referenced_columns: vec!["id".into()],
                on_update: "CASCADE".into(),
                on_delete: "SET NULL".into(),
                deferrability: ForeignKeyDeferrability::NotDeferrable,
            },
        };
        let stmt = PostgresMigrationRenderer.render(&op).unwrap();
        assert_eq!(
            stmt.sql,
            "ALTER TABLE \"orders\" ADD CONSTRAINT \"orders_user_id_fkey\" FOREIGN KEY (\"user_id\") REFERENCES \"public\".\"users\" (\"id\") ON UPDATE CASCADE ON DELETE SET NULL NOT DEFERRABLE"
        );
        assert_eq!(
            stmt.rollback_sql.as_deref(),
            Some("ALTER TABLE \"orders\" DROP CONSTRAINT \"orders_user_id_fkey\"")
        );
        assert!(PostgresMigrationCapabilities.supports(&op));
    }

    #[test]
    fn renders_deferred_foreign_key_semantics_and_rejects_unknown() {
        let mut operation = MigrationOperation::AddForeignKey {
            table: "orders".into(),
            foreign_key: ForeignKeyInfo {
                name: "orders_user_id_fkey".into(),
                columns: vec!["user_id".into()],
                referenced_table: "users".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "NO ACTION".into(),
                deferrability: ForeignKeyDeferrability::DeferrableInitiallyDeferred,
            },
        };

        let deferred = PostgresMigrationRenderer
            .render(&operation)
            .expect("deferred FK should render");
        assert!(deferred.sql.ends_with("DEFERRABLE INITIALLY DEFERRED"));

        if let MigrationOperation::AddForeignKey { foreign_key, .. } = &mut operation {
            foreign_key.deferrability = ForeignKeyDeferrability::Unknown;
        }
        assert!(PostgresMigrationRenderer.render(&operation).is_err());
    }

    #[test]
    fn renders_view_create_replace_and_drop_with_rollback() {
        let current = MigrationView {
            schema: Some("reporting".into()),
            name: "active_users".into(),
            definition: "SELECT id FROM users WHERE active".into(),
        };
        let desired = MigrationView {
            definition: "SELECT id, email FROM users WHERE active".into(),
            ..current.clone()
        };
        let create = PostgresMigrationRenderer
            .render(&MigrationOperation::CreateView {
                view: desired.clone(),
            })
            .unwrap();
        assert_eq!(
            create.sql,
            "CREATE VIEW \"reporting\".\"active_users\" AS SELECT id, email FROM users WHERE active"
        );
        assert_eq!(
            create.rollback_sql.as_deref(),
            Some("DROP VIEW \"reporting\".\"active_users\"")
        );

        let replace = PostgresMigrationRenderer
            .render(&MigrationOperation::ReplaceView {
                current: current.clone(),
                desired: desired.clone(),
            })
            .unwrap();
        assert!(replace
            .sql
            .starts_with("CREATE OR REPLACE VIEW \"reporting\".\"active_users\""));
        assert!(replace
            .rollback_sql
            .as_deref()
            .is_some_and(|sql| sql.contains("SELECT id FROM users WHERE active")));

        let drop = PostgresMigrationRenderer
            .render(&MigrationOperation::DropView { view: desired })
            .unwrap();
        assert_eq!(drop.risk, MigrationRisk::Destructive);
        assert!(drop.rollback_sql.is_some());
    }

    #[test]
    fn view_definition_rejects_script_injection() {
        let op = MigrationOperation::CreateView {
            view: MigrationView {
                schema: None,
                name: "v".into(),
                definition: "SELECT 1; DROP TABLE users".into(),
            },
        };
        assert!(PostgresMigrationRenderer.render(&op).is_err());
    }

    fn pg_function(definition: &str) -> MigrationRoutine {
        MigrationRoutine {
            kind: ObjectKind::Function,
            schema: Some("public".into()),
            name: "calculate_total".into(),
            signature: Some("integer".into()),
            definition: definition.into(),
        }
    }

    #[test]
    fn renders_routine_create_replace_drop_with_identity_and_rollback() {
        let current = pg_function(
            "CREATE OR REPLACE FUNCTION public.calculate_total(integer) RETURNS integer AS $$ SELECT 1 $$",
        );
        let desired = pg_function(
            "CREATE OR REPLACE FUNCTION public.calculate_total(integer) RETURNS integer AS $$ SELECT 2 $$",
        );
        let create = PostgresMigrationRenderer
            .render(&MigrationOperation::CreateRoutine {
                routine: desired.clone(),
            })
            .unwrap();
        assert!(create.sql.starts_with("CREATE OR REPLACE FUNCTION"));
        assert_eq!(
            create.rollback_sql.as_deref(),
            Some("DROP FUNCTION \"public\".\"calculate_total\"(integer)")
        );
        let replace = PostgresMigrationRenderer
            .render(&MigrationOperation::ReplaceRoutine {
                current: current.clone(),
                desired: desired.clone(),
            })
            .unwrap();
        assert!(replace.sql.contains("SELECT 2"));
        assert!(replace
            .rollback_sql
            .is_some_and(|sql| sql.contains("SELECT 1")));
        let drop = PostgresMigrationRenderer
            .render(&MigrationOperation::DropRoutine { routine: current })
            .unwrap();
        assert_eq!(drop.risk, MigrationRisk::Destructive);
        assert!(drop
            .sql
            .contains("DROP FUNCTION \"public\".\"calculate_total\"(integer)"));
        assert!(drop.rollback_sql.is_some());
    }

    fn pg_sequence(definition: &str) -> MigrationSequence {
        MigrationSequence {
            schema: Some("public".into()),
            name: "orders_id_seq".into(),
            definition: definition.into(),
        }
    }

    #[test]
    fn renders_sequence_create_replace_drop_and_requires_exact_identity() {
        let current = pg_sequence(
            "CREATE SEQUENCE \"public\".\"orders_id_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE; ALTER SEQUENCE \"public\".\"orders_id_seq\" OWNED BY \"public\".\"orders\".\"id\";",
        );
        let desired = pg_sequence(
            "CREATE SEQUENCE \"public\".\"orders_id_seq\" AS bigint INCREMENT BY 10 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 2 CYCLE; ALTER SEQUENCE \"public\".\"orders_id_seq\" OWNED BY \"public\".\"orders\".\"id\";",
        );
        let create = PostgresMigrationRenderer
            .render(&MigrationOperation::CreateSequence {
                sequence: current.clone(),
            })
            .unwrap();
        assert_eq!(create.risk, MigrationRisk::Additive);
        assert!(create
            .sql
            .starts_with("CREATE SEQUENCE \"public\".\"orders_id_seq\""));
        assert_eq!(
            create.rollback_sql.as_deref(),
            Some("DROP SEQUENCE \"public\".\"orders_id_seq\"")
        );
        let replace = PostgresMigrationRenderer
            .render(&MigrationOperation::ReplaceSequence {
                current: current.clone(),
                desired: desired.clone(),
            })
            .unwrap();
        assert_eq!(replace.risk, MigrationRisk::Destructive);
        assert!(replace
            .sql
            .contains("DROP SEQUENCE \"public\".\"orders_id_seq\""));
        assert!(replace.sql.contains("INCREMENT BY 10"));
        assert!(replace.rollback_sql.is_none());
        let drop = PostgresMigrationRenderer
            .render(&MigrationOperation::DropSequence {
                sequence: desired.clone(),
            })
            .unwrap();
        assert_eq!(drop.risk, MigrationRisk::Destructive);
        assert!(drop.rollback_sql.is_some());
        let mismatched = MigrationSequence {
            schema: Some("other".into()),
            ..desired.clone()
        };
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::ReplaceSequence {
                current,
                desired: mismatched,
            })
            .is_err());
    }

    #[test]
    fn sequence_renderer_rejects_name_mismatch_and_unsafe_catalog_ddl() {
        let unsafe_sequence = MigrationSequence {
            schema: Some("public".into()),
            name: "orders_id_seq".into(),
            definition: "CREATE SEQUENCE \"public\".\"other_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE;".into(),
        };
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::CreateSequence {
                sequence: unsafe_sequence,
            })
            .is_err());
        assert!(PostgresMigrationCapabilities.supports(
            &MigrationOperation::CreateSequence {
                sequence: pg_sequence("CREATE SEQUENCE \"public\".\"orders_id_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE;")
            }
        ));
    }

    fn pg_type(definition: &str) -> MigrationType {
        MigrationType {
            schema: Some("public".into()),
            name: "mood".into(),
            definition: definition.into(),
        }
    }

    #[test]
    fn renders_type_create_replace_drop_with_identity_and_rollback() {
        let current = pg_type("CREATE TYPE public.mood AS ENUM ('sad');");
        let desired = pg_type("CREATE TYPE public.mood AS ENUM ('sad', 'happy');");
        let create = PostgresMigrationRenderer
            .render(&MigrationOperation::CreateType {
                type_definition: current.clone(),
            })
            .unwrap();
        assert_eq!(create.risk, MigrationRisk::Additive);
        assert_eq!(
            create.rollback_sql.as_deref(),
            Some("DROP TYPE \"public\".\"mood\"")
        );

        let replace = PostgresMigrationRenderer
            .render(&MigrationOperation::ReplaceType {
                current: current.clone(),
                desired: desired.clone(),
            })
            .unwrap();
        assert_eq!(replace.risk, MigrationRisk::Destructive);
        assert!(replace.sql.contains("DROP TYPE \"public\".\"mood\""));
        assert!(replace.sql.contains("'happy'"));
        assert!(replace.rollback_sql.is_some());

        let drop = PostgresMigrationRenderer
            .render(&MigrationOperation::DropType {
                type_definition: current,
            })
            .unwrap();
        assert_eq!(drop.sql, "DROP TYPE \"public\".\"mood\"");
        assert_eq!(drop.risk, MigrationRisk::Destructive);
        assert!(drop.rollback_sql.is_some());
    }

    #[test]
    fn type_renderer_rejects_identity_mismatch_scripts_and_kind_changes() {
        let mismatch = MigrationType {
            schema: Some("public".into()),
            name: "mood".into(),
            definition: "CREATE TYPE public.other AS ENUM ('ok')".into(),
        };
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::CreateType {
                type_definition: mismatch,
            })
            .is_err());
        let script = pg_type("CREATE TYPE public.mood AS ENUM ('ok'); DROP TABLE users");
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::CreateType {
                type_definition: script,
            })
            .is_err());
        let domain = MigrationType {
            definition: "CREATE DOMAIN public.mood AS text".into(),
            ..pg_type("CREATE TYPE public.mood AS ENUM ('ok')")
        };
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::ReplaceType {
                current: pg_type("CREATE TYPE public.mood AS ENUM ('ok')"),
                desired: domain,
            })
            .is_err());
    }

    #[test]
    fn renders_trigger_operations_and_rejects_missing_target_or_ddl() {
        let trigger = MigrationTrigger {
            schema: Some("public".into()),
            name: "audit_insert".into(),
            target_schema: Some("public".into()),
            target_name: "orders".into(),
            definition:
                "CREATE TRIGGER audit_insert AFTER INSERT ON public.orders EXECUTE FUNCTION audit()"
                    .into(),
        };
        let create = PostgresMigrationRenderer
            .render(&MigrationOperation::CreateTrigger {
                trigger: trigger.clone(),
            })
            .unwrap();
        assert!(create.sql.starts_with("CREATE TRIGGER"));
        assert!(create
            .rollback_sql
            .is_some_and(|sql| sql
                .contains("DROP TRIGGER \"public\".\"audit_insert\" ON \"public\".\"orders\"")));
        let drop = PostgresMigrationRenderer
            .render(&MigrationOperation::DropTrigger {
                trigger: trigger.clone(),
            })
            .unwrap();
        assert_eq!(drop.risk, MigrationRisk::Destructive);
        let missing_target = MigrationTrigger {
            target_name: String::new(),
            ..trigger.clone()
        };
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::DropTrigger {
                trigger: missing_target
            })
            .is_err());
        assert!(PostgresMigrationRenderer
            .render(&MigrationOperation::CreateRoutine {
                routine: MigrationRoutine {
                    definition: "SELECT 1".into(),
                    ..pg_function("CREATE FUNCTION calculate_total(integer) RETURNS integer AS $$ SELECT 1 $$")
                }
            })
            .is_err());
    }
}
