//! The DataTable column header — a Rune port of
//! `packages/frontend/src/components/DataTable/TableHeader.tsx`.
//!
//! One 40pt row: an optional row-number gutter, then one cell per column. A cell
//! holds the name and type, a filter button, the sort position badge, the sort
//! direction arrow, the sort-menu chevron and the resize grip — in that order, as
//! in the React JSX.
//!
//! # Key convention
//! `data-table-header-<role>`; the React `data-col-header` / `data-sort-badge` /
//! `data-sort-indicator` / `data-sort-menu` attributes become element keys.
//!
//! | part                  | key                              |
//! |-----------------------|----------------------------------|
//! | header row            | `data-table-header`              |
//! | row-number gutter     | `data-table-header-gutter`       |
//! | gutter label (`#`)    | `data-table-header-gutter-label` |
//! | column cell           | `data-table-header-col-<i>`      |
//! | column label block    | `data-table-header-label-<i>`    |
//! | column name           | `data-table-header-name-<i>`     |
//! | column type           | `data-table-header-type-<i>`     |
//! | filter button         | `data-table-header-filter-<i>`   |
//! | sort position badge   | `data-table-header-sort-position-<i>` |
//! | sort direction arrow  | `data-table-header-sort-direction-<i>` |
//! | sort menu trigger     | `data-table-header-sort-<i>`     |
//! | sort menu             | `data-table-header-sort-menu-<i>` (`-item0`…`-item2`) |
//! | resize grip           | `data-table-header-resize-<i>`   |
//!
//! # State the view owns
//! React keeps the drag marker (`dragIndex`/`dropIndex`) and the open sort menu in
//! `useState` and calls `showNativeContextMenu` for the menu. Rune has neither
//! component-local state nor native popups, so the app state owns `reorder_drag`,
//! `reorder_drop` and `sort_menu`, and the callbacks below drive them; the sort
//! menu is a `ui::menu::Menu` anchored under its chevron.
//!
//! # Deliberate gaps
//! * `group-hover:opacity-100` on the sort chevron has no Rune equivalent (no
//!   ancestor-hover styling): the chevron stays visible at low opacity.
//! * `dataTypeTextClass` (`text-dt-*` from `themes.css`) has no Rust port and no
//!   kit token; the type label uses `c.text_muted`. React's 11pt mono is on the
//!   kit's 12pt type scale to stay on tokens.
//! * The resize grip reports pointer moves itself (`on_resize_move` /
//!   `on_resize_end`) instead of React's window-level mouse listeners.
//! * `onDragStart`/`onDragOver`/`onDrop` map to `El::on_drag` (Down/Move/Up). Rune
//!   sends every phase to the pressed cell only, so `on_reorder_over` hit-tests the
//!   pointer against the header's column offsets instead of receiving a hovered
//!   cell; the pressed cell also receives the Up, so the drop commit reads the app
//!   state. The insertion marker is a pinned 2pt left strip (`border-l-2` in React).
//! * React's `aria-label="Filter"` is hardcoded English; it uses the existing
//!   `filter.filter` key here (no dedicated `dataTable.*` key exists).
//! * There is no `min-w-max`: a Rune row cannot size itself from its children and
//!   still fill the viewport, so the header is `w_full()` and the grid's
//!   horizontal scroll owns a column set wider than the window.

use std::rc::Rc;

use rune::{el, icon, row, text, Align, Cursor, El};
use rune_kit::{ControlSize, IconButton, IconButtonVariant};

use crate::i18n::t;
use crate::icons::{ICON_ARROW_DOWN, ICON_ARROW_UP, ICON_CHEVRON_DOWN, ICON_FILTER};
use crate::ui::menu::{Item, Menu, Placement};

const HEADER_HEIGHT: f32 = 40.0;
const GUTTER_WIDTH: f32 = 40.0;
const DEFAULT_COLUMN_WIDTH: f32 = 160.0;
const CELL_PAD: f32 = 12.0;
const CELL_GAP: f32 = 8.0;
const GRIP_WIDTH: f32 = 5.0;
const SORT_GLYPH: f32 = 16.0;

/// `ColumnDef` from the React props.
#[derive(Clone, Debug, Default)]
pub(crate) struct Column {
    /// React's list key (`key={col.id}`); Rune derives element keys from the
    /// column index, so this is kept for the port's parity only.
    #[allow(dead_code)]
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) ty: Option<String>,
}

