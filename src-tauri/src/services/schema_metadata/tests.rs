use super::*;
use crate::commands::driver_command::{execute_driver_command_impl, ExecuteDriverCommandRequest};
use crate::testing::{app_state::TestAppState, mock_driver::MockDriverOptions};

async fn run(
    test: &TestAppState,
    session: &str,
    command: &str,
    input: Value,
) -> Result<CommandResult, CommandError> {
    execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(session.into()),
            driver_type: None,
            command: command.into(),
            input,
            database: None,
            schema: None,
        },
    )
    .await
}

#[tokio::test]
async fn reads_deduplicate_exact_identities_and_cache_distinct_schemas() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-identities").await;
    let input = json!({ "relations": [
        { "database": "app", "schema": "public", "name": "users" },
        { "database": "app", "schema": "archive", "name": "users" },
        { "database": "app", "schema": "public", "name": "users" }
    ] });
    let result = run(&test, &session, "read_relation_columns", input.clone())
        .await
        .unwrap();
    let decoded: ReadColumnsOutput = serde_json::from_value(result.data).unwrap();
    assert_eq!(decoded.results.len(), 2);
    assert_eq!(test.mock.get_columns_calls(), 2);
    run(&test, &session, "read_relation_columns", input)
        .await
        .unwrap();
    assert_eq!(test.mock.get_columns_calls(), 2);
    assert!(test.mock.use_database_calls().is_empty());
}

#[tokio::test]
async fn invalid_target_fails_preflight_before_any_relation_read() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-preflight").await;
    let error = run(
        &test,
        &session,
        "read_relation_columns",
        json!({ "relations": [
        { "database": "app", "schema": "public", "name": "users" },
        { "database": "app", "schema": null, "name": "orders" }
    ] }),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        CommandError::Driver(_) | CommandError::Validation(_)
    ));
    assert_eq!(test.mock.get_columns_calls(), 0);
}

#[tokio::test]
async fn catalog_includes_empty_schemas_without_fake_relations() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        tables: vec![
            datazen_driver_api::TableInfo {
                name: "".into(),
                schema: Some("empty".into()),
                table_type: datazen_driver_api::TableType::Table,
                row_count: None,
            },
            datazen_driver_api::TableInfo {
                name: "users".into(),
                schema: Some("public".into()),
                table_type: datazen_driver_api::TableType::Table,
                row_count: Some(2),
            },
        ],
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-catalog").await;
    let result = run(
        &test,
        &session,
        "list_catalog",
        json!({ "database": "app", "schema": null }),
    )
    .await
    .unwrap();
    let catalog: ListCatalogOutput = serde_json::from_value(result.data).unwrap();
    assert_eq!(catalog.schemas, ["empty", "public"]);
    assert_eq!(catalog.relations.len(), 1);
    assert_eq!(catalog.relations[0].relation.name, "users");
}

#[tokio::test]
async fn full_schema_populates_columns_tier_for_the_same_identity() {
    let test = TestAppState::with_tables().await;
    let (_, session) = test.save_and_connect("metadata-full").await;
    let relation = json!({ "database": "app", "schema": null, "name": "users" });
    let result = run(
        &test,
        &session,
        "read_relation_schema",
        json!({ "relation": relation }),
    )
    .await
    .unwrap();
    assert_eq!(result.data["value"]["ref"]["database"], "app");
    run(
        &test,
        &session,
        "read_relation_columns",
        json!({ "relations": [relation] }),
    )
    .await
    .unwrap();
    assert_eq!(test.mock.get_columns_calls(), 0);
}

#[tokio::test]
async fn metadata_envelope_target_is_rejected_instead_of_ignored() {
    let test = TestAppState::with_tables().await;
    let (_, session) = test.save_and_connect("metadata-envelope").await;
    let error = execute_driver_command_impl(
        &test.state,
        ExecuteDriverCommandRequest {
            db_session_id: Some(session),
            driver_type: None,
            command: "list_catalog".into(),
            input: json!({ "database": "app", "schema": null }),
            database: Some("other".into()),
            schema: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(error, CommandError::Validation(_)));
}

#[tokio::test]
async fn a_batch_preserves_successes_and_redacts_per_relation_failures() {
    let test = TestAppState::with_options(MockDriverOptions {
        column_errors_by_table: std::collections::HashMap::from([(
            "denied".into(),
            "postgres://root:hunter2@localhost/app".into(),
        )]),
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-partial").await;
    let result = run(
        &test,
        &session,
        "read_relation_columns",
        json!({ "relations": [
        { "database": "app", "schema": null, "name": "users" },
        { "database": "app", "schema": null, "name": "denied" }
    ] }),
    )
    .await
    .unwrap();
    assert_eq!(result.data["results"][0]["status"], "ok");
    assert_eq!(result.data["results"][1]["status"], "error");
    assert_eq!(result.data["results"][1]["ref"]["name"], "denied");
    assert_eq!(result.data["results"][1]["error"]["code"], "read-failed");
    assert!(!result.data.to_string().contains("hunter2"));
}

#[tokio::test]
async fn relation_refresh_invalidates_only_the_exact_schema_identity() {
    let test = TestAppState::with_options(MockDriverOptions {
        has_schema_level: true,
        ..Default::default()
    })
    .await;
    let (_, session) = test.save_and_connect("metadata-refresh").await;
    let input = json!({ "relations": [
        { "database": "app", "schema": "public", "name": "users" },
        { "database": "app", "schema": "archive", "name": "users" }
    ] });
    run(&test, &session, "read_relation_columns", input.clone())
        .await
        .unwrap();
    let before = test.state.schema_cache.generation();
    let result = run(
        &test,
        &session,
        "refresh_schema_metadata",
        json!({ "scope": {
        "kind": "relation", "relation": { "database": "app", "schema": "public", "name": "users" }
    } }),
    )
    .await
    .unwrap();
    assert!(result.data["revision"].as_u64().unwrap() > before);
    run(&test, &session, "read_relation_columns", input)
        .await
        .unwrap();
    assert_eq!(test.mock.get_columns_calls(), 3);
}

#[tokio::test]
async fn catalog_preserves_driver_owned_list_tables_command_results() {
    let test = TestAppState::with_options(MockDriverOptions {
        command_results: std::collections::HashMap::from([("list_tables".into(), json!({
            "tables": [{ "name": "custom_collection", "schema": null, "tableType": "table", "rowCount": null }]
        }))]), ..Default::default()
    }).await;
    let (_, session) = test.save_and_connect("metadata-driver-catalog").await;
    let result = run(
        &test,
        &session,
        "list_catalog",
        json!({ "database": "app", "schema": null }),
    )
    .await
    .unwrap();
    let catalog: ListCatalogOutput = serde_json::from_value(result.data).unwrap();
    assert_eq!(catalog.relations.len(), 1);
    assert_eq!(catalog.relations[0].relation.name, "custom_collection");
}
