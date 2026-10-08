//! Contract tests for the Data Sync prepare/apply Job pair at the **command**
//! layer (data-migration-jobs.md §2.1, §5, §10 CM-41/43/44/45).
//!
//! `packages/data-sync/tests/cm4*` drive `DataSyncHandler` against a `FakeHost`.
//! That proves the handler's own refusals, but a handler that is never reached
//! passes them too — so every test here enters through the production path
//! (`compare_data_sync_impl`, `jobs::submit_prepare`, `jobs::submit_apply`,
//! `exec::start_data_sync_apply_job_impl`) and reaches real drivers through
//! `ConnectionManager`. The gate chain, the budget reservation, the plan
//! consumption and the effect-outcome projection are therefore part of what is
//! asserted, not decoration around it.
//!
//! One fixture serves every case: two endpoints of **one** family served by two
//! **distinct** mock driver instances. Distinct instances are not a detail —
//! two sessions of one instance hand both sides identical rows, every compare
//! reports no diff, and the apply path becomes unreachable.

use std::collections::HashMap;
use std::sync::Arc;

use datazen_driver_api::mock_driver::MockDriver;
use datazen_driver_api::{ColumnSchema, DatabaseDriver, TableSchema};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::JobState;

use crate::data_sync::job::body::{ApplySpec, PrepareSpec};
use crate::data_sync::Endpoint;
use crate::data_sync::{ChangeOperation, SyncOptions, TableMapping};
use crate::testing::app_state::{rich_mock_options, sample_postgres_config, TestAppState};
use crate::testing::mock_driver::MockDriverOptions;

use super::super::error::CommandError;
use super::super::AppState;
use super::host::state::{self, StoredSelection};
use super::jobs::{submit_apply, submit_prepare, SyncJobOutcome};
use super::plans::{
    SyncComparisonPreview, SyncRunSelection, SyncSelectionMode, SyncTableSelection,
};

/// Mock rows are `(id, name)` pairs against the default `id/name` schema.
pub fn mock_options(rows: &[(i64, &str)]) -> MockDriverOptions {
    MockDriverOptions {
        // One page then end-of-stream on both sides. Without this the handler
        // sees the same page twice and refuses the compare ("not strictly
        // increasing") instead of freezing a change set.
        empty_keyset_after_cursor: true,
        parameterized_writes: true,
        // The apply handler re-reads the reviewed row by key before writing it
        // (§5.3). Without a keyed read the double answers every key with the
        // first row, and a legitimate `Insert` would look like target drift.
        filter_rows_by_key_equality: true,
        execute_rows_affected: 1,
        count_total: rows.len() as i64,
        query_rows: rows
            .iter()
            .map(|(id, name)| {
                vec![
                    Some(crate::db::Value::Integer(*id)),
                    Some(crate::db::Value::String((*name).to_string())),
                ]
            })
            .collect(),
        ..rich_mock_options()
    }
}

/// Two connected endpoints of one family, backed by different mocks.
pub struct Pair {
    pub test: TestAppState,
    pub target: Arc<MockDriver>,
    pub source_session: String,
    pub target_session: String,
}

impl Pair {
    pub fn state(&self) -> &AppState {
        &self.test.state
    }
}

pub async fn pair(prefix: &str, source_rows: &[(i64, &str)], target_rows: &[(i64, &str)]) -> Pair {
    gated_pair(prefix, source_rows, target_rows, |target| target).await
}

/// The same fixture as [`pair`], but the target endpoint is served by whatever
/// `wrap` returns instead of the bare mock.
///
/// The inner mock is still returned on [`Pair::target`], so a wrapper can hold
/// back writes while the test keeps querying the mock's own counters through
/// the registry.
pub async fn gated_pair(
    prefix: &str,
    source_rows: &[(i64, &str)],
    target_rows: &[(i64, &str)],
    wrap: impl FnOnce(Arc<MockDriver>) -> Arc<dyn DatabaseDriver>,
) -> Pair {
    let test = TestAppState::with_options(mock_options(source_rows)).await;
    // The target is registered under a *different* db type spelling of the same
    // family, which is what the pairing rule canonicalises (`family_of`).
    let target = MockDriver::new("postgresql", mock_options(target_rows));
    test.registry
        .register_test_driver("postgresql", wrap(target.clone()))
        .await;
    let mut target_config = sample_postgres_config(&format!("{prefix}-target"));
    target_config.database_type = "postgresql".into();
    target_config.host = Some("target.example.invalid".into());
    test.store.save_connection(target_config).await.unwrap();
    let (_, source_session) = test.save_and_connect(&format!("{prefix}-source")).await;
    let target_session = test.connect_config(&format!("{prefix}-target")).await;
    Pair {
        test,
        target,
        source_session,
        target_session,
    }
}