// The view that builds columns from the schema lands in a later task.
#[allow(dead_code)]
impl Column {
    pub(crate) fn new(id: impl Into<String>, name: impl Into<String>) -> Column {
        Column {
            id: id.into(),
            name: name.into(),
            ty: None,
        }
    }

    pub(crate) fn ty(mut self, ty: impl Into<String>) -> Column {
        self.ty = Some(ty.into());
        self
    }
}

/// `SortCondition` from the React props.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SortCondition {
    pub(crate) column: String,
    pub(crate) descending: bool,
}

// The view that hands the sorts down lands in a later task.
#[allow(dead_code)]
impl SortCondition {
    pub(crate) fn new(column: impl Into<String>, descending: bool) -> SortCondition {
        SortCondition {
            column: column.into(),
            descending,
        }
    }
}

/// How this column sorts right now; `None` is React's `'none'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sorted {
    None,
    Asc,
    Desc,
}

type SortFn<S> = Rc<dyn Fn(&mut S, &str, bool)>;
type ColumnFn<S> = Rc<dyn Fn(&mut S, &str)>;
type IndexFn<S> = Rc<dyn Fn(&mut S, usize)>;

/// The header row. Build it with the columns and sorts the view owns, then hand the
/// callbacks the sort, filter and resize events.
pub(crate) struct TableHeader<S: 'static> {
    key: String,
    columns: Vec<Column>,
    sorts: Vec<SortCondition>,
    column_widths: Option<Vec<f32>>,
    frozen_columns: usize,
    show_row_numbers: bool,
    sortable: bool,
    grid_semantics: bool,
    reorder_drag: Option<usize>,
    reorder_drop: Option<usize>,
    sort_menu: Option<usize>,
    on_sort: Option<SortFn<S>>,
    on_remove_sort: Option<ColumnFn<S>>,
    on_filter_column: Option<ColumnFn<S>>,
    on_resize_start: Option<Rc<dyn Fn(&mut S, usize, f32)>>,
    on_resize_move: Option<Rc<dyn Fn(&mut S, usize, f32)>>,
    on_resize_end: Option<Rc<dyn Fn(&mut S)>>,
    on_reorder_start: Option<IndexFn<S>>,
    on_reorder_over: Option<IndexFn<S>>,
    on_reorder_drop: Option<Rc<dyn Fn(&mut S)>>,
    on_sort_menu_toggle: Option<IndexFn<S>>,
    on_sort_menu_close: Option<Rc<dyn Fn(&mut S)>>,
}

