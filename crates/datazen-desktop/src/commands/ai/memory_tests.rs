use super::*;
use crate::commands::ai::chat::ai_chat;
use crate::commands::ai::command_test_host::{host_command_app, HostedCommandApp};
use crate::store::ai_memory::AiMemoryRecord;
use crate::testing::app_state_test_support::TestAppState;

async fn app() -> HostedCommandApp {
    host_command_app(TestAppState::new().await)
}

fn input(subject: &str, content: &str) -> AiMemoryCreateInput {
    AiMemoryCreateInput {
        id: None,
        scope_type: MemoryScopeKind::User,
        scope_id: None,
        connection_id: None,
        database_name: None,
        schema_name: None,
        table_name: None,
        column_name: None,
        kind: MemoryKind::Preference,
        subject: subject.into(),
        content: content.into(),
        structured_json: None,
        source: Some(MemorySource::Explicit),
        source_ref: None,
        confidence: Some(0.9),
        sensitivity: Some(MemorySensitivity::Normal),
        status: Some(MemoryStatus::Active),
        pinned: Some(false),
        expires_at: None,
    }
}

async fn enable_memory(app: &HostedCommandApp) {
    let mut settings = app.state().store.get_ai_settings_config().await;
    settings.memory.enabled = true;
    app.state()
        .store
        .save_ai_settings_config(&settings)
        .await
        .unwrap();
}

async fn disable_memory(app: &HostedCommandApp) {
    let mut settings = app.state().store.get_ai_settings_config().await;
    settings.memory.enabled = false;
    app.state()
        .store
        .save_ai_settings_config(&settings)
        .await
        .unwrap();
}

async fn list_all(app: &HostedCommandApp) -> AiMemoryPage {
    ai_memory_list(app.state(), None, None, None, None, None, None, None, None)
        .await
        .unwrap()
}

#[tokio::test]
async fn memory_ipc_create_list_update_status_delete_and_stats() {
    let app = app().await;
    enable_memory(&app).await;
    let created = ai_memory_create(
        app.state(),
        input("Customer locale", "Use en-US for reports"),
    )
    .await
    .unwrap();
    assert_eq!(created.status, MemoryStatus::Active);
    assert_eq!(created.confidence, 0.9);
    assert_eq!(created.scope_type, MemoryScopeKind::User);

    let all = list_all(&app).await;
    assert_eq!(all.total, 1);
    assert_eq!(all.entries[0].id, created.id);

    let updated = ai_memory_update(
        app.state(),
        created.id.clone(),
        AiMemoryUpdateInput {
            subject: None,
            content: Some("Use en-GB for reports".into()),
            kind: None,
            structured_json: None,
            source_ref: None,
            confidence: Some(0.75),
            sensitivity: None,
            status: None,
            pinned: Some(true),
            expires_at: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.content, "Use en-GB for reports");
    assert_eq!(updated.confidence, 0.75);
    assert!(updated.pinned);

    let rejected = ai_memory_reject(app.state(), created.id.clone())
        .await
        .unwrap();
    assert_eq!(rejected.status, MemoryStatus::Rejected);
    let confirmed = ai_memory_confirm(app.state(), created.id.clone())
        .await
        .unwrap();
    assert_eq!(confirmed.status, MemoryStatus::Active);

    let stats = ai_memory_stats(app.state()).await.unwrap();
    assert_eq!(stats.total, 1);
    assert_eq!(stats.by_status.get("active"), Some(&1));
    assert_eq!(stats.by_kind.get("preference"), Some(&1));

    assert!(ai_memory_delete(app.state(), created.id.clone())
        .await
        .unwrap());
    assert_eq!(list_all(&app).await.total, 0);
}

#[tokio::test]
async fn memory_ipc_paginates_and_searches_fts() {
    let app = app().await;
    enable_memory(&app).await;
    for (subject, content) in [
        ("postgres preference", "Customer prefers PostgreSQL"),
        ("report timezone", "Reports use UTC"),
        ("style rule", "Use lowercase identifiers"),
    ] {
        ai_memory_create(app.state(), input(subject, content))
            .await
            .unwrap();
    }
    let first = ai_memory_list(
        app.state(),
        None,
        None,
        Some(MemoryStatus::Active),
        None,
        None,
        None,
        Some(1),
        Some(0),
    )
    .await
    .unwrap();
    assert_eq!(first.total, 3);
    assert_eq!(first.entries.len(), 1);
    assert_eq!(first.limit, 1);

    let second = ai_memory_list(
        app.state(),
        None,
        None,
        Some(MemoryStatus::Active),
        None,
        None,
        None,
        Some(1),
        Some(1),
    )
    .await
    .unwrap();
    assert_eq!(second.entries.len(), 1);
    assert_ne!(first.entries[0].id, second.entries[0].id);

    let found = ai_memory_search(
        app.state(),
        "PostgreSQL".into(),
        None,
        None,
        Some(MemoryStatus::Active),
        Some(10),
        Some(0),
    )
    .await
    .unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.entries[0].subject, "postgres preference");
}

