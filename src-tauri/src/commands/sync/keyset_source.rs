//! Live driver-backed keyset page source for Data Sync compare.

use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::Value;

use crate::data_sync::{
    build_keyset_select_sql, mysql_placeholder, postgres_placeholder, DataSyncError, Row,
    RowPageSource,
};
use crate::db::{ConnectionHandle, DatabaseDriver, SqlTarget};

pub struct DriverKeysetSource {
    driver: Arc<dyn DatabaseDriver>,
    handle: ConnectionHandle,
    table: String,
    database: Option<String>,
    schema: Option<String>,
    columns: Vec<String>,
    pk_columns: Vec<String>,
    quote: char,
    family: String,
}

impl DriverKeysetSource {
    pub fn new(
        driver: Arc<dyn DatabaseDriver>,
        handle: ConnectionHandle,
        table: String,
        database: Option<String>,
        schema: Option<String>,
        columns: Vec<String>,
        pk_columns: Vec<String>,
        quote: char,
        family: &str,
    ) -> Self {
        Self {
            driver,
            handle,
            table,
            database,
            schema,
            columns,
            pk_columns,
            quote,
            family: family.to_string(),
        }
    }
}

#[async_trait]
impl RowPageSource for DriverKeysetSource {
    async fn next_page(
        &mut self,
        after_key: Option<&[Value]>,
        limit: u32,
    ) -> Result<Vec<Row>, DataSyncError> {
        let family = self.family.clone();
        let quote = self.quote;
        let (sql, params) = build_keyset_select_sql(
            &self.table,
            self.database.as_deref(),
            self.schema.as_deref(),
            &family,
            &self.columns,
            &self.pk_columns,
            after_key,
            limit,
            quote,
            |i| {
                if family == "mysql" {
                    mysql_placeholder(i)
                } else {
                    postgres_placeholder(i)
                }
            },
        )?;
        // The keyset SQL is already fully qualified for MySQL-family drivers,
        // but PostgreSQL cannot cross databases in one statement: the target is
        // what selects the pool the read must run on.
        let result = self
            .driver
            .query_with_params_at(
                &self.handle,
                &sql,
                &params,
                SqlTarget::new(self.database.as_deref(), self.schema.as_deref()),
            )
            .await
            .map_err(|e| DataSyncError::validation(e.to_string()))?;
        Ok(result.rows)
    }
}
