//! Panel count strategy from `stores/tableData/countStrategy.ts` (T089).
//!
//! The catalog prior is this file's own rule. The mode → strategy mapping
//! is the contract in `lib/rowCountStrategy.ts` (task T585, segment 2, not
//! landed). It is mirrored here so every panel read can ask for a strategy
//! without waiting on that port. When T585 lands, this function should call
//! that one instead of keeping a second copy.

#![allow(dead_code)]

use crate::table_data_connection_state::CountStrategy;

/// `DEFAULT_ROW_COUNT_MODE`.
pub(crate) const DEFAULT_ROW_COUNT_MODE: RowCountMode = RowCountMode::Auto;
/// `DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD`.
pub(crate) const DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD: f64 = 200_000.0;

/// `RowCountMode` (`types/settings.ts`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowCountMode {
    None,
    Exact,
    Estimated,
    Auto,
}

/// The slice of a schema-store table this lookup reads.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CatalogTable {
    pub name: String,
    pub schema: Option<String>,
    /// `null` / missing becomes "no prior". `NaN` is kept and the auto
    /// mode then declines (`Number.isFinite`).
    pub row_count: Option<f64>,
}

/// Settings the panel reads. `None` mode or threshold falls back to the
/// defaults (`??` in the React call).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct CountSettings {
    pub mode: Option<RowCountMode>,
    pub threshold: Option<f64>,
}

/// `TableChangeContext` fields the lookup uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CountContext {
    pub table: String,
    pub schema: Option<String>,
}

/// `resolvePanelCountStrategy`. `tables` is
/// `getConnectionSchema(dbSessionId)?.tables`; `None` means no schema.
///
/// The return is `None` when the strategy is `undefined` (auto, and the
/// driver should decide).
pub(crate) fn resolve_panel_count_strategy(
    context: &CountContext,
    tables: Option<&[CatalogTable]>,
    settings: &CountSettings,
) -> Option<CountStrategy> {
    resolve_count_strategy(
        settings.mode.unwrap_or(DEFAULT_ROW_COUNT_MODE),
        settings.threshold.unwrap_or(DEFAULT_ROW_COUNT_ESTIMATE_THRESHOLD),
        catalog_row_count(context, tables),
    )
}

/// `catalogRowCount`. First table whose name matches and whose schema is
/// compatible: a missing schema on either side matches anything.
pub(crate) fn catalog_row_count(context: &CountContext, tables: Option<&[CatalogTable]>) -> Option<f64> {
    let tables = tables?;
    let found = tables.iter().find(|info| {
        info.name == context.table && schemas_compatible(&context.schema, &info.schema)
    })?;
    found.row_count
}

/// Mirror of `resolveCountStrategy` (`lib/rowCountStrategy.ts`).
pub(crate) fn resolve_count_strategy(
    mode: RowCountMode,
    threshold: f64,
    known_row_count: Option<f64>,
) -> Option<CountStrategy> {
    match mode {
        RowCountMode::None => return Some(CountStrategy::Unsupported),
        RowCountMode::Exact => return Some(CountStrategy::Exact),
        RowCountMode::Estimated => return Some(CountStrategy::Estimated),
        RowCountMode::Auto => {}
    }
    let Some(known) = known_row_count else {
        return None;
    };
    if !known.is_finite() {
        return None;
    }
    if !threshold.is_finite() || threshold <= 0.0 {
        return Some(CountStrategy::Exact);
    }
    if known > threshold {
        Some(CountStrategy::Estimated)
    } else {
        Some(CountStrategy::Exact)
    }
}

fn schemas_compatible(context_schema: &Option<String>, info_schema: &Option<String>) -> bool {
    schema_missing(context_schema) || schema_missing(info_schema) || context_schema == info_schema
}

/// JS falsy: `undefined`, `null`, `""`.
fn schema_missing(schema: &Option<String>) -> bool {
    match schema {
        None => true,
        Some(value) => value.is_empty(),
    }
}
