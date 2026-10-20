//! information_schema column reads for one database.

use super::*;

impl MysqlDriver {
    pub(crate) async fn fetch_columns_with_db<'e, E>(
        executor: E,
        current_db: &str,
        table: &str,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError>
    where
        E: sqlx::Executor<'e, Database = sqlx::MySql>,
    {
        let cols = sqlx::query(
            r#"
            SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
                   COLUMN_COMMENT, COLUMN_KEY, EXTRA
            FROM information_schema.COLUMNS
            WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?
            ORDER BY ORDINAL_POSITION
            "#,
        )
        .bind(current_db)
        .bind(table)
        .fetch_all(executor)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut pk_names: Vec<String> = Vec::new();
        let columns: Vec<ColumnSchema> = cols
            .iter()
            .map(|r| {
                let name = decode_mysql_text(r, "COLUMN_NAME");
                let nullable = decode_mysql_text(r, "IS_NULLABLE");
                let key = decode_mysql_text(r, "COLUMN_KEY");
                let extra = decode_mysql_text(r, "EXTRA");
                let is_pk = key == "PRI";
                if is_pk {
                    pk_names.push(name.clone());
                }
                ColumnSchema {
                    is_primary_key: is_pk,
                    name,
                    data_type: decode_mysql_text(r, "COLUMN_TYPE"),
                    nullable: nullable == "YES",
                    default_value: decode_mysql_text_opt(r, "COLUMN_DEFAULT"),
                    comment: {
                        let s = decode_mysql_text(r, "COLUMN_COMMENT");
                        if s.is_empty() {
                            None
                        } else {
                            Some(s)
                        }
                    },
                    is_auto_increment: extra.contains("auto_increment"),
                }
            })
            .collect();

        Ok((columns, pk_names))
    }

    /// Batch-fetch columns for ALL tables in the given database in one query.
    pub(crate) async fn fetch_all_columns_with_db<'e, E>(
        executor: E,
        current_db: &str,
    ) -> Result<HashMap<String, (Vec<ColumnSchema>, Vec<String>)>, DriverError>
    where
        E: sqlx::Executor<'e, Database = sqlx::MySql>,
    {
        let cols = sqlx::query(
            r#"
            SELECT TABLE_NAME, COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT,
                   COLUMN_COMMENT, COLUMN_KEY, EXTRA
            FROM information_schema.COLUMNS
            WHERE TABLE_SCHEMA = ?
            ORDER BY TABLE_NAME, ORDINAL_POSITION
            "#,
        )
        .bind(current_db)
        .fetch_all(executor)
        .await
        .map_err(|e| DriverError::QueryFailed(e.to_string()))?;

        let mut result: HashMap<String, (Vec<ColumnSchema>, Vec<String>)> = HashMap::new();

        for r in &cols {
            let table_name = decode_mysql_text(r, "TABLE_NAME");
            let name = decode_mysql_text(r, "COLUMN_NAME");
            let nullable = decode_mysql_text(r, "IS_NULLABLE");
            let key = decode_mysql_text(r, "COLUMN_KEY");
            let extra = decode_mysql_text(r, "EXTRA");
            let is_pk = key == "PRI";

            let column = ColumnSchema {
                is_primary_key: is_pk,
                name: name.clone(),
                data_type: decode_mysql_text(r, "COLUMN_TYPE"),
                nullable: nullable == "YES",
                default_value: decode_mysql_text_opt(r, "COLUMN_DEFAULT"),
                comment: {
                    let s = decode_mysql_text(r, "COLUMN_COMMENT");
                    if s.is_empty() {
                        None
                    } else {
                        Some(s)
                    }
                },
                is_auto_increment: extra.contains("auto_increment"),
            };

            let entry = result.entry(table_name).or_default();
            entry.0.push(column);
            if is_pk {
                entry.1.push(name);
            }
        }

        Ok(result)
    }
}
