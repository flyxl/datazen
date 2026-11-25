//! Port of `windows/connection/QueryPanel.tsx`.
//!
//! React's QueryPanel is the glue of the query workspace: it resolves the
//! focused pane, infers the database/schema context, routes editor actions to
//! that pane, splits the editor from the results host, toggles the history and
//! favorites sidebars, and hosts the transaction / retry / dangerous-SQL /
//! strategy-ask / result-message dialogs. This port keeps that observable
//! contract in [`QueryPanelVm`] plus pure helpers; every command is an injected
//! [`QueryPanelAction`] the host executes — no fake IPC, timer or thread.
//!
//! ## Composition
//!
//! The already-ported child views (editor section, sidebar, execution gate,
//! transaction modals, strategy ask/select) own their own VMs and host ports.
//! The query-panel host translates [`QueryPanelAction`] into those child calls;
//! until it lands, the editor/results/sidebar areas are honest keyed slots and
//! the result-message dialog and session banner are rendered here. Those slots
//! and every reusable item carry per-item `#[allow(dead_code)]` with removal
//! stage "drop when the workspace wires the query-panel host".
//!
//! ## Keys
//!
//! `query-panel` (React testid), `query-results-host`, `editor-new-session-button`
//! and `result-message-ok` are kept verbatim; the remaining region keys are
//! named after the React elements (`query-panel-editor`, `query-panel-sidebar`,
//! `query-panel-session-banner`, ...).

use std::rc::Rc;

use rune::{col, row, El};

mod dialogs;
mod panes;

pub const ROOT_KEY: &str = "query-panel";
pub const EDITOR_KEY: &str = "query-panel-editor";
pub const RESULTS_HOST_KEY: &str = "query-results-host";
pub const SIDEBAR_KEY: &str = "query-panel-sidebar";
pub const SESSION_BANNER_KEY: &str = "query-panel-session-banner";
pub const NEW_SESSION_KEY: &str = "editor-new-session-button";
pub const MESSAGE_DIALOG_KEY: &str = "result-message-dialog";
pub const MESSAGE_OK_KEY: &str = "result-message-ok";
pub const EXECUTE_KEY: &str = "query-panel-execute";
pub const STOP_KEY: &str = "query-panel-stop";

/// React `SessionView.state` values the panel reacts to.
#[allow(dead_code)] // VM mirror; remove when the session host lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PanelSessionState {
    #[default]
    Active,
    Lost,
    Closed,
}

impl PanelSessionState {
    /// React `['lost', 'closed'].includes(state)`.
    pub fn is_disconnected(self) -> bool {
        matches!(self, Self::Lost | Self::Closed)
    }
}

/// React's cancel button availability: execute vs stop, and why stop is off.
#[allow(dead_code)] // VM helper; remove when the execution host lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CancelAvailability {
    #[default]
    Execute,
    Stop,
    /// Running with no execution id yet, or the driver cannot cancel.
    StopDisabled,
}

/// React `ResultMessageDialog` kind.
#[allow(dead_code)] // VM mirror; remove when the message host lands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MessageKind {
    #[default]
    Error,
    Success,
}

/// React's message dialog `useState` triple.
#[allow(dead_code)] // VM mirror; remove when the message host lands.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MessageDialogVm {
    pub open: bool,
    pub kind: MessageKind,
    pub text: String,
}

/// The observable panel state. The host owns the real store/editor/session.
#[allow(dead_code)] // VM surface; remove when the workspace wires the panel host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QueryPanelVm {
    pub panel_id: String,
    /// `None` means the tab is not split (the pre-pane call contract).
    pub focused_pane_id: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub current_database: Option<String>,
    pub current_schema: Option<String>,
    /// A confirmed session namespace overrides the panel's bound context.
    pub observed_database: Option<String>,
    pub observed_schema: Option<String>,
    pub namespace_confirmed: bool,
    pub is_path_hierarchy: bool,
    pub sql: String,
    pub running: bool,
    pub execution_id: Option<String>,
    pub error: String,
    pub results_count: usize,
    pub show_explain: bool,
    pub diagnosis_visible: bool,
    /// `supportsCancelQuery && supportsQueryExecutionCancel`. `None` = unknown.
    pub cancel_capability: Option<bool>,
    pub in_transaction: bool,
    pub session_state: PanelSessionState,
    pub history_visible: bool,
    pub favorites_visible: bool,
    pub message: MessageDialogVm,
    /// React's `copied` feedback flag; the host clears it after its deadline.
    pub message_copied: bool,
    pub dangerous_confirm_open: bool,
    pub switch_tx_confirm_open: bool,
    pub strategy_ask_open: bool,
    pub execution_seq: u32,
}

/// React `paneKey(panelId, pane)`: `panel::pane` for a split, bare panel id
/// otherwise.
#[allow(dead_code)] // pure helper; remove when the panel store port lands.
pub fn pane_key(panel_id: &str, pane: Option<&str>) -> String {
    match pane {
        Some(pane) if !pane.is_empty() => format!("{panel_id}::{pane}"),
        _ => panel_id.to_owned(),
    }
}

