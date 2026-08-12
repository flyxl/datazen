//! Unified schema prompt seeding for Chat / NL2SQL.

use crate::ai::context::SchemaContextBuilder;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct PromptSeed {
    pub database_type: String,
    pub table_names: Vec<String>,
    pub pinned_schema_ddl: String,
    pub attach_db_tools: bool,
    pub fallback_schema_ddl: Option<String>,
}

pub fn format_table_names_block(names: &[String], max_names: usize) -> String {
    if names.is_empty() {
        return "(no tables)".to_string();
    }
    if names.len() <= max_names {
        return names.join(", ");
    }
    let head: Vec<&str> = names.iter().take(max_names).map(String::as_str).collect();
    format!("{}, …and {} more", head.join(", "), names.len() - max_names)
}

pub fn compose_schema_system_suffix(seed: &PromptSeed) -> String {
    let names = format_table_names_block(&seed.table_names, 200);
    let mut out = format!(
        "Database type: {}\nAvailable tables:\n{}\n",
        seed.database_type, names
    );
    if !seed.pinned_schema_ddl.trim().is_empty() {
        out.push_str("\nPinned table schemas (user @ selection):\n");
        out.push_str(&seed.pinned_schema_ddl);
        out.push('\n');
    }
    if seed.attach_db_tools {
        out.push_str(
            "\nUse list_tables / get_table_schema tools to fetch schemas for tables you need beyond the pinned set.\n",
        );
    } else if let Some(fb) = &seed.fallback_schema_ddl {
        out.push_str("\nSchema:\n");
        out.push_str(fb);
        out.push('\n');
    }
    out
}

pub struct SchemaContextPipeline {
    builder: Arc<SchemaContextBuilder>,
}

impl SchemaContextPipeline {
    pub fn new(builder: Arc<SchemaContextBuilder>) -> Self {
        Self { builder }
    }

    pub async fn resolve(
        &self,
        connection_id: &str,
        database: &str,
        pinned_tables: &[String],
        supports_tools: bool,
        pinned_budget: usize,
        fallback_budget: usize,
    ) -> Result<PromptSeed, String> {
        let (db_type, table_names) = self
            .builder
            .get_table_names(connection_id, database)
            .await
            .unwrap_or_else(|_| (String::new(), Vec::new()));

        let pinned_schema_ddl = if pinned_tables.is_empty() {
            String::new()
        } else {
            match self
                .builder
                .build_selective_context(connection_id, database, pinned_tables, pinned_budget)
                .await
            {
                Ok(c) => c.schema_ddl,
                Err(e) => {
                    tracing::warn!(
                        connection_id = %connection_id,
                        database = %database,
                        pinned_count = pinned_tables.len(),
                        error = %e,
                        "schema_pipeline: selective context build failed"
                    );
                    String::new()
                }
            }
        };

        let fallback_schema_ddl = if supports_tools {
            None
        } else {
            let ctx = self
                .builder
                .build_sql_context(connection_id, database, None, &[], fallback_budget)
                .await?;
            Some(ctx.schema_ddl)
        };

        Ok(assemble_seed(
            db_type,
            table_names,
            pinned_schema_ddl,
            supports_tools,
            fallback_schema_ddl,
        ))
    }
}

fn assemble_seed(
    database_type: String,
    table_names: Vec<String>,
    pinned_schema_ddl: String,
    supports_tools: bool,
    fallback_schema_ddl: Option<String>,
) -> PromptSeed {
    PromptSeed {
        database_type,
        table_names,
        pinned_schema_ddl,
        attach_db_tools: supports_tools,
        fallback_schema_ddl: if supports_tools {
            None
        } else {
            fallback_schema_ddl
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_table_names_truncates_with_more() {
        let names: Vec<String> = (0..5).map(|i| format!("t{i}")).collect();
        let block = format_table_names_block(&names, 3);
        assert!(block.contains("t0"));
        assert!(block.contains("t2"));
        assert!(block.contains("and 2 more"));
        assert!(!block.contains("t3"));
    }

    #[test]
    fn compose_includes_pinned_and_tools_hint() {
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names: vec!["users".into(), "orders".into()],
            pinned_schema_ddl: "  users (id int PK)".into(),
            attach_db_tools: true,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("users"));
        assert!(text.contains("orders"));
        assert!(text.contains("users (id int PK)"));
        assert!(text.contains("get_table_schema"));
        assert!(!text.contains("FULL SCHEMA FALLBACK"));
    }

    #[test]
    fn decide_seed_fields_tools_on() {
        let seed = assemble_seed(
            "Postgres".into(),
            vec!["u".into()],
            "  u (id int)".into(),
            true,
            Some("SHOULD_NOT_USE".into()),
        );
        assert!(seed.attach_db_tools);
        assert!(seed.fallback_schema_ddl.is_none());
        assert_eq!(seed.pinned_schema_ddl, "  u (id int)");
    }

    #[test]
    fn decide_seed_fields_tools_off_keeps_fallback() {
        let seed = assemble_seed(
            "Postgres".into(),
            vec!["u".into()],
            String::new(),
            false,
            Some("  u (id int)".into()),
        );
        assert!(!seed.attach_db_tools);
        assert_eq!(seed.fallback_schema_ddl.as_deref(), Some("  u (id int)"));
    }

    #[test]
    fn compose_fallback_when_no_tools() {
        let seed = PromptSeed {
            database_type: "Mysql".into(),
            table_names: vec!["a".into()],
            pinned_schema_ddl: String::new(),
            attach_db_tools: false,
            fallback_schema_ddl: Some("  a (id int)\n  b (id int)".into()),
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("a (id int)"));
        assert!(!text.contains("get_table_schema"));
    }
}
