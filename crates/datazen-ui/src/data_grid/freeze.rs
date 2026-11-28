//! Live SQLite Table Data column freezing. This is presentation state owned
//! by the opened table tab, not a setting or a driver operation.
use std::collections::BTreeMap;

use datazen_ui_api::TabKind;
use rune::{el, El, PointerPress};

use crate::{
    i18n::t,
    types::{Section, TableView},
    ui::{kit::kit, menu::{context_menu, Item}},
    AppState,
};

const MENU: &str = "table-column-freeze";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct HeaderLease {
    tab: u32,
    session: u32,
    read: u32,
    display: u64,
}

pub(super) struct Owner<'a> {
    pub(super) tab: u32,
    pub(super) session: u32,
    pub(super) connection: &'a str,
    pub(super) table: &'a str,
}

impl<'a> Owner<'a> {
    pub(super) fn current(state: &'a AppState) -> Option<Self> {
        (state.active == Section::Workspace && state.table_view == TableView::Data)
            .then(|| Self::loaded(state)).flatten()
    }

    // Reads may complete while the same tab shows its Structure page. The
    // scoped projection belongs to its data, not to the active sub-view.
    pub(super) fn loaded(state: &'a AppState) -> Option<Self> {
        if state.native_kv.is_some()
            || state.er_schema.is_some()
            || state.table_read_error.is_some()
            || !state.grid.has_page()
        {
            return None;
        }
        let tab = state.tabs.selected()?;
        if tab.kind != TabKind::Table || state.grid.table_label() != tab.title.as_str() {
            return None;
        }
        let connection = state.desktop_connection.as_deref()?;
        if !state.desktop_connections.as_ref()?.iter()
            .any(|profile| profile.id == connection && profile.engine == "sqlite")
        {
            return None;
        }
        // SQLite's ordinary table route acts on a bare native table name.
        // Do not infer identity for demos, views or ambiguous catalog entries.
        let matches = state.desktop_catalog.as_ref()?.databases.iter()
            .flat_map(|db| &db.schemas)
            .flat_map(|schema| &schema.tables)
            .filter(|table| table.as_str() == tab.title.as_str()).take(2).count();
        if matches != 1 {
            return None;
        }
        Some(Self {
            tab: tab.id,
            session: state.navigator_value_search_session_generation?,
            connection,
            table: &tab.title,
        })
    }
}

struct Prefix {
    session: u32,
    connection: String,
    table: String,
    // A raw prefix snapshot guards schema replacement. The count itself
    // always applies to the first N *displayed* columns as in Chromium.
    columns: Vec<String>,
    count: u32,
}

struct Menu {
    lease: HeaderLease,
    nonce: u64,
    column: u32,
    point: PointerPress,
}

#[derive(Default)]
pub(crate) struct TableFreeze {
    tabs: BTreeMap<u32, Prefix>,
    menu: Option<Menu>,
    nonce: u64,
}

impl TableFreeze {
    pub(crate) fn dismiss(&mut self) {
        self.menu = None;
    }

    pub(crate) fn drop_tab(&mut self, id: u32) {
        self.tabs.remove(&id);
        if self.menu.as_ref().is_some_and(|menu| menu.lease.tab == id) {
            self.dismiss();
        }
    }

    pub(crate) fn clear_session(&mut self) {
        self.tabs.clear();
        self.dismiss();
    }
}

// None means the untouched legacy sheet; Some(0) is the new unpinned default.
pub(super) fn frozen_columns(state: &AppState) -> Option<u32> {
    let owner = Owner::current(state)?;
    let prefix = state.table_freeze.tabs.get(&owner.tab).filter(|entry| {
        entry.session == owner.session && entry.connection == owner.connection
            && entry.table == owner.table
    });
    let count = prefix.map_or(0, |entry| {
        let matched = entry.columns.iter().enumerate().take_while(|(i, name)| {
            state.grid.column_name(*i as u32) == Some(name.as_str())
        }).count() as u32;
        if matched == entry.columns.len() as u32 {
            entry.count.min(state.grid.displayed_count())
        } else {
            // The schema, not just the presentation, changed underneath.
            // Do not pin an inserted/reordered identity by stale raw index.
            (0..state.grid.displayed_count())
                .take_while(|at| state.grid.displayed_raw(*at).is_some_and(|raw| raw < matched))
                .count().min(entry.count as usize) as u32
        }
    });
    Some(count)
}

