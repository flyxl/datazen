//! Builds compact schema context for AI prompts from the SchemaCache.

use crate::cache::SchemaCache;
use crate::services::ConnectionManager;
use datazen_ai_api::SqlGenerationContext;
use datazen_driver_api::TableSchema;
use std::sync::Arc;

pub struct SchemaContextBuilder {
    schema_cache: Arc<SchemaCache>,
    connection_manager: Arc<ConnectionManager>,
}

impl SchemaContextBuilder {
    pub fn new(
        schema_cache: Arc<SchemaCache>,
        connection_manager: Arc<ConnectionManager>,
    ) -> Self {
        Self {
            schema_cache,
            connection_manager,
        }
    }

    pub async fn build_sql_context(
        &self,
        connection_id: &str,
        database: &str,
        current_table: Option<&str>,
        recent_queries: &[String],
        max_tokens_budget: usize,
    ) -> Result<SqlGenerationContext, String> {
        let (driver, handle) = self
            .connection_manager
            .get_connection(connection_id)
            .await
            .map_err(|e| e.to_string())?;

        let db_type = format!("{:?}", driver.driver_type());

        let tables = driver
            .get_tables(&handle, database)
            .await
            .map_err(|e| e.to_string())?;

        let mut ddl_parts = Vec::new();
        let mut token_estimate = 0;

        let mut sorted_tables: Vec<_> = tables
            .iter()
            .map(|t| t.name.clone())
            .collect();

        if let Some(current) = current_table {
            sorted_tables.sort_by_key(|name| if name == current { 0 } else { 1 });
        }

        for table_name in &sorted_tables {
            let schema = self
                .schema_cache
                .get_table_schema(connection_id, database, table_name, &driver, &handle)
                .await;

            if let Ok(schema) = schema {
                let ddl_line = format_compact_ddl(table_name, &schema);
                let line_tokens = ddl_line.len() / 4;
                if token_estimate + line_tokens > max_tokens_budget {
                    break;
                }
                token_estimate += line_tokens;
                ddl_parts.push(ddl_line);
            }
        }

        Ok(SqlGenerationContext {
            database_type: db_type,
            database_version: None,
            schema_ddl: ddl_parts.join("\n"),
            current_table: current_table.map(String::from),
            recent_queries: recent_queries.to_vec(),
        })
    }
}

fn format_compact_ddl(table_name: &str, schema: &TableSchema) -> String {
    let columns: Vec<String> = schema
        .columns
        .iter()
        .map(|col| {
            let mut parts = vec![col.name.clone(), col.data_type.clone()];
            if col.is_primary_key {
                parts.push("PK".into());
            }
            if !col.nullable {
                parts.push("NOT NULL".into());
            }
            parts.join(" ")
        })
        .collect();

    format!("  {} ({})", table_name, columns.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::ColumnSchema;

    #[test]
    fn test_format_compact_ddl_basic() {
        let schema = TableSchema {
            table_name: "users".to_string(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "int4".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                    is_auto_increment: true,
                    comment: None,
                },
                ColumnSchema {
                    name: "name".into(),
                    data_type: "varchar(255)".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                },
                ColumnSchema {
                    name: "email".into(),
                    data_type: "varchar(255)".into(),
                    nullable: true,
                    default_value: None,
                    is_primary_key: false,
                    is_auto_increment: false,
                    comment: None,
                },
            ],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
        };

        let ddl = format_compact_ddl("users", &schema);
        assert!(ddl.contains("users"));
        assert!(ddl.contains("id int4 PK NOT NULL"));
        assert!(ddl.contains("name varchar(255) NOT NULL"));
        assert!(ddl.contains("email varchar(255)"));
        assert!(!ddl.contains("email varchar(255) NOT NULL"));
    }

    #[test]
    fn test_format_compact_ddl_empty_table() {
        let schema = TableSchema {
            table_name: "empty".to_string(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        };

        let ddl = format_compact_ddl("empty", &schema);
        assert_eq!(ddl, "  empty ()");
    }
}
