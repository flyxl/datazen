//! Port of `packages/frontend/src/lib/session/tableSession.ts`.
//!
//! Pure helpers that bind a table panel to the query-panel session registry:
//!
//! * `table_session_key` — the `:table`-suffixed pane key; a duplicated panel
//!   owns a distinct key and therefore a distinct owner/session.
//! * `table_session_controller_request` — validates the change context and
//!   derives the exact `(key, target, owner)` triple the registry `get` takes.
//! * `table_manual_handle` — returns the live handle only when the session is
//!   still usable and has an active/aborted transaction; a closing/closed/lost
//!   session throws rather than handing back a stale handle.
//!
//! Everything called here is reused: `QueryPanelSessionRegistry` (T183),
//! `SessionView`/`SessionHandle`/`SessionState`/`TransactionState` and the DTOs
//! from T083. No DTO is duplicated. `TableChangeContext` is a local VM for the
//! React interface of the same name; removal stage: import it from the shared
//! table-change module when that lands. The request builder is pure (it returns
//! the arguments instead of calling the ambient root); removal stage: call
//! `getQueryPanelSessionRegistry().get(...)` in the host once the table view
//! owns the root.
//!
//! No view consumer yet: every item carries a per-item `#[allow(dead_code)]`
//! with the removal stage "drop when the table view wires these helpers".

use crate::session::editor_session_controller::{
    ExecutionTarget, NamespaceTarget, OwnerRef, SessionHandle, TransactionState,
};
use crate::session::query_panel_session::QueryPanelSessionRegistry;

/// React `TableChangeContext` (only the fields this module reads).
#[allow(dead_code)] // local VM; remove with the shared table-change module.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableChangeContext {
    pub(crate) connection_id: Option<String>,
    pub(crate) db_session_id: String,
    pub(crate) driver_type: Option<String>,
    pub(crate) database: Option<String>,
    pub(crate) schema: Option<String>,
    pub(crate) table: String,
}

/// The two fail-loud conditions from the React source.
#[allow(dead_code)] // error mirror; remove with the table view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TableSessionError {
    ConnectionRequired,
    SessionLost,
}

#[allow(dead_code)] // error mirror; remove with the table view.
impl TableSessionError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::ConnectionRequired => "Table connection required",
            Self::SessionLost => "Table session lost; explicitly open a new session.",
        }
    }
}

/// React `tableSessionKey(panelId)`.
#[allow(dead_code)] // helper; remove with the table view.
pub(crate) fn table_session_key(panel_id: &str) -> String {
    format!("{panel_id}:table")
}

/// The exact registry-lookup inputs React passes to `registry.get`.
#[allow(dead_code)] // helper; remove with the table view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TableControllerRequest {
    pub(crate) key: String,
    pub(crate) target: ExecutionTarget,
    pub(crate) owner: OwnerRef,
}

/// Validate the context and derive `(key, target, owner)` for the registry.
/// `client_instance_id` is the host-issued identity T208's
/// `editorOwnerForPane` would fetch; pass it through verbatim.
#[allow(dead_code)] // helper; remove with the table view.
pub(crate) fn table_session_controller_request(
    panel_id: &str,
    context: &TableChangeContext,
    client_instance_id: &str,
) -> Result<TableControllerRequest, TableSessionError> {
    let connection_id = context
        .connection_id
        .as_deref()
        .filter(|value| !value.is_empty())
        .ok_or(TableSessionError::ConnectionRequired)?;
    let key = table_session_key(panel_id);
    let target = ExecutionTarget {
        connection_id: connection_id.to_owned(),
        namespace: NamespaceTarget {
            database: context.database.clone(),
            catalog: None,
            schema: context.schema.clone(),
            path: Vec::new(),
        },
        object: None,
    };
    let owner = OwnerRef::Editor {
        client_instance_id: client_instance_id.to_owned(),
        editor_session_id: key.clone(),
    };
    Ok(TableControllerRequest { key, target, owner })
}

/// React `tableManualHandle(panelId)`: the live handle only for a session with
/// an active/aborted transaction. A missing view yields `None`; a
/// closing/closed/lost view fails loudly instead of returning a stale handle.
#[allow(dead_code)] // helper; remove with the table view.
pub(crate) fn table_manual_handle(
    registry: &QueryPanelSessionRegistry,
    panel_id: &str,
) -> Result<Option<SessionHandle>, TableSessionError> {
    let Some(view) = registry
        .peek(&table_session_key(panel_id))
        .and_then(|controller| controller.snapshot())
    else {
        return Ok(None);
    };
    if view.state.refuses_reuse() {
        return Err(TableSessionError::SessionLost);
    }
    Ok(matches!(
        view.observed_context.transaction_state,
        TransactionState::Active | TransactionState::Aborted
    )
    .then(|| view.handle.clone()))
}