pub(super) fn header_lease(state: &AppState) -> Option<HeaderLease> {
    if state.connect_loading.is_some() || state.table_loading.is_some()
        || state.commit_loading.is_some() || state.grid.is_editing()
        || state.active_cell_preview().is_some()
    {
        return None;
    }
    let owner = Owner::current(state)?;
    Some(HeaderLease {
        tab: owner.tab, session: owner.session, read: state.read_generation,
        display: state.grid.display_revision(),
    })
}

pub(super) fn open(state: &mut AppState, lease: HeaderLease, column: u32, point: PointerPress) {
    if header_lease(state) != Some(lease) || state.grid.displayed_raw(column).is_none() {
        return;
    }
    state.table_freeze.nonce = state.table_freeze.nonce.wrapping_add(1);
    state.table_freeze.menu = Some(Menu { lease, nonce: state.table_freeze.nonce, column, point });
}

fn current_menu(state: &AppState) -> Option<&Menu> {
    let menu = state.table_freeze.menu.as_ref()?;
    (header_lease(state) == Some(menu.lease)).then_some(menu)
}

pub(crate) fn blocks_grid_keys(state: &AppState) -> bool {
    current_menu(state).is_some()
}

pub(crate) fn retire_stale_menu(state: &mut AppState) -> bool {
    if state.table_freeze.menu.is_some() && current_menu(state).is_none() {
        state.table_freeze.dismiss();
        return true;
    }
    false
}

fn apply(state: &mut AppState, nonce: u64, freeze: bool) {
    let Some(menu) = current_menu(state).filter(|menu| menu.nonce == nonce) else { return; };
    let tab = menu.lease.tab;
    let next_count = if freeze { menu.column + 1 } else { 0 };
    let Some(current) = frozen_columns(state) else { return; };
    if next_count == current {
        return;
    }
    if next_count == 0 {
        state.table_freeze.tabs.remove(&tab);
    } else {
        let Some(owner) = Owner::current(state) else { return; };
        let Some(last_raw) = state.grid.displayed_raw(next_count - 1) else { return; };
        let Some(columns) = (0..=last_raw)
            .map(|column| state.grid.column_name(column).map(str::to_owned))
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let entry = Prefix {
            session: owner.session,
            connection: owner.connection.to_owned(),
            table: owner.table.to_owned(),
            columns,
            count: next_count,
        };
        state.table_freeze.tabs.insert(tab, entry);
    }
}

pub(crate) fn mount_menu(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    let Some(menu) = current_menu(state) else { return; };
    let Some(count) = frozen_columns(state) else { return; };
    let nonce = menu.nonce;
    // Use the real window overlay/menu wrapper: outside click, arrow keys,
    // Escape, flipping at the edge and disabled item semantics are shared.
    let trigger = el().size(0.0, 0.0).a11y_hidden();
    let _ = context_menu::<AppState>(MENU, trigger, Some(menu.point))
        .min_width(224.0)
        .item(Item::new(t("dataTable.freezeThrough")).disabled(count == menu.column + 1)
            .on_select(move |state| apply(state, nonce, true)))
        .item(Item::new(t("dataTable.unfreezeColumns")).disabled(count == 0)
            .on_select(move |state| apply(state, nonce, false)))
        .on_close(move |state| {
            if state.table_freeze.menu.as_ref().is_some_and(|menu| menu.nonce == nonce) {
                state.table_freeze.dismiss();
            }
        })
        .build(kit(dark), layer);
}

#[cfg(test)]
mod tests {
    use datazen_ui_api::{Catalog, ConnectionProfile, DatabaseNode, SchemaNode, TablePage};

    use super::*;

    fn customer_table() -> AppState {
        let mut state = AppState::default();
        state.active = Section::Workspace;
        state.desktop_connection = Some("profile".into());
        state.navigator_value_search_session_generation = Some(10);
        state.desktop_connections = Some(vec![ConnectionProfile {
            id: "profile".into(),
            name: "Fixture".into(),
            engine: "sqlite".into(),
            host: None,
        }]);
        state.desktop_catalog = Some(Catalog {
            databases: vec![DatabaseNode {
                name: "fixture".into(),
                schemas: vec![SchemaNode {
                    name: "main".into(),
                    tables: vec!["customers".into()],
                    views: Vec::new(),
                }],
            }],
        });
        state.tabs.open(TabKind::Table, "customers").unwrap();
        let page = TablePage {
            table: "customers".into(),
            columns: vec!["id".into(), "first name".into(), "分数".into()],
            rows: vec![vec!["1".into(), "Ana".into(), "7".into()]],
            page: 0,
            page_size: 25,
            total_rows: 1,
            sort: None,
            filter: None,
            primary_key: vec!["id".into()],
            read_only: None,
        };
        state.grid.load_page(&page);
        state.grid.select(0, 1);
        state
    }

