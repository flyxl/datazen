use std::borrow::Cow;

use datazen_ui_api::{
    row_height_for_density,
    types::{DEFAULT_GRID_DENSITY, DEFAULT_GRID_ROW_NUMBERS},
    NativeTableRowCount,
};
use rune::{El, GrowthSpec, ScrollPos, TextStyle, col, el, row, text};

use crate::i18n::t;
use crate::ui::palette;

use super::{scrollbars, DataGrid, GUTTER_W, ROW_H};
use crate::ui::field::TextField;

/// Which grid a render call targets: the table page's or the query result's.
/// The result keys are prefixed so both can be mounted in one frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridSlot {
    Table,
    Result,
    /// The sync wizard's preview: the same look and keys as a result page; the
    /// tools page and the workspace never mount at once.
    SyncPreview,
}

impl GridSlot {
    fn key(self, base: &str) -> String {
        match self {
            Self::Table => base.to_owned(),
            Self::Result | Self::SyncPreview => format!("result-{base}"),
        }
    }
}

pub fn grid_of<'a>(state: &'a crate::AppState, slot: GridSlot) -> &'a DataGrid {
    match slot {
        GridSlot::Table => &state.grid,
        GridSlot::Result => state.result_grid.as_ref().expect("the result grid is mounted"),
        GridSlot::SyncPreview => state
            .sync_preview_grid
            .as_ref()
            .expect("the sync preview grid is mounted"),
    }
}

#[derive(Clone, Copy)]
struct CellPaint {
    dark: bool,
    alternating_rows: bool,
    show_row_numbers: bool,
    row_tail_width: f32,
    row_height: f32,
}

impl CellPaint {
    fn for_slot(state: &crate::AppState, slot: GridSlot, dark: bool) -> Self {
        // Settings applies to the two user-facing data grids, not Sync's
        // independent comparison preview.
        let shared = slot != GridSlot::SyncPreview;
        let row_height = match slot {
            GridSlot::SyncPreview => ROW_H,
            GridSlot::Table | GridSlot::Result => {
                let base = if slot == GridSlot::Result { 32 } else { ROW_H as i64 };
                let density = state.desktop.read_native_grid_density().unwrap_or(DEFAULT_GRID_DENSITY);
                row_height_for_density(base, density) as f32
            }
        };
        Self {
            dark,
            alternating_rows: !shared || state.desktop.read_native_grid_alternating_rows().unwrap_or(true),
            show_row_numbers: !shared || state.desktop.read_native_grid_row_numbers()
                .unwrap_or(DEFAULT_GRID_ROW_NUMBERS),
            row_tail_width: 0.0,
            row_height,
        }
    }

    fn gutter_width(self) -> f32 {
        if self.show_row_numbers { GUTTER_W } else { 0.0 }
    }

    fn row_background(self, row_index: u32, gp: palette::GridPalette) -> rune::Color {
        if !self.alternating_rows || row_index % 2 == 0 {
            gp.zebra_even
        } else if self.dark {
            gp.zebra_odd_dark
        } else {
            gp.zebra_odd_light
        }
    }
}

