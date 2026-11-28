//! Navigator panel and tree views (connections + session objects).
//!
//! This is the Rune port of `windows/connection/ConnectionNavigatorTree.tsx`.
//! The React component is an orchestrator: the visible structure it produces is
//! a "Databases" title bar, a filtered search field, clickable Recent/All group
//! headers and connection rows, followed by the session's object tree. All of
//! that lives here, built on the shared primitives in `crate::ui::nav` so the
//! keys stay identical on both sides of the migration.
//!
//! Adaptations forced by the host contract (recorded as leftovers, not silent
//! drops):
//!
//! * React renders the flat rows through a virtualised `VirtualTree` with
//!   `overscan={25}`. DataZen's Rune contract addresses rows by stable DOM keys
//!   (`nav-conn-<id>`, `nav-group-*`, `tree-*`) rather than by visible position,
//!   and the connection list is bounded by the profile store, so the tree here
//!   is a keyed, clipped and scrolling column instead of a positional virtual
//!   list. The object tree follows the same rule. Removal stage: when the
//!   contract keys can be expressed by a key-aware virtual list, swap the
//!   column for `ui::tree::DataTree` (which already owns `reveal_offset` and the
//!   cursor-change heartbeat).
//! * React's drag/drop reordering, context menus, confirmation dialogs and
//!   database value search belong to the host's `AppState`/shell and are not
//!   owned by this view; the keys for the parts that do render are preserved.
//! * The shell's Tab order is pinned by existing dialog journeys (five tabs to
//!   the new-connection trigger), so the tree's focusable rows carry
//!   `tab_index(-1)`: pointer focus plus arrows/Enter still drive it, but the
//!   tree adds no Tab stops. Removal stage: when the shell's tab contract moves
//!   to a roving single stop per tree, replace this with a normal tab stop.
//! * External selection is `state.desktop_connection` (the Rune equivalent of
//!   `model.select`). rune's `El::reveal_selected` only fires for virtual
//!   stacks, and the key-stable rows here are a plain scrolling column, so the
//!   reveal is a layout-time watcher that asks the runtime to scroll the named
//!   row into view once per selection change. It uses the same frame channel as
//!   the tree base's cursor heartbeat without a permanently looping element.

use crate::i18n::t;
use crate::icons;
use crate::state_def::AppState;
use crate::ui::nav::{
    nav_branch_row, nav_connection_row, nav_empty_row, nav_leaf_row, nav_section_header,
    nav_toolbar_button, search_field,
};
use crate::ui::theme::{NAV_GAP, NAV_SEARCH_H};
use datazen_ui_api::ConnectionProfile;
use rune::tok::{self, type_};
use rune::{col, el, row, text, El, Key, KeyEvent, LoopSpec, ScrollSpec, Tx};

/// Session-only group expansion keys (React's `expandedGroups` seeds).
const RECENT_GROUP_KEY: &str = "nav:recent";
const ALL_GROUP_KEY: &str = "nav:all";

