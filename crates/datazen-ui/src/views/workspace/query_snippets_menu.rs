//! Mount the reusable SnippetMenuButton on the active Native Query toolbar.
//! The VM is a projection of the existing snippet store, not another store.

use std::rc::Rc;

use rune::{col, Align, El, Side, Tx};

use crate::{
    sql_editor::builtin_snippets,
    views::query::toolbar::snippet_menu_button::{
        self, SnippetAction, SnippetItemVm, SnippetMenuButtonVm,
    },
    AppState,
};

pub(super) const TRIGGER: &str = "query-library-snippets-toggle";
const FIRST: &str = "snippet-select-all";

fn toggle(state: &mut AppState, tx: &mut Tx) {
    if state.query_snippet_menu_owner().is_some() {
        dismiss(state, tx);
        return;
    }
    let Some(owner) = state.snippet_edit_owner() else { return; };
    state.set_query_snippet_menu_owner(Some(owner));
    state.sql.focused = false;
    state.sql.completion_close();
    state.result_focused = false;
    tx.instant();
    tx.focus(FIRST, tx.by_keys());
}

fn dismiss(state: &mut AppState, tx: &mut Tx) {
    state.set_query_snippet_menu_owner(None);
    tx.instant();
    tx.return_focus(TRIGGER, tx.by_keys());
}

pub(super) fn trigger(state: &AppState, dark: bool) -> El {
    let vm = SnippetMenuButtonVm {
        open: state.query_snippet_menu_owner().is_some(),
        icon_only: true,
        disabled: state.snippet_edit_owner().is_none(),
        ..Default::default()
    };
    snippet_menu_button::trigger(&vm, TRIGGER,
        Some(Rc::new(|state: &mut AppState, tx, action| {
            if action == SnippetAction::ToggleMenu {
                toggle(state, tx);
            }
        })), dark)
}

fn saved_id(index: usize) -> String { format!("saved-{index}") }

pub(super) fn mount(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    let Some(owner) = state.query_snippet_menu_owner() else { return; };
    let saved = state.host_snippets();
    let vm = SnippetMenuButtonVm {
        open: true,
        icon_only: true,
        user_snippets: saved.iter().enumerate().map(|(index, snippet)| SnippetItemVm {
            id: saved_id(index),
            description_key: String::new(), // the adapter supplies the literal name
            prefix: String::new(), // native storage has no prefix
            template: snippet.sql.clone(),
        }).collect(),
        ..Default::default()
    };
    let saved_index = |id: &str| id.strip_prefix("saved-").and_then(|i| i.parse::<usize>().ok());
    let labels = saved.iter().map(|item| item.name.clone()).collect::<Vec<_>>();
    let menu = snippet_menu_button::menu(&vm,
        |id| saved_index(id).and_then(|i| labels.get(i).cloned()),
        // Neither typed-prefix expansion nor tabstop navigation is mounted.
        false,
        Some(Rc::new(move |state: &mut AppState, tx, action| {
            let SnippetAction::Insert { id, template } = action else { return; };
            if state.query_snippet_menu_owner() != Some(owner) {
                return;
            }
            if let Some(index) = saved_index(&id) {
                let Some(item) = saved.get(index) else { return; };
                if template.is_empty() || !state.host_snippets().iter()
                    .any(|current| current.name == item.name && current.sql == template)
                {
                    return;
                }
                // Never parse saved SQL as a snippet template.
                state.insert_snippet_sql(&template);
            } else if !builtin_snippets::insert(&mut state.sql, &template) {
                return;
            }

            state.set_query_snippet_menu_owner(None);
            state.sql.focus();
            state.result_focused = false;
            tx.instant();
            tx.focus("sql-editor-scroll", false);
            tx.scroll_into_view("sql-caret");
        })), dark);
    let colors = &crate::ui::kit::kit(dark).c;
    layer.push(col().key("query-snippets-popup").w(240.0).min_h(0.0).max_h(352.0)
        .fill(colors.bg_base).border(1.0, colors.border_base).radius(8.0)
        .anchored_fit(TRIGGER, Side::Bottom, Align::Start, 0.0)
        .focus_return(TRIGGER)
        .modal_tx(true, dismiss)
        .child(menu));
}
