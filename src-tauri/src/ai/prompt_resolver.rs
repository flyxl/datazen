//! Prompt resolution with driver + user override support.
//!
//! Resolution order (highest priority first):
//! 1. User override for (driver_type, scenario) — exact match
//! 2. User override for (*, scenario) — global override
//! 3. Driver-specific prompt from `DatabaseDriver::prompt_overrides()`
//! 4. Built-in default from `PromptBuilder`

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use datazen_driver_api::{DatabaseDriver, PromptScenario};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// A single user prompt override entry persisted on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptOverrideEntry {
    /// `"*"` means global override for all driver types.
    pub driver_type: String,
    pub scenario: PromptScenario,
    pub system_zh: String,
    pub system_en: String,
}

/// Persisted file format for prompt overrides.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PromptOverridesFile {
    pub overrides: Vec<PromptOverrideEntry>,
}

/// Prompt metadata returned to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptInfo {
    pub scenario: PromptScenario,
    pub label: String,
    pub source: PromptSource,
    pub system_zh: String,
    pub system_en: String,
    pub default_zh: String,
    pub default_en: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PromptSource {
    Default,
    Driver,
    User,
}

pub struct PromptResolver {
    file_path: PathBuf,
    user_overrides: RwLock<Vec<PromptOverrideEntry>>,
}

impl PromptResolver {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            file_path: data_dir.join("prompt_overrides.json"),
            user_overrides: RwLock::new(Vec::new()),
        }
    }

    pub async fn load(&self) -> Result<(), String> {
        if !self.file_path.exists() {
            return Ok(());
        }
        let data = tokio::fs::read_to_string(&self.file_path)
            .await
            .map_err(|e| e.to_string())?;
        let file: PromptOverridesFile =
            serde_json::from_str(&data).map_err(|e| e.to_string())?;
        *self.user_overrides.write().await = file.overrides;
        Ok(())
    }

    async fn save(&self) -> Result<(), String> {
        let overrides = self.user_overrides.read().await.clone();
        let file = PromptOverridesFile { overrides };
        let json =
            serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        if let Some(parent) = self.file_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| e.to_string())?;
        }
        tokio::fs::write(&self.file_path, json)
            .await
            .map_err(|e| e.to_string())
    }

    /// Resolve the effective system prompt for a scenario.
    ///
    /// `driver_type_name` is e.g. `"PostgreSQL"`, `"MySQL"`, etc.
    pub async fn resolve(
        &self,
        scenario: PromptScenario,
        driver: Option<&dyn DatabaseDriver>,
        lang: &str,
    ) -> String {
        let driver_type_name = driver.and_then(|d| {
            serde_json::to_value(&d.driver_type())
                .ok()
                .and_then(|v| v.as_str().map(String::from))
        });

        let overrides = self.user_overrides.read().await;

        // 1. User override for (driver_type, scenario)
        if let Some(ref dt) = driver_type_name {
            if let Some(entry) = overrides
                .iter()
                .find(|o| o.driver_type == *dt && o.scenario == scenario)
            {
                return Self::select_lang(&entry.system_zh, &entry.system_en, lang);
            }
        }

        // 2. User override for (*, scenario)
        if let Some(entry) = overrides
            .iter()
            .find(|o| o.driver_type == "*" && o.scenario == scenario)
        {
            return Self::select_lang(&entry.system_zh, &entry.system_en, lang);
        }

        // 3. Driver-specific prompt
        if let Some(d) = driver {
            let driver_prompts = d.prompt_overrides();
            if let Some(tpl) = driver_prompts.get(&scenario) {
                return Self::select_lang(&tpl.system_zh, &tpl.system_en, lang);
            }
        }

        // 4. Built-in default
        let (zh, en) = default_prompt(scenario);
        Self::select_lang(&zh, &en, lang)
    }

    /// Get all prompt infos for a specific driver type (for settings UI).
    pub async fn list_prompts(
        &self,
        driver: Option<&dyn DatabaseDriver>,
    ) -> Vec<PromptInfo> {
        let driver_type_name = driver.and_then(|d| {
            serde_json::to_value(&d.driver_type())
                .ok()
                .and_then(|v| v.as_str().map(String::from))
        });
        let driver_prompts = driver
            .map(|d| d.prompt_overrides())
            .unwrap_or_default();
        let overrides = self.user_overrides.read().await;

        PromptScenario::all()
            .iter()
            .map(|&scenario| {
                let (default_zh, default_en) = default_prompt(scenario);
                let mut source = PromptSource::Default;
                let mut system_zh = default_zh.clone();
                let mut system_en = default_en.clone();

                // Check driver override
                if let Some(tpl) = driver_prompts.get(&scenario) {
                    source = PromptSource::Driver;
                    system_zh = tpl.system_zh.clone();
                    system_en = tpl.system_en.clone();
                }

                // Check user override (exact driver match)
                if let Some(ref dt) = driver_type_name {
                    if let Some(entry) = overrides
                        .iter()
                        .find(|o| o.driver_type == *dt && o.scenario == scenario)
                    {
                        source = PromptSource::User;
                        system_zh = entry.system_zh.clone();
                        system_en = entry.system_en.clone();
                    }
                }

                // Check user override (global)
                if source != PromptSource::User {
                    if let Some(entry) = overrides
                        .iter()
                        .find(|o| o.driver_type == "*" && o.scenario == scenario)
                    {
                        source = PromptSource::User;
                        system_zh = entry.system_zh.clone();
                        system_en = entry.system_en.clone();
                    }
                }

                PromptInfo {
                    scenario,
                    label: scenario.label().to_string(),
                    source,
                    system_zh,
                    system_en,
                    default_zh,
                    default_en,
                }
            })
            .collect()
    }

    /// Save a user prompt override.
    pub async fn set_override(&self, entry: PromptOverrideEntry) -> Result<(), String> {
        {
            let mut overrides = self.user_overrides.write().await;
            overrides.retain(|o| {
                !(o.driver_type == entry.driver_type && o.scenario == entry.scenario)
            });
            overrides.push(entry);
        }
        self.save().await
    }

    /// Remove a user prompt override, reverting to driver/default.
    pub async fn remove_override(
        &self,
        driver_type: &str,
        scenario: PromptScenario,
    ) -> Result<(), String> {
        {
            let mut overrides = self.user_overrides.write().await;
            overrides.retain(|o| {
                !(o.driver_type == driver_type && o.scenario == scenario)
            });
        }
        self.save().await
    }

    /// Get all user overrides (raw).
    pub async fn get_all_overrides(&self) -> Vec<PromptOverrideEntry> {
        self.user_overrides.read().await.clone()
    }

    fn select_lang(zh: &str, en: &str, lang: &str) -> String {
        if lang.starts_with("zh") {
            zh.to_string()
        } else {
            en.to_string()
        }
    }
}

