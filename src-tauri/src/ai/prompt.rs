//! Prompt templates for AI features.

use datazen_ai_api::{ChatMessage, MessageRole, SqlGenerationContext};

pub struct PromptBuilder;

fn is_zh(lang: &str) -> bool {
    lang.starts_with("zh")
}

impl PromptBuilder {
    pub fn nl2sql_system(context: &SqlGenerationContext, lang: &str) -> ChatMessage {
        let version_str = context
            .database_version
            .as_deref()
            .map(|v| format!(" {v}"))
            .unwrap_or_default();

        let recent = if context.recent_queries.is_empty() {
            String::new()
        } else {
            let label = if is_zh(lang) { "近期查询（供风格参考）" } else { "Recent queries (for style reference)" };
            format!(
                "\n\n{label}:\n{}",
                context
                    .recent_queries
                    .iter()
                    .map(|q| format!("- {q}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };

        let (role_desc, rules) = if is_zh(lang) {
            (
                "你是一位 SQL 专家。根据用户的自然语言描述和下面的数据库 schema，生成可执行的 SQL。",
                format!(
                    r#"规则：
- 仅返回可执行的 SQL，不要解释或 markdown
- 使用 {db_type} 的正确方言
- 使用表别名提高可读性
- 如果描述有歧义，使用最合理的常见解释
- 仅引用 schema 中存在的表和列"#,
                    db_type = context.database_type
                ),
            )
        } else {
            (
                "You are a SQL expert. Generate executable SQL based on the user's natural language description and the database schema below.",
                format!(
                    r#"Rules:
- Return ONLY executable SQL, no explanations or markdown
- Use the correct dialect for {db_type}
- Use table aliases for readability
- If the description is ambiguous, use the most common reasonable interpretation
- Reference only tables and columns that exist in the schema"#,
                    db_type = context.database_type
                ),
            )
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                "{role_desc}\n\nDatabase: {db_type}{version}\nSchema:\n{schema}\n\n{rules}{recent}",
                db_type = context.database_type,
                version = version_str,
                schema = context.schema_ddl,
            ),
        }
    }

    pub fn diagnose_system(db_type: &str, schema_ddl: &str, lang: &str) -> ChatMessage {
        let desc = if is_zh(lang) {
            "你是一位数据库错误诊断专家。分析 SQL 错误并提供修复方案。"
        } else {
            "You are a database error diagnostician. Analyze SQL errors and provide fixes."
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"{desc}

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

    pub fn nl_filter_system(db_type: &str, columns_ddl: &str, lang: &str) -> ChatMessage {
        let (desc, rules) = if is_zh(lang) {
            (
                "你是一个筛选条件解析器。将自然语言描述转换为结构化的表数据筛选条件。",
                r#"规则：
- 仅使用上面 schema 中存在的列
- 为用户意图选择最合适的运算符
- 数值列使用数字值（不是字符串）
- "包含" 或 "含有" 使用 "like" 配合 %value%
- "以...开头" 使用 "like" 配合 value%
- "以...结尾" 使用 "like" 配合 %value
- 空值检查使用 "isNull" 或 "isNotNull"（无需 value 字段）
- "in" 运算符的 value 应为 JSON 数组
- 仅返回 JSON 数组，不要解释"#,
            )
        } else {
            (
                "You are a filter condition parser. Convert natural language descriptions into structured filter conditions for table data.",
                r#"Rules:
- Use ONLY columns that exist in the schema above
- Choose the most appropriate operator for the user's intent
- For numeric columns, use numeric values (not strings)
- For "contains" or "includes", use "like" with %value%
- For "starts with", use "like" with value%
- For "ends with", use "like" with %value
- For null checks, use "isNull" or "isNotNull" (no value field needed)
- For "in" operator, value should be a JSON array
- Return ONLY the JSON array, no explanations"#,
            )
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"{desc}

Database: {db_type}
Available columns:
{columns_ddl}

Each filter condition must be one of these operators:
- eq: equals
- ne: not equals
- gt: greater than
- lt: less than
- gte: greater than or equal
- lte: less than or equal
- like: pattern matching (use % as wildcard)
- in: value in list
- isNull: value is null
- isNotNull: value is not null

Respond in this exact JSON format (an array of filter conditions):
[
  {{"column": "column_name", "operator": "eq", "value": "some_value"}},
  {{"column": "age", "operator": "gt", "value": 18}}
]

{rules}"#,
            ),
        }
    }

    pub fn schema_doc_select_tables(db_type: &str, table_names: &[String]) -> ChatMessage {
        let names_list = table_names.join(", ");
        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"You are a database documentation expert.

Database: {db_type}
Tables: {names_list}

From the table list above, select the most important user-created tables that should be documented. Exclude system/internal tables (e.g., pg_*, information_schema.*, sql_*, sqlite_*).
Return ONLY a JSON array of table names, no explanation.
Example: ["users", "orders", "products"]
If there are more than 30 important tables, pick the top 30."#,
            ),
        }
    }

    pub fn schema_doc_system(db_type: &str, schema_ddl: &str, lang: &str) -> ChatMessage {
        let (desc, format_guide) = if is_zh(lang) {
            (
                "你是一位数据库文档专家。为下面的数据库 schema 生成全面的文档。",
                r#"使用 Markdown 格式生成文档，包含：
1. **概述** — 简要描述此数据库/schema 的用途
2. **表** — 每个表包括：
   - 用途和描述
   - 列说明（从名称、类型和关系推断含义）
   - 主键和约束
   - 关系（外键、引用表）
3. **实体关系** — 描述表之间的关系
4. **备注** — 命名规范、模式或潜在问题的观察

规则：
- 撰写清晰、专业的文档
- 当含义不明显时，从列名和类型推断用途
- 使用 Markdown 格式（标题、表格、列表）
- 简洁但全面"#,
            )
        } else {
            (
                "You are a database documentation expert. Generate comprehensive documentation for the database schema.",
                r#"Generate documentation in Markdown format with:
1. **Overview** — Brief description of what this database/schema is likely used for
2. **Tables** — For each table:
   - Purpose and description
   - Column descriptions (infer meaning from names, types, and relationships)
   - Primary keys and constraints
   - Relationships (foreign keys, referenced tables)
3. **Entity Relationships** — Describe relationships between tables
4. **Notes** — Any observations about naming conventions, patterns, or potential issues

Rules:
- Write clear, professional documentation
- Infer purpose from column names and types when not obvious
- Use Markdown formatting with headers, tables, and lists
- Be concise but thorough"#,
            )
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                "{desc}\n\nDatabase: {db_type}\nSchema:\n{schema_ddl}\n\n{format_guide}",
            ),
        }
    }

    pub fn connection_diagnose_system(lang: &str) -> ChatMessage {
        let desc = if is_zh(lang) {
            "你是一位数据库连接专家。诊断连接失败原因并提供可操作的解决方案。"
        } else {
            "You are a database connectivity expert. Diagnose connection failures and provide actionable solutions."
        };

        let categories = if is_zh(lang) {
            r#"常见类别：
- auth: 认证失败（密码错误、凭据过期、权限不足）
- network: 网络问题（超时、DNS、防火墙、端口被阻止）
- config: 配置错误（主机名、端口、数据库名、SSL 设置错误）
- server: 服务端问题（未运行、最大连接数、资源限制）
- driver: 客户端/驱动问题（版本不匹配、缺少库）"#
        } else {
            r#"Common categories:
- auth: authentication failures (wrong password, expired credentials, missing permissions)
- network: connectivity issues (timeout, DNS, firewall, port blocked)
- config: configuration errors (wrong host, port, database name, SSL settings)
- server: server-side issues (not running, max connections, resource limits)
- driver: client/driver issues (version mismatch, missing libraries)"#
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"{desc}

Respond in this exact JSON format:
{{
  "diagnosis": "Clear explanation of why the connection failed",
  "possibleCauses": ["Cause 1", "Cause 2"],
  "solutions": [
    {{"description": "Step-by-step fix", "command": "optional shell/SQL command"}}
  ],
  "category": "auth|network|config|server|driver"
}}

{categories}"#,
            ),
        }
    }

