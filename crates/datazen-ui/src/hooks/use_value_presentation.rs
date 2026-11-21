//! Port of `packages/frontend/src/hooks/useValuePresentation.ts` (T445)
//! — the host presentation pipeline wiring hook (issue-32 T04,
//! design §7).
//!
//! 面板层共享 hook：列计划通过本 hook 产出，再由 T05 经 props /
//! context 注入 DataTable 与 DetailPanel；组件本身不直接读 Store
//! 或 IPC。
//!
//! ## What maps where
//!
//! - The `useSyncExternalStore` subscription (subscribe +
//!   getSnapshot → revision) is the [`ValuePresentationHookState`]'s
//!   `contribution_revision` + [`Self::note_registry_revision`]: the
//!   host observes the registry (the shared
//!   `support/value_presentation/registry.rs`) and reports its
//!   revision; a register/unregister bumps it and the next
//!   reconcile rebuilds.
//! - The `plans` memo whose ONLY dependency is the cache key is
//!   [`Self::reconcile`]: the key (T157's
//!   `compute_presentation_plan_cache_key`) already covers every
//!   invalidation factor — source identity, per-column metadata,
//!   the config revision, the contribution revision, locale,
//!   localTimeZone — so the key unchanged means the plans unchanged
//!   and prepare is NOT rebuilt (the React file's own eslint note,
//!   the load-bearing rule of this hook).
//! - `planByColumn`'s first-wins map is
//!   [`Self::plan_by_column`] (same-named columns keep the first
//!   plan; the plans list itself keeps one per input, in input
//!   order).
//! - `presentValue` is [`Self::present_value`]: a missing plan
//!   answers `unchanged("no-plan")` (the React literal's shape).
//! - The hook reads no store, DOM, IPC or file: the snapshot is a
//!   parameter (T06 supplies it from the store).
//!
//! Known deltas: the registry default (`valuePresentationRegistry`)
//! is the host's wiring — the port takes the revision report
//! through the note method, so any registry instance drives it
//! identically; the `source`'s `Omit<ColumnSource, 'column'>` is
//! T157's own `ColumnSource`-without-column shape (the plan builder
//! fills the column).

// No production consumer yet (the DataTable / DetailPanel track
// wires it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use std::collections::HashMap;

use crate::support::value_presentation::contracts::{ColumnSource, PresentationResult};
use crate::support::value_presentation::presentation_plan::{
    BuildColumnPresentationPlansInput, ColumnPresentationPlan, PresentationEnvironmentSnapshot,
    PresentationPlanCacheKeyInput, PresentationPlanColumn, ValuePresentationSnapshot,
    build_column_presentation_plans, compute_presentation_plan_cache_key,
};
use crate::support::value_presentation::registry::ValuePresentationRegistry;

/// The hook's output bundle (the `UseValuePresentationResult` shape,
/// owned): the plans, the first-wins map, the revisions and the key.
/// (The plans hold boxed presenters — no derive; the manual Debug
/// prints the plan faces only.)
pub(crate) struct ValuePresentationPlans {
    /// One plan per input column, in input order.
    pub(crate) plans: Vec<ColumnPresentationPlan>,
    /// Same-named columns keep the first plan.
    pub(crate) plan_by_column: HashMap<String, usize>,
    /// The registry revision the plans were built under.
    pub(crate) contribution_revision: u64,
    /// The cache key the plans were built for (the memo's own
    /// dependency — equal key, equal plans).
    pub(crate) cache_key: String,
}

impl std::fmt::Debug for ValuePresentationPlans {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let columns: Vec<&str> =
            self.plans.iter().map(|p| p.column_name.as_str()).collect();
        let map: std::collections::BTreeMap<&str, usize> = self
            .plan_by_column
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect();
        f.debug_struct("ValuePresentationPlans")
            .field("plan_columns", &columns)
            .field("plan_by_column", &map)
            .field("contribution_revision", &self.contribution_revision)
            .field("cache_key", &self.cache_key)
            .finish()
    }
}

/// `useValuePresentation()`'s state machine.
#[derive(Debug, Default)]
pub(crate) struct ValuePresentationHookState {
    /// The registry revision last observed (the external store's
    /// snapshot).
    contribution_revision: u64,
    /// The last build (the memo's output; `None` = never built).
    built: Option<ValuePresentationPlans>,
}

