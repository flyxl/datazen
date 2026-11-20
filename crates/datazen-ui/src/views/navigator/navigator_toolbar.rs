//! Port of `windows/connection/navigator/NavigatorToolbar.tsx`.
//!
//! React renders a 48pt title row (database label + icon buttons) followed by
//! a search row (input + optional value-search entry). Every callback is a
//! prop; this port keeps that seam with [`NavigatorToolbarHostPort`] and an
//! optional [`QueryEditFn`] accessor for the controlled search input. No fake
//! IPC, timer or thread: a missing callback mounts an inert button exactly as
//! React omits an optional prop's button.
//!
//! ## Keys
//!
//! React `data-testid`s are kept verbatim: `navigator-title-row`,
//! `new-connection-button`, `navigator-refresh`, `connection-search-input` and
//! `navigator-search-values`. The remaining buttons carry Rune observation
//! keys named after their React element (`navigator-export`, `navigator-import`,
//! `navigator-new-group`, `navigator-collapse-all`, `navigator-collapse-sidebar`).
//! React's test also pins that no `global-object-search-toggle` is rendered
//! here — that key belongs to `GlobalObjectSearch`, not this toolbar.
//!
//! ## Icons and removal stage
//!
//! Four React lucide glyphs are absent from the shared registry (Upload,
//! FolderPlus, ChevronsDownUp, TextSearch); the closest registered glyph is
//! used for each and noted at its call site. No inline SVG and no new
//! colour literal: kit tokens and registry icons only. No production host
//! mounts this toolbar yet, so every reusable item carries a per-item
//! `#[allow(dead_code)]` with the removal stage "drop when the navigator host
//! wires the toolbar".

use std::rc::Rc;

use rune::{col, icon, row, text, El, Icon, TextEdit};

use crate::i18n::t;
use crate::icons::{
    ICON_ARROW_UP, ICON_CHEVRON_DOWN, ICON_DOWNLOAD, ICON_FOLDER, ICON_PANEL_LEFT_CLOSE,
    ICON_PLUS, ICON_REFRESH, ICON_SEARCH,
};
use crate::ui::field::TextField;
use crate::ui::kit::kit;

/// React `h-12 min-h-[48px]` on the title row.
#[allow(dead_code)] // geometric contract; remove when the host owns the row.
pub(crate) const TITLE_ROW_HEIGHT: f32 = 48.0;
/// React `py-1` + `h-6` on the search row.
#[allow(dead_code)] // geometric contract; remove when the host owns the row.
pub(crate) const SEARCH_ROW_HEIGHT: f32 = 32.0;

#[allow(dead_code)] // React testid; remove when the host owns the row.
pub(crate) const TITLE_ROW_KEY: &str = "navigator-title-row";
#[allow(dead_code)] // React testid; remove when the host owns the button.
pub(crate) const NEW_CONNECTION_KEY: &str = "new-connection-button";
#[allow(dead_code)] // React testid; remove when the host owns the button.
pub(crate) const REFRESH_KEY: &str = "navigator-refresh";
#[allow(dead_code)] // React testid; remove when the host owns the input.
pub(crate) const SEARCH_INPUT_KEY: &str = "connection-search-input";
#[allow(dead_code)] // React testid; remove when the host owns the button.
pub(crate) const SEARCH_VALUES_KEY: &str = "navigator-search-values";

/// React's controlled `searchQuery` value.
#[allow(dead_code)] // VM mirror; remove when the navigator host owns the query.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct NavigatorToolbarVm {
    pub(crate) search_query: TextEdit,
}

impl NavigatorToolbarVm {
    /// React's `searchQuery` string (trimmed-free, exactly as typed).
    #[allow(dead_code)] // VM helper; remove when the host owns the query.
    pub(crate) fn search_text(&self) -> &str {
        &self.search_query.text
    }
}

type Press<S> = Rc<dyn Fn(&mut S) + 'static>;
#[allow(dead_code)] // text accessor; remove when the host owns the query.
pub(crate) type QueryEditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>;

/// Every React prop callback, kept as an injected host port. `new_connection`,
/// `new_group` and `collapse_all` are required in React (always rendered,
/// inert when unwired here); the rest are optional (mounted only when present).
#[allow(dead_code)] // host port; remove when the navigator host wires the toolbar.
pub(crate) struct NavigatorToolbarHostPort<S> {
    pub(crate) on_new_connection: Option<Press<S>>,
    pub(crate) on_new_group: Option<Press<S>>,
    pub(crate) on_collapse_all: Option<Press<S>>,
    pub(crate) on_export: Option<Press<S>>,
    pub(crate) on_import: Option<Press<S>>,
    pub(crate) on_refresh: Option<Press<S>>,
    pub(crate) on_collapse_sidebar: Option<Press<S>>,
    pub(crate) on_search_values: Option<Press<S>>,
}

impl<S> Default for NavigatorToolbarHostPort<S> {
    fn default() -> Self {
        Self {
            on_new_connection: None,
            on_new_group: None,
            on_collapse_all: None,
            on_export: None,
            on_import: None,
            on_refresh: None,
            on_collapse_sidebar: None,
            on_search_values: None,
        }
    }
}

