use super::types::{DataSyncRowChange, DataSyncTableResult, PAGE_SIZE};

pub fn server_paged(table: &DataSyncTableResult) -> bool {
    table.page_size.is_some() || table.row_count.is_some()
}

pub fn diff_rows(table: &DataSyncTableResult) -> Vec<DataSyncRowChange> {
    table
        .rows
        .as_ref()
        .map(|rows| {
            rows.iter()
                .filter(|row| row.operation != super::types::DataSyncOperation::Unchanged)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

pub fn page_count(table: &DataSyncTableResult, rows: &[DataSyncRowChange], page_size: usize) -> usize {
    if server_paged(table) {
        let count = table.row_count.unwrap_or(rows.len());
        let size = table.page_size.unwrap_or(PAGE_SIZE).max(1);
        return ((count + size - 1) / size).max(1);
    }
    let size = page_size.max(1);
    ((rows.len() + size - 1) / size).max(1)
}

pub fn safe_page(
    table: &DataSyncTableResult,
    page: usize,
    page_index: usize,
    page_count: usize,
) -> usize {
    if server_paged(table) {
        page_index
    } else {
        page.min(page_count.saturating_sub(1))
    }
}

pub fn page_rows(
    table: &DataSyncTableResult,
    rows: &[DataSyncRowChange],
    safe_page: usize,
    page_size: usize,
) -> Vec<DataSyncRowChange> {
    if server_paged(table) {
        return rows.to_vec();
    }
    let size = page_size.max(1);
    let start = safe_page.saturating_mul(size);
    let end = start.saturating_add(size).min(rows.len());
    if start >= rows.len() {
        return Vec::new();
    }
    rows[start..end].to_vec()
}

pub fn max_cols(rows: &[DataSyncRowChange]) -> usize {
    rows.iter().fold(0usize, |max, row| {
        let source = row.source_row.as_ref().map_or(0, Vec::len);
        let target = row.target_row.as_ref().map_or(0, Vec::len);
        max.max(source).max(target)
    })
}

pub fn is_empty_state(rows: &[DataSyncRowChange], table: &DataSyncTableResult, has_next_page: bool) -> bool {
    rows.is_empty() && (!server_paged(table) || !has_next_page)
}

pub fn footer_visible(page_count: usize, table: &DataSyncTableResult) -> bool {
    page_count > 1 || server_paged(table)
}

pub fn prev_disabled(table: &DataSyncTableResult, page_loading: bool, safe_page: usize) -> bool {
    if server_paged(table) {
        page_loading
    } else {
        safe_page <= 0
    }
}

pub fn next_disabled(
    table: &DataSyncTableResult,
    page_loading: bool,
    has_next_page: bool,
    safe_page: usize,
    page_count: usize,
) -> bool {
    if server_paged(table) {
        page_loading || !has_next_page
    } else {
        safe_page >= page_count.saturating_sub(1)
    }
}