/// Returns (zh, en) default prompts for each scenario.
///
/// These are the same prompts that were previously hardcoded in `PromptBuilder`,
/// but extracted as static templates. They use `{{variable}}` placeholders.
fn default_prompt(scenario: PromptScenario) -> (String, String) {
    match scenario {
        PromptScenario::Nl2Sql => (
            r#"你是一位 SQL 专家。根据用户的自然语言描述和下面的数据库 schema，生成可执行的 SQL。

Database: {{db_type}}{{version}}
Schema:
{{schema}}

规则：
- 仅返回可执行的 SQL，不要解释或 markdown
- 使用 {{db_type}} 的正确方言
- 使用表别名提高可读性
- 如果描述有歧义，使用最合理的常见解释
- 仅引用 schema 中存在的表和列{{recent}}"#
                .into(),
            r#"You are a SQL expert. Generate executable SQL based on the user's natural language description and the database schema below.

Database: {{db_type}}{{version}}
Schema:
{{schema}}

Rules:
- Return ONLY executable SQL, no explanations or markdown
- Use the correct dialect for {{db_type}}
- Use table aliases for readability
- If the description is ambiguous, use the most common reasonable interpretation
- Reference only tables and columns that exist in the schema{{recent}}"#
                .into(),
        ),
        PromptScenario::Diagnose => (
            r#"你是一位数据库错误诊断专家。分析 SQL 错误并提供修复方案。

Database: {{db_type}}
Schema:
{{schema}}

Respond in this exact JSON format:
{
  "explanation": "Clear explanation of why the error occurred",
  "suggestedSql": "Corrected SQL query (or null if unfixable)",
  "changes": ["Description of each change made"]
}"#
                .into(),
            r#"You are a database error diagnostician. Analyze SQL errors and provide fixes.

Database: {{db_type}}
Schema:
{{schema}}

Respond in this exact JSON format:
{
  "explanation": "Clear explanation of why the error occurred",
  "suggestedSql": "Corrected SQL query (or null if unfixable)",
  "changes": ["Description of each change made"]
}"#
                .into(),
        ),
        PromptScenario::NlFilter => (
            r#"你是一个筛选条件解析器。将自然语言描述转换为结构化的表数据筛选条件。

Database: {{db_type}}
Available columns:
{{columns}}

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
  {"column": "column_name", "operator": "eq", "value": "some_value"},
  {"column": "age", "operator": "gt", "value": 18}
]

规则：
- 仅使用上面 schema 中存在的列
- 为用户意图选择最合适的运算符
- 数值列使用数字值（不是字符串）
- "包含" 或 "含有" 使用 "like" 配合 %value%
- "以...开头" 使用 "like" 配合 value%
- "以...结尾" 使用 "like" 配合 %value
- 空值检查使用 "isNull" 或 "isNotNull"（无需 value 字段）
- "in" 运算符的 value 应为 JSON 数组
- 仅返回 JSON 数组，不要解释"#
                .into(),
            r#"You are a filter condition parser. Convert natural language descriptions into structured filter conditions for table data.

Database: {{db_type}}
Available columns:
{{columns}}

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
  {"column": "column_name", "operator": "eq", "value": "some_value"},
  {"column": "age", "operator": "gt", "value": 18}
]

Rules:
- Use ONLY columns that exist in the schema above
- Choose the most appropriate operator for the user's intent
- For numeric columns, use numeric values (not strings)
- For "contains" or "includes", use "like" with %value%
- For "starts with", use "like" with value%
- For "ends with", use "like" with %value
- For null checks, use "isNull" or "isNotNull" (no value field needed)
- For "in" operator, value should be a JSON array
- Return ONLY the JSON array, no explanations"#
                .into(),
        ),
        PromptScenario::SchemaDocSelectTables => (
            r#"You are a database documentation expert.

Database: {{db_type}}
Tables: {{table_names}}

From the table list above, select the most important user-created tables that should be documented. Exclude system/internal tables (e.g., pg_*, information_schema.*, sql_*, sqlite_*).
Return ONLY a JSON array of table names, no explanation.
Example: ["users", "orders", "products"]
If there are more than 30 important tables, pick the top 30."#
                .into(),
            r#"You are a database documentation expert.

Database: {{db_type}}
Tables: {{table_names}}

From the table list above, select the most important user-created tables that should be documented. Exclude system/internal tables (e.g., pg_*, information_schema.*, sql_*, sqlite_*).
Return ONLY a JSON array of table names, no explanation.
Example: ["users", "orders", "products"]
If there are more than 30 important tables, pick the top 30."#
                .into(),
        ),
        PromptScenario::SchemaDoc => (
            r#"你是一位数据库文档专家。为下面的数据库 schema 生成全面的文档。

Database: {{db_type}}
Schema:
{{schema}}

使用 Markdown 格式生成文档，包含：
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
- 简洁但全面"#
                .into(),
            r#"You are a database documentation expert. Generate comprehensive documentation for the database schema.

Database: {{db_type}}
Schema:
{{schema}}

Generate documentation in Markdown format with:
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
- Be concise but thorough"#
                .into(),
        ),
        PromptScenario::ConnectionDiagnose => (
            r#"你是一位数据库连接专家。诊断连接失败原因并提供可操作的解决方案。

Respond in this exact JSON format:
{
  "diagnosis": "Clear explanation of why the connection failed",
  "possibleCauses": ["Cause 1", "Cause 2"],
  "solutions": [
    {"description": "Step-by-step fix", "command": "optional shell/SQL command"}
  ],
  "category": "auth|network|config|server|driver"
}

常见类别：
- auth: 认证失败（密码错误、凭据过期、权限不足）
- network: 网络问题（超时、DNS、防火墙、端口被阻止）
- config: 配置错误（主机名、端口、数据库名、SSL 设置错误）
- server: 服务端问题（未运行、最大连接数、资源限制）
- driver: 客户端/驱动问题（版本不匹配、缺少库）"#
                .into(),
            r#"You are a database connectivity expert. Diagnose connection failures and provide actionable solutions.

Respond in this exact JSON format:
{
  "diagnosis": "Clear explanation of why the connection failed",
  "possibleCauses": ["Cause 1", "Cause 2"],
  "solutions": [
    {"description": "Step-by-step fix", "command": "optional shell/SQL command"}
  ],
  "category": "auth|network|config|server|driver"
}

Common categories:
- auth: authentication failures (wrong password, expired credentials, missing permissions)
- network: connectivity issues (timeout, DNS, firewall, port blocked)
- config: configuration errors (wrong host, port, database name, SSL settings)
- server: server-side issues (not running, max connections, resource limits)
- driver: client/driver issues (version mismatch, missing libraries)"#
                .into(),
        ),
        PromptScenario::QuerySummary => (
            r#"你是一位 SQL 查询分析师。分析 SQL 查询列表并提供洞察。

Respond in this exact JSON format:
{
  "summary": "Brief overview of query patterns",
  "categories": [
    {"name": "Category name", "count": 5, "examples": ["SELECT ...", "UPDATE ..."]}
  ],
  "insights": [
    "Observation about query patterns",
    "Performance concern or optimization suggestion"
  ],
  "frequentTables": ["table1", "table2"],
  "recommendations": ["Recommendation 1", "Recommendation 2"]
}

规则：
- 按类型分组查询（SELECT、INSERT、UPDATE、DELETE、DDL）
- 识别最常访问的表
- 注意潜在的性能问题（缺少 WHERE、SELECT * 等）
- 建议要可操作且具体"#
                .into(),
            r#"You are a SQL query analyst. Analyze a list of SQL queries and provide insights.

Respond in this exact JSON format:
{
  "summary": "Brief overview of query patterns",
  "categories": [
    {"name": "Category name", "count": 5, "examples": ["SELECT ...", "UPDATE ..."]}
  ],
  "insights": [
    "Observation about query patterns",
    "Performance concern or optimization suggestion"
  ],
  "frequentTables": ["table1", "table2"],
  "recommendations": ["Recommendation 1", "Recommendation 2"]
}

Rules:
- Group queries by type (SELECT, INSERT, UPDATE, DELETE, DDL)
- Identify the most frequently accessed tables
- Note any potential performance issues (missing WHERE, SELECT *, etc.)
- Keep recommendations actionable and specific"#
                .into(),
        ),
        PromptScenario::ExplainAnalysis => (
            r#"你是一位数据库性能专家。分析 EXPLAIN 输出并识别瓶颈。

Database: {{db_type}}

Respond in this exact JSON format:
{
  "summary": "One-line performance summary",
  "bottlenecks": [
    {"node": "Node name", "description": "Why it's slow", "severity": "high|medium|low"}
  ],
  "suggestions": [
    {"description": "What to do", "sql": "CREATE INDEX ... (or null)", "impact": "Expected improvement"}
  ]
}"#
                .into(),
            r#"You are a database performance expert. Analyze the EXPLAIN output and identify bottlenecks.

Database: {{db_type}}

Respond in this exact JSON format:
{
  "summary": "One-line performance summary",
  "bottlenecks": [
    {"node": "Node name", "description": "Why it's slow", "severity": "high|medium|low"}
  ],
  "suggestions": [
    {"description": "What to do", "sql": "CREATE INDEX ... (or null)", "impact": "Expected improvement"}
  ]
}"#
                .into(),
        ),
        PromptScenario::Chat => (
            "你是一个有用的数据库助手。帮助用户处理 SQL 查询、数据库概念和数据分析。编写 SQL 时请使用正确的格式并解释你的思路。".into(),
            "You are a helpful database assistant. Help the user with SQL queries, database concepts, and data analysis. When writing SQL, use proper formatting and explain your reasoning.".into(),
        ),
    }
}

