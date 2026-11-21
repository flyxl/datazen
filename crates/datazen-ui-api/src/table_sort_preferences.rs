use std::cell::RefCell;

use serde_json::{json, Value};

use crate::{SessionLocalStorage, SortCondition};

pub const TABLE_SORT_STORAGE_PREFIX: &str = "datazen:table-sort:v1";

pub const TABLE_SORT_PREFS_SURFACE: [&str; 5] = [
    "tableSortStorageKey",
    "readPersistedTableSorts",
    "persistTableSorts",
    "SortCondition",
    "TABLE_SORT_PREFS_GAP",
];

pub const TABLE_SORT_PREFS_GAP: [&str; 4] = [
    "window.localStorage availability",
    "JSON.parse / JSON.stringify via serde_json",
    "storage quota and disabled-storage errors",
    "the table store that reads saved sorts on mount",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TableSortContext {
    pub connection_id: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

pub fn table_sort_storage_key(context: &TableSortContext) -> Option<String> {
    let connection_id = context.connection_id.as_deref()?;
    if connection_id.is_empty() {
        return None;
    }
    let identity = json!([
        connection_id,
        context.database.as_deref().unwrap_or(""),
        context.schema.as_deref().unwrap_or(""),
        context.table,
    ]);
    Some(format!("{TABLE_SORT_STORAGE_PREFIX}:{identity}"))
}

pub fn is_sort_condition(value: &Value) -> bool {
    match value.as_object() {
        Some(object) => {
            object.get("column").is_some_and(Value::is_string)
                && object.get("descending").is_some_and(Value::is_boolean)
        }
        None => false,
    }
}

pub fn read_persisted_table_sorts<S>(
    context: &TableSortContext,
    storage: Option<&S>,
) -> Vec<SortCondition>
where
    S: SessionLocalStorage,
{
    let key = match table_sort_storage_key(context) {
        Some(key) => key,
        None => return Vec::new(),
    };
    let storage = match storage {
        Some(storage) => storage,
        None => return Vec::new(),
    };
    let raw = match storage.get(&key) {
        Ok(Some(raw)) if !raw.is_empty() => raw,
        _ => return Vec::new(),
    };
    let parsed: Value = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let items = match parsed.as_array() {
        Some(items) => items,
        None => return Vec::new(),
    };
    items
        .iter()
        .filter(|item| is_sort_condition(item))
        .filter_map(|item| serde_json::from_value(item.clone()).ok())
        .collect()
}

pub fn persist_table_sorts<S>(
    context: &TableSortContext,
    sorts: &[SortCondition],
    storage: Option<&S>,
) where
    S: SessionLocalStorage,
{
    let key = match table_sort_storage_key(context) {
        Some(key) => key,
        None => return,
    };
    let storage = match storage {
        Some(storage) => storage,
        None => return,
    };
    if sorts.is_empty() {
        let _ = storage.remove(&key);
    } else if let Ok(encoded) = serde_json::to_string(sorts) {
        let _ = storage.set(&key, &encoded);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemoryStorage {
        values: RefCell<HashMap<String, String>>,
        calls: RefCell<Vec<String>>,
        fail_get: RefCell<bool>,
        fail_set: RefCell<bool>,
        fail_remove: RefCell<bool>,
    }

    impl SessionLocalStorage for MemoryStorage {
        fn get(&self, key: &str) -> Result<Option<String>, String> {
            self.calls.borrow_mut().push(format!("get:{key}"));
            if *self.fail_get.borrow() {
                return Err("get failed".to_owned());
            }
            Ok(self.values.borrow().get(key).cloned())
        }

        fn set(&self, key: &str, value: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("set:{key}:{value}"));
            if *self.fail_set.borrow() {
                return Err("set failed".to_owned());
            }
            self.values
                .borrow_mut()
                .insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        fn remove(&self, key: &str) -> Result<(), String> {
            self.calls.borrow_mut().push(format!("remove:{key}"));
            if *self.fail_remove.borrow() {
                return Err("remove failed".to_owned());
            }
            self.values.borrow_mut().remove(key);
            Ok(())
        }
    }

    fn context() -> TableSortContext {
        TableSortContext {
            connection_id: Some("c1".to_owned()),
            database: Some("shop".to_owned()),
            schema: None,
            table: "users".to_owned(),
        }
    }

    fn sort(column: &str, descending: bool) -> SortCondition {
        SortCondition {
            column: column.to_owned(),
            descending,
        }
    }

    #[test]
    fn storage_key_needs_a_connection_id_and_embeds_the_identity() {
        let key = table_sort_storage_key(&context()).unwrap();
        assert_eq!(
            key,
            "datazen:table-sort:v1:[\"c1\",\"shop\",\"\",\"users\"]".to_owned()
        );
        let mut missing = context();
        missing.connection_id = None;
        assert_eq!(table_sort_storage_key(&missing), None);
        missing.connection_id = Some(String::new());
        assert_eq!(table_sort_storage_key(&missing), None);
    }

    #[test]
    fn only_well_shaped_sort_conditions_are_accepted() {
        assert!(is_sort_condition(&json!({"column":"a","descending":true})));
        assert!(!is_sort_condition(&json!({"column":"a"})));
        assert!(!is_sort_condition(
            &json!({"column":"a","descending":"yes"})
        ));
        assert!(!is_sort_condition(&json!(null)));
        assert!(!is_sort_condition(&json!([])));
    }

    #[test]
    fn reading_without_a_usable_store_returns_empty() {
        assert!(read_persisted_table_sorts(&context(), None::<&MemoryStorage>).is_empty());
        let mut missing = context();
        missing.connection_id = None;
        let storage = MemoryStorage::default();
        assert!(read_persisted_table_sorts(&missing, Some(&storage)).is_empty());
        let failing = MemoryStorage {
            fail_get: RefCell::new(true),
            ..Default::default()
        };
        assert!(read_persisted_table_sorts(&context(), Some(&failing)).is_empty());
    }

    #[test]
    fn malformed_or_empty_payloads_read_as_no_sorts() {
        let storage = MemoryStorage::default();
        let key = table_sort_storage_key(&context()).unwrap();
        for raw in ["", "not json", "[]", "{}"] {
            storage
                .values
                .borrow_mut()
                .insert(key.clone(), raw.to_owned());
            assert!(
                read_persisted_table_sorts(&context(), Some(&storage)).is_empty(),
                "{raw}"
            );
        }
        storage
            .values
            .borrow_mut()
            .insert(key.clone(), "[1,2]".to_owned());
        assert!(read_persisted_table_sorts(&context(), Some(&storage)).is_empty());
    }

    #[test]
    fn reading_filters_bad_entries_and_keeps_valid_ones() {
        let storage = MemoryStorage::default();
        let key = table_sort_storage_key(&context()).unwrap();
        storage.values.borrow_mut().insert(
            key,
            r#"[{"column":"a","descending":true},{"column":1,"descending":false},{"column":"b","descending":false}]"#.to_owned(),
        );
        let sorts = read_persisted_table_sorts(&context(), Some(&storage));
        assert_eq!(sorts, [sort("a", true), sort("b", false)]);
    }

    #[test]
    fn persisting_writes_json_or_clears_the_key() {
        let storage = MemoryStorage::default();
        persist_table_sorts(&context(), &[sort("a", true)], Some(&storage));
        let key = table_sort_storage_key(&context()).unwrap();
        assert_eq!(
            storage.values.borrow().get(&key).map(String::as_str),
            Some(r#"[{"column":"a","descending":true}]"#)
        );
        persist_table_sorts(&context(), &[], Some(&storage));
        assert!(!storage.values.borrow().contains_key(&key));
        assert!(storage
            .calls
            .borrow()
            .iter()
            .any(|call| call.starts_with("set:")));
        assert!(storage
            .calls
            .borrow()
            .iter()
            .any(|call| call.starts_with("remove:")));
    }

    #[test]
    fn storage_failures_are_swallowed() {
        let storage = MemoryStorage {
            fail_set: RefCell::new(true),
            fail_remove: RefCell::new(true),
            ..Default::default()
        };
        persist_table_sorts(&context(), &[sort("a", false)], Some(&storage));
        persist_table_sorts(&context(), &[], Some(&storage));
        let mut missing = context();
        missing.connection_id = None;
        persist_table_sorts(&missing, &[], Some(&storage));
        assert!(storage.values.borrow().is_empty());
        assert_eq!(TABLE_SORT_PREFS_SURFACE.len(), 5);
        assert_eq!(TABLE_SORT_PREFS_GAP.len(), 4);
        assert_eq!(crate::TABLE_SORT_PREFS_GAP.len(), 4);
    }
}
