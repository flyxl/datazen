use std::collections::BTreeSet;

use crate::{
    resolve_cell_type, CellType, TableInfo, TableType, ValueSearchTypeGroup, ValueSearchTypeScope,
};
use crate::types::ColumnSchema;

use super::types::{ValueSearchTarget, ValueSearchTableResult};

pub type ExtraTypesProvider = dyn Fn(&str) -> Vec<ValueSearchTypeGroup>;

pub fn value_search_type_groups_for(
    driver_type: Option<&str>,
    provider: &ExtraTypesProvider,
) -> Vec<ValueSearchTypeGroup> {
    match driver_type {
        Some(driver) => provider(driver),
        None => Vec::new(),
    }
}

pub fn type_scopes_for_driver(
    driver_type: Option<&str>,
    provider: &ExtraTypesProvider,
) -> Vec<ValueSearchTypeScope> {
    let groups = value_search_type_groups_for(driver_type, provider);
    let number = groups.contains(&ValueSearchTypeGroup::Number);
    let datetime = groups.contains(&ValueSearchTypeGroup::Datetime);
    let mut scopes = vec![ValueSearchTypeScope::Text];
    if number {
        scopes.push(ValueSearchTypeScope::TextNumber);
    }
    if datetime {
        scopes.push(ValueSearchTypeScope::TextDatetime);
    }
    if number && datetime {
        scopes.push(ValueSearchTypeScope::TextNumberDatetime);
    }
    scopes
}

pub fn base_cell_types() -> BTreeSet<CellType> {
    [CellType::Text, CellType::Enum, CellType::Unknown].into_iter().collect()
}

pub fn cell_types_for(scope: ValueSearchTypeScope) -> BTreeSet<CellType> {
    let mut types = base_cell_types();
    if matches!(
        scope,
        ValueSearchTypeScope::TextNumber | ValueSearchTypeScope::TextNumberDatetime
    ) {
        types.insert(CellType::Number);
    }
    if matches!(
        scope,
        ValueSearchTypeScope::TextDatetime | ValueSearchTypeScope::TextNumberDatetime
    ) {
        types.extend([CellType::Date, CellType::Time, CellType::DateTime]);
    }
    types
}

pub fn searchable_columns(columns: &[ColumnSchema], cell_types: &BTreeSet<CellType>) -> Vec<String> {
    columns
        .iter()
        .filter(|column| cell_types.contains(&resolve_cell_type(Some(&column.data_type), None)))
        .map(|column| column.name.clone())
        .collect()
}

pub fn target_key(target: &ValueSearchTarget) -> String {
    match target.schema.as_deref() {
        Some(schema) if !schema.is_empty() => format!("{schema}.{}", target.table),
        _ => target.table.clone(),
    }
}

pub fn value_search_targets(tables: &[TableInfo]) -> Vec<ValueSearchTarget> {
    tables
        .iter()
        .filter(|table| table.table_type == TableType::Table)
        .map(|table| ValueSearchTarget {
            table: table.name.clone(),
            schema: table.schema.clone(),
        })
        .collect()
}

pub fn rank_tables(results: &[ValueSearchTableResult], limit: usize) -> Vec<ValueSearchTableResult> {
    let mut ranked: Vec<ValueSearchTableResult> = results
        .iter()
        .filter(|result| result.matches.unwrap_or(0) > 0)
        .cloned()
        .collect();
    ranked.sort_by(|left, right| right.matches.unwrap_or(0).cmp(&left.matches.unwrap_or(0)));
    ranked.truncate(limit);
    ranked
}