/// The scrolling sheet the table page and the query result share: a sticky header
/// over a window of virtual rows.
fn sheet(
    grid: &DataGrid,
    slot: GridSlot,
    ink: rune::Color,
    well: rune::Color,
    height: f32,
    mut paint: CellPaint,
) -> El {
    let scroll_x = grid.scroll_x();
    let table_scroll_y = (slot == GridSlot::Table).then(|| grid.table_scroll_y());
    let scroll_key = slot.key("data-grid-scroll");
    let data_width = grid.sheet_data_width();
    let classic = slot != GridSlot::SyncPreview;
    let frame_width = classic.then(|| {
        rune::memo::layout_with(&scrollbars::area_key(&scroll_key), |rect| rect[2])
    }).flatten();
    let gutters = scrollbars::Gutters::for_content(
        frame_width.map(|width| [width, height]),
        [paint.gutter_width() + data_width, ROW_H + grid.rows as f32 * paint.row_height],
    );
    if !paint.show_row_numbers && slot == GridSlot::Table && grid.cols > 0 {
        // Only real row background is selectable, not cells or virtual
        // pads. Leave the classic vertical strip to the actual scrollbar.
        let viewport_width = frame_width.unwrap_or(0.0) - gutters.right();
        paint.row_tail_width = (viewport_width - data_width).max(0.0);
    }
    let width = paint.gutter_width() + data_width + paint.row_tail_width;
    // Changing the estimated height alone keeps old measured heights in Rune's
    // ledger. Key the virtual stack by its geometry, not the data it paints:
    // row/cell keys, the viewport, and keyed reveals keep their identities.
    let rows_key = if paint.row_height == ROW_H {
        slot.key("data-grid-rows")
    } else {
        slot.key(&format!("data-grid-rows-{}", paint.row_height as i64))
    };
    let sheet = col()
        .key(slot.key("data-grid-sheet"))
        .w(width)
        .child(header_row(grid, slot, ink, paint).sticky_top(0.0))
        .child(
            col()
                .key(rows_key)
                .w(width)
                .growth(GrowthSpec::default())
                .virtual_rows(
                    grid.rows as usize,
                    paint.row_height,
                    move |_state: &crate::AppState, index| {
                        Cow::Owned(slot.key(&format!("grid-row-{index}")))
                    },
                    move |state: &crate::AppState, index| {
                        paint_row(grid_of(state, slot), slot, index as u32, ink, paint)
                    },
                )
                .virtual_rows_version(1u32),
        );
    let (focus_row, focus_col) = grid.focus();
    let active = grid.selected_row().map_or_else(
        || format!("cell-{focus_row}-{focus_col}"),
        |row| if paint.show_row_numbers { format!("rownum-{row}") } else { format!("grid-row-{row}") },
    );
    let viewport = col()
        .key(scroll_key)
        .w_full()
        .h(height)
        .fill(well)
        .radius(8.0)
        .role("grid")
        .label(grid.table_label())
        .a11y_value(format!("{} rows × {} columns", grid.rows, grid.cols))
        .active_descendant(slot.key(&active))
        .scroll_xy()
        .on_scroll_offset(move |at: ScrollPos| {
            scroll_x.set(at.offset[0]);
            if let Some(scroll_y) = &table_scroll_y {
                scroll_y.set(at.offset[1]);
            }
        })
        .child(sheet);
    if classic {
        let k = crate::ui::kit::kit(paint.dark);
        scrollbars::attach::<crate::AppState>(viewport, gutters, well, k.c.border_strong, k.c.text_faint)
    } else {
        viewport
    }
}

pub fn panel(
    state: &crate::AppState,
    ink: rune::Color,
    muted: rune::Color,
    well: rune::Color,
    viewport: rune::Color,
    dark: bool,
) -> El {
    let grid = &state.grid;
    let status = status_line(grid);
    col()
        .key("data-grid")
        .w_full()
        .h(560.0)
        .gap(8.0)
        .child(text(status).font(label_style()).color(muted).key("data-grid-status"))
        .child(status_banner(state, muted, well))
        .child(pending_changes_bar(state, muted, well))
        .child(filter_bar(state, ink, muted, well, dark))
        .child(fk_bar(state, ink, muted, well))
        .child(sheet(grid, GridSlot::Table, ink, viewport, 380.0,
            CellPaint::for_slot(state, GridSlot::Table, dark)))
        .child(footer(state, ink, muted))
}