thread_local! {
    /// Selection observed at the navigator's last layout pass. One navigator
    /// per runtime; tests run in their own threads so the watchers cannot mix.
    static NAV_LAST_SEEN_SELECTION: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

/// The connections tree: Databases filter → Recent / All groups → connection
/// rows, with the selection riding the app state and the focus/keyboard contract
/// of the tree role.
///
/// Keyboard: the container is a `tree` focus group, so Up/Down (and Home/End)
/// rove between the focusable group headers and connection rows, and Enter or
/// Space presses the focused row. Left/Right additionally collapse/expand a
/// group header (rune's tree group only roves vertically). Selection is the
/// `focus_selected` row; `active_descendant` names that same mounted row, and a
/// layout-time watcher scrolls it back into view after an external selection
/// change, even when no key or pointer event triggered the rebuild.
pub fn navigator_connections_tree(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let visible = state.visible_connections();
    let matches = |c: &ConnectionProfile| {
        filter.is_empty()
            || c.name.to_lowercase().contains(&filter)
            || c.engine.to_lowercase().contains(&filter)
            || c.id.to_lowercase().contains(&filter)
            || c.host
                .as_deref()
                .is_some_and(|host| host.to_lowercase().contains(&filter))
    };

    let mut recent: Vec<ConnectionProfile> = Vec::new();
    for id in &state.recent_connection_ids {
        if let Some(c) = visible.iter().find(|c| &c.id == id) {
            if matches(c) {
                recent.push(c.clone());
            }
        }
    }
    let all: Vec<ConnectionProfile> = visible.into_iter().filter(matches).collect();

    let selected_conn = state.desktop_connection.clone();
    let filtering = !filter.is_empty();

    let mut tree = col()
        .key("navigator-connections")
        .role("tree")
        .focus_group()
        .focusable(true)
        // The shell's Tab order is contract-pinned (dialog tests count five
        // tabs to the new-connection trigger). Keep the tree keyboard-driven
        // through pointer focus + arrows, but out of the Tab sequence.
        .tab_index(-1)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .clip_soft(0.0)
        .scroll(ScrollSpec {
            stick_bottom: false,
        });
    if let Some(selected_key) = selected_row_key(&recent, &all, selected_conn.as_deref()) {
        tree = tree.active_descendant(selected_key);
    }

    // Recent section (session-only UI state). A non-empty filter forces the
    // groups open, matching React's `isOpen(expandedGroups.has(...))` override.
    let recent_expanded = filtering || !state.collapsed.contains(RECENT_GROUP_KEY);
    tree = tree.child(connection_group_header(
        "nav-group-recent",
        t("context.recent"),
        recent.len(),
        recent_expanded,
        RECENT_GROUP_KEY,
        theme,
    ));
    if recent_expanded {
        if recent.is_empty() {
            tree = tree.child(nav_empty_row(
                "nav-group-recent-empty",
                if filter.is_empty() {
                    t("main.noConnections")
                } else {
                    t("select.noMatches")
                },
                1,
                theme,
            ));
        } else {
            for c in &recent {
                let id = c.id.clone();
                let selected = selected_conn.as_deref() == Some(c.id.as_str());
                let row_el = nav_connection_row(
                    &format!("nav-conn-recent-{}", c.id),
                    &c.name,
                    &c.engine,
                    1,
                    selected,
                    theme,
                    dark,
                )
                .role("treeitem")
                .focusable(true)
                .tab_index(-1)
                .focus_selected(selected)
                .on_press(move |state: &mut AppState| state.open_connection(&id));
                tree = tree.child(row_el);
            }
        }
    }

    // All connections section.
    let all_expanded = filtering || !state.collapsed.contains(ALL_GROUP_KEY);
    tree = tree.child(connection_group_header(
        "nav-group-all",
        t("main.ungrouped"),
        all.len(),
        all_expanded,
        ALL_GROUP_KEY,
        theme,
    ));
    if all_expanded {
        if all.is_empty() {
            tree = tree.child(nav_empty_row(
                "nav-group-all-empty",
                if filter.is_empty() {
                    t("main.noConnections")
                } else {
                    t("select.noMatches")
                },
                1,
                theme,
            ));
        } else {
            for c in &all {
                let id = c.id.clone();
                let selected = selected_conn.as_deref() == Some(c.id.as_str());
                let row_el = nav_connection_row(
                    &format!("nav-conn-{}", c.id),
                    &c.name,
                    &c.engine,
                    1,
                    selected,
                    theme,
                    dark,
                )
                .role("treeitem")
                .focusable(true)
                .tab_index(-1)
                .focus_selected(selected)
                .on_press(move |state: &mut AppState| state.open_connection(&id));
                tree = tree.child(row_el);
            }
        }
    }

    // External selection (React's `model.select` path) must reveal its row even
    // when the state change did not come through this view: a direct state
    // write does not dirty layout, so a plain `on_layout` callback never runs.
    // A tiny looping probe reports its drawn rect every frame (the same frame
    // channel the tree base's cursor heartbeat uses); it reveals once per
    // selection change and stays live-only otherwise, so it causes no rebuild.
    tree = tree.child(
        el()
            .key("navigator-connections-cursor-heartbeat")
            .absolute(0.0, 0.0)
            .size(4.0, 4.0)
            .a11y_hidden()
            .looping(LoopSpec::slide(0.5, [-4.0, 0.0], [4.0, 0.0]))
            .on_drawn_layout_tx(|state: &mut AppState, _rect: [f32; 4], tx: &mut Tx| {
                let selected = state.desktop_connection.clone();
                let seen = NAV_LAST_SEEN_SELECTION.with(|cell| cell.borrow().clone());
                if selected != seen {
                    NAV_LAST_SEEN_SELECTION.with(|cell| *cell.borrow_mut() = selected.clone());
                    if let Some(id) = selected {
                        let row_key = if state.recent_connection_ids.iter().any(|recent| recent == &id) {
                            format!("nav-conn-recent-{id}")
                        } else {
                            format!("nav-conn-{id}")
                        };
                        tx.scroll_into_view_in("navigator-connections".to_owned(), row_key);
                    }
                } else {
                    tx.live_only();
                }
            }),
    );

    tree
}

/// A clickable Recent/All group header. The press toggles expansion (the React
/// section row), and Left/Right mirror that for keyboard users; the chevron and
/// folder glyphs are drawn by `nav_section_header` from the icon registry.
fn connection_group_header(
    key: &str,
    label: &str,
    count: usize,
    expanded: bool,
    expand_key: &str,
    theme: tok::Theme,
) -> El {
    let collapse = expand_key.to_owned();
    let expand = expand_key.to_owned();
    nav_section_header(key, label, count, expanded, theme)
        .role("treeitem")
        .expanded(expanded)
        .focusable(true)
        .tab_index(-1)
        .on_key(move |state: &mut AppState, ev: &KeyEvent, _tx: &mut Tx| -> bool {
            match ev.key {
                Key::Left if expanded => {
                    state.toggle_expand(&collapse);
                    true
                }
                Key::Right if !expanded => {
                    state.toggle_expand(&collapse);
                    true
                }
                _ => false,
            }
        })
        .on_press(move |state: &mut AppState| state.toggle_expand(&expand))
}

/// The navigator panel: Databases title bar, filter input, connections tree,
/// object tree and session actions.
pub fn navigator_panel(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let width = state.navigator_width() as f32;

    // React's `navigator-title-row` holds the Databases label and the new
    // connection action. Keep the outer `navigator-toolbar` key for the shell.
    let title_row = row()
        .key("navigator-title-row")
        .w_full()
        .h(32.0)
        .gap(4.0)
        .center_y()
        .child(
            text(t("nav.databases"))
                .key("navigator-connections-title")
                .font(type_::TITLE)
                .color(theme.text)
                .grow(1.0)
                .min_w(0.0)
                .truncate(),
        )
        .child(nav_toolbar_button(
            "open-query",
            &icons::ICON_PLUS,
            t("common.newQuery"),
            theme,
            |state| {
                let _ = state.open_query_tab("Query");
            },
        ));
    let title_row = title_row.children(
        state.native_object_search_is_available().then(|| {
            let k = crate::ui::kit::kit(dark);
            let label = t("objectSearch.native.title");
            row().key(super::navigator::native_object_search::OPEN_KEY).size(24.0, 24.0)
                .shrink().center().radius(6.0).role("button").label(label).help(label)
                .focusable(true).hover(k.c.surface_raised)
                .child(rune::icon(&icons::ICON_SEARCH, 14.0).color(k.c.text_muted).a11y_hidden())
                .on_press(AppState::open_native_object_search)
        }),
    );
    let toolbar = row()
        .key("navigator-toolbar")
        .w_full()
        .h(32.0)
        .child(title_row);

    // React puts its testid on the editable input. Rune keeps `navigator-filter`
    // on the search row and `navigator-filter-input` on the input (both are used
    // by existing tests), so the React testid is added as a namespaced wrapper
    // instead of renaming either one. The `navigator-` prefix keeps it out of
    // the home page's `connection-*` card-key namespace.
    let search = search_field(
        "navigator-filter",
        &state.navigator_filter,
        t("main.searchPlaceholder"),
        theme,
        dark,
        |state| &mut state.navigator_filter,
    );
    let mut filter = row()
        .key("navigator-connection-search-input")
        .w_full()
        .h(NAV_SEARCH_H);
    if state.available_navigator_value_search().is_some() {
        let colors = crate::ui::theme::colors(dark);
        let label = t("valueSearch.toolbarEntry");
        filter = filter
            .center_y()
            .gap(4.0)
            // 20pt of navigator padding + 24pt button + 4pt separation.
            .child(search.w(width - 48.0))
            .child(
                row()
                    .key(super::navigator::navigator_toolbar::SEARCH_VALUES_KEY)
                    .size(24.0, 24.0)
                    .center()
                    .radius(6.0)
                    .role("button")
                    .label(label)
                    .help(label)
                    .focusable(true)
                    .hover(colors.surface_raised)
                    .hover_group()
                    .child(
                        rune::icon(&icons::ICON_TEXT_SEARCH, 14.0)
                            .color(colors.fg_muted)
                            .group_color(colors.fg)
                            .a11y_hidden(),
                    )
                    .on_press(AppState::open_navigator_value_search),
            );
    } else {
        filter = filter.child(search);
    }

    let mut panel = col()
        .key("datazen-navigator")
        .w(width)
        .h_full()
        .px(10.0)
        .py(10.0)
        .gap(8.0)
        .fill(theme.bg)
        .child(toolbar)
        .child(filter)
        .child(navigator_connections_tree(state, theme, dark));

    if state.session_open() {
        panel = panel
            .child(section_divider("navigator-objects-divider", theme))
            .child(object_tree(state, theme))
            .child(col().w_full().grow(1.0).min_h(0.0))
            .child(navigator_actions(state, theme));
    } else {
        panel = panel.child(col().w_full().grow(1.0).min_h(0.0));
    }

    panel
}

fn section_divider(key: &str, theme: tok::Theme) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .h(1.0)
        .fill(theme.card_border.alpha(0.6))
}

