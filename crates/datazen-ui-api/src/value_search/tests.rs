use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::rc::Rc;

use crate::{
    CellType, FilterConnector, FilterOperator, TableInfo, TableType,
    ValueSearchTypeGroup, ValueSearchTypeScope,
};
use crate::types::ColumnSchema;

use super::*;

fn column(name: &str, data_type: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_owned(),
        data_type: data_type.to_owned(),
        nullable: true,
        default_value: None,
        is_primary_key: None,
        is_auto_increment: None,
        comment: None,
    }
}

fn table(name: &str, schema: Option<&str>, table_type: TableType) -> TableInfo {
    TableInfo {
        name: name.to_owned(),
        schema: schema.map(str::to_owned),
        table_type,
        row_count: None,
    }
}

fn target(table: &str, schema: Option<&str>) -> ValueSearchTarget {
    ValueSearchTarget { table: table.to_owned(), schema: schema.map(str::to_owned) }
}

fn provider(groups: Vec<ValueSearchTypeGroup>) -> Box<ExtraTypesProvider> {
    Box::new(move |_| groups.clone())
}

fn result(table: &str, schema: Option<&str>, matches: Option<i64>) -> ValueSearchTableResult {
    ValueSearchTableResult {
        table: table.to_owned(),
        schema: schema.map(str::to_owned),
        matches,
        columns: Vec::new(),
        reason: None,
    }
}

#[test]
fn mode_literals_are_pinned() {
    assert_eq!(ValueSearchMode::Contains.as_str(), "contains");
    assert_eq!(ValueSearchMode::Exact.as_str(), "exact");
    assert_eq!(ValueSearchMode::Prefix.as_str(), "prefix");
    assert_eq!(ValueSearchMode::Suffix.as_str(), "suffix");
    assert_eq!(ValueSearchMode::NotContains.as_str(), "notContains");
}

#[test]
fn driver_declared_groups_and_scopes() {
    let none = provider(Vec::new());
    assert!(value_search_type_groups_for(None, &*none).is_empty());
    assert_eq!(type_scopes_for_driver(None, &*none), vec![ValueSearchTypeScope::Text]);

    let both = provider(vec![ValueSearchTypeGroup::Number, ValueSearchTypeGroup::Datetime]);
    assert_eq!(type_scopes_for_driver(Some("mysql"), &*both), vec![
        ValueSearchTypeScope::Text,
        ValueSearchTypeScope::TextNumber,
        ValueSearchTypeScope::TextDatetime,
        ValueSearchTypeScope::TextNumberDatetime,
    ]);
    let number = provider(vec![ValueSearchTypeGroup::Number]);
    assert_eq!(type_scopes_for_driver(Some("sqlite"), &*number), vec![
        ValueSearchTypeScope::Text,
        ValueSearchTypeScope::TextNumber,
    ]);
}

#[test]
fn base_and_scope_cell_types() {
    let base = base_cell_types();
    assert!(base.contains(&CellType::Text));
    assert!(base.contains(&CellType::Enum));
    assert!(base.contains(&CellType::Unknown));
    assert!(!base.contains(&CellType::Number));

    let full = cell_types_for(ValueSearchTypeScope::TextNumberDatetime);
    assert!(full.contains(&CellType::Number));
    assert!(full.contains(&CellType::Date));
    assert!(full.contains(&CellType::Time));
    assert!(full.contains(&CellType::DateTime));
    let text = cell_types_for(ValueSearchTypeScope::Text);
    assert!(!text.contains(&CellType::Number));
    assert_eq!(text, base);
}

#[test]
fn searchable_columns_follow_the_cell_type_set() {
    let columns = vec![
        column("name", "varchar(50)"),
        column("amount", "int"),
        column("created", "timestamp"),
        column("mystery", "weird"),
    ];
    let names = searchable_columns(&columns, &base_cell_types());
    assert_eq!(names, vec!["name", "mystery"]);
    let full = cell_types_for(ValueSearchTypeScope::TextNumberDatetime);
    assert_eq!(searchable_columns(&columns, &full), vec!["name", "amount", "created", "mystery"]);
}

#[test]
fn target_key_and_targets_are_schema_qualified_and_tables_only() {
    assert_eq!(target_key(&target("users", Some("public"))), "public.users");
    assert_eq!(target_key(&target("users", None)), "users");
    assert_eq!(target_key(&target("users", Some(""))), "users");
    let tables = vec![
        table("users", Some("public"), TableType::Table),
        table("v", None, TableType::View),
        table("orders", None, TableType::Table),
    ];
    assert_eq!(value_search_targets(&tables), vec![
        target("users", Some("public")),
        target("orders", None),
    ]);
}

