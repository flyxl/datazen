use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;

use serde_json::{json, Value};

use super::*;

fn context() -> LayoutTableContext {
    LayoutTableContext {
        connection_id: Some("conn".to_owned()),
        db_session_id: "s1".to_owned(),
        driver_type: Some("postgres".to_owned()),
        database: Some("db".to_owned()),
        schema: Some("public".to_owned()),
        table: "users".to_owned(),
    }
}

fn widths(pairs: &[(&str, f64)]) -> BTreeMap<String, f64> {
    pairs.iter().map(|(name, width)| ((*name).to_owned(), *width)).collect()
}

fn memory_storage() -> (ColumnLayoutStorage, Rc<RefCell<HashMap<String, String>>>) {
    let data = Rc::new(RefCell::new(HashMap::new()));
    let get_data = data.clone();
    let set_data = data.clone();
    let remove_data = data.clone();
    let storage = ColumnLayoutStorage {
        get: Box::new(move |key| get_data.borrow().get(key).cloned()),
        set: Box::new(move |key, raw| {
            set_data.borrow_mut().insert(key.to_owned(), raw.to_owned());
        }),
        remove: Box::new(move |key| {
            remove_data.borrow_mut().remove(key);
        }),
    };
    (storage, data)
}

#[test]
fn storage_key_needs_a_connection_and_normalizes_absent_database_and_schema() {
    let key = column_layout_storage_key(&context()).expect("a stable key");
    assert_eq!(
        key,
        r#"datazen:table-columns:v1:["conn","db","public","users"]"#
    );
    let mut partial = context();
    partial.database = None;
    partial.schema = None;
    assert_eq!(
        column_layout_storage_key(&partial).as_deref(),
        Some(r#"datazen:table-columns:v1:["conn","","","users"]"#)
    );
    let mut no_connection = context();
    no_connection.connection_id = None;
    assert_eq!(column_layout_storage_key(&no_connection), None);
    assert_eq!(COLUMN_LAYOUT_STORAGE_PREFIX, "datazen:table-columns:v1");
    assert_eq!(COLUMN_LAYOUT_GAP.len(), 2);
}

#[test]
fn untrusted_order_and_widths_are_filtered() {
    assert_eq!(valid_order(&json!(["a", "b"])), Some(vec!["a".to_owned(), "b".to_owned()]));
    assert_eq!(valid_order(&json!(["a", 1, "b"])), Some(vec!["a".to_owned(), "b".to_owned()]));
    assert_eq!(valid_order(&json!([1, 2])), None);
    assert_eq!(valid_order(&json!([])), None);
    assert_eq!(valid_order(&json!("a")), None);

    let raw = json!({"a": 120, "b": 0, "c": -5, "d": "200", "e": 80.5});
    assert_eq!(valid_widths(&raw), widths(&[("a", 120.0), ("e", 80.5)]));
    assert!(valid_widths(&json!([])).is_empty());
}

#[test]
fn read_handles_missing_corrupt_and_valid_entries() {
    let (storage, data) = memory_storage();
    assert_eq!(read_persisted_column_layout(&context(), None), PersistedColumnLayout::default());
    assert_eq!(read_persisted_column_layout(&context(), Some(&storage)), PersistedColumnLayout::default());

    let key = column_layout_storage_key(&context()).expect("key");
    data.borrow_mut().insert(key.clone(), "not json".to_owned());
    assert_eq!(read_persisted_column_layout(&context(), Some(&storage)), PersistedColumnLayout::default());
    data.borrow_mut().insert(key.clone(), json!([1, 2]).to_string());
    assert_eq!(read_persisted_column_layout(&context(), Some(&storage)), PersistedColumnLayout::default());

    data.borrow_mut().insert(
        key,
        json!({"order": ["name", "id", 7], "widths": {"name": 90, "id": -1}}).to_string(),
    );
    let layout = read_persisted_column_layout(&context(), Some(&storage));
    assert_eq!(layout.order, Some(vec!["name".to_owned(), "id".to_owned()]));
    assert_eq!(layout.widths, widths(&[("name", 90.0)]));
}

#[test]
fn persisting_order_keeps_widths_and_an_empty_layout_removes_the_entry() {
    let (storage, data) = memory_storage();
    let key = column_layout_storage_key(&context()).expect("key");
    data.borrow_mut().insert(
        key.clone(),
        json!({"order": ["old"], "widths": {"name": 90}}).to_string(),
    );
    persist_column_order(
        &context(),
        Some(vec!["name".to_owned(), "id".to_owned()]),
        Some(&storage),
    );
    let raw: Value = serde_json::from_str(&data.borrow().get(&key).cloned().expect("stored")).expect("json");
    assert_eq!(raw.get("order"), Some(&json!(["name", "id"])));
    assert_eq!(raw.get("widths"), Some(&json!({"name": 90.0})));

    persist_column_order(&context(), None, Some(&storage));
    let raw: Value = serde_json::from_str(&data.borrow().get(&key).cloned().expect("stored")).expect("json");
    assert_eq!(raw.get("order"), None, "an empty order is omitted");
    assert_eq!(raw.get("widths"), Some(&json!({"name": 90.0})), "stored widths survive");
}

#[test]
fn persisting_widths_keeps_the_order_and_writes_the_given_widths() {
    let (storage, data) = memory_storage();
    let key = column_layout_storage_key(&context()).expect("key");
    data.borrow_mut().insert(key.clone(), json!({"order": ["name"]}).to_string());
    persist_column_widths(
        &context(),
        &widths(&[("id", 40.0), ("name", 120.0)]),
        Some(&storage),
    );
    let raw: Value = serde_json::from_str(&data.borrow().get(&key).cloned().expect("stored")).expect("json");
    assert_eq!(raw.get("order"), Some(&json!(["name"])));
    assert_eq!(raw.get("widths"), Some(&json!({"id": 40.0, "name": 120.0})));

    persist_column_widths(&context(), &BTreeMap::new(), Some(&storage));
    let raw: Value = serde_json::from_str(&data.borrow().get(&key).cloned().expect("stored")).expect("json");
    assert_eq!(raw.get("order"), Some(&json!(["name"])), "stored order survives");
    assert_eq!(raw.get("widths"), None, "empty widths are omitted");

    let (fresh, fresh_data) = memory_storage();
    let fresh_key = column_layout_storage_key(&context()).expect("key");
    fresh_data.borrow_mut().insert(fresh_key.clone(), "corrupt".to_owned());
    persist_column_order(&context(), None, Some(&fresh));
    assert!(fresh_data.borrow().get(&fresh_key).is_none(), "a corrupt entry is removed when no layout remains");
}

#[test]
fn absent_or_failing_storage_is_a_noop() {
    let (storage, _) = memory_storage();
    let mut no_connection = context();
    no_connection.connection_id = None;
    persist_column_order(&no_connection, Some(vec!["a".to_owned()]), Some(&storage));
    persist_column_widths(&no_connection, &widths(&[("a", 10.0)]), Some(&storage));
    assert_eq!(read_persisted_column_layout(&no_connection, Some(&storage)), PersistedColumnLayout::default());

    persist_column_order(&context(), Some(vec!["a".to_owned()]), None);
    persist_column_widths(&context(), &widths(&[("a", 10.0)]), None);
    assert_eq!(read_persisted_column_layout(&context(), None), PersistedColumnLayout::default());
}

#[test]
fn crate_root_surface_is_reexported() {
    let _ = crate::column_layout_storage_key;
    let _ = crate::valid_order;
    let _ = crate::valid_widths;
    let _ = crate::read_persisted_column_layout;
    let _ = crate::persist_column_order;
    let _ = crate::persist_column_widths;
    assert_eq!(crate::COLUMN_LAYOUT_STORAGE_PREFIX, "datazen:table-columns:v1");
    assert_eq!(crate::COLUMN_LAYOUT_GAP.len(), 2);
    let _: Option<crate::LayoutTableContext> = None;
    let _: Option<crate::PersistedColumnLayout> = None;
    let _: Option<crate::ColumnLayoutStorage> = None;
}
