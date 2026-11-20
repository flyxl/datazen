//! T094 audit: the `commands/ai.ts` shaper port, verified against the
//! React source re-read line by line (there is no React wire test for
//! this file — the shapes below come straight from `aiCommands`' object
//! literal), plus `workflowClient.ts`'s `MethodMap` for the workflow
//! slice.
//!
//! Tester-written: every key, command name, normalization and stub policy
//! below is re-derived from the sources.

use serde_json::json;

use crate::commands::ai as aic;
use crate::commands::ai::wire::{
    AiRequest, ChatParams, DiagnoseErrorParams, E2eStubPolicy, GenerateSqlParams,
    PromptOverrideEntry, PromptScenario, SessionsListParams,
    ai_apply_write_request, ai_deny_write_request, ai_list_write_audit_request,
    analyze_queries_request, cancel_request, chat_request, connection_notes_get_request,
    fetch_remote_models_request, generate_sql_request, grant_editor_session_request,
    memory_update_request, prompt_list_request, prompt_remove_override_request,
    prompt_set_override_request, revoke_editor_grant_request, sessions_delete_request,
    sessions_export_request, sessions_list_request, set_active_profile_request,
    touch_editor_grant_request, validate_config_request,
};
use crate::commands::ai::workflow::{
    AiSubmissionTokenVm, WorkflowMethod, save_workflow_request, save_workflow_yaml_request,
    with_submission_token, workflow_delete_request, workflow_execute_request,
    workflow_get_dir_request, workflow_get_request, workflow_history_clear_request,
    workflow_history_list_request, workflow_list_request, WorkflowExecuteParams,
};

fn req(command: &'static str, value: serde_json::Value) -> AiRequest {
    AiRequest::native(command, value)
}

#[test]
fn scalar_and_bag_shapers_match_the_react_literal() {
    assert_eq!(fetch_remote_models_request("openai", "https://x", "sk-1").command, "ai_fetch_remote_models");
    assert_eq!(
        fetch_remote_models_request("openai", "https://x", "sk-1").value,
        json!({ "protocol": "openai", "endpoint": "https://x", "apiKey": "sk-1" })
    );
    assert_eq!(
        validate_config_request(&json!({"a":1})).value,
        json!({ "config": {"a": 1} })
    );
    assert_eq!(
        set_active_profile_request("p-1"),
        req("ai_set_active_profile", json!({ "profileId": "p-1" }))
    );
    assert_eq!(
        memory_update_request("m-1", &json!({"content":"x"})).value,
        json!({ "id": "m-1", "patch": {"content": "x"} })
    );
    assert_eq!(
        sessions_export_request("s-1", "markdown"),
        req("ai_sessions_export", json!({ "id": "s-1", "format": "markdown" }))
    );
    assert_eq!(
        sessions_delete_request("s-1"),
        req("ai_sessions_delete", json!({ "id": "s-1" }))
    );
    assert_eq!(
        connection_notes_get_request("c-1"),
        req("ai_connection_notes_get", json!({ "connectionId": "c-1" }))
    );
    assert_eq!(
        ai_deny_write_request("pr-1"),
        req("ai_deny_write", json!({ "proposalId": "pr-1" }))
    );
    assert_eq!(
        cancel_request("r-1"),
        req("ai_cancel", json!({ "requestId": "r-1" }))
    );
}

#[test]
fn null_normalizations_are_explicit_where_react_writes_them() {
    // `?? null` keys ride the wire as null, present.
    assert_eq!(
        ai_list_write_audit_request(None).value,
        json!({ "limit": null })
    );
    assert_eq!(ai_list_write_audit_request(Some(5)).value, json!({ "limit": 5 }));
    assert_eq!(prompt_list_request(None).value, json!({ "driverType": null }));
    assert_eq!(prompt_list_request(Some("sqlite")).value, json!({ "driverType": "sqlite" }));
    // sessionsList: all three keys always present, null when absent.
    assert_eq!(
        sessions_list_request(&SessionsListParams::default()).value,
        json!({ "connectionId": null, "database": null, "sessionKey": null })
    );
    assert_eq!(
        sessions_list_request(&SessionsListParams {
            connection_id: Some("c".into()),
            database: None,
            session_key: Some("k".into()),
        })
        .value,
        json!({ "connectionId": "c", "database": null, "sessionKey": "k" })
    );
    // A spread params object drops its undefined keys: analyzeQueries({})
    // is the empty bag, not `{ dbSessionId: null }`.
    assert_eq!(analyze_queries_request(None).value, json!({}));
    assert_eq!(analyze_queries_request(Some("s-1")).value, json!({ "dbSessionId": "s-1" }));
}

