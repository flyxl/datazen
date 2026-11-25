//! Pure ports of `lib/fkNavigation.ts` — plain functions over plain data.

use super::model::{
    json_quote, FkFilterVm, FkGroupAffordanceVm, FkIndexVm, FkNavTargetIdentityVm, FkValue,
    ForeignKeyInfoVm,
};

/// React `FK_NAV_MAX_DEPTH`.
#[allow(dead_code)] // contract constant; consumed by history and tests.
pub const FK_NAV_MAX_DEPTH: usize = 32;

/// React `targetLabelOf`: a single column uses `table.column`, a composite key
/// uses `table.(a, b)`.
#[allow(dead_code)] // pure helper; consumed by indexing and tests.
pub fn target_label_of(reference_table: &str, target_columns: &[String]) -> String {
    if target_columns.len() == 1 {
        format!("{reference_table}.{}", target_columns[0])
    } else {
        format!("{reference_table}.({})", target_columns.join(", "))
    }
}

/// React `indexForeignKeys`: every member column maps to the same group; the
/// first constraint covering a column wins.
#[allow(dead_code)] // pure rule; consumed by the navigation decision and tests.
pub fn index_foreign_keys(foreign_keys: &[ForeignKeyInfoVm]) -> FkIndexVm {
    let mut index = FkIndexVm::new();
    for foreign_key in foreign_keys {
        let group = FkGroupAffordanceVm {
            local_columns: foreign_key.columns.clone(),
            target_columns: foreign_key.referenced_columns.clone(),
            target_label: target_label_of(
                &foreign_key.referenced_table,
                &foreign_key.referenced_columns,
            ),
            constraint_name: foreign_key.name.clone(),
            reference_table: foreign_key.referenced_table.clone(),
        };
        for column in &foreign_key.columns {
            if index.contains_key(column) {
                continue;
            }
            index.insert(column.clone(), group.clone());
        }
    }
    index
}

/// React `isFkCellNavigable` input.
#[allow(dead_code)] // VM surface; supplied by the grid host.
#[derive(Clone, Debug, PartialEq)]
pub struct NavigableInput<'a> {
    pub column_name: &'a str,
    pub row: Option<&'a std::collections::BTreeMap<String, FkValue>>,
    pub index: &'a FkIndexVm,
    pub visible_columns: &'a [String],
    pub editing: bool,
}

/// React `isFkCellNavigable`: not editing, group present, every member valued,
/// and the cell is the first visible member (one arrow per composite key).
#[allow(dead_code)] // pure rule; consumed by the grid and tests.
pub fn is_fk_cell_navigable(input: &NavigableInput<'_>) -> bool {
    if input.editing {
        return false;
    }
    let Some(group) = input.index.get(input.column_name) else {
        return false;
    };
    let Some(row) = input.row else {
        return false;
    };
    for column in &group.local_columns {
        if row.get(column).map(FkValue::is_absent).unwrap_or(true) {
            return false;
        }
    }
    let first_visible = input
        .visible_columns
        .iter()
        .find(|column| group.local_columns.contains(column));
    first_visible.is_some_and(|column| column == input.column_name)
}

/// React `buildFkFilters`: positional `eq` conditions, `None` when lengths differ
/// or a value is absent — a NULL never degrades into `IS NULL`.
#[allow(dead_code)] // pure rule; consumed by the jump and tests.
pub fn build_fk_filters(
    columns: &[String],
    values: &[FkValue],
) -> Option<Vec<FkFilterVm>> {
    if columns.is_empty() || columns.len() != values.len() {
        return None;
    }
    let mut filters = Vec::with_capacity(columns.len());
    for (column, value) in columns.iter().zip(values) {
        if value.is_absent() {
            return None;
        }
        filters.push(FkFilterVm {
            column: column.clone(),
            operator: "eq",
            value: value.clone(),
        });
    }
    Some(filters)
}

/// React `fkNavKey`: `JSON.stringify([database, schema, table, columns, values])`.
/// A missing schema (`null`) is distinct from an empty string.
#[allow(dead_code)] // pure rule; consumed by the jump and tests.
pub fn fk_nav_key(target: &FkNavTargetIdentityVm, values: &[FkValue]) -> String {
    let schema = target
        .schema
        .as_ref()
        .map(|value| json_quote(value))
        .unwrap_or_else(|| "null".to_owned());
    let columns = target
        .columns
        .iter()
        .map(|column| json_quote(column))
        .collect::<Vec<_>>()
        .join(",");
    let values = values
        .iter()
        .map(FkValue::json)
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "[{},{},{},[{}],[{}]]",
        json_quote(&target.database),
        schema,
        json_quote(&target.table),
        columns,
        values
    )
}

/// React `findForeignKey`: a matching constraint name wins; otherwise the
/// ordered local columns must match exactly.
#[allow(dead_code)] // pure rule; consumed by the request path and tests.
pub fn find_foreign_key(
    foreign_keys: &[ForeignKeyInfoVm],
    columns: &[String],
    constraint_name: Option<&str>,
) -> Option<ForeignKeyInfoVm> {
    if let Some(name) = constraint_name.filter(|name| !name.is_empty()) {
        if let Some(named) = foreign_keys.iter().find(|foreign_key| foreign_key.name == name) {
            return Some(named.clone());
        }
    }
    foreign_keys
        .iter()
        .find(|foreign_key| foreign_key.columns == columns)
        .cloned()
}

/// React `pickAnchorRow`: the active row only while it is on screen; otherwise
/// the first visible row.
#[allow(dead_code)] // pure rule; consumed by viewport snapshotting and tests.
pub fn pick_anchor_row(
    scroll_top: f64,
    row_height: f64,
    viewport_height: f64,
    active_row: Option<usize>,
) -> (usize, bool) {
    let row_height = row_height.max(1.0);
    let first_visible = (scroll_top / row_height).floor().max(0.0) as usize;
    let visible_rows = (viewport_height / row_height).floor().max(1.0) as usize;
    let last_visible = first_visible + visible_rows - 1;
    match active_row {
        Some(active) if active >= first_visible && active <= last_visible => (active, true),
        _ => (first_visible, false),
    }
}

/// React `scrollTopForAnchorRow`: clamps non-finite inputs and negatives to 0.
#[allow(dead_code)] // pure rule; consumed by viewport restore and tests.
pub fn scroll_top_for_anchor_row(anchor_row: f64, row_height: f64) -> f64 {
    if !anchor_row.is_finite() || !row_height.is_finite() {
        return 0.0;
    }
    (anchor_row * row_height).max(0.0)
}

/// React `findEntryIdByNavKey` search order: newest first.
#[allow(dead_code)] // pure helper; consumed by the history machine and tests.
pub fn find_entry_id_by_nav_key(
    back: &[super::model::FkNavEntryVm],
    nav_key: &str,
) -> Option<String> {
    back.iter()
        .rev()
        .find(|entry| entry.nav_key == nav_key)
        .map(|entry| entry.id.clone())
}
