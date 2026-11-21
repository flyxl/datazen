//! Pure Query-Builder context helpers ported from the `QueryEditorSection.tsx`
//! `qbContextKey` / `useLayoutEffect` rebind logic (see the qbContext test).
//!
//! React JSON.stringifies the connection/session/database/schema tuple to key
//! the Pro contribution's draft. Rune keeps the same key as a string and the
//! host stores the last bound key; the view emits `OpenVisualBuilder` again
//! whenever an open builder's context no longer matches the bound one.

use super::{QueryEditorAction, QueryEditorSectionVm};

/// The escaping `JSON.stringify` applies to one string element: quote,
/// backslash, the named C0 controls, and `\\u00xx` for every other C0 byte.
/// Non-ASCII is emitted verbatim, as JSON.stringify does.
fn json_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            other if (other as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", other as u32)),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

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
        .map(|part| json_string(part))
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
