//! Per-tab, fetch-free Field Filter for the real SQLite Table Data panel.
use std::{collections::BTreeMap, rc::Rc};

use rune::{Align, El, Side, TextEdit, Tx, col, text};
use rune_kit::{Button, ButtonVariant};

use crate::{
    AppState,
    i18n::t,
    ui::kit::kit,
    views::table_column_filter::{self as ui, FilterColumn, TableColumnFilterAction as Action},
};

use super::freeze::Owner;

#[derive(Clone, Copy, PartialEq, Eq)]
struct Lease {
    tab: u32,
    session: u32,
    read: u32,
}

impl Lease {
    fn current(state: &AppState) -> Option<Self> {
        let owner = Owner::current(state)?;
        if state.connect_loading.is_some() || state.table_loading.is_some()
            || state.commit_loading.is_some() || state.grid.is_editing()
            || state.active_cell_preview().is_some() || state.grid.cols == 0
        {
            return None;
        }
        Some(Self { tab: owner.tab, session: owner.session, read: state.read_generation })
    }
}

struct Choice {
    session: u32,
    connection: String,
    table: String,
    visible: Vec<String>,
}

#[derive(Clone, Copy)]
struct Popup {
    lease: Lease,
    nonce: u64,
}

pub(crate) struct TableFieldFilter {
    tabs: BTreeMap<u32, Choice>,
    popup: Option<Popup>,
    nonce: u64,
    search: TextEdit,
}

impl Default for TableFieldFilter {
    fn default() -> Self {
        Self {
            tabs: BTreeMap::new(), popup: None, nonce: 0,
            search: TextEdit::new(String::new()),
        }
    }
}

impl TableFieldFilter {
    pub(crate) fn dismiss(&mut self) {
        if self.popup.take().is_some() {
            self.search = TextEdit::new(String::new());
        }
    }

    pub(crate) fn drop_tab(&mut self, id: u32) {
        self.tabs.remove(&id);
        if self.popup.is_some_and(|popup| popup.lease.tab == id) {
            self.dismiss();
        }
    }

    pub(crate) fn clear_session(&mut self) {
        self.tabs.clear();
        self.dismiss();
    }
}

fn choice<'a>(state: &'a AppState, owner: &Owner<'_>) -> Option<&'a Choice> {
    state.table_field_filter.tabs.get(&owner.tab).filter(|choice| {
        choice.session == owner.session && choice.connection == owner.connection
            && choice.table == owner.table
    })
}

pub(crate) fn restore_after_read(state: &mut AppState) {
    let names = Owner::loaded(state)
        .and_then(|owner| choice(state, &owner))
        .map(|choice| choice.visible.clone());
    state.grid.set_displayed_columns(names.as_deref());
}

fn change(state: &mut AppState, lease: Lease, names: Option<Vec<String>>) {
    if Lease::current(state) != Some(lease) {
        return;
    }
    let Some(owner) = Owner::current(state) else { return; };
    let tab = owner.tab;
    if let Some(visible) = names {
        let choice = Choice {
            session: owner.session,
            connection: owner.connection.to_owned(),
            table: owner.table.to_owned(),
            visible,
        };
        state.table_field_filter.tabs.insert(tab, choice);
    } else {
        state.table_field_filter.tabs.remove(&tab);
    }
    state.table_freeze.dismiss();
    restore_after_read(state);
}

fn current_popup(state: &AppState) -> Option<Popup> {
    let popup = state.table_field_filter.popup?;
    (Lease::current(state) == Some(popup.lease)).then_some(popup)
}

pub(crate) fn blocks_grid_keys(state: &AppState) -> bool {
    current_popup(state).is_some()
}

pub(crate) fn retire_stale_popup(state: &mut AppState) -> bool {
    if state.table_field_filter.popup.is_some() && current_popup(state).is_none() {
        state.table_field_filter.dismiss();
        return true;
    }
    false
}

fn toggle(state: &mut AppState, lease: Lease, tx: &mut Tx) {
    if Lease::current(state) != Some(lease) {
        return;
    }
    if current_popup(state).is_some() {
        state.table_field_filter.dismiss();
    } else {
        state.table_freeze.dismiss();
        let filter = &mut state.table_field_filter;
        filter.nonce = filter.nonce.wrapping_add(1);
        filter.popup = Some(Popup { lease, nonce: filter.nonce });
        tx.instant();
        tx.focus(ui::SEARCH_KEY, tx.by_keys());
    }
}

