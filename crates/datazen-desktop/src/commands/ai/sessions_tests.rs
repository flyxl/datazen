use super::*;
use crate::commands::ai::command_test_host::{host_command_app, HostedCommandApp};
use crate::testing::app_state_test_support::TestAppState;

async fn app() -> HostedCommandApp {
    host_command_app(TestAppState::new().await)
}

fn message(id: &str, role: &str, content: &str, status: &str) -> AiSessionMessageInput {
    AiSessionMessageInput {
        id: Some(id.into()),
        role: role.into(),
        content: content.into(),
        status: Some(status.into()),
        created_at: None,
        metadata: Some(serde_json::json!({"memoryUsed": [{"id": "m1"}]})),
    }
}

fn session_input(id: &str, key: &str, database: &str) -> AiSessionUpsertInput {
    AiSessionUpsertInput {
        id: Some(id.into()),
        session_key: key.into(),
        connection_id: Some("conn-1".into()),
        database: Some(database.into()),
        title: Some("Debug report".into()),
        metadata: Some(serde_json::json!({"templateId": "default", "memoryMode": "auto"})),
        last_access_at: None,
        messages: vec![
            message("m1", "user", "question", "complete"),
            message("m2", "assistant", "answer", "complete"),
            message("m3", "assistant", "stopped", "cancelled"),
            message("m4", "assistant", "failed", "failed"),
        ],
        append: false,
    }
}

#[tokio::test]
async fn session_ipc_round_trips_status_metadata_list_and_export() {
    let app = app().await;
    let saved = ai_sessions_upsert(app.state(), session_input("s1", "conn-1::db", "db"))
        .await
        .unwrap();
    assert_eq!(saved.id, "s1");
    assert_eq!(saved.messages.len(), 4);
    assert_eq!(saved.message_count, 4);
    assert_eq!(saved.preview, "question");
    assert_eq!(saved.messages[2].status, "cancelled");
    assert_eq!(saved.messages[3].status, "failed");
    assert_eq!(
        saved.messages[0].metadata.as_ref().unwrap()["memoryUsed"][0]["id"],
        "m1"
    );
    assert!(saved.metadata.as_ref().unwrap()["templateId"].is_string());

    let listed = ai_sessions_list(
        app.state(),
        Some("conn-1".into()),
        Some("db".into()),
        Some("conn-1::db".into()),
    )
    .await
    .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].message_count, 4);
    assert_eq!(listed[0].preview, "question");
    assert!(
        ai_sessions_list(app.state(), Some("conn-2".into()), Some("db".into()), None,)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(ai_sessions_list(
        app.state(),
        Some("conn-1".into()),
        Some("other-db".into()),
        None,
    )
    .await
    .unwrap()
    .is_empty());

    let loaded = ai_sessions_get(app.state(), "s1".into()).await.unwrap();
    assert_eq!(loaded.messages[0].content, "question");

    let json = ai_sessions_export(app.state(), "s1".into(), "json".into())
        .await
        .unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed["messageCount"], 4);
    assert_eq!(parsed["preview"], "question");
    assert_eq!(parsed["messages"][2]["status"], "cancelled");
    let markdown = ai_sessions_export(app.state(), "s1".into(), "markdown".into())
        .await
        .unwrap();
    assert!(markdown.contains("# Debug report"));
    assert!(markdown.contains("## Assistant (cancelled)"));
    assert!(markdown.contains("## Assistant (failed)"));
}

#[tokio::test]
async fn session_upsert_replaces_then_appends_and_delete_cascades() {
    let app = app().await;
    ai_sessions_upsert(app.state(), session_input("s1", "conn-1::db", "db"))
        .await
        .unwrap();

    let mut replacement = session_input("s1", "conn-1::db", "db");
    replacement.messages = vec![message("m5", "user", "new question", "complete")];
    let replaced = ai_sessions_upsert(app.state(), replacement.clone())
        .await
        .unwrap();
    assert_eq!(replaced.messages.len(), 1);
    assert_eq!(replaced.messages[0].content, "new question");

    replacement.append = true;
    replacement.messages = vec![message("m6", "assistant", "appended", "complete")];
    let appended = ai_sessions_upsert(app.state(), replacement.clone())
        .await
        .unwrap();
    assert_eq!(appended.messages.len(), 2);
    // Re-appending the same id is idempotent.
    let appended_again = ai_sessions_upsert(app.state(), replacement).await.unwrap();
    assert_eq!(appended_again.messages.len(), 2);

    assert!(ai_sessions_delete(app.state(), "s1".into()).await.unwrap());
    assert!(ai_sessions_get(app.state(), "s1".into()).await.is_err());
    assert!(!ai_sessions_delete(app.state(), "s1".into()).await.unwrap());
}

#[tokio::test]
async fn session_upsert_requires_a_key_and_valid_scope() {
    let app = app().await;
    let mut input = session_input("s1", " ", "db");
    assert!(ai_sessions_upsert(app.state(), input.clone())
        .await
        .is_err());
    input.session_key = "conn-1::db".into();
    input.connection_id = Some("conn-1".into());
    assert!(ai_sessions_upsert(app.state(), input).await.is_ok());
}
