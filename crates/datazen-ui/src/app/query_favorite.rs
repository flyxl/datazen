//! The focused native Query Save chord opens the existing favorite-name modal.
//! Only a confirmed title is allowed to cross the host's favorites-store port.

use std::rc::Rc;

use rune::{El, KeyEvent, TextEdit, Tx};

use crate::{
    query_editor_tabs::QueryEditorViewport,
    state_def::AppState,
    views::query::query_transaction_modals::{
        build_live_favorite, can_save_favorite, FavoriteNameDialogVm,
        QueryTransactionHostPort, QueryTransactionModalAction,
    },
};

#[derive(Default)]
pub(crate) struct NativeQueryFavorite {
    pending: Option<FavoriteDraft>,
    name: TextEdit,
    epoch: u64,
    save_failed: bool,
}

struct FavoriteDraft {
    owner: QueryEditorViewport,
    connection: String,
    session_generation: u32,
    sql: String,
}

impl FavoriteDraft {
    fn still_current(&self, state: &AppState) -> bool {
        self.owner.owns_current_query(state)
            && state.desktop_connection.as_deref() == Some(self.connection.as_str())
            && state.navigator_value_search_session_generation == Some(self.session_generation)
    }
}

impl AppState {
    pub(crate) fn has_query_favorite_shortcut_context(&self) -> bool {
        self.has_query_shortcut_context()
            && self.desktop_catalog.is_some()
            && self.desktop_connection.is_some()
            && self.navigator_value_search_session_generation.is_some()
    }

    pub(super) fn native_query_favorite_active(&self) -> bool {
        self.native_query_favorite.pending.as_ref().is_some_and(|draft| draft.still_current(self))
    }

    pub(super) fn retire_stale_query_favorite(&mut self) -> bool {
        if self.native_query_favorite.pending.is_some() && !self.native_query_favorite_active() {
            self.close_query_favorite();
            return true;
        }
        false
    }

    fn close_query_favorite(&mut self) {
        self.native_query_favorite.pending = None;
        self.native_query_favorite.name = TextEdit::default();
        self.native_query_favorite.save_failed = false;
    }

    fn open_query_favorite(&mut self) {
        let sql = self.sql.doc().trim();
        if sql.is_empty() || self.native_query_favorite.pending.is_some() {
            return;
        }
        let (Some(owner), Some(connection), Some(session_generation)) = (
            self.query_editor_viewport(),
            self.desktop_connection.clone(),
            self.navigator_value_search_session_generation,
        ) else { return };
        let draft = FavoriteDraft { owner, connection, session_generation, sql: sql.to_owned() };
        if draft.still_current(self) {
            self.native_query_favorite.name = TextEdit::default();
            self.native_query_favorite.name.focused = true;
            self.native_query_favorite.epoch = self.native_query_favorite.epoch.wrapping_add(1);
            self.native_query_favorite.save_failed = false;
            self.native_query_favorite.pending = Some(draft);
        }
    }

    fn handle_query_favorite_action(&mut self, epoch: u64, action: QueryTransactionModalAction) {
        use QueryTransactionModalAction::{FavoriteCancel, FavoriteSave};

        // Don't let a late callback from a removed/previous modal write,
        // close a newer dialog, or capture SQL from the newly selected tab.
        if self.native_query_favorite.epoch != epoch || !self.native_query_favorite_active() {
            return;
        }
        match action {
            FavoriteCancel => self.close_query_favorite(),
            FavoriteSave if can_save_favorite(&self.native_query_favorite.name.text) => {
                let Some(draft) = self.native_query_favorite.pending.as_ref() else { return };
                let result = self.desktop.save_favorite(
                    &draft.connection, self.native_query_favorite.name.text.trim(), &draft.sql,
                );
                if result.is_ok() {
                    self.close_query_favorite();
                } else {
                    // The host error may contain a local path. Keep the form
                    // retryable and show a localized, non-sensitive error.
                    self.native_query_favorite.save_failed = true;
                }
            }
            _ => {}
        }
    }
}

pub(super) fn shortcut(state: &mut AppState, ev: &KeyEvent, tx: &Tx) -> bool {
    if !super::query_shortcuts::SAVE_QUERY.matches_tx(ev, tx)
        || !state.has_query_favorite_shortcut_context()
        || !state.owns_query_document_shortcut(tx)
    {
        return false;
    }
    // React's CodeMirror save binding consumes Mod+S even for a blank
    // document. It outranks an overlapping Format key but never writes SQL.
    if !tx.key_repeat() {
        state.open_query_favorite();
    }
    true
}

pub(crate) fn mount(state: &AppState, dark: bool, layer: &mut Vec<El>) {
    let form = &state.native_query_favorite;
    let Some(draft) = form.pending.as_ref().filter(|draft| draft.still_current(state)) else { return };
    let epoch = form.epoch;
    let favorite = FavoriteNameDialogVm {
        open: true,
        favorite_name: form.name.clone(),
        favorite_dialog_sql: draft.sql.clone(),
    };
    let host = QueryTransactionHostPort {
        on_action: Some(Rc::new(move |s: &mut AppState, action| s.handle_query_favorite_action(epoch, action))),
        favorite_name_edit: Some(Rc::new(|s: &mut AppState| &mut s.native_query_favorite.name)),
    };
    build_live_favorite(&favorite, &host, epoch, form.save_failed, dark, layer);
}
