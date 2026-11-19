//! Port of `packages/frontend/src/lib/valuePresentation/presentationPlan.ts`
//! (issue-32 T04) — the host presentation pipeline and per-column plans.
//!
//! Pure logic: no state, no Store, no Rune tree. The host builds one
//! [`ColumnPresentationPlan`] per column at panel-plan time (prepare runs
//! once per plan, never per row); the grid and aside consume the same
//! `plan.present(value)`.
//!
//! # Decision order (React verbatim)
//!
//! 1. Overrides: with a source, look up [`column_override_key`] in
//!    `snapshot.overrides`; query-result columns (no source) never match.
//! 2. Override `raw` → status `raw`, prepare never runs.
//! 3. Override `custom` → validate the options against the target
//!    contribution's `validate_config` (failure → `invalid-config`);
//!    an explicit custom override **bypasses the enabled gate** — a
//!    specific choice beats the global switch.
//! 4. Rules (overrides > rules > global default): after overrides, before
//!    the default route. The snapshot's rules are non-empty only while the
//!    timestamp renderer is enabled (the snapshot builder gates); the
//!    first matching rule's options become the effective config and the
//!    plan records `matched_rule_name`. A missing contribution falls to
//!    the default route (which reports missing-renderer / default).
//! 5. `inherit` or no match → the default route: temporal semantic kinds
//!    default to `datazen.timestamp`; the route checks the enabled gate
//!    (→ `disabled`), contribution presence (→ `missing-renderer`) and
//!    config presence (→ `missing-config`) in that order.
//! 6. An effective config whose version differs from the contribution's
//!    declaration → `invalid-config` without calling prepare.
//! 7. A panicking `prepare` is isolated into status `error` /
//!    `plan-error` (React's try/catch, here `catch_unwind`) — the plan
//!    stays usable and never records raw row values.
//!
//! # What maps where
//!
//! - `JSON.stringify` key/cache assembly → `serde_json::to_string` on
//!   value arrays: compact, key-order-stable, byte-identical to React's
//!   `JSON.stringify` for these shapes.
//! - `isZonedCellType` / `resolveCellType` come from the already-ported
//!   `support::cell_types` (T253-256 base layer).
//! - `resolvePresentationEnvironment`'s `navigator.language` /
//!   `Intl.DateTimeFormat().resolvedOptions().timeZone` fallbacks become
//!   `'en-US'` and `iana-time-zone`'s answer: the Rust host has no
//!   navigator, and callers pass the app locale explicitly in practice.
//! - The global `valuePresentationRegistry` default parameter becomes an
//!   explicit `&ValuePresentationRegistry` argument (the Rust host owns
//!   its registry instance).
//! - prepare's exception isolation → `catch_unwind` around the
//!   contribution call (the trait cannot express failure, so isolation is
//!   the host's job, exactly like React's).

use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::json;

use crate::support::cell_types::{
    CellType, CellTypeHints, CellTypeMatch, CellTypeRule, is_zoned_cell_type_with_hints,
    resolve_cell_type,
};
use crate::support::value_presentation::contracts::{
    ColumnPresentationOverride, ColumnSource, PresentationColumnContext, PresentationEnvironment,
    PresentationPrepareInput, PresentationResult, PreparedPresentation, SemanticKind,
    ValuePresentationContribution, VersionedPresentationConfig,
};
use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::timestamp_contribution::{
    TIMESTAMP_CONTRIBUTION_ID, timestamp_options_to_config_value,
};
use crate::support::value_presentation::value_presentation_rules::{
    ValuePresentationRule, ValuePresentationRuleContext, find_first_matching_rule,
};

// ---------------------------------------------------------------------------
// Semantic kinds (via the ported cellTypes layer)
// ---------------------------------------------------------------------------

/// A column input for plan building (React `PresentationPlanColumn`): the
/// host's minimal metadata, independent of the grid's column model.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PresentationPlanColumn<'a> {
    pub(crate) name: &'a str,
    pub(crate) data_type: Option<&'a str>,
    pub(crate) cell_type: Option<CellType>,
    pub(crate) hints: Option<&'a CellTypeHints>,
}

