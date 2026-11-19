//! Independent audit for T155 (the presentationPlan port).
//!
//! Every assertion comes from re-reading
//! `packages/frontend/src/lib/valuePresentation/presentationPlan.ts` and
//! its React test `__tests__/presentationPlan.test.ts` — the semantic-kind
//! mapping table, the override key's stability/case/missing-vs-empty
//! contract, the default snapshot, the default routing and the three
//! missing states, the override matrix (raw never calls prepare, custom
//! bypasses the enabled gate, version mismatch never calls prepare,
//! invalid config bodies surface through present), prepare-panic
//! isolation, the fallback reason table, the cache key factors, the shape
//! rules and the PRD anchors.
//!
//! Type-level notes: the throwing-contribution tests use a panicking test
//! contribution (the plan builder isolates it via catch_unwind — React's
//! try/catch); the "prepare must not be called" tests use a probe
//! contribution with a called flag.

use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

use serde_json::json;

use crate::support::cell_types::{CellType, CellTypeHints, CellTypeMatch, CellTypeRule};
use crate::support::value_presentation::contracts::{
    ColumnPresentationOverride, ColumnSource, ConfigValidationResult,
    PresentationColumnContext, PresentationPrepareInput, PresentationResult,
    PreparedPresentation, SemanticKind, ValuePresentationContribution,
    VersionedPresentationConfig,
};
use crate::support::value_presentation::presentation_plan::{
    BuildColumnPresentationPlansInput, ColumnPresentationPlan, ColumnPresentationStatus,
    ColumnsRevision, PresentationPlanCacheKeyInput, PresentationPlanColumn,
    ValuePresentationSnapshot, build_column_presentation_plans,
    build_presentation_column_context, column_override_key, compute_presentation_plan_cache_key,
    fallback_presentation_reason, resolve_presentation_environment,
    resolve_presentation_semantic_kind,
};
use crate::support::value_presentation::registry::ValuePresentationRegistry;
use crate::support::value_presentation::timestamp_contribution::{
    TIMESTAMP_CONTRIBUTION_ID, timestamp_contribution,
};
use crate::support::value_presentation::value_presentation_rules::{
    ValuePresentationRule, validate_value_presentation_rule,
    ValuePresentationRuleValidation,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const TIMESTAMP_ID: &str = TIMESTAMP_CONTRIBUTION_ID;

fn source() -> ColumnSource<'static> {
    ColumnSource {
        connection_id: "conn-1",
        database: Some("shop"),
        namespace: Some("public"),
        table: "orders",
        column: "created_at",
    }
}

fn native_shanghai_iso_tz() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "native" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn unix_ms_shanghai_iso_tz() -> serde_json::Value {
    json!({
        "interpretation": { "kind": "unix", "unit": "milliseconds" },
        "format": "iso-tz",
        "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
    })
}

fn timestamp_config(options: serde_json::Value) -> VersionedPresentationConfig {
    VersionedPresentationConfig { config_version: 1, config: options }
}

struct SnapshotOptions {
    configs: Option<BTreeMap<String, VersionedPresentationConfig>>,
    enabled: Option<Vec<String>>,
    overrides: Option<BTreeMap<String, ColumnPresentationOverride>>,
    rules: Vec<ValuePresentationRule>,
}

fn make_snapshot(options: SnapshotOptions) -> ValuePresentationSnapshot {
    let mut configs = BTreeMap::new();
    configs.insert(
        TIMESTAMP_ID.to_string(),
        timestamp_config(native_shanghai_iso_tz()),
    );
    ValuePresentationSnapshot {
        revision: 1,
        environment: crate::support::value_presentation::presentation_plan::PresentationEnvironmentSnapshot {
            locale: "en-US".to_string(),
            local_time_zone: "Asia/Shanghai".to_string(),
        },
        configs: options.configs.unwrap_or(configs),
        enabled_renderer_ids: options
            .enabled
            .unwrap_or_else(|| vec![TIMESTAMP_ID.to_string()]),
        overrides: options.overrides.unwrap_or_default(),
        rules: options.rules,
    }
}

fn default_snapshot() -> ValuePresentationSnapshot {
    make_snapshot(SnapshotOptions { configs: None, enabled: None, overrides: None, rules: Vec::new() })
}

fn make_registry(with_timestamp: bool) -> ValuePresentationRegistry {
    let mut registry = ValuePresentationRegistry::new();
    if with_timestamp {
        registry.register(timestamp_contribution()).expect("register");
    }
    registry
}

fn col<'a>(name: &'a str, data_type: Option<&'a str>) -> PresentationPlanColumn<'a> {
    PresentationPlanColumn { name, data_type, cell_type: None, hints: None }
}

fn col_typed<'a>(
    name: &'a str,
    data_type: Option<&'a str>,
    cell_type: Option<CellType>,
) -> PresentationPlanColumn<'a> {
    PresentationPlanColumn { name, data_type, cell_type, hints: None }
}

