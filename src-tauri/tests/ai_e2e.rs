//! End-to-end integration tests for AI features.
//!
//! These tests require a real LLM API and are skipped if `.env.test` is absent.
//! Run with: `cargo test -p datazen --test ai_e2e -- --nocapture`

use std::path::PathBuf;

fn load_test_config() -> Option<(String, String, String, String)> {
    let env_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join(".env.test");

    if !env_path.exists() {
        eprintln!("⏭  Skipping AI E2E tests: .env.test not found");
        return None;
    }

    let content = std::fs::read_to_string(&env_path).ok()?;
    let mut provider = String::new();
    let mut endpoint = String::new();
    let mut api_key = String::new();
    let mut model = String::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "AI_PROVIDER" => provider = v.trim().to_string(),
                "AI_ENDPOINT" => endpoint = v.trim().to_string(),
                "AI_API_KEY" => api_key = v.trim().to_string(),
                "AI_MODEL" => model = v.trim().to_string(),
                _ => {}
            }
        }
    }

    if provider.is_empty() || model.is_empty() {
        eprintln!("⏭  Skipping: incomplete .env.test");
        return None;
    }

    Some((provider, endpoint, api_key, model))
}

mod provider_tests {
    use super::*;
    use datazen_ai_api::*;

    async fn create_provider(
        provider_type: &str,
        endpoint: &str,
        api_key: &str,
    ) -> Box<dyn AiProvider> {
        match provider_type {
            "open_ai" => {
                let p = datazen::ai::openai::OpenAiProvider::new();
                let config = AiProviderConfig {
                    provider_type: AiProviderType::OpenAi,
                    api_key: Some(api_key.to_string()),
                    endpoint: Some(endpoint.to_string()),
                    model: String::new(),
                    extra: serde_json::Value::Null,
                };
                p.initialize(&config).await.expect("Failed to initialize");
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
                        let wait = retry_after_secs.min(10);
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
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let request = CompletionRequest {
            request_id: "test-phase0".into(),
            model: model.clone(),
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Reply with exactly the word: OK".into(),
            }],
            temperature: Some(0.0),
            max_tokens: Some(100),
            stop: None,
        };

        let resp = complete_with_retry(provider.as_ref(), &request, 3).await
            .expect("complete() failed after retries");
        eprintln!("  Phase 0 response: {:?}", resp.content);
        assert!(
            !resp.content.is_empty(),
            "Response content should not be empty"
        );
    }

    // ─── Phase 1: NL2SQL ───