#[tokio::test]
async fn memory_ipc_rejects_secret_and_invalid_confidence() {
    let app = app().await;
    enable_memory(&app).await;
    let mut secret = input("secret", "password=hunter2");
    secret.sensitivity = Some(MemorySensitivity::Secret);
    let error = ai_memory_create(app.state(), secret).await.unwrap_err();
    assert!(matches!(error, CommandError::Validation(_)));
    assert_eq!(list_all(&app).await.total, 0);

    let mut bad = input("bad confidence", "x");
    bad.confidence = Some(1.5);
    assert!(matches!(
        ai_memory_create(app.state(), bad).await.unwrap_err(),
        CommandError::Validation(_)
    ));
}

#[tokio::test]
async fn memory_ipc_scopes_to_organization_and_principal() {
    let app = app().await;
    enable_memory(&app).await;
    ai_memory_create(app.state(), input("mine", "visible"))
        .await
        .unwrap();
    let (_, principal) = scope(&app.state()).unwrap();
    let now = now();
    app.state()
        .store
        .app_db()
        .upsert_ai_memory(&AiMemoryRecord {
            id: "other-org-memory".into(),
            organization_id: "other-org".into(),
            owner_principal_id: principal,
            scope_type: "user".into(),
            scope_id: "someone".into(),
            connection_id: None,
            database_name: None,
            schema_name: None,
            table_name: None,
            column_name: None,
            kind: "preference".into(),
            subject: "other".into(),
            content: "must not leak".into(),
            structured_json: None,
            source: "explicit".into(),
            source_ref: None,
            confidence: 0.9,
            sensitivity: "normal".into(),
            status: "active".into(),
            pinned: false,
            use_count: 0,
            last_used_at: None,
            expires_at: None,
            content_hash: "hash-other".into(),
            created_at: now.to_rfc3339(),
            updated_at: now.to_rfc3339(),
        })
        .unwrap();
    let all = list_all(&app).await;
    assert_eq!(all.total, 1);
    assert_eq!(all.entries[0].subject, "mine");
}

#[tokio::test]
async fn memory_ipc_export_and_clear_are_scoped_and_formatted() {
    let app = app().await;
    enable_memory(&app).await;
    ai_memory_create(app.state(), input("one", "alpha"))
        .await
        .unwrap();
    ai_memory_create(app.state(), input("two", "beta"))
        .await
        .unwrap();

    let json = ai_memory_export(app.state(), "json".into(), None, None)
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.as_array().unwrap().len(), 2);

    let markdown = ai_memory_export(app.state(), "markdown".into(), None, None)
        .await
        .unwrap();
    assert!(markdown.contains("# DataZen AI Memory"));
    assert!(markdown.contains("alpha"));

    let cleared = ai_memory_clear(app.state(), None, None, None)
        .await
        .unwrap();
    assert_eq!(cleared, 2);
    assert_eq!(list_all(&app).await.total, 0);
}

