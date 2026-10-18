//! Database backup reads on the provider's retained physical connection.
use crate::PostgresDriver;
use datazen_driver_api::{
    resource::{ResourceError, ResultSink},
    sql_dump::stream::{self, RetainedBackupSource},
    *,
};
use sqlx::Row;

struct Source<'a> {
    driver: &'a PostgresDriver,
    handle: &'a ConnectionHandle,
    execution: &'a QueryExecutionId,
}
#[async_trait]
impl RetainedBackupSource for Source<'_> {
    fn driver(&self) -> &dyn DatabaseDriver {
        self.driver
    }
    async fn objects(&self) -> Result<Vec<TableInfo>, ResourceError> {
        let result=self.driver.query(self.handle,"SELECT n.nspname, c.relname, c.relkind::text FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE c.relkind IN ('r','p','v','m','f') AND n.nspname NOT IN ('pg_catalog','information_schema') AND n.nspname NOT LIKE 'pg_toast%' AND NOT c.relispartition ORDER BY 1,2").await?;
        result
            .rows
            .into_iter()
            .map(|row| {
                let value = |i: usize| match row.get(i) {
                    Some(Some(Value::String(s))) => Ok(s.clone()),
                    _ => Err(DriverError::QueryFailed(
                        "invalid backup catalog row".into(),
                    )),
                };
                Ok(TableInfo {
                    schema: Some(value(0)?),
                    name: value(1)?,
                    table_type: match value(2)?.as_str() {
                        "v" => TableType::View,
                        "m" => TableType::MaterializedView,
                        _ => TableType::Table,
                    },
                    row_count: None,
                })
            })
            .collect()
    }
    async fn ddl(&self, object: &TableInfo) -> Result<String, ResourceError> {
        let retained = self
            .driver
            .transactions
            .lock()
            .await
            .get(&self.handle.id)
            .cloned()
            .ok_or_else(|| DriverError::ConnectionFailed("SessionLost".into()))?;
        let mut connection = retained.lock().await;
        let name = sql_dump::qualified_ident(
            &|n| self.driver.quote_ident(n),
            object.schema.as_deref(),
            &object.name,
        );
        if sql_dump::is_view_like(&object.table_type) {
            let row = sqlx::query("SELECT pg_get_viewdef($1::regclass,true) AS ddl")
                .bind(&name)
                .fetch_one(&mut **connection)
                .await
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            let ddl: String = row
                .try_get("ddl")
                .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
            Ok(format!(
                "CREATE {}VIEW {name} AS\n{};\n",
                if matches!(object.table_type, TableType::MaterializedView) {
                    "MATERIALIZED "
                } else {
                    ""
                },
                ddl.trim_end().trim_end_matches(';')
            ))
        } else {
            Ok(
                crate::catalog::fetch_pg_table_ddl_on_connection(&mut connection, &name, |n| {
                    self.driver.quote_ident(n)
                })
                .await?,
            )
        }
    }
    async fn columns(&self, object: &TableInfo) -> Result<Vec<String>, ResourceError> {
        let name = sql_dump::qualified_ident(
            &|n| self.driver.quote_ident(n),
            object.schema.as_deref(),
            &object.name,
        );
        Ok(self
            .driver
            .query(self.handle, &format!("SELECT * FROM {name} LIMIT 0"))
            .await?
            .columns
            .into_iter()
            .map(|c| c.name)
            .collect())
    }
    async fn rows(&self, sql: &str, sink: &dyn ResultSink) -> Result<(), ResourceError> {
        self.driver
            .stream_resource_query(self.handle, self.execution, sql, None, None, sink)
            .await?;
        Ok(())
    }
    async fn extras(&self, routines: bool, sink: &dyn ResultSink) -> Result<(), ResourceError> {
        let sql = if routines {
            "SELECT pg_get_functiondef(p.oid) FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace WHERE n.nspname NOT IN ('pg_catalog','information_schema') AND p.prokind IN ('f','p') ORDER BY n.nspname,p.proname"
        } else {
            "SELECT pg_get_triggerdef(t.oid) FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE NOT t.tgisinternal AND n.nspname NOT IN ('pg_catalog','information_schema') ORDER BY n.nspname,t.tgname"
        };
        self.rows(sql, &stream::DdlSink(sink)).await
    }
}
impl PostgresDriver {
    pub(crate) async fn retained_backup(
        &self,
        handle: &ConnectionHandle,
        execution: &QueryExecutionId,
        input: &serde_json::Value,
        sink: &dyn ResultSink,
    ) -> Result<(), ResourceError> {
        if !self.fixed_sessions.read().await.contains_key(&handle.id) {
            return Err(ResourceError::unsupported(
                "postgresql",
                "backup_dump",
                "retained session required",
            ));
        }
        let database = input["database"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| DriverError::InvalidConfig("backup database required".into()))?;
        let opts: BackupDumpOptions = serde_json::from_value(input["options"].clone())
            .map_err(|_| DriverError::InvalidConfig("invalid backup options".into()))?;
        if opts.schema_only && opts.data_only {
            return Err(DriverError::InvalidConfig("conflicting backup options".into()).into());
        }
        let current = self.query(handle, "SELECT current_database()").await?;
        if !matches!(current.rows.first().and_then(|row|row.first()), Some(Some(Value::String(name))) if name == database)
        {
            return Err(DriverError::InvalidConfig(
                "backup database does not match retained resource".into(),
            )
            .into());
        }
        if opts.single_transaction {
            self.execute(handle, "BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY")
                .await?;
        }
        let outcome = async {
            if opts.create_database {
                stream::text(
                    sink,
                    &format!("CREATE DATABASE {};\n\n", self.quote_ident(database)),
                )
                .await?;
            }
            stream::dump(
                &Source {
                    driver: self,
                    handle,
                    execution,
                },
                database,
                &opts,
                sink,
            )
            .await
        }
        .await;
        if opts.single_transaction {
            let end = self
                .execute(
                    handle,
                    if outcome.is_ok() {
                        "COMMIT"
                    } else {
                        "ROLLBACK"
                    },
                )
                .await;
            if outcome.is_ok() {
                end?;
            }
        }
        outcome
    }
}
