//! Pure Query-Builder context helpers ported from the `QueryEditorSection.tsx`
//! `qbContextKey` / `useLayoutEffect` rebind logic (see the qbContext test).
//!
//! React JSON.stringifies the connection/session/database/schema tuple to key
//! the Pro contribution's draft. Rune keeps the same key as a string and the
//! host stores the last bound key; the view emits `OpenVisualBuilder` again
//! whenever an open builder's context no longer matches the bound one.

use super::{QueryEditorAction, QueryEditorSectionVm};

/// React `JSON.stringify([connectionId ?? '', dbSessionId, selectedDatabase ?? '', selectedSchema ?? ''])`.
#[allow(dead_code)] // pure helper; consumed by the view and tests.
pub(crate) fn qb_context_key(
    connection_id: &str,
    db_session_id: &str,
    database: Option<&str>,
    schema: Option<&str>,
) -> String {
    let parts = [
        connection_id,
        db_session_id,
        database.unwrap_or(""),
        schema.unwrap_or(""),
    ];
    let body = parts
        .iter()
        .map(|part| format!("\"{part}\""))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

/// React `openPanelId === panelId`.
#[allow(dead_code)] // pure helper; consumed by the view and tests.
pub(crate) fn qb_is_open(vm: &QueryEditorSectionVm) -> bool {
    vm.qb_open_panel.as_deref() == Some(vm.panel_id.as_str())
}

/// The useLayoutEffect gate: a builder open for this panel whose bound context
/// is absent or stale must be rebound before the new database/schema paints.
#[allow(dead_code)] // pure helper; consumed by the view and tests.
pub(crate) fn qb_needs_rebind(vm: &QueryEditorSectionVm) -> bool {
    if !qb_is_open(vm) || !vm.query_builder_available {
        return false;
    }
    let current = qb_context_key(
        &vm.connection_id,
        &vm.db_session_id,
        vm.selected_database.as_deref(),
        vm.selected_schema.as_deref(),
    );
    vm.qb_bound_context_key.as_deref() != Some(current.as_str())
}

/// The host calls this on each rebuild while a builder is open, exactly where
/// React's `useLayoutEffect` runs. The returned intent is the `openFor` rebind;
/// the host performs it and records the new bound key in its VM.
#[allow(dead_code)] // host hook; removed with the query-builder host wiring.
pub(crate) fn qb_rebind_action(vm: &QueryEditorSectionVm) -> Option<QueryEditorAction> {
    if !qb_needs_rebind(vm) {
        return None;
    }
    Some(QueryEditorAction::OpenVisualBuilder {
        panel_id: vm.panel_id.clone(),
        context_key: qb_context_key(
            &vm.connection_id,
            &vm.db_session_id,
            vm.selected_database.as_deref(),
            vm.selected_schema.as_deref(),
        ),
    })
}
