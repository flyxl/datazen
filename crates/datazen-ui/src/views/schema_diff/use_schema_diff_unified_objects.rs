//! Port of `packages/frontend/src/windows/schema-diff/
//! useSchemaDiffUnifiedObjects.ts` (T420).
//!
//! The unified (non-table) object catalogs for the wizard's objects
//! step: the six kind walks per endpoint, the opt-in selection state
//! over identity keys, and the fixed lifecycle evidence
//! (`never-started` / `loading` / `completed` / `invalidated`) that
//! distinguishes an empty catalog from a load that never ran or whose
//! endpoint owner was invalidated before publication.
//!
//! ## What maps where
//!
//! - `listObjects` walks [`OBJECT_KINDS`] **sequentially** — one
//!   catalog query in flight per session (some drivers share a single
//!   connection for catalog reads and do not permit overlapping
//!   commands); the host performs the `getDatabaseObjects` reads and
//!   feeds the answers into [`list_objects_fold`], which applies the
//!   same order and the same per-kind error capture.
//! - `normalizeObject` is [`normalize_object`]: the kind comes from
//!   the walk (not the row), the driver's optional
//!   `signature`/`targetSchema`/`targetName` survive, everything else
//!   is `?? null`.
//! - The `requestIdRef` staleness gate is [`begin_load`] /
//!   [`on_load_answer`]: a superseded load's answer is dropped whole
//!   (`None`), leaving the newer load's state alone.
//! - `clear` bumps the request id so an in-flight load cannot
//!   repopulate, flips a started lifecycle to `invalidated`, and
//!   empties everything.
//! - `restoreSelection` intersects the desired objects with the
//!   catalog by identity key (deduped, desired order preserved) and
//!   reports the missing counts; `toggle`/`selectAll` keep the
//!   selection as keys in insertion order (React's `Set` iteration
//!   order), and `selected*Objects` derives the object lists.
//!
//! Known deltas: `Promise.all` over the two endpoints becomes the
//! host's parallel reads; the fold itself is order-independent. The
//! picker's [`ObjectLoadErrors`] (`Vec<(kind, message)>`) is the
//! established Rust face of the React per-kind error map (one entry
//! per failed kind).

use crate::views::schema_diff::schema_diff_object_identity::{
    SchemaDiffObjectIdentityVm, SchemaDiffObjectKindVm, schema_diff_object_identity_key,
};
use crate::views::schema_diff::schema_diff_unified_objects_picker::ObjectLoadErrors;

pub(crate) use datazen_driver_api::DatabaseObject;

/// The kind walk order (`OBJECT_KINDS`).
pub(crate) const OBJECT_KINDS: [SchemaDiffObjectKindVm; 6] = [
    SchemaDiffObjectKindVm::View,
    SchemaDiffObjectKindVm::Type,
    SchemaDiffObjectKindVm::Sequence,
    SchemaDiffObjectKindVm::Function,
    SchemaDiffObjectKindVm::Procedure,
    SchemaDiffObjectKindVm::Trigger,
];

/// `normalizeObject`: the row becomes an identity under the walk's
/// kind; the driver's optional trigger/routine fields survive, the
/// rest is `?? null`.
pub(crate) fn normalize_object(
    object: &DatabaseObject,
    kind: SchemaDiffObjectKindVm,
) -> SchemaDiffObjectIdentityVm {
    SchemaDiffObjectIdentityVm {
        kind,
        schema: object.schema.clone(),
        name: object.name.clone(),
        signature: object.signature.clone(),
        target_schema: object.target_schema.clone(),
        target_name: object.target_name.clone(),
    }
}

/// One endpoint's `listObjects` fold: the per-kind answers (in walk
/// order) become the identity list plus the per-kind error pairs —
/// a failed kind's rows are simply not reported.
pub(crate) fn list_objects_fold(
    answers: &[(SchemaDiffObjectKindVm, Result<Vec<DatabaseObject>, String>)],
) -> (Vec<SchemaDiffObjectIdentityVm>, ObjectLoadErrors) {
    let mut objects = Vec::new();
    let mut errors: ObjectLoadErrors = Vec::new();
    for (kind, answer) in answers {
        match answer {
            Ok(rows) => objects.extend(rows.iter().map(|row| normalize_object(row, *kind))),
            Err(message) => errors.push((*kind, message.clone())),
        }
    }
    (objects, errors)
}