#[test]
fn rank_tables_uses_hit_count_desc_and_skips_zero() {
    let results = vec![
        result("a", None, Some(2)),
        result("b", None, Some(7)),
        result("c", None, Some(0)),
        result("d", None, None),
        result("e", None, Some(5)),
    ];
    let ranked = rank_tables(&results, 2);
    assert_eq!(ranked.iter().map(|item| item.table.as_str()).collect::<Vec<_>>(), vec!["b", "e"]);
    assert!(rank_tables(&results, 0).is_empty());
}

#[test]
fn locate_conditions_mirror_the_scan_question() {
    assert_eq!(filter_operator_for_mode(ValueSearchMode::Contains), FilterOperator::Contains);
    assert_eq!(filter_operator_for_mode(ValueSearchMode::NotContains), FilterOperator::NotContains);
    assert_eq!(filter_operator_for_mode(ValueSearchMode::Prefix), FilterOperator::StartsWith);
    assert_eq!(filter_operator_for_mode(ValueSearchMode::Suffix), FilterOperator::EndsWith);
    assert_eq!(filter_operator_for_mode(ValueSearchMode::Exact), FilterOperator::Eq);

    let intent = ValueSearchLocateIntent {
        table: "users".to_owned(),
        schema: Some("public".to_owned()),
        database: "db".to_owned(),
        value: "alice".to_owned(),
        mode: ValueSearchMode::Prefix,
        columns: vec!["name".to_owned(), "nick".to_owned()],
    };
    let conditions = locate_conditions(&intent);
    assert_eq!(conditions.len(), 2);
    assert_eq!(conditions[0].column, "name");
    assert_eq!(conditions[0].operator, FilterOperator::StartsWith);
    assert_eq!(conditions[0].value, Some(crate::Value::String("alice".to_owned())));
    assert_eq!(conditions[1].column, "nick");
    assert_eq!(locate_filter_logic(ValueSearchMode::NotContains), FilterConnector::And);
    assert_eq!(locate_filter_logic(ValueSearchMode::Contains), FilterConnector::Or);
    assert!(locate_conditions(&ValueSearchLocateIntent { columns: Vec::new(), ..intent }).is_empty());
}

struct DriverFixture {
    driver: ValueSearchDriver,
    clock: Rc<Cell<i64>>,
    search_calls: Rc<RefCell<Vec<String>>>,
    fail_table: Rc<RefCell<Option<String>>>,
}

fn fixture(targets: Vec<ValueSearchTarget>) -> DriverFixture {
    let clock = Rc::new(Cell::new(0i64));
    let search_calls = Rc::new(RefCell::new(Vec::new()));
    let fail_table = Rc::new(RefCell::new(None));
    let sink = search_calls.clone();
    let failing = fail_table.clone();
    let time = clock.clone();
    let driver = ValueSearchDriver::new(ValueSearchDriverParams {
        db_session_id: "s1".to_owned(),
        database: "db".to_owned(),
        targets,
        value: "alice".to_owned(),
        mode: ValueSearchMode::Contains,
        cell_types: base_cell_types(),
        read_schema: Box::new(|target| {
            if target.table == "empty" {
                return Ok(vec![column("code", "int")]);
            }
            Ok(vec![column("name", "varchar"), column("code", "int")])
        }),
        search_table: Box::new(move |request| {
            sink.borrow_mut().push(request.table.clone());
            if failing.borrow().as_deref() == Some(request.table.as_str()) {
                return Err("table unreadable".to_owned());
            }
            Ok(SearchTableValuesResponse {
                table: request.table.clone(),
                matches: 3,
                searched_columns: request.columns.clone(),
            })
        }),
        clock: Box::new(move || time.get()),
    });
    DriverFixture { driver, clock, search_calls, fail_table }
}

#[test]
fn driver_idle_progress_and_subscribe_notification() {
    let mut fixture = fixture(vec![target("users", Some("public"))]);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let sink = seen.clone();
    let unsubscribe = fixture.driver.subscribe(Rc::new(RefCell::new(move |progress: &ValueSearchProgress| {
        sink.borrow_mut().push((progress.status, progress.completed));
    })));
    assert_eq!(seen.borrow().len(), 1, "subscribe emits the current state immediately");
    assert_eq!(fixture.driver.total(), 1);
    assert_eq!(fixture.driver.progress().elapsed_ms, None);
    unsubscribe();
    fixture.driver.start();
    assert_eq!(seen.borrow().len(), 1, "unsubscribed listeners are silent");
    fixture.clock.set(10);
    assert_eq!(fixture.driver.progress().elapsed_ms, Some(10), "a running scan reads the clock");
}

