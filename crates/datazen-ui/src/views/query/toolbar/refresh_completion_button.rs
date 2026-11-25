//! Port of `windows/connection/query/toolbar/RefreshCompletionButton.tsx`.
//!
//! React exposes one icon-only ghost toolbar button that explicitly refreshes
//! the completion schema cache: it guards empty sessions and double clicks,
//! marks itself busy, then asks the host to refresh the session. A failure is
//! shown as the button's tooltip instead of a success message. This port keeps
//! that observable contract; the four commands behind the click (cache
//! invalidation, schema refresh, table reload, completion notice) are host
//! work, reached only through [`RefreshCompletionAction::Refresh`] and the
//! [`RefreshCompletionVm::apply_refresh`] answer — no fake IPC or timer.
//!
//! ## Keys
//!
//! `editor-refresh-completion-button` is React's `tid(...)` testid and the Rune
//! element key. The accessible name is `query.refreshCompletion`; the tooltip
//! (`help`) is `query.refreshCompletionTitle` until an error replaces it.
//!
//! ## Local stand-ins (removal stage)
//!
//! The VM mirrors the props and the two local `useState` values. No production
//! host mounts this button yet, so every reusable item carries a per-item
//! `#[allow(dead_code)]` with the removal stage "drop when the query toolbar
//! host wires the completion refresh".

use std::rc::Rc;

use rune::El;

use crate::i18n::t;
use crate::icons::ICON_REFRESH;
use crate::ui::toolbar::{ToolbarButton, ToolbarButtonVariant};

pub const BUTTON_KEY: &str = "editor-refresh-completion-button";

/// React `RefreshCompletionButtonProps` plus `busy` and `error` from `useState`.
#[allow(dead_code)] // VM mirror; remove when the toolbar host lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefreshCompletionVm {
    pub db_session_id: String,
    pub database: Option<String>,
    pub icon_only: bool,
    pub disabled: bool,
    pub busy: bool,
    pub error: String,
    /// Set on success: the text React passes to `onRefreshed`.
    pub done_message: Option<String>,
}

#[allow(dead_code)] // VM helpers; remove with the host.
impl RefreshCompletionVm {
    /// React `disabled={disabled || busy || !dbSessionId}`.
    pub fn effective_disabled(&self) -> bool {
        self.disabled || self.busy || self.db_session_id.trim().is_empty()
    }

    /// React `title={error ?? t('query.refreshCompletionTitle')}`.
    pub fn title(&self) -> String {
        if self.error.is_empty() {
            t("query.refreshCompletionTitle").to_owned()
        } else {
            self.error.clone()
        }
    }

    /// React's accessible name: `label={t('query.refreshCompletion')}`.
    pub fn label(&self) -> String {
        t("query.refreshCompletion").to_owned()
    }

    /// React `handleRefresh` prologue; false means the click was ignored.
    pub fn begin_refresh(&mut self) -> bool {
        if self.db_session_id.trim().is_empty() || self.busy {
            return false;
        }
        self.busy = true;
        self.error.clear();
        self.done_message = None;
        true
    }

    /// React's `finally` plus the success/error branches. A failed refresh
    /// keeps the error in the tooltip and never reports success.
    pub fn apply_refresh(&mut self, result: Result<(), String>) {
        self.busy = false;
        match result {
            Ok(()) => {
                self.error.clear();
                self.done_message = Some(t("query.refreshCompletionDone").to_owned());
            }
            Err(message) => {
                self.error = message;
                self.done_message = None;
            }
        }
    }
}

/// The single host request behind the button.
#[allow(dead_code)] // host port; remove when the toolbar host wires the button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshCompletionAction {
    Refresh,
}

#[allow(dead_code)] // host port; remove when the toolbar host wires the button.
pub type RefreshCompletionActionFn<S> =
    Rc<dyn Fn(&mut S, RefreshCompletionAction) + 'static>;

/// Build the button. A disabled button has no press handler, matching React's
/// guarded `onClick` (Rune's a11y `disabled` alone does not block handlers).
#[allow(dead_code)] // view entry; remove when the toolbar host wires it.
pub fn build<S: 'static>(
    vm: &RefreshCompletionVm,
    on_action: Option<RefreshCompletionActionFn<S>>,
    dark: bool,
) -> El {
    let label = vm.label();
    let mut button = ToolbarButton::new(BUTTON_KEY, label.clone())
        .icon(&ICON_REFRESH)
        .icon_only(vm.icon_only)
        .variant(ToolbarButtonVariant::Ghost)
        .disabled(vm.effective_disabled());
    if !vm.effective_disabled() {
        if let Some(callback) = on_action {
            button = button.on_press(move |s: &mut S| callback(s, RefreshCompletionAction::Refresh));
        }
    }
    button
        .build(dark)
        .key(BUTTON_KEY)
        .label(label)
        .help(vm.title())
}