/// The lifecycle evidence (`catalogLoadState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CatalogLoadState {
    #[default]
    NeverStarted,
    Loading,
    Completed,
    Invalidated,
}

/// The side a selection belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ObjectSide {
    Source,
    Target,
}

/// `SchemaDiffObjectCatalog`.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SchemaDiffObjectCatalog {
    pub(crate) source_objects: Vec<SchemaDiffObjectIdentityVm>,
    pub(crate) target_objects: Vec<SchemaDiffObjectIdentityVm>,
}

/// `SchemaDiffObjectSelectionRestore`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SchemaDiffObjectSelectionRestore {
    pub(crate) missing_source: usize,
    pub(crate) missing_target: usize,
}

/// The two endpoints' load errors (`SchemaDiffObjectLoadErrors`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct SchemaDiffObjectLoadErrors {
    pub(crate) source: ObjectLoadErrors,
    pub(crate) target: ObjectLoadErrors,
}

/// `useSchemaDiffUnifiedObjects()`'s state machine.
pub(crate) struct UseSchemaDiffUnifiedObjects {
    source_objects: Vec<SchemaDiffObjectIdentityVm>,
    target_objects: Vec<SchemaDiffObjectIdentityVm>,
    selected_source_keys: Vec<String>,
    selected_target_keys: Vec<String>,
    errors: SchemaDiffObjectLoadErrors,
    request_id: u64,
    catalog_load_state: CatalogLoadState,
}

impl Default for UseSchemaDiffUnifiedObjects {
    fn default() -> Self {
        Self::new()
    }
}

impl UseSchemaDiffUnifiedObjects {
    pub(crate) fn new() -> Self {
        Self {
            source_objects: Vec::new(),
            target_objects: Vec::new(),
            selected_source_keys: Vec::new(),
            selected_target_keys: Vec::new(),
            errors: SchemaDiffObjectLoadErrors::default(),
            request_id: 0,
            catalog_load_state: CatalogLoadState::NeverStarted,
        }
    }

    /// `catalogLoadState`.
    pub(crate) fn catalog_load_state(&self) -> CatalogLoadState {
        self.catalog_load_state
    }

    /// `sourceObjects` / `targetObjects`.
    pub(crate) fn source_objects(&self) -> &[SchemaDiffObjectIdentityVm] {
        &self.source_objects
    }

    pub(crate) fn target_objects(&self) -> &[SchemaDiffObjectIdentityVm] {
        &self.target_objects
    }

    /// `selectedSourceKeys` / `selectedTargetKeys` (insertion order).
    pub(crate) fn selected_source_keys(&self) -> &[String] {
        &self.selected_source_keys
    }

    pub(crate) fn selected_target_keys(&self) -> &[String] {
        &self.selected_target_keys
    }

    /// `selectedSourceObjects` / `selectedTargetObjects`: the selected
    /// objects, in catalog order.
    pub(crate) fn selected_source_objects(&self) -> Vec<SchemaDiffObjectIdentityVm> {
        self.source_objects
            .iter()
            .filter(|object| self.selected_source_keys.contains(&schema_diff_object_identity_key(object)))
            .cloned()
            .collect()
    }

    pub(crate) fn selected_target_objects(&self) -> Vec<SchemaDiffObjectIdentityVm> {
        self.target_objects
            .iter()
            .filter(|object| self.selected_target_keys.contains(&schema_diff_object_identity_key(object)))
            .cloned()
            .collect()
    }

    /// `errors`.
    pub(crate) fn errors(&self) -> &SchemaDiffObjectLoadErrors {
        &self.errors
    }

    /// `load`'s entry: mint the request id and mark the lifecycle
    /// loading. The host walks the kinds per endpoint and calls
    /// [`on_load_answer`] with this id.
    pub(crate) fn begin_load(&mut self) -> u64 {
        self.request_id += 1;
        self.catalog_load_state = CatalogLoadState::Loading;
        self.request_id
    }