#[test]
fn driver_scan_records_per_table_answers_and_finishes() {
    let mut fixture = fixture(vec![target("users", Some("public")), target("empty", None)]);
    fixture.driver.start();
    assert_eq!(fixture.driver.status(), ValueSearchStatus::Running);
    assert_eq!(fixture.driver.run_to_completion(10), 2);
    let progress = fixture.driver.progress();
    assert_eq!(progress.status, ValueSearchStatus::Done);
    assert_eq!(progress.completed, 2);
    assert_eq!(progress.total, 2);
    assert_eq!(progress.current, None);
    assert_eq!(progress.results.len(), 2);
    assert_eq!(progress.results[0].schema.as_deref(), Some("public"));
    assert_eq!(progress.results[0].matches, Some(3));
    assert_eq!(progress.results[0].columns, vec!["name"]);
    assert_eq!(progress.results[1].matches, Some(0), "an empty column set is a zero-match answer");
    assert!(progress.results[1].columns.is_empty());
    assert_eq!(fixture.search_calls.borrow().len(), 1, "the empty table never reaches the search command");
    assert_eq!(fixture.driver.progress().elapsed_ms, Some(0), "elapsed is fixed at completion");
}

#[test]
fn a_failed_table_is_recorded_without_failing_the_scan() {
    let mut fixture = fixture(vec![target("users", None), target("orders", None)]);
    *fixture.fail_table.borrow_mut() = Some("users".to_owned());
    fixture.driver.start();
    fixture.driver.run_to_completion(10);
    let progress = fixture.driver.progress();
    assert_eq!(progress.status, ValueSearchStatus::Done);
    assert_eq!(progress.results[0].matches, None);
    assert_eq!(progress.results[0].reason.as_deref(), Some("table unreadable"));
    assert_eq!(progress.results[1].matches, Some(3), "the other table still ran");
}

#[test]
fn pause_holds_between_tables_and_start_resumes() {
    let mut fixture = fixture(vec![target("users", None), target("orders", None)]);
    fixture.driver.start();
    let token = fixture.driver.token();
    assert!(fixture.driver.run_step_for_token(token));
    fixture.driver.pause();
    assert_eq!(fixture.driver.status(), ValueSearchStatus::Paused);
    assert!(!fixture.driver.run_step_for_token(token), "a paused scan does not advance");
    fixture.driver.start();
    assert_eq!(fixture.driver.status(), ValueSearchStatus::Running);
    assert_eq!(fixture.driver.token(), token, "resume keeps the token");
    fixture.driver.run_to_completion(10);
    assert_eq!(fixture.driver.progress().completed, 2);
}

#[test]
fn cancel_settles_and_stale_tokens_are_rejected() {
    let mut fx = fixture(vec![target("users", None), target("orders", None)]);
    fx.driver.start();
    let token = fx.driver.token();
    fx.driver.cancel();
    assert_eq!(fx.driver.status(), ValueSearchStatus::Cancelled);
    assert!(!fx.driver.run_step_for_token(token));
    assert_eq!(fx.driver.progress().completed, 0);
    fx.driver.cancel();
    assert_eq!(fx.driver.status(), ValueSearchStatus::Cancelled, "double cancel is inert");

    let mut fixture = fixture(vec![target("users", None)]);
    fixture.driver.start();
    let stale = fixture.driver.token();
    fixture.driver.cancel();
    fixture.driver.start();
    assert!(fixture.driver.token() > stale);
    assert!(!fixture.driver.run_step_for_token(stale));
}

#[test]
fn cancel_from_idle_stays_idle_and_done_ignores_start() {
    let mut fixture = fixture(vec![target("users", None)]);
    fixture.driver.cancel();
    assert_eq!(fixture.driver.status(), ValueSearchStatus::Idle);
    fixture.driver.start();
    fixture.driver.run_to_completion(10);
    assert_eq!(fixture.driver.status(), ValueSearchStatus::Done);
    fixture.driver.start();
    assert_eq!(fixture.driver.status(), ValueSearchStatus::Done, "start is inert on a done scan");
    assert_eq!(fixture.search_calls.borrow().len(), 1);
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::value_search_type_groups_for;
    let _ = crate::type_scopes_for_driver;
    let _ = crate::cell_types_for;
    let _ = crate::base_cell_types;
    let _ = crate::searchable_columns;
    let _ = crate::target_key;
    let _ = crate::value_search_targets;
    let _ = crate::rank_tables;
    let _ = crate::filter_operator_for_mode;
    let _ = crate::locate_conditions;
    let _ = crate::locate_filter_logic;
    let _: Option<crate::ValueSearchDriver> = None;
    let _: Option<crate::ValueSearchDriverParams> = None;
    let _: Option<crate::ValueSearchMode> = None;
    let _: Option<crate::ValueSearchStatus> = None;
    let _: Option<crate::ValueSearchProgress> = None;
    let _: Option<crate::ValueSearchTableResult> = None;
    let _: Option<crate::ValueSearchTarget> = None;
    let _: Option<crate::ValueSearchLocateIntent> = None;
    let _: Option<crate::SearchTableValuesRequest> = None;
    let _: Option<crate::SearchTableValuesResponse> = None;
    let _: Option<&crate::ExtraTypesProvider> = None;
}