// The DataTable view that calls this lands in a later migration task;
// `tests/data_table_parts.rs` is the caller today.
#[allow(dead_code)]
impl<S: 'static> TableHeader<S> {
    pub(crate) fn new(columns: Vec<Column>, sorts: Vec<SortCondition>) -> TableHeader<S> {
        TableHeader {
            key: "data-table-header".to_owned(),
            columns,
            sorts,
            column_widths: None,
            frozen_columns: 0,
            show_row_numbers: true,
            sortable: true,
            grid_semantics: false,
            reorder_drag: None,
            reorder_drop: None,
            sort_menu: None,
            on_sort: None,
            on_remove_sort: None,
            on_filter_column: None,
            on_resize_start: None,
            on_resize_move: None,
            on_resize_end: None,
            on_reorder_start: None,
            on_reorder_over: None,
            on_reorder_drop: None,
            on_sort_menu_toggle: None,
            on_sort_menu_close: None,
        }
    }

    /// Root key; the per-cell keys derive from it.
    pub(crate) fn key(mut self, key: impl Into<String>) -> TableHeader<S> {
        self.key = key.into();
        self
    }

    /// Widths by column index (React `columnWidths`); a missing entry is 160pt.
    pub(crate) fn column_widths(mut self, widths: Vec<f32>) -> TableHeader<S> {
        self.column_widths = Some(widths);
        self
    }

    /// How many leading columns stay pinned while the body scrolls sideways.
    pub(crate) fn frozen_columns(mut self, frozen: usize) -> TableHeader<S> {
        self.frozen_columns = frozen;
        self
    }

    /// The `#` gutter (React `showRowNumbers`, default on).
    pub(crate) fn show_row_numbers(mut self, show: bool) -> TableHeader<S> {
        self.show_row_numbers = show;
        self
    }

    /// React `sortable`; off means a cell press does not sort.
    pub(crate) fn sortable(mut self, sortable: bool) -> TableHeader<S> {
        self.sortable = sortable;
        self
    }

    /// React `gridSemantics`: `role="row"` / `role="columnheader"` / `aria-sort`.
    pub(crate) fn grid_semantics(mut self, grid: bool) -> TableHeader<S> {
        self.grid_semantics = grid;
        self
    }

    /// The column being dragged for a reorder, if any (React `dragIndex`).
    pub(crate) fn reorder_drag(mut self, drag: Option<usize>) -> TableHeader<S> {
        self.reorder_drag = drag;
        self
    }

    /// The insertion column under the dragged one (React `dropIndex`).
    pub(crate) fn reorder_drop(mut self, drop: Option<usize>) -> TableHeader<S> {
        self.reorder_drop = drop;
        self
    }

    /// The column whose sort menu is open, if any.
    pub(crate) fn sort_menu(mut self, open: Option<usize>) -> TableHeader<S> {
        self.sort_menu = open;
        self
    }

    /// A cell press or one of the menu items.
    pub(crate) fn on_sort(mut self, f: impl Fn(&mut S, &str, bool) + 'static) -> TableHeader<S> {
        self.on_sort = Some(Rc::new(f));
        self
    }

    /// The menu's third item; also the third press of the sort cycle.
    pub(crate) fn on_remove_sort(mut self, f: impl Fn(&mut S, &str) + 'static) -> TableHeader<S> {
        self.on_remove_sort = Some(Rc::new(f));
        self
    }

    /// The per-cell filter button.
    pub(crate) fn on_filter_column(mut self, f: impl Fn(&mut S, &str) + 'static) -> TableHeader<S> {
        self.on_filter_column = Some(Rc::new(f));
        self
    }

    /// The resize grip's press, with the column index and the pointer's window x.
    pub(crate) fn on_resize_start(
        mut self,
        f: impl Fn(&mut S, usize, f32) + 'static,
    ) -> TableHeader<S> {
        self.on_resize_start = Some(Rc::new(f));
        self
    }

    /// Pointer travel while the grip is held, in window points.
    pub(crate) fn on_resize_move(
        mut self,
        f: impl Fn(&mut S, usize, f32) + 'static,
    ) -> TableHeader<S> {
        self.on_resize_move = Some(Rc::new(f));
        self
    }

    /// The grip was released.
    pub(crate) fn on_resize_end(mut self, f: impl Fn(&mut S) + 'static) -> TableHeader<S> {
        self.on_resize_end = Some(Rc::new(f));
        self
    }

    /// A drag left the pressed cell (React `onDragStart`).
    pub(crate) fn on_reorder_start(
        mut self,
        f: impl Fn(&mut S, usize) + 'static,
    ) -> TableHeader<S> {
        self.on_reorder_start = Some(Rc::new(f));
        self
    }

    /// The column under the pointer (React `onDragOver`); hit-tested from the
    /// pressed cell's local drag position, so it can differ from the pressed column.
    pub(crate) fn on_reorder_over(mut self, f: impl Fn(&mut S, usize) + 'static) -> TableHeader<S> {
        self.on_reorder_over = Some(Rc::new(f));
        self
    }

    /// The pointer was released (React `onDrop` + `onDragEnd`); the app commits the
    /// move from its own `reorder_drag` / `reorder_drop` and clears both.
    pub(crate) fn on_reorder_drop(mut self, f: impl Fn(&mut S) + 'static) -> TableHeader<S> {
        self.on_reorder_drop = Some(Rc::new(f));
        self
    }

    /// The chevron under a column; the app opens that column's menu.
    pub(crate) fn on_sort_menu_toggle(
        mut self,
        f: impl Fn(&mut S, usize) + 'static,
    ) -> TableHeader<S> {
        self.on_sort_menu_toggle = Some(Rc::new(f));
        self
    }

    /// The menu closed through Escape or an outside press.
    pub(crate) fn on_sort_menu_close(mut self, f: impl Fn(&mut S) + 'static) -> TableHeader<S> {
        self.on_sort_menu_close = Some(Rc::new(f));
        self
    }

    /// Builds the header row. `layer` receives the open sort menu.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = crate::ui::kit::kit(dark);
        let c = &k.c;
        let TableHeader {
            key,
            columns,
            sorts,
            column_widths,
            frozen_columns,
            show_row_numbers,
            sortable,
            grid_semantics,
            reorder_drag,
            reorder_drop,
            sort_menu,
            on_sort,
            on_remove_sort,
            on_filter_column,
            on_resize_start,
            on_resize_move,
            on_resize_end,
            on_reorder_start,
            on_reorder_over,
            on_reorder_drop,
            on_sort_menu_toggle,
            on_sort_menu_close,
        } = self;
        let width_of = |index: usize| {
            column_widths
                .as_ref()
                .and_then(|widths| widths.get(index).copied())
                .unwrap_or(DEFAULT_COLUMN_WIDTH)
        };
        let widths: Vec<f32> = (0..columns.len()).map(width_of).collect();
        // Where each column starts inside the header, gutter included: the reorder
        // hit test reads the pointer position against these offsets.
        let column_lefts: Vec<f32> = {
            let mut lefts = Vec::with_capacity(widths.len());
            let mut left = if show_row_numbers { GUTTER_WIDTH } else { 0.0 };
            for width in &widths {
                lefts.push(left);
                left += width;
            }
            lefts
        };
        let reorder_lefts = Rc::new(column_lefts.clone());
        let reorder_widths = Rc::new(widths.clone());
        // Frozen columns stack from the gutter: each keeps the left edge of the
        // columns before it, so a scroll slides content under the pinned block.
        let mut frozen_lefts = Vec::with_capacity(columns.len());
        let mut left = if show_row_numbers { GUTTER_WIDTH } else { 0.0 };
        for index in 0..columns.len() {
            frozen_lefts.push(left);
            if index < frozen_columns {
                left += width_of(index);
            }
        }
        let reorderable = on_reorder_drop.is_some() && columns.len() > 1;

        // React keeps the header pinned (`sticky top-0`) inside the grid's scroll
        // box; the fill keeps the rows from showing through.
        let mut header = row()
            .key(key.clone())
            .w_full()
            .h(HEADER_HEIGHT)
            .sticky_top(0.0)
            .fill(c.layer_01);
        if grid_semantics {
            header = header.role("row");
        }
        if show_row_numbers {
            let mut gutter = row()
                .key(format!("{key}-gutter"))
                .w(GUTTER_WIDTH)
                .h_full()
                .items_center()
                .justify_center()
                .border_sides([0.0, 1.0, 0.0, 0.0], c.border_weak)
                .child(
                    text("#")
                        .key(format!("{key}-gutter-label"))
                        .font(k.ty.text_xsmall)
                        .color(c.text_muted)
                        .align(Align::Center),
                );
            if grid_semantics {
                gutter = gutter.role("columnheader");
            }
            if frozen_columns > 0 {
                gutter = gutter.sticky_left(0.0).fill(c.layer_01);
            }
            header = header.child(gutter);
        }

        for (col_index, column) in columns.iter().enumerate() {
            let sorted = sorts
                .iter()
                .find(|sort| sort.column == column.name)
                .map(|sort| {
                    if sort.descending {
                        Sorted::Desc
                    } else {
                        Sorted::Asc
                    }
                })
                .unwrap_or(Sorted::None);
            let sort_position = sorts.iter().position(|sort| sort.column == column.name);
            let frozen = col_index < frozen_columns;
            let last_frozen = frozen && col_index + 1 == frozen_columns;
            let is_drop_marker = reorder_drop == Some(col_index)
                && reorder_drag.is_some_and(|drag| drag != col_index);

            // The cell's right edge is the normal rule, a stronger accent edge at the
            // end of the frozen block. React's reorder marker is `border-l-2`, so the
            // left edge is a pinned strip and both edges keep their own colour.
            let (right_border, right_color) = if last_frozen {
                (2.0, k.alpha(c.bg_accent, 0.4))
            } else {
                (1.0, c.border_weak)
            };
            let mut cell = row()
                .key(format!("{key}-col-{col_index}"))
                .w(width_of(col_index))
                .h_full()
                .px(CELL_PAD)
                .gap(CELL_GAP)
                .items_center()
                .justify_between()
                .border_sides([0.0, right_border, 0.0, 0.0], right_color);
            if grid_semantics {
                cell = cell.role("columnheader").label(column.name.clone());
                if sorted != Sorted::None {
                    cell = cell.sorted(Some(sorted == Sorted::Asc));
                }
            }
            if frozen {
                cell = cell.sticky_left(frozen_lefts[col_index]).fill(c.layer_01);
            }
            if is_drop_marker {
                cell = cell.child(
                    el().key(format!("{key}-drop-marker-{col_index}"))
                        .pin(Some(0.0), Some(0.0), None, Some(0.0))
                        .w(2.0)
                        .fill(c.bg_accent)
                        .a11y_hidden(),
                );
            }

            // A press cycles none → ascending → descending → (remove sort | ascending).
            let press_sort = on_sort.clone();
            let press_remove = on_remove_sort.clone();
            let press_name = column.name.clone();
            let can_remove = on_remove_sort.is_some() && sorted != Sorted::None;
            cell = cell.on_press(move |state: &mut S| {
                if !sortable {
                    return;
                }
                match sorted {
                    Sorted::None => {
                        if let Some(sort) = &press_sort {
                            sort(state, &press_name, false);
                        }
                    }
                    Sorted::Asc => {
                        if let Some(sort) = &press_sort {
                            sort(state, &press_name, true);
                        }
                    }
                    Sorted::Desc => match (&press_remove, &press_sort) {
                        (Some(remove), _) => remove(state, &press_name),
                        (None, Some(sort)) => sort(state, &press_name, false),
                        _ => {}
                    },
                }
            });
            if reorderable {
                let start = on_reorder_start.clone();
                let over = on_reorder_over.clone();
                let drop = on_reorder_drop.clone();
                let lefts = Rc::clone(&reorder_lefts);
                let widths_at = Rc::clone(&reorder_widths);
                cell = cell.on_drag(move |state: &mut S, drag: &mut rune::Drag, _tx| {
                    if drag.cancelled() {
                        if let Some(drop) = &drop {
                            drop(state);
                        }
                        return;
                    }
                    match drag.phase {
                        rune::Phase::Down => {
                            if let Some(start) = &start {
                                start(state, col_index);
                            }
                        }
                        rune::Phase::Move => {
                            if let Some(over) = &over {
                                // Rune hands the drag only to the pressed cell, so the
                                // pointer is hit-tested against the column offsets. The
                                // header's window x follows from the pressed cell: the
                                // pointer's window x, less its local x, less the cell's
                                // own offset.
                                let window_x = drag.window_point()[0];
                                let header_left = window_x - drag.x - lefts[col_index];
                                let pointer = window_x - header_left;
                                let mut target = lefts.len().saturating_sub(1);
                                for (index, left) in lefts.iter().enumerate() {
                                    if pointer < left + widths_at[index] {
                                        target = index;
                                        break;
                                    }
                                }
                                over(state, target);
                            }
                        }
                        rune::Phase::Up => {
                            if let Some(drop) = &drop {
                                drop(state);
                            }
                        }
                    }
                });
            }

            // Name over type, truncated together.
            let mut label = rune::col()
                .key(format!("{key}-label-{col_index}"))
                .min_w(0.0)
                .grow(1.0)
                .child(
                    text(column.name.clone())
                        .key(format!("{key}-name-{col_index}"))
                        .font(k.ty.text_xsmall_medium)
                        .color(c.text_muted)
                        .truncate()
                        .help(column.name.clone()),
                );
            if let Some(ty) = &column.ty {
                label = label.child(
                    text(ty.clone())
                        .key(format!("{key}-type-{col_index}"))
                        .font(k.ty.text_xsmall.mono())
                        .color(c.text_muted)
                        .truncate()
                        .help(ty.clone()),
                );
            }
            cell = cell.child(label);

            if let Some(filter) = &on_filter_column {
                let filter = Rc::clone(filter);
                let filter_name = column.name.clone();
                let button = IconButton::new(&ICON_FILTER)
                    .variant(IconButtonVariant::Ghost)
                    .control_size(ControlSize::Large)
                    .label(t("filter.filter"))
                    .help(t("filter.filter"))
                    .on_press(move |state: &mut S| filter(state, &filter_name))
                    .build(k)
                    .key(format!("{key}-filter-{col_index}"));
                cell = cell.child(button);
            }

            if sortable && sorted != Sorted::None {
                // Only with several sorts does the position matter.
                if sorts.len() > 1 {
                    if let Some(position) = sort_position {
                        // React's `title="#1"`, read out by assistive tech too.
                        // There is no `dataTable.sortPosition` key; the rank is the
                        // whole label, as in React.
                        let rank = format!("#{}", position + 1);
                        cell = cell.child(
                            text((position + 1).to_string())
                                .key(format!("{key}-sort-position-{col_index}"))
                                .font(k.ty.text_xsmall_medium)
                                .color(c.text_accent)
                                .size(16.0, 16.0)
                                .align(Align::Center)
                                .fill(k.alpha(c.bg_accent, 0.15))
                                .radius(k.radius.sm)
                                .label(rank.clone())
                                .help(rank),
                        );
                    }
                }
                let glyph = if sorted == Sorted::Asc {
                    &ICON_ARROW_UP
                } else {
                    &ICON_ARROW_DOWN
                };
                let direction = if sorted == Sorted::Asc {
                    t("dataTable.sortAscending")
                } else {
                    t("dataTable.sortDescending")
                };
                cell = cell.child(
                    icon(glyph, SORT_GLYPH)
                        .key(format!("{key}-sort-direction-{col_index}"))
                        .color(c.text_accent)
                        .label(direction),
                );
            }

            if sortable {
                let menu_open = sort_menu == Some(col_index);
                let toggle = on_sort_menu_toggle.clone();
                let mut chevron = IconButton::new(&ICON_CHEVRON_DOWN)
                    .variant(IconButtonVariant::Ghost)
                    .control_size(ControlSize::Regular)
                    .label(t("dataTable.sortMenuLabel"))
                    .help(t("dataTable.sortMenuLabel"))
                    // No ancestor-hover styling in Rune: keep it dim but reachable.
                    .build(k)
                    .key(format!("{key}-sort-{col_index}"))
                    .opacity(if menu_open { 1.0 } else { 0.55 });
                if let Some(toggle) = &toggle {
                    let toggle = Rc::clone(toggle);
                    chevron = chevron.on_press(move |state: &mut S| toggle(state, col_index));
                }
                let mut menu = Menu::new(format!("{key}-sort-menu-{col_index}"))
                    .trigger(chevron)
                    .open(menu_open)
                    .placement(Placement::BottomStart)
                    .close_on_select(true);
                let ascending = on_sort.clone();
                let ascending_name = column.name.clone();
                menu = menu.item(Item::new(t("dataTable.sortColumnAscending")).on_select(
                    move |state: &mut S| {
                        if let Some(sort) = &ascending {
                            sort(state, &ascending_name, false);
                        }
                    },
                ));
                let descending = on_sort.clone();
                let descending_name = column.name.clone();
                menu = menu.item(Item::new(t("dataTable.sortColumnDescending")).on_select(
                    move |state: &mut S| {
                        if let Some(sort) = &descending {
                            sort(state, &descending_name, true);
                        }
                    },
                ));
                let remove = on_remove_sort.clone();
                let remove_name = column.name.clone();
                menu = menu.item(
                    Item::new(t("dataTable.removeSort"))
                        .disabled(!can_remove)
                        .on_select(move |state: &mut S| {
                            if let Some(remove) = &remove {
                                remove(state, &remove_name);
                            }
                        }),
                );
                if let Some(close) = &on_sort_menu_close {
                    let close = Rc::clone(close);
                    menu = menu.on_close(move |state: &mut S| close(state));
                }
                cell = cell.child(menu.build(k, layer));
            }

            if let (Some(start), true) = (&on_resize_start, col_index < columns.len()) {
                let start = Rc::clone(start);
                let move_handler = on_resize_move.clone();
                let end_handler = on_resize_end.clone();
                let grip = el()
                    .key(format!("{key}-resize-{col_index}"))
                    .pin(None, Some(0.0), Some(0.0), None)
                    .w(GRIP_WIDTH)
                    .h_full()
                    .cursor(Cursor::ResizeColumn)
                    .hover(k.alpha(c.bg_accent, 0.4))
                    .on_drag(move |state: &mut S, drag: &mut rune::Drag, _tx| {
                        if drag.cancelled() {
                            if let Some(end) = &end_handler {
                                end(state);
                            }
                            return;
                        }
                        match drag.phase {
                            rune::Phase::Down => start(state, col_index, drag.window_point()[0]),
                            rune::Phase::Move => {
                                if let Some(move_handler) = &move_handler {
                                    move_handler(state, col_index, drag.dx);
                                }
                            }
                            rune::Phase::Up => {
                                if let Some(end) = &end_handler {
                                    end(state);
                                }
                            }
                        }
                    });
                cell = cell.child(grip);
            }
            header = header.child(cell);
        }

        header
    }
}