    /// `load`'s completion: install both endpoints' answers. A
    /// superseded request's answer is dropped whole (`None`) — its
    /// state changes nothing. Newly discovered schema objects are
    /// opt-in: the selections reset (this preserves the existing
    /// table-only migration scope and prevents surprise DDL).
    pub(crate) fn on_load_answer(
        &mut self,
        request_id: u64,
        source: (Vec<SchemaDiffObjectIdentityVm>, ObjectLoadErrors),
        target: (Vec<SchemaDiffObjectIdentityVm>, ObjectLoadErrors),
    ) -> Option<SchemaDiffObjectCatalog> {
        if request_id != self.request_id {
            return None;
        }
        self.catalog_load_state = CatalogLoadState::Completed;
        self.source_objects = source.0.clone();
        self.target_objects = target.0.clone();
        self.selected_source_keys.clear();
        self.selected_target_keys.clear();
        self.errors = SchemaDiffObjectLoadErrors { source: source.1, target: target.1 };
        Some(SchemaDiffObjectCatalog {
            source_objects: source.0,
            target_objects: target.0,
        })
    }

    /// `clear`: bump the request id (an in-flight load can no longer
    /// publish), flip a started lifecycle to `invalidated`, empty
    /// everything. A never-started lifecycle stays `never-started`.
    pub(crate) fn clear(&mut self) {
        self.request_id += 1;
        self.catalog_load_state = match self.catalog_load_state {
            CatalogLoadState::NeverStarted => CatalogLoadState::NeverStarted,
            _ => CatalogLoadState::Invalidated,
        };
        self.source_objects.clear();
        self.target_objects.clear();
        self.selected_source_keys.clear();
        self.selected_target_keys.clear();
        self.errors = SchemaDiffObjectLoadErrors::default();
    }

    /// `restoreSelection`: intersect the desired objects with the
    /// catalog by identity key (deduped, desired order) and report
    /// how many were missing per side.
    pub(crate) fn restore_selection(
        &mut self,
        catalog: &SchemaDiffObjectCatalog,
        desired_source: &[SchemaDiffObjectIdentityVm],
        desired_target: &[SchemaDiffObjectIdentityVm],
    ) -> SchemaDiffObjectSelectionRestore {
        let restore_side = |desired: &[SchemaDiffObjectIdentityVm], catalog: &[SchemaDiffObjectIdentityVm]| -> (Vec<String>, usize) {
            let known: std::collections::HashSet<String> =
                catalog.iter().map(schema_diff_object_identity_key).collect();
            let mut selected: Vec<String> = Vec::new();
            for object in desired {
                let key = schema_diff_object_identity_key(object);
                if known.contains(&key) && !selected.contains(&key) {
                    selected.push(key);
                }
            }
            let missing = desired.len() - selected.len();
            (selected, missing)
        };
        let (source_keys, missing_source) = restore_side(desired_source, &catalog.source_objects);
        let (target_keys, missing_target) = restore_side(desired_target, &catalog.target_objects);
        self.selected_source_keys = source_keys;
        self.selected_target_keys = target_keys;
        SchemaDiffObjectSelectionRestore { missing_source, missing_target }
    }

    /// `toggle(side, object)`: insert or remove the identity key,
    /// keeping insertion order.
    pub(crate) fn toggle(&mut self, side: ObjectSide, object: &SchemaDiffObjectIdentityVm) {
        let keys = match side {
            ObjectSide::Source => &mut self.selected_source_keys,
            ObjectSide::Target => &mut self.selected_target_keys,
        };
        let key = schema_diff_object_identity_key(object);
        match keys.iter().position(|k| *k == key) {
            Some(index) => {
                keys.remove(index);
            }
            None => keys.push(key),
        }
    }

