//! Connection lifecycle and the catalog reads that hang off it.

use super::*;

impl MysqlDriver {
    pub(crate) async fn test_connection_inner(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ServerInfo, DriverError> {
        let opts = build_mysql_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let pool = MySqlPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(timeout)
            .connect_with(opts)
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;

        let result = sqlx::query("SELECT version()")
            .fetch_one(&pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()));

        pool.close().await;

        let row = result?;
        let version: String = row.try_get(0).unwrap_or_default();

        let server_type = if version.to_lowercase().contains("mariadb") {
            "MariaDB"
        } else {
            "MySQL"
        };

        Ok(ServerInfo {
            server_version: version,
            server_type: server_type.to_string(),
        })
    }

    pub(crate) async fn connect_inner(
        &self,
        config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        let opts = build_mysql_options(config)?;
        let timeout = Duration::from_secs(config.connection_timeout as u64);

        let max = config.effective_max_pool_size();
        let min = 2u32.min(max);
        let pool = Self::open_pool(opts, timeout, max, min).await?;

        let acquire_result: Result<(), DriverError> = async {
            let _c1 = pool
                .acquire()
                .await
                .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            if max >= 2 {
                let _c2 = pool
                    .acquire()
                    .await
                    .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
            }
            Ok(())
        }
        .await;

        if let Err(e) = acquire_result {
            pool.close().await;
            return Err(e);
        }

        let pool_id = uuid::Uuid::new_v4().to_string();
        let connection_id = uuid::Uuid::new_v4().to_string();
        self.control_keys
            .write()
            .await
            .insert(pool_id.clone(), Self::control_pool_key(config));
        if let Err(error) = self.acquire_control_pool(config, timeout).await {
            // The cancel-only connection is an auxiliary capability, and the
            // primary path must not depend on it: a resource that cannot cancel is
            // degraded, not unusable. `control_pool_for` answers `None`, `KILL
            // QUERY` reports the unavailable control pool, and the MySQL
            // transaction-evidence probe in `fixed_session` skips the check.
            tracing::warn!(
                %error,
                "cancel-only control connection unavailable; this resource cannot cancel precisely"
            );
        }

        if let Some(db) = config
            .database
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            self.active_databases
                .write()
                .await
                .insert(pool_id.clone(), db.to_string());
        }

        self.pools.write().await.insert(pool_id.clone(), pool);

