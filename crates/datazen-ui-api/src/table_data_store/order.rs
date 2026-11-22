use crate::{types::ColumnSchema, DefaultOrderOverride, DefaultTableSort, SortCondition};

use super::types::{OrderRequest, TableTargetData};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableContextData {
    pub db_session_id: String,
    pub table: String,
    pub connection_id: Option<String>,
    pub driver_type: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
}

pub fn target_of(context: &TableContextData) -> TableTargetData {
    TableTargetData {
        db_session_id: context.db_session_id.clone(),
        table: context.table.clone(),
        connection_id: context.connection_id.clone(),
        driver_type: context.driver_type.clone(),
        database: context.database.clone(),
        schema: context.schema.clone(),
    }
}

pub fn resolve_default_table_sort(
    override_value: DefaultOrderOverride,
    setting: DefaultTableSort,
) -> DefaultTableSort {
    match override_value {
        DefaultOrderOverride::Inherit => setting,
        DefaultOrderOverride::PrimaryKeyAsc => DefaultTableSort::PrimaryKeyAsc,
        DefaultOrderOverride::PrimaryKeyDesc => DefaultTableSort::PrimaryKeyDesc,
        DefaultOrderOverride::None => DefaultTableSort::None,
    }
}

pub fn order_request(
    override_value: DefaultOrderOverride,
    columns: &[ColumnSchema],
    sorts: &[SortCondition],
    setting: DefaultTableSort,
) -> OrderRequest {
    let order = resolve_default_table_sort(override_value, setting);
    if order == DefaultTableSort::None {
        return OrderRequest { default_order_columns: Some(Vec::new()), sorts: sorts.to_vec() };
    }
    if order == DefaultTableSort::PrimaryKeyDesc && sorts.is_empty() {
        let key_columns: Vec<&ColumnSchema> = columns
            .iter()
            .filter(|column| column.is_primary_key == Some(true))
            .collect();
        if key_columns.len() == 1 {
            return OrderRequest {
                sorts: vec![SortCondition {
                    column: key_columns[0].name.clone(),
                    descending: true,
                }],
                default_order_columns: None,
            };
        }
    }
    OrderRequest { sorts: sorts.to_vec(), default_order_columns: None }
}

pub fn upsert_sort(sorts: &[SortCondition], sort: SortCondition) -> Vec<SortCondition> {
    if let Some(index) = sorts.iter().position(|entry| entry.column == sort.column) {
        let mut next = sorts.to_vec();
        next[index] = sort;
        return next;
    }
    let mut next = sorts.to_vec();
    next.push(sort);
    next
}
