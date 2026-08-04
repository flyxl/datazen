//! End-to-end integration tests for AI features.
//!
//! These tests require a real LLM API and are skipped if `.env.test` is absent.
//! Run with: `cargo test -p datazen --test ai_e2e -- --nocapture --test-threads=1`
//!
//! Tests must run sequentially (`--test-threads=1`) to avoid API rate limiting.

use std::path::PathBuf;

/// Strip markdown fences from an AI response (e.g. ```json ... ```).
fn strip_fences(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        let inner = rest
            .strip_prefix("json")
            .or_else(|| rest.strip_prefix("JSON"))
            .unwrap_or(rest);
        if let Some(end) = inner.rfind("```") {
            return inner[..end].trim().to_string();
        }
    }
    trimmed.to_string()
}

struct TestConfig {
    provider: String,
    endpoint: String,
    api_key: String,
    model: String,
    max_tokens: u32,
}

struct DeepSeekTestConfig {
    api_key: String,
    model: String,
    max_tokens: u32,
}

fn read_env_file() -> Option<std::collections::HashMap<String, String>> {
    let env_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".env.test");

    if !env_path.exists() {
        eprintln!("⏭  Skipping AI E2E tests: .env.test not found");
        return None;
    }

    let content = std::fs::read_to_string(&env_path).ok()?;
    let mut vars = std::collections::HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            vars.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    Some(vars)
}

