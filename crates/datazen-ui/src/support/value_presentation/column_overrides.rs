//! Port of `packages/frontend/src/lib/valuePresentation/columnOverrides.ts`
//! (issue-32 T10, design §5/§6, PRD §5/§6 acceptance 6) — table-column
//! override persistence.
//!
//! The stored shape stays version 1 with the optional `columnOverrides`
//! field (the T08/T10 adjudication: additive optional fields; a version
//! bump would force un-migrated v2 payloads down the
//! unknown-version-→-empty-snapshot branch and *lose* configuration).
//!
//! - **Keys** are [`column_override_key`] strings: the JSON 5-tuple of
//!   connectionId / database / namespace / table / column. The key *is*
//!   the source identity — no panel or session ids, and the stable
//!   ConnectionConfig.id only (current user/org scoping is the storage
//!   layer's job, never a client-supplied user id).
//! - **Case** is never folded; missing fields are null and empty strings
//!   stay distinct (the BUG-004 lesson).
//! - **Values** store `raw` / `custom` only — `inherit` means no entry:
//!   the lenient parse drops inherit entries, and saving inherit deletes
//!   the column's entry (restore-by-deletion, PRD acceptance 4).
//! - **Cap**: [`MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES`] entries, first
//!   come first served; over the cap the save is refused, never silently
//!   truncated.
//! - **Unknown top-level versions** are never parsed and never written
//!   (the original stays in the store).
//!
//! ## The save road ([`save_column_override`])
//!
//! 1. No source (a query-result column) → `no-source`; the panel's save
//!    button is already disabled — this is the defensive re-check.
//! 2. Invalid override → `invalid-override`.
//! 3. Version ≠ 1 → `unknown-version`.
//! 4. Over the cap with a new key → `too-many-overrides`.
//! 5. Optimistic session write (`set_column_override`): a failed persist
//!    keeps the user's fresh choice; inherit deletes at the session
//!    layer (nothing to keep — the draft survives, the save is retryable).
//! 6. Persist via the injected callback with the **updater** form
//!    (T11): the store re-reads the authoritative value and applies only
//!    this column's override — two windows saving simultaneously never
//!    revert each other's fields. Failure → `save-failed` (the old
//!    config keeps working, the session entry stays, the panel reports
//!    in place).
//! 7. On success (or a no-change save) the same key's session entry is
//!    cleared so it cannot shadow the persisted value.
//!
//! # What maps where
//!
//! - The async `persist` callback becomes `FnMut(...) -> Result<(), ()>`
//!   (the only thing callers can observe is success vs failure).
//! - React's `next === stored` reference check becomes structural
//!   equality (compare the serde forms — the canonical shapes are
//!   deterministic).
//! - The module-level session singleton becomes an explicit
//!   `&mut ColumnPresentationSession` parameter (ownership replaces the
//!   global).
//! - The settings/save *types* this module leans on live in their home
//!   files as documented skeletons; their own logic lands with T182/T310.

use std::collections::BTreeMap;

use crate::support::value_presentation::column_presentation::ColumnSourceBase;
use crate::support::value_presentation::column_presentation::{
    ColumnPresentationOverrideValidation, validate_column_presentation_override,
};
use crate::support::value_presentation::column_presentation_session::ColumnPresentationSession;
use crate::support::value_presentation::contracts::ColumnPresentationOverride;
use crate::support::value_presentation::presentation_plan::column_override_key;
use crate::support::value_presentation::settings::StoredValuePresentationSettings;
use crate::support::value_presentation::value_presentation_save::ValuePresentationSettingsUpdate;

/// The column-override entry cap (storage-volume guard; over the cap the
/// save is refused, never silently truncated).
pub(crate) const MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES: usize = 200;

