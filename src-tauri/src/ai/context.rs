//! Builds compact schema context for AI prompts from the SchemaCache.
//!
//! [`SchemaContextBuilder`] provides methods to fetch table names and build
//! selective/full DDL context.  [`format_compact_ddl`] produces a compact
//! `CREATE TABLE` representation including PK, FK, indexes, defaults, and
//! comments.

use crate::cache::SchemaCache;
use crate::services::ConnectionManager;
use datazen_ai_api::SqlGenerationContext;
use datazen_driver_api::TableSchema;
use std::sync::Arc;

/// Stable prompt-facing database type string (no Debug quotes).
pub fn prompt_db_type(driver: &dyn crate::db::DatabaseDriver) -> String {
    driver.driver_type()
}

pub struct SchemaContextBuilder {
    schema_cache: Arc<SchemaCache>,
    connection_manager: Arc<ConnectionManager>,
}

impl SchemaContextBuilder {
    pub fn new(schema_cache: Arc<SchemaCache>, connection_manager: Arc<ConnectionManager>) -> Self {
        Self {
            schema_cache,
            connection_manager,
        }
    }

    /// Schema to use for the AI's metadata reads.
    ///
    /// The AI only ever has a database plus table names scraped from the
    /// request or the SQL text — never a schema. So it takes the connection
    /// config's schema and, failing that, the driver's own convention
    /// (`public` for PostgreSQL, `dbo` for SQL Server). Nothing is remembered
    /// on the session: every read below passes this target explicitly.
    async fn resolve_schema(
        &self,
        db_session_id: &str,
        driver: &std::sync::Arc<dyn crate::db::DatabaseDriver>,
    ) -> Option<String> {
        let config_schema = self
            .connection_manager
            .get_session_config(db_session_id)
            .await
            .ok()
            .and_then(|config| config.schema);
        crate::services::metadata_schema(driver.as_ref(), None, None, config_schema.as_deref())
    }

    /// Returns only table names (no column details). Much cheaper for initial LLM calls.
    pub async fn get_table_names(
        &self,
        db_session_id: &str,
        database: &str,
    ) -> Result<(String, Vec<String>), String> {
        let (driver, handle) = self
            .connection_manager
            .get_session(db_session_id)
            .await
            .map_err(|e| e.to_string())?;

        let db_type = prompt_db_type(driver.as_ref());
        let schema = self.resolve_schema(db_session_id, &driver).await;

        let tables = driver
            .get_tables(&handle, database, schema.as_deref())
            .await
            .map_err(|e| e.to_string())?;

        let names: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();
        Ok((db_type, names))
    }

    /// Build detailed DDL for specific tables only.
    pub async fn build_selective_context(
        &self,
        db_session_id: &str,
        database: &str,
        table_names: &[String],
        max_tokens_budget: usize,
    ) -> Result<SqlGenerationContext, String> {
        let (driver, handle) = self
            .connection_manager
            .get_session(db_session_id)
            .await
            .map_err(|e| e.to_string())?;

        let db_type = prompt_db_type(driver.as_ref());
        let schema = self.resolve_schema(db_session_id, &driver).await;
        let mut ddl_parts = Vec::new();
        let mut token_estimate = 0;

        for table_name in table_names {
            let schema = self
                .schema_cache
                .get_table_schema(
                    db_session_id,
                    database,
                    schema.as_deref(),
                    table_name,
                    &driver,
                    &handle,
                )
                .await;

            match schema {
                Ok(schema) => {
                    let ddl_line = format_compact_ddl(table_name, &schema);
                    let line_tokens = crate::ai::budget::estimate_tokens(&ddl_line);
                    if token_estimate + line_tokens > max_tokens_budget {
                        break;
                    }
                    token_estimate += line_tokens;
                    ddl_parts.push(ddl_line);
                }
                Err(e) => {
                    tracing::warn!(
                        db_session_id = %db_session_id,
                        database = %database,
                        table = %table_name,
                        error = %e,
                        "build_selective_context: skipping pinned table (schema fetch failed)"
                    );
                }
            }
        }

        Ok(SqlGenerationContext {
            database_type: db_type,
            database_version: None,
            schema_ddl: ddl_parts.join("\n"),
            current_table: None,
            recent_queries: vec![],
        })
    }

