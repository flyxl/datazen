//! Port of `windows/connection/query/QueryTransactionModals.tsx`.
//!
//! React's file owns three modals: the unclosed-transaction confirm, the
//! aborted-transaction rollback prompt, and the favorite-name save dialog.
//! (The transaction hook and results pane in the same file are separate
//! concerns and stay with their hosts.) A closed React `Dialog` mounts
//! nothing; this port pushes only the open ones into the app layer. Every
//! confirm / cancel / save / rollback is a [`QueryTransactionModalAction`]
//! dispatched through an injected host port — no fake IPC, controller or timer.
//!
//! ## Keys
//!
//! `query-favorite-save` and `query-favorite-title-input` are React's
//! `data-testid`s and are kept verbatim. The `query-tx-*` keys name Rune-side
//! handles for the two transaction dialogs and their controls. The dialog base
//! adds its documented `<key>-title/-body/-footer/-close/-scrim/-layer` keys.
//!
//! ## Leftover L1 (removal stage)
//!
//! Mount these from the query-panel host, feed the transaction state and busy
//! flag from its session controller, bind the host port to the real
//! confirm/cancel/rollback/save callbacks, and drop the per-item allows and
//! Rune-only observation keys.

use std::rc::Rc;

use rune::TextEdit;
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;

mod aborted;
mod favorite;
mod unclosed;

pub(crate) use aborted::{ABORTED_KEY, DETAIL_KEY, ROLLBACK_KEY, SKIP_KEY};
pub(crate) use favorite::{
    CANCEL_KEY as FAVORITE_CANCEL_KEY, FAVORITE_KEY, SAVE_KEY, SQL_KEY, TITLE_INPUT_KEY,
};
pub(crate) use unclosed::{BODY_KEY, CANCEL_KEY, CONFIRM_KEY, UNCLOSED_KEY};

/// React `QueryTransactionModalsProps` as a cloneable VM.
#[allow(dead_code)] // VM mirror; remove when the query-panel host wires the modals.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct QueryTransactionModalsVm {
    pub tx_unclosed_open: bool,
    pub tx_aborted_open: bool,
    pub tx_aborted_detail: Option<String>,
    pub tx_busy: bool,
}

/// React `FavoriteNameDialogProps` as a cloneable VM.
#[allow(dead_code)] // VM mirror; remove when the query-panel host wires the dialog.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FavoriteNameDialogVm {
    pub open: bool,
    pub favorite_name: TextEdit,
    pub favorite_dialog_sql: String,
}

impl Default for FavoriteNameDialogVm {
    fn default() -> Self {
        Self {
            open: false,
            favorite_name: TextEdit::new(String::new()),
            favorite_dialog_sql: String::new(),
        }
    }
}

/// Every callback the three modals ask the host to perform.
#[allow(dead_code)] // host port; remove when the query-panel host wires the modals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QueryTransactionModalAction {
    ConfirmUnclosed,
    CancelUnclosed,
    AbortedRollback,
    AbortedSkip,
    FavoriteSave,
    FavoriteCancel,
}

#[allow(dead_code)] // host port; remove when the query-panel host wires the modals.
pub(crate) type ModalActionFn<S> = Rc<dyn Fn(&mut S, QueryTransactionModalAction) + 'static>;

/// The injected host port: actions plus the favorite-name text accessor the
/// dialog needs for editing and Enter-to-save.
#[allow(dead_code)] // host port; remove when the query-panel host wires the modals.
pub(crate) struct QueryTransactionHostPort<S> {
    pub on_action: Option<ModalActionFn<S>>,
    pub favorite_name_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
}

impl<S> Default for QueryTransactionHostPort<S> {
    fn default() -> Self {
        Self {
            on_action: None,
            favorite_name_edit: None,
        }
    }
}

impl<S: 'static> QueryTransactionHostPort<S> {
    #[allow(dead_code)] // host port; remove when the query-panel host wires the modals.
    pub(crate) fn emit(&self, state: &mut S, action: QueryTransactionModalAction) {
        if let Some(on_action) = &self.on_action {
            on_action(state, action);
        }
    }
}

/// A press handler that dispatches one action to the host.
fn emit<S: 'static>(
    host: &QueryTransactionHostPort<S>,
    action: QueryTransactionModalAction,
) -> impl Fn(&mut S) + Clone {
    let callback = host.on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action);
        }
    }
}

/// A footer button matching React's `<Button variant=…>`; `disabled` mirrors
/// the abort dialog's `txBusy` gate and the favorite dialog's empty-name gate.
fn footer_button<S: 'static>(
    key: &str,
    label: &str,
    variant: ButtonVariant,
    disabled: bool,
    host: &QueryTransactionHostPort<S>,
    action: QueryTransactionModalAction,
    k: &Kit,
) -> rune::El {
    Button::new(t(label))
        .variant(variant)
        .control_size(ControlSize::Small)
        .disabled(disabled)
        .on_press(emit(host, action))
        .build(k)
        .key(key)
}

/// Push the open dialogs into `layer`, in React's render order.
#[allow(dead_code)] // view entry; remove when the query-panel host wires the modals.
pub(crate) fn build<S: 'static>(
    vm: &QueryTransactionModalsVm,
    favorite: Option<&FavoriteNameDialogVm>,
    host: &QueryTransactionHostPort<S>,
    dark: bool,
    layer: &mut Vec<rune::El>,
) {
    if vm.tx_unclosed_open {
        unclosed::build(host, dark, layer);
    }
    if vm.tx_aborted_open {
        aborted::build(vm, host, dark, layer);
    }
    if let Some(favorite) = favorite {
        if favorite.open {
            favorite::build::<S>(favorite, host, dark, layer);
        }
    }
}

/// The favorite dialog's Enter-to-save guard: React only saves when the title
/// has a non-whitespace value.
#[allow(dead_code)] // pure helper; remove with the favorite dialog.
pub(crate) fn can_save_favorite(name: &str) -> bool {
    !name.trim().is_empty()
}