/// The override key's shape check: a JSON array
/// `[connectionId, database|null, namespace|null, table, column]` with
/// string members where required. A malformed key is dropped — the key is
/// source identity, and a misspelled key must never hit the wrong column
/// after a restart.
fn is_valid_column_override_key(key: &str) -> bool {
    let Ok(serde_json::Value::Array(parts)) = serde_json::from_str::<serde_json::Value>(key)
    else {
        return false;
    };
    if parts.len() != 5 {
        return false;
    }
    let is_string = |value: &serde_json::Value| value.is_string();
    let is_string_or_null = |value: &serde_json::Value| value.is_string() || value.is_null();
    is_string(&parts[0])
        && is_string_or_null(&parts[1])
        && is_string_or_null(&parts[2])
        && is_string(&parts[3])
        && is_string(&parts[4])
}

/// Lenient parse of a stored overrides record (storage boundary):
/// non-objects → empty; per entry the key shape and the override value
/// are validated (invalid keys / invalid overrides / `inherit` entries
/// drop — the original stays in the store); at most
/// [`MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES`] valid entries survive.
pub(crate) fn validate_stored_column_overrides(
    input: &serde_json::Value,
) -> BTreeMap<String, ColumnPresentationOverride> {
    let mut overrides: BTreeMap<String, ColumnPresentationOverride> = BTreeMap::new();
    let Some(object) = input.as_object() else {
        return overrides;
    };
    for (key, entry) in object {
        if overrides.len() >= MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES {
            break;
        }
        if !is_valid_column_override_key(key) {
            continue;
        }
        let validated = validate_column_presentation_override(entry);
        let ColumnPresentationOverrideValidation::Ok(override_value) = validated else {
            continue;
        };
        // inherit means no entry: storing one would only mislead the
        // "this column has an override" checks after a restart.
        if matches!(override_value, ColumnPresentationOverride::Inherit) {
            continue;
        }
        overrides.insert(key.clone(), override_value);
    }
    overrides
}

/// Pure: apply one column's override to a stored value, returning the
/// next stored value.
///
/// - version ≠ 1 → the input verbatim (no override field written);
/// - `inherit` → delete the entry (nothing stored → the input verbatim);
/// - `raw` / `custom` → write by key (an invalid override returns the
///   input; over the cap with a new key returns the input — the save
///   path reports `too-many-overrides`);
/// - structurally unchanged → the input verbatim (no phantom save).
pub(crate) fn with_column_override(
    stored: &StoredValuePresentationSettings,
    source: ColumnSourceBase<'_>,
    column_name: &str,
    override_value: &ColumnPresentationOverride,
) -> StoredValuePresentationSettings {
    if stored.version != 1 {
        return stored.clone();
    }
    let key = column_override_key(&source.with_column(column_name));
    let current: BTreeMap<String, ColumnPresentationOverride> =
        stored.column_overrides.clone().unwrap_or_default();
    if matches!(override_value, ColumnPresentationOverride::Inherit) {
        if !current.contains_key(&key) {
            return stored.clone();
        }
        let mut next = current;
        next.remove(&key);
        let mut stored = stored.clone();
        stored.column_overrides = Some(next);
        return stored;
    }
    let validated = validate_column_presentation_override(&serde_json::to_value(override_value)
        .unwrap_or(serde_json::Value::Null));
    let ColumnPresentationOverrideValidation::Ok(validated) = validated else {
        return stored.clone();
    };
    if let Some(existing) = current.get(&key) {
        let existing_json = serde_json::to_value(existing).expect("serializable");
        let validated_json = serde_json::to_value(&validated).expect("serializable");
        if existing_json == validated_json {
            return stored.clone();
        }
    }
    if !current.contains_key(&key) && current.len() >= MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES {
        return stored.clone();
    }
    let mut next = current;
    next.insert(key, validated);
    let mut stored = stored.clone();
    stored.column_overrides = Some(next);
    stored
}