fn load_test_config() -> Option<TestConfig> {
    let vars = read_env_file()?;
    let provider = vars.get("AI_PROVIDER").cloned().unwrap_or_default();
    let model = vars.get("AI_MODEL").cloned().unwrap_or_default();

    if provider.is_empty() || model.is_empty() {
        eprintln!("⏭  Skipping: incomplete .env.test (AI_PROVIDER/AI_MODEL)");
        return None;
    }

    Some(TestConfig {
        provider,
        endpoint: vars.get("AI_ENDPOINT").cloned().unwrap_or_default(),
        api_key: vars.get("AI_API_KEY").cloned().unwrap_or_default(),
        model,
        max_tokens: vars.get("AI_MAX_TOKENS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096),
    })
}

fn load_deepseek_config() -> Option<DeepSeekTestConfig> {
    let vars = read_env_file()?;
    let api_key = vars.get("DEEPSEEK_API_KEY").cloned().unwrap_or_default();

    if api_key.is_empty() {
        eprintln!("⏭  Skipping DeepSeek tests: DEEPSEEK_API_KEY not set in .env.test");
        return None;
    }

    Some(DeepSeekTestConfig {
        api_key,
        model: vars.get("DEEPSEEK_MODEL").cloned().unwrap_or_else(|| "deepseek-v4-flash".into()),
        max_tokens: vars.get("DEEPSEEK_MAX_TOKENS")
            .and_then(|v| v.parse().ok())
            .unwrap_or(200_000),
    })
}

mod provider_tests {
    use super::*;
    use datazen_ai_api::*;

    async fn create_provider(
        provider_type: &str,
        endpoint: &str,
        api_key: &str,
        max_tokens: u32,
    ) -> Box<dyn AiProvider> {
        match provider_type {
            "open_ai" => {
                let p = datazen::ai::openai::OpenAiProvider::new();
                let config = AiProviderConfig {
                    provider_type: AiProviderType::OpenAi,
                    api_key: Some(api_key.to_string()),
                    endpoint: Some(endpoint.to_string()),
                    model: String::new(),
                    max_tokens,
                    extra: serde_json::Value::Null,
                };
                p.initialize(&config).await.expect("Failed to initialize");
                Box::new(p)
            }
            "deep_seek" => {
                let p = datazen::ai::deepseek::DeepSeekProvider::new();
                let config = AiProviderConfig {
                    provider_type: AiProviderType::DeepSeek,
                    api_key: Some(api_key.to_string()),
                    endpoint: Some(endpoint.to_string()),
                    model: String::new(),
                    max_tokens,
                    extra: serde_json::Value::Null,
                };
                p.initialize(&config).await.expect("Failed to initialize DeepSeek");
                Box::new(p)
            }
            _ => panic!("Unsupported provider: {provider_type}"),
        }
    }

    async fn complete_with_retry(
        provider: &dyn AiProvider,
        request: &CompletionRequest,
        max_retries: u32,
    ) -> Result<CompletionResponse, AiError> {
        for attempt in 0..=max_retries {
            match provider.complete(request).await {
                Ok(resp) => return Ok(resp),
                Err(AiError::RateLimited { retry_after_secs }) => {
                    if attempt < max_retries {
                        let wait = retry_after_secs.max(1);
                        eprintln!("  ⏳ Rate limited, waiting {wait}s (attempt {}/{})", attempt + 1, max_retries);
                        tokio::time::sleep(std::time::Duration::from_secs(wait)).await;
                    } else {
                        return Err(AiError::RateLimited { retry_after_secs });
                    }
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    // ─── Phase 0: Provider validation ───

    #[tokio::test]
    async fn test_phase0_provider_validate_and_complete() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let request = CompletionRequest {
            request_id: "test-phase0".into(),
            model: cfg.model.clone(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Reply with exactly the word: OK".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.0),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let resp = complete_with_retry(provider.as_ref(), &request, 3).await
            .expect("complete() failed after retries");
        eprintln!("  Phase 0 response: {:?}", resp.content);
        eprintln!("  Phase 0 reasoning: {:?}", resp.reasoning);
        assert!(
            !resp.content.is_empty() || resp.reasoning.is_some(),
            "Response should have content or reasoning"
        );
    }

    // ─── Phase 1: NL2SQL ───

    #[tokio::test]
    async fn test_phase1_nl2sql_generation() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let context = SqlGenerationContext {
            database_type: "PostgreSQL".into(),
            database_version: Some("15.0".into()),
            schema_ddl: "  users (id int4 PK, name varchar, email varchar, created_at timestamp)\n  orders (id int4 PK, user_id int4 FK→users, amount numeric, status varchar)"
                .into(),
            current_table: Some("users".into()),
            recent_queries: vec![],
        };

        let system = datazen::ai::PromptBuilder::nl2sql_system(&context, "en");
        let request = CompletionRequest {
            request_id: "test-nl2sql".into(),
            model: cfg.model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "查询所有用户的姓名和邮箱".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.0),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3).await.expect("NL2SQL failed");
        let sql = response.content.to_uppercase();
        eprintln!("  Phase 1 NL2SQL: {}", response.content);
        assert!(sql.contains("SELECT"), "Response should contain SELECT");
        assert!(
            sql.contains("USER") || sql.contains("NAME"),
            "Response should reference user/name"
        );
    }

    // ─── Phase 1: SQL error diagnosis ───

    #[tokio::test]
    async fn test_phase1_sql_diagnosis() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let system = datazen::ai::PromptBuilder::diagnose_system(
            "PostgreSQL",
            "  users (id int4 PK, name varchar, email varchar)",
            "en",
        );
        let request = CompletionRequest {
            request_id: "test-diagnose".into(),
            model: cfg.model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "SQL:\n```\nSELECT * FROM user WHERE name = 'test'\n```\n\nError:\nERROR: relation \"user\" does not exist".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.0),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3).await.expect("Diagnosis failed");
        eprintln!("  Phase 1 Diagnosis: {}", response.content);

        let json_str = strip_fences(&response.content);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
        assert!(
            parsed.is_ok(),
            "Diagnosis response should be valid JSON: {}",
            json_str
        );

        let val = parsed.unwrap();
        assert!(val.get("explanation").is_some(), "Should have explanation");
        assert!(
            val.get("suggestedSql").is_some(),
            "Should have suggestedSql"
        );
    }

    // ─── Phase 2: EXPLAIN analysis ───

    #[tokio::test]
    async fn test_phase2_explain_analysis() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let system = datazen::ai::PromptBuilder::explain_analysis_system("PostgreSQL", "en");
        let request = CompletionRequest {
            request_id: "test-explain".into(),
            model: cfg.model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "EXPLAIN output:\nSeq Scan on users  (cost=0.00..1.05 rows=5 width=556)\n\nSQL: SELECT * FROM users WHERE name = 'test'".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.0),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("EXPLAIN analysis failed");
        eprintln!("  Phase 2 EXPLAIN: {}", response.content);

        let json_str = strip_fences(&response.content);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
        assert!(
            parsed.is_ok(),
            "EXPLAIN response should be valid JSON: {}",
            json_str
        );

        let val = parsed.unwrap();
        assert!(val.get("summary").is_some(), "Should have summary");
        assert!(val.get("bottlenecks").is_some(), "Should have bottlenecks");
        assert!(val.get("suggestions").is_some(), "Should have suggestions");
    }

    // ─── Phase 7: Smart filter ───

    #[tokio::test]
    async fn test_phase7_smart_filter() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let system = datazen::ai::PromptBuilder::nl_filter_system(
            "PostgreSQL",
            "  id int4 NOT NULL PK\n  name varchar NULL\n  age int4 NULL\n  status varchar NULL",
            "en",
        );
        let request = CompletionRequest {
            request_id: "test-filter".into(),
            model: cfg.model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "年龄大于18岁且状态为active的用户".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.0),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Smart filter failed");
        eprintln!("  Phase 7 Filter: {}", response.content);

        let json_str = strip_fences(&response.content);

        let parsed: Result<Vec<serde_json::Value>, _> = serde_json::from_str(&json_str);
        assert!(
            parsed.is_ok(),
            "Filter response should be valid JSON array: {}",
            json_str
        );

        let filters = parsed.unwrap();
        assert!(!filters.is_empty(), "Should have at least one filter");

        let has_age = filters.iter().any(|f| {
            f.get("column")
                .and_then(|v| v.as_str())
                .map(|s| s == "age")
                .unwrap_or(false)
        });
        assert!(has_age, "Should have a filter on 'age' column");
    }

    // ─── Phase 8: Schema documentation ───

    #[tokio::test]
    async fn test_phase8_schema_doc() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let system = datazen::ai::PromptBuilder::schema_doc_system(
            "PostgreSQL",
            "  users (id int4 PK, name varchar NOT NULL, email varchar NOT NULL UNIQUE)\n  orders (id int4 PK, user_id int4 NOT NULL, amount numeric, created_at timestamp DEFAULT now())",
            "en",
        );
        let request = CompletionRequest {
            request_id: "test-schema-doc".into(),
            model: cfg.model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "Generate documentation for the database schema above.".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.3),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Schema doc generation failed");
        let preview_end = {
            let max = 200;
            let mut end = max.min(response.content.len());
            while end > 0 && !response.content.is_char_boundary(end) {
                end -= 1;
            }
            end
        };
        eprintln!("  Phase 8 Schema Doc (first 200 chars): {}", &response.content[..preview_end]);

        let content = response.content.to_lowercase();
        assert!(content.contains("user"), "Doc should mention users table");
        assert!(content.contains("order"), "Doc should mention orders table");
    }

    // ─── Phase 8: Connection diagnosis ───

    #[tokio::test]
    async fn test_phase8_connection_diagnosis() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let system = datazen::ai::PromptBuilder::connection_diagnose_system("en");
        let request = CompletionRequest {
            request_id: "test-conn-diag".into(),
            model: cfg.model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "Connection details:\nConnection type: PostgreSQL\nHost: localhost\nPort: 5432\nDatabase: mydb\nUsername: admin\nSSL: Disable\nSSH Tunnel: disabled\nTimeout: 10s\n\nError:\nconnection refused (os error 111)".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.0),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Connection diagnosis failed");
        eprintln!("  Phase 8 Conn Diagnosis: {}", response.content);

        let json_str = strip_fences(&response.content);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
        assert!(
            parsed.is_ok(),
            "Connection diagnosis should be valid JSON: {}",
            json_str
        );

        let val = parsed.unwrap();
        assert!(val.get("diagnosis").is_some(), "Should have diagnosis");
        assert!(
            val.get("possibleCauses").is_some(),
            "Should have possibleCauses"
        );
        assert!(val.get("solutions").is_some(), "Should have solutions");
        assert!(val.get("category").is_some(), "Should have category");
    }

    // ─── Phase 8: Query analysis ───

    #[tokio::test]
    async fn test_phase8_query_analysis() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let system = datazen::ai::PromptBuilder::query_summary_system("en");
        let request = CompletionRequest {
            request_id: "test-query-analysis".into(),
            model: cfg.model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "Analyze these 5 queries:\n\nSELECT * FROM users WHERE id = 1\n---\nSELECT u.name, COUNT(o.id) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.name\n---\nINSERT INTO orders (user_id, amount) VALUES (1, 99.99)\n---\nSELECT * FROM orders WHERE status = 'pending'\n---\nUPDATE users SET name = 'Alice' WHERE id = 1".into(),
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: None,
                },
            ],
            temperature: Some(0.2),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Query analysis failed");
        eprintln!("  Phase 8 Query Analysis: {}", response.content);

        let json_str = strip_fences(&response.content);

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&json_str);
        assert!(
            parsed.is_ok(),
            "Query analysis should be valid JSON: {}",
            json_str
        );

        let val = parsed.unwrap();
        assert!(val.get("summary").is_some(), "Should have summary");
        assert!(val.get("categories").is_some(), "Should have categories");
        assert!(
            val.get("frequentTables").is_some(),
            "Should have frequentTables"
        );
    }

    // ─── Phase 1: Streaming ───

    #[tokio::test]
    async fn test_phase1_streaming() {
        let Some(cfg) = load_test_config() else {
            return;
        };

        let provider = create_provider(&cfg.provider, &cfg.endpoint, &cfg.api_key, cfg.max_tokens).await;

        let request = CompletionRequest {
            request_id: "test-stream".into(),
            model: cfg.model,
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Count from 1 to 5".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.0),

            stop: None,
            tools: None,
            previous_response_id: None,
        };

        let (tx, mut rx) = tokio::sync::mpsc::channel::<Result<StreamChunk, datazen_ai_api::AiError>>(32);
        let result = provider.stream_complete(&request, tx).await;
        assert!(result.is_ok(), "stream_complete failed: {:?}", result.err());

        let mut chunks = Vec::new();
        while let Some(chunk_result) = rx.recv().await {
            match chunk_result {
                Ok(chunk) => chunks.push(chunk),
                Err(e) => panic!("Stream error: {e}"),
            }
        }

        assert!(!chunks.is_empty(), "Should receive at least one chunk");

        let has_done = chunks.iter().any(|c| c.done);
        assert!(has_done, "Should have a final chunk with done=true");

        let full_content: String = chunks.iter().map(|c| c.content.as_str()).collect();
        eprintln!("  Streaming content: {full_content}");
        assert!(!full_content.is_empty(), "Streamed content should not be empty");
    }
}