impl ValuePresentationHookState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The registry's revision report (the external store's change
    /// notification): a bump invalidates the next reconcile (the
    /// cache key covers it).
    pub(crate) fn note_registry_revision(&mut self, revision: u64) {
        self.contribution_revision = revision;
    }

    /// The observed registry revision (`contributionRevision`).
    pub(crate) fn contribution_revision(&self) -> u64 {
        self.contribution_revision
    }

    /// The plans (empty before the first build; the memo's output).
    pub(crate) fn plans(&self) -> &[ColumnPresentationPlan] {
        self.built.as_ref().map(|b| b.plans.as_slice()).unwrap_or(&[])
    }

    /// `planByColumn`: the first plan for a column name.
    pub(crate) fn plan_by_column(&self, column_name: &str) -> Option<&ColumnPresentationPlan> {
        let built = self.built.as_ref()?;
        built
            .plan_by_column
            .get(column_name)
            .and_then(|index| built.plans.get(*index))
    }

    /// The current cache key (the last build's, or the one the next
    /// build would use — the memo's dependency is observable).
    pub(crate) fn cache_key(&self) -> &str {
        self.built.as_ref().map(|b| b.cache_key.as_str()).unwrap_or("")
    }

    /// The memo's reconcile: compute the cache key over the inputs
    /// and rebuild the plans ONLY when the key changed (the React
    /// rule: 键不变即计划不变，不逐渲染重建 prepare). Answers whether
    /// the build ran.
    pub(crate) fn reconcile<'a>(
        &mut self,
        columns: Vec<PresentationPlanColumn<'a>>,
        source: Option<SourceIdentity<'a>>,
        snapshot: &'a ValuePresentationSnapshot,
    ) -> bool {
        let key = compute_presentation_plan_cache_key(&PresentationPlanCacheKeyInput {
            source: source.map(|s| s.borrow(columns.first())),
            columns: columns.clone(),
            config_revision: snapshot.revision,
            contribution_revision: self.contribution_revision,
            locale: &snapshot.environment.locale,
            local_time_zone: &snapshot.environment.local_time_zone,
            columns_revision: None,
        });
        if self.built.as_ref().is_some_and(|built| built.cache_key == key) {
            return false;
        }
        // Rebuild: the source borrows its column names from the
        // caller's columns, so the build input carries the borrowed
        // form directly.
        let input_source = source.map(|s| s.borrow(columns.first()));
        let plans = build_column_presentation_plans(
            &BuildColumnPresentationPlansInput {
                columns,
                source: input_source,
                snapshot,
            },
            // The registry the host wires (the default
            // `valuePresentationRegistry` in React); the plan builder
            // reads its contributions through it.
            &with_default_contributions(),
        );
        // The first-wins map.
        let mut plan_by_column: HashMap<String, usize> = HashMap::new();
        for (index, plan) in plans.iter().enumerate() {
            plan_by_column.entry(plan.column_name.clone()).or_insert(index);
        }
        self.built = Some(ValuePresentationPlans {
            plans,
            plan_by_column,
            contribution_revision: self.contribution_revision,
            cache_key: key,
        });
        true
    }

    /// `presentValue`: the column's plan presents the value; a
    /// missing plan answers `unchanged("no-plan")` (the React
    /// literal).
    pub(crate) fn present_value(
        &self,
        column_name: &str,
        value: &serde_json::Value,
    ) -> PresentationResult {
        match self.plan_by_column(column_name) {
            Some(plan) => plan.present(value),
            None => PresentationResult::unchanged("no-plan"),
        }
    }
}

/// The `Omit<ColumnSource, 'column'>` input: the table identity the
/// plan builder fills a column into (absent for query-result
/// columns — overrides unavailable).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceIdentity<'a> {
    pub(crate) connection_id: &'a str,
    pub(crate) database: Option<&'a str>,
    pub(crate) namespace: Option<&'a str>,
    pub(crate) table: &'a str,
}