fn navigator_actions(state: &AppState, theme: tok::Theme) -> El {
    let mut actions = col().key("navigator-actions").w_full().gap(4.0);
    actions = actions.child(refresh_button(state, theme));
    actions = actions.child(er_button(state, theme));
    actions = actions.child(disconnect_button(state, theme));
    actions
}

/// The object tree below the connections tree: catalog branches (database →
/// schema → category) and leaves (table/view) with indent and icons, narrowed by
/// the navigator filter.
pub fn object_tree(state: &AppState, theme: tok::Theme) -> El {
    let catalog = state.catalog();
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let mut rows: Vec<El> = Vec::new();

    for database in &catalog.databases {
        let db_match = filter.is_empty() || database.name.to_lowercase().contains(&filter);
        let mut schema_rows: Vec<El> = Vec::new();
        let mut db_has_match = db_match;

        for schema in &database.schemas {
            let schema_match = db_match || schema.name.to_lowercase().contains(&filter);
            let schema_path = format!("{}/{}", database.name, schema.name);
            let view_path = format!("{schema_path}/@views");
            let mut table_rows = Vec::new();
            let mut view_rows = Vec::new();
            for (name, is_view) in schema
                .tables
                .iter()
                .map(|name| (name, false))
                .chain(schema.views.iter().map(|name| (name, true)))
            {
                if !filter.is_empty()
                    && !db_match
                    && !schema_match
                    && !name.to_lowercase().contains(&filter)
                {
                    continue;
                }
                let title = name.clone();
                let row_el = nav_leaf_row(
                    &format!("tree-table-{name}"),
                    name,
                    if is_view { 3 } else { 2 },
                    &icons::ICON_TABLE,
                    theme,
                )
                .on_press(move |state: &mut AppState| {
                    state.open_table_tab(&title);
                });
                if is_view {
                    view_rows.push(row_el);
                } else {
                    table_rows.push(row_el);
                }
            }
            if table_rows.is_empty() && view_rows.is_empty() && !(filter.is_empty() || schema_match) {
                continue;
            }
            db_has_match = true;
            let schema_key = format!("tree-schema-{}-{}", database.name, schema.name);
            let collapsed = state.collapsed.contains(&schema_path) && filter.is_empty();
            let schema_label = format!("{} ({})", schema.name, schema.tables.len() + schema.views.len());
            let schema_row = nav_branch_row(
                &schema_key,
                &schema_label,
                Some(t("query.objectTree.schema")),
                1,
                !collapsed,
                &icons::ICON_FOLDER,
                theme,
            )
            .on_press(move |state: &mut AppState| state.toggle_expand(&schema_path));
            schema_rows.push(schema_row);
            if !collapsed {
                schema_rows.extend(table_rows);
                if !view_rows.is_empty() {
                    let views_collapsed = state.collapsed.contains(&view_path) && filter.is_empty();
                    let labels = format!("{} ({})", t("schemaTree.views"), view_rows.len());
                    schema_rows.push(nav_branch_row(
                        &format!("tree-views-{}-{}", database.name, schema.name),
                        &labels,
                        None,
                        2,
                        !views_collapsed,
                        &icons::ICON_FOLDER,
                        theme,
                    ).on_press(move |state: &mut AppState| state.toggle_expand(&view_path)));
                    if !views_collapsed {
                        schema_rows.extend(view_rows);
                    }
                }
            }
        }
        if !db_has_match && !filter.is_empty() {
            continue;
        }
        let db_path = database.name.clone();
        let db_collapsed = state.collapsed.contains(&database.name) && filter.is_empty();
        let db_label = format!("{} ({})", database.name, database.schemas.len());
        let db_row = nav_branch_row(
            &format!("tree-db-{}", database.name),
            &db_label,
            Some("database"),
            0,
            !db_collapsed,
            &icons::ICON_CONNECTIONS,
            theme,
        )
        .on_press(move |state: &mut AppState| state.toggle_expand(&db_path));
        rows.push(db_row);
        if !db_collapsed {
            rows.extend(schema_rows);
        }
    }

    if rows.is_empty() {
        let msg = if filter.is_empty() {
            t("query.objectTree.noObjects")
        } else {
            t("select.noMatches")
        };
        rows.push(nav_empty_row("object-tree-empty", msg, 0, theme));
    }

    let mut tree = col().key("object-tree").w_full().gap(NAV_GAP);
    for row_el in rows {
        tree = tree.child(row_el);
    }
    tree
}