// ─── DeepSeek Workflow Tool Calling Tests ─────────────────────────────────
mod deepseek_workflow_tests {
    use super::*;
    use datazen_ai_api::*;

    fn workflow_tools() -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "ask_questions".into(),
                description: "Ask the user structured questions to gather information.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "questions": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "id": { "type": "string" },
                                    "prompt": { "type": "string" },
                                    "options": {
                                        "type": "array",
                                        "items": {
                                            "type": "object",
                                            "properties": {
                                                "id": { "type": "string" },
                                                "label": { "type": "string" }
                                            },
                                            "required": ["id", "label"]
                                        }
                                    }
                                },
                                "required": ["id", "prompt"]
                            }
                        }
                    },
                    "required": ["questions"]
                }),
            },
            ToolDefinition {
                name: "list_connections".into(),
                description: "List all configured database connections.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {},
                    "required": []
                }),
            },
            ToolDefinition {
                name: "list_databases".into(),
                description: "List all databases on a connected database server.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_id": { "type": "string" }
                    },
                    "required": ["connection_id"]
                }),
            },
            ToolDefinition {
                name: "list_tables".into(),
                description: "List all tables in a database.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_id": { "type": "string" },
                        "database": { "type": "string" }
                    },
                    "required": ["connection_id"]
                }),
            },
            ToolDefinition {
                name: "get_table_schema".into(),
                description: "Get detailed schema of one or more tables.".into(),
                parameters: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "connection_id": { "type": "string" },
                        "tables": {
                            "type": "array",
                            "items": { "type": "string" }
                        }
                    },
                    "required": ["connection_id", "tables"]
                }),
            },
        ]
    }

    fn mock_tool_result(tool_call: &ToolCall) -> String {
        let args: serde_json::Value = serde_json::from_str(&tool_call.arguments).unwrap_or_default();
        match tool_call.name.as_str() {
            "list_connections" => serde_json::json!([
                { "id": "conn_pg01", "name": "test_orders", "databaseType": "PostgreSQL", "host": "localhost" },
                { "id": "conn_my01", "name": "MySQL Test", "databaseType": "MySQL", "host": "localhost" }
            ]).to_string(),
            "list_databases" => {
                let conn_id = args["connection_id"].as_str().unwrap_or("");
                if conn_id.contains("pg") {
                    serde_json::json!(["test_orders", "postgres"]).to_string()
                } else {
                    serde_json::json!(["logistics_db", "information_schema", "mysql"]).to_string()
                }
            }
            "list_tables" => {
                let conn_id = args["connection_id"].as_str().unwrap_or("");
                if conn_id.contains("pg") {
                    serde_json::json!([
                        { "name": "orders", "type": "table", "row_count": 15000 },
                        { "name": "users", "type": "table", "row_count": 500 }
                    ]).to_string()
                } else {
                    serde_json::json!([
                        { "name": "shipments", "type": "table", "row_count": 12000 },
                        { "name": "carriers", "type": "table", "row_count": 20 }
                    ]).to_string()
                }
            }
            "get_table_schema" => {
                let tables = args["tables"].as_array()
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
                    .unwrap_or_default();
                let conn_id = args["connection_id"].as_str().unwrap_or("");
                let mut schemas = serde_json::Map::new();
                for table in tables {
                    let cols = if conn_id.contains("pg") {
                        match table {
                            "orders" => serde_json::json!({
                                "columns": [
                                    { "name": "id", "type": "int4", "primary_key": true },
                                    { "name": "uid", "type": "int4", "nullable": false },
                                    { "name": "product_name", "type": "varchar(255)" },
                                    { "name": "amount", "type": "numeric(10,2)" },
                                    { "name": "status", "type": "varchar(50)" },
                                    { "name": "created_at", "type": "timestamp" }
                                ],
                                "indexes": [
                                    { "name": "idx_orders_uid", "columns": ["uid"] }
                                ]
                            }),
                            "users" => serde_json::json!({
                                "columns": [
                                    { "name": "id", "type": "int4", "primary_key": true },
                                    { "name": "name", "type": "varchar(100)" },
                                    { "name": "email", "type": "varchar(255)" }
                                ]
                            }),
                            _ => serde_json::json!({ "error": "table not found" })
                        }
                    } else {
                        match table {
                            "shipments" => serde_json::json!({
                                "columns": [
                                    { "name": "id", "type": "INT", "primary_key": true },
                                    { "name": "order_id", "type": "INT", "nullable": false },
                                    { "name": "carrier_id", "type": "INT" },
                                    { "name": "tracking_no", "type": "VARCHAR(100)" },
                                    { "name": "status", "type": "VARCHAR(50)" },
                                    { "name": "shipped_at", "type": "DATETIME" },
                                    { "name": "delivered_at", "type": "DATETIME" }
                                ],
                                "indexes": [
                                    { "name": "idx_shipments_order_id", "columns": ["order_id"] }
                                ]
                            }),
                            "carriers" => serde_json::json!({
                                "columns": [
                                    { "name": "id", "type": "INT", "primary_key": true },
                                    { "name": "name", "type": "VARCHAR(100)" },
                                    { "name": "code", "type": "VARCHAR(20)" }
                                ]
                            }),
                            _ => serde_json::json!({ "error": "table not found" })
                        }
                    };
                    schemas.insert(table.to_string(), cols);
                }
                serde_json::Value::Object(schemas).to_string()
            }
            _ => format!("Unknown tool: {}", tool_call.name),
        }
    }

    fn workflow_system_prompt() -> String {
        r#"你是 DataZen 的 Workflow 创建助手。你的任务是通过对话帮助用户创建数据库工作流（YAML 格式）。

## Workflow YAML 格式规范
- 步骤类型：query（SQL 查询）、ai（AI 分析）、condition（条件分支）、foreach（循环）
- 变量类型：string、number、connection
- 模板语法：{{变量名}}、{{steps.步骤id.result}}

## 数据库探索工具
你可以使用以下工具来了解用户的数据库结构：
- list_connections: 列出所有可用的数据库连接
- list_databases: 列出某个连接下的所有数据库
- list_tables: 列出某个数据库中的所有表
- get_table_schema: 获取表的详细 schema

**重要**：在生成 workflow 之前，主动调用这些工具获取表结构信息。

## 输出格式
当信息充足时，在回复中包含完整的 workflow YAML，用 ```yaml 代码块包裹。"#.to_string()
    }

    /// Full workflow creation test with DeepSeek: multi-round tool calling loop.
    /// Tests the AI's ability to explore database schema via tools and generate
    /// a cross-database workflow YAML.
    #[tokio::test]
    async fn test_deepseek_workflow_creation_with_tool_calling() {
        let Some(cfg) = load_deepseek_config() else {
            return;
        };

        let provider = datazen::ai::deepseek::DeepSeekProvider::new();
        let init_config = AiProviderConfig {
            provider_type: AiProviderType::DeepSeek,
            api_key: Some(cfg.api_key.clone()),
            endpoint: None, // use default
            model: String::new(),
            max_tokens: cfg.max_tokens,
            extra: serde_json::Value::Null,
        };
        provider.initialize(&init_config).await.expect("Failed to initialize DeepSeek");

        let tools = workflow_tools();
        let system_prompt = workflow_system_prompt();

        let mut messages = vec![
            ChatMessage {
                role: MessageRole::System,
                content: system_prompt,
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
            ChatMessage {
                role: MessageRole::User,
                content: "从test_orders连接中通过uid查询用户的订单信息，然后到MySQL Test连接中查询订单物流信息".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            },
        ];

        let max_rounds = 10;
        let mut round = 0;
        let mut final_content = String::new();
        let mut total_tool_calls = 0;
        let mut previous_response_id: Option<String> = None;

        loop {
            if round >= max_rounds {
                eprintln!("  ⚠ Reached max rounds ({max_rounds}), stopping");
                break;
            }

            eprintln!("  ── Round {round} ──");

            let request = CompletionRequest {
                request_id: format!("test-wf-round-{round}"),
                model: cfg.model.clone(),
                messages: messages.clone(),
                temperature: Some(0.7),
                stop: None,
                tools: Some(tools.clone()),
                previous_response_id: previous_response_id.clone(),
            };

            // Use streaming to test the full streaming path
            let (tx, mut rx) = tokio::sync::mpsc::channel(64);
            provider.stream_complete(&request, tx).await
                .unwrap_or_else(|e| panic!("stream_complete failed in round {round}: {e}"));

            let mut round_content = String::new();
            let mut round_reasoning = String::new();
            let mut round_tool_calls: Option<Vec<ToolCall>> = None;
            let mut round_response_id: Option<String> = None;

            while let Some(chunk_result) = rx.recv().await {
                let chunk = chunk_result.unwrap_or_else(|e| panic!("Stream error in round {round}: {e}"));
                round_content.push_str(&chunk.content);
                if let Some(r) = &chunk.reasoning {
                    round_reasoning.push_str(r);
                }
                if chunk.done {
                    round_tool_calls = chunk.tool_calls;
                    round_response_id = chunk.response_id;
                }
            }

            let content_preview = {
                let max = 200;
                let mut end = max.min(round_content.len());
                while end > 0 && !round_content.is_char_boundary(end) {
                    end -= 1;
                }
                &round_content[..end]
            };
            eprintln!("  Content: {content_preview}...");
            if !round_reasoning.is_empty() {
                let rlen = round_reasoning.len().min(100);
                let mut rend = rlen;
                while rend > 0 && !round_reasoning.is_char_boundary(rend) {
                    rend -= 1;
                }
                eprintln!("  Reasoning: {}...", &round_reasoning[..rend]);
            }

            // Check for tool calls
            let tool_calls = match round_tool_calls {
                Some(tcs) if !tcs.is_empty() => tcs,
                _ => {
                    eprintln!("  No tool calls — final response received");
                    final_content = round_content;
                    break;
                }
            };

            let db_tools: Vec<&ToolCall> = tool_calls.iter()
                .filter(|tc| tc.name != "ask_questions")
                .collect();

            if db_tools.is_empty() {
                eprintln!("  Only ask_questions tool calls — treating as final");
                final_content = round_content;
                break;
            }

            total_tool_calls += db_tools.len();
            eprintln!("  Tool calls: {:?}", db_tools.iter().map(|t| &t.name).collect::<Vec<_>>());

            // Handle previous_response_id for stateful conversations
            if round_response_id.is_some() {
                previous_response_id = round_response_id;
                // Stateful mode: clear history, only add tool results
                messages.retain(|m| m.role == MessageRole::System);
            } else {
                // Stateless: add assistant message to history
                messages.push(ChatMessage {
                    role: MessageRole::Assistant,
                    content: round_content,
                    reasoning: if round_reasoning.is_empty() { None } else { Some(round_reasoning) },
                    tool_calls: Some(tool_calls.clone()),
                    tool_call_id: None,
                });
            }

            // Execute tool calls and add results
            for tc in &tool_calls {
                if tc.name == "ask_questions" {
                    continue;
                }
                let result = mock_tool_result(tc);
                eprintln!("  Tool result for {}: {}...", tc.name, &result[..result.len().min(80)]);
                messages.push(ChatMessage {
                    role: MessageRole::Tool,
                    content: result,
                    reasoning: None,
                    tool_calls: None,
                    tool_call_id: Some(tc.id.clone()),
                });
            }

            round += 1;
        }

        eprintln!("\n  ═══ Test Results ═══");
        eprintln!("  Total rounds: {}", round + 1);
        eprintln!("  Total DB tool calls: {total_tool_calls}");

        // Assertions
        assert!(total_tool_calls > 0, "AI should have called at least one database tool");

        assert!(
            final_content.contains("```yaml") || final_content.contains("```YAML") || final_content.contains("yaml\n"),
            "Final response should contain a YAML code block. Content:\n{final_content}"
        );

        let content_lower = final_content.to_lowercase();
        assert!(
            content_lower.contains("steps") || content_lower.contains("query"),
            "YAML should contain workflow steps. Content:\n{final_content}"
        );

        // Should reference both connections
        assert!(
            content_lower.contains("test_orders") || content_lower.contains("conn_pg"),
            "Workflow should reference test_orders connection"
        );
        assert!(
            content_lower.contains("mysql") || content_lower.contains("conn_my"),
            "Workflow should reference MySQL connection"
        );

        eprintln!("  ✅ Workflow creation with tool calling passed!");
    }

    /// Test that DeepSeek streaming works with tool definitions (basic).
    #[tokio::test]
    async fn test_deepseek_streaming_with_tools() {
        let Some(cfg) = load_deepseek_config() else {
            return;
        };

        let provider = datazen::ai::deepseek::DeepSeekProvider::new();
        let init_config = AiProviderConfig {
            provider_type: AiProviderType::DeepSeek,
            api_key: Some(cfg.api_key.clone()),
            endpoint: None,
            model: String::new(),
            max_tokens: cfg.max_tokens,
            extra: serde_json::Value::Null,
        };
        provider.initialize(&init_config).await.expect("Failed to initialize");

        let tools = workflow_tools();
        let request = CompletionRequest {
            request_id: "test-ds-stream-tools".into(),
            model: cfg.model,
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Reply with 'Hello'. Do not use any tools.".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            temperature: Some(0.0),
            stop: None,
            tools: Some(tools),
            previous_response_id: None,
        };

        let (tx, mut rx) = tokio::sync::mpsc::channel(32);
        provider.stream_complete(&request, tx).await.expect("stream_complete failed");

        let mut content = String::new();
        let mut has_done = false;
        while let Some(chunk_result) = rx.recv().await {
            let chunk = chunk_result.expect("Stream chunk error");
            content.push_str(&chunk.content);
            if chunk.done {
                has_done = true;
            }
        }

        assert!(has_done, "Stream should have a done chunk");
        eprintln!("  DeepSeek streaming with tools: {content}");
        assert!(!content.is_empty(), "Content should not be empty");
    }
}