/// The semantic-kind mapping table (React's verbatim):
/// datetime → zoned ? instant : local-datetime; date → date; time → time;
/// number → number; text / enum / uuid → text; bool / json / bytes /
/// unknown → other. A missing cellType resolves through
/// [`resolve_cell_type`].
pub(crate) fn resolve_presentation_semantic_kind(
    column: &PresentationPlanColumn<'_>,
) -> SemanticKind {
    let cell_type = column
        .cell_type
        .unwrap_or_else(|| resolve_cell_type(column.data_type, column.hints));
    match cell_type {
        CellType::DateTime => {
            let zoned = match column.data_type {
                Some(data_type) => is_zoned_cell_type_with_hints(data_type, column.hints),
                None => false,
            };
            if zoned {
                SemanticKind::Instant
            } else {
                SemanticKind::LocalDatetime
            }
        }
        CellType::Date => SemanticKind::Date,
        CellType::Time => SemanticKind::Time,
        CellType::Number => SemanticKind::Number,
        CellType::Text | CellType::Enum | CellType::Uuid => SemanticKind::Text,
        CellType::Bool | CellType::Json | CellType::Bytes | CellType::Unknown => {
            SemanticKind::Other
        }
    }
}

/// Column input + full source → the column context prepare receives.
pub(crate) fn build_presentation_column_context<'a>(
    column: &PresentationPlanColumn<'a>,
    source: Option<ColumnSource<'a>>,
) -> PresentationColumnContext<'a> {
    PresentationColumnContext {
        column_name: column.name,
        database_type_name: column.data_type,
        semantic_kind: resolve_presentation_semantic_kind(column),
        source,
    }
}

// ---------------------------------------------------------------------------
// The override key
// ---------------------------------------------------------------------------

/// The table-column override key:
/// `JSON.stringify([connectionId, database ?? null, namespace ?? null,
/// table, column])` — no case folding (driver identifier semantics are
/// preserved); missing fields are null and empty strings stay distinct
/// from them (design §6, BUG-004 lesson).
pub(crate) fn column_override_key(source: &ColumnSource<'_>) -> String {
    let parts = json!([
        source.connection_id,
        source.database,
        source.namespace,
        source.table,
        source.column,
    ]);
    serde_json::to_string(&parts).expect("a JSON array always serializes")
}

// ---------------------------------------------------------------------------
// Environment + snapshot
// ---------------------------------------------------------------------------

/// An owned environment snapshot (React's `PresentationEnvironment`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PresentationEnvironmentSnapshot {
    pub(crate) locale: String,
    pub(crate) local_time_zone: String,
}

impl PresentationEnvironmentSnapshot {
    fn borrowed(&self) -> PresentationEnvironment<'_> {
        PresentationEnvironment {
            locale: &self.locale,
            local_time_zone: &self.local_time_zone,
        }
    }
}

/// Environment resolution: an explicit non-empty locale wins; otherwise
/// 'en-US' (React's navigator.language fallback — the Rust host has no
/// navigator and passes the app locale explicitly). The local time zone
/// comes from `iana-time-zone` (React's Intl resolvedOptions), never
/// empty.
pub(crate) fn resolve_presentation_environment(
    locale: Option<&str>,
) -> PresentationEnvironmentSnapshot {
    let locale = match locale {
        Some(locale) if !locale.is_empty() => locale.to_string(),
        _ => "en-US".to_string(),
    };
    let local_time_zone = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
    let local_time_zone = if local_time_zone.is_empty() {
        "UTC".to_string()
    } else {
        local_time_zone
    };
    PresentationEnvironmentSnapshot { locale, local_time_zone }
}

/// The in-memory presentation snapshot (immutable; revision identifies
/// the configuration generation). Rules ride the same revision as the
/// global config (one save bumps one revision — see the cache key).
#[derive(Clone, Debug)]
pub(crate) struct ValuePresentationSnapshot {
    pub(crate) revision: u64,
    pub(crate) environment: PresentationEnvironmentSnapshot,
    pub(crate) configs: BTreeMap<String, VersionedPresentationConfig>,
    pub(crate) enabled_renderer_ids: Vec<String>,
    pub(crate) overrides: BTreeMap<String, ColumnPresentationOverride>,
    pub(crate) rules: Vec<ValuePresentationRule>,
}