fn on_action(state: &mut AppState, nonce: u64, action: Action) {
    let Some(popup) = current_popup(state).filter(|popup| popup.nonce == nonce) else { return; };
    match action {
        Action::ToggleColumn { name, .. } => {
            let Some(raw) = state.grid.column_index(&name) else { return; };
            let mut displayed = (0..state.grid.displayed_count())
                .filter_map(|at| state.grid.displayed_raw(at)).collect::<Vec<_>>();
            if let Some(index) = displayed.iter().position(|&current| current == raw) {
                displayed.remove(index);
            } else {
                displayed.push(raw);
            }
            let visible = (displayed.len() != state.grid.cols as usize).then(|| {
                displayed.into_iter()
                    .filter_map(|raw| state.grid.column_name(raw).map(str::to_owned)).collect()
            });
            change(state, popup.lease, visible);
        }
        Action::SelectAll | Action::Reset => change(state, popup.lease, None),
        Action::ClearAll => change(state, popup.lease, Some(Vec::new())),
        Action::SetSearch(label) => {
            state.table_field_filter.search = TextEdit::new(label);
        }
        Action::Close => state.table_field_filter.dismiss(),
        Action::ToggleOpen => {}
    }
}

fn vm(state: &AppState) -> ui::TableColumnFilterVm {
    let grid = &state.grid;
    let columns = (0..grid.cols).filter_map(|raw| {
        let name = grid.column_name(raw)?;
        Some(FilterColumn {
            name: name.to_owned(),
            data_type: grid.column_type(raw).unwrap_or_default().to_owned(),
            is_primary_key: grid.is_primary_key(name),
        })
    }).collect();
    let visible_columns = grid.is_projected().then(|| {
        (0..grid.displayed_count()).filter_map(|at| {
            grid.displayed_raw(at).and_then(|raw| grid.column_name(raw)).map(str::to_owned)
        }).collect()
    });
    ui::TableColumnFilterVm {
        columns, visible_columns,
        disabled: Lease::current(state).is_none(),
        open: current_popup(state).is_some(),
        search_query: state.table_field_filter.search.clone(),
        loading: false,
    }
}

/// Return the actual inline trigger, and mount the anchored popup in the
/// window layer (outside the grid's clipped scroll container).
pub(crate) fn mount(state: &AppState, dark: bool, layer: &mut Vec<El>) -> Option<El> {
    Owner::current(state)?;
    let vm = vm(state);
    let mut trigger = ui::trigger(&vm, dark);
    if let Some(lease) = Lease::current(state) {
        trigger = trigger.on_press_tx(move |state, tx| toggle(state, lease, tx));
    }
    if let Some(popup) = current_popup(state) {
        let nonce = popup.nonce;
        let callback: ui::TableColumnFilterActionFn<AppState> =
            Rc::new(move |state, action| on_action(state, nonce, action));
        let edit: ui::SearchEditFn<AppState> =
            Rc::new(|state| &mut state.table_field_filter.search);
        layer.push(ui::popover(&vm, &Some(callback), Some(edit), dark)
            .anchored_fit(ui::TOGGLE_KEY, Side::Bottom, Align::End, 4.0)
            .focus_return(ui::TOGGLE_KEY)
            .modal_tx(true, move |state: &mut AppState, tx| {
                if current_popup(state).is_some_and(|popup| popup.nonce == nonce) {
                    state.table_field_filter.dismiss();
                    tx.return_focus(ui::TOGGLE_KEY, tx.by_keys());
                }
            }));
    }
    Some(trigger)
}

pub(super) fn empty(state: &AppState, dark: bool) -> El {
    let k = kit(dark);
    let lease = Lease::current(state);
    let mut reset = Button::new(t("tableData.resetColumns").to_owned())
        .variant(ButtonVariant::Ghost)
        .build(k)
        .key("table-reset-hidden-columns")
        .disabled(lease.is_none());
    if let Some(lease) = lease {
        reset = reset.on_press(move |state: &mut AppState| change(state, lease, None));
    }
    col().key("all-columns-hidden-state").w_full().grow(1.0).min_h(0.0)
        .role("status").center().gap(8.0)
        .child(text(t("tableData.allColumnsHidden")).font(k.ty.text_small).color(k.c.text_muted))
        .child(reset)
}
