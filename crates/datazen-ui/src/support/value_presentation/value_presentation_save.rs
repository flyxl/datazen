//! Port of `packages/frontend/src/lib/valuePresentation/valuePresentationSave.ts`
//! (issue-32 T11) — the path-scoped update form of valuePresentation
//! saves.
//!
//! The update type and [`apply_value_presentation_update`] landed with
//! T037 (columnOverrides passes an updater into `persist`); this task
//! (T310) fills in the path helpers [`with_timestamp_renderer`] and
//! [`with_value_presentation_rules`].
//!
//! The problem being solved: a stored-settings object rewritten wholesale
//! from one window's stale base silently reverts fields another window
//! just saved. The adjudication: saves pass an updater
//! `(fresh) => next`; the store re-reads the authoritative value before
//! saving and the updater replaces only the paths its caller owns (one
//! renderer entry, the rule list, or one column override) — everything
//! else comes from `fresh` verbatim. The residual race window shrinks
//! from "the whole local base is stale" to the milliseconds between
//! re-read and save. Unknown top-level versions keep their fields (only
//! the version-1 path helpers write; whole-object updates pass through
//! untouched).

use crate::support::value_presentation::settings::StoredValuePresentationSettings;

/// A save's input: the old whole-object form (wholesale replacement) or
/// the updater form (own paths only, everything else from `fresh`). The
/// lifetime is the updater's captured borrow (the source identity).
pub enum ValuePresentationSettingsUpdate<'a> {
    Whole(StoredValuePresentationSettings),
    Apply(
        Box<
            dyn FnOnce(StoredValuePresentationSettings) -> StoredValuePresentationSettings + 'a,
        >,
    ),
}

/// Resolve one save input into the next stored value (objects pass
/// through; updaters apply to `fresh`).
pub fn apply_value_presentation_update(
    fresh: StoredValuePresentationSettings,
    update: ValuePresentationSettingsUpdate<'_>,
) -> StoredValuePresentationSettings {
    match update {
        ValuePresentationSettingsUpdate::Whole(next) => next,
        ValuePresentationSettingsUpdate::Apply(updater) => updater(fresh),
    }
}

use crate::support::value_presentation::settings::StoredRendererSettings;
use crate::support::value_presentation::timestamp_contribution::TIMESTAMP_CONTRIBUTION_ID;
use crate::support::value_presentation::value_presentation_rules::ValuePresentationRule;

/// Replace only the `datazen.timestamp` renderer entry (enabled +
/// options in their stored JSON shape): every other renderer entry, the
/// rules, the column overrides, the version and all unknown fields come
/// from `fresh` verbatim — an unknown version never loses its fields.
pub fn with_timestamp_renderer(
    fresh: &StoredValuePresentationSettings,
    enabled: bool,
    options: &serde_json::Value,
) -> StoredValuePresentationSettings {
    let mut next = fresh.clone();
    next.renderers.insert(
        TIMESTAMP_CONTRIBUTION_ID.to_string(),
        StoredRendererSettings { enabled, options: options.clone() },
    );
    next
}

/// Replace the global rule list wholesale (a list has no per-item
/// identity to merge by — last writer wins, and the panel prompts on
/// external updates with unsaved edits). Only version 1 writes `rules`;
/// an unknown version returns `fresh` verbatim (never interpreted, never
/// written back).
pub fn with_value_presentation_rules(
    fresh: &StoredValuePresentationSettings,
    rules: &[ValuePresentationRule],
) -> StoredValuePresentationSettings {
    if fresh.version != 1 {
        return fresh.clone();
    }
    let mut next = fresh.clone();
    next.rules = Some(rules.to_vec());
    next
}