#[tokio::test]
async fn memory_create_is_rejected_while_settings_are_disabled() {
    let app = app().await;
    let mut candidate = input("disabled candidate", "must not be stored");
    candidate.status = Some(MemoryStatus::Candidate);
    let error = ai_memory_create(app.state(), candidate.clone())
        .await
        .unwrap_err();
    assert!(
        format!("{error:?}").contains("memory is disabled"),
        "unexpected error: {error:?}"
    );
    assert_eq!(list_all(&app).await.total, 0);

    enable_memory(&app).await;
    let created = ai_memory_create(app.state(), candidate).await.unwrap();
    assert_eq!(created.status, MemoryStatus::Candidate);
    assert_eq!(list_all(&app).await.total, 1);
}

#[tokio::test]
async fn chat_auto_injects_memory_and_emits_reference_chunk() {
    use datazen::ai::{ChatMessage, MessageRole};
    use std::sync::{Arc, Mutex};
    use tauri::Listener;

    let (test, mock) = TestAppState::with_wiremock_ai().await;
    let app = host_command_app(test);
    enable_memory(&app).await;
    let created = ai_memory_create(
        app.state(),
        input("timezone preference", "question: reports must use UTC"),
    )
    .await
    .unwrap();
    mock.mount_chat_stream_text("SELECT 1").await;

    let chunks = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let captured = chunks.clone();
    app.window().listen("ai:stream-chunk", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            captured.lock().unwrap().push(payload);
        }
    });

    ai_chat(
        app.state(),
        app.window(),
        None,
        None,
        vec![ChatMessage {
            role: MessageRole::User,
            content: "question: what timezone?".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-memory-auto".into(),
        false,
        None,
        None,
        None,
        None,
        Some("conn-memory".into()),
        Some(datazen::ai::AiAgentMode::Ask),
        None,
        None,
        None,
        None,
        None,
        None,
        Some(MemoryMode::Auto),
        Some(MemoryScopeKind::User),
        None,
    )
    .await
    .unwrap();
    mock.mount_chat_stream_text("SELECT 1").await;

    let memory_chunk = chunks
        .lock()
        .unwrap()
        .iter()
        .find(|chunk| chunk.get("memoryUsed").is_some())
        .cloned()
        .expect("memory_used chunk");
    let used = memory_chunk["memoryUsed"].as_array().unwrap();
    assert_eq!(used.len(), 1);
    assert_eq!(used[0]["reference"], "M1");
    assert_eq!(used[0]["subject"], "timezone preference");

    let requests = mock.server.received_requests().await.unwrap_or_default();
    let body: serde_json::Value =
        serde_json::from_slice(&requests.last().expect("provider request").body).unwrap();
    let system = body["messages"][0]["content"].as_str().unwrap_or_default();
    assert!(system.contains("<memory>"));
    assert!(system.contains("reports must use UTC"));

    let audit = app
        .state()
        .store
        .app_db()
        .list_ai_memory_audit(&created.id)
        .unwrap();
    assert!(audit.iter().any(|entry| entry.action == "inject"));
}

#[tokio::test]
async fn chat_auto_is_forced_off_when_memory_setting_is_disabled() {
    use datazen::ai::{ChatMessage, MessageRole};

    let (test, mock) = TestAppState::with_wiremock_ai().await;
    let app = host_command_app(test);
    enable_memory(&app).await;
    let created = ai_memory_create(
        app.state(),
        input("timezone preference", "question: reports must use UTC"),
    )
    .await
    .unwrap();
    disable_memory(&app).await;
    mock.mount_chat_stream_text("SELECT 1").await;

    ai_chat(
        app.state(),
        app.window(),
        None,
        None,
        vec![ChatMessage {
            role: MessageRole::User,
            content: "question: what timezone?".into(),
            reasoning: None,
            tool_calls: None,
            tool_call_id: None,
        }],
        "req-memory-disabled".into(),
        false,
        None,
        None,
        None,
        None,
        Some("conn-memory".into()),
        Some(datazen::ai::AiAgentMode::Ask),
        None,
        None,
        None,
        None,
        None,
        None,
        Some(MemoryMode::Auto),
        Some(MemoryScopeKind::User),
        None,
    )
    .await
    .unwrap();
    mock.mount_chat_stream_text("SELECT 1").await;

    let requests = mock.server.received_requests().await.unwrap_or_default();
    let body: serde_json::Value =
        serde_json::from_slice(&requests.last().expect("provider request").body).unwrap();
    let system = body["messages"][0]["content"].as_str().unwrap_or_default();
    assert!(!system.contains("<memory>"));
    let audit = app
        .state()
        .store
        .app_db()
        .list_ai_memory_audit(&created.id)
        .unwrap();
    assert!(!audit.iter().any(|entry| entry.action == "inject"));
}

#[tokio::test]
async fn chat_off_and_ask_never_inject_memory() {
    use datazen::ai::{ChatMessage, MessageRole};
    use std::sync::{Arc, Mutex};
    use tauri::Listener;

    let (test, mock) = TestAppState::with_wiremock_ai().await;
    let app = host_command_app(test);
    enable_memory(&app).await;
    let created = ai_memory_create(
        app.state(),
        input("timezone preference", "question: reports must use UTC"),
    )
    .await
    .unwrap();
    mock.mount_chat_stream_text("SELECT 1").await;

    let chunks = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let candidates = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let captured = chunks.clone();
    app.window().listen("ai:stream-chunk", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            captured.lock().unwrap().push(payload);
        }
    });
    let captured_candidates = candidates.clone();
    app.window().listen("ai:memory-candidates", move |event| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(event.payload()) {
            captured_candidates.lock().unwrap().push(payload);
        }
    });

    for (request_id, mode) in [
        ("req-memory-off", MemoryMode::Off),
        ("req-memory-ask", MemoryMode::Ask),
    ] {
        ai_chat(
            app.state(),
            app.window(),
            None,
            None,
            vec![ChatMessage {
                role: MessageRole::User,
                content: "question: what timezone?".into(),
                reasoning: None,
                tool_calls: None,
                tool_call_id: None,
            }],
            request_id.into(),
            false,
            None,
            None,
            None,
            None,
            Some("conn-memory".into()),
            Some(datazen::ai::AiAgentMode::Ask),
            None,
            None,
            None,
            None,
            None,
            None,
            Some(mode),
            Some(MemoryScopeKind::User),
            None,
        )
        .await
        .unwrap();
        mock.mount_chat_stream_text("SELECT 1").await;
    }

    assert!(chunks
        .lock()
        .unwrap()
        .iter()
        .all(|chunk| chunk.get("memoryUsed").is_none()));
    assert_eq!(candidates.lock().unwrap().len(), 1);
    let requests = mock.server.received_requests().await.unwrap_or_default();
    for request in requests {
        let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
        let system = body["messages"][0]["content"].as_str().unwrap_or_default();
        assert!(!system.contains("<memory>"), "off/ask must not inject");
    }
}