fn plans_for(
    columns: Vec<PresentationPlanColumn<'_>>,
    snapshot: &ValuePresentationSnapshot,
    registry: &ValuePresentationRegistry,
) -> Vec<ColumnPresentationPlan> {
    build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns,
            source: Some(source()),
            snapshot,
        },
        registry,
    )
}

fn plan_for(
    column: PresentationPlanColumn<'_>,
    snapshot: &ValuePresentationSnapshot,
    registry: &ValuePresentationRegistry,
) -> ColumnPresentationPlan {
    let plans = plans_for(vec![column], snapshot, registry);
    assert_eq!(plans.len(), 1);
    plans.into_iter().next().expect("one plan")
}

fn assert_text(result: &PresentationResult, expected: &str, interpretation: &str) {
    match result {
        PresentationResult::Text { text, interpretation: actual } => {
            assert_eq!(text, expected, "text");
            assert_eq!(actual.as_deref(), Some(interpretation), "interpretation");
        }
        other => panic!("expected text {expected:?}, got {other:?}"),
    }
}

fn assert_unchanged(result: &PresentationResult, reason: &str) {
    match result {
        PresentationResult::Unchanged { reason: actual } => {
            assert_eq!(actual.as_deref(), Some(reason));
        }
        other => panic!("expected unchanged {reason:?}, got {other:?}"),
    }
}

/// A probe contribution: records whether prepare ran.
struct ProbeContribution {
    called: Rc<Cell<bool>>,
    id: &'static str,
}

impl ValuePresentationContribution for ProbeContribution {
    fn id(&self) -> &str {
        self.id
    }
    fn config_version(&self) -> u32 {
        1
    }
    fn validate_config(&self, _input: &serde_json::Value) -> ConfigValidationResult {
        ConfigValidationResult::Ok(json!({}))
    }
    fn prepare(&self, _input: &PresentationPrepareInput<'_>) -> Box<dyn PreparedPresentation> {
        self.called.set(true);
        Box::new(ProbePrepared)
    }
}

struct ProbePrepared;

impl PreparedPresentation for ProbePrepared {
    fn present(&self, _value: &serde_json::Value) -> PresentationResult {
        PresentationResult::unchanged("probe")
    }
}

/// A contribution whose prepare panics (the React suite's throwing
/// contribution).
struct PanicContribution;

impl ValuePresentationContribution for PanicContribution {
    fn id(&self) -> &str {
        TIMESTAMP_ID
    }
    fn config_version(&self) -> u32 {
        1
    }
    fn validate_config(&self, _input: &serde_json::Value) -> ConfigValidationResult {
        ConfigValidationResult::Ok(json!({}))
    }
    fn prepare(&self, _input: &PresentationPrepareInput<'_>) -> Box<dyn PreparedPresentation> {
        panic!("boom");
    }
}

// ---------------------------------------------------------------------------
// Semantic-kind mapping
// ---------------------------------------------------------------------------