/// Replace `{{key}}` placeholders in a template.
pub fn render_template(template: &str, vars: &HashMap<&str, &str>) -> String {
    let mut result = template.to_string();
    for (&key, &val) in vars {
        result = result.replace(&format!("{{{{{key}}}}}"), val);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_prompt_covers_all_scenarios() {
        for scenario in PromptScenario::all() {
            let (zh, en) = default_prompt(*scenario);
            assert!(!zh.is_empty(), "{scenario:?} zh prompt is empty");
            assert!(!en.is_empty(), "{scenario:?} en prompt is empty");
        }
    }

    #[test]
    fn test_render_template() {
        let template = "Database: {{db_type}}\nSchema:\n{{schema}}";
        let mut vars = HashMap::new();
        vars.insert("db_type", "PostgreSQL");
        vars.insert("schema", "users (id int PK)");
        let result = render_template(template, &vars);
        assert!(result.contains("PostgreSQL"));
        assert!(result.contains("users (id int PK)"));
    }

    #[test]
    fn test_render_template_missing_var() {
        let template = "DB: {{db_type}} Version: {{version}}";
        let mut vars = HashMap::new();
        vars.insert("db_type", "MySQL");
        let result = render_template(template, &vars);
        assert!(result.contains("MySQL"));
        assert!(result.contains("{{version}}"));
    }

    #[tokio::test]
    async fn test_resolver_default_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let resolver = PromptResolver::new(tmp.path());
        let result = resolver.resolve(PromptScenario::Nl2Sql, None, "en").await;
        assert!(result.contains("SQL expert"));
    }

    #[tokio::test]
    async fn test_resolver_user_override() {
        let tmp = tempfile::tempdir().unwrap();
        let resolver = PromptResolver::new(tmp.path());

        resolver
            .set_override(PromptOverrideEntry {
                driver_type: "*".into(),
                scenario: PromptScenario::Chat,
                system_zh: "自定义中文".into(),
                system_en: "Custom English".into(),
            })
            .await
            .unwrap();

        let result = resolver.resolve(PromptScenario::Chat, None, "zh-CN").await;
        assert_eq!(result, "自定义中文");

        let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
        assert_eq!(result, "Custom English");
    }

    #[tokio::test]
    async fn test_resolver_remove_override() {
        let tmp = tempfile::tempdir().unwrap();
        let resolver = PromptResolver::new(tmp.path());

        resolver
            .set_override(PromptOverrideEntry {
                driver_type: "*".into(),
                scenario: PromptScenario::Chat,
                system_zh: "自定义".into(),
                system_en: "Custom".into(),
            })
            .await
            .unwrap();

        resolver
            .remove_override("*", PromptScenario::Chat)
            .await
            .unwrap();

        let result = resolver.resolve(PromptScenario::Chat, None, "en").await;
        assert!(result.contains("database assistant"));
    }

    #[tokio::test]
    async fn test_resolver_persist_and_reload() {
        let tmp = tempfile::tempdir().unwrap();

        {
            let resolver = PromptResolver::new(tmp.path());
            resolver
                .set_override(PromptOverrideEntry {
                    driver_type: "PostgreSQL".into(),
                    scenario: PromptScenario::Nl2Sql,
                    system_zh: "PG专属".into(),
                    system_en: "PG specific".into(),
                })
                .await
                .unwrap();
        }

        {
            let resolver = PromptResolver::new(tmp.path());
            resolver.load().await.unwrap();
            let overrides = resolver.get_all_overrides().await;
            assert_eq!(overrides.len(), 1);
            assert_eq!(overrides[0].driver_type, "PostgreSQL");
        }
    }

    #[tokio::test]
    async fn test_list_prompts() {
        let tmp = tempfile::tempdir().unwrap();
        let resolver = PromptResolver::new(tmp.path());
        let prompts = resolver.list_prompts(None).await;
        assert_eq!(prompts.len(), PromptScenario::all().len());
        for p in &prompts {
            assert_eq!(p.source, PromptSource::Default);
            assert!(!p.default_zh.is_empty());
            assert!(!p.default_en.is_empty());
        }
    }
}