#[tokio::test]
async fn strict_cloud_retrieval_excludes_internal_memories() {
    let app = app().await;
    enable_memory(&app).await;
    let mut internal = input("internal-only", "question: keep this on device");
    internal.sensitivity = Some(MemorySensitivity::Internal);
    ai_memory_create(app.state(), internal).await.unwrap();

    let cloud = resolve_chat_memory(
        &app.state(),
        MemoryMode::Auto,
        Some(MemoryScopeKind::User),
        None,
        None,
        "question",
        true,
        AiDataEgressLevel::Strict,
        datazen::ai::AiMemoryCloudPolicy::AllowInternal,
    )
    .await
    .unwrap();
    assert!(cloud.injection.is_none());
    assert!(cloud.used.is_empty());

    let local = resolve_chat_memory(
        &app.state(),
        MemoryMode::Auto,
        Some(MemoryScopeKind::User),
        None,
        None,
        "question",
        false,
        AiDataEgressLevel::Strict,
        datazen::ai::AiMemoryCloudPolicy::NormalOnly,
    )
    .await
    .unwrap();
    assert_eq!(local.used.len(), 1);

    // "Normal only" filters internal material before the injector sees it.
    let normal_only = resolve_chat_memory(
        &app.state(),
        MemoryMode::Auto,
        Some(MemoryScopeKind::User),
        None,
        None,
        "question",
        true,
        AiDataEgressLevel::Unrestricted,
        datazen::ai::AiMemoryCloudPolicy::NormalOnly,
    )
    .await
    .unwrap();
    assert!(normal_only.used.is_empty());

    // "Disable cloud" keeps cloud providers memory-free while local providers
    // still retrieve it.
    let disabled_cloud = resolve_chat_memory(
        &app.state(),
        MemoryMode::Auto,
        Some(MemoryScopeKind::User),
        None,
        None,
        "question",
        true,
        AiDataEgressLevel::Unrestricted,
        datazen::ai::AiMemoryCloudPolicy::DisableCloud,
    )
    .await
    .unwrap();
    assert!(disabled_cloud.used.is_empty());
    let local_allowed = resolve_chat_memory(
        &app.state(),
        MemoryMode::Auto,
        Some(MemoryScopeKind::User),
        None,
        None,
        "question",
        false,
        AiDataEgressLevel::Unrestricted,
        datazen::ai::AiMemoryCloudPolicy::DisableCloud,
    )
    .await
    .unwrap();
    assert_eq!(local_allowed.used.len(), 1);
}

