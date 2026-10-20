//! Server information, physical identity, and structure dumps.

use super::*;

impl MysqlDriver {
    pub(crate) async fn get_server_info_inner(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<ServerInfo, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let row = sqlx::query("SELECT version()")
            .fetch_one(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let version: String = row.try_get(0).unwrap_or_default();
        let server_type = if self.is_mariadb || version.to_lowercase().contains("mariadb") {
            "MariaDB"
        } else {
            "MySQL"
        };
        Ok(ServerInfo {
            server_version: version,
            server_type: server_type.to_string(),
        })
    }

    pub(crate) async fn physical_database_identity_inner(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Option<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let Ok(database_row) = sqlx::query("SELECT DATABASE() AS database_name")
            .fetch_one(pool)
            .await
        else {
            return Ok(None);
        };
        let database: Option<String> = database_row.try_get("database_name").unwrap_or(None);
        let Some(database) = database else {
            return Ok(None);
        };

        let cluster_scope = if self.is_mariadb {
            let Ok(rows) = sqlx::query("SHOW GLOBAL STATUS").fetch_all(pool).await else {
                return Ok(None);
            };
            let mut statuses = HashMap::new();
            let mut wsrep_detected = false;
            for row in rows {
                let Ok(name) = row.try_get::<String, _>(0) else {
                    return Ok(None);
                };
                let name = name.to_ascii_lowercase();
                if !name.starts_with("wsrep_") {
                    continue;
                }
                wsrep_detected = true;
                if let Ok(value) = row.try_get::<String, _>(1) {
                    statuses.insert(name, value);
                }
            }
            if wsrep_detected && statuses.is_empty() {
                statuses.insert("wsrep_status_incomplete".to_string(), String::new());
            }
            mariadb_wsrep_scope(Some(&statuses))
        } else {
            let Ok(group_row) =
                sqlx::query("SELECT @@GLOBAL.group_replication_group_name AS group_name")
                    .fetch_one(pool)
                    .await
            else {
                // Older or non-GR MySQL builds may not expose the variable at
                // all. Only fall back when SHOW PLUGINS proves GR is absent or
                // inactive; an active plugin with an unreadable variable is
                // ambiguous and must fail closed.
                let Ok(plugin_rows) = sqlx::query("SHOW PLUGINS").fetch_all(pool).await else {
                    return Ok(None);
                };
                let mut plugin_statuses = HashMap::new();
                for row in plugin_rows {
                    let (Ok(name), Ok(status)) =
                        (row.try_get::<String, _>(0), row.try_get::<String, _>(1))
                    else {
                        return Ok(None);
                    };
                    plugin_statuses.insert(name.to_ascii_lowercase(), status);
                }
                return match mysql_group_replication_plugin_active(Some(&plugin_statuses)) {
                    Some(false) => self.physical_database_node_identity(pool, &database).await,
                    Some(true) | None => Ok(None),
                };
            };
            let Ok(group_name) = group_row.try_get::<String, _>("group_name") else {
                return Ok(None);
            };
            let group_name = group_name.trim();
            if group_name.is_empty() {
                mysql_group_replication_scope(Some(group_name), Ok(None))
            } else {
                // A configured group name alone is not proof that this server
                // is a healthy member. Failure to inspect membership must fail
                // closed rather than treating a peer in that group as separate.
                let membership = sqlx::query(
                    "SELECT MEMBER_STATE AS member_state FROM performance_schema.replication_group_members WHERE MEMBER_ID = @@server_uuid",
                )
                .fetch_optional(pool)
                .await;
                match membership {
                    Ok(None) => mysql_group_replication_scope(Some(group_name), Ok(None)),
                    Ok(Some(row)) => match row.try_get::<String, _>("member_state") {
                        Ok(state) => {
                            mysql_group_replication_scope(Some(group_name), Ok(Some(&state)))
                        }
                        Err(_) => ClusterScopeDecision::Unknown,
                    },
                    Err(_) => ClusterScopeDecision::Unknown,
                }
            }
        };

        match cluster_scope {
            ClusterScopeDecision::Cluster { scope, id } => Ok(Some(canonical_database_identity(
                if self.is_mariadb { "mariadb" } else { "mysql" },
                scope,
                &id,
                &database,
            ))),
            ClusterScopeDecision::Unknown => Ok(None),
            ClusterScopeDecision::NodeFallback => {
                self.physical_database_node_identity(pool, &database).await
            }
        }
    }

    pub(crate) async fn dump_table_ddl_inner(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let db = self.effective_database(handle, database).await;
        let sql = format!(
            "SHOW CREATE TABLE {}",
            Self::qualified_table_ref(&db, table)
        );
        match self.query(handle, &sql).await {
            Ok(result) => {
                if let Some(create) = extract_show_create_table(&result) {
                    let mut ddl = create;
                    let trimmed = ddl.trim_end();
                    if !trimmed.ends_with(';') {
                        ddl = format!("{};\n", trimmed);
                    } else if !ddl.ends_with('\n') {
                        ddl.push('\n');
                    }
                    return Ok(ddl);
                }
                sql_dump::dump_table_ddl_from_schema(self, handle, table, database, schema).await
            }
            Err(_) => {
                sql_dump::dump_table_ddl_from_schema(self, handle, table, database, schema).await
            }
        }
    }

    pub(crate) async fn dump_view_ddl_inner(
        &self,
        handle: &ConnectionHandle,
        view: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<String, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let db = self.effective_database(handle, database).await;
        let sql = format!("SHOW CREATE VIEW {}", Self::qualified_table_ref(&db, view));
        let result = self.query(handle, &sql).await?;
        let create = extract_named_create_column(&result, "Create View").or_else(|| {
            if result.columns.len() >= 2 {
                let row = result.rows.first()?;
                let cell = row.get(1)?.as_ref()?;
                match cell {
                    Value::String(s) if !s.is_empty() => Some(s.clone()),
                    _ => None,
                }
            } else {
                None
            }
        });
        let Some(create) = create else {
            return Err(DriverError::QueryFailed(format!(
                "View definition not found: {view}"
            )));
        };
        let trimmed = create.trim_end();
        if trimmed.ends_with(';') {
            Ok(format!("{trimmed}\n"))
        } else {
            Ok(format!("{trimmed};\n"))
        }
    }

    pub(crate) async fn dump_database_inner(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
    ) -> Result<String, DriverError> {
        self.dump_database_with_progress(handle, database, opts, &mut |_| {})
            .await
    }

    pub(crate) async fn dump_database_with_progress_inner(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        opts: &BackupDumpOptions,
        on_progress: &mut (dyn FnMut(DumpProgress) + Send),
    ) -> Result<String, DriverError> {
        let snapshot = if opts.single_transaction {
            self.begin_transaction(handle).await.ok()
        } else {
            None
        };
        let result = async {
            let mut out = String::new();
            if opts.create_database {
                let q = self.quote_ident(database);
                out.push_str(&format!(
                    "CREATE DATABASE IF NOT EXISTS {};\nUSE {};\n\n",
                    q, q
                ));
            }
            out.push_str(
                &sql_dump::dump_sql_database_with_progress(
                    self,
                    handle,
                    database,
                    opts,
                    on_progress,
                )
                .await?,
            );
            Ok(out)
        }
        .await;
        if let Some(tx) = snapshot {
            if result.is_ok() {
                let _ = self.commit(tx).await;
            } else {
                let _ = self.rollback(tx).await;
            }
        }
        result
    }

    pub(crate) async fn dump_routines_inner(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(dump_mysql_routines(self, handle).await)
    }

    pub(crate) async fn dump_triggers_inner(
        &self,
        handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<String, DriverError> {
        Ok(dump_mysql_triggers(self, handle).await)
    }

    pub(crate) async fn structure_capabilities_inner(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<StructureCapabilities, DriverError> {
        Ok(structure::baseline_capabilities(&self.driver_type()))
    }

    pub(crate) async fn plan_structure_changes_inner(
        &self,
        handle: &ConnectionHandle,
        request: &StructureChangeRequest,
    ) -> Result<StructureChangePlan, DriverError> {
        let caps = self.structure_capabilities(handle).await?;
        structure::plan_structure_changes(&caps, request)
    }
}
