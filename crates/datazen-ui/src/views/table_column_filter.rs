//! Port of `windows/connection/TableColumnFilter.tsx`.
//!
//! React is a column-visibility popover on the table toolbar: a toggle button
//! (with a `n/total` badge when filtered), a popover with select-all / clear-all
//! / reset, a search field filtering by name or data type, and a checkbox list
//! of columns with PK badges. Escape and an outside press close the popover,
//! and opening it focuses the search field. This port keeps that contract;
//! every change is emitted as a [`TableColumnFilterAction`] through an injected
//! host port — no fake IPC, clipboard or timer.
//!
//! ## Keys
//!
//! React `data-testid`s are kept verbatim: `table-column-filter-toggle`,
//! `column-filter-count-badge`, `table-column-filter-popover`,
//! `table-column-filter-select-all`, `table-column-filter-clear-all`,
//! `table-column-filter-reset`, `table-column-filter-search`,
//! `column-filter-item-{name}` and `column-filter-checkbox-{name}`.
//!
//! ## Local stand-ins (removal stage)
//!
//! The VM mirrors the props plus the two local `useState` values (`open`,
//! `searchQuery`); `loading` is a documented stand-in for a host-driven busy
//! state React does not render. The live host mounts the trigger in the grid
//! bar and anchors the shared body in the window overlay.

use std::rc::Rc;

use rune::{col, icon, row, text, El, Key, KeyEvent, TextEdit, Tx};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{ICON_REFRESH, ICON_SEARCH, ICON_TABLE, ICON_X};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

pub const TOGGLE_KEY: &str = "table-column-filter-toggle";
pub const BADGE_KEY: &str = "column-filter-count-badge";
pub const POPOVER_KEY: &str = "table-column-filter-popover";
pub const SELECT_ALL_KEY: &str = "table-column-filter-select-all";
pub const CLEAR_ALL_KEY: &str = "table-column-filter-clear-all";
pub const RESET_KEY: &str = "table-column-filter-reset";
pub const SEARCH_KEY: &str = "table-column-filter-search";

/// React `ColumnSchema` fields the filter reads.
#[allow(dead_code)] // VM mirror; remove when the host owns the columns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FilterColumn {
    pub name: String,
    pub data_type: String,
    pub is_primary_key: bool,
}

/// Props plus the two local `useState` values.
#[allow(dead_code)] // VM surface; remove when the host wires the filter.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableColumnFilterVm {
    pub columns: Vec<FilterColumn>,
    /// `null` means "all visible" (React's `visibleColumns`).
    pub visible_columns: Option<Vec<String>>,
    pub disabled: bool,
    pub open: bool,
    pub search_query: TextEdit,
    /// Stand-in: a host-driven busy state (React renders no such state).
    pub loading: bool,
}

#[allow(dead_code)] // pure helpers; remove when the host wires the filter.
impl TableColumnFilterVm {
    pub fn all_column_names(&self) -> Vec<String> {
        self.columns.iter().map(|column| column.name.clone()).collect()
    }

    /// React `activeVisible = visibleColumns ?? allColumnNames`.
    pub fn active_visible(&self) -> Vec<String> {
        self.visible_columns.clone().unwrap_or_else(|| self.all_column_names())
    }

    /// React `visibleColumns !== null && visibleColumns.length < columns.length`.
    pub fn is_filtered(&self) -> bool {
        self.visible_columns
            .as_ref()
            .is_some_and(|visible| visible.len() < self.columns.len())
    }

    pub fn visible_count(&self) -> usize {
        self.active_visible().len()
    }

    pub fn count_text(&self) -> String {
        format!("{}/{}", self.visible_count(), self.columns.len())
    }

    /// React `columns.filter(name/dataType includes the trimmed, lowered query)`.
    pub fn filtered_columns(&self) -> Vec<&FilterColumn> {
        let query = self.search_query.text.trim().to_lowercase();
        self.columns
            .iter()
            .filter(|column| {
                query.is_empty()
                    || column.name.to_lowercase().contains(&query)
                    || column.data_type.to_lowercase().contains(&query)
            })
            .collect()
    }

    /// React `handleToggleColumn`: remove a visible column; add a hidden one,
    /// collapsing to `None` (all) when the last hidden column is re-added.
    pub fn toggle_column(&self, name: &str) -> Option<Vec<String>> {
        let active = self.active_visible();
        if active.iter().any(|column| column == name) {
            Some(active.into_iter().filter(|column| column != name).collect())
        } else {
            let mut next = active;
            next.push(name.to_owned());
            if next.len() == self.columns.len() {
                None
            } else {
                Some(next)
            }
        }
    }

    /// React's select-all / reset both pass `null`.
    pub fn select_all(&self) -> Option<Vec<String>> {
        None
    }