    #[test]
    fn action_is_bound_to_the_open_popup_and_keeps_raw_prefix_and_selection() {
        let mut state = customer_table();
        let lease = header_lease(&state).unwrap();
        let point = PointerPress { x: 20.0, y: 30.0 };
        assert_eq!(frozen_columns(&state), Some(0));
        open(&mut state, lease, 0, point);
        let superseded = state.table_freeze.nonce;
        open(&mut state, lease, 1, point);
        let current = state.table_freeze.nonce;
        apply(&mut state, superseded, true);
        assert_eq!(frozen_columns(&state), Some(0));
        apply(&mut state, current, true);
        assert_eq!(frozen_columns(&state), Some(2));
        assert_eq!(state.grid.focus(), (0, 1));
        assert_eq!(state.grid.copy_tsv(), "Ana");
        assert_eq!(state.grid.dirty_cells(), 0);
        assert_eq!(state.table_freeze.tabs[&lease.tab].columns, ["id", "first name"]);

        // Sorting/page loads keep the same table/tab owner but invalidate
        // a menu that referred to the previous page.
        state.next_read_generation();
        apply(&mut state, current, false);
        assert_eq!(frozen_columns(&state), Some(2));
        assert!(current_menu(&state).is_none());

        let same_name = state.tabs.open(TabKind::Table, "customers").unwrap();
        assert_eq!(frozen_columns(&state), Some(0));
        open(&mut state, lease, 1, point);
        assert!(!blocks_grid_keys(&state));
        assert!(state.tabs.close(same_name));
        assert_eq!(frozen_columns(&state), Some(2));
        state.navigator_value_search_session_generation = Some(11);
        assert_eq!(frozen_columns(&state), Some(0));
    }

    #[test]
    fn visibility_changes_remap_the_displayed_prefix_and_invalidate_old_header_actions() {
        let mut state = customer_table();
        let lease = header_lease(&state).unwrap();
        let point = PointerPress { x: 20.0, y: 30.0 };
        open(&mut state, lease, 1, point);
        let nonce = state.table_freeze.nonce;
        apply(&mut state, nonce, true);
        assert_eq!(frozen_columns(&state), Some(2));

        state.grid.set_displayed_columns(Some(&["first name".into(), "分数".into()]));
        assert_eq!(state.grid.displayed_raw(0), Some(1));
        assert_eq!(state.grid.displayed_raw(1), Some(2));
        assert_eq!(frozen_columns(&state), Some(2));
        apply(&mut state, nonce, false);
        assert_eq!(frozen_columns(&state), Some(2), "old viewport menu cannot unfreeze");
        state.grid.set_displayed_columns(Some(&[]));
        assert_eq!(frozen_columns(&state), Some(0));
        state.grid.set_displayed_columns(None);
        assert_eq!(frozen_columns(&state), Some(2), "zero displayed was not a reset");
    }

    #[test]
    fn scope_rejects_inline_edits_loading_and_out_of_scope_routes() {
        let mut state = customer_table();
        let lease = header_lease(&state).unwrap();
        let point = PointerPress { x: 20.0, y: 30.0 };
        state.grid.begin_edit();
        assert!(state.grid.is_editing());
        open(&mut state, lease, 1, point);
        assert!(current_menu(&state).is_none());
        state.grid.cancel_edit();
        state.table_loading = Some(1);
        open(&mut state, lease, 1, point);
        assert!(current_menu(&state).is_none());
        state.table_loading = None;
        open(&mut state, lease, 1, point);
        state.table_view = TableView::Ddl;
        assert!(retire_stale_menu(&mut state));
        assert_eq!(frozen_columns(&state), None);
        state.table_view = TableView::Data;
        state.desktop_connections.as_mut().unwrap()[0].engine = "postgresql".into();
        assert!(header_lease(&state).is_none());
    }
}
