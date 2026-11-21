use serde_json::Value;

use super::types::{NamedView, NamedViewExportFile, NAMED_VIEW_EXPORT_VERSION};

pub fn build_named_view_export(views: &[NamedView], exported_at: &str) -> NamedViewExportFile {
    NamedViewExportFile {
        version: NAMED_VIEW_EXPORT_VERSION,
        exported_at: exported_at.to_owned(),
        views: views.to_vec(),
    }
}

fn is_record(value: &Value) -> bool {
    matches!(value, Value::Object(_))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_owned)
}

fn string_array_field(value: &Value, key: &str) -> Option<Vec<String>> {
    match value.get(key) {
        Some(Value::Array(items)) if items.iter().all(Value::is_string) => Some(
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect(),
        ),
        _ => None,
    }
}

fn read_exported_view(entry: &Value, now: &str) -> Option<NamedView> {
    if !is_record(entry) {
        return None;
    }
    let id = string_field(entry, "id")?;
    let name = string_field(entry, "name")?;
    let scope = entry.get("scope")?;
    if !is_record(scope) {
        return None;
    }
    let connection_id = string_field(scope, "connectionId")?;
    let table = string_field(scope, "table")?;
    let filters: Vec<crate::FilterCondition> = match entry.get("filters") {
        Some(Value::Array(items)) => items
            .iter()
            .cloned()
            .map(serde_json::from_value::<crate::FilterCondition>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?,
        _ => return None,
    };
    let filter_logic = string_field(entry, "filterLogic")?;
    if filter_logic != "and" && filter_logic != "or" {
        return None;
    }
    let sorts: Vec<crate::SortCondition> = match entry.get("sorts") {
        Some(Value::Array(items)) => items
            .iter()
            .cloned()
            .map(serde_json::from_value::<crate::SortCondition>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?,
        _ => return None,
    };
    Some(NamedView {
        id,
        name,
        scope: super::types::NamedViewScope {
            connection_id,
            database: string_field(scope, "database"),
            schema: string_field(scope, "schema"),
            table,
        },
        filters,
        filter_logic,
        raw_where: string_field(entry, "rawWhere"),
        sorts,
        visible_columns: string_array_field(entry, "visibleColumns"),
        column_order: string_array_field(entry, "columnOrder"),
        is_default: entry.get("isDefault") == Some(&Value::Bool(true)),
        created_at: string_field(entry, "createdAt").unwrap_or_else(|| now.to_owned()),
        updated_at: string_field(entry, "updatedAt"),
    })
}

pub fn parse_named_view_export(text: &str, now: &str) -> Result<NamedViewExportFile, String> {
    let payload: Value =
        serde_json::from_str(text).map_err(|_| "The file is not valid JSON.".to_owned())?;
    if !is_record(&payload) {
        return Err("The file does not hold a saved-view export.".to_owned());
    }
    let version = payload.get("version").and_then(Value::as_u64);
    if version != Some(NAMED_VIEW_EXPORT_VERSION as u64) {
        return Err(format!(
            "The file has version {}, expected {}.",
            version.map_or("undefined".to_owned(), |value| value.to_string()),
            NAMED_VIEW_EXPORT_VERSION
        ));
    }
    let entries = match payload.get("views") {
        Some(Value::Array(items)) => items,
        _ => return Err("The file does not hold a list of views.".to_owned()),
    };
    let mut views = Vec::with_capacity(entries.len());
    for entry in entries {
        match read_exported_view(entry, now) {
            Some(view) => views.push(view),
            None => return Err("The file holds a view that is not a saved view.".to_owned()),
        }
    }
    Ok(NamedViewExportFile {
        version: NAMED_VIEW_EXPORT_VERSION,
        exported_at: string_field(&payload, "exportedAt").unwrap_or_else(|| now.to_owned()),
        views,
    })
}