#[test]
fn semantic_kind_full_table() {
    let cases: [(PresentationPlanColumn<'_>, SemanticKind); 25] = [
        (col("c", Some("timestamptz")), SemanticKind::Instant),
        (col("c", Some("timestamp with time zone")), SemanticKind::Instant),
        (col("c", Some("datetimeoffset")), SemanticKind::Instant),
        (col("c", Some("timestamp")), SemanticKind::LocalDatetime),
        (col("c", Some("datetime")), SemanticKind::LocalDatetime),
        // Explicit datetime + no dataType → local-datetime.
        (col_typed("c", None, Some(CellType::DateTime)), SemanticKind::LocalDatetime),
        // Explicit datetime + zoned dataType → instant.
        (
            col_typed("c", Some("timestamptz"), Some(CellType::DateTime)),
            SemanticKind::Instant,
        ),
        (col("c", Some("date")), SemanticKind::Date),
        (col_typed("c", None, Some(CellType::Date)), SemanticKind::Date),
        (col("c", Some("time")), SemanticKind::Time),
        (col_typed("c", None, Some(CellType::Time)), SemanticKind::Time),
        (col("c", Some("bigint")), SemanticKind::Number),
        (col("c", Some("integer")), SemanticKind::Number),
        (col("c", Some("decimal(10,2)")), SemanticKind::Number),
        (col_typed("c", None, Some(CellType::Number)), SemanticKind::Number),
        (col("c", Some("varchar(10)")), SemanticKind::Text),
        (col("c", Some("text")), SemanticKind::Text),
        (col_typed("c", None, Some(CellType::Enum)), SemanticKind::Text),
        (col_typed("c", Some("uuid"), Some(CellType::Uuid)), SemanticKind::Text),
        (col("c", Some("boolean")), SemanticKind::Other),
        (col("c", Some("jsonb")), SemanticKind::Other),
        (col("c", Some("bytea")), SemanticKind::Other),
        (col("c", Some("some-weird-type")), SemanticKind::Other),
        (col("c", None), SemanticKind::Other),
        (col_typed("c", None, Some(CellType::Unknown)), SemanticKind::Other),
    ];
    for (column, expected) in cases {
        assert_eq!(
            resolve_presentation_semantic_kind(&column),
            expected,
            "column {:?}/{:?}",
            column.name,
            column.data_type
        );
    }
}

#[test]
fn semantic_kind_driver_hints_decide_zoned() {
    let hints = CellTypeHints {
        overrides: Vec::new(),
        enum_types: Vec::new(),
        zoned_types: vec!["my_tz_type".to_string()],
        read_only_types: Vec::new(),
    };
    let column = PresentationPlanColumn {
        name: "c",
        data_type: Some("my_tz_type"),
        cell_type: Some(CellType::DateTime),
        hints: Some(&hints),
    };
    assert_eq!(resolve_presentation_semantic_kind(&column), SemanticKind::Instant);
}

#[test]
fn semantic_kind_driver_overrides_win() {
    let hints = CellTypeHints {
        overrides: vec![CellTypeRule {
            match_kind: CellTypeMatch::Exact,
            pattern: "rowversion",
            cell_type: CellType::Bytes,
        }],
        enum_types: Vec::new(),
        zoned_types: Vec::new(),
        read_only_types: Vec::new(),
    };
    let column = PresentationPlanColumn {
        name: "c",
        data_type: Some("rowversion"),
        cell_type: None,
        hints: Some(&hints),
    };
    assert_eq!(resolve_presentation_semantic_kind(&column), SemanticKind::Other);
}

#[test]
fn build_context_assembles_column_and_source() {
    let context =
        build_presentation_column_context(&col("created_at", Some("timestamptz")), Some(source()));
    assert_eq!(context.column_name, "created_at");
    assert_eq!(context.database_type_name, Some("timestamptz"));
    assert_eq!(context.semantic_kind, SemanticKind::Instant);
    assert_eq!(context.source, Some(source()));
}

#[test]
fn build_context_without_source_leaves_source_absent() {
    let context = build_presentation_column_context(&col("n", Some("integer")), None);
    assert!(context.source.is_none());
    assert_eq!(context.semantic_kind, SemanticKind::Number);
}

// ---------------------------------------------------------------------------
// columnOverrideKey
// ---------------------------------------------------------------------------

#[test]
fn override_key_is_stable_and_matches_json_array() {
    let full = source();
    assert_eq!(column_override_key(&full), column_override_key(&full));
    assert_eq!(
        column_override_key(&full),
        r#"["conn-1","shop","public","orders","created_at"]"#
    );
}

#[test]
fn override_key_does_no_case_folding() {
    let full = source();
    let lower_table = ColumnSource { table: "Orders", ..source() };
    let lower_column = ColumnSource { column: "Created_At", ..source() };
    assert_ne!(column_override_key(&lower_table), column_override_key(&full));
    assert_ne!(column_override_key(&lower_column), column_override_key(&full));
}

#[test]
fn override_key_distinguishes_missing_and_empty_fields() {
    let no_database = ColumnSource {
        connection_id: "c",
        database: None,
        namespace: None,
        table: "t",
        column: "x",
    };
    let empty_database = ColumnSource {
        connection_id: "c",
        database: Some(""),
        namespace: Some(""),
        table: "t",
        column: "x",
    };
    assert_ne!(column_override_key(&no_database), column_override_key(&empty_database));
    // The cache key's source part honours the same distinction.
    let cache_base_columns = vec![col("x", None)];
    let key_missing = compute_presentation_plan_cache_key(&PresentationPlanCacheKeyInput {
        source: Some(ColumnSource {
            connection_id: "c",
            database: None,
            namespace: None,
            table: "t",
            column: "x",
        }),
        columns: cache_base_columns.clone(),
        config_revision: 1,
        contribution_revision: 1,
        locale: "en-US",
        local_time_zone: "UTC",
        columns_revision: None,
    });
    let key_empty = compute_presentation_plan_cache_key(&PresentationPlanCacheKeyInput {
        source: Some(ColumnSource {
            connection_id: "c",
            database: Some(""),
            namespace: None,
            table: "t",
            column: "x",
        }),
        columns: cache_base_columns,
        config_revision: 1,
        contribution_revision: 1,
        locale: "en-US",
        local_time_zone: "UTC",
        columns_revision: None,
    });
    assert_ne!(key_missing, key_empty);
}

// ---------------------------------------------------------------------------
// Default snapshot + environment
// ---------------------------------------------------------------------------

#[test]
fn default_snapshot_answers_disabled_for_temporal_columns() {
    use crate::support::value_presentation::presentation_plan::PresentationEnvironmentSnapshot;
    let snapshot = ValuePresentationSnapshot::default_snapshot(PresentationEnvironmentSnapshot {
        locale: "en-US".to_string(),
        local_time_zone: "Asia/Shanghai".to_string(),
    });
    assert_eq!(snapshot.revision, 0);
    assert!(snapshot.configs.is_empty());
    assert!(snapshot.enabled_renderer_ids.is_empty());
    assert!(snapshot.overrides.is_empty());
    let plan = plan_for(col("created_at", Some("timestamptz")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Disabled);
    assert_unchanged(&plan.present(&json!("2026-01-15T14:30:00Z")), "disabled");
}

#[test]
fn environment_resolution_explicit_locale_wins() {
    let env = resolve_presentation_environment(Some("zh-CN"));
    assert_eq!(env.locale, "zh-CN");
    assert!(!env.local_time_zone.is_empty());
}

#[test]
fn environment_resolution_falls_back_to_en_us() {
    let env = resolve_presentation_environment(None);
    assert_eq!(env.locale, "en-US");
    assert!(!env.local_time_zone.is_empty());
}

// ---------------------------------------------------------------------------
// Default routing
// ---------------------------------------------------------------------------

#[test]
fn temporal_semantics_default_to_the_timestamp_renderer() {
    for (column, _kind) in [
        (col("created_at", Some("timestamptz")), "instant"),
        (col("created_at", Some("timestamp")), "local-datetime"),
        (col("d", Some("date")), "date"),
        (col("t", Some("time")), "time"),
    ] {
        let plan = plan_for(column, &default_snapshot(), &make_registry(true));
        assert_eq!(plan.status, ColumnPresentationStatus::Ready, "column {column:?}");
        assert_eq!(plan.renderer_id.as_deref(), Some(TIMESTAMP_ID));
        assert!(plan.prepared.is_some());
    }
}

#[test]
fn non_temporal_semantics_have_no_default_renderer() {
    for (column, _kind) in [
        (col("amount", Some("bigint")), "number"),
        (col("name", Some("varchar(10)")), "text"),
        (col("flag", Some("boolean")), "other"),
        (col("payload", Some("jsonb")), "other"),
    ] {
        let plan = plan_for(column, &default_snapshot(), &make_registry(true));
        assert_eq!(plan.status, ColumnPresentationStatus::Default);
        assert!(plan.renderer_id.is_none());
        assert_eq!(plan.reason, Some("no-renderer"));
        assert_unchanged(&plan.present(&json!(42)), "no-renderer");
    }
}

// ---------------------------------------------------------------------------
// The three missing states
// ---------------------------------------------------------------------------

#[test]
fn missing_enabled_answers_disabled() {
    let snapshot = make_snapshot(SnapshotOptions {
        enabled: Some(Vec::new()),
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("timestamptz")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Disabled);
    assert_eq!(plan.renderer_id.as_deref(), Some(TIMESTAMP_ID));
    assert_eq!(plan.reason, Some("disabled"));
    assert_unchanged(&plan.present(&json!("2026-01-15T14:30:00Z")), "disabled");
}

fn default_options() -> SnapshotOptions {
    SnapshotOptions { configs: None, enabled: None, overrides: None, rules: Vec::new() }
}

#[test]
fn missing_contribution_answers_missing_renderer() {
    let plan = plan_for(
        col("created_at", Some("timestamptz")),
        &default_snapshot(),
        &make_registry(false),
    );
    assert_eq!(plan.status, ColumnPresentationStatus::MissingRenderer);
    assert_eq!(plan.renderer_id.as_deref(), Some(TIMESTAMP_ID));
    assert_eq!(plan.reason, Some("missing-renderer"));
    assert_unchanged(&plan.present(&json!("2026-01-15T14:30:00Z")), "missing-renderer");
}

#[test]
fn missing_config_answers_missing_config() {
    let snapshot = make_snapshot(SnapshotOptions {
        configs: Some(BTreeMap::new()),
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("timestamptz")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::MissingConfig);
    assert_eq!(plan.renderer_id.as_deref(), Some(TIMESTAMP_ID));
    assert_eq!(plan.reason, Some("missing-config"));
    assert_unchanged(&plan.present(&json!("2026-01-15T14:30:00Z")), "missing-config");
}

#[test]
fn disabled_takes_precedence_over_missing_renderer() {
    let snapshot = make_snapshot(SnapshotOptions {
        enabled: Some(Vec::new()),
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("timestamptz")), &snapshot, &make_registry(false));
    assert_eq!(plan.status, ColumnPresentationStatus::Disabled);
}

// ---------------------------------------------------------------------------
// Overrides: raw / custom / inherit
// ---------------------------------------------------------------------------

fn override_snapshot(
    mode: ColumnPresentationOverride,
    column_name: &str,
) -> ValuePresentationSnapshot {
    let mut overrides = BTreeMap::new();
    overrides.insert(
        column_override_key(&ColumnSource { column: column_name, ..source() }),
        mode,
    );
    make_snapshot(SnapshotOptions {
        overrides: Some(overrides),
        ..default_options()
    })
}

#[test]
fn raw_override_never_calls_prepare() {
    let called = Rc::new(Cell::new(false));
    let mut registry = ValuePresentationRegistry::new();
    registry
        .register(Rc::new(ProbeContribution { called: Rc::clone(&called), id: TIMESTAMP_ID }))
        .expect("register");
    let plan = plan_for(
        col("created_at", Some("timestamptz")),
        &override_snapshot(ColumnPresentationOverride::Raw, "created_at"),
        &registry,
    );
    assert_eq!(plan.status, ColumnPresentationStatus::Raw);
    assert_eq!(plan.reason, Some("raw-override"));
    assert_unchanged(&plan.present(&json!("2026-01-15T14:30:00Z")), "raw-override");
    assert!(!called.get(), "prepare must not run for raw overrides");
}

#[test]
fn custom_override_bypasses_the_enabled_gate() {
    let snapshot = override_snapshot(
        ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_ID.to_string(),
            options: unix_ms_shanghai_iso_tz(),
        },
        "event_ms",
    );
    let disabled_snapshot = ValuePresentationSnapshot {
        enabled_renderer_ids: Vec::new(),
        ..snapshot
    };
    let plan = plan_for(col("event_ms", Some("bigint")), &disabled_snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert_eq!(plan.renderer_id.as_deref(), Some(TIMESTAMP_ID));
    assert_text(
        &plan.present(&json!(1705316400000i64)),
        "2024-01-15T19:00:00+08:00",
        "unix",
    );
}

#[test]
fn custom_override_with_missing_renderer_answers_missing_renderer() {
    let plan = plan_for(
        col("created_at", Some("timestamptz")),
        &override_snapshot(
            ColumnPresentationOverride::Custom {
                renderer_id: "datazen.nope".to_string(),
                options: native_shanghai_iso_tz(),
            },
            "created_at",
        ),
        &make_registry(true),
    );
    assert_eq!(plan.status, ColumnPresentationStatus::MissingRenderer);
    assert_eq!(plan.renderer_id.as_deref(), Some("datazen.nope"));
    assert_eq!(plan.reason, Some("missing-renderer"));
}

#[test]
fn custom_override_with_invalid_options_answers_invalid_config() {
    let plan = plan_for(
        col("created_at", Some("timestamptz")),
        &override_snapshot(
            ColumnPresentationOverride::Custom {
                renderer_id: TIMESTAMP_ID.to_string(),
                options: json!({
                    "interpretation": { "kind": "bogus" },
                    "format": "iso-tz",
                    "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
                }),
            },
            "created_at",
        ),
        &make_registry(true),
    );
    assert_eq!(plan.status, ColumnPresentationStatus::InvalidConfig);
    assert_eq!(plan.renderer_id.as_deref(), Some(TIMESTAMP_ID));
    assert_eq!(plan.reason, Some("invalid-config"));
    assert_unchanged(&plan.present(&json!("2026-01-15T14:30:00Z")), "invalid-config");
}

#[test]
fn inherit_uses_the_default_route() {
    let plan = plan_for(
        col("created_at", Some("timestamptz")),
        &override_snapshot(ColumnPresentationOverride::Inherit, "created_at"),
        &make_registry(true),
    );
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert_text(
        &plan.present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
}

#[test]
fn inherit_does_not_bypass_the_enabled_gate() {
    let snapshot = override_snapshot(ColumnPresentationOverride::Inherit, "created_at");
    let disabled_snapshot = ValuePresentationSnapshot {
        enabled_renderer_ids: Vec::new(),
        ..snapshot
    };
    let plan = plan_for(col("created_at", Some("timestamptz")), &disabled_snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Disabled);
}

#[test]
fn version_mismatch_answers_invalid_config_without_prepare() {
    let called = Rc::new(Cell::new(false));
    let mut registry = ValuePresentationRegistry::new();
    registry
        .register(Rc::new(ProbeContribution { called: Rc::clone(&called), id: TIMESTAMP_ID }))
        .expect("register");
    let mut configs = BTreeMap::new();
    configs.insert(
        TIMESTAMP_ID.to_string(),
        VersionedPresentationConfig {
            config_version: 99,
            config: native_shanghai_iso_tz(),
        },
    );
    let snapshot = make_snapshot(SnapshotOptions {
        configs: Some(configs),
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("timestamptz")), &snapshot, &registry);
    assert_eq!(plan.status, ColumnPresentationStatus::InvalidConfig);
    assert_eq!(plan.reason, Some("invalid-config"));
    assert!(!called.get(), "prepare must not run on version mismatch");
}

#[test]
fn invalid_config_body_surfaces_through_present() {
    // The host only checks version and presence; the body's semantics are
    // the contribution's business (its prepare falls back).
    let mut configs = BTreeMap::new();
    configs.insert(
        TIMESTAMP_ID.to_string(),
        VersionedPresentationConfig {
            config_version: 1,
            config: json!({
                "interpretation": { "kind": "bogus" },
                "format": "iso-tz",
                "displayTimeZone": { "kind": "iana", "name": "Asia/Shanghai" },
            }),
        },
    );
    let snapshot = make_snapshot(SnapshotOptions {
        configs: Some(configs),
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("timestamptz")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert_unchanged(&plan.present(&json!("2026-01-15T14:30:00Z")), "invalid-config");
}

// ---------------------------------------------------------------------------
// prepare isolation
// ---------------------------------------------------------------------------

#[test]
fn panicking_prepare_is_isolated_into_error_status() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {})); // silence the intentional panic
    let mut registry = ValuePresentationRegistry::new();
    registry.register(Rc::new(PanicContribution)).expect("register");
    let plan = plan_for(
        col("created_at", Some("timestamptz")),
        &default_snapshot(),
        &registry,
    );
    std::panic::set_hook(previous_hook);
    assert_eq!(plan.status, ColumnPresentationStatus::Error);
    assert_eq!(plan.reason, Some("plan-error"));
    let result = plan.present(&json!("2026-01-15T14:30:00Z"));
    assert_unchanged(&result, "plan-error");
    // The fallback never records raw row values.
    let serialized = format!("{result:?}");
    assert!(!serialized.contains("2026-01-15"));
}

// ---------------------------------------------------------------------------
// Fallback reason table
// ---------------------------------------------------------------------------

#[test]
fn fallback_reason_full_table() {
    let cases = [
        (ColumnPresentationStatus::Default, "no-renderer"),
        (ColumnPresentationStatus::Raw, "raw-override"),
        (ColumnPresentationStatus::Disabled, "disabled"),
        (ColumnPresentationStatus::MissingRenderer, "missing-renderer"),
        (ColumnPresentationStatus::MissingConfig, "missing-config"),
        (ColumnPresentationStatus::InvalidConfig, "invalid-config"),
        (ColumnPresentationStatus::Error, "plan-error"),
    ];
    for (status, reason) in cases {
        assert_eq!(fallback_presentation_reason(status), reason);
    }
    // Ready's defensive value matches error's (React parity).
    assert_eq!(fallback_presentation_reason(ColumnPresentationStatus::Ready), "plan-error");
}

// ---------------------------------------------------------------------------
// Cache key
// ---------------------------------------------------------------------------

fn cache_key_base<'a>() -> PresentationPlanCacheKeyInput<'a> {
    PresentationPlanCacheKeyInput {
        source: Some(source()),
        columns: vec![col("created_at", Some("timestamptz")), col("amount", Some("bigint"))],
        config_revision: 3,
        contribution_revision: 5,
        locale: "en-US",
        local_time_zone: "Asia/Shanghai",
        columns_revision: None,
    }
}

#[test]
fn cache_key_is_stable_for_equal_inputs() {
    assert_eq!(
        compute_presentation_plan_cache_key(&cache_key_base()),
        compute_presentation_plan_cache_key(&cache_key_base())
    );
}

#[test]
fn cache_key_changes_with_any_factor() {
    let key = compute_presentation_plan_cache_key(&cache_key_base());
    // source variants
    let mut variant = cache_key_base();
    variant.source = Some(ColumnSource { connection_id: "conn-2", ..source() });
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.source = Some(ColumnSource { table: "items", ..source() });
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.source = None;
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    // columns variants
    let mut variant = cache_key_base();
    variant.columns = vec![col("created_at", Some("timestamptz")), col("amount", Some("integer"))];
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.columns = vec![col("created_at", Some("timestamp")), col("amount", Some("bigint"))];
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.columns = vec![col("other", Some("timestamptz")), col("amount", Some("bigint"))];
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    // scalar variants
    let mut variant = cache_key_base();
    variant.config_revision = 4;
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.contribution_revision = 6;
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.locale = "zh-CN";
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.local_time_zone = "UTC";
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.columns_revision = Some(ColumnsRevision::Number(7));
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
    let mut variant = cache_key_base();
    variant.columns_revision = Some(ColumnsRevision::Text("v2".to_string()));
    assert_ne!(compute_presentation_plan_cache_key(&variant), key);
}

#[test]
fn cache_key_respects_column_order() {
    let mut variant = cache_key_base();
    variant.columns = vec![col("amount", Some("bigint")), col("created_at", Some("timestamptz"))];
    assert_ne!(
        compute_presentation_plan_cache_key(&variant),
        compute_presentation_plan_cache_key(&cache_key_base())
    );
}

#[test]
fn cache_key_columns_revision_same_value_same_key() {
    let mut a = cache_key_base();
    a.columns_revision = Some(ColumnsRevision::Number(7));
    let mut b = cache_key_base();
    b.columns_revision = Some(ColumnsRevision::Number(7));
    assert_eq!(
        compute_presentation_plan_cache_key(&a),
        compute_presentation_plan_cache_key(&b)
    );
    b.columns_revision = Some(ColumnsRevision::Number(8));
    assert_ne!(
        compute_presentation_plan_cache_key(&a),
        compute_presentation_plan_cache_key(&b)
    );
}

// ---------------------------------------------------------------------------
// Shape rules
// ---------------------------------------------------------------------------

#[test]
fn empty_columns_yield_empty_plans() {
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: Vec::new(),
            source: Some(source()),
            snapshot: &default_snapshot(),
        },
        &make_registry(true),
    );
    assert!(plans.is_empty());
}

#[test]
fn duplicate_column_names_produce_independent_plans() {
    let plans = plans_for(
        vec![col("created_at", Some("timestamptz")), col("created_at", Some("timestamptz"))],
        &default_snapshot(),
        &make_registry(true),
    );
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].status, ColumnPresentationStatus::Ready);
    assert_eq!(plans[1].status, ColumnPresentationStatus::Ready);
    for plan in &plans {
        assert_text(
            &plan.present(&json!("2026-01-15T14:30:00Z")),
            "2026-01-15T22:30:00+08:00",
            "native",
        );
    }
    // Two distinct plan objects (pointer inequality, React's toBe).
    let first = &plans[0] as *const _;
    let second = &plans[1] as *const _;
    assert_ne!(first, second);
}

