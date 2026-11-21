use super::types::{VirtualRowData, OVERSCAN};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DisplayRow {
    Real(usize),
    Draft(usize),
}

pub fn display_rows(real_count: usize, draft_count: usize) -> Vec<DisplayRow> {
    let mut rows = Vec::with_capacity(real_count + draft_count);
    rows.extend((0..real_count).map(DisplayRow::Real));
    rows.extend((0..draft_count).map(DisplayRow::Draft));
    rows
}

pub fn row_kind(index: usize, real_count: usize) -> DisplayRow {
    if index < real_count {
        DisplayRow::Real(index)
    } else {
        DisplayRow::Draft(index - real_count)
    }
}

pub struct VirtualTableResult {
    pub virtual_rows: Vec<VirtualRowData>,
    pub total_height: i64,
}

pub fn virtual_table(
    count: usize,
    row_height: i64,
    scroll_top: i64,
    viewport_height: i64,
) -> VirtualTableResult {
    let total_height = (count as i64).saturating_mul(row_height.max(0));
    if count == 0 || row_height <= 0 {
        return VirtualTableResult { virtual_rows: Vec::new(), total_height };
    }
    let first_visible = ((scroll_top.max(0) / row_height) as usize).min(count);
    let last_visible = (((scroll_top.max(0) + viewport_height.max(0)) / row_height) as usize).min(count);
    let start = first_visible.saturating_sub(OVERSCAN);
    let end = last_visible.saturating_add(OVERSCAN).min(count).max(start);
    let virtual_rows = (start..end)
        .map(|index| VirtualRowData {
            index,
            key: format!("virtual-row-{index}"),
            start: (index as i64).saturating_mul(row_height),
        })
        .collect();
    VirtualTableResult { virtual_rows, total_height }
}

pub fn container_height(total_height: i64, row_height: i64, has_add_draft_row: bool) -> i64 {
    if has_add_draft_row {
        total_height + row_height
    } else {
        total_height
    }
}