/// A4: status banner (状态横幅) surfacing load/save/read-only/filter and errors.
fn status_banner(state: &crate::AppState, muted: rune::Color, well: rune::Color) -> El {
    let grid = &state.grid;
    let mut lines: Vec<(String, String)> = Vec::new();
    if state.table_loading.is_some() {
        lines.push(("table-loading-banner".into(), "Loading table page…".into()));
    }
    if let Some(error) = state.active_cell_preview_error() {
        lines.push(("pending-preview-error".into(), error.to_owned()));
    }
    if state.commit_loading.is_some() {
        lines.push(("commit-loading-banner".into(), "Saving staged changes…".into()));
    }
    if let Some(reason) = grid.read_only_reason() {
        lines.push(("grid-readonly".into(), format!("{}", t("query.gridReadOnly.tooltip").replace("{reason}", &reason))));
    }
    if let Some(filter) = &state.filter {
        lines.push((
            "grid-filter-active".into(),
            format!("Filter: {} contains \"{}\"", filter.column, filter.contains),
        ));
    }
    if let Some(error) = &state.fk_error {
        // "fk-error" kept for existing tests/consumers.
        lines.push(("fk-error".into(), error.clone()));
    }
    let mut col_el = col().key("grid-status-banner").w_full().gap(4.0);
    for (key, msg) in lines {
        col_el = col_el.child(
            row()
                .key(key)
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(well)
                .child(text(msg).font(label_style()).color(muted)),
        );
    }
    col_el
}

/// A4: pending/staged changes bar (待提交条) showing staged change count.
/// Commit/Rollback buttons remain in the footer for stable hit-testing and
/// compatibility with existing interactions.
fn pending_changes_bar(state: &crate::AppState, muted: rune::Color, well: rune::Color) -> El {
    let dirty = state.grid.dirty_cells();
    if dirty == 0 || state.grid.read_only_reason().is_some() {
        return row().key("pending-changes-bar").w_full().h(0.0);
    }
    let mut bar = row()
        .key("pending-changes-bar")
        .w_full()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .fill(well)
        .child(
            text(format!(
                "{dirty} pending change{} — use Commit/Rollback",
                if dirty == 1 { "" } else { "s" }
            ))
            .font(label_style())
            .color(muted),
        );
    if state.desktop.supports_native_cell_preview() && state.commit_loading.is_none() {
        bar = bar.child(row()
            .key("pending-preview")
            .h(24.0)
            .px(8.0)
            .center_y()
            .role("button")
            .focusable(true)
            .label(t("tableData.preview"))
            .child(text(t("tableData.preview")).font(label_style()).color(muted))
            .on_press(|state: &mut crate::AppState| state.request_cell_preview()));
    }
    bar
}