#[test]
fn plans_follow_input_order() {
    let plans = plans_for(
        vec![col("amount", Some("bigint")), col("created_at", Some("timestamptz")), col("name", Some("text"))],
        &default_snapshot(),
        &make_registry(true),
    );
    let names: Vec<&str> = plans.iter().map(|plan| plan.column_name.as_str()).collect();
    assert_eq!(names, ["amount", "created_at", "name"]);
    let statuses: Vec<ColumnPresentationStatus> =
        plans.iter().map(|plan| plan.status).collect();
    assert_eq!(
        statuses,
        [ColumnPresentationStatus::Default, ColumnPresentationStatus::Ready, ColumnPresentationStatus::Default]
    );
}

#[test]
fn query_result_columns_cannot_use_overrides() {
    let snapshot = override_snapshot(ColumnPresentationOverride::Raw, "created_at");
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: vec![col("created_at", Some("timestamptz"))],
            source: None,
            snapshot: &snapshot,
        },
        &make_registry(true),
    );
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].status, ColumnPresentationStatus::Ready);
    assert_text(
        &plans[0].present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
}

// ---------------------------------------------------------------------------
// PRD anchors end-to-end
// ---------------------------------------------------------------------------

#[test]
fn prd_anchor_native_iso_tz_shanghai() {
    let plan = plan_for(col("created_at", Some("timestamptz")), &default_snapshot(), &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert_text(
        &plan.present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
}

#[test]
fn prd_anchor_unix_milliseconds_on_datetime_column() {
    let mut configs = BTreeMap::new();
    configs.insert(TIMESTAMP_ID.to_string(), timestamp_config(unix_ms_shanghai_iso_tz()));
    let snapshot = make_snapshot(SnapshotOptions {
        configs: Some(configs),
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("timestamp")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert_text(
        &plan.present(&json!(1705316400000i64)),
        "2024-01-15T19:00:00+08:00",
        "unix",
    );
}

#[test]
fn bug003_keeps_the_millisecond_remainder_via_custom_override() {
    let snapshot = override_snapshot(
        ColumnPresentationOverride::Custom {
            renderer_id: TIMESTAMP_ID.to_string(),
            options: unix_ms_shanghai_iso_tz(),
        },
        "event_ms",
    );
    let disabled_snapshot = ValuePresentationSnapshot {
        enabled_renderer_ids: Vec::new(),
        ..snapshot
    };
    let plan = plan_for(col("event_ms", Some("bigint")), &disabled_snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert_text(
        &plan.present(&json!(1705316400123i64)),
        "2024-01-15T19:00:00.123+08:00",
        "unix",
    );
    assert_text(
        &plan.present(&json!(1705316400000i64)),
        "2024-01-15T19:00:00+08:00",
        "unix",
    );
}

// ---------------------------------------------------------------------------
// Global rules (issue-32 T08) — the cases the T291 audit deferred here
// ---------------------------------------------------------------------------

fn rule(input: serde_json::Value) -> ValuePresentationRule {
    match validate_value_presentation_rule(&input) {
        ValuePresentationRuleValidation::Ok(rule) => *rule,
        ValuePresentationRuleValidation::Err(problems) => {
            panic!("test fixture invalid: {problems:#?}")
        }
    }
}

fn unix_ms_rule() -> ValuePresentationRule {
    rule(json!({
        "name": "unix-ms",
        "enabled": true,
        "match": {
            "column": { "kind": "regex", "source": "^(created_at|updated_at)$" },
            "databaseTypeName": { "kind": "exact", "value": "BIGINT" },
        },
        "options": unix_ms_shanghai_iso_tz(),
    }))
}

#[test]
fn first_matching_rule_applies_and_records_its_name() {
    let snapshot = make_snapshot(SnapshotOptions {
        rules: vec![unix_ms_rule()],
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("BIGINT")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert_eq!(plan.matched_rule_name.as_deref(), Some("unix-ms"));
    assert_text(
        &plan.present(&json!(1705316400000i64)),
        "2024-01-15T19:00:00+08:00",
        "unix",
    );
}

#[test]
fn disabled_rules_fall_through_to_the_global_default() {
    let disabled = rule(json!({
        "name": "off",
        "enabled": false,
        "match": { "column": { "kind": "exact", "value": "created_at" } },
        "options": unix_ms_shanghai_iso_tz(),
    }));
    let snapshot = make_snapshot(SnapshotOptions {
        rules: vec![disabled],
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("BIGINT")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Default);
    assert!(plan.matched_rule_name.is_none());
}

#[test]
fn first_match_freezes_even_when_a_later_rule_would_also_match() {
    let second = rule(json!({
        "name": "unix-s",
        "enabled": true,
        "match": { "column": { "kind": "exact", "value": "created_at" } },
        "options": {
            "interpretation": { "kind": "unix", "unit": "seconds" },
            "format": "iso",
            "displayTimeZone": { "kind": "utc" },
        },
    }));
    let snapshot = make_snapshot(SnapshotOptions {
        rules: vec![unix_ms_rule(), second],
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("BIGINT")), &snapshot, &make_registry(true));
    assert_eq!(plan.matched_rule_name.as_deref(), Some("unix-ms"));
    assert_text(
        &plan.present(&json!(1705316400000i64)),
        "2024-01-15T19:00:00+08:00",
        "unix",
    );
}

#[test]
fn table_conditions_need_a_source() {
    let table_rule = rule(json!({
        "name": "orders-only",
        "enabled": true,
        "match": { "table": { "kind": "exact", "value": "orders" } },
        "options": unix_ms_shanghai_iso_tz(),
    }));
    let snapshot = make_snapshot(SnapshotOptions {
        rules: vec![table_rule],
        ..default_options()
    });
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: vec![col("created_at", Some("BIGINT"))],
            source: None,
            snapshot: &snapshot,
        },
        &make_registry(true),
    );
    assert_eq!(plans[0].status, ColumnPresentationStatus::Default);
    assert!(plans[0].matched_rule_name.is_none());
}

#[test]
fn column_only_rules_apply_without_a_source() {
    let snapshot = make_snapshot(SnapshotOptions {
        rules: vec![unix_ms_rule()],
        ..default_options()
    });
    let plans = build_column_presentation_plans(
        &BuildColumnPresentationPlansInput {
            columns: vec![col("created_at", Some("BIGINT"))],
            source: None,
            snapshot: &snapshot,
        },
        &make_registry(true),
    );
    assert_eq!(plans[0].status, ColumnPresentationStatus::Ready);
    assert_eq!(plans[0].matched_rule_name.as_deref(), Some("unix-ms"));
}

#[test]
fn a_column_override_beats_a_matching_rule() {
    let snapshot = override_snapshot(ColumnPresentationOverride::Raw, "created_at");
    let snapshot = ValuePresentationSnapshot {
        rules: vec![unix_ms_rule()],
        ..snapshot
    };
    let plan = plan_for(col("created_at", Some("BIGINT")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Raw);
    assert!(plan.matched_rule_name.is_none());
}

#[test]
fn rules_do_not_apply_when_the_contribution_is_missing() {
    let snapshot = make_snapshot(SnapshotOptions {
        rules: vec![unix_ms_rule()],
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("BIGINT")), &snapshot, &make_registry(false));
    assert_eq!(plan.status, ColumnPresentationStatus::Default);
    assert!(plan.matched_rule_name.is_none());
}

#[test]
fn no_matching_rule_falls_through_to_the_global_default() {
    let snapshot = make_snapshot(SnapshotOptions {
        rules: vec![unix_ms_rule()],
        ..default_options()
    });
    let plan = plan_for(col("created_at", Some("timestamptz")), &snapshot, &make_registry(true));
    assert_eq!(plan.status, ColumnPresentationStatus::Ready);
    assert!(plan.matched_rule_name.is_none());
    assert_text(
        &plan.present(&json!("2026-01-15T14:30:00Z")),
        "2026-01-15T22:30:00+08:00",
        "native",
    );
}
