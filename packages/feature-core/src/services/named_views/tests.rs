//! The shared named-view rules, exercised without a store.
//!
//! The repository here is an in-memory one, so these cases say what the
//! rules do — not what a particular storage does with them. The desktop
//! store's own suite (`src-tauri/src/store/tests/named_views_tests.rs`)
//! covers the persistence.

use std::collections::HashSet;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use datazen_driver_api::filters::{FilterCondition, FilterConnector, FilterOperator};

use super::{
    delete_named_view, list_named_views, normalize_view_name, resolve_import_name, save_named_view,
    scope_key, view_names_equal, NamedView, NamedViewError, NamedViewIdMinter, NamedViewRepository,
    NamedViewScope,
};

/**
 * The key sets of the serialized record.
 *
 * They are pinned here **and** in the frontend (`src/lib/__tests__/namedViews.test.ts`),
 * with the same literals: a field that appears on one side only is a contract
 * break that neither side can see alone.
 */
const NAMED_VIEW_KEYS: &str =
    "columnOrder,createdAt,filterLogic,filters,id,isDefault,name,scope,sorts,updatedAt,visibleColumns";
const SCOPE_KEYS: &str = "connectionId,database,schema,table";
const FILTER_KEYS_WITH_CONNECTOR: &str = "column,connector,operator,value";
const FILTER_KEYS_WITHOUT_CONNECTOR: &str = "column,operator,value";

/// An in-memory repository: the whole collection, nothing else.
#[derive(Default)]
struct MemoryRepository {
    views: Mutex<Vec<NamedView>>,
    fail_next_load: Mutex<bool>,
}

#[async_trait::async_trait]
impl NamedViewRepository for MemoryRepository {
    async fn load_views(&self) -> Result<Vec<NamedView>, NamedViewError> {
        if *self.fail_next_load.lock().expect("lock") {
            return Err(NamedViewError::Storage("disk is on fire".into()));
        }
        Ok(self.views.lock().expect("lock").clone())
    }

    async fn save_views(&self, views: &[NamedView]) -> Result<(), NamedViewError> {
        *self.views.lock().expect("lock") = views.to_vec();
        Ok(())
    }
}

/// A minter that counts, so a test can tell a minted id from a kept one.
struct CountingMinter {
    counter: Mutex<u32>,
}

impl CountingMinter {
    fn new() -> Self {
        Self {
            counter: Mutex::new(0),
        }
    }
}

impl NamedViewIdMinter for CountingMinter {
    fn mint_id(&self) -> String {
        let mut counter = self.counter.lock().expect("lock");
        *counter += 1;
        format!("view-{counter:04}")
    }
}

fn scope(table: &str) -> NamedViewScope {
    NamedViewScope {
        connection_id: "conn-1".into(),
        database: Some("app".into()),
        schema: None,
        table: table.into(),
    }
}

fn condition(column: &str, connector: Option<FilterConnector>) -> FilterCondition {
    FilterCondition {
        column: column.into(),
        operator: FilterOperator::Eq,
        value: datazen_driver_api::Value::String("x".into()),
        connector,
    }
}

fn view(table: &str, name: &str) -> NamedView {
    NamedView {
        id: String::new(),
        name: name.into(),
        scope: scope(table),
        filters: vec![condition("id", None)],
        filter_logic: "and".into(),
        sorts: Vec::new(),
        visible_columns: None,
        column_order: None,
        is_default: false,
        created_at: Utc::now(),
        updated_at: None,
    }
}

fn sorted_keys(value: &serde_json::Value) -> String {
    let mut keys: Vec<String> = value
        .as_object()
        .expect("an object")
        .keys()
        .cloned()
        .collect();
    keys.sort();
    keys.join(",")
}

// ── The name and scope rules ────────────────────────────────────────────

#[test]
fn normalize_view_name_trims_and_collapses_whitespace() {
    assert_eq!(normalize_view_name("  Open   orders  "), "Open orders");
    assert_eq!(normalize_view_name("Open\torders\nnow"), "Open orders now");
    assert_eq!(normalize_view_name("   "), "");
}

#[test]
fn view_names_equal_ignores_case_and_whitespace() {
    assert!(view_names_equal("Users", "users"));
    assert!(view_names_equal("  Open   orders ", "open orders"));
    assert!(!view_names_equal("Users", "User"));
}

#[test]
fn scope_key_separates_a_missing_database_from_an_empty_one() {
    let mut with_none = scope("users");
    with_none.database = None;
    let mut with_empty = scope("users");
    with_empty.database = Some(String::new());

    assert_ne!(scope_key(&with_none), scope_key(&with_empty));
    assert_ne!(scope_key(&with_none), scope_key(&scope("orders")));
    assert_eq!(scope_key(&scope("users")), scope_key(&scope("users")));
}