    /// React's clear-all passes `[]`.
    pub fn clear_all(&self) -> Option<Vec<String>> {
        Some(Vec::new())
    }
}

/// The interactions the filter emits. `Option<Vec<String>>` is the same
/// contract as React's `onChange`: `None` = all columns, empty = none.
#[allow(dead_code)] // host port; remove when the toolbar host wires the filter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TableColumnFilterAction {
    ToggleOpen,
    ToggleColumn { name: String, next: Option<Vec<String>> },
    SelectAll,
    ClearAll,
    Reset,
    SetSearch(String),
    Close,
}

#[allow(dead_code)] // host port; remove when the toolbar host wires the filter.
pub type TableColumnFilterActionFn<S> = Rc<dyn Fn(&mut S, TableColumnFilterAction) + 'static>;
#[allow(dead_code)] // text accessor; remove when the host owns the search field.
pub type SearchEditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

fn emit<S: 'static>(
    on_action: &Option<TableColumnFilterActionFn<S>>,
    action: TableColumnFilterAction,
) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Build the filter into `layer`.
#[allow(dead_code)] // view entry; remove when the host wires the filter.
pub fn build<S: 'static>(
    vm: &TableColumnFilterVm,
    on_action: &Option<TableColumnFilterActionFn<S>>,
    search_edit: Option<SearchEditFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let toggle_disabled = vm.disabled || vm.columns.is_empty();
    // React installs a document mousedown listener that closes the popover on
    // an outside press. A full-window transparent scrim behind the toggle and
    // popover carries that intent; the two controls stay above it and keep
    // their own presses. A disabled filter is inert, so it installs no scrim.
    if vm.open && !toggle_disabled {
        let mut scrim = row()
            .key("table-column-filter-dismiss")
            .absolute(0.0, 0.0)
            .w_full()
            .h_full()
            .a11y_hidden();
        let dismiss = emit(on_action, TableColumnFilterAction::Close);
        scrim = scrim.on_press(move |s: &mut S| dismiss(s));
        layer.push(scrim);
    }
    let mut toggle = trigger(vm, dark);
    if !toggle_disabled {
        toggle = toggle.on_press(emit(on_action, TableColumnFilterAction::ToggleOpen));
    }
    layer.push(toggle);

    if vm.open {
        let mut popover = popover(vm, on_action, search_edit, dark);
        // React's Escape handler closes the popover and returns focus to the
        // toggle; the popover carries the key handler when focused.
        let escape_callback = on_action.clone();
        popover = popover.focusable(true).on_key(move |s: &mut S, event: &KeyEvent, _tx: &mut Tx| {
            if event.key == Key::Escape {
                if let Some(callback) = &escape_callback {
                    callback(s, TableColumnFilterAction::Close);
                }
                true
            } else {
                false
            }
        });
        layer.push(popover);
    }
}

pub(crate) fn trigger(vm: &TableColumnFilterVm, dark: bool) -> El {
    let k = kit(dark);
    let toggle_disabled = vm.disabled || vm.columns.is_empty();
    let mut toggle = row()
        .key(TOGGLE_KEY)
        .role("button")
        .expanded(vm.open)
        .focusable(!toggle_disabled)
        .disabled(toggle_disabled)
        .h(24.0)
        .gap(4.0)
        .center_y()
        .px(6.0)
        .radius(4.0)
        .fill(if vm.is_filtered() {
            k.alpha(k.c.text_accent, 0.15)
        } else if vm.open {
            k.c.surface_raised
        } else {
            rune::Color::TRANSPARENT
        })
        .label(t("tableData.columnFilter"))
        .help(t("tableData.columnFilter"))
        .child(icon(&ICON_TABLE, 12.0).color(k.c.text_muted))
        .child(
            text(t("tableData.columnFilter"))
                .key("table-column-filter-label")
                .font(k.ty.text_small)
                .color(if vm.is_filtered() {
                    k.c.text_accent
                } else {
                    k.c.text_base
                }),
        );
    if vm.is_filtered() {
        toggle = toggle.child(
            text(vm.count_text())
                .key(BADGE_KEY)
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent),
        );
    }
    toggle
}