/// `save_column_override`'s failure reasons (the caller reports them in
/// place).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SaveColumnOverrideReason {
    NoSource,
    InvalidOverride,
    UnknownVersion,
    TooManyOverrides,
    SaveFailed,
}

pub(crate) type SaveColumnOverrideResult = Result<(), SaveColumnOverrideReason>;

/// `save_column_override`'s input. `persist` is the injected persistence
/// entry (the settings store's update function); the session is the
/// optimistic-write layer.
pub(crate) struct SaveColumnOverrideInput<'a> {
    pub(crate) stored: &'a StoredValuePresentationSettings,
    /// The table identity; absent for query-result columns (save denied).
    pub(crate) source: Option<ColumnSourceBase<'a>>,
    pub(crate) column_name: &'a str,
    pub(crate) override_value: &'a ColumnPresentationOverride,
    pub(crate) session: &'a mut ColumnPresentationSession,
    pub(crate) persist:
        &'a mut dyn FnMut(ValuePresentationSettingsUpdate) -> Result<(), ()>,
}

/// Save one column's override: optimistic session write → persist via
/// the updater → clear the session entry on success. Never panics; every
/// failure path returns its reason (see the module header's road).
pub(crate) fn save_column_override(
    input: &mut SaveColumnOverrideInput<'_>,
) -> SaveColumnOverrideResult {
    let Some(source) = input.source else {
        return Err(SaveColumnOverrideReason::NoSource);
    };
    let validated =
        validate_column_presentation_override(&serde_json::to_value(input.override_value)
            .unwrap_or(serde_json::Value::Null));
    let validated = match validated {
        ColumnPresentationOverrideValidation::Ok(validated) => validated,
        ColumnPresentationOverrideValidation::Err(_) => {
            return Err(SaveColumnOverrideReason::InvalidOverride);
        }
    };
    if input.stored.version != 1 {
        return Err(SaveColumnOverrideReason::UnknownVersion);
    }
    let key = column_override_key(&source.with_column(input.column_name));
    let current_len = input
        .stored
        .column_overrides
        .as_ref()
        .map(|overrides| overrides.len())
        .unwrap_or(0);
    let is_new_key = !input
        .stored
        .column_overrides
        .as_ref()
        .is_some_and(|overrides| overrides.contains_key(&key));
    if !matches!(validated, ColumnPresentationOverride::Inherit)
        && is_new_key
        && current_len >= MAX_VALUE_PRESENTATION_COLUMN_OVERRIDES
    {
        return Err(SaveColumnOverrideReason::TooManyOverrides);
    }
    // Optimistic session write: before persist, so a failure keeps the
    // user's fresh choice.
    input
        .session
        .set_column_override(source, input.column_name, &validated);

    let next = with_column_override(
        input.stored,
        source,
        input.column_name,
        &validated,
    );
    let changed = serde_json::to_value(&next).expect("serializable")
        != serde_json::to_value(input.stored).expect("serializable");
    if !changed {
        // No change (identical value, or deleting a missing entry): no
        // persistence write, but the optimistic session entry must still
        // be cleared — a residual entry equal to the stored value would
        // shadow the persisted value and inflate the merged snapshot's
        // revision (T10-BUG-001).
        input.session.clear_column_override(source, input.column_name);
        return Ok(());
    }
    // T11: hand the store the "this column's override" path only; the
    // store applies it to its freshly re-read authoritative value (the
    // version and cap re-checks happen inside with_column_override
    // against `fresh`).
    let column_name = input.column_name.to_string();
    let override_value = validated.clone();
    let update = ValuePresentationSettingsUpdate::Apply(Box::new(move |fresh| {
        with_column_override(&fresh, source, &column_name, &override_value)
    }));
    if (input.persist)(update).is_err() {
        return Err(SaveColumnOverrideReason::SaveFailed);
    }
    // Success: clear the same key's session entry so the session value
    // cannot shadow the persisted one.
    input.session.clear_column_override(source, input.column_name);
    Ok(())
}