#[tokio::test]
async fn database_scope_memory_is_recalled_with_host_qualified_id() {
    let app = app().await;
    enable_memory(&app).await;
    let mut memory = input("database timezone", "question: database uses UTC");
    memory.scope_type = MemoryScopeKind::Database;
    memory.connection_id = Some("conn-1".into());
    memory.database_name = Some("db-1".into());
    let created = ai_memory_create(app.state(), memory).await.unwrap();
    assert_eq!(created.scope_id, "conn-1/db-1");

    let found = resolve_chat_memory(
        &app.state(),
        MemoryMode::Auto,
        Some(MemoryScopeKind::Database),
        Some("conn-1"),
        Some("db-1"),
        "question",
        false,
        AiDataEgressLevel::Unrestricted,
        datazen::ai::AiMemoryCloudPolicy::NormalOnly,
    )
    .await
    .unwrap();
    assert_eq!(found.used.len(), 1);
    assert_eq!(found.used[0].id, created.id);
}

async fn enable_auto_learn(app: &HostedCommandApp) {
    enable_memory(app).await;
    let mut settings = app.state().store.get_ai_settings_config().await;
    settings.memory.auto_learn = true;
    app.state()
        .store
        .save_ai_settings_config(&settings)
        .await
        .unwrap();
}

#[tokio::test]
async fn correction_candidate_stays_pending_and_literal_only_edits_are_ignored() {
    let app = app().await;
    enable_auto_learn(&app).await;
    let corrected = ai_memory_correction_candidate(
        app.state(),
        AiMemoryCorrectionInput {
            original_sql: "SELECT * FROM orders o JOIN users u ON o.user_id = u.id".into(),
            new_sql: "SELECT * FROM orders o JOIN users u ON o.customer_id = u.id".into(),
            connection_id: Some("conn-1".into()),
            database: Some("sales".into()),
        },
    )
    .await
    .unwrap()
    .expect("correction candidate");
    assert_eq!(corrected.status, MemoryStatus::Candidate);
    assert_eq!(corrected.source, MemorySource::Correction);
    assert_eq!(corrected.kind, MemoryKind::Correction);
    assert_eq!(corrected.connection_id.as_deref(), Some("conn-1"));
    assert_eq!(corrected.database_name.as_deref(), Some("sales"));
    assert!(!corrected.conflict);
    assert!(corrected
        .structured_json
        .as_ref()
        .and_then(|value| value.get("diffSummary"))
        .is_some());
    let repeated = ai_memory_correction_candidate(
        app.state(),
        AiMemoryCorrectionInput {
            original_sql: "SELECT * FROM orders o JOIN users u ON o.user_id = u.id".into(),
            new_sql: "SELECT * FROM orders o JOIN users u ON o.customer_id = u.id".into(),
            connection_id: Some("conn-1".into()),
            database: Some("sales".into()),
        },
    )
    .await
    .unwrap()
    .expect("deduped correction");
    assert_eq!(repeated.id, corrected.id);

    let before = list_all(&app).await.total;
    let literal_only = ai_memory_correction_candidate(
        app.state(),
        AiMemoryCorrectionInput {
            original_sql: "SELECT * FROM orders WHERE id = 1".into(),
            new_sql: "SELECT * FROM orders WHERE id = 2".into(),
            connection_id: Some("conn-1".into()),
            database: Some("sales".into()),
        },
    )
    .await
    .unwrap();
    assert!(literal_only.is_none());
    assert_eq!(list_all(&app).await.total, before);
}