/// A 24×24 icon button: React's `h-6 w-6 rounded-md text-fg-muted
/// hover:bg-surface-raised hover:text-fg`.
fn tool_button<S: 'static>(
    key: &str,
    title: &str,
    glyph: &'static Icon,
    press: Option<Press<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut button = row()
        .key(key.to_owned())
        .role("button")
        .label(title.to_owned())
        .focusable(true)
        .size(24.0, 24.0)
        .center()
        .radius(6.0)
        .hover(k.c.surface_raised)
        .child(icon(glyph, 14.0).color(k.c.text_muted).a11y_hidden());
    if let Some(press) = press {
        button = button.on_press(move |s: &mut S| press(s));
    }
    button
}

/// Build the two React sibling rows into `layer`.
#[allow(dead_code)] // view entry; remove when the navigator host wires it.
pub(crate) fn build<S: 'static>(
    vm: &NavigatorToolbarVm,
    host: &NavigatorToolbarHostPort<S>,
    query_edit: Option<QueryEditFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    let k = kit(dark);

    // ── Title row ────────────────────────────────────────────────────
    let mut buttons = row()
        .key("navigator-toolbar-buttons")
        .gap(2.0)
        .center_y()
        .shrink();
    if let Some(press) = host.on_export.clone() {
        // React's Upload glyph has no registry entry; the up arrow is closest.
        buttons = buttons.child(tool_button::<S>(
            "navigator-export",
            t("common.exportConnections"),
            &ICON_ARROW_UP,
            Some(press),
            dark,
        ));
    }
    if let Some(press) = host.on_import.clone() {
        buttons = buttons.child(tool_button::<S>(
            "navigator-import",
            t("common.importConnections"),
            &ICON_DOWNLOAD,
            Some(press),
            dark,
        ));
    }
    buttons = buttons.child(tool_button::<S>(
        NEW_CONNECTION_KEY,
        t("common.newConnection"),
        &ICON_PLUS,
        host.on_new_connection.clone(),
        dark,
    ));
    // React's FolderPlus has no registry entry; a folder is closest.
    buttons = buttons.child(tool_button::<S>(
        "navigator-new-group",
        t("common.newGroup"),
        &ICON_FOLDER,
        host.on_new_group.clone(),
        dark,
    ));
    // React's ChevronsDownUp has no registry entry; a down chevron is closest.
    buttons = buttons.child(tool_button::<S>(
        "navigator-collapse-all",
        t("connWin.collapseAll"),
        &ICON_CHEVRON_DOWN,
        host.on_collapse_all.clone(),
        dark,
    ));
    if let Some(press) = host.on_refresh.clone() {
        buttons = buttons.child(tool_button::<S>(
            REFRESH_KEY,
            t("connWin.refresh"),
            &ICON_REFRESH,
            Some(press),
            dark,
        ));
    }
    if let Some(press) = host.on_collapse_sidebar.clone() {
        buttons = buttons.child(tool_button::<S>(
            "navigator-collapse-sidebar",
            t("connWin.collapseSidebar"),
            &ICON_PANEL_LEFT_CLOSE,
            Some(press),
            dark,
        ));
    }

    let title_row = row()
        .key("navigator-title-row-content")
        .w_full()
        .h(TITLE_ROW_HEIGHT - 1.0)
        .gap(8.0)
        .center_y()
        .justify_between()
        .px(8.0)
        .child(
            text(t("nav.databases"))
                .key("navigator-title")
                .font(k.ty.ui_medium)
                .color(k.c.text_base)
                .truncate()
                .min_w(0.0),
        )
        .child(buttons);
    layer.push(
        col()
            .key(TITLE_ROW_KEY)
            .w_full()
            .h(TITLE_ROW_HEIGHT)
            .min_h(TITLE_ROW_HEIGHT)
            .shrink()
            .child(title_row)
            .child(
                row()
                    .key("navigator-title-row-border")
                    .w_full()
                    .h(1.0)
                    .fill(k.c.border_base)
                    .a11y_hidden(),
            ),
    );

    // ── Search row ───────────────────────────────────────────────────
    let mut field = TextField::new(SEARCH_INPUT_KEY, &vm.search_query)
        .height(24.0)
        .grow()
        .pad_x(8.0)
        .text_style(k.ty.text_small)
        .placeholder(t("main.searchPlaceholder"));
    if let Some(edit) = query_edit {
        field = field.on_edit(move |s: &mut S| edit(s));
    } else {
        field = field.disabled(true);
    }
    let mut search_row = row()
        .key("navigator-search-row")
        .w_full()
        .h(SEARCH_ROW_HEIGHT)
        .shrink()
        .gap(4.0)
        .center_y()
        .px(8.0)
        .py(4.0)
        .child(
            row()
                .key("navigator-search-input-wrap")
                .w_full()
                .grow(1.0)
                .min_w(0.0)
                .h(24.0)
                .gap(4.0)
                .center_y()
                .child(
                    icon(&ICON_SEARCH, 14.0)
                        .key("navigator-search-icon")
                        .color(k.c.text_muted)
                        .a11y_hidden(),
                )
                .child(field.build(dark)),
        );
    if let Some(press) = host.on_search_values.clone() {
        // React's TextSearch glyph has no registry entry; a search is closest.
        search_row = search_row.child(tool_button::<S>(
            SEARCH_VALUES_KEY,
            t("valueSearch.toolbarEntry"),
            &ICON_SEARCH,
            Some(press),
            dark,
        ));
    }
    layer.push(search_row);
}