pub fn er_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("er-button-absent").h(0.0);
    }
    action_row("er-button", "ER diagram", &icons::ICON_LAYOUT, theme)
        .on_press(|state: &mut AppState| state.open_er())
}

pub fn refresh_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("tree-refresh-absent").h(0.0);
    }
    action_row(
        "tree-refresh",
        "Refresh catalog",
        &icons::ICON_REFRESH,
        theme,
    )
    .on_press(|state: &mut AppState| state.refresh_catalog())
}

pub fn disconnect_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("disconnect-absent").h(0.0);
    }
    row()
        .key("disconnect")
        .w_full()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(theme.hover)
        .child(rune::icon(&icons::ICON_X, 14.0).color(theme.text_secondary))
        .child(text(t("main.ctx.disconnect")).font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.disconnect())
}

fn action_row(key: &str, label: &str, icon: &'static rune::Icon, theme: tok::Theme) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .h(28.0)
        .px(8.0)
        .gap(8.0)
        .center_y()
        .radius(6.0)
        .hover(theme.hover)
        .child(rune::icon(icon, 14.0).color(theme.text_secondary))
        .child(text(label.to_owned()).font(type_::LABEL).color(theme.text))
}

/// The visible row key of the connection the app state has selected (the
/// flat-row cursor), so the tree announces `active_descendant` on the mounted
/// row.
fn selected_row_key(
    recent: &[ConnectionProfile],
    all: &[ConnectionProfile],
    selected: Option<&str>,
) -> Option<String> {
    let id = selected?;
    if recent.iter().any(|c| c.id.as_str() == id) {
        return Some(format!("nav-conn-recent-{id}"));
    }
    if all.iter().any(|c| c.id.as_str() == id) {
        return Some(format!("nav-conn-{id}"));
    }
    None
}