#[tokio::test]
async fn candidate_extraction_is_off_when_auto_learn_is_off() {
    let app = app().await;
    enable_memory(&app).await;
    let corrected = ai_memory_correction_candidate(
        app.state(),
        AiMemoryCorrectionInput {
            original_sql: "SELECT * FROM orders o JOIN users u ON o.user_id = u.id".into(),
            new_sql: "SELECT * FROM orders o JOIN users u ON o.customer_id = u.id".into(),
            connection_id: None,
            database: None,
        },
    )
    .await
    .unwrap();
    assert!(corrected.is_none());
    let extracted = ai_memory_extract_candidates(
        app.state(),
        AiMemoryExtractionInput {
            sql: "SELECT * FROM orders o JOIN users u ON o.user_id = u.id".into(),
            connection_id: None,
            database: None,
            history: Vec::new(),
            window_days: None,
            min_occurrences: None,
            model_output_json: None,
        },
    )
    .await
    .unwrap();
    assert!(extracted.is_empty());
    assert_eq!(list_all(&app).await.total, 0);
}

#[tokio::test]
async fn conflicting_candidate_preserves_active_memory_and_marks_conflict() {
    let app = app().await;
    enable_memory(&app).await;
    let active = ai_memory_create(app.state(), input("Report locale", "Use en-US for reports"))
        .await
        .unwrap();
    let mut candidate_input = input("Report locale", "Use en-GB for reports");
    candidate_input.status = Some(MemoryStatus::Candidate);
    candidate_input.source = Some(MemorySource::Correction);
    let candidate = ai_memory_create(app.state(), candidate_input)
        .await
        .unwrap();
    assert_eq!(candidate.status, MemoryStatus::Candidate);
    assert!(candidate.conflict);
    let structured = candidate.structured_json.unwrap();
    assert_eq!(structured["conflict"], true);
    assert_eq!(structured["conflictWith"][0], active.id.as_str());
    assert!(structured.get("conflictResolution").is_some());

    let all = list_all(&app).await;
    assert_eq!(all.total, 2);
    let still_active = all
        .entries
        .iter()
        .find(|entry| entry.id == active.id)
        .expect("active row");
    assert_eq!(still_active.status, MemoryStatus::Active);
    assert_eq!(still_active.content, "Use en-US for reports");

    let confirmed = ai_memory_confirm(app.state(), candidate.id.clone())
        .await
        .unwrap();
    assert_eq!(confirmed.status, MemoryStatus::Active);
    assert!(!confirmed.conflict);
    assert_eq!(
        confirmed
            .structured_json
            .as_ref()
            .and_then(|value| value.get("conflictResolved"))
            .and_then(serde_json::Value::as_bool),
        Some(true)
    );
    let all = list_all(&app).await;
    assert_eq!(
        all.entries
            .iter()
            .filter(|entry| entry.status == MemoryStatus::Active)
            .count(),
        1,
        "PRD requires exactly one active row per slot"
    );
    let old = all
        .entries
        .iter()
        .find(|entry| entry.id == active.id)
        .expect("old active row");
    assert_eq!(old.status, MemoryStatus::Archived);
    assert_eq!(confirmed.content, "Use en-GB for reports");
}

