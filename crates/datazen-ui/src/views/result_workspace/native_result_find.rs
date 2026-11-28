//! Local Find over already retained cells of the active Native result tab.
//! The separate full result-find port includes filters and write-back that
//! this workspace does not support; do not advertise those controls here.

use datazen_ui_api::TabKind;
use rune::{icon, row, text, El, Key, KeyEvent, TextEdit, Tx};

use crate::data_grid::DataGrid;
use crate::i18n::t;
use crate::icons::{ICON_SEARCH, ICON_X};
use crate::state_def::AppState;
use crate::types::{ResultView, Section};
use crate::ui::{field::TextField, kit::kit};

use super::result_find_bar::{CLOSE_KEY, COUNT_KEY};

const INPUT: &str = "result-find-input";
const TOGGLE: &str = "result-find-toggle";
const APPLY: &str = "result-find-apply";
const PREV: &str = "result-find-prev";
const NEXT: &str = "result-find-next";

impl AppState {
    fn native_result_find_scope(&self) -> Option<(u32, u32)> {
        if self.active != Section::Workspace || self.effective_result_view() != ResultView::Table {
            return None;
        }
        let query = self.tabs.selected().filter(|tab| tab.kind == TabKind::Query)?;
        let result = self.selected_query_result?;
        let tab = self.query_result_tabs.iter().find(|tab| tab.id == result)?;
        self.result_grid
            .as_ref()
            .filter(|grid| !grid.is_editable() && tab.error_key.is_none())
            .map(|_| (query.id, result))
    }

    pub(crate) fn native_result_find_open(&self) -> bool {
        self.result_find_owner.is_some() && self.native_result_find_scope() == self.result_find_owner
    }

    pub(crate) fn reset_native_result_find(&mut self) {
        self.result_find_owner = None;
        self.result_find = TextEdit::new(String::new());
        if let Some(grid) = self.result_grid.as_mut() {
            if !grid.find_needle().is_empty() {
                grid.set_find_needle("");
            }
        }
    }
}

fn open(state: &mut AppState, tx: &mut Tx) {
    let Some(scope) = state.native_result_find_scope() else { return };
    state.reset_native_result_find();
    state.result_find_owner = Some(scope);
    state.result_focused = false;
    state.sql.completion_close();
    state.sql.focused = false;
    tx.instant();
    tx.focus(INPUT, tx.by_keys());
}

fn close(state: &mut AppState, tx: &mut Tx) {
    if !state.native_result_find_open() {
        return;
    }
    state.reset_native_result_find();
    state.result_focused = false;
    state.sql.focus();
    tx.instant();
    tx.focus("sql-editor-scroll", tx.by_keys());
}

fn reveal(grid: &DataGrid, tx: &mut Tx) {
    if let Some((row, column)) = grid.current_find_hit() {
        // Rune resolves the keyed virtual row before its cell, including a
        // hit outside the currently mounted result window.
        tx.scroll_into_view_in(
            "result-data-grid-scroll",
            format!("result-grid-row-{row}/result-cell-{row}-{column}"),
        );
    }
}

fn apply(state: &mut AppState, tx: &mut Tx) {
    if !state.native_result_find_open() {
        return;
    }
    state.apply_result_find();
    if let Some(grid) = state.result_grid.as_ref() {
        reveal(grid, tx);
    }
}

fn navigate(state: &mut AppState, tx: &mut Tx, forward: bool) {
    if !state.native_result_find_open() {
        return;
    }
    if forward {
        state.result_find_next();
    } else {
        state.result_find_prev();
    }
    if let Some(grid) = state.result_grid.as_ref() {
        reveal(grid, tx);
    }
    state.result_focused = false;
    tx.focus(INPUT, tx.by_keys());
}

pub(crate) fn escape_if_focused(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if ev.key == Key::Escape
        && state.native_result_find_open()
        && matches!(tx.focused(), Some(INPUT | TOGGLE | APPLY | PREV | NEXT | CLOSE_KEY))
    {
        close(state, tx);
        true
    } else {
        false
    }
}

pub(crate) fn toggle(state: &AppState, dark: bool) -> Option<El> {
    state.native_result_find_scope()?;
    let k = kit(dark);
    Some(row()
        .key(TOGGLE)
        .h(24.0)
        .px(8.0)
        .gap(4.0)
        .center_y()
        .radius(6.0)
        .role("button")
        .label(t("resultFind.open"))
        .focusable(true)
        .expanded(state.native_result_find_open())
        .child(icon(&ICON_SEARCH, 14.0).color(k.c.text_muted))
        .child(text(t("resultFind.openLabel")).font(k.ty.text_xsmall).color(k.c.text_base))
        .on_press_tx(|state: &mut AppState, tx| {
            if state.native_result_find_open() { close(state, tx) } else { open(state, tx) }
        }))
}

fn navigation_button(key: &str, label: &str, help: &str, forward: bool, enabled: bool, dark: bool) -> El {
    let k = kit(dark);
    let button = row()
        .key(key)
        .h(24.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .role("button")
        .label(help)
        .focusable(enabled)
        .disabled(!enabled)
        .child(text(label).font(k.ty.text_xsmall).color(if enabled { k.c.text_base } else { k.c.text_muted }));
    if enabled {
        button.on_press_tx(move |s: &mut AppState, tx| navigate(s, tx, forward))
    } else {
        button
    }
}

pub(crate) fn panel(state: &AppState, grid: &DataGrid, dark: bool) -> Option<El> {
    if !state.native_result_find_open() {
        return None;
    }
    let k = kit(dark);
    let count = grid.find_hit_count();
    let count_label = if grid.find_needle().trim().is_empty() {
        String::new()
    } else if count == 0 {
        t("select.noMatches").to_owned()
    } else {
        format!("{} of {count}", grid.find_current_index())
    };
    Some(row()
        .key("result-find-bar")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(icon(&ICON_SEARCH, 14.0).color(k.c.text_muted))
        .child(TextField::new(INPUT, &state.result_find)
            .height(24.0)
            .width(176.0)
            .label(t("resultFind.open"))
            .placeholder(t("resultFind.placeholder"))
            .on_edit(|s: &mut AppState| &mut s.result_find)
            .on_change(apply)
            .on_key(|s: &mut AppState, ev, tx| match ev.key {
                Key::Escape => { close(s, tx); true }
                Key::Enter if !ev.mods.cmd && !ev.mods.ctrl && !ev.mods.alt && !ev.mods.logo => {
                    navigate(s, tx, !ev.mods.shift);
                    true
                }
                _ => false,
            })
            .build(dark))
        .child(row()
            .key(APPLY)
            .h(24.0)
            .px(8.0)
            .center_y()
            .radius(6.0)
            .role("button")
            .focusable(true)
            .child(text(t("resultFind.openLabel")).font(k.ty.text_xsmall).color(k.c.text_base))
            .on_press_tx(apply))
        .child(navigation_button(PREV, "↑", t("query.nativeFind.previous"), false, count != 0, dark))
        .child(navigation_button(NEXT, "↓", t("query.nativeFind.next"), true, count != 0, dark))
        .child(text(count_label)
            .key(COUNT_KEY)
            .role("status")
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted))
        .child(row()
            .key(CLOSE_KEY)
            .size(24.0, 24.0)
            .center()
            .role("button")
            .label(t("resultFind.close"))
            .focusable(true)
            .child(icon(&ICON_X, 14.0).color(k.c.text_muted))
            .on_press_tx(close)))
}