/// T5: foreign-key navigation bar with A4 breadcrumb trail.
fn fk_bar(state: &crate::AppState, ink: rune::Color, muted: rune::Color, well: rune::Color) -> El {
    let mut bar = row()
        .key("fk-navigation")
        .w_full()
        .h(28.0)
        .gap(8.0)
        .center_y();
    if let Some((key, index)) = state.outbound_fk_of_focus() {
        let target = key
            .target_columns
            .get(index)
            .cloned()
            .unwrap_or_else(|| "?".into());
        bar = bar.child(
            row()
                .key("fk-go-to")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(well)
                .child(
                    text(format!("\u{2192} {} · {}", key.target_table, target))
                        .font(label_style())
                        .color(ink),
                )
                .on_press(|state: &mut crate::AppState| state.go_to_referenced_row()),
        );
    }
    let inbound_open = state.inbound_refs.is_some();
    bar = bar.child(
        row()
            .key("fk-referenced-by")
            .h(24.0)
            .px(8.0)
            .center_y()
            .radius(6.0)
            .fill(well)
            .child(
                text(if inbound_open { "Referenced by \u{00d7}" } else { "Referenced by\u{2026}" })
                    .font(label_style())
                    .color(ink),
            )
            .on_press(move |state: &mut crate::AppState| {
                if inbound_open {
                    state.close_inbound_refs();
                } else {
                    state.load_inbound_refs();
                }
            }),
    );
    if state.inbound_loading.is_some() {
        bar = bar.child(
            text("scanning the references…".to_owned())
                .key("fk-inbound-loading")
                .font(label_style())
                .color(muted),
        );
    }
    let mut column = col().key("fk-bar").w_full().gap(4.0).child(bar);
    // A4: breadcrumb trail of FK navigation steps.
    if !state.fk_breadcrumbs.is_empty() {
        let mut crumbs = row().key("fk-breadcrumbs").w_full().h(24.0).gap(4.0).center_y();
        for (i, crumb) in state.fk_breadcrumbs.iter().enumerate() {
            crumbs = crumbs.child(
                row()
                    .key(format!("fk-crumb-{i}"))
                    .h(22.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(well)
                    .child(text(format!("{} ›", crumb.label)).font(label_style()).color(ink))
                    .on_press(move |state: &mut crate::AppState| {
                        state.navigate_fk_breadcrumb(i);
                    }),
            );
        }
        crumbs = crumbs.child(
            text(state.grid.table_label().to_owned())
                .key("fk-crumb-current")
                .font(label_style())
                .color(muted),
        );
        column = column.child(crumbs);
    }
    if let Some(refs) = &state.inbound_refs {
        let mut list = col().key("fk-inbound-list").w_full().gap(4.0);
        if refs.is_empty() {
            list = list.child(
                text("No table references this one".to_owned())
                    .key("fk-inbound-empty")
                    .font(label_style())
                    .color(muted),
            );
        }
        for (index, reference) in refs.iter().enumerate() {
            list = list.child(
                row()
                    .key(format!("fk-inbound-{index}"))
                    .w_full()
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(well)
                    .child(
                        text(format!(
                            "\u{2190} {} · {} (references {})",
                            reference.table, reference.column, reference.referenced_column
                        ))
                        .font(label_style())
                        .color(ink),
                    )
                    .on_press(move |state: &mut crate::AppState| {
                        state.locate_inbound_ref(index);
                    }),
            );
        }
        column = column.child(list);
    }
    column
}

/// The query result area: the same sheet as the table page, without the filter
/// bar and the footer, and not editable. The sync preview reuses it.
pub fn result_panel(
    state: &crate::AppState,
    grid: &DataGrid,
    slot: GridSlot,
    ink: rune::Color,
    muted: rune::Color,
    well: rune::Color,
    dark: bool,
) -> El {
    col()
        .key("result-grid")
        .w_full()
        .gap(8.0)
        .child(
            text(format!("{} · {} rows × {} cols", grid.table_label(), grid.rows, grid.cols))
                .font(label_style())
                .color(muted)
                .key("result-grid-status"),
        )
        .child(sheet(grid, slot, ink, well, 200.0, CellPaint::for_slot(state, slot, dark)))
}

/// The status line above the grid: page shape, column window, selection, dirty cells.
pub fn status_line(grid: &DataGrid) -> String {
    let selection = if let Some(row) = grid.selected_row() {
        format!("row {}", grid.page().saturating_mul(grid.page_size()).saturating_add(row + 1))
    } else if grid.has_selection() {
        let (r0, r1, c0, c1) = grid.rect();
        format!("r{r0}c{c0}..r{r1}c{c1}")
    } else {
        "none".to_owned()
    };
    format!(
        "{}×{} · window at column {} · selection {selection} · {} dirty",
        grid.rows,
        grid.cols,
        grid.origin_col(),
        grid.dirty_cells()
    )
}

pub(super) fn table_count_text(grid: &DataGrid) -> String {
    match grid.row_count() {
        NativeTableRowCount::Exact(total) => format!("{total} rows"),
        NativeTableRowCount::Estimated(total) => format!(
            "{} {}",
            t("pagination.rowCountEstimated").replace("{count}", &total.to_string()),
            t("connWin.rowCount"),
        ),
        NativeTableRowCount::Unknown => t("pagination.rowsUnknown").to_owned(),
    }
}

pub(super) fn table_page_text(grid: &DataGrid) -> String {
    let current = u64::from(grid.page()) + 1;
    match grid.exact_page_count() {
        Some(pages) => format!("Page {current} / {pages}"),
        None => format!("Page {current}"),
    }
}

/// One text filter on one column: a column chip that cycles, a text field, the row count.
fn filter_bar(state: &crate::AppState, ink: rune::Color, muted: rune::Color, well: rune::Color, dark: bool) -> El {
    let column = state
        .grid
        .column_name(state.filter_column)
        .map(str::to_owned)
        .unwrap_or_else(|| "-".into());
    row()
        .key("grid-filter")
        .w_full()
        .h(28.0)
        .gap(8.0)
        .center_y()
        .child(
            row()
                .key("filter-column")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(well)
                .child(text(format!("in: {column}")).font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.cycle_filter_column()),
        )
        .child(
            TextField::new("filter-input", &state.filter_editor)
                .height(24.0)
                .text_style(label_style())
                .width(220.0)
                .on_edit(|state: &mut crate::AppState| &mut state.filter_editor)
                .on_submit(|state: &mut crate::AppState, tx| {
                    state.apply_filter();
                    tx.blur();
                })
                .build(dark),
        )
        .child(
            text(table_count_text(&state.grid))
                .font(label_style())
                .color(muted),
        )
}

/// Prev / next paging over the table behind the loaded page, and host job markers.
fn footer(state: &crate::AppState, ink: rune::Color, muted: rune::Color) -> El {
    let grid = &state.grid;
    let mut footer = row()
        .key("grid-footer")
        .w_full()
        .h(28.0)
        .gap(8.0)
        .center_y();
    if state.commit_loading.is_some() {
        footer = footer.child(
            row()
                .key("commit-loading")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("Saving…").font(label_style()).color(muted)),
        );
    } else if state.table_loading.is_some() {
        footer = footer.child(
            row()
                .key("table-loading")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text(t("common.loading")).font(label_style()).color(muted)),
        );
    }
    let next_enabled = state.can_next_table_page();
    let next = row()
        .key("page-next")
        .role("button")
        .disabled(!next_enabled)
        .h(24.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .child(text("Next ›").font(label_style()).color(if next_enabled { ink } else { muted }));
    let next = if next_enabled {
        next.on_press(|state: &mut crate::AppState| state.next_table_page())
    } else {
        next
    };
    footer = footer
        .child(
            row()
                .key("page-prev")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text("‹ Prev").font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.prev_table_page()),
        )
        .child(
            text(table_page_text(grid))
                .key("grid-page-label")
                .font(label_style())
                .color(muted),
        )
        .child(next)
        .child(
            text(format!("{} per page · {}", grid.page_size(), table_count_text(grid)))
                .font(label_style())
                .color(muted),
        )
        .child(
            row()
                .key("commit-staged")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text(t("tableData.commit")).font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.commit_staged_edits()),
        )
        .child(
            row()
                .key("rollback-staged")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .child(text(t("tableData.rollback")).font(label_style()).color(ink))
                .on_press(|state: &mut crate::AppState| state.rollback_staged_edits()),
        );
    footer
}

