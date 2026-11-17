//! Navigator panel and tree views (connections + session objects).
//!
//! Built on shared UI primitives in `crate::ui` so the connection navigator can
//! be styled consistently and reused as a base for richer tree interactions.

use crate::i18n::t;
use crate::icons;
use crate::state_def::AppState;
use crate::ui::nav::{
    nav_branch_row, nav_connection_row, nav_empty_row, nav_leaf_row, nav_section_header,
    nav_toolbar_button, search_field,
};
use crate::ui::theme::NAV_GAP;
use datazen_ui_api::ConnectionProfile;
use rune::tok::{self, type_};
use rune::{col, row, text, Color, El};

pub(crate) fn navigator_connections_tree(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let visible = state.visible_connections();
    let matches = |c: &ConnectionProfile| {
        filter.is_empty()
            || c.name.to_lowercase().contains(&filter)
            || c.engine.to_lowercase().contains(&filter)
            || c.id.to_lowercase().contains(&filter)
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

    let mut tree = col().key("navigator-connections").w_full().gap(NAV_GAP);
    tree = tree.child(
        row()
            .key("navigator-connections-title")
            .w_full()
            .h(24.0)
            .px(4.0)
            .center_y()
            .child(
                text("Connections")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            ),
    );

    let filtering = !filter.is_empty();

    // Recent section (session-only UI state).
    let recent_key = "nav:recent";
    let recent_expanded = filtering || !state.collapsed.contains(recent_key);
    tree = tree.child(
        nav_section_header(
            "nav-group-recent",
            "Recent",
            recent.len(),
            recent_expanded,
            theme,
        )
        .on_press(|state: &mut AppState| state.toggle_expand("nav:recent")),
    );
    if recent_expanded {
        if recent.is_empty() {
            tree = tree.child(nav_empty_row(
                "nav-group-recent-empty",
                if filter.is_empty() {
                    "No recent connections"
                } else {
                    t("select.noMatches")
                },
                1,
                theme,
            ));
        } else {
            for c in recent {
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
                .on_press(move |state: &mut AppState| state.open_connection(&id));
                tree = tree.child(row_el);
            }
        }
    }

    // All connections section.
    let all_key = "nav:all";
    let all_expanded = filtering || !state.collapsed.contains(all_key);
    tree = tree.child(
        nav_section_header(
            "nav-group-all",
            "All connections",
            all.len(),
            all_expanded,
            theme,
        )
        .on_press(|state: &mut AppState| state.toggle_expand("nav:all")),
    );
    if all_expanded {
        if all.is_empty() {
            tree = tree.child(nav_empty_row(
                "nav-group-all-empty",
                if filter.is_empty() {
                    "No connections"
                } else {
                    t("select.noMatches")
                },
                1,
                theme,
            ));
        } else {
            for c in all {
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
                .on_press(move |state: &mut AppState| state.open_connection(&id));
                tree = tree.child(row_el);
            }
        }
    }

    tree
}

pub(crate) fn navigator_panel(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    let width = state.navigator_width() as f32;

    let toolbar = row()
        .key("navigator-toolbar")
        .w_full()
        .h(32.0)
        .gap(4.0)
        .center_y()
        .child(
            text(t("nav.databases"))
                .key("navigator-title")
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

    let filter = search_field(
        "navigator-filter",
        &state.navigator_filter,
        "Search…",
        theme,
        dark,
        |state| &mut state.navigator_filter,
    );

    // Keep legacy key on the actual input for existing interactions/tests.
    // `search_field` wraps the input; also expose a stable-key alias row if needed
    // is not required because tests look up elements by key; our input key is
    // `navigator-filter-input` inside the wrapper.
    let filter = filter; // wrapper already keyed; input has `navigator-filter-input`

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

pub(crate) fn object_tree(state: &AppState, theme: tok::Theme) -> El {
    let catalog = state.catalog();
    let filter = state.navigator_filter.text.trim().to_lowercase();
    let mut rows: Vec<El> = Vec::new();

    for database in &catalog.databases {
        let db_match = filter.is_empty() || database.name.to_lowercase().contains(&filter);
        let mut schema_rows: Vec<El> = Vec::new();
        let mut db_has_match = db_match;

        for schema in &database.schemas {
            let schema_match = db_match || schema.name.to_lowercase().contains(&filter);
            let mut table_rows: Vec<El> = Vec::new();
            for name in &schema.tables {
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
                    2,
                    &icons::ICON_TABLE,
                    theme,
                )
                .on_press(move |state: &mut AppState| {
                    state.open_table_tab(&title);
                });
                table_rows.push(row_el);
            }
            if table_rows.is_empty() && !(filter.is_empty() || schema_match) {
                continue;
            }
            db_has_match = true;
            let schema_path = format!("{}/{}", database.name, schema.name);
            let schema_key = format!("tree-schema-{}-{}", database.name, schema.name);
            let collapsed = state.collapsed.contains(&schema_path) && filter.is_empty();
            let schema_label = format!("{} ({})", schema.name, schema.tables.len());
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

pub(crate) fn er_button(state: &AppState, theme: tok::Theme) -> El {
    if !state.session_open() {
        return row().key("er-button-absent").h(0.0);
    }
    action_row("er-button", "ER diagram", &icons::ICON_LAYOUT, theme)
        .on_press(|state: &mut AppState| state.open_er())
}

pub(crate) fn refresh_button(state: &AppState, theme: tok::Theme) -> El {
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

pub(crate) fn disconnect_button(state: &AppState, theme: tok::Theme) -> El {
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
