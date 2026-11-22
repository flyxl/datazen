use std::collections::BTreeMap;

use serde_json::{json, Value};

#[cfg(test)]
mod tests;

pub const COLUMN_LAYOUT_STORAGE_PREFIX: &str = "datazen:table-columns:v1";
pub const COLUMN_LAYOUT_GAP: [&str; 2] = [
    "window.localStorage getItem/setItem/removeItem",
    "JSON.parse of an untrusted stored entry",
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LayoutTableContext {
    pub connection_id: Option<String>,
    pub db_session_id: String,
    pub driver_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PersistedColumnLayout {
    pub order: Option<Vec<String>>,
    pub widths: BTreeMap<String, f64>,
}

pub struct ColumnLayoutStorage {
    pub get: Box<dyn Fn(&str) -> Option<String>>,
    pub set: Box<dyn Fn(&str, &str)>,
    pub remove: Box<dyn Fn(&str)>,
}

pub fn column_layout_storage_key(context: &LayoutTableContext) -> Option<String> {
    let connection = context.connection_id.as_ref()?;
    let identity = json!([
        connection,
        context.database.as_deref().unwrap_or(""),
        context.schema.as_deref().unwrap_or(""),
        context.table,
    ]);
    Some(format!("{COLUMN_LAYOUT_STORAGE_PREFIX}:{}", identity.to_string()))
}

fn is_plain_object(value: &Value) -> bool {
    matches!(value, Value::Object(_))
}

pub fn valid_order(value: &Value) -> Option<Vec<String>> {
    let names: Vec<String> = match value {
        Value::Array(items) => {
            items.iter().filter_map(|item| item.as_str().map(str::to_owned)).collect()
        }
        _ => return None,
    };
    if names.is_empty() {
        None
    } else {
        Some(names)
    }
}

pub fn valid_widths(value: &Value) -> BTreeMap<String, f64> {
    let mut widths = BTreeMap::new();
    if let Value::Object(entries) = value {
        for (name, width) in entries {
            if let Some(number) = width.as_f64() {
                if number.is_finite() && number > 0.0 {
                    widths.insert(name.clone(), number);
                }
            }
        }
    }
    widths
}

fn read_stored(context: &LayoutTableContext, storage: Option<&ColumnLayoutStorage>) -> Value {
    let key = match column_layout_storage_key(context) {
        Some(key) => key,
        None => return Value::Null,
    };
    let storage = match storage {
        Some(storage) => storage,
        None => return Value::Null,
    };
    let raw = match (storage.get)(&key) {
        Some(raw) if !raw.is_empty() => raw,
        _ => return Value::Null,
    };
    match serde_json::from_str::<Value>(&raw) {
        Ok(value) if is_plain_object(&value) => value,
        _ => Value::Null,
    }
}

fn write_stored(
    context: &LayoutTableContext,
    order: Option<&[String]>,
    widths: &BTreeMap<String, f64>,
    storage: Option<&ColumnLayoutStorage>,
) {
    let key = match column_layout_storage_key(context) {
        Some(key) => key,
        None => return,
    };
    let storage = match storage {
        Some(storage) => storage,
        None => return,
    };
    let has_order = order.is_some_and(|names| !names.is_empty());
    let has_widths = !widths.is_empty();
    if !has_order && !has_widths {
        (storage.remove)(&key);
        return;
    }
    let mut record = serde_json::Map::new();
    if let Some(names) = order.filter(|names| !names.is_empty()) {
        record.insert("order".to_owned(), json!(names));
    }
    if has_widths {
        record.insert("widths".to_owned(), json!(widths));
    }
    (storage.set)(&key, &Value::Object(record).to_string());
}

pub fn read_persisted_column_layout(
    context: &LayoutTableContext,
    storage: Option<&ColumnLayoutStorage>,
) -> PersistedColumnLayout {
    let stored = read_stored(context, storage);
    PersistedColumnLayout {
        order: stored.get("order").and_then(valid_order),
        widths: stored.get("widths").map(valid_widths).unwrap_or_default(),
    }
}

pub fn persist_column_order(
    context: &LayoutTableContext,
    order: Option<Vec<String>>,
    storage: Option<&ColumnLayoutStorage>,
) {
    let stored = read_stored(context, storage);
    let widths = stored.get("widths").map(valid_widths).unwrap_or_default();
    write_stored(context, order.as_deref(), &widths, storage);
}

pub fn persist_column_widths(
    context: &LayoutTableContext,
    widths: &BTreeMap<String, f64>,
    storage: Option<&ColumnLayoutStorage>,
) {
    let stored = read_stored(context, storage);
    let order = stored.get("order").and_then(valid_order);
    write_stored(context, order.as_deref(), widths, storage);
}