        Ok(ConnectionHandle {
            id: connection_id,
            pool_id,
        })
    }

    /// MySQL serves every database from one pool by qualifying names, so it
    /// holds no per-database resource at all: `close_database` can never release
    /// one. Reporting the connection's own database here would make the tree
    /// mark a database the user never opened, so the default empty report
    /// applies — the tree's own expand state is what shows a database as open.
    pub(crate) async fn open_databases_inner(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<Vec<String>, DriverError> {
        Ok(Vec::new())
    }

    pub(crate) async fn disconnect_inner(
        &self,
        handle: ConnectionHandle,
    ) -> Result<(), DriverError> {
        let removed = self.transactions.lock().await.remove(&handle.id);
        if let Some(pinned) = removed {
            let mut conn = pinned.lock().await;
            let _ = Self::execute_text_on_conn(&mut *conn, "ROLLBACK").await;
        }
        self.fixed_sessions.write().await.remove(&handle.id);
        self.fixed_states.write().await.remove(&handle.id);
        self.active_databases.write().await.remove(&handle.pool_id);
        self.query_executions
            .lock()
            .await
            .retain(|_, execution| execution.session_id != handle.id);
        // Keep the same pools registered across cancellation/timeouts. Removing
        // before close().await would make the next disconnect falsely succeed
        // without waiting for the outstanding physical connections.
        let pool = self.pools.read().await.get(&handle.pool_id).cloned();
        if let Some(pool) = pool {
            pool.close().await;
        }
        // The control connection is shared per (connection identity, database), so
        // this drops one reference: the last resource out closes it.
        self.release_control_pool(&handle.pool_id).await;
        self.pools.write().await.remove(&handle.pool_id);
        Ok(())
    }

    pub(crate) async fn get_databases_inner(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<Vec<String>, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query("SHOW DATABASES")
            .fetch_all(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        Ok(rows.iter().map(|r| decode_mysql_text_idx(r, 0)).collect())
    }

    pub(crate) async fn has_complete_foreign_key_catalog_visibility_inner(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let rows = sqlx::query("SHOW GRANTS FOR CURRENT_USER()")
            .fetch_all(pool)
            .await
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        let grants = rows
            .iter()
            .map(|row| decode_mysql_text_idx(row, 0))
            .collect::<Vec<_>>();
        Ok(Self::grants_prove_server_wide_catalog_visibility(&grants))
    }

    pub(crate) async fn get_tables_inner(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        // MySQL's "schema" *is* the database, so a schema argument is a caller bug.
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;

        let rows = sqlx::query(
            r#"
            SELECT TABLE_NAME, TABLE_TYPE
            FROM information_schema.TABLES
            WHERE TABLE_SCHEMA = ?
            ORDER BY TABLE_NAME
            "#,
        )
        .bind(database)
        .fetch_all(pool)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        if rows.is_empty() {
            // An unknown database and a genuinely empty one both yield zero
            // rows here. Distinguish them so the tree reports a bad target
            // instead of silently presenting an empty database — PostgreSQL
            // errors on the same input, and the two must not disagree.
            let known: Option<String> = sqlx::query_scalar(
                "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
            )
            .bind(database)
            .fetch_optional(pool)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            if known.is_none() {
                return Err(DriverError::QueryFailed(format!(
                    "Unknown database '{database}'"
                )));
            }
        }

        Ok(rows
            .iter()
            .map(|r| {
                let tt = decode_mysql_text(r, "TABLE_TYPE");
                TableInfo {
                    schema: None,
                    name: decode_mysql_text(r, "TABLE_NAME"),
                    table_type: match tt.as_str() {
                        "VIEW" => TableType::View,
                        "SYSTEM VIEW" => TableType::SystemTable,
                        _ => TableType::Table,
                    },
                    row_count: None,
                }
            })
            .collect())
    }

    pub(crate) async fn get_columns_inner(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let db = self.effective_database(handle, database).await;
        let pools = self.pools.read().await;
        let pool = Self::get_pool(&pools, handle)?;
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        // `information_schema` is filtered by TABLE_SCHEMA, so the borrowed
        // connection keeps whatever default schema it was born with.
        Self::fetch_columns_with_db(&mut *conn, &db, Self::bare_table_name(table)).await
    }

    pub(crate) async fn get_all_columns_inner(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let db = self.effective_database(handle, database).await;
        // Clone the pool (cheap Arc) so no lock is held across the query.
        let pool = {
            let pools = self.pools.read().await;
            Self::get_pool(&pools, handle)?.clone()
        };
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        // The batch query is filtered by TABLE_SCHEMA: an explicit per-call
        // database is honoured and no `USE` is issued, so a stale session
        // default can never leak into the result.
        Self::fetch_all_columns_with_db(&mut *conn, &db).await
    }

    pub(crate) async fn get_table_schema_inner(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        let pinned = self.transactions.lock().await.get(&handle.id).cloned();

        if let Some(pinned) = pinned {
            let mut conn = pinned.lock().await;
            let db = if database.trim().is_empty() {
                sqlx::query_scalar::<_, Option<String>>("SELECT DATABASE()")
                    .fetch_one(&mut **conn)
                    .await
                    .map_err(|e| DriverError::QueryFailed(e.to_string()))?
                    .ok_or_else(|| DriverError::InvalidConfig("database is required".into()))?
            } else {
                database.to_string()
            };
            return self
                .table_schema_on_connection(&mut **conn, table, &db)
                .await;
        }
        let db = self.effective_database(handle, database).await;
        let pool = {
            let pools = self.pools.read().await;
            Self::get_pool(&pools, handle)?.clone()
        };
        let mut conn = pool
            .acquire()
            .await
            .map_err(|e| DriverError::ConnectionFailed(e.to_string()))?;
        self.table_schema_on_connection(&mut *conn, table, &db)
            .await
    }
}