impl ValuePresentationSnapshot {
    /// The default snapshot (revision 0, all collections empty): the
    /// loading state — temporal columns answer `disabled` and render with
    /// the default display.
    pub(crate) fn default_snapshot(
        environment: PresentationEnvironmentSnapshot,
    ) -> Self {
        Self {
            revision: 0,
            environment,
            configs: BTreeMap::new(),
            enabled_renderer_ids: Vec::new(),
            overrides: BTreeMap::new(),
            rules: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Plans
// ---------------------------------------------------------------------------

/// A plan's status.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColumnPresentationStatus {
    Ready,
    Default,
    Raw,
    Disabled,
    MissingRenderer,
    MissingConfig,
    InvalidConfig,
    Error,
}

/// The stable present-fallback reason for each non-ready status.
pub(crate) fn fallback_presentation_reason(status: ColumnPresentationStatus) -> &'static str {
    match status {
        // Ready always carries a prepared presenter; defensive parity with
        // React's error value.
        ColumnPresentationStatus::Ready => "plan-error",
        ColumnPresentationStatus::Default => "no-renderer",
        ColumnPresentationStatus::Raw => "raw-override",
        ColumnPresentationStatus::Disabled => "disabled",
        ColumnPresentationStatus::MissingRenderer => "missing-renderer",
        ColumnPresentationStatus::MissingConfig => "missing-config",
        ColumnPresentationStatus::InvalidConfig => "invalid-config",
        ColumnPresentationStatus::Error => "plan-error",
    }
}

/// One column's presentation plan: prepare's outcome plus the unified
/// present entry point.
pub(crate) struct ColumnPresentationPlan {
    pub(crate) column_name: String,
    pub(crate) column: PresentationColumnContext<'static>,
    pub(crate) status: ColumnPresentationStatus,
    pub(crate) renderer_id: Option<String>,
    pub(crate) reason: Option<&'static str>,
    /// The matching global rule's name (source hint, PRD §3.3).
    pub(crate) matched_rule_name: Option<String>,
    pub(crate) prepared: Option<Box<dyn PreparedPresentation>>,
}

impl ColumnPresentationPlan {
    /// Present one value: ready plans delegate to the prepared presenter;
    /// every other status answers its stable fallback reason.
    pub(crate) fn present(&self, value: &serde_json::Value) -> PresentationResult {
        match &self.prepared {
            Some(prepared) => prepared.present(value),
            None => PresentationResult::unchanged(self.reason.unwrap_or("plan-error")),
        }
    }
}

/// `build_column_presentation_plans`' input.
pub(crate) struct BuildColumnPresentationPlansInput<'a> {
    pub(crate) columns: Vec<PresentationPlanColumn<'a>>,
    /// The table identity; absent for query-result columns (overrides
    /// unavailable).
    pub(crate) source: Option<ColumnSource<'a>>,
    pub(crate) snapshot: &'a ValuePresentationSnapshot,
}

fn make_fallback_plan(
    column_name: &str,
    column: PresentationColumnContext<'_>,
    status: ColumnPresentationStatus,
    renderer_id: Option<&str>,
) -> ColumnPresentationPlan {
    let reason = fallback_presentation_reason(status);
    ColumnPresentationPlan {
        column_name: column_name.to_string(),
        column: own_context(column),
        status,
        renderer_id: renderer_id.map(str::to_string),
        reason: Some(reason),
        matched_rule_name: None,
        prepared: None,
    }
}

/// Leak a string into the plan's 'static context (plans outlive their
/// build inputs; the leaked data is plain, immutable metadata).
fn leak_str(value: &str) -> &'static str {
    let leaked: &'static mut str = value.to_string().leak();
    &*leaked
}

/// The context borrows input lifetimes the plan does not own; plans are
/// returned after the input is dropped, so the context is rebuilt with
/// owned strings (column names, type names and source parts are plain
/// data — no semantic loss).
fn own_context(context: PresentationColumnContext<'_>) -> PresentationColumnContext<'static> {
    PresentationColumnContext {
        column_name: leak_str(context.column_name),
        database_type_name: context
            .database_type_name
            .map(|value| leak_str(value)),
        semantic_kind: context.semantic_kind,
        source: context.source.map(|source| ColumnSource {
            connection_id: leak_str(source.connection_id),
            database: source.database.map(|v| leak_str(v)),
            namespace: source.namespace.map(|v| leak_str(v)),
            table: leak_str(source.table),
            column: leak_str(source.column),
        }),
    }
}

/// A ready plan from prepare (host isolation: a panicking prepare becomes
/// status `error`).
fn prepare_plan(
    column_name: &str,
    column: PresentationColumnContext<'_>,
    renderer_id: &str,
    contribution: &dyn ValuePresentationContribution,
    effective_config: &VersionedPresentationConfig,
    column_override: Option<&ColumnPresentationOverride>,
    environment: &PresentationEnvironmentSnapshot,
) -> ColumnPresentationPlan {
    if effective_config.config_version != contribution.config_version() {
        // Unknown versions never overwrite old configs (design §4/§6):
        // prepare is not called.
        return make_fallback_plan(
            column_name,
            column,
            ColumnPresentationStatus::InvalidConfig,
            Some(renderer_id),
        );
    }
    let input = PresentationPrepareInput {
        config: effective_config,
        column_override,
        column,
        environment: environment.borrowed(),
    };
    let prepared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        contribution.prepare(&input)
    }));
    match prepared {
        Ok(prepared) => ColumnPresentationPlan {
            column_name: column_name.to_string(),
            column: own_context(column),
            status: ColumnPresentationStatus::Ready,
            renderer_id: Some(renderer_id.to_string()),
            reason: None,
            matched_rule_name: None,
            prepared: Some(prepared),
        },
        Err(_) => {
            // Host isolation: a contribution panic never escapes to the
            // caller; the plan stays usable (fallback display).
            make_fallback_plan(column_name, column, ColumnPresentationStatus::Error, Some(renderer_id))
        }
    }
}

