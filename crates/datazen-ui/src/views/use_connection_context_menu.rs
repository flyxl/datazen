//! Port of `windows/connection/useConnectionContextMenu.ts` — the schema-tree
//! node context-menu hook — as pure decision logic (T608).
//!
//! React's hook derives a handful of capability flags from the driver meta and
//! the saved connection, then asks `buildSchemaTreeContextMenuItems` for the
//! exact item set/order and wires each item to a host handler. This port keeps
//! those *decisions*: [`derive_context_flags`] computes the guards and the
//! [`menu`] module decides the items; the danger SQL is built by pure helpers.
//! The host owns the menu surface, confirm dialog, clipboard and IPC — no fake
//! menu, promise or timer lives here.
//!
//! ## Consumer / removal stage
//!
//! No production host wires these decisions yet; the connection view will feed
//! the context and apply the rendered menu. Every reusable item carries a
//! per-item `#[allow(dead_code)]` with this stage.

mod menu;

pub(crate) use menu::{
    build_menu, entry_ids, FeatureFlags, MenuArgs, MenuEntry, MenuKind,
};

/// The hook's capability inputs (driver meta + saved connection + safe mode).
#[allow(dead_code)] // VM mirror; supplied by the host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ContextMetaVm {
    pub driver_read_only: bool,
    pub saved_read_only: bool,
    /// `canOpenStructureEditor(meta)` from the host capability layer.
    pub can_open_structure_editor: bool,
    /// `supportsErDiagram !== false` — defaults to shown.
    pub supports_er_diagram: bool,
    pub supports_any_export: bool,
    pub supports_full_table_export: bool,
}

/// The flags the hook passes into the menu builder.
#[allow(dead_code)] // VM mirror; consumed by the builder and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ContextFlags {
    pub read_only: bool,
    pub show_open_structure: bool,
    pub show_er_focus: bool,
    /// `kind === 'table' ? false : ctxExportDataSupported` for the single export.
    pub show_export: Option<bool>,
    pub show_batch_export: Option<bool>,
    /// `ctxShowStructureEditor` gates new-table on database/blank.
    pub show_new_table: bool,
}

/// React's flag derivation in `handleNodeContextMenu`: read-only wins over the
/// capability, ER focus defaults on, table export is suppressed and batch
/// export follows the driver's full-table capability.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn derive_context_flags(meta: &ContextMetaVm) -> ContextFlags {
    let read_only = meta.driver_read_only || meta.saved_read_only;
    ContextFlags {
        read_only,
        show_open_structure: true,
        show_er_focus: meta.supports_er_diagram,
        // React `showExport: kind === 'table' ? false : ctxExportDataSupported`;
        // the per-kind override is applied by the caller.
        show_export: None,
        // React forces `showBatchExport: kind === 'table' ? false : supported`.
        // The safe default for a kind-agnostic derivation is false; callers for
        // non-table kinds use `show_batch_export_for_kind`.
        show_batch_export: Some(false),
        show_new_table: meta.can_open_structure_editor && !read_only,
    }
}

/// React `showExport` for a concrete kind: tables always suppress the single
/// export; other kinds follow `supportsAnyExport`.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn show_export_for_kind(meta: &ContextMetaVm, kind: MenuKind) -> Option<bool> {
    if kind == MenuKind::Table {
        Some(false)
    } else {
        Some(meta.supports_any_export)
    }
}

/// React `useConnectionContextMenu.ts:327`: tables always suppress batch
/// export; other kinds follow `supportsFullTableExport`.
#[allow(dead_code)] // pure rule; remove with the host wiring.
pub(crate) fn show_batch_export_for_kind(meta: &ContextMetaVm, kind: MenuKind) -> Option<bool> {
    if kind == MenuKind::Table {
        Some(false)
    } else {
        Some(meta.supports_full_table_export)
    }
}

/// React's destructive SQL builders: `escapeIdent(name, type)` is host-supplied
/// as `quoted`; the dialect's `getTruncateTableSql` falls back to the standard
/// statement, and DROP is chosen by node kind.
#[allow(dead_code)] // pure helper; remove with the host wiring.
pub(crate) fn truncate_sql(quoted: &str, dialect_sql: Option<String>) -> String {
    dialect_sql.unwrap_or_else(|| format!("TRUNCATE TABLE {quoted}"))
}

#[allow(dead_code)] // pure helper; remove with the host wiring.
pub(crate) fn drop_sql(quoted: &str, is_view: bool) -> String {
    if is_view {
        format!("DROP VIEW {quoted}")
    } else {
        format!("DROP TABLE {quoted}")
    }
}

/// React's `confirmAndRun` gate: the dialog must be approved before any SQL is
/// executed; a decline is a no-op. The host performs the actual query.
#[allow(dead_code)] // pure guard; remove with the host wiring.
pub(crate) fn may_run_dangerous(read_only: bool, safe_mode: bool, approved: bool) -> bool {
    !read_only && !safe_mode && approved
}