#[test]
fn resolve_import_name_keeps_a_free_name_and_suffixes_a_taken_one() {
    let taken: HashSet<String> = ["users".to_string()].into_iter().collect();
    assert_eq!(resolve_import_name("Orders", &taken), "Orders");
    // The comparison is case-insensitive, and the case of the imported name
    // is kept: "Users (2)", not "users (2)".
    assert_eq!(resolve_import_name("Users", &taken), "Users (2)");
    assert_eq!(
        resolve_import_name(
            "Users",
            &["users".to_string(), "Users (2)".to_string()]
                .into_iter()
                .collect()
        ),
        "Users (3)"
    );
}

// ── The validation rules ────────────────────────────────────────────────

#[test]
fn validate_refuses_an_empty_name() {
    let mut record = view("users", "   ");
    record.normalize();
    let error = record.validate().expect_err("an empty name is refused");
    assert!(error.contains("name"), "{error}");
}

#[test]
fn validate_refuses_an_unknown_filter_logic() {
    let mut record = view("users", "Users");
    record.filter_logic = "xor".into();
    let error = record.validate().expect_err("xor is not a logic");
    assert!(error.contains("filter logic"), "{error}");
}

#[test]
fn validate_refuses_a_condition_without_a_column() {
    let mut record = view("users", "Users");
    record.filters = vec![condition("", None)];
    let error = record.validate().expect_err("a condition needs a column");
    assert!(error.contains("column"), "{error}");
}

#[test]
fn validate_accepts_a_well_formed_record() {
    let mut record = view("users", "Users");
    record.normalize();
    assert!(record.validate().is_ok());
}

// ── The service rules ───────────────────────────────────────────────────

#[tokio::test]
async fn save_mints_an_id_and_stamps_the_timestamps() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();

    let saved = save_named_view(&repository, &minter, view("users", "Users"))
        .await
        .expect("saved");

    assert_eq!(saved.id, "view-0001");
    assert!(saved.created_at <= Utc::now());
    assert!(saved.updated_at.is_some());
}

#[tokio::test]
async fn save_refuses_a_duplicate_name_in_the_same_scope() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();
    save_named_view(&repository, &minter, view("users", "Users"))
        .await
        .expect("saved");

    let error = save_named_view(&repository, &minter, view("users", "users"))
        .await
        .expect_err("the name is taken");
    assert!(matches!(error, NamedViewError::NameExists { .. }));
    assert!(
        error.to_string().starts_with("grid.view.nameExists: "),
        "{error}"
    );
}

/// D1: a host message must not quote what the user typed.
///
/// The name is whatever the user typed, and a host message reaches the IPC
/// payload and the host's log lines — where the redactor does not cover free
/// text. So the message says only what the host knows, and the name stays
/// structured.
#[tokio::test]
async fn a_duplicate_name_message_never_quotes_the_name() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();
    save_named_view(&repository, &minter, view("users", "password=hunter2"))
        .await
        .expect("saved");

    let error = save_named_view(&repository, &minter, view("users", "password=hunter2"))
        .await
        .expect_err("the name is taken");

    let message = error.to_string();
    assert!(
        message.starts_with("grid.view.nameExists: "),
        "the code stays at the front: {message}"
    );
    assert!(
        !message.contains("hunter2"),
        "the message must not carry the name: {message}"
    );
    assert!(
        !message.contains("password="),
        "the message must not carry the name: {message}"
    );

    // The name is still there — as structured data, for the caller that
    // wants to say which name was taken.
    assert_eq!(error.name(), Some("password=hunter2"));
    assert_eq!(error.code(), Some("grid.view.nameExists"));
}

/// D1: the same rule for the other free-text field — the filter logic.
#[tokio::test]
async fn an_invalid_filter_logic_message_never_quotes_the_value() {
    let mut record = view("users", "Users");
    record.filter_logic = "password=hunter2".into();
    let error = record
        .validate()
        .expect_err("a filter logic must be and/or");

    assert!(error.starts_with("the filter logic must be"), "{error}");
    assert!(
        !error.contains("hunter2"),
        "the message must not carry the value: {error}"
    );
    assert!(
        !error.contains("password="),
        "the message must not carry the value: {error}"
    );
}

#[tokio::test]
async fn save_allows_the_same_name_in_another_scope() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();
    save_named_view(&repository, &minter, view("users", "Open"))
        .await
        .expect("saved");

    // Another table of the same connection.
    save_named_view(&repository, &minter, view("orders", "Open"))
        .await
        .expect("another scope may carry the same name");

    // Another connection's table of the same name.
    let mut other_connection = view("users", "Open");
    other_connection.scope.connection_id = "conn-2".into();
    save_named_view(&repository, &minter, other_connection)
        .await
        .expect("another connection may carry the same name");
}