    pub fn query_summary_system(lang: &str) -> ChatMessage {
        let desc = if is_zh(lang) {
            "你是一位 SQL 查询分析师。分析 SQL 查询列表并提供洞察。"
        } else {
            "You are a SQL query analyst. Analyze a list of SQL queries and provide insights."
        };

        let rules = if is_zh(lang) {
            r#"规则：
- 按类型分组查询（SELECT、INSERT、UPDATE、DELETE、DDL）
- 识别最常访问的表
- 注意潜在的性能问题（缺少 WHERE、SELECT * 等）
- 建议要可操作且具体"#
        } else {
            r#"Rules:
- Group queries by type (SELECT, INSERT, UPDATE, DELETE, DDL)
- Identify the most frequently accessed tables
- Note any potential performance issues (missing WHERE, SELECT *, etc.)
- Keep recommendations actionable and specific"#
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"{desc}

Respond in this exact JSON format:
{{
  "summary": "Brief overview of query patterns",
  "categories": [
    {{"name": "Category name", "count": 5, "examples": ["SELECT ...", "UPDATE ..."]}},
  ],
  "insights": [
    "Observation about query patterns",
    "Performance concern or optimization suggestion"
  ],
  "frequentTables": ["table1", "table2"],
  "recommendations": ["Recommendation 1", "Recommendation 2"]
}}

{rules}"#,
            ),
        }
    }

    pub fn explain_analysis_system(db_type: &str, lang: &str) -> ChatMessage {
        let desc = if is_zh(lang) {
            "你是一位数据库性能专家。分析 EXPLAIN 输出并识别瓶颈。"
        } else {
            "You are a database performance expert. Analyze the EXPLAIN output and identify bottlenecks."
        };

        ChatMessage {
            role: MessageRole::System,
            content: format!(
                r#"{desc}

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
    fn test_nl2sql_system_prompt_zh() {
        let context = SqlGenerationContext {
            database_type: "PostgreSQL".into(),
            database_version: Some("15.4".into()),
            schema_ddl: "  users (id int4 PK, name varchar)".into(),
            current_table: Some("users".into()),
            recent_queries: vec!["SELECT * FROM users".into()],
        };

        let msg = PromptBuilder::nl2sql_system(&context, "zh-CN");
        assert_eq!(msg.role, MessageRole::System);
        assert!(msg.content.contains("PostgreSQL"));
        assert!(msg.content.contains("SQL 专家"));
        assert!(msg.content.contains("近期查询"));
    }

    #[test]
    fn test_nl2sql_system_prompt_en() {
        let context = SqlGenerationContext {
            database_type: "MySQL".into(),
            database_version: None,
            schema_ddl: "  orders (id int PK)".into(),
            current_table: None,
            recent_queries: vec![],
        };

        let msg = PromptBuilder::nl2sql_system(&context, "en");
        assert!(msg.content.contains("SQL expert"));
        assert!(!msg.content.contains("Recent queries"));
    }

    #[test]
    fn test_diagnose_system_prompt() {
        let msg = PromptBuilder::diagnose_system("PostgreSQL", "  users (id int4 PK)", "en");
        assert_eq!(msg.role, MessageRole::System);
        assert!(msg.content.contains("diagnostician"));
        assert!(msg.content.contains("suggestedSql"));
    }

    #[test]
    fn test_explain_analysis_system_prompt() {
        let msg = PromptBuilder::explain_analysis_system("MySQL", "zh-CN");
        assert!(msg.content.contains("性能专家"));
        assert!(msg.content.contains("bottlenecks"));
    }

    #[test]
    fn test_schema_doc_system_prompt() {
        let msg = PromptBuilder::schema_doc_system(
            "PostgreSQL",
            "  users (id int4 PK, name varchar)",
            "zh-CN",
        );
        assert!(msg.content.contains("文档专家"));
        assert!(msg.content.contains("实体关系"));
    }

    #[test]
    fn test_connection_diagnose_system_prompt() {
        let msg = PromptBuilder::connection_diagnose_system("en");
        assert!(msg.content.contains("connectivity expert"));
        assert!(msg.content.contains("possibleCauses"));
    }

    #[test]
    fn test_query_summary_system_prompt() {
        let msg = PromptBuilder::query_summary_system("zh-CN");
        assert!(msg.content.contains("查询分析师"));
        assert!(msg.content.contains("frequentTables"));
    }
}