/// Source holds one row the target lacks ⇒ exactly one `Insert` block.
pub async fn diff_pair(prefix: &str) -> Pair {
    pair(prefix, &[(1, "alice"), (2, "bob")], &[(1, "alice")]).await
}

/// Both sides hold the same row ⇒ the compare succeeds with an empty change set.
pub async fn identical_pair(prefix: &str) -> Pair {
    pair(prefix, &[(1, "alice")], &[(1, "alice")]).await
}

/// Run the production compare command: inspect → key contracts → prepare Job →
/// planId + `ComparisonStore`.
pub async fn compare(
    pair: &Pair,
    source_database: Option<String>,
    target_database: Option<String>,
) -> Result<SyncComparisonPreview, CommandError> {
    super::apply::compare_data_sync_impl(
        pair.state(),
        pair.source_session.clone(),
        pair.target_session.clone(),
        Vec::new(),
        None,
        source_database,
        target_database,
        None,
        None,
        SyncOptions::default(),
        &[],
        &HashMap::new(),
    )
    .await
}

/// The reviewed selection a user confirms for a planId: every operation of the
/// `users` table, at the revision the review panel displayed.
pub fn insert_selection(revision: u64) -> SyncRunSelection {
    SyncRunSelection {
        revision,
        rows: Vec::new(),
        scopes: vec![SyncTableSelection {
            source_table: "users".into(),
            target_table: "users".into(),
            selection_mode: SyncSelectionMode::All,
            operations: vec![ChangeOperation::Insert],
            excluded_rows: Vec::new(),
        }],
    }
}

/// Hand the apply Job its single-consumption selection, exactly as the IPC layer
/// does after the user confirms.
pub fn confirm(plan_id: &str, selection: SyncRunSelection) {
    state::store_confirmed_selection(
        plan_id,
        StoredSelection {
            selection,
            options: SyncOptions::default(),
        },
    );
}

/// A target schema that no longer matches the frozen fingerprint.
fn drifted_schema() -> TableSchema {
    let mut schema = MockDriver::default_table_schema("users");
    schema.columns.push(ColumnSchema {
        name: "nickname".into(),
        data_type: "text".into(),
        nullable: true,
        default_value: None,
        comment: None,
        is_primary_key: false,
        is_auto_increment: false,
    });
    schema
}

