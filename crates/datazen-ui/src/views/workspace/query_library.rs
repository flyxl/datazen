//! Compact, real disclosures for the existing Query Library views.

use crate::icons::{ICON_CLOCK, ICON_CODE_2, ICON_X};
use crate::i18n::t;
use crate::state_def::AppState;
use crate::types::QuerySidebarPanel::{self, Favorites, History, Snippets};
use rune::{icon, row, text, tok, El, Icon, Tx};

// Same authored Lucide bookmark as QueryEditorSection's Favorites control.
static BOOKMARK: Icon = Icon {
    name: "query-library-bookmark",
    view: 24.0,
    body: r#"<path stroke="currentColor" stroke-width="2" fill="none" stroke-linecap="round" stroke-linejoin="round" d="m19 21-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16z"/>"#,
};

pub(super) fn label_key(panel: QuerySidebarPanel) -> &'static str {
    match panel {
        Favorites => "query.favorites",
        History => "query.history",
        Snippets => "query.snippets",
    }
}

fn trigger_key(panel: QuerySidebarPanel) -> &'static str {
    match panel {
        Favorites => "query-library-favorites-toggle",
        History => "query-library-history-toggle",
        Snippets => "query-library-snippets-toggle",
    }
}

/// A real close button also remains on the pane when the two saved
/// splitter widths leave little room in the main toolbar.
pub(super) fn header(state: &AppState, theme: tok::Theme) -> El {
    let trigger = trigger_key(state.query_sidebar_panel);
    row().w_full().center_y().gap(6.0)
        .child(text("Query library").font(tok::type_::TITLE).color(theme.text)
            .grow(1.0).min_w(0.0).truncate())
        .child(row().key("query-library-close").size(22.0, 22.0).shrink().center()
            .radius(6.0).role("button").label(t("common.close")).help(t("common.close"))
            .focusable(true).hover(theme.hover)
            .child(icon(&ICON_X, 14.0).color(theme.text_secondary).a11y_hidden())
            .on_press_tx(move |state: &mut AppState, tx: &mut Tx| {
                state.close_query_sidebar();
                tx.focus(trigger, true);
            }))
}

pub(super) fn controls(state: &AppState, theme: tok::Theme) -> El {
    let mut group = row().key("query-library-controls").shrink().px(8.0).gap(2.0).center_y();
    for (panel, glyph) in [
        (Snippets, &ICON_CODE_2),
        (History, &ICON_CLOCK),
        (Favorites, &BOOKMARK),
    ] {
        let active = state.query_sidebar_shows(panel);
        let label = t(label_key(panel));
        let mut button = row()
            .key(trigger_key(panel))
            .size(28.0, 28.0)
            .shrink()
            .center()
            .radius(9.0)
            .role("button")
            .label(label)
            .help(label)
            .expanded(active)
            .focusable(true)
            .child(icon(glyph, 14.0)
                .color(if active { theme.text } else { theme.text_secondary })
                .a11y_hidden())
            .on_press(move |state: &mut AppState| state.toggle_query_sidebar(panel));
        button = if active { button.fill(theme.control_off) } else { button.hover(theme.hover) };
        group = group.child(button);
    }
    group
}