pub(crate) fn popover<S: 'static>(
    vm: &TableColumnFilterVm,
    on_action: &Option<TableColumnFilterActionFn<S>>,
    search_edit: Option<SearchEditFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let header = row()
        .key("table-column-filter-header")
        .w_full()
        .min_w(0.0)
        .center_y()
        .justify_between()
        .pb(8.0)
        .child(
            row()
                .gap(4.0)
                .center_y()
                .child(
                    text(t("tableData.visibleColumns"))
                        .font(k.ty.text_small)
                        .color(k.c.text_base),
                )
                .child(
                    text(vm.count_text())
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                ),
        )
        .child(
            row()
                .gap(4.0)
                .center_y()
                .child(action_button::<S>(
                    SELECT_ALL_KEY,
                    t("tableData.selectAllColumns"),
                    TableColumnFilterAction::SelectAll,
                    on_action,
                    dark,
                ))
                .child(action_button::<S>(
                    CLEAR_ALL_KEY,
                    t("tableData.deselectAllColumns"),
                    TableColumnFilterAction::ClearAll,
                    on_action,
                    dark,
                ))
                .child({
                    if vm.is_filtered() {
                        action_button::<S>(
                            RESET_KEY,
                            t("tableData.resetColumns"),
                            TableColumnFilterAction::Reset,
                            on_action,
                            dark,
                        )
                        .child(icon(&ICON_REFRESH, 10.0).color(k.c.text_muted))
                    } else {
                        row().key("table-column-filter-reset-absent").size(0.0, 0.0).a11y_hidden()
                    }
                }),
        );

    let mut field = TextField::new(SEARCH_KEY, &vm.search_query)
        .height(28.0)
        .grow()
        .pad_x(8.0)
        .text_style(k.ty.text_small)
        .placeholder(t("tableData.searchColumns"));
    if let Some(edit) = search_edit {
        field = field.on_edit(rune::edit_access(move |s: &mut S| edit(s)));
    } else {
        field = field.disabled(true);
    }
    let search = row()
        .key("table-column-filter-search-row")
        .w_full()
        .min_w(0.0)
        .gap(4.0)
        .center_y()
        .mt(8.0)
        .child(icon(&ICON_SEARCH, 14.0).color(k.c.text_muted))
        .child(field.build(dark))
        .child({
            if !vm.search_query.text.is_empty() {
                icon(&ICON_X, 12.0)
                    .key("table-column-filter-search-clear")
                    .role("button")
                    .label(t("common.close"))
                    .on_press(emit(on_action, TableColumnFilterAction::SetSearch(String::new())))
            } else {
                row().key("table-column-filter-search-clear-absent").size(0.0, 0.0).a11y_hidden()
            }
        });

    let mut list = col().key("table-column-filter-list").w_full().min_w(0.0).mt(8.0).gap(2.0);
    let filtered = vm.filtered_columns();
    if filtered.is_empty() {
        list = list.child(
            row().key("table-column-filter-no-columns").w_full().py(16.0).child(
                text(t("tableData.noColumnsFound"))
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            ),
        );
    }
    let active = vm.active_visible();
    for column in filtered {
        let checked = active.iter().any(|name| name == &column.name);
        let mut check = Checkbox::new(
            format!("column-filter-checkbox-{}", column.name),
            checked,
        ).label(column.name.clone());
        if let Some(callback) = on_action.clone() {
            let name = column.name.clone();
            let next = vm.toggle_column(&name);
            check = check.on_toggle(emit(
                &Some(callback),
                TableColumnFilterAction::ToggleColumn { name, next },
            ));
        }
        let mut item = row()
            .key(format!("column-filter-item-{}", column.name))
            .w_full()
            .min_w(0.0)
            .gap(8.0)
            .center_y()
            .px(8.0)
            .py(6.0)
            .radius(4.0)
            .hover(k.c.surface_raised)
            .child(check.build(k))
            .child(
                text(column.name.clone())
                    .key(format!("column-filter-name-{}", column.name))
                    .font(k.ty.text_small)
                    .color(if checked { k.c.text_base } else { k.c.text_muted })
                    .grow(1.0)
                    .min_w(0.0)
                    .truncate(),
            );
        if column.is_primary_key {
            item = item.child(
                text("PK")
                    .key(format!("column-filter-pk-{}", column.name))
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            );
        }
        item = item.child(
            text(column.data_type.clone())
                .key(format!("column-filter-type-{}", column.name))
                .font(k.ty.mono)
                .color(k.c.text_muted),
        );
        if on_action.is_some() {
            item = item.on_press(emit(on_action, TableColumnFilterAction::ToggleColumn {
                name: column.name.clone(),
                next: vm.toggle_column(&column.name),
            }));
        }
        list = list.child(item);
    }

    col()
        .key(POPOVER_KEY)
        .role("dialog")
        .w(288.0)
        .min_h(0.0)
        .padding(10.0)
        .radius(12.0)
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .child(header)
        .child(search)
        .child(list.max_h(240.0).scroll(rune::ScrollSpec { stick_bottom: false }))
}

fn action_button<S: 'static>(
    key: &str,
    label: &str,
    action: TableColumnFilterAction,
    on_action: &Option<TableColumnFilterActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut button = text(label.to_owned())
        .key(key.to_owned())
        .role("button")
        .focusable(true)
        .label(label.to_owned())
        .font(k.ty.text_small)
        .color(k.c.text_accent);
    let press = emit(on_action, action);
    button = button.on_press(move |s: &mut S| press(s));
    button
}
