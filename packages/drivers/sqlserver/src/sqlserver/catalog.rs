//! Server metadata reads: databases, tables, one table's structure, and the
//! batched column map the connection tree renders from.
//!
//! Split out from `super` because these are the driver's only catalog *read*
//! paths. Each one locks the client map, runs catalog SQL and maps rows into
//! `datazen-driver-api` types; none of them mutates session state, and keeping
//! them together makes the row-to-type mapping contracts readable in one place.

use super::*;

impl SqlServerDriver {
    pub(crate) async fn get_databases(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<Vec<String>, DriverError> {
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let result = Self::run(client, "SELECT name FROM sys.databases ORDER BY name").await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| r.into_iter().next().flatten())
            .map(|v| datazen_driver_http_support::value_display(&v))
            .collect())
    }

    pub(crate) async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        // Listing may legitimately span every schema; an explicit schema just
        // narrows the set. A schema-less model would be a caller bug.
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let sql = Self::build_tables_sql(database, schema);
        let result = Self::run(client, &sql).await?;
        Ok(result
            .rows
            .into_iter()
            .filter_map(|r| {
                let schema_name = r
                    .get(0)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))
                    .unwrap_or_default();
                let name = r
                    .get(1)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))?;
                let kind = r
                    .get(2)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))
                    .unwrap_or_default();
                Some(TableInfo {
                    name,
                    // The backup/dump path feeds this back into
                    // `get_table_schema`, which requires an exact schema.
                    schema: Some(schema_name),
                    table_type: if kind == "VIEW" {
                        TableType::View
                    } else {
                        TableType::Table
                    },
                    row_count: None,
                })
            })
            .collect())
    }

    pub(crate) async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::ExactSchema)?;
        // The validator already guarantees a schema for this driver; resolve it
        // through `default_schema()` rather than hardcoding `dbo` at the call
        // site, so the convention stays owned by the driver.
        let schema = self.effective_schema(schema).unwrap_or_default();
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let parameters = [
            Value::String(schema.to_string()),
            Value::String(table.to_string()),
            Value::String(database.trim().to_string()),
        ];
        let column_rows =
            Self::run_with_params(client, &crate::metadata::columns_sql(database), &parameters)
                .await?;
        let (columns, primary_keys) = crate::metadata::parse_columns(column_rows)?;
        // A relation always has at least one column, so "no columns" means the
        // table is absent (or not visible) rather than a column-less table.
        // Reporting `Ok` here would let callers cache a blank structure; this is
        // the same defect class PostgreSQL fixed in BUG-003.
        if columns.is_empty() {
            return Err(DriverError::QueryFailed(format!(
                "Table '{schema}.{table}' does not exist in database '{database}'"
            )));
        }
        let index_rows =
            Self::run_with_params(client, &crate::metadata::indexes_sql(database), &parameters)
                .await?;
        let indexes = crate::metadata::parse_indexes(index_rows)?;
        let foreign_key_rows = Self::run_with_params(
            client,
            &crate::metadata::foreign_keys_sql(database),
            &parameters,
        )
        .await?;
        let foreign_keys = crate::metadata::parse_foreign_keys(foreign_key_rows)?;
        let check_rows =
            Self::run_with_params(client, &crate::metadata::checks_sql(database), &parameters)
                .await?;
        let check_constraints = crate::metadata::parse_checks(check_rows)?;
        let migration_blockers = schema_migration_blockers_for_indexes(&indexes);
        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes,
            foreign_keys,
            check_constraints,
            table_options: TableOptions {
                migration_blockers,
                ..TableOptions::default()
            },
        })
    }

    pub(crate) async fn get_all_columns(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError> {
        validate_schema_target(self, database, schema, SchemaScope::AnySchema)?;
        let mut map = self.clients.write().await;
        let client = map
            .get_mut(&handle.pool_id)
            .ok_or_else(|| DriverError::ConnectionFailed("Connection pool not found".into()))?;
        let result = Self::run(client, &Self::build_all_columns_sql(database, schema)).await?;

        let mut all_columns: HashMap<String, (Vec<ColumnSchema>, Vec<(i64, String)>)> =
            HashMap::new();
        let mut owners: HashMap<String, String> = HashMap::new();

        for row in &result.rows {
            // SQL: schema_name(0), table_name(1), column_name(2), data_type(3),
            //      is_nullable(4), is_identity(5), default_value(6), comment(7),
            //      is_pk(8), pk_ordinal(9)
            let table_schema = row
                .get(0)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let table_name = row
                .get(1)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let col_name = row
                .get(2)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let data_type = row
                .get(3)
                .cloned()
                .flatten()
                .map(|v| datazen_driver_http_support::value_display(&v))
                .unwrap_or_default();
            let nullable = Self::bit_true(&row.get(4).cloned().flatten());
            let is_pk = Self::bit_true(&row.get(8).cloned().flatten());

            // The payload is keyed by bare table name, so two same-named tables
            // in different schemas cannot both be represented. Keep the first
            // schema seen for a name and say so, rather than merging columns
            // from two different tables. Rows of the *same* table must all be
            // collected — only a name/schema clash skips a row.
            let seen_schema = owners.get(&table_name).cloned();
            match seen_schema {
                Some(existing) if existing != table_schema => {
                    tracing::warn!(
                        table = %table_name,
                        kept = %existing,
                        skipped = %table_schema,
                        "get_all_columns: same-named table in another schema skipped"
                    );
                    continue;
                }
                Some(_) => {}
                None => {
                    owners.insert(table_name.clone(), table_schema.clone());
                }
            }

            let column = ColumnSchema {
                name: col_name.clone(),
                data_type,
                nullable,
                default_value: row
                    .get(6)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v)),
                comment: row
                    .get(7)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v)),
                is_primary_key: is_pk,
                is_auto_increment: Self::bit_true(&row.get(5).cloned().flatten()),
            };

            let entry = all_columns.entry(table_name).or_default();
            entry.0.push(column);
            if is_pk {
                let key_ordinal = row
                    .get(9)
                    .cloned()
                    .flatten()
                    .map(|v| datazen_driver_http_support::value_display(&v))
                    .and_then(|v| v.parse::<i64>().ok())
                    .filter(|ordinal| *ordinal > 0)
                    .ok_or_else(|| {
                        DriverError::QueryFailed(
                            "SQL Server returned incomplete composite primary-key metadata".into(),
                        )
                    })?;
                entry.1.push((key_ordinal, col_name));
            }
        }

        Ok(all_columns
            .into_iter()
            .map(|(table, (columns, mut key_columns))| {
                key_columns.sort_by_key(|(ordinal, _)| *ordinal);
                (
                    table,
                    (
                        columns,
                        key_columns.into_iter().map(|(_, name)| name).collect(),
                    ),
                )
            })
            .collect())
    }
}