#[test]
fn only_chat_and_generate_sql_are_e2e_capturable() {
    // The grant lifecycle is nativeOnly: the stub's '' would be an
    // invalid receipt (the React file's own warning).
    for request in [
        grant_editor_session_request(&json!({})),
        revoke_editor_grant_request("g-1"),
        touch_editor_grant_request("g-1"),
        cancel_request("r-1"),
    ] {
        assert_eq!(request.stub, E2eStubPolicy::NativeOnly, "{}", request.command);
    }
    let chat = chat_request(&ChatParams {
        messages: json!([]),
        request_id: "r-1".into(),
        ..ChatParams::default()
    });
    assert_eq!(chat.command, "ai_chat");
    assert_eq!(chat.stub, E2eStubPolicy::Capturable);
    let gen = generate_sql_request(&GenerateSqlParams {
        db_session_id: "s".into(),
        database: "db".into(),
        natural_language: "list users".into(),
        request_id: "r-2".into(),
        ..GenerateSqlParams::default()
    });
    assert_eq!(gen.command, "ai_generate_sql");
    assert_eq!(gen.stub, E2eStubPolicy::Capturable);
}

#[test]
fn optional_members_drop_like_json_stringify() {
    // generateSql: the required five ride; every optional member is
    // absent until set (JS undefined keys are dropped).
    let params = GenerateSqlParams {
        db_session_id: "s".into(),
        database: "db".into(),
        natural_language: "q".into(),
        request_id: "r".into(),
        ..GenerateSqlParams::default()
    };
    assert_eq!(
        generate_sql_request(&params).value,
        json!({
            "dbSessionId": "s",
            "database": "db",
            "naturalLanguage": "q",
            "requestId": "r",
        })
    );
    let full = GenerateSqlParams {
        current_table: Some("users".into()),
        recent_queries: Some(vec!["SELECT 1".into()]),
        memory_mode: Some("auto".into()),
        ..params.clone()
    };
    let value = generate_sql_request(&full).value;
    assert_eq!(value["currentTable"], json!("users"));
    assert_eq!(value["recentQueries"], json!(["SELECT 1"]));
    assert_eq!(value["memoryMode"], json!("auto"));
    assert!(value.get("contextFiles").is_none());

    // chat: same rule; the non-scalar members ride verbatim.
    let chat = chat_request(&ChatParams {
        db_session_id: Some("s".into()),
        messages: json!([{ "role": "user", "content": "hi" }]),
        request_id: "r".into(),
        target_claim: Some(json!({ "kind": "editorGrant" })),
        ..ChatParams::default()
    });
    assert_eq!(
        chat.value,
        json!({
            "dbSessionId": "s",
            "messages": [{ "role": "user", "content": "hi" }],
            "requestId": "r",
            "targetClaim": { "kind": "editorGrant" },
        })
    );

    // The typed param bags keep their exact key spellings.
    assert_eq!(
        generate_sql_request(&params).value["naturalLanguage"],
        json!("q")
    );
    let diagnose = DiagnoseErrorParams {
        db_session_id: "s".into(),
        database: "db".into(),
        sql: "SELCT".into(),
        error_message: "syntax error".into(),
    };
    assert_eq!(
        crate::commands::ai::wire::diagnose_error_request(&diagnose).value,
        json!({
            "dbSessionId": "s",
            "database": "db",
            "sql": "SELCT",
            "errorMessage": "syntax error",
        })
    );
}

