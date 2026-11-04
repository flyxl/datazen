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
        mut input: serde_json::Value,
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
                if input
                    .get("database")
                    .and_then(serde_json::Value::as_str)
                    .is_none()
                {
                    let context = self.read_fixed_session(handle).await?;
                    input["database"] =
                        serde_json::json!(context.namespace.database.ok_or_else(|| {
                            DriverError::InvalidConfig("database is required".into())
                        })?);
                }
                match parts.as_slice() {
                    [table] => {
                        input["table"] = serde_json::json!(table);
                        input["schema"] = serde_json::Value::Null;
                    }
                    [database, table] => {
                        input["database"] = serde_json::json!(database);
                        input["table"] = serde_json::json!(table);
                        input["schema"] = serde_json::Value::Null;
                    }
                    _ => {
                        return Err(DriverError::InvalidConfig(
                            "unsupported relation namespace".into(),
                        ))
                    }
                }
            }
        }
        let raw_relation_result = if command == "get_table_schema"
            && input.get("relationSql").is_some()
            && self.fixed_sessions.read().await.contains_key(&handle.id)
        {
            let database = input["database"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?;
            let table = input["table"]
                .as_str()
                .ok_or_else(|| DriverError::InvalidConfig("table is required".into()))?;
            let pinned = self
                .transactions
                .lock()
                .await
                .get(&handle.id)
                .cloned()
                .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
            let mut conn = pinned.lock().await;
            let schema = self
                .table_schema_exact_on_connection(&mut **conn, table, database)
                .await?;
            Some(CommandResult::new(serde_json::json!({"schema": schema})))
        } else {
            None
        };
        if let Some(mut result) = match raw_relation_result {
            Some(result) => Some(result),
            None => {
                try_execute_schema_catalog_command(self, handle, command, input.clone()).await?
            }
        } {
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
                let relation = if input.get("relationSql").is_some() {
                    format!(
                        "`{}`.`{}`",
                        database.replace('`', "``"),
                        table.replace('`', "``")
                    )
                } else {
                    Self::qualified_table_ref(&database, table)
                };
                let create_row = sqlx::query(&format!("SHOW CREATE TABLE {}", relation))
                    .fetch_one(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
                let temporary = decode_mysql_text_idx(&create_row, 1)
                    .trim_start()
                    .to_ascii_uppercase()
                    .starts_with("CREATE TEMPORARY TABLE");
                result.data["resolvedTarget"] = serde_json::json!({ "database": database, "catalog": null, "schema": null, "table": decode_mysql_text_idx(&create_row, 0), "path": [], "sessionScoped": temporary });
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
