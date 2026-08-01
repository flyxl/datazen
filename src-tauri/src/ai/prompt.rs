//! Prompt templates for AI features.

use datazen_ai_api::{ChatMessage, MessageRole, SqlGenerationContext};

pub struct PromptBuilder;

impl PromptBuilder {
    pub fn nl2sql_system(context: &SqlGenerationContext) -> ChatMessage {
        let version_str = context
            .database_version
            .as_deref()
            .map(|v| format!(" {v}"))
            .unwrap_or_default();

        let recent = if context.recent_queries.is_empty() {
            String::new()
        } else {
            format!(
                "\n\nRecent queries (for style reference):\n{}",
                context
                    .recent_queries
                    .iter()
                    .map(|q| format!("- {q}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"You are a SQL expert. Generate executable SQL based on the user's natural language description and the database schema below.

Database: {db_type}{version}
Schema:
{schema}

Rules:
- Return ONLY executable SQL, no explanations or markdown
- Use the correct dialect for {db_type}
- Use table aliases for readability
- If the description is ambiguous, use the most common reasonable interpretation
- Reference only tables and columns that exist in the schema{recent}"#,
                db_type = context.database_type,
                version = version_str,
                schema = context.schema_ddl,
            ),
        }
    }

    pub fn diagnose_system(db_type: &str, schema_ddl: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"You are a database error diagnostician. Analyze SQL errors and provide fixes.

Database: {db_type}
Schema:
{schema_ddl}

Respond in this exact JSON format:
{{
  "explanation": "Clear explanation of why the error occurred",
  "suggestedSql": "Corrected SQL query (or null if unfixable)",
  "changes": ["Description of each change made"]
}}"#,
            ),
        }
    }

    pub fn explain_analysis_system(db_type: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"You are a database performance expert. Analyze the EXPLAIN output and identify bottlenecks.

Database: {db_type}

Respond in this exact JSON format:
{{
  "summary": "One-line performance summary",
  "bottlenecks": [
    {{"node": "Node name", "description": "Why it's slow", "severity": "high|medium|low"}}
  ],
  "suggestions": [
    {{"description": "What to do", "sql": "CREATE INDEX ... (or null)", "impact": "Expected improvement"}}
  ]
}}"#,
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nl2sql_system_prompt() {
        let context = SqlGenerationContext {
            database_type: "PostgreSQL".into(),
            database_version: Some("15.4".into()),
            schema_ddl: "  users (id int4 PK, name varchar)".into(),
            current_table: Some("users".into()),
            recent_queries: vec!["SELECT * FROM users".into()],
        };

        let msg = PromptBuilder::nl2sql_system(&context);
        assert_eq!(msg.role, MessageRole::System);
        assert!(msg.content.contains("PostgreSQL"));
        assert!(msg.content.contains("15.4"));
        assert!(msg.content.contains("users (id int4 PK"));
        assert!(msg.content.contains("SELECT * FROM users"));
        assert!(msg.content.contains("ONLY executable SQL"));
    }

    #[test]
    fn test_nl2sql_system_prompt_no_recent() {
        let context = SqlGenerationContext {
            database_type: "MySQL".into(),
            database_version: None,
            schema_ddl: "  orders (id int PK)".into(),
            current_table: None,
            recent_queries: vec![],
        };

        let msg = PromptBuilder::nl2sql_system(&context);
        assert!(!msg.content.contains("Recent queries"));
    }

    #[test]
    fn test_diagnose_system_prompt() {
        let msg = PromptBuilder::diagnose_system("PostgreSQL", "  users (id int4 PK)");
        assert_eq!(msg.role, MessageRole::System);
        assert!(msg.content.contains("error diagnostician"));
        assert!(msg.content.contains("PostgreSQL"));
        assert!(msg.content.contains("suggestedSql"));
    }

    #[test]
    fn test_explain_analysis_system_prompt() {
        let msg = PromptBuilder::explain_analysis_system("MySQL");
        assert_eq!(msg.role, MessageRole::System);
        assert!(msg.content.contains("performance expert"));
        assert!(msg.content.contains("MySQL"));
        assert!(msg.content.contains("bottlenecks"));
        assert!(msg.content.contains("suggestions"));
    }
}