#[tokio::test]
async fn extraction_creates_repeated_pattern_candidate_with_strict_json_fallback() {
    let app = app().await;
    enable_auto_learn(&app).await;
    let sql = "SELECT DATE_TRUNC('week', paid_at), SUM(amount) FROM orders o JOIN users u ON o.user_id = u.id WHERE o.status != 'legacy' GROUP BY 1";
    let history = vec![
        AiMemorySqlSample {
            sql: sql.into(),
            observed_at: now() - chrono::Duration::days(2),
        },
        AiMemorySqlSample {
            sql: sql.into(),
            observed_at: now() - chrono::Duration::days(1),
        },
    ];
    let candidates = ai_memory_extract_candidates(
        app.state(),
        AiMemoryExtractionInput {
            sql: sql.into(),
            connection_id: Some("conn-1".into()),
            database: Some("sales".into()),
            history,
            window_days: Some(14),
            min_occurrences: Some(3),
            model_output_json: Some("not valid json".into()),
        },
    )
    .await
    .unwrap();
    assert!(candidates
        .iter()
        .any(|candidate| candidate.kind == MemoryKind::Pattern
            && candidate.status == MemoryStatus::Candidate));
    assert!(candidates
        .iter()
        .all(|candidate| candidate.status == MemoryStatus::Candidate));
    assert!(
        candidates
            .iter()
            .filter(|candidate| candidate.kind == MemoryKind::Pattern)
            .count()
            >= 1
    );
    let rerun = ai_memory_extract_candidates(
        app.state(),
        AiMemoryExtractionInput {
            sql: sql.into(),
            connection_id: Some("conn-1".into()),
            database: Some("sales".into()),
            history: Vec::new(),
            window_days: None,
            min_occurrences: None,
            model_output_json: None,
        },
    )
    .await
    .unwrap();
    assert!(rerun
        .iter()
        .all(|candidate| !candidates.iter().any(|first| first.id == candidate.id)));
    assert_eq!(list_all(&app).await.total, candidates.len());
}

#[tokio::test]
async fn schema_deletion_marks_table_memories_stale() {
    let app = app().await;
    enable_memory(&app).await;
    let mut table_input = input("Orders grain", "One row per order");
    table_input.scope_type = MemoryScopeKind::Table;
    table_input.connection_id = Some("conn-1".into());
    table_input.database_name = Some("sales".into());
    table_input.table_name = Some("orders".into());
    let table_memory = ai_memory_create(app.state(), table_input).await.unwrap();
    let mut unrelated = input("Other grain", "One row per customer");
    unrelated.scope_type = MemoryScopeKind::Table;
    unrelated.connection_id = Some("conn-1".into());
    unrelated.database_name = Some("sales".into());
    unrelated.table_name = Some("customers".into());
    let unrelated = ai_memory_create(app.state(), unrelated).await.unwrap();

    let stale = ai_memory_mark_stale(
        app.state(),
        AiMemoryStaleInput {
            connection_id: Some("conn-1".into()),
            database: Some("sales".into()),
            schema: None,
            table: Some("orders".into()),
            column: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(stale.len(), 1);
    assert_eq!(stale[0].id, table_memory.id);
    assert_eq!(stale[0].status, MemoryStatus::Stale);

    let all = list_all(&app).await;
    let table = all
        .entries
        .iter()
        .find(|entry| entry.id == table_memory.id)
        .unwrap();
    assert_eq!(table.status, MemoryStatus::Stale);
    let other = all
        .entries
        .iter()
        .find(|entry| entry.id == unrelated.id)
        .unwrap();
    assert_eq!(other.status, MemoryStatus::Active);

    let injected = resolve_chat_memory(
        &app.state(),
        MemoryMode::Auto,
        Some(MemoryScopeKind::User),
        Some("conn-1"),
        Some("sales"),
        "orders grain",
        false,
        AiDataEgressLevel::Strict,
        datazen::ai::AiMemoryCloudPolicy::NormalOnly,
    )
    .await
    .unwrap();
    assert!(injected.injection.is_none());
    assert!(injected.used.is_empty());
}