fn header_row(grid: &DataGrid, slot: GridSlot, ink: rune::Color, paint: CellPaint) -> El {
    let gp = palette::grid_palette(paint.dark);
    let mut line = row().key(slot.key("data-grid-header")).h(ROW_H).fill(gp.header_bg);
    if paint.show_row_numbers {
        line = line.child(
            row()
                .key(slot.key("head-rownum"))
                .w(GUTTER_W)
                .h(ROW_H)
                .px(6.0)
                .center_y()
                .fill(gp.header_gutter_bg)
                .child(text("#").font(label_style()).color(ink)),
        );
    }
    let mut cursor = 0u32;
    for col_index in grid.built_columns() {
        if col_index > cursor {
            line = line.child(el().w(grid.span_width(cursor, col_index)).h(ROW_H));
        }
        line = line.child(header_cell(grid, slot, col_index, ink, paint));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w(grid.span_width(cursor, grid.cols)).h(ROW_H));
    }
    line
}

fn paint_row(grid: &DataGrid, slot: GridSlot, row_index: u32, ink: rune::Color, paint: CellPaint) -> El {
    let cols = grid.built_columns();
    let gp = palette::grid_palette(paint.dark);
    let mut line = row()
        .key(slot.key(&format!("grid-row-{row_index}")))
        .role("row")
        .h(paint.row_height);
    let row_selected = grid.selected_row() == Some(row_index);
    if paint.show_row_numbers {
        // The existing A4 numbering stays global and 1-based.
        let num = grid.page().saturating_mul(grid.page_size()).saturating_add(row_index + 1);
        let mut gutter = row()
            .key(slot.key(&format!("rownum-{row_index}")))
            .role("rowheader")
            .w(GUTTER_W)
            .h(paint.row_height)
            .px(6.0)
            .center_y()
            .fill(if row_selected { gp.selected } else { gp.gutter_bg })
            .child(text(num.to_string()).font(label_style()).color(ink));
        if slot == GridSlot::Table {
            gutter = gutter.on_press(move |state: &mut crate::AppState| {
                state.grid.select_row(row_index);
            });
        }
        line = line.child(gutter);
    }
    let mut cursor = 0u32;
    for col_index in cols {
        if col_index > cursor {
            line = line.child(el().w(grid.span_width(cursor, col_index)).h(paint.row_height));
        }
        line = line.child(cell(grid, slot, row_index, col_index, ink, paint));
        cursor = col_index + 1;
    }
    if cursor < grid.cols {
        line = line.child(el().w(grid.span_width(cursor, grid.cols)).h(paint.row_height));
    }
    if paint.row_tail_width > 0.0 {
        line = line.child(
            el().key(format!("grid-row-background-{row_index}"))
                .w(paint.row_tail_width)
                .h(paint.row_height)
                .fill(if row_selected { gp.selected } else { paint.row_background(row_index, gp) })
                .a11y_hidden()
                .on_press(move |state: &mut crate::AppState| state.grid.select_row(row_index)),
        );
    }
    line
}