    pub async fn build_sql_context(
        &self,
        db_session_id: &str,
        database: &str,
        current_table: Option<&str>,
        recent_queries: &[String],
        max_tokens_budget: usize,
    ) -> Result<SqlGenerationContext, String> {
        let (driver, handle) = self
            .connection_manager
            .get_session(db_session_id)
            .await
            .map_err(|e| e.to_string())?;

        let db_type = prompt_db_type(driver.as_ref());
        let schema = self.resolve_schema(db_session_id, &driver).await;
        let tables = driver
            .get_tables(&handle, database, schema.as_deref())
            .await
            .map_err(|e| e.to_string())?;

        let names: Vec<String> = tables.iter().map(|t| t.name.clone()).collect();

        // If a current table is specified, prioritize it
        let prioritized = if let Some(ct) = current_table {
            let mut p: Vec<String> = Vec::new();
            if names.iter().any(|n| n == ct) {
                p.push(ct.to_string());
            }
            for n in &names {
                if n != ct {
                    p.push(n.clone());
                }
            }
            p
        } else {
            names
        };

        let ranked = rank_tables(&prioritized, current_table);

        let mut ddl_parts = Vec::new();
        let mut token_estimate = 0;

        for table_name in &ranked {
            let schema = self
                .schema_cache
                .get_table_schema(
                    db_session_id,
                    database,
                    schema.as_deref(),
                    table_name,
                    &driver,
                    &handle,
                )
                .await;

            match schema {
                Ok(schema) => {
                    let ddl_line = format_compact_ddl(table_name, &schema);
                    let line_tokens = crate::ai::budget::estimate_tokens(&ddl_line);
                    if token_estimate + line_tokens > max_tokens_budget {
                        break;
                    }
                    token_estimate += line_tokens;
                    ddl_parts.push(ddl_line);
                }
                Err(e) => {
                    tracing::warn!(
                        db_session_id = %db_session_id,
                        database = %database,
                        table = %table_name,
                        error = %e,
                        "build_sql_context: skipping table (schema fetch failed)"
                    );
                }
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

/// Rank tables by relevance: boost `current_table`, keep original order for the rest.
///
/// This is a simple local scoring heuristic that returns up to 30 tables.
pub fn rank_tables(tables: &[String], current_table: Option<&str>) -> Vec<String> {
    let max_tables = 30;
    let mut scored: Vec<(usize, String)> = tables
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let boost = if Some(name.as_str()) == current_table {
                0 // lowest index = highest priority
            } else {
                i + 1
            };
            (boost, name.clone())
        })
        .collect();
    scored.sort_by_key(|(boost, _)| *boost);
    scored
        .into_iter()
        .take(max_tables)
        .map(|(_, name)| name)
        .collect()
}

/// Format a compact DDL string for a table.
///
/// Format: `tablename (col1 type1 [PK] [NOT NULL] [DEFAULT x] [COMMENT 'text'], col2 type2, ...)`
/// Includes: primary keys, NOT NULL constraints, default values, comments,
/// foreign keys, and indexes.
pub fn format_compact_ddl(table_name: &str, schema: &TableSchema) -> String {
    let pk_set: std::collections::HashSet<&str> =
        schema.primary_keys.iter().map(|s| s.as_str()).collect();

    let columns: Vec<String> = schema
        .columns
        .iter()
        .map(|c| {
            let mut parts = vec![c.name.clone(), c.data_type.clone()];
            if pk_set.contains(c.name.as_str()) {
                parts.push("PK".into());
            }
            // PK implies NOT NULL; only annotate explicitly when non-PK and non-nullable
            if !c.nullable && !pk_set.contains(c.name.as_str()) {
                parts.push("NOT NULL".into());
            }
            if let Some(ref default) = c.default_value {
                parts.push(format!("DEFAULT {default}"));
            }
            if let Some(ref comment) = c.comment {
                if !comment.is_empty() {
                    // BUG-08: Truncate column comments to 40 chars to avoid blowing up the token budget.
                    let truncated = if comment.len() > 40 {
                        let end = comment
                            .char_indices()
                            .take_while(|(i, _)| *i < 40)
                            .last()
                            .map(|(i, c)| i + c.len_utf8())
                            .unwrap_or(40);
                        &comment[..end]
                    } else {
                        comment.as_str()
                    };
                    parts.push(format!("COMMENT '{}'", truncated.replace('\'', "''")));
                }
            }
            parts.join(" ")
        })
        .collect();

    let mut out = format!("{} ({})", table_name, columns.join(", "));

    // Append foreign keys
    if !schema.foreign_keys.is_empty() {
        let fks: Vec<String> = schema
            .foreign_keys
            .iter()
            .map(|fk| {
                let cols = fk.columns.join(", ");
                let ref_cols = fk.referenced_columns.join(", ");
                format!("FK: {} -> {}.{}", cols, fk.referenced_table, ref_cols)
            })
            .collect();
        out.push_str(&format!(" | FKs: [{}]", fks.join(", ")));
    }

    // Append indexes (skip primary key indexes)
    let non_pk_indexes: Vec<_> = schema
        .indexes
        .iter()
        .filter(|idx| !idx.is_primary)
        .collect();
    if !non_pk_indexes.is_empty() {
        let idxs: Vec<String> = non_pk_indexes
            .iter()
            .map(|idx| {
                let unique = if idx.is_unique { "UNIQUE " } else { "" };
                format!("{}{}({})", unique, idx.name, idx.columns.join(", "))
            })
            .collect();
        out.push_str(&format!(" | Indexes: [{}]", idxs.join(", ")));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_driver_api::{ColumnSchema, ForeignKeyInfo, IndexInfo};

    #[test]
    fn format_compact_ddl_basic() {
        let schema = TableSchema {
            table_name: "users".to_string(),
            columns: vec![
                ColumnSchema {
                    name: "id".into(),
                    data_type: "int".into(),
                    nullable: false,
                    default_value: None,
                    is_primary_key: true,
                    is_auto_increment: true,
                    comment: None,
                },
                ColumnSchema {
                    name: "name".into(),
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
        assert!(ddl.starts_with("users ("));
        // PK implies NOT NULL; format_compact_ddl no longer repeats NOT NULL for PK
        assert!(ddl.contains("id int PK"));
        assert!(!ddl.contains("id int PK NOT NULL"));
        assert!(ddl.contains("name varchar(255)"));
    }

    #[test]
    fn format_compact_ddl_with_default_and_comment() {
        let schema = TableSchema {
            table_name: "settings".to_string(),
            columns: vec![ColumnSchema {
                name: "theme".into(),
                data_type: "varchar(50)".into(),
                nullable: true,
                default_value: Some("'light'".into()),
                is_primary_key: false,
                is_auto_increment: false,
                comment: Some("User theme preference".into()),
            }],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        };
        let ddl = format_compact_ddl("settings", &schema);
        assert!(ddl.contains("DEFAULT 'light'"));
        assert!(ddl.contains("COMMENT 'User theme preference'"));
    }

    #[test]
    fn format_compact_ddl_with_fk_and_index() {
        let schema = TableSchema {
            table_name: "orders".to_string(),
            columns: vec![ColumnSchema {
                name: "user_id".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                is_primary_key: false,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec![],
            indexes: vec![IndexInfo {
                name: "idx_user_id".into(),
                columns: vec!["user_id".into()],
                is_unique: false,
                is_primary: false,
                index_type: "btree".into(),
            }],
            foreign_keys: vec![ForeignKeyInfo {
                name: "fk_user".into(),
                columns: vec!["user_id".into()],
                referenced_table: "users".into(),
                referenced_columns: vec!["id".into()],
                on_update: "NO ACTION".into(),
                on_delete: "CASCADE".into(),
            }],
        };
        let ddl = format_compact_ddl("orders", &schema);
        assert!(ddl.contains("FK: user_id -> users.id"));
        assert!(ddl.contains("Indexes: [idx_user_id(user_id)]"));
    }

    #[test]
    fn format_compact_ddl_empty() {
        let schema = TableSchema {
            table_name: "empty".to_string(),
            columns: vec![],
            primary_keys: vec![],
            indexes: vec![],
            foreign_keys: vec![],
        };
        let ddl = format_compact_ddl("empty", &schema);
        assert_eq!(ddl, "empty ()");
    }

    #[test]
    fn format_compact_ddl_pk_without_not_null_flag() {
        let schema = TableSchema {
            table_name: "t".to_string(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "int".into(),
                nullable: true,
                default_value: None,
                is_primary_key: true,
                is_auto_increment: false,
                comment: None,
            }],
            primary_keys: vec!["id".into()],
            indexes: vec![],
            foreign_keys: vec![],
        };
        let ddl = format_compact_ddl("t", &schema);
        assert!(ddl.contains("id int PK"));
        assert!(!ddl.contains("NOT NULL"));
    }

    #[test]
    fn format_compact_ddl_skips_pk_index() {
        let schema = TableSchema {
            table_name: "t".to_string(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "int".into(),
                nullable: false,
                default_value: None,
                is_primary_key: true,
                is_auto_increment: true,
                comment: None,
            }],
            primary_keys: vec!["id".into()],
            indexes: vec![IndexInfo {
                name: "PRIMARY".into(),
                columns: vec!["id".into()],
                is_unique: true,
                is_primary: true,
                index_type: "btree".into(),
            }],
            foreign_keys: vec![],
        };
        let ddl = format_compact_ddl("t", &schema);
        // PK index should NOT appear in the Indexes section
        assert!(!ddl.contains("Indexes:"));
    }

    #[test]
    fn test_rank_tables_boosts_current() {
        let tables = vec!["orders".into(), "users".into(), "products".into()];
        let ranked = rank_tables(&tables, Some("products"));
        assert_eq!(ranked[0], "products");
        assert_eq!(ranked.len(), 3);
    }

    #[test]
    fn test_rank_tables_no_current() {
        let tables = vec!["a".into(), "b".into(), "c".into()];
        let ranked = rank_tables(&tables, None);
        assert_eq!(ranked, tables);
    }

    #[test]
    fn test_rank_tables_limits_to_30() {
        let tables: Vec<String> = (0..50).map(|i| format!("t{i}")).collect();
        let ranked = rank_tables(&tables, None);
        assert_eq!(ranked.len(), 30);
    }
}