pub fn message(outcome: &SyncJobOutcome) -> String {
    outcome.message.clone().unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Prepare through the command layer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn prepare_job_freezes_a_change_set_and_closes_both_snapshots() {
    let pair = diff_pair("prepare-freeze").await;
    let spec = PrepareSpec {
        source: Endpoint {
            connection_id: pair.source_session.clone(),
            database: "app".into(),
            schema: None,
        },
        target: Endpoint {
            connection_id: pair.target_session.clone(),
            database: "app".into(),
            schema: None,
        },
        mappings: vec![TableMapping::auto("users")],
        options: SyncOptions::default(),
        filters: HashMap::new(),
        plan_id: Some("prepare-freeze-plan".into()),
    };

    let outcome = submit_prepare(pair.state(), spec, None).await.unwrap();

    assert_eq!(outcome.state, JobState::Succeeded);
    assert_eq!(outcome.effect, EffectOutcome::Completed);
    assert_eq!(message(&outcome), "", "a clean prepare reports no reason");
    let artifact = state::load_artifact("prepare-freeze-plan")
        .expect("the prepare Job freezes an artifact under the planId");
    assert_eq!(artifact.blocks.len(), 1);
    assert_eq!(artifact.blocks[0].operation, ChangeOperation::Insert);
    assert_eq!(
        pair.test.mock.open_transaction_count(),
        0,
        "the compare read snapshot must be closed by the prepare Job"
    );
    assert_eq!(pair.target.open_transaction_count(), 0);
}

// ---------------------------------------------------------------------------
// Apply through the command layer
// ---------------------------------------------------------------------------

#[tokio::test]
async fn apply_job_commits_the_confirmed_selection_once() {
    let pair = diff_pair("apply-once").await;
    let preview = compare(&pair, None, None).await.unwrap();
    assert_eq!(preview.selection_revision, 1);
    // The preview is only trustworthy because the prepare Job froze it.
    let artifact = state::load_artifact(&preview.plan_id).expect("frozen change set");
    assert_eq!(artifact.blocks.len(), 1);
    assert_eq!(artifact.blocks[0].operation, ChangeOperation::Insert);

    confirm(
        &preview.plan_id,
        insert_selection(preview.selection_revision),
    );
    let outcome = submit_apply(
        pair.state(),
        ApplySpec {
            plan_id: preview.plan_id.clone(),
            selection_revision: preview.selection_revision,
            options: SyncOptions::default(),
        },
        None,
    )
    .await
    .unwrap();

    assert_eq!(
        outcome.state,
        JobState::Succeeded,
        "the apply Job must commit the confirmed selection; host said: {}",
        message(&outcome)
    );
    assert_eq!(outcome.effect, EffectOutcome::Completed);
    assert_eq!(outcome.committed, 1);
    assert_eq!(
        pair.target.open_transaction_count(),
        0,
        "every batch lease must be released before the Job reports Succeeded"
    );
}

// ---------------------------------------------------------------------------
// Rejections that must happen before (or instead of) any Job
// ---------------------------------------------------------------------------

#[tokio::test]
async fn compare_refuses_a_non_database_target_before_any_job_is_submitted() {
    let pair = diff_pair("non-db-target").await;
    let mut foreign_config = sample_postgres_config("non-db-target-foreign");
    foreign_config.database_type = "redis".into();
    pair.test
        .store
        .save_connection(foreign_config)
        .await
        .unwrap();
    pair.test
        .registry
        .register_test_driver("redis", MockDriver::new("redis", mock_options(&[])))
        .await;
    let foreign = pair.test.connect_config("non-db-target-foreign").await;

    // The pairing rule runs in the compare pre-flight (apply.rs), above
    // `open_job`, so nothing is accepted and no planId is minted.
    let error = super::apply::compare_data_sync_impl(
        pair.state(),
        pair.source_session.clone(),
        foreign,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        SyncOptions::default(),
        &[],
        &HashMap::new(),
    )
    .await
    .unwrap_err();
    let text = error.to_string();
    assert!(
        text.contains("not supported"),
        "a non-database target must be refused by the pairing rule (pairing.rs \
         `require_data_sync_family`), got: {text}"
    );
}

#[tokio::test]
async fn an_empty_database_scope_resolves_to_the_connection_database() {
    let pair = diff_pair("empty-scope").await;
    // No explicit database on either side: `resolve_db_name` falls back to the
    // persisted connection config and never errors, so the frozen artifact —
    // not a rejection — is where the scope must become visible.
    let preview = compare(&pair, None, None).await.unwrap();
    let artifact = state::load_artifact(&preview.plan_id).unwrap();
    assert_eq!(artifact.source.database, "app");
    assert_eq!(artifact.target.database, "app");

    let blank = compare(&pair, Some("   ".into()), Some("   ".into()))
        .await
        .unwrap();
    let resolved = state::load_artifact(&blank.plan_id).unwrap();
    assert_eq!(
        resolved.source.database, "app",
        "a blank scope is trimmed and falls back, never silently scoping nothing"
    );
}

#[tokio::test]
async fn the_budget_refuses_a_self_overlapping_pair_before_the_compare_reads_a_row() {
    // One connection profile, two db sessions: same owner ⇒ the same
    // `sync|<owner>` service key for both endpoints, same database object read
    // and written.
    let test = TestAppState::with_options(mock_options(&[(1, "alice")])).await;
    test.save_connection("self-sync").await;
    // `get_or_connect_session` shares one db session per profile, so the two
    // endpoints of a self-sync are opened explicitly: two sessions of one
    // profile still carry the same owner, and the owner is the service key.
    let source = test
        .state
        .connection_manager
        .connect("self-sync")
        .await
        .expect("open the source session of one profile");
    let target = test
        .state
        .connection_manager
        .connect("self-sync")
        .await
        .expect("open the target session of the same profile");
    assert_ne!(source, target, "a self-sync needs two distinct db sessions");

    let error = super::apply::compare_data_sync_impl(
        &test.state,
        source,
        target,
        Vec::new(),
        None,
        None,
        None,
        None,
        None,
        SyncOptions::default(),
        &[],
        &HashMap::new(),
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("jobBudgetRejected"),
        "the durable endpoint budget must refuse the self-sync pair before reads, got: {error}"
    );
    assert_eq!(
        test.mock.open_transaction_count(),
        0,
        "the refusal must precede every driver round trip"
    );
}

// ---------------------------------------------------------------------------
// CM-41: one planId, one consumption — even when the apply fails
// ---------------------------------------------------------------------------

#[tokio::test]
async fn preflight_target_drift_does_not_consume_the_durable_plan_receipt() {
    let pair = diff_pair("failed-consumes").await;
    let preview = compare(&pair, None, None).await.unwrap();
    confirm(
        &preview.plan_id,
        insert_selection(preview.selection_revision),
    );
    // The target drifts after the compare: the apply Job must re-verify the
    // structure fingerprint and refuse before writing anything.
    pair.target
        .set_table_schema_for_test("app", "users", drifted_schema());

    let error = match submit_apply(
        pair.state(),
        ApplySpec {
            plan_id: preview.plan_id.clone(),
            selection_revision: preview.selection_revision,
            options: SyncOptions::default(),
        },
        None,
    )
    .await
    {
        Err(error) => error,
        Ok(_) => panic!("preflight must refuse a stale target before accepting an apply"),
    };
    assert!(
        error.to_string().contains("target schema/key changed"),
        "the stale target must be refused before durable accept, got: {error}"
    );
    assert_eq!(pair.target.open_transaction_count(), 0);

    // The refusal happened before durable accept, so repairing the target may
    // use the same reviewed plan. Once accepted, retries return that receipt.
    pair.target.set_table_schema_for_test(
        "app",
        "users",
        MockDriver::default_table_schema("users"),
    );
    let outcome = submit_apply(
        pair.state(),
        ApplySpec {
            plan_id: preview.plan_id.clone(),
            selection_revision: preview.selection_revision,
            options: SyncOptions::default(),
        },
        None,
    )
    .await
    .expect("the unconsumed plan can be applied after the target is repaired");
    assert_eq!(outcome.state, JobState::Succeeded);
    assert_eq!(outcome.committed, 1);
    let receipt = super::jobs::find_apply_receipt_for_plan(pair.state(), &preview.plan_id)
        .await
        .unwrap()
        .expect("the accepted durable apply receipt remains discoverable");
    assert_eq!(receipt.state, JobState::Succeeded);
}

#[tokio::test]
async fn two_concurrent_applies_of_one_plan_id_commit_once() {
    let pair = diff_pair("concurrent-apply").await;
    let preview = compare(&pair, None, None).await.unwrap();
    confirm(
        &preview.plan_id,
        insert_selection(preview.selection_revision),
    );
    let spec = || ApplySpec {
        plan_id: preview.plan_id.clone(),
        selection_revision: preview.selection_revision,
        options: SyncOptions::default(),
    };

    let (first, second) = tokio::join!(
        submit_apply(pair.state(), spec(), None),
        submit_apply(pair.state(), spec(), None),
    );
    let first = first.expect("first request receives an accepted durable job");
    let second = second.expect("retry receives the same durable receipt");
    assert_eq!(
        first.job_id, second.job_id,
        "one plan has one apply receipt"
    );
    assert_eq!(
        first.state,
        JobState::Succeeded,
        "the winner commits; host said: {}",
        message(&first)
    );
    assert_eq!(
        second.state,
        JobState::Succeeded,
        "the retry observes the original completed job"
    );
    assert_eq!(first.committed, 1, "the durable receipt records one row");
    assert_eq!(second.committed, 1, "a retry does not change the receipt");
    assert_eq!(pair.target.open_transaction_count(), 0);
}
