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

const LARGE_TABLE_THRESHOLD: usize = 500;

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
        if seed.table_names.len() > LARGE_TABLE_THRESHOLD {
            out.push_str(
                "\nThis database has many tables. Use search_tables with a keyword to find relevant tables first, then get_table_schema for the ones you need.\n",
            );
        } else {
            out.push_str(
                "\nUse list_tables / get_table_schema tools to fetch schemas for tables you need beyond the pinned set.\n",
            );
        }
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
        db_session_id: &str,
        database: &str,
        pinned_tables: &[String],
        supports_tools: bool,
        pinned_budget: usize,
        fallback_budget: usize,
    ) -> Result<PromptSeed, String> {
        let (db_type, table_names) = self
            .builder
            .get_table_names(db_session_id, database)
            .await
            .unwrap_or_else(|_| (String::new(), Vec::new()));

        let pinned_schema_ddl = if pinned_tables.is_empty() {
            String::new()
        } else {
            match self
                .builder
                .build_selective_context(db_session_id, database, pinned_tables, pinned_budget)
                .await
            {
                Ok(c) => c.schema_ddl,
                Err(e) => {
                    tracing::warn!(
                        db_session_id = %db_session_id,
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
                .build_sql_context(db_session_id, database, None, &[], fallback_budget)
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
    fn compose_suggests_search_tables_for_large_table_count() {
        let table_names: Vec<String> = (0..600).map(|i| format!("table_{i}")).collect();
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names,
            pinned_schema_ddl: String::new(),
            attach_db_tools: true,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("search_tables"));
        assert!(!text.contains("list_tables"));
    }

    #[test]
    fn compose_suggests_list_tables_for_small_table_count() {
        let table_names: Vec<String> = (0..50).map(|i| format!("table_{i}")).collect();
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names,
            pinned_schema_ddl: String::new(),
            attach_db_tools: true,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("list_tables"));
        assert!(!text.contains("search_tables"));
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

    #[test]
    fn compose_at_threshold_boundary_uses_list_tables() {
        let table_names: Vec<String> = (0..LARGE_TABLE_THRESHOLD)
            .map(|i| format!("t{i}"))
            .collect();
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names,
            pinned_schema_ddl: String::new(),
            attach_db_tools: true,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("list_tables"));
        assert!(!text.contains("search_tables"));
    }

    #[test]
    fn compose_just_above_threshold_uses_search_tables() {
        let table_names: Vec<String> = (0..=LARGE_TABLE_THRESHOLD)
            .map(|i| format!("t{i}"))
            .collect();
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names,
            pinned_schema_ddl: String::new(),
            attach_db_tools: true,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("search_tables"));
        assert!(!text.contains("list_tables"));
    }

    #[test]
    fn compose_empty_tables_with_tools() {
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names: vec![],
            pinned_schema_ddl: String::new(),
            attach_db_tools: true,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("(no tables)"));
        assert!(text.contains("list_tables"));
    }

    #[test]
    fn compose_no_tools_no_fallback() {
        let seed = PromptSeed {
            database_type: "Postgres".into(),
            table_names: vec!["a".into()],
            pinned_schema_ddl: String::new(),
            attach_db_tools: false,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("Postgres"));
        assert!(!text.contains("list_tables"));
        assert!(!text.contains("search_tables"));
        assert!(!text.contains("Schema:"));
    }

    #[test]
    fn compose_pinned_only_no_tools() {
        let seed = PromptSeed {
            database_type: "Mysql".into(),
            table_names: vec!["users".into(), "orders".into()],
            pinned_schema_ddl: "CREATE TABLE users (id INT)".into(),
            attach_db_tools: false,
            fallback_schema_ddl: None,
        };
        let text = compose_schema_system_suffix(&seed);
        assert!(text.contains("Pinned table schemas"));
        assert!(text.contains("CREATE TABLE users"));
        assert!(!text.contains("list_tables"));
    }

    #[test]
    fn format_table_names_all_fit() {
        let names: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
        let block = format_table_names_block(&names, 5);
        assert_eq!(block, "a, b, c");
        assert!(!block.contains("more"));
    }

    #[test]
    fn format_table_names_empty() {
        let names: Vec<String> = vec![];
        let block = format_table_names_block(&names, 5);
        assert_eq!(block, "(no tables)");
    }

    #[test]
    fn format_table_names_exact_limit() {
        let names: Vec<String> = (0..3).map(|i| format!("t{i}")).collect();
        let block = format_table_names_block(&names, 3);
        assert_eq!(block, "t0, t1, t2");
    }
}