#[test]
fn the_write_face_and_apply_bag_match_react() {
    let propose = crate::commands::ai::wire::ai_propose_write_request(
        &crate::commands::ai::wire::ProposeWriteParams {
            connection_id: "c".into(),
            database: Some("db".into()),
            sql: "UPDATE t".into(),
            previous_sql: None,
        },
    );
    assert_eq!(
        propose.value,
        json!({ "connectionId": "c", "database": "db", "sql": "UPDATE t" })
    );
    let apply = ai_apply_write_request(&crate::commands::ai::wire::ApplyWriteParams {
        proposal_id: "p".into(),
        connection_id: "c".into(),
        database: "db".into(),
        sql: "UPDATE t".into(),
    });
    assert_eq!(
        apply.value,
        json!({ "proposalId": "p", "connectionId": "c", "database": "db", "sql": "UPDATE t" })
    );
}

#[test]
fn mcp_shapers_match_the_react_literal() {
    use crate::commands::ai::mcp::*;
    assert_eq!(mcp_http_enable_request(None).value, json!({}));
    assert_eq!(mcp_http_enable_request(Some(8080)).value, json!({ "port": 8080 }));
    assert_eq!(
        mcp_respond_approval_request("req-1", "allow").value,
        json!({ "requestId": "req-1", "decision": "allow" })
    );
    assert_eq!(mcp_list_audit_log_request(None).value, json!({ "limit": null }));
    assert_eq!(
        mcp_client_disconnect_request("srv-1").value,
        json!({ "serverId": "srv-1" })
    );
    assert_eq!(
        mcp_client_call_tool_request(&McpCallToolParams {
            server_id: "srv".into(),
            tool_name: "query".into(),
            arguments: json!({ "sql": "SELECT 1" }),
        })
        .value,
        json!({ "serverId": "srv", "toolName": "query", "arguments": { "sql": "SELECT 1" } })
    );
    // Every mcp command is native-only (no e2e stubbing).
    assert_eq!(mcp_get_status_request().stub, E2eStubPolicy::NativeOnly);
}

#[test]
fn prompt_vocabulary_serializes_with_react_spellings() {
    assert_eq!(
        serde_json::to_value(PromptScenario::SchemaDocSelectTables).unwrap_or_default(),
        json!("schema_doc_select_tables")
    );
    assert_eq!(
        serde_json::to_value(PromptScenario::WorkflowGenerate).unwrap_or_default(),
        json!("workflow_generate")
    );
    let entry = PromptOverrideEntry {
        driver_type: "sqlite".into(),
        scenario: PromptScenario::Nl2sql,
        system: "be brief".into(),
    };
    assert_eq!(
        prompt_set_override_request(&entry).value,
        json!({ "entry": { "driverType": "sqlite", "scenario": "nl2sql", "system": "be brief" } })
    );
    assert_eq!(
        prompt_remove_override_request("sqlite", PromptScenario::Chat).value,
        json!({ "driverType": "sqlite", "scenario": "chat" })
    );
}

#[test]
fn the_stream_edges_carry_the_channel_names_and_payloads() {
    assert_eq!(aic::AI_STREAM_CHUNK_CHANNEL, "ai:stream-chunk");
    assert_eq!(aic::AI_STREAM_ERROR_CHANNEL, "ai:stream-error");
    assert_eq!(aic::AI_CONFIG_CHANGED_CHANNEL, "ai:config-changed");
    // StreamChunkPayload round-trips with the exact wire keys.
    let payload = aic::StreamChunkPayload {
        request_id: "r-1".into(),
        content: "SELECT".into(),
        reasoning: None,
        done: true,
        cancelled: Some(true),
        usage: Some(aic::StreamUsage {
            prompt_tokens: 10,
            completion_tokens: 5,
            total_tokens: 15,
        }),
        egress_summary: None,
    };
    let wire = serde_json::to_value(&payload).unwrap_or_default();
    assert_eq!(
        wire,
        json!({
            "requestId": "r-1",
            "content": "SELECT",
            "done": true,
            "cancelled": true,
            "usage": { "promptTokens": 10, "completionTokens": 5, "totalTokens": 15 },
        })
    );
    let error = aic::StreamErrorPayload {
        request_id: "r-2".into(),
        error: "boom".into(),
    };
    assert_eq!(serde_json::to_value(&error).unwrap_or_default(), json!({ "requestId": "r-2", "error": "boom" }));
}