fn header_cell(
    grid: &DataGrid,
    slot: GridSlot,
    col_index: u32,
    ink: rune::Color,
    paint: CellPaint,
) -> El {
    let label = grid
        .column_name(col_index)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("c{col_index}"));
    let label = match grid.sort() {
        Some((col, true)) if col == col_index => format!("{label} ↓"),
        Some((col, false)) if col == col_index => format!("{label} ↑"),
        _ => label,
    };
    let w = grid.col_w(col_index);
    let mut cell = row()
        .key(slot.key(&format!("head-{col_index}")))
        .role("columnheader")
        .w(w)
        .h(ROW_H)
        .px(8.0)
        .center_y()
        .child(text(label).font(label_style()).color(ink));
    if slot == GridSlot::Table {
        cell = cell.on_press(move |state: &mut crate::AppState| state.cycle_sort(col_index));
    }
    if col_index == 0 {
        let gp = palette::grid_palette(paint.dark);
        cell = cell.sticky_left(paint.gutter_width()).fill(gp.frozen_bg);
    }
    cell
}

/// A hit can be a text child of a cell. Resolve the enclosing key in the
/// presented tree instead of inferring a column from scroll offsets.
fn table_cell_at(hit: &rune::Picked) -> Option<(u32, u32)> {
    hit.path().rsplit('/').find_map(|part| {
        let (row, col) = part.strip_prefix("cell-")?.split_once('-')?;
        Some((row.parse().ok()?, col.parse().ok()?))
    })
}

