//! Port of `packages/frontend/src/lib/valuePresentation/valuePresentationSave.ts`
//! (issue-32 T11) — the path-scoped update form of valuePresentation
//! saves.
//!
//! **Landing in two steps (documented split):** T037 (columnOverrides)
//! passes an *updater* into `persist`, so this file's update type and
//! [`apply_value_presentation_update`] land with it. The path helpers
//! `with_timestamp_renderer` and `with_value_presentation_rules` are
//! valuePresentationSave's own logic and land with T310.
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
pub(crate) enum ValuePresentationSettingsUpdate<'a> {
    Whole(StoredValuePresentationSettings),
    Apply(
        Box<
            dyn FnOnce(StoredValuePresentationSettings) -> StoredValuePresentationSettings + 'a,
        >,
    ),
}

/// Resolve one save input into the next stored value (objects pass
/// through; updaters apply to `fresh`).
pub(crate) fn apply_value_presentation_update(
    fresh: StoredValuePresentationSettings,
    update: ValuePresentationSettingsUpdate<'_>,
) -> StoredValuePresentationSettings {
    match update {
        ValuePresentationSettingsUpdate::Whole(next) => next,
        ValuePresentationSettingsUpdate::Apply(updater) => updater(fresh),
    }
}
