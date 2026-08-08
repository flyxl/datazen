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
    format!(
        "{}, …and {} more",
        head.join(", "),
        names.len() - max_names
    )
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

    // resolve implemented in Task 2
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
