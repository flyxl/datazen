//! Backup provider adapter: every read goes through the same retained handle.
use datazen_driver_api::{
    resource::{ResourceError, ResultSink},
    sql_dump::stream::{self, RetainedBackupSource},
    *,
};
struct Source<'a> {
    driver: &'a dyn DatabaseDriver,
    handle: &'a ConnectionHandle,
    execution: &'a QueryExecutionId,
    database: &'a str,
}
fn string(row: &[Option<Value>], index: usize) -> Result<String, ResourceError> {
    match row.get(index) {
        Some(Some(Value::String(s))) => Ok(s.clone()),
        _ => Err(DriverError::QueryFailed("invalid backup catalog row".into()).into()),
    }
}
#[async_trait]
impl RetainedBackupSource for Source<'_> {
    fn driver(&self) -> &dyn DatabaseDriver {
        self.driver
    }
    async fn objects(&self) -> Result<Vec<TableInfo>, ResourceError> {
        let result=self.driver.query(self.handle,"SELECT TABLE_NAME,TABLE_TYPE FROM information_schema.tables WHERE TABLE_SCHEMA=DATABASE() AND TABLE_TYPE IN ('BASE TABLE','VIEW') ORDER BY TABLE_NAME").await?;
        result
            .rows
            .into_iter()
            .map(|r| {
                Ok(TableInfo {
                    name: string(&r, 0)?,
                    schema: Some(self.database.into()),
                    table_type: if string(&r, 1)? == "VIEW" {
                        TableType::View
                    } else {
                        TableType::Table
                    },
                    row_count: None,
                })
            })
            .collect()
    }
    async fn ddl(&self, object: &TableInfo) -> Result<String, ResourceError> {
        let kind = if sql_dump::is_view_like(&object.table_type) {
            "VIEW"
        } else {
            "TABLE"
        };
        let relation = sql_dump::qualified_ident(
            &|n| self.driver.quote_ident(n),
            Some(self.database),
            &object.name,
        );
        let result = self
            .driver
            .query(self.handle, &format!("SHOW CREATE {kind} {relation}"))
            .await?;
        Ok(format!(
            "{};\n",
            string(
                result
                    .rows
                    .first()
                    .ok_or_else(|| DriverError::QueryFailed("missing backup DDL".into()))?,
                1
            )?
            .trim_end()
            .trim_end_matches(';')
        ))
    }
    async fn columns(&self, object: &TableInfo) -> Result<Vec<String>, ResourceError> {
        let relation = sql_dump::qualified_ident(
            &|n| self.driver.quote_ident(n),
            Some(self.database),
            &object.name,
        );
        Ok(self
            .driver
            .query(self.handle, &format!("SELECT * FROM {relation} LIMIT 0"))
            .await?
            .columns
            .into_iter()
            .map(|c| c.name)
            .collect())
    }
    async fn rows(&self, sql: &str, sink: &dyn ResultSink) -> Result<(), ResourceError> {
        self.driver
            .query_to_sink(self.handle, self.execution, sql, None, None, sink)
            .await?;
        Ok(())
    }
    async fn extras(&self, routines: bool, sink: &dyn ResultSink) -> Result<(), ResourceError> {
        let queries: &[(&str, &str, &str)] = if routines {
            &[
                (
                    "SHOW PROCEDURE STATUS WHERE Db = DATABASE()",
                    "PROCEDURE",
                    "Name",
                ),
                (
                    "SHOW FUNCTION STATUS WHERE Db = DATABASE()",
                    "FUNCTION",
                    "Name",
                ),
            ]
        } else {
            &[("SHOW TRIGGERS", "TRIGGER", "Trigger")]
        };
        for (sql, kind, name_column) in queries {
            let result = self.driver.query(self.handle, sql).await?;
            let index = result
                .columns
                .iter()
                .position(|c| c.name.eq_ignore_ascii_case(name_column))
                .ok_or_else(|| DriverError::QueryFailed("missing backup object name".into()))?;
            for row in result.rows {
                let name = string(&row, index)?;
                let result = self
                    .driver
                    .query(
                        self.handle,
                        &format!(
                            "SHOW CREATE {kind} {}.{}",
                            self.driver.quote_ident(self.database),
                            self.driver.quote_ident(&name)
                        ),
                    )
                    .await?;
                let index = result
                    .columns
                    .iter()
                    .position(|c| {
                        c.name.starts_with("Create ") || c.name == "SQL Original Statement"
                    })
                    .ok_or_else(|| DriverError::QueryFailed("missing backup object DDL".into()))?;
                let ddl = string(
                    result.rows.first().ok_or_else(|| {
                        DriverError::QueryFailed("missing backup object DDL".into())
                    })?,
                    index,
                )?;
                let delimiter = if ddl.contains("$$") { "//" } else { "$$" };
                stream::text(
                    sink,
                    &format!(
                        "-- {kind}: {}\nDELIMITER {delimiter}\n{}{delimiter}\nDELIMITER ;\n\n",
                        stream::comment_label(&name),
                        ddl.trim_end().trim_end_matches(';')
                    ),
                )
                .await?;
            }
        }
        Ok(())
    }
}
pub(crate) async fn dump(
    driver: &dyn DatabaseDriver,
    handle: &ConnectionHandle,
    execution: &QueryExecutionId,
    input: &serde_json::Value,
    sink: &dyn ResultSink,
) -> Result<(), ResourceError> {
    let database = input["database"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| DriverError::InvalidConfig("backup database required".into()))?;
    let current = driver.query(handle, "SELECT DATABASE()").await?;
    if current
        .rows
        .first()
        .map(|r| string(r, 0))
        .transpose()?
        .as_deref()
        != Some(database)
    {
        return Err(DriverError::InvalidConfig(
            "backup database does not match retained resource".into(),
        )
        .into());
    }
    let opts: BackupDumpOptions = serde_json::from_value(input["options"].clone())
        .map_err(|_| DriverError::InvalidConfig("invalid backup options".into()))?;
    if opts.schema_only && opts.data_only {
        return Err(DriverError::InvalidConfig("conflicting backup options".into()).into());
    }
    if opts.single_transaction {
        driver
            .execute(handle, "START TRANSACTION WITH CONSISTENT SNAPSHOT")
            .await?;
    }
    let outcome = async {
        if opts.create_database {
            stream::text(
                sink,
                &format!(
                    "CREATE DATABASE IF NOT EXISTS {};\nUSE {};\n\n",
                    driver.quote_ident(database),
                    driver.quote_ident(database)
                ),
            )
            .await?;
        }
        stream::dump(
            &Source {
                driver,
                handle,
                execution,
                database,
            },
            database,
            &opts,
            sink,
        )
        .await
    }
    .await;
    if opts.single_transaction {
        let end = driver
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
