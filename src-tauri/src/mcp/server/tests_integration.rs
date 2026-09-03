    #[tokio::test]
    async fn mcp_tool_handlers_with_mock_driver() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("mcp-cfg").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let conns = server.list_connections().await.unwrap();
        assert!(conns.contains("mcp-cfg"));

        let dbs = server
            .list_databases(rmcp::handler::server::wrapper::Parameters(
                ListDatabasesInput {
                    connection_id: "mcp-cfg".into(),
                },
            ))
            .await
            .unwrap();
        assert!(dbs.contains("app"));

        let tables = server
            .list_tables(rmcp::handler::server::wrapper::Parameters(
                ListTablesInput {
                    connection_id: "mcp-cfg".into(),
                    database: Some("app".into()),
                },
            ))
            .await
            .unwrap();
        assert!(tables.contains("users"));

        let schema = server
            .get_schema(rmcp::handler::server::wrapper::Parameters(GetSchemaInput {
                connection_id: "mcp-cfg".into(),
                table: "users".into(),
            }))
            .await
            .unwrap();
        assert!(schema.contains("users"));

        let desc = server
            .describe_table(rmcp::handler::server::wrapper::Parameters(
                DescribeTableInput {
                    connection_id: "mcp-cfg".into(),
                    table: "users".into(),
                },
            ))
            .await
            .unwrap();
        assert!(desc.contains("Table: users"));

        let explain = server
            .explain_query(rmcp::handler::server::wrapper::Parameters(
                ExplainQueryInput {
                    connection_id: "mcp-cfg".into(),
                    sql: "SELECT 1".into(),
                },
            ))
            .await
            .unwrap();
        assert!(!explain.is_empty());

        let query_out = server
            .query(rmcp::handler::server::wrapper::Parameters(QueryInput {
                connection_id: "mcp-cfg".into(),
                sql: "SELECT 1".into(),
                limit: Some(10),
            }))
            .await
            .unwrap();
        assert!(query_out.contains("alice") || query_out.contains("1"));

        let workflows = server.list_workflows().await.unwrap();
        assert!(
            workflows.contains("builtin-hello-query") || workflows.contains("[]"),
            "expected builtin workflows or an empty list, got: {workflows}"
        );

        let search_result = server
            .search_tables(rmcp::handler::server::wrapper::Parameters(
                SearchTablesInput {
                    connection_id: "mcp-cfg".into(),
                    database: Some("app".into()),
                    pattern: "user".into(),
                    limit: Some(10),
                },
            ))
            .await
            .unwrap();
        assert!(search_result.contains("users"));
        assert!(search_result.contains("totalMatches"));

        let search_no_match = server
            .search_tables(rmcp::handler::server::wrapper::Parameters(
                SearchTablesInput {
                    connection_id: "mcp-cfg".into(),
                    database: Some("app".into()),
                    pattern: "zzz_nonexistent".into(),
                    limit: None,
                },
            ))
            .await
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&search_no_match).unwrap();
        assert_eq!(parsed["totalMatches"].as_u64(), Some(0));
    }

    #[tokio::test]
    async fn mcp_search_tables_via_call_tool_inner() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("cti-search").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let result = server
            .call_tool_inner(
                "search_tables",
                Some(
                    serde_json::json!({"connection_id":"cti-search","database":"app","pattern":"user"}),
                ),
            )
            .await
            .unwrap();
        let text = result.content[0].as_text().unwrap().text.as_str();
        assert!(text.contains("users"));
        assert!(text.contains("totalMatches"));
    }

    #[tokio::test]
    async fn mcp_prompt_handlers_build_messages() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        let (config, _db_session_id) = test.save_and_connect("mcp-prompt").await;
        let connection_id = config.id.clone();
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let nl2sql = server
            .nl2sql_prompt(rmcp::handler::server::wrapper::Parameters(Nl2SqlArgs {
                connection_id: connection_id.clone(),
                question: "count users".into(),
                database: Some("app".into()),
            }))
            .await
            .unwrap();
        assert!(nl2sql.messages[0]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("Schema:"));
        assert!(nl2sql.messages[1]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("count users"));

        let diag = server
            .diagnose_error_prompt(rmcp::handler::server::wrapper::Parameters(
                DiagnoseErrorArgs {
                    connection_id: connection_id.clone(),
                    sql: "SELECT bad".into(),
                    error: "column missing".into(),
                },
            ))
            .await
            .unwrap();
        assert!(diag.messages[1]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("column missing"));

        let plan = server
            .explain_plan_prompt(rmcp::handler::server::wrapper::Parameters(
                ExplainPlanArgs {
                    connection_id,
                    sql: "SELECT 1".into(),
                },
            ))
            .await
            .unwrap();
        assert!(plan.messages[1]
            .content
            .as_text()
            .unwrap()
            .text
            .contains("EXPLAIN"));
    }

    #[tokio::test]
    async fn mcp_tool_router_lists_registered_tools() {
        let tools = DataZenMcpServer::tool_router().list_all();
        assert_eq!(tools.len(), MCP_ALL_TOOLS.len());
        assert!(tools.iter().any(|t| t.name == "query"));
    }

    #[tokio::test]
    async fn mcp_read_resource_inner_paths() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("res-inner").await;
        let (_config, _db_session_id) = test.save_and_connect("res-inner").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let conns = server
            .read_resource_inner("datazen://connections")
            .await
            .unwrap();
        match &conns.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(text.contains("res-inner"));
            }
            _ => panic!("expected text resource"),
        }

        // Seed one row so the resource contract below is exercised on real output.
        test.store
            .add_query_history(crate::store::QueryHistoryEntry {
                id: "hist-1".into(),
                connection_id: "res-inner".into(),
                database: "app".into(),
                schema: None,
                sql: "SELECT 1".into(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: 1,
                rows_affected: None,
                success: true,
                error_message: None,
            })
            .await
            .expect("add_query_history");

        let hist = server
            .read_resource_inner("datazen://query-history")
            .await
            .unwrap();
        // External contract: history entries must serialize with `connectionId`
        // (camelCase of `connection_id`); the legacy `configId` key is gone.
        assert!(!hist.contents.is_empty());
        match &hist.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(
                    text.contains("\"connectionId\""),
                    "query-history resource must expose connectionId, got: {text}"
                );
                assert!(
                    !text.contains("configId"),
                    "legacy configId key must not appear in resource output"
                );
            }
            _ => panic!("expected text resource"),
        }

        let schema_uri = "datazen://schema/res-inner/app";
        let schema = server.read_resource_inner(&schema_uri).await.unwrap();
        match &schema.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(!text.is_empty());
            }
            _ => panic!("expected text resource"),
        }

        let err = server
            .read_resource_inner("datazen://missing")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unknown resource"));
    }

    #[tokio::test]
    async fn mcp_call_tool_inner_unknown_tool_includes_help() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let err = server
            .call_tool_inner("missing_tool", None)
            .await
            .unwrap_err();
        let msg = err.message.to_string();
        assert!(msg.contains("Unknown or disabled tool"));
        assert!(msg.contains("list_connections"));
        assert!(msg.contains("Example"));
    }

    #[tokio::test]
    async fn mcp_call_tool_inner_disabled_tool_includes_help() {
        use crate::mcp::permission::McpPermissionMode;
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_disabled_tools(&["query".into()])
            .with_permission_mode(McpPermissionMode::SafeWrite);

        let err = server
            .call_tool_inner(
                "query",
                Some(serde_json::json!({"connection_id":"x","sql":"SELECT 1"})),
            )
            .await
            .unwrap_err();
        let msg = err.message.to_string();
        assert!(msg.contains("disabled"));
        assert!(msg.contains("Tool: query"));
        assert!(msg.contains("connection_id"));
    }

    #[tokio::test]
    async fn mcp_call_tool_inner_allowlist_error_includes_help() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("blocked-cfg").await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_allowed_connections(&["allowed-only".into()]);

        let err = server
            .call_tool_inner(
                "list_databases",
                Some(serde_json::json!({"connection_id":"blocked-cfg"})),
            )
            .await
            .unwrap_err();
        let msg = err.message.to_string();
        assert!(msg.contains("allowlist"));
        assert!(msg.contains("Tool: list_databases"));
        assert!(msg.contains("Example:"));
    }

    #[tokio::test]
    async fn mcp_query_rejects_disallowed_sql_in_readonly() {
        use crate::mcp::permission::McpPermissionMode;
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("ro-cfg").await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_permission_mode(McpPermissionMode::ReadOnly);

        let err = server
            .query(rmcp::handler::server::wrapper::Parameters(QueryInput {
                connection_id: "ro-cfg".into(),
                sql: "DELETE FROM users".into(),
                limit: None,
            }))
            .await
            .unwrap_err();
        assert!(
            err.to_string().to_lowercase().contains("delete")
                || err.to_string().contains("not allowed")
                || err.to_string().contains("permission")
        );
    }

    #[tokio::test]
    async fn mcp_get_info_exposes_capabilities() {
        use crate::testing::app_state::TestAppState;
        use rmcp::ServerHandler;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));
        let info = server.get_info();
        assert_eq!(info.server_info.name, "datazen");
        assert!(info.capabilities.tools.is_some());
    }

    #[tokio::test]
    async fn mcp_disabled_tools_and_permission_mode() {
        use crate::mcp::permission::McpPermissionMode;
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_disabled_tools(&["query".into()])
            .with_permission_mode(McpPermissionMode::ReadOnly);

        let mut router = DataZenMcpServer::tool_router();
        for name in MCP_ALL_TOOLS {
            if !crate::mcp::permission::is_tool_listed(
                name,
                McpPermissionMode::ReadOnly,
                &server.disabled_tools,
            ) {
                router.disable_route(name.to_string());
            }
        }
        let listed = router.list_all();
        assert!(!listed.iter().any(|t| t.name == "query"));
        assert!(listed.iter().any(|t| t.name == "list_connections"));
    }

    #[tokio::test]
    async fn mcp_list_tools_and_resources_inner() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));
        assert_eq!(server.list_tools_inner().len(), MCP_ALL_TOOLS.len());
        assert_eq!(server.list_resources_inner().len(), 3);
    }

    #[tokio::test]
    async fn mcp_call_tool_inner_database_tools() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("cti-cfg").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let dbs = server
            .call_tool_inner(
                "list_databases",
                Some(serde_json::json!({"connection_id":"cti-cfg"})),
            )
            .await
            .unwrap();
        assert!(dbs.content[0].as_text().unwrap().text.contains("app"));

        let tables = server
            .call_tool_inner(
                "list_tables",
                Some(serde_json::json!({"connection_id":"cti-cfg","database":"app"})),
            )
            .await
            .unwrap();
        assert!(tables.content[0].as_text().unwrap().text.contains("users"));

        let query = server
            .call_tool_inner(
                "query",
                Some(serde_json::json!({"connection_id":"cti-cfg","sql":"SELECT 1","limit":5})),
            )
            .await
            .unwrap();
        assert!(!query.content[0].as_text().unwrap().text.is_empty());

        assert!(server.call_tool_inner("missing_tool", None).await.is_err());
    }

    #[tokio::test]
    async fn mcp_run_workflow_not_found() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));
        let err = server
            .run_workflow(rmcp::handler::server::wrapper::Parameters(
                RunWorkflowInput {
                    workflow_id: "missing".into(),
                    variables: serde_json::json!({}),
                    connection_id: None,
                },
            ))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn mcp_query_history_read_only_returns_empty() {
        use crate::mcp::permission::McpPermissionMode;
        use crate::store::QueryHistoryEntry;
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("hist-ro").await;
        test.store
            .add_query_history(QueryHistoryEntry {
                id: "hist-ro-1".into(),
                connection_id: "hist-ro".into(),
                database: "app".into(),
                schema: None,
                sql: "SELECT secret FROM users".into(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: 1,
                rows_affected: None,
                success: true,
                error_message: None,
            })
            .await
            .expect("add_query_history");

        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_permission_mode(McpPermissionMode::ReadOnly);

        assert_eq!(server.list_resources_inner().len(), 2);
        assert!(!server
            .list_resources_inner()
            .iter()
            .any(|r| r.uri.as_str() == "datazen://query-history"));

        let hist = server
            .read_resource_inner("datazen://query-history")
            .await
            .unwrap();
        match &hist.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert_eq!(text.trim(), "[]");
                assert!(!text.contains("secret"));
            }
            _ => panic!("expected text resource"),
        }
    }

    #[tokio::test]
    async fn mcp_query_history_respects_connection_allowlist() {
        use crate::store::QueryHistoryEntry;
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("hist-allowed").await;
        test.save_connection("hist-blocked").await;
        for (id, sql) in [("hist-a", "SELECT 1"), ("hist-b", "SELECT 2")] {
            test.store
                .add_query_history(QueryHistoryEntry {
                    id: id.into(),
                    connection_id: if id == "hist-a" {
                        "hist-allowed".into()
                    } else {
                        "hist-blocked".into()
                    },
                    database: "app".into(),
                    schema: None,
                    sql: sql.into(),
                    executed_at: chrono::Utc::now(),
                    execution_time_ms: 1,
                    rows_affected: None,
                    success: true,
                    error_message: None,
                })
                .await
                .expect("add_query_history");
        }

        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_allowed_connections(&["hist-allowed".into()]);

        let hist = server
            .read_resource_inner("datazen://query-history")
            .await
            .unwrap();
        match &hist.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(text.contains("hist-allowed") || text.contains("\"connectionId\""));
                assert!(text.contains("SELECT 1"));
                assert!(!text.contains("hist-blocked"));
                assert!(!text.contains("SELECT 2"));
            }
            _ => panic!("expected text resource"),
        }
    }

    #[tokio::test]
    async fn mcp_connections_resource_respects_allowlist() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("conn-visible").await;
        test.save_connection("conn-hidden").await;

        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_allowed_connections(&["conn-visible".into()]);

        let conns = server
            .read_resource_inner("datazen://connections")
            .await
            .unwrap();
        match &conns.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => {
                assert!(text.contains("conn-visible"));
                assert!(!text.contains("conn-hidden"));
            }
            _ => panic!("expected text resource"),
        }
    }

    #[tokio::test]
    async fn mcp_schema_resource_rejects_disallowed_connection() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("schema-blocked").await;
        let server = DataZenMcpServer::new(Arc::new(test.state))
            .with_allowed_connections(&["other-only".into()]);

        let err = server
            .read_resource_inner("datazen://schema/schema-blocked/app")
            .await
            .unwrap_err();
        assert!(err.to_string().contains("allowlist"));
    }

    /// [tester] handler gate (tool_is_registered) stays aligned with tools.rs tool_router.
    #[tokio::test]
    async fn test_tester_tool_is_registered_cross_module_consistency() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        for name in MCP_ALL_TOOLS {
            assert!(
                server.tool_is_registered(name),
                "handler gate must see tool_router route for {name}"
            );
            assert!(
                DataZenMcpServer::tool_router().get(name).is_some(),
                "tools module must register {name}"
            );
        }
        assert!(
            !server.tool_is_registered(""),
            "empty tool name must not pass handler registration gate"
        );
        assert!(!server.tool_is_registered("not_a_real_tool"));
    }

    /// [tester] list_resources_inner URIs are readable via read_resource_inner (resources module).
    #[tokio::test]
    async fn test_tester_listed_resources_readable_via_read_resource_inner() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("res-list").await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        for resource in server.list_resources_inner() {
            let uri = resource.uri.as_str();
            let result = server.read_resource_inner(uri).await;
            assert!(
                result.is_ok(),
                "listed resource {uri} must be readable, got {:?}",
                result.err()
            );
        }
    }

    /// [tester] empty tool name rejected before tool_router dispatch (call_tool_inner path).
    #[tokio::test]
    async fn test_tester_call_tool_inner_rejects_empty_tool_name() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let err = server.call_tool_inner("", None).await.unwrap_err();
        let msg = err.message.to_string();
        assert!(
            msg.contains("Unknown or disabled tool"),
            "expected unknown-tool error for empty name, got: {msg}"
        );
    }

    /// [tester] blank resource URI returns resource_not_found (resources module boundary).
    #[tokio::test]
    async fn test_tester_read_resource_blank_uri_returns_not_found() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let err = server.read_resource_inner("").await.unwrap_err();
        assert!(
            err.to_string().contains("Unknown resource"),
            "blank URI must not panic or succeed, got: {err}"
        );
    }

    /// [tester] concurrent list_connections + read_resource without cross-request corruption.
    #[tokio::test]
    async fn test_tester_concurrent_mcp_operations() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::with_tables().await;
        test.save_connection("conc-a").await;
        test.save_connection("conc-b").await;
        let server = Arc::new(DataZenMcpServer::new(Arc::new(test.state)));

        let (conns, workflows, inner_conns) = tokio::join!(
            server.read_resource_inner("datazen://connections"),
            server.read_resource_inner("datazen://workflows"),
            server.list_connections(),
        );

        let conns_text = match &conns.unwrap().contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text.clone(),
            _ => panic!("expected text resource"),
        };
        let wf_text = match &workflows.unwrap().contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text.clone(),
            _ => panic!("expected text resource"),
        };
        let inner = inner_conns.unwrap();

        assert!(conns_text.contains("conc-a"));
        assert!(conns_text.contains("conc-b"));
        assert!(inner.contains("conc-a"));
        assert!(inner.contains("conc-b"));
        assert!(
            wf_text.contains("workflow") || wf_text.trim() == "[]",
            "workflows resource must be valid JSON list"
        );
    }

    /// [tester] post-split regression: list_tools_inner names stay aligned with MCP_ALL_TOOLS.
    #[tokio::test]
    async fn test_tester_list_tools_inner_snapshot_matches_mcp_all_tools() {
        use crate::testing::app_state::TestAppState;
        use std::sync::Arc;

        let test = TestAppState::new().await;
        let server = DataZenMcpServer::new(Arc::new(test.state));

        let mut actual: Vec<String> = server
            .list_tools_inner()
            .iter()
            .map(|t| t.name.to_string())
            .collect();
        actual.sort_unstable();

        let mut expected: Vec<String> = MCP_ALL_TOOLS.iter().map(|s| (*s).to_string()).collect();
        expected.sort_unstable();

        assert_eq!(
            actual, expected,
            "list_tools_inner must expose exactly MCP_ALL_TOOLS after module split"
        );
    }