fn build_one_column_plan(
    column: &PresentationPlanColumn<'_>,
    source: Option<ColumnSource<'_>>,
    snapshot: &ValuePresentationSnapshot,
    registry: &ValuePresentationRegistry,
) -> ColumnPresentationPlan {
    let context = build_presentation_column_context(column, source.map(|source| ColumnSource {
        connection_id: source.connection_id,
        database: source.database,
        namespace: source.namespace,
        table: source.table,
        column: column.name,
    }));

    // a. Override lookup: only with a source.
    let r#override: Option<&ColumnPresentationOverride> = source.and_then(|source| {
        let key = column_override_key(&ColumnSource {
            connection_id: source.connection_id,
            database: source.database,
            namespace: source.namespace,
            table: source.table,
            column: column.name,
        });
        snapshot.overrides.get(&key)
    });

    // b. raw → status raw, prepare never runs.
    if let Some(ColumnPresentationOverride::Raw) = r#override {
        return make_fallback_plan(column.name, context, ColumnPresentationStatus::Raw, None);
    }

    // c. custom → the explicit renderer choice, bypassing the enabled gate.
    if let Some(ColumnPresentationOverride::Custom { renderer_id, options }) = r#override {
        let renderer_id = renderer_id.as_str();
        let options: &serde_json::Value = options;
        let Some(contribution) = registry.get(renderer_id) else {
            return make_fallback_plan(
                column.name,
                context,
                ColumnPresentationStatus::MissingRenderer,
                Some(renderer_id),
            );
        };
        match contribution.validate_config(options) {
            crate::support::value_presentation::contracts::ConfigValidationResult::Err(_) => {
                return make_fallback_plan(
                    column.name,
                    context,
                    ColumnPresentationStatus::InvalidConfig,
                    Some(renderer_id),
                );
            }
            crate::support::value_presentation::contracts::ConfigValidationResult::Ok(config) => {
                let effective = VersionedPresentationConfig {
                    config_version: contribution.config_version(),
                    config,
                };
                return prepare_plan(
                    column.name,
                    context,
                    renderer_id,
                    contribution.as_ref(),
                    &effective,
                    r#override,
                    &snapshot.environment,
                );
            }
        }
    }

    // d. Rules: after overrides, before the default route. The first
    //    match freezes the column's interpretation. A missing contribution
    //    falls through to the default route.
    if !snapshot.rules.is_empty() {
        if let Some(rule_contribution) = registry.get(TIMESTAMP_CONTRIBUTION_ID) {
            let rule_context = ValuePresentationRuleContext {
                connection_id: context.source.map(|source| source.connection_id),
                database: context.source.and_then(|source| source.database),
                namespace: context.source.and_then(|source| source.namespace),
                table: context.source.map(|source| source.table),
                column: column.name,
                database_type_name: column.data_type,
            };
            if let Some(rule) = find_first_matching_rule(&snapshot.rules, &rule_context) {
                let effective = VersionedPresentationConfig {
                    config_version: rule_contribution.config_version(),
                    config: timestamp_options_to_config_value(&rule.options),
                };
                let mut plan = prepare_plan(
                    column.name,
                    context,
                    TIMESTAMP_CONTRIBUTION_ID,
                    rule_contribution.as_ref(),
                    &effective,
                    r#override,
                    &snapshot.environment,
                );
                plan.matched_rule_name = Some(rule.name.clone());
                return plan;
            }
        }
    }

    // e. inherit or no match → the default route.
    let semantic_kind = context.semantic_kind;
    let temporal = matches!(
        semantic_kind,
        SemanticKind::Instant
            | SemanticKind::LocalDatetime
            | SemanticKind::Date
            | SemanticKind::Time
    );
    if !temporal {
        // number / text / other have no default renderer.
        return make_fallback_plan(column.name, context, ColumnPresentationStatus::Default, None);
    }
    let renderer_id = TIMESTAMP_CONTRIBUTION_ID;
    if !snapshot.enabled_renderer_ids.iter().any(|id| id == renderer_id) {
        return make_fallback_plan(
            column.name,
            context,
            ColumnPresentationStatus::Disabled,
            Some(renderer_id),
        );
    }
    let Some(contribution) = registry.get(renderer_id) else {
        return make_fallback_plan(
            column.name,
            context,
            ColumnPresentationStatus::MissingRenderer,
            Some(renderer_id),
        );
    };
    let Some(effective_config) = snapshot.configs.get(renderer_id) else {
        return make_fallback_plan(
            column.name,
            context,
            ColumnPresentationStatus::MissingConfig,
            Some(renderer_id),
        );
    };
    prepare_plan(
        column.name,
        context,
        renderer_id,
        contribution.as_ref(),
        effective_config,
        r#override,
        &snapshot.environment,
    )
}

