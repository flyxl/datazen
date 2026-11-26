//! Driver Command API surface.

use super::*;

impl MysqlDriver {
    pub(crate) fn command_definitions_inner(&self) -> Vec<DriverCommandDefinition> {
        crate::admin_commands::mysql_admin_command_definitions()
    }

    pub(crate) async fn execute_command_inner(
        &self,
        handle: &ConnectionHandle,
        command: &str,
        input: serde_json::Value,
    ) -> Result<CommandResult, DriverError> {
        match execute_standard_sql_command(self, handle, command, input.clone()).await {
            Err(DriverError::Unsupported(_)) => {}
            other => return other,
        }
        if command == "get_table_schema" {
            if let Some(raw) = input.get("relationSql").and_then(serde_json::Value::as_str) {
                let parsed = sqlparser::parser::Parser::parse_sql(
                    &sqlparser::dialect::MySqlDialect {},
                    &format!("SELECT * FROM {raw}"),
                )
                .map_err(|_| DriverError::InvalidConfig("invalid relation identifier".into()))?;
                let [sqlparser::ast::Statement::Query(query)] = parsed.as_slice() else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                let sqlparser::ast::SetExpr::Select(select) = &*query.body else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                let [relation] = select.from.as_slice() else {
                    return Err(DriverError::InvalidConfig(
                        "one relation is required".into(),
                    ));
                };
                if !relation.joins.is_empty() {
                    return Err(DriverError::InvalidConfig(
                        "a join cannot identify one editable table".into(),
                    ));
                }
                let sqlparser::ast::TableFactor::Table { name, .. } = &relation.relation else {
                    return Err(DriverError::InvalidConfig(
                        "one named relation is required".into(),
                    ));
                };
                let parts = name
                    .0
                    .iter()
                    .map(|part| {
                        part.as_ident()
                            .map(|ident| ident.value.clone())
                            .ok_or_else(|| {
                                DriverError::InvalidConfig("named relation is required".into())
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let (requested, table) = match parts.as_slice() {
                    [table] => (
                        input.get("database").and_then(serde_json::Value::as_str),
                        table,
                    ),
                    [database, table] => (Some(database.as_str()), table),
                    _ => {
                        return Err(DriverError::InvalidConfig(
                            "unsupported relation namespace".into(),
                        ));
                    }
                };
                let mut conn = crate::session_connection::for_session(self, handle).await?;
                let database = match requested.filter(|db| !db.trim().is_empty()) {
                    Some(db) => db.to_string(),
                    None if conn.is_pinned() => {
                        sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
                            .fetch_one(&mut *conn)
                            .await
                            .map_err(|e| DriverError::QueryFailed(e.to_string()))?
                            .ok_or_else(|| {
                                DriverError::InvalidConfig("database is required".into())
                            })?
                    }
                    None => self.effective_database(handle, "").await,
                };
                if database.is_empty() {
                    return Err(DriverError::InvalidConfig("database is required".into()));
                }
                let schema = self
                    .table_schema_exact_on_connection(&mut conn, table, &database)
                    .await?;
                let mut result = CommandResult::new(serde_json::json!({"schema": schema}));
                if self.fixed_sessions.read().await.contains_key(&handle.id) {
                    let relation = Self::exact_table_ref(&database, table);
                    result.data["resolvedTarget"] =
                        resolved_relation_target(&mut conn, &relation, &database).await?;
                }
                return Ok(result);
            }
        }
        if let Some(mut result) =
            try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
        {
            if command == "get_table_schema"
                && self.fixed_sessions.read().await.contains_key(&handle.id)
            {
                let table = input["table"]
                    .as_str()
                    .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
                let requested = input["database"]
                    .as_str()
                    .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?;
                let pinned = self
                    .transactions
                    .lock()
                    .await
                    .get(&handle.id)
                    .cloned()
                    .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
                let mut conn = pinned.lock().await;
                let database = if requested.is_empty() {
                    sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
                        .fetch_one(&mut **conn)
                        .await
                        .map_err(|e| DriverError::QueryFailed(e.to_string()))?
                        .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?
                } else {
                    requested.to_string()
                };
                let resolved = result.data["schema"]["tableName"].as_str().unwrap_or(table);
                let relation = Self::exact_table_ref(&database, resolved);
                result.data["resolvedTarget"] =
                    resolved_relation_target(&mut conn, &relation, &database).await?;
            }
            return Ok(result);
        }
        if is_schema_object_command(command) {
            return execute_schema_object_command(
                self,
                &self.driver_type(),
                handle,
                command,
                input,
            )
            .await;
        }
        // Admin commands run on the **session's own** connection. A retained
        // session pins the pool's only connection, so borrowing a second one
        // here would wait for a checkout that never returns (it timed out after
        // 30s and surfaced as a bare "pool timed out"). `for_session` hands back
        // the pinned connection when the session has one and a pooled connection
        // otherwise, which keeps `estimate_table_rows` and the CREATE/GRANT
        // family scoped to the database the user is looking at.
        let mut conn = crate::session_connection::for_session(self, handle).await?;
        crate::admin_commands::execute_mysql_admin_command(&mut conn, command, input).await
    }
}

async fn resolved_relation_target(
    conn: &mut sqlx::MySqlConnection,
    relation: &str,
    database: &str,
) -> Result<serde_json::Value, DriverError> {
    let create_row = sqlx::query(&format!("SHOW CREATE TABLE {relation}"))
        .fetch_one(conn)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
    let temporary = decode_mysql_text_idx(&create_row, 1)
        .trim_start()
        .to_ascii_uppercase()
        .starts_with("CREATE TEMPORARY TABLE");
    Ok(serde_json::json!({
        "database": database, "catalog": null, "schema": null,
        "table": decode_mysql_text_idx(&create_row, 0), "path": [], "sessionScoped": temporary
    }))
}