impl<'a> SourceIdentity<'a> {
    /// The plan builder's `ColumnSource` over the first column's
    /// name (the builder's own contract: the column arrives from
    /// the iteration, not the source).
    fn borrow(self, _first_column: Option<&PresentationPlanColumn<'a>>) -> ColumnSource<'a> {
        ColumnSource {
            connection_id: self.connection_id,
            database: self.database,
            namespace: self.namespace,
            table: self.table,
            column: "",
        }
    }
}

/// The registry the hook builds against: the default registry with
/// the bundled contributions registered (React's global
/// `valuePresentationRegistry` after the bootstrap registers the
/// timestamp contribution — the `registerBundledContributions`
/// default).
fn with_default_contributions() -> ValuePresentationRegistry {
    crate::support::value_presentation::presentation_plan::registry_with_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;


    fn environment() -> PresentationEnvironmentSnapshot {
        PresentationEnvironmentSnapshot {
            locale: "zh-CN".to_owned(),
            local_time_zone: "Asia/Shanghai".to_owned(),
        }
    }

    fn snapshot() -> ValuePresentationSnapshot {
        ValuePresentationSnapshot::default_snapshot(environment())
    }

    fn columns(names: &[&'static str]) -> Vec<PresentationPlanColumn<'static>> {
        names
            .iter()
            .map(|name| PresentationPlanColumn {
                name: *name,
                data_type: Some("text"),
                cell_type: None,
                hints: None,
            })
            .collect()
    }

    /// The memo rule: an equal cache key does not rebuild (the
    /// plans keep their identity slot), a changed factor does.
    #[test]
    fn the_plans_rebuild_only_on_a_key_change() {
        let mut hook = ValuePresentationHookState::new();
        let snapshot = snapshot();

        // The first build runs.
        assert!(hook.reconcile(columns(&["a", "b"]), None, &snapshot));
        assert_eq!(hook.plans().len(), 2);
        let first_key = hook.cache_key().to_owned();

        // The same inputs: no rebuild (键不变即计划不变).
        assert!(!hook.reconcile(columns(&["a", "b"]), None, &snapshot));
        assert_eq!(hook.cache_key(), first_key);

        // A changed column set rebuilds.
        assert!(hook.reconcile(columns(&["a", "b", "c"]), None, &snapshot));
        assert_eq!(hook.plans().len(), 3);
        assert_ne!(hook.cache_key(), first_key);

        // A changed snapshot revision rebuilds.
        let first_key = hook.cache_key().to_owned();
        let mut next = snapshot.clone();
        next.revision += 1;
        assert!(hook.reconcile(columns(&["a", "b", "c"]), None, &next));
        assert_ne!(hook.cache_key(), first_key);

        // A changed registry revision rebuilds (the external store's
        // change).
        hook.note_registry_revision(7);
        assert!(hook.reconcile(columns(&["a", "b", "c"]), None, &next));
        assert_eq!(
            hook.built.as_ref().unwrap().contribution_revision, 7,
            "the build records the revision it used"
        );
        // An unchanged revision reports through the accessor.
        assert_eq!(hook.contribution_revision(), 7);
    }

    /// The first-wins map: duplicate names keep the first plan while
    /// the plans list keeps one per input (a same-named pair with
    /// different types answers the FIRST plan's behaviour).
    #[test]
    fn same_named_columns_keep_the_first_plan() {
        let mut hook = ValuePresentationHookState::new();
        let snapshot = snapshot();
        // The first `a` is a text column (no renderer → the default
        // fallback); the second `a` is a temporal column (disabled
        // in the default snapshot). Which one the map keeps is
        // observable through presentValue's reason.
        let columns = vec![
            PresentationPlanColumn {
                name: "a",
                data_type: Some("text"),
                cell_type: None,
                hints: None,
            },
            PresentationPlanColumn {
                name: "a",
                data_type: Some("datetime"),
                cell_type: None,
                hints: None,
            },
            PresentationPlanColumn {
                name: "b",
                data_type: Some("text"),
                cell_type: None,
                hints: None,
            },
        ];
        assert!(hook.reconcile(columns, None, &snapshot));
        assert_eq!(hook.plans().len(), 3, "no dedup in the list");
        // The first plan wins: the text column's fallback reason.
        match hook.present_value("a", &serde_json::json!("x")) {
            PresentationResult::Unchanged { reason } => {
                assert_eq!(
                    reason.as_deref(),
                    Some("no-renderer"),
                    "the first (text) plan answers, not the temporal one"
                );
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert!(hook.plan_by_column("b").is_some());
        assert!(hook.plan_by_column("missing").is_none());
    }

    /// `presentValue`: a missing plan answers the unchanged/no-plan
    /// shape (the React literal).
    #[test]
    fn a_missing_plan_presents_unchanged() {
        let mut hook = ValuePresentationHookState::new();
        let snapshot = snapshot();
        assert!(hook.reconcile(columns(&["a"]), None, &snapshot));
        match hook.present_value("missing", &serde_json::json!("x")) {
            PresentationResult::Unchanged { reason } => {
                assert_eq!(reason.as_deref(), Some("no-plan"));
            }
            other => panic!("unexpected result: {other:?}"),
        }
        // A planned column presents through its plan (the default
        // snapshot: a text column without a renderer answers the
        // default status's fallback).
        let result = hook.present_value("a", &serde_json::json!("x"));
        assert!(
            matches!(result, PresentationResult::Unchanged { .. }),
            "the default snapshot's text plan has no renderer: {result:?}"
        );
    }

    /// Before the first build the hook answers empty plans and an
    /// empty key.
    #[test]
    fn a_fresh_hook_answers_nothing() {
        let hook = ValuePresentationHookState::new();
        assert!(hook.plans().is_empty());
        assert_eq!(hook.cache_key(), "");
        assert_eq!(hook.contribution_revision(), 0);
        assert!(hook.plan_by_column("a").is_none());
    }
}