    /// `selectAll(side, selected)`: every catalog key, or none.
    pub(crate) fn select_all(&mut self, side: ObjectSide, selected: bool) {
        let (objects, keys) = match side {
            ObjectSide::Source => (&self.source_objects, &mut self.selected_source_keys),
            ObjectSide::Target => (&self.target_objects, &mut self.selected_target_keys),
        };
        *keys = if selected {
            objects.iter().map(schema_diff_object_identity_key).collect()
        } else {
            Vec::new()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn object(kind: SchemaDiffObjectKindVm, name: &str, schema: Option<&str>) -> DatabaseObject {
        DatabaseObject {
            kind: kind.wire_str().to_owned(),
            schema: schema.map(str::to_owned),
            name: name.to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    fn identity(kind: SchemaDiffObjectKindVm, name: &str, schema: Option<&str>) -> SchemaDiffObjectIdentityVm {
        SchemaDiffObjectIdentityVm {
            kind,
            schema: schema.map(str::to_owned),
            name: name.to_owned(),
            signature: None,
            target_schema: None,
            target_name: None,
        }
    }

    #[test]
    fn the_kind_walk_is_the_react_order() {
        assert_eq!(
            OBJECT_KINDS.map(|kind| kind.wire_str()),
            ["view", "type", "sequence", "function", "procedure", "trigger"]
        );
    }

    #[test]
    fn normalize_takes_the_kind_from_the_walk_and_survives_the_optionals() {
        let mut trigger_row = object(SchemaDiffObjectKindVm::Trigger, "trg", Some("dbo"));
        trigger_row.target_schema = Some("dbo".to_owned());
        trigger_row.target_name = Some("orders".to_owned());
        let normalized = normalize_object(&trigger_row, SchemaDiffObjectKindVm::Trigger);
        assert_eq!(normalized.kind, SchemaDiffObjectKindVm::Trigger);
        assert_eq!(normalized.schema.as_deref(), Some("dbo"));
        assert_eq!(normalized.target_name.as_deref(), Some("orders"));
        // The row's own `kind` string is not consulted.
        let odd_row = object(SchemaDiffObjectKindVm::View, "v", None);
        let normalized = normalize_object(&odd_row, SchemaDiffObjectKindVm::Sequence);
        assert_eq!(normalized.kind, SchemaDiffObjectKindVm::Sequence);
        // Absent optionals are null.
        assert_eq!(normalized.signature, None);
        assert_eq!(normalized.target_schema, None);
    }

    #[test]
    fn the_fold_collects_rows_and_captures_per_kind_errors() {
        let answers = vec![
            (SchemaDiffObjectKindVm::View, Ok(vec![object(SchemaDiffObjectKindVm::View, "v1", None)])),
            (SchemaDiffObjectKindVm::Type, Err("no permission".to_owned())),
            (SchemaDiffObjectKindVm::Function, Ok(vec![object(SchemaDiffObjectKindVm::Function, "f1", Some("public"))])),
        ];
        let (objects, errors) = list_objects_fold(&answers);
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].name, "v1");
        assert_eq!(objects[1].name, "f1");
        // A failed kind is one error entry; its rows are simply absent.
        assert_eq!(errors, vec![(SchemaDiffObjectKindVm::Type, "no permission".to_owned())]);
    }

    #[test]
    fn a_load_completes_the_lifecycle_and_resets_the_selection() {
        let mut hook = UseSchemaDiffUnifiedObjects::new();
        assert_eq!(hook.catalog_load_state(), CatalogLoadState::NeverStarted);
        hook.toggle(ObjectSide::Source, &identity(SchemaDiffObjectKindVm::View, "stale", None));
        let request = hook.begin_load();
        assert_eq!(hook.catalog_load_state(), CatalogLoadState::Loading);
        let source = (
            vec![identity(SchemaDiffObjectKindVm::View, "v1", None)],
            vec![(SchemaDiffObjectKindVm::Type, "boom".to_owned())],
        );
        let target = (vec![identity(SchemaDiffObjectKindVm::Sequence, "s1", None)], Vec::new());
        let catalog = hook.on_load_answer(request, source, target).expect("publishes");
        assert_eq!(catalog.source_objects.len(), 1);
        assert_eq!(hook.catalog_load_state(), CatalogLoadState::Completed);
        assert_eq!(hook.source_objects().len(), 1);
        assert_eq!(hook.errors().source.len(), 1);
        // Opt-in: the pre-load selection did not survive.
        assert!(hook.selected_source_keys().is_empty());
    }

    #[test]
    fn a_superseded_answer_is_dropped_whole() {
        let mut hook = UseSchemaDiffUnifiedObjects::new();
        let first = hook.begin_load();
        let second = hook.begin_load();
        let payload = (vec![identity(SchemaDiffObjectKindVm::View, "v", None)], Vec::new());
        assert!(hook.on_load_answer(first, payload.clone(), payload.clone()).is_none());
        assert_eq!(hook.catalog_load_state(), CatalogLoadState::Loading);
        assert!(hook.source_objects().is_empty(), "the stale answer published nothing");
        assert!(hook.on_load_answer(second, payload.clone(), payload).is_some());
        assert_eq!(hook.catalog_load_state(), CatalogLoadState::Completed);
    }

    #[test]
    fn clear_invalidates_a_started_lifecycle_and_empties_everything() {
        let mut hook = UseSchemaDiffUnifiedObjects::new();
        // A never-started lifecycle stays never-started.
        hook.clear();
        assert_eq!(hook.catalog_load_state(), CatalogLoadState::NeverStarted);
        // After a load: invalidated, everything emptied.
        let request = hook.begin_load();
        let payload = (vec![identity(SchemaDiffObjectKindVm::View, "v", None)], Vec::new());
        hook.on_load_answer(request, payload.clone(), payload).expect("publishes");
        hook.select_all(ObjectSide::Source, true);
        hook.clear();
        assert_eq!(hook.catalog_load_state(), CatalogLoadState::Invalidated);
        assert!(hook.source_objects().is_empty());
        assert!(hook.selected_source_keys().is_empty());
        assert!(hook.errors().source.is_empty());
        // And the in-flight load can no longer publish (its id is
        // superseded by the clear's bump).
        let replay = (vec![identity(SchemaDiffObjectKindVm::View, "v", None)], Vec::new());
        assert!(hook.on_load_answer(request, replay, (Vec::new(), Vec::new())).is_none());
        let _ = payload;
    }

    #[test]
    fn restore_selection_intersects_dedupes_and_counts_the_missing() {
        let mut hook = UseSchemaDiffUnifiedObjects::new();
        let catalog = SchemaDiffObjectCatalog {
            source_objects: vec![
                identity(SchemaDiffObjectKindVm::View, "v1", None),
                identity(SchemaDiffObjectKindVm::View, "v2", None),
            ],
            target_objects: vec![identity(SchemaDiffObjectKindVm::Sequence, "s1", None)],
        };
        // The catalog is what a fresh load published (the React
        // selectedSourceObjects derive over the LOADED objects, so
        // the hook must hold them before the restore).
        let request = hook.begin_load();
        hook.on_load_answer(
            request,
            (catalog.source_objects.clone(), Vec::new()),
            (catalog.target_objects.clone(), Vec::new()),
        )
        .expect("publishes");
        // Desired: one present twice (dedup), one absent, plus a
        // target miss.
        let desired_source = vec![
            identity(SchemaDiffObjectKindVm::View, "v1", None),
            identity(SchemaDiffObjectKindVm::View, "v1", None),
            identity(SchemaDiffObjectKindVm::View, "gone", None),
        ];
        let desired_target = vec![
            identity(SchemaDiffObjectKindVm::Sequence, "s1", None),
            identity(SchemaDiffObjectKindVm::Sequence, "missing", None),
        ];
        let restore = hook.restore_selection(&catalog, &desired_source, &desired_target);
        // React's arithmetic: desired.length - selected.length — a
        // duplicated desired entry counts toward missing (3 - 1 = 2).
        assert_eq!(restore.missing_source, 2);
        assert_eq!(restore.missing_target, 1);
        assert_eq!(hook.selected_source_keys().len(), 1);
        assert_eq!(hook.selected_source_objects().len(), 1);
        assert_eq!(hook.selected_source_objects()[0].name, "v1");
        assert_eq!(hook.selected_target_keys().len(), 1);
    }

    #[test]
    fn toggle_flips_membership_in_insertion_order() {
        let mut hook = UseSchemaDiffUnifiedObjects::new();
        let v1 = identity(SchemaDiffObjectKindVm::View, "v1", None);
        let v2 = identity(SchemaDiffObjectKindVm::View, "v2", None);
        hook.toggle(ObjectSide::Source, &v1);
        hook.toggle(ObjectSide::Source, &v2);
        assert_eq!(hook.selected_source_keys().len(), 2);
        hook.toggle(ObjectSide::Source, &v1);
        assert_eq!(hook.selected_source_keys().len(), 1);
        // The sides are independent.
        assert!(hook.selected_target_keys().is_empty());
    }

    #[test]
    fn select_all_takes_the_whole_side_or_nothing() {
        let mut hook = UseSchemaDiffUnifiedObjects::new();
        let request = hook.begin_load();
        let payload = (
            vec![
                identity(SchemaDiffObjectKindVm::View, "v1", None),
                identity(SchemaDiffObjectKindVm::View, "v2", None),
            ],
            Vec::new(),
        );
        hook.on_load_answer(request, payload.clone(), payload).expect("publishes");
        hook.select_all(ObjectSide::Source, true);
        assert_eq!(hook.selected_source_keys().len(), 2);
        assert_eq!(hook.selected_source_objects().len(), 2);
        hook.select_all(ObjectSide::Source, false);
        assert!(hook.selected_source_keys().is_empty());
        assert!(hook.selected_source_objects().is_empty());
    }
}
