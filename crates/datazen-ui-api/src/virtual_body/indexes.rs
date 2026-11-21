use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::types::{
    CellTypeHintsData, ColumnDefData, ColumnEditorInfoData, FkGroupAffordanceData,
    VirtualBodyConfig,
};

fn translate(key: &str) -> String {
    format!("t:{key}")
}

pub fn format_dirty_value(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => translate("tableData.dirtyValueNull"),
        Some(Value::String(text)) if text.is_empty() => translate("tableData.dirtyValueEmpty"),
        Some(value @ (Value::Object(_) | Value::Array(_))) => value.to_string(),
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(flag)) => flag.to_string(),
    }
}

pub fn format_dirty_expression() -> String {
    translate("tableData.dirtyValueExpression")
}

pub fn dirty_cell_tooltip(
    original: Option<&Value>,
    next: Option<&Value>,
    expression: Option<&str>,
) -> String {
    let to = if expression == Some("currentTimestamp") {
        format_dirty_expression()
    } else {
        format_dirty_value(next)
    };
    format!("{} \u{2192} {to}", format_dirty_value(original))
}

pub fn dirty_cell_aria_label(
    original: Option<&Value>,
    next: Option<&Value>,
    expression: Option<&str>,
) -> String {
    let to = if expression == Some("currentTimestamp") {
        format_dirty_expression()
    } else {
        format_dirty_value(next)
    };
    format!(
        "t:tableData.dirtyCellAria:original={}:next={to}",
        format_dirty_value(original)
    )
}

fn normalize_type(data_type: Option<&str>) -> String {
    data_type.unwrap_or("").trim().to_ascii_lowercase()
}

pub fn resolve_cell_type(data_type: Option<&str>, hints: Option<&CellTypeHintsData>) -> String {
    let normalized = normalize_type(data_type);
    if normalized.is_empty() {
        return "unknown".to_owned();
    }
    if let Some(hints) = hints {
        if let Some(cell_type) = hints.type_overrides.get(&normalized) {
            return cell_type.clone();
        }
        if hints.enum_types.iter().any(|name| name == &normalized) {
            return "enum".to_owned();
        }
    }
    let numeric = [
        "int",
        "integer",
        "bigint",
        "smallint",
        "tinyint",
        "serial",
        "bigserial",
        "decimal",
        "numeric",
        "float",
        "double",
        "real",
        "money",
    ];
    if numeric.iter().any(|token| normalized.contains(token)) {
        return "number".to_owned();
    }
    if normalized.contains("bool") || normalized == "bit" {
        return "boolean".to_owned();
    }
    if normalized.contains("json") {
        return "json".to_owned();
    }
    if normalized.contains("timestamp") || normalized.contains("datetime") {
        return "datetime".to_owned();
    }
    if normalized.contains("date") || normalized.contains("time") {
        return "date".to_owned();
    }
    if normalized.contains("blob") || normalized.contains("binary") || normalized.contains("bytea") {
        return "binary".to_owned();
    }
    "text".to_owned()
}

pub fn is_integer_cell_type(data_type: Option<&str>, hints: Option<&CellTypeHintsData>) -> bool {
    let normalized = normalize_type(data_type);
    if normalized.is_empty() || resolve_cell_type(Some(&normalized), hints) != "number" {
        return false;
    }
    ["int", "serial"].iter().any(|token| normalized.contains(token))
}

pub fn build_column_editor_info_from_defs(
    columns: &[ColumnDefData],
    hints: Option<&CellTypeHintsData>,
) -> BTreeMap<String, ColumnEditorInfoData> {
    columns
        .iter()
        .map(|column| {
            let cell_type = resolve_cell_type(column.column_type.as_deref(), hints);
            (
                column.name.clone(),
                ColumnEditorInfoData {
                    integer_only: is_integer_cell_type(column.column_type.as_deref(), hints),
                    nullable: column.nullable.unwrap_or(true),
                    cell_type,
                    read_only_reason: None,
                },
            )
        })
        .collect()
}

pub fn editor_info(
    provided: Option<&BTreeMap<String, ColumnEditorInfoData>>,
    columns: &[ColumnDefData],
    hints: Option<&CellTypeHintsData>,
) -> BTreeMap<String, ColumnEditorInfoData> {
    match provided {
        Some(info) if !info.is_empty() => info.clone(),
        _ => build_column_editor_info_from_defs(columns, hints),
    }
}

pub fn is_fk_cell_navigable(
    column_name: &str,
    row: &BTreeMap<String, Option<Value>>,
    index: &BTreeMap<String, FkGroupAffordanceData>,
    visible_columns: &[String],
    editing: bool,
) -> bool {
    if editing {
        return false;
    }
    let group = match index.get(column_name) {
        Some(group) => group,
        None => return false,
    };
    for column in &group.local_columns {
        if !row.get(column).and_then(|value| value.as_ref()).is_some_and(|value| *value != Value::Null) {
            return false;
        }
    }
    let first_visible = visible_columns.iter().find(|column| group.local_columns.contains(column));
    first_visible.is_some_and(|column| column == column_name)
}

pub fn fk_navigable_by_row(
    config: &VirtualBodyConfig,
) -> BTreeMap<usize, BTreeSet<String>> {
    let mut result = BTreeMap::new();
    let foreign_keys = match config.foreign_keys.as_ref() {
        Some(foreign_keys) if !foreign_keys.index.is_empty() => foreign_keys,
        _ => return result,
    };
    let column_indexes: BTreeMap<&str, usize> = config
        .columns
        .iter()
        .enumerate()
        .map(|(index, column)| (column.name.as_str(), index))
        .collect();

    for (row_index, row) in config.rows.iter().enumerate() {
        let full = foreign_keys.rows.get(row_index);
        let mut record: BTreeMap<String, Option<Value>> = BTreeMap::new();
        for column_name in foreign_keys.index.keys() {
            let value = match full {
                Some(full) => full.get(column_name).cloned(),
                None => column_indexes
                    .get(column_name.as_str())
                    .and_then(|index| row.get(*index))
                    .cloned()
                    .unwrap_or(None),
            };
            record.insert(column_name.clone(), value);
        }
        let mut navigable = BTreeSet::new();
        for column_name in foreign_keys.index.keys() {
            if is_fk_cell_navigable(
                column_name,
                &record,
                &foreign_keys.index,
                &foreign_keys.visible_columns,
                false,
            ) {
                navigable.insert(column_name.clone());
            }
        }
        result.insert(row_index, navigable);
    }
    result
}

pub fn error_columns_by_row(errors: &BTreeSet<String>) -> BTreeMap<usize, BTreeSet<String>> {
    let mut result = BTreeMap::new();
    for key in errors {
        let separator = match key.find(':') {
            Some(index) if index > 0 => index,
            _ => continue,
        };
        let row_index = match key[..separator].parse::<usize>() {
            Ok(index) => index,
            Err(_) => continue,
        };
        let column_name = &key[separator + 1..];
        if column_name.is_empty() {
            continue;
        }
        result
            .entry(row_index)
            .or_insert_with(BTreeSet::new)
            .insert(column_name.to_owned());
    }
    result
}
