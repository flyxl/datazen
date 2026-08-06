//! Core driver traits.

use async_trait::async_trait;
use std::collections::HashMap;

use crate::types::*;

#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    fn driver_category(&self) -> DriverCategory {
        DriverCategory::Sql
    }

    fn quote_char(&self) -> char {
        '"'
    }

    fn quote_ident(&self, name: &str) -> String {
        let q = self.quote_char();
        if q == '`' {
            format!("`{}`", name.replace('`', "``"))
        } else {
            format!("\"{}\"", name.replace('"', "\"\""))
        }
    }

    fn skip_count_query(&self) -> bool {
        false
    }

    /// Whether the driver's SQL dialect supports `OFFSET` in pagination.
    /// Drivers that don't (e.g. Presto/Hive via Superset) should return `false`.
    fn supports_offset(&self) -> bool {
        true
    }

    fn format_sql_literal(&self, value: &Option<Value>) -> String {
        match value {
            None | Some(Value::Null) => "NULL".to_string(),
            Some(Value::Bool(b)) => {
                if *b {
                    "TRUE".to_string()
                } else {
                    "FALSE".to_string()
                }
            }
            Some(Value::Integer(i)) => i.to_string(),
            Some(Value::Float(f)) => f.to_string(),
            Some(Value::String(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Bytes(b)) => {
                format!("'{}'", String::from_utf8_lossy(b).replace('\'', "''"))
            }
            Some(Value::Timestamp(s)) => format!("'{}'", s.replace('\'', "''")),
            Some(Value::Json(j)) => format!("'{}'", j.to_string().replace('\'', "''")),
        }
    }

    fn build_update_sql(
        &self,
        table: &str,
        set_columns: &[(&str, Option<Value>)],
        pk_columns: &[(&str, Option<Value>)],
    ) -> String {
        let set_clauses: Vec<String> = set_columns
            .iter()
            .map(|(col, val)| {
                format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(val)
                )
            })
            .collect();
        let where_clauses: Vec<String> = pk_columns
            .iter()
            .map(|(col, val)| match val {
                None | Some(Value::Null) => format!("{} IS NULL", self.quote_ident(col)),
                Some(v) => format!(
                    "{} = {}",
                    self.quote_ident(col),
                    self.format_sql_literal(&Some(v.clone()))
                ),
            })
            .collect();
        format!(
            "UPDATE {} SET {} WHERE {}",
            self.quote_ident(table),
            set_clauses.join(", "),
            where_clauses.join(" AND ")
        )
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError>;

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError>;

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError>;

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError>;

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Vec<TableInfo>, DriverError>;

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError>;

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        let schema = self.get_table_schema(handle, table).await?;
        Ok((schema.columns, schema.primary_keys))
    }

    async fn query(&self, handle: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError>;

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError>;

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError>;

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError>;

    async fn begin_transaction(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn commit(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn rollback(&self, _tx: TransactionHandle) -> Result<(), DriverError> {
        Err(DriverError::TransactionError(
            "Not supported for this driver type".into(),
        ))
    }

    async fn explain(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        Err(DriverError::QueryFailed(
            "Not supported for this driver type".into(),
        ))
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError>;

    /// Fetch server version info using an existing connection handle.
    /// Unlike `test_connection` which creates a temporary pool, this reuses the live connection.
    async fn get_server_info(&self, _handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: self.driver_type(),
        })
    }

    /// Switch the active database for subsequent queries.
    /// Drivers that maintain per-session state (e.g. Kiwi) should override this.
    async fn use_database(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<(), DriverError> {
        Ok(())
    }

    /// Return driver-specific prompt overrides for AI features.
    ///
    /// Templates can use `{{variable}}` placeholders. Available variables per
    /// scenario are documented in the main application's `PromptResolver`.
    /// The default implementation returns an empty map (use global defaults).
    fn prompt_overrides(&self) -> HashMap<PromptScenario, PromptTemplate> {
        HashMap::new()
    }
}

#[async_trait]
pub trait KeyValueDriver: Send + Sync {
    fn driver_type(&self) -> DatabaseType;

    async fn scan_keys_with_info(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        pattern: &str,
        cursor: u64,
        count: u32,
    ) -> Result<(u64, Vec<KeyEntry>, u64), DriverError>;

    async fn get_key_detail(
        &self,
        handle: &ConnectionHandle,
        db_index: u32,
        key: &str,
    ) -> Result<KeyDetail, DriverError>;
}