#[test]
fn workflow_slice_matches_the_method_map() {
    // The wire names are workflowClient.ts's MethodMap keys.
    let cases = [
        (WorkflowMethod::ListWorkflows, "listWorkflows"),
        (WorkflowMethod::GetWorkflow, "getWorkflow"),
        (WorkflowMethod::GetWorkflowYaml, "getWorkflowYaml"),
        (WorkflowMethod::SaveWorkflow, "saveWorkflow"),
        (WorkflowMethod::SaveWorkflowYaml, "saveWorkflowYaml"),
        (WorkflowMethod::DeleteWorkflow, "deleteWorkflow"),
        (WorkflowMethod::ExecuteWorkflow, "executeWorkflow"),
        (WorkflowMethod::ReloadWorkflows, "reloadWorkflows"),
        (WorkflowMethod::ListWorkflowRuns, "listWorkflowRuns"),
        (WorkflowMethod::GetWorkflowRun, "getWorkflowRun"),
        (WorkflowMethod::ClearWorkflowRuns, "clearWorkflowRuns"),
    ];
    for (method, name) in cases {
        assert_eq!(method.wire_name(), name);
    }

    let token = AiSubmissionTokenVm {
        idempotency_key: "k-1".into(),
        expires_at: "2026-01-01T00:00:00Z".into(),
    };
    // save: the definition rides the { workflow } bag + token.
    let (method, value) = save_workflow_request(&json!({ "id": "w-1" }), &token);
    assert_eq!(method, WorkflowMethod::SaveWorkflow);
    assert_eq!(
        value,
        json!({
            "workflow": { "id": "w-1" },
            "submissionToken": { "idempotencyKey": "k-1", "expiresAt": "2026-01-01T00:00:00Z" },
        })
    );
    // save-yaml: { yaml } + token.
    let (method, value) = save_workflow_yaml_request("id: w-1", &token);
    assert_eq!(method, WorkflowMethod::SaveWorkflowYaml);
    assert_eq!(value["yaml"], json!("id: w-1"));
    assert!(value["submissionToken"].is_object());
    // execute: the flat request + token.
    let (method, value) = workflow_execute_request(
        &WorkflowExecuteParams {
            workflow_id: "w-1".into(),
            variables: json!({ "x": 1 }),
            connection_id: Some("c".into()),
        },
        &token,
    );
    assert_eq!(method, WorkflowMethod::ExecuteWorkflow);
    assert_eq!(
        value,
        json!({
            "workflowId": "w-1",
            "variables": { "x": 1 },
            "connectionId": "c",
            "submissionToken": { "idempotencyKey": "k-1", "expiresAt": "2026-01-01T00:00:00Z" },
        })
    );
    // Bodyless faces carry no token.
    let (method, value) = workflow_delete_request("w-1");
    assert_eq!(method, WorkflowMethod::DeleteWorkflow);
    assert_eq!(value, json!({ "workflowId": "w-1" }));
    // History: absent id = the whole history ({}), not a null key.
    assert_eq!(workflow_history_list_request(None), (WorkflowMethod::ListWorkflowRuns, json!({})));
    assert_eq!(
        workflow_history_list_request(Some("w-1")),
        (WorkflowMethod::ListWorkflowRuns, json!({ "workflowId": "w-1" }))
    );
    assert_eq!(
        workflow_history_clear_request(None),
        (WorkflowMethod::ClearWorkflowRuns, json!({}))
    );
    // get/get-yaml/list: plain id bags.
    assert_eq!(workflow_get_request("w-1").1, json!({ "workflowId": "w-1" }));
    assert_eq!(workflow_list_request().1, json!({}));
    // get_dir is the desktop-only legacy command.
    let dir = workflow_get_dir_request();
    assert_eq!(dir.command, "workflow_get_dir");
    assert_eq!(dir.value, json!({}));
    // The token merge is a plain key fold.
    assert_eq!(
        with_submission_token(json!({ "a": 1 }), &token)["submissionToken"]["idempotencyKey"],
        json!("k-1")
    );
}