fn cell(
    grid: &DataGrid,
    slot: GridSlot,
    row_index: u32,
    col_index: u32,
    ink: rune::Color,
    paint: CellPaint,
) -> El {
    let dark = paint.dark;
    let selected = grid.covers(row_index, col_index);
    let active = selected && grid.selected_row().is_none() && grid.focus == (row_index, col_index);
    let frozen = col_index == 0;
    let current_hit = grid.is_current_find_hit(row_index, col_index);
    let any_hit = grid.is_find_hit(row_index, col_index);
    let gp = palette::grid_palette(dark);
    let fill = if current_hit {
        gp.find_current
    } else if any_hit {
        gp.find_hit
    } else if active {
        gp.active
    } else if selected {
        gp.selected
    } else if frozen {
        gp.frozen_cell
    } else {
        paint.row_background(row_index, gp)
    };
    let w = grid.col_w(col_index);
    let mut box_el = row()
        .key(slot.key(&format!("cell-{row_index}-{col_index}")))
        .role("gridcell")
        .w(w)
        .h(paint.row_height)
        .px(8.0)
        .center_y()
        .fill(fill);
    if slot == GridSlot::Table {
        box_el = box_el
            .on_press_tx(move |state: &mut crate::AppState, tx| {
                state.grid.click_cell(row_index, col_index, tx.mods().shift);
            })
            .on_drag(move |state: &mut crate::AppState, drag, tx| {
                use rune::Phase;
                if drag.phase == Phase::Down {
                    // A disappearing virtualized press has no cell callback.
                    // Do not let its saved selection be reused by a new drag.
                    state.grid.finish_cell_drag(true);
                    return;
                }
                if state.grid.cancel_table_drag_if_needed(true) {
                    return;
                }
                if drag.cancelled() {
                    state.grid.finish_cell_drag(true);
                    return;
                }
                // Match Rune's click slop so small pointer jitter preserves
                // both plain click and double-click into the editor.
                if drag.data == 0.0 {
                    const CLICK_SLOP: f32 = 4.0;
                    if drag.dx.abs().max(drag.dy.abs()) <= CLICK_SLOP
                        || !state.grid.begin_cell_drag(row_index, col_index)
                    {
                        return;
                    }
                    drag.data = 1.0;
                }
                let [x, y] = drag.window_point();
                let finished = drag.phase == Phase::Up;
                tx.element_at_in("data-grid-scroll", x, y, move |state: &mut crate::AppState, hit, _| {
                    if let Some((end_row, end_col)) = hit.as_ref().and_then(table_cell_at) {
                        state.grid.extend_cell_drag(end_row, end_col);
                    }
                    if finished {
                        state.grid.finish_cell_drag(false);
                    }
                });
            })
            .on_double_press_tx(move |state: &mut crate::AppState, tx| {
                if tx.mods().shift {
                    return;
                }
                if state.grid.selected_row().is_some() || state.grid.focus() != (row_index, col_index) {
                    state.grid.select(row_index, col_index);
                }
                if !state.grid.editing {
                    state.grid.begin_edit();
                }
            });
    } else if slot == GridSlot::Result {
        box_el = box_el.on_press(move |state: &mut crate::AppState| {
            if let Some(grid) = state.result_grid.as_mut() {
                grid.select(row_index, col_index);
            }
            state.result_focused = true;
        });
    }
    if frozen {
        box_el = box_el.sticky_left(paint.gutter_width());
    }
    if active && grid.editing {
        box_el.child(
            TextField::new(slot.key("grid-editor"), &grid.editor)
                .height(paint.row_height - 4.0)
                .text_style(editor_font())
                .width((w - 16.0).max(24.0))
                .on_edit(|state: &mut crate::AppState| &mut state.grid.editor)
                .on_key(|state: &mut crate::AppState, key, tx| {
                    if matches!(key.key, rune::Key::Escape) {
                        state.grid.cancel_edit();
                        tx.blur();
                        true
                    } else {
                        false
                    }
                })
                .on_submit(|state: &mut crate::AppState, tx| {
                    state.commit_grid_edit();
                    tx.blur();
                })
                .build(dark),
        )
    } else {
        box_el.child(text(grid.cell(row_index, col_index)).font(label_style()).color(ink))
    }
}

fn label_style() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}

fn editor_font() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}