/// React `resolveFocusedPaneId`: the focused pane or the bare panel id.
#[allow(dead_code)] // pure helper; remove when the panel store port lands.
pub fn resolve_focused_pane_id(focused: Option<&str>) -> String {
    focused.filter(|pane| !pane.is_empty()).unwrap_or("").to_owned()
}

#[allow(dead_code)] // pure helpers; remove when the panel host lands.
impl QueryPanelVm {
    /// React's `selectedDatabase`: confirmed namespace, else path-hierarchy
    /// `(currentDatabase ?? boundDatabase)`, else `(boundDatabase ?? currentDatabase)`.
    /// A bound empty string is treated as unset (`database?.trim() ? database : null`).
    pub fn selected_database(&self) -> Option<String> {
        let bound = self.database.as_deref().filter(|db| !db.trim().is_empty());
        if self.namespace_confirmed {
            // React returns observedNamespace.database directly, even when null.
            return self.observed_database.clone();
        }
        if self.is_path_hierarchy {
            self.current_database
                .clone()
                .or_else(|| bound.map(str::to_owned))
        } else {
            bound.map(str::to_owned).or_else(|| self.current_database.clone())
        }
    }

    /// React's `selectedSchema`: confirmed namespace, else `schema ?? currentSchema`.
    pub fn selected_schema(&self) -> Option<String> {
        if self.namespace_confirmed {
            self.observed_schema.clone()
        } else {
            self.schema
                .clone()
                .or_else(|| self.current_schema.clone())
        }
    }

    /// React `resultsIdle`: nothing executed and no explanation/diagnosis open.
    pub fn results_idle(&self) -> bool {
        !self.running
            && self.error.is_empty()
            && !self.show_explain
            && !self.diagnosis_visible
            && self.results_count == 0
    }

    /// React's execute/cancel button state.
    pub fn cancel_availability(&self) -> CancelAvailability {
        if !self.running {
            return CancelAvailability::Execute;
        }
        if self.execution_id.is_none() || self.cancel_capability != Some(true) {
            return CancelAvailability::StopDisabled;
        }
        CancelAvailability::Stop
    }

    /// React `cancelQuery` is only called for an enabled stop button.
    pub fn can_cancel(&self) -> bool {
        !self.running || self.cancel_availability() == CancelAvailability::Stop
    }

    /// React shows the disconnect banner only for a lost/closed session.
    pub fn session_disconnected(&self) -> bool {
        self.session_state.is_disconnected()
    }

    /// React `needsTransactionSwitchConfirm` only guards an active transaction.
    pub fn needs_switch_confirm(&self) -> bool {
        self.in_transaction
    }

    /// React `showMessageDialog`: set text/kind and open the dialog.
    pub fn show_message(&mut self, text: impl Into<String>, kind: MessageKind) {
        self.message = MessageDialogVm {
            open: true,
            kind,
            text: text.into(),
        };
    }

    /// React `onClose` of the result message dialog.
    pub fn close_message(&mut self) {
        self.message.open = false;
    }
}

/// Every interaction the panel asks the host to perform.
#[allow(dead_code)] // host port; remove when the workspace wires the panel host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryPanelAction {
    /// Route an "execute all" to the focused pane.
    Execute { pane: Option<String> },
    ExecuteSelection { sql: String, pane: Option<String> },
    Cancel { pane: Option<String> },
    Format { pane: Option<String> },
    UpdateSql { sql: String, pane: Option<String> },
    ToggleHistory,
    ToggleFavorites,
    OpenNewSession,
    ConfirmDangerous,
    CancelDangerous,
    ConfirmSwitchTx,
    CancelSwitchTx,
    CloseMessage,
    /// Copy the result-message text; the host owns the clipboard write and the
    /// 1500ms feedback deadline.
    CopyMessage { text: String },
    /// Confirm the strategy ask with a chosen execution mode.
    ChooseStrategy { entire_script: bool },
}

#[allow(dead_code)] // host port; remove when the workspace wires the panel host.
pub type QueryPanelActionFn<S> = Rc<dyn Fn(&mut S, QueryPanelAction) + 'static>;

/// The focused pane of this VM (the bare pane id, or `None` when unsplit).
fn focused_pane(vm: &QueryPanelVm) -> Option<String> {
    vm.focused_pane_id.clone()
}

/// Build the panel into `layer`.
#[allow(dead_code)] // view entry; remove when the workspace wires the panel host.
pub fn build<S: 'static>(
    vm: &QueryPanelVm,
    on_action: &Option<QueryPanelActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    panes::build(vm, on_action, dark, layer);
    dialogs::build(vm, on_action, dark, layer);
    // `focused_pane` is the single source of routing for the host callback;
    // keeping the projection here documents that unsplit panels pass `None`.
    let _ = focused_pane(vm);
}