#[tokio::test]
async fn rename_keeps_the_id_and_moves_the_update_time() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();
    let saved = save_named_view(&repository, &minter, view("users", "Users"))
        .await
        .expect("saved");

    let mut renamed = saved.clone();
    renamed.name = "Open users".into();
    let updated = save_named_view(&repository, &minter, renamed)
        .await
        .expect("renamed");

    assert_eq!(updated.id, saved.id, "a rename keeps the identity");
    assert_eq!(updated.created_at, saved.created_at);
    let views = list_named_views(&repository, None).await.expect("listed");
    assert_eq!(views.len(), 1, "a rename does not add a record");
    assert_eq!(views[0].name, "Open users");
}

#[tokio::test]
async fn only_one_default_per_scope() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();

    let mut first = view("users", "First");
    first.is_default = true;
    save_named_view(&repository, &minter, first)
        .await
        .expect("saved");

    let mut second = view("users", "Second");
    second.is_default = true;
    save_named_view(&repository, &minter, second)
        .await
        .expect("saved");

    let views = list_named_views(&repository, Some(&scope("users")))
        .await
        .expect("listed");
    assert_eq!(
        views.iter().filter(|entry| entry.is_default).count(),
        1,
        "exactly one default per scope"
    );
    assert!(views[0].is_default, "the default comes first");
}

#[tokio::test]
async fn delete_is_idempotent() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();
    let saved = save_named_view(&repository, &minter, view("users", "Users"))
        .await
        .expect("saved");

    let remaining = delete_named_view(&repository, &saved.id)
        .await
        .expect("deleted");
    assert!(remaining.is_empty());

    // Deleting it again changes nothing and is not an error.
    let remaining = delete_named_view(&repository, &saved.id)
        .await
        .expect("deleted again");
    assert!(remaining.is_empty());
}

#[tokio::test]
async fn list_orders_the_default_first_and_then_by_name() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();

    for name in ["Zulu", "alpha", "Mike"] {
        save_named_view(&repository, &minter, view("users", name))
            .await
            .expect("saved");
    }
    let mut default = view("users", "Default one");
    default.is_default = true;
    save_named_view(&repository, &minter, default)
        .await
        .expect("saved");

    let views = list_named_views(&repository, Some(&scope("users")))
        .await
        .expect("listed");
    let names: Vec<&str> = views.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["Default one", "alpha", "Mike", "Zulu"],
        "the default first, then the names in dictionary order"
    );
}

#[tokio::test]
async fn list_filters_by_scope() {
    let repository = MemoryRepository::default();
    let minter = CountingMinter::new();
    save_named_view(&repository, &minter, view("users", "Users"))
        .await
        .expect("saved");
    save_named_view(&repository, &minter, view("orders", "Orders"))
        .await
        .expect("saved");

    assert_eq!(
        list_named_views(&repository, Some(&scope("users")))
            .await
            .expect("listed")
            .len(),
        1
    );
    assert_eq!(
        list_named_views(&repository, None)
            .await
            .expect("listed")
            .len(),
        2
    );
}

#[tokio::test]
async fn a_failing_store_is_reported_without_a_grid_code() {
    let repository = MemoryRepository::default();
    *repository.fail_next_load.lock().expect("lock") = true;
    let error = list_named_views(&repository, None)
        .await
        .expect_err("the store failed");
    assert!(matches!(error, NamedViewError::Storage(_)));
    assert!(
        !error.to_string().starts_with("grid."),
        "a disk failure is not a grid rule: {error}"
    );
}

// ── The serialized shape ────────────────────────────────────────────────

#[test]
fn the_serialized_shape_is_the_one_the_frontend_expects() {
    let mut record = view("users", "Users");
    record.id = "view-0001".into();
    record.filters = vec![
        condition("id", Some(FilterConnector::Or)),
        condition("name", None),
    ];
    record.created_at = DateTime::from_timestamp_millis(1_700_000_000_000).expect("a timestamp");

    let json = serde_json::to_value(&record).expect("serialized");

    assert_eq!(sorted_keys(&json), NAMED_VIEW_KEYS);
    assert_eq!(sorted_keys(&json["scope"]), SCOPE_KEYS);
    assert_eq!(sorted_keys(&json["filters"][0]), FILTER_KEYS_WITH_CONNECTOR);
    assert_eq!(
        sorted_keys(&json["filters"][1]),
        FILTER_KEYS_WITHOUT_CONNECTOR,
        "an absent connector is not written"
    );
    assert_eq!(json["filters"][0]["connector"], "or");
    // `updatedAt` is nullable on the wire: a record that was never changed
    // carries `null`, and the frontend type says exactly that.
    assert!(json["updatedAt"].is_null());

    let mut touched = record.clone();
    touched.updated_at = Some(Utc::now());
    let json = serde_json::to_value(&touched).expect("serialized");
    assert!(json["updatedAt"].is_string());
}