/// Build plans for every column, in input order. Empty input → empty
/// output; duplicate names produce independent plans (no dedup, no
/// error).
pub(crate) fn build_column_presentation_plans(
    input: &BuildColumnPresentationPlansInput<'_>,
    registry: &ValuePresentationRegistry,
) -> Vec<ColumnPresentationPlan> {
    input
        .columns
        .iter()
        .map(|column| build_one_column_plan(column, input.source, input.snapshot, registry))
        .collect()
}

// ---------------------------------------------------------------------------
// Plan cache key
// ---------------------------------------------------------------------------

/// `columnsRevision` — a number or a string (React's `number | string`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ColumnsRevision {
    Number(u64),
    Text(String),
}

/// `compute_presentation_plan_cache_key`'s input.
pub(crate) struct PresentationPlanCacheKeyInput<'a> {
    pub(crate) source: Option<ColumnSource<'a>>,
    pub(crate) columns: Vec<PresentationPlanColumn<'a>>,
    pub(crate) config_revision: u64,
    pub(crate) contribution_revision: u64,
    pub(crate) locale: &'a str,
    pub(crate) local_time_zone: &'a str,
    pub(crate) columns_revision: Option<ColumnsRevision>,
}

/// The plan cache key: a stable compact-JSON string covering every
/// invalidation factor of design §7 — source identity, per-column metadata
/// (name + dataType + semanticKind), the config revision, the contribution
/// revision, locale, local time zone and (when given) the columns
/// revision. Rules share the config revision (one save bumps one
/// revision), so they add no factor of their own.
pub(crate) fn compute_presentation_plan_cache_key(
    input: &PresentationPlanCacheKeyInput<'_>,
) -> String {
    let source_part = match input.source {
        None => serde_json::Value::Null,
        Some(source) => json!([
            source.connection_id,
            source.database,
            source.namespace,
            source.table,
        ]),
    };
    let columns_part: Vec<serde_json::Value> = input
        .columns
        .iter()
        .map(|column| {
            json!([
                column.name,
                column.data_type.unwrap_or(""),
                semantic_kind_str(resolve_presentation_semantic_kind(column)),
            ])
        })
        .collect();
    let columns_revision = match &input.columns_revision {
        None => serde_json::Value::Null,
        Some(ColumnsRevision::Number(value)) => json!(value),
        Some(ColumnsRevision::Text(value)) => json!(value),
    };
    let parts = json!([
        source_part,
        columns_part,
        input.config_revision,
        input.contribution_revision,
        input.locale,
        input.local_time_zone,
        columns_revision,
    ]);
    serde_json::to_string(&parts).expect("a JSON array always serializes")
}

fn semantic_kind_str(kind: SemanticKind) -> &'static str {
    match kind {
        SemanticKind::Instant => "instant",
        SemanticKind::LocalDatetime => "local-datetime",
        SemanticKind::Date => "date",
        SemanticKind::Time => "time",
        SemanticKind::Number => "number",
        SemanticKind::Text => "text",
        SemanticKind::Other => "other",
    }
}

/// A registry handle the plan builder can share with callers that also
/// need to register (kept for API symmetry with React's default-parameter
/// form).
pub(crate) fn registry_with_timestamp() -> ValuePresentationRegistry {
    let mut registry = ValuePresentationRegistry::new();
    let _ = registry.register(crate::support::value_presentation::timestamp_contribution::timestamp_contribution());
    registry
}

/// Silence unused-import warnings for the re-exported cellTypes surface
/// (part of this module's public seam).
#[allow(dead_code)]
fn _cell_types_surface() -> Option<CellTypeRule> {
    Some(CellTypeRule {
        match_kind: CellTypeMatch::Exact,
        pattern: "x",
        cell_type: CellType::Bytes,
    })
}

/// A shared-contribution helper for callers holding an `Rc`.
#[allow(dead_code)]
fn _rc_surface(_value: Rc<dyn ValuePresentationContribution>) {}
