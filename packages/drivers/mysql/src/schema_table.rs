//! Complete SHOW-based introspection on the retained session, including temp tables.
use super::*;
impl MysqlDriver {
    pub(super) async fn table_schema_on_connection(
        &self,
        conn: &mut sqlx::MySqlConnection,
        table: &str,
        db: &str,
    ) -> Result<TableSchema, DriverError> {
        self.table_schema_exact_on_connection(conn, Self::bare_table_name(table), db)
            .await
    }

    pub(super) async fn table_schema_exact_on_connection(
        &self,
        conn: &mut sqlx::MySqlConnection,
        table: &str,
        db: &str,
    ) -> Result<TableSchema, DriverError> {
        let t0 = std::time::Instant::now();
        // Fully qualified, so the SHOW family resolves against `database`
        // instead of whatever schema the pooled connection happens to default
        // to. MySQL accepts `db.table` here — no `USE`, no pollution.
        let q = format!("`{}`.`{}`", db.replace('`', "``"), table.replace('`', "``"));

        let col_rows = match sqlx::query(&format!("SHOW FULL COLUMNS FROM {}", q))
            .fetch_all(&mut *conn)
            .await
        {
            Ok(rows) => rows,
            Err(e) => {
                let err_msg = e.to_string();
                if Self::is_table_not_found_error(&err_msg) {
                    // Never report a missing relation as a table with no
                    // columns: callers cache that, blanking the data grid.
                    return Err(DriverError::QueryFailed(format!(
                        "Table '{}' does not exist in database '{}'",
                        table, db
                    )));
                }
                return Err(DriverError::QueryFailed(err_msg));
            }
        };
        let idx_rows = sqlx::query(&format!("SHOW INDEX FROM {}", q))
            .fetch_all(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let create_row = sqlx::query(&format!("SHOW CREATE TABLE {}", q))
            .fetch_one(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let table_options_row = sqlx::query(MYSQL_TABLE_OPTIONS_QUERY)
            .bind(&db)
            .bind(table)
            .fetch_optional(&mut *conn)
            .await
            .map_err(|e| DriverError::QueryFailed(e.to_string()))?;
        let engine = table_options_row
            .as_ref()
            .and_then(|row| row.try_get::<Option<String>, _>("ENGINE").ok().flatten());
        let charset = table_options_row.as_ref().and_then(|row| {
            row.try_get::<Option<String>, _>("TABLE_CHARSET")
                .ok()
                .flatten()
        });
        let collation = table_options_row.as_ref().and_then(|row| {
            row.try_get::<Option<String>, _>("TABLE_COLLATION")
                .ok()
                .flatten()
        });
        let table_comment = table_options_row.as_ref().and_then(|row| {
            row.try_get::<Option<String>, _>("TABLE_COMMENT")
                .ok()
                .flatten()
        });

        tracing::info!(%table, col_rows = col_rows.len(), idx_rows = idx_rows.len(),
            ms = t0.elapsed().as_millis() as u64,
            "mysql get_table_schema: SHOW queries done");

        // ── columns from SHOW FULL COLUMNS ──
        let mut pk_names: Vec<String> = Vec::new();
        let columns: Vec<ColumnSchema> = col_rows
            .iter()
            .map(|r| {
                let name = decode_mysql_text(r, "Field");
                let col_type = decode_mysql_text(r, "Type");
                let nullable = decode_mysql_text(r, "Null");
                let key = decode_mysql_text(r, "Key");
                let extra = decode_mysql_text(r, "Extra");
                let comment: Option<String> = {
                    let s = decode_mysql_text(r, "Comment");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                };
                let is_pk = key == "PRI";
                if is_pk {
                    pk_names.push(name.clone());
                }
                ColumnSchema {
                    is_primary_key: is_pk,
                    name,
                    data_type: col_type,
                    nullable: nullable == "YES",
                    default_value: decode_mysql_text_opt(r, "Default"),
                    comment,
                    is_auto_increment: extra.contains("auto_increment"),
                }
            })
            .collect();

        // ── indexes from SHOW INDEX ──
        let mut idx_map: HashMap<String, IndexInfo> = HashMap::new();
        for r in &idx_rows {
            let idx_name = decode_mysql_text(r, "Key_name");
            let col_name = decode_mysql_text(r, "Column_name");
            let non_unique: i64 = r.try_get::<i64, _>("Non_unique").unwrap_or(1);
            let idx_type = decode_mysql_text(r, "Index_type");
            let sub_part: Option<i64> = r
                .try_get::<Option<i64>, _>("Sub_part")
                .ok()
                .flatten()
                .or_else(|| {
                    r.try_get::<Option<i32>, _>("Sub_part")
                        .ok()
                        .flatten()
                        .map(|v| v as i64)
                });
            let col_spec = match sub_part {
                Some(sp) if sp > 0 => format!("{col_name}({sp})"),
                _ => col_name,
            };

            let entry = idx_map
                .entry(idx_name.clone())
                .or_insert_with(|| IndexInfo {
                    name: idx_name.clone(),
                    columns: Vec::new(),
                    is_unique: non_unique == 0,
                    is_primary: idx_name == "PRIMARY",
                    index_type: idx_type,
                });
            entry.columns.push(col_spec);
        }

        let primary_keys = idx_map
            .get("PRIMARY")
            .map(|idx| {
                idx.columns
                    .iter()
                    .map(|c| c.split('(').next().unwrap_or(c).to_string())
                    .collect()
            })
            .unwrap_or(pk_names);

        let mut indexes: Vec<IndexInfo> = idx_map.into_values().collect();
        indexes.sort_by(|a, b| b.is_primary.cmp(&a.is_primary).then(a.name.cmp(&b.name)));

        // ── foreign keys parsed from SHOW CREATE TABLE output ──
        let create_sql = decode_mysql_text_idx(&create_row, 1);
        let foreign_keys = Self::parse_fk_from_create_table(&create_sql);
        let check_constraints = Self::parse_check_from_create_table(&create_sql);

        tracing::info!(%table, cols = columns.len(), indexes = indexes.len(), fks = foreign_keys.len(), checks = check_constraints.len(),
            total_ms = t0.elapsed().as_millis() as u64, "mysql get_table_schema: complete");

        Ok(TableSchema {
            table_name: table.to_string(),
            columns,
            primary_keys,
            indexes,
            foreign_keys,
            check_constraints,
            table_options: TableOptions {
                supports_consistent_snapshot: Some(supports_consistent_snapshot_engine(
                    engine.as_deref(),
                )),
                engine,
                charset,
                collation,
                comment: table_comment,
                ..TableOptions::default()
            },
        })
    }
}