    #[tokio::test]
    async fn test_phase1_nl2sql_generation() {
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

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
            model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "查询所有用户的姓名和邮箱".into(),
                },
            ],
            temperature: Some(0.0),
            max_tokens: Some(500),
            stop: None,
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
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let system = datazen::ai::PromptBuilder::diagnose_system(
            "PostgreSQL",
            "  users (id int4 PK, name varchar, email varchar)",
            "en",
        );
        let request = CompletionRequest {
            request_id: "test-diagnose".into(),
            model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "SQL:\n```\nSELECT * FROM user WHERE name = 'test'\n```\n\nError:\nERROR: relation \"user\" does not exist".into(),
                },
            ],
            temperature: Some(0.0),
            max_tokens: Some(1000),
            stop: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3).await.expect("Diagnosis failed");
        eprintln!("  Phase 1 Diagnosis: {}", response.content);

        let content = response.content.trim();
        let json_str = if content.starts_with("```") {
            let lines: Vec<&str> = content.lines().collect();
            lines[1..lines.len() - 1].join("\n")
        } else {
            content.to_string()
        };

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
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let system = datazen::ai::PromptBuilder::explain_analysis_system("PostgreSQL", "en");
        let request = CompletionRequest {
            request_id: "test-explain".into(),
            model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "EXPLAIN output:\nSeq Scan on users  (cost=0.00..1.05 rows=5 width=556)\n\nSQL: SELECT * FROM users WHERE name = 'test'".into(),
                },
            ],
            temperature: Some(0.0),
            max_tokens: Some(1500),
            stop: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("EXPLAIN analysis failed");
        eprintln!("  Phase 2 EXPLAIN: {}", response.content);

        let content = response.content.trim();
        let json_str = if content.starts_with("```") {
            let lines: Vec<&str> = content.lines().collect();
            lines[1..lines.len() - 1].join("\n")
        } else {
            content.to_string()
        };

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
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let system = datazen::ai::PromptBuilder::nl_filter_system(
            "PostgreSQL",
            "  id int4 NOT NULL PK\n  name varchar NULL\n  age int4 NULL\n  status varchar NULL",
            "en",
        );
        let request = CompletionRequest {
            request_id: "test-filter".into(),
            model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "年龄大于18岁且状态为active的用户".into(),
                },
            ],
            temperature: Some(0.0),
            max_tokens: Some(500),
            stop: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Smart filter failed");
        eprintln!("  Phase 7 Filter: {}", response.content);

        let content = response.content.trim();
        let json_str = if content.starts_with("```") {
            let lines: Vec<&str> = content.lines().collect();
            lines[1..lines.len() - 1].join("\n")
        } else {
            content.to_string()
        };

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
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let system = datazen::ai::PromptBuilder::schema_doc_system(
            "PostgreSQL",
            "  users (id int4 PK, name varchar NOT NULL, email varchar NOT NULL UNIQUE)\n  orders (id int4 PK, user_id int4 NOT NULL, amount numeric, created_at timestamp DEFAULT now())",
            "en",
        );
        let request = CompletionRequest {
            request_id: "test-schema-doc".into(),
            model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "Generate documentation for the database schema above.".into(),
                },
            ],
            temperature: Some(0.3),
            max_tokens: Some(2000),
            stop: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Schema doc generation failed");
        eprintln!("  Phase 8 Schema Doc (first 200 chars): {}", &response.content[..response.content.len().min(200)]);

        let content = response.content.to_lowercase();
        assert!(content.contains("user"), "Doc should mention users table");
        assert!(content.contains("order"), "Doc should mention orders table");
    }

    // ─── Phase 8: Connection diagnosis ───

    #[tokio::test]
    async fn test_phase8_connection_diagnosis() {
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let system = datazen::ai::PromptBuilder::connection_diagnose_system("en");
        let request = CompletionRequest {
            request_id: "test-conn-diag".into(),
            model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "Connection details:\nConnection type: PostgreSQL\nHost: localhost\nPort: 5432\nDatabase: mydb\nUsername: admin\nSSL: Disable\nSSH Tunnel: disabled\nTimeout: 10s\n\nError:\nconnection refused (os error 111)".into(),
                },
            ],
            temperature: Some(0.0),
            max_tokens: Some(1500),
            stop: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Connection diagnosis failed");
        eprintln!("  Phase 8 Conn Diagnosis: {}", response.content);

        let content = response.content.trim();
        let json_str = if content.starts_with("```") {
            let lines: Vec<&str> = content.lines().collect();
            lines[1..lines.len() - 1].join("\n")
        } else {
            content.to_string()
        };

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
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let system = datazen::ai::PromptBuilder::query_summary_system("en");
        let request = CompletionRequest {
            request_id: "test-query-analysis".into(),
            model,
            messages: vec![
                system,
                ChatMessage {
                    role: MessageRole::User,
                    content: "Analyze these 5 queries:\n\nSELECT * FROM users WHERE id = 1\n---\nSELECT u.name, COUNT(o.id) FROM users u JOIN orders o ON u.id = o.user_id GROUP BY u.name\n---\nINSERT INTO orders (user_id, amount) VALUES (1, 99.99)\n---\nSELECT * FROM orders WHERE status = 'pending'\n---\nUPDATE users SET name = 'Alice' WHERE id = 1".into(),
                },
            ],
            temperature: Some(0.2),
            max_tokens: Some(2000),
            stop: None,
        };

        let response = complete_with_retry(provider.as_ref(), &request, 3)
            .await
            .expect("Query analysis failed");
        eprintln!("  Phase 8 Query Analysis: {}", response.content);

        let content = response.content.trim();
        let json_str = if content.starts_with("```") {
            let lines: Vec<&str> = content.lines().collect();
            lines[1..lines.len() - 1].join("\n")
        } else {
            content.to_string()
        };

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
        let Some((ptype, endpoint, api_key, model)) = load_test_config() else {
            return;
        };

        let provider = create_provider(&ptype, &endpoint, &api_key).await;

        let request = CompletionRequest {
            request_id: "test-stream".into(),
            model,
            messages: vec![ChatMessage {
                role: MessageRole::User,
                content: "Count from 1 to 5".into(),
            }],
            temperature: Some(0.0),
            max_tokens: Some(200),
            stop: None,
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
