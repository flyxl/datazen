//! CM-44 mid-flight cancel contract for the Data Sync **apply** Job
//! (`docs/architecture/platform/data-migration-jobs.md` §5.3, CM-44).
//!
//! # Why this file exists
//!
//! The contract that used to live in `jobs_contract.rs`
//! (`cancel_job_reaches_the_flag_the_running_stage_polls`) called `cancel_job`
//! and then asserted that an `Arc<AtomicBool>` it had obtained beforehand now
//! read `true`. That assertion cannot fail for the reason it was written for.
//! The flag was the one the **host** minted in its own store, and the running
//! stage polled an entirely different flag (the kernel `CancelToken`'s). Set the
//! host's flag, and the test goes green while nothing on the write path can
//! tell. It certified a bridge that was severed at both ends.
//!
//! So nothing here reads a cancel flag. Every assertion is something a user
//! could observe:
//!
//! 1. the apply Job reaches `Cancelled` **while its stage was mid-flight**;
//! 2. the statements queued behind the in-flight one never reach the database
//!    — the driver write counter, not a handle;
//! 3. nothing is committed and the batch lease is released;
//! 4. the reported reason is the apply stage's own cancellation error
//!    (`ApplyFailure::cancelled`), not a generic host rejection.
//!
//! `active_cancel_watchers()` is deliberately absent too: it is incremented
//! when the watcher spawns and only decremented in `Drop`, and `run_stage`
//! holds the handle on its own dispatch stack — so it reads 1 both while the
//! poll loop is live and after it self-exited. It proves nothing.
//!
//! # How the stage is caught mid-flight
//!
//! [`GatedDriver`] counts every write the target endpoint receives and parks
//! the first one until the test opens the gate (see [`Gate`]). Three inserted
//! rows at `batch_size: 2` therefore produce two batches — 2 statements and
//! 1 statement — and the only place that can still refuse work between the two
//! statements of batch 0 is the executor's pre-write checkpoint
//! (`host/executor.rs`). That is exactly the behaviour the contract is about.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use datazen_driver_api::mock_driver::MockDriver;
use datazen_driver_api::{
    ColumnSchema, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DdlAtomicity,
    DriverCategory, DriverError, MultiQueryResult, QueryResult, ServerInfo, TableInfo, TableSchema,
    TransactionHandle, Value,
};
use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::JobState;
use tokio::sync::{mpsc, Semaphore};

use crate::data_sync::job::body::ApplySpec;
use crate::data_sync::SyncOptions;

use super::jobs::{cancel_job, submit_apply};
use super::jobs_contract::{compare, confirm, insert_selection, message, Pair};

/// The runtime's cancel watcher polls the Job record every
/// `CANCEL_POLL_INTERVAL` (50 ms). The test waits 10 polls before opening the
/// gate, so the token is certain to be set before the executor's next
/// checkpoint runs. This is the only wall-clock dependency in the contract.
const SETTLE: Duration = Duration::from_millis(500);

/// More permits than the apply path can ever need, so opening the gate can
/// never park a write that should have run (and turn a regression into a hang).
const OPEN_PERMITS: usize = 64;

// ---------------------------------------------------------------------------
// Gate
// ---------------------------------------------------------------------------

/// A hold-back point in front of the target's write path.
///
/// Zero initial permits, so [`Gate::pass`] parks every write until
/// [`Gate::open`]. That shape matters: a `Notify` would let a write that
/// *should have been refused* block forever when the checkpoint that was
/// supposed to refuse it is missing, and the run would hang instead of
/// failing. Here, opening the gate unconditionally releases everything, so a
/// missing checkpoint shows up as a wrong write count.
struct Gate {
    writes: AtomicUsize,
    reached: mpsc::UnboundedSender<()>,
    permits: Arc<Semaphore>,
}

impl Gate {
    fn new(reached: mpsc::UnboundedSender<()>) -> Self {
        Self {
            writes: AtomicUsize::new(0),
            reached,
            permits: Arc::new(Semaphore::new(0)),
        }
    }

    /// Count the write, announce it, then park until the test opens the gate.
    ///
    /// The announcement happens *before* the park, so the test's "a write is in
    /// flight" signal means the statement has left the executor and reached the
    /// driver's write path — past `begin`, past the per-block keyed
    /// re-verification, and past any checkpoint that was going to fire for it.
    async fn pass(&self) {
        self.writes.fetch_add(1, Ordering::SeqCst);
        let _ = self.reached.send(());
        // The permit is released as soon as this statement finishes; the gate
        // is an arrival barrier, not mutual exclusion.
        let _ = self.permits.clone().acquire_owned().await;
    }

    fn writes(&self) -> usize {
        self.writes.load(Ordering::SeqCst)
    }

    fn open(&self) {
        self.permits.add_permits(OPEN_PERMITS);
    }
}

// ---------------------------------------------------------------------------
// GatedDriver
// ---------------------------------------------------------------------------

/// A pass-through around `MockDriver` that routes the two write entry points
/// through a [`Gate`].
///
/// Every other method delegates to the inner mock rather than re-deriving it:
/// `parameter_placeholder` and `quote_ident` decide the keyed re-read SQL, and
/// `begin_transaction` / `commit` / `rollback` are the only implementations
/// that track lease state — the trait defaults fail closed, so delegating is
/// what keeps this wrapper honest.
struct GatedDriver {
    inner: Arc<MockDriver>,
    gate: Arc<Gate>,
}

#[async_trait]
impl DatabaseDriver for GatedDriver {
    fn driver_type(&self) -> DatabaseType {
        self.inner.driver_type()
    }

    fn sync_family(&self) -> String {
        self.inner.sync_family()
    }

    fn driver_category(&self) -> DriverCategory {
        self.inner.driver_category()
    }

    fn ddl_atomicity(&self) -> DdlAtomicity {
        self.inner.ddl_atomicity()
    }

    fn has_schema_level(&self) -> bool {
        self.inner.has_schema_level()
    }

    fn default_schema(&self) -> Option<&'static str> {
        self.inner.default_schema()
    }

    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        self.inner.qualify_sql_target(sql, database, schema)
    }

    fn quote_char(&self) -> char {
        self.inner.quote_char()
    }

    fn quote_ident(&self, name: &str) -> String {
        self.inner.quote_ident(name)
    }

    fn parameter_placeholder(
        &self,
        index: usize,
        data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        self.inner.parameter_placeholder(index, data_type)
    }

    async fn connect(&self, config: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        self.inner.connect(config).await
    }

    async fn test_connection(&self, config: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        self.inner.test_connection(config).await
    }

    async fn disconnect(&self, handle: ConnectionHandle) -> Result<(), DriverError> {
        self.inner.disconnect(handle).await
    }

    async fn get_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.get_databases(handle).await
    }

    async fn get_tables(
        &self,
        handle: &ConnectionHandle,
        database: &str,
        schema: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        self.inner.get_tables(handle, database, schema).await
    }

    async fn get_table_schema(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        self.inner
            .get_table_schema(handle, table, database, schema)
            .await
    }

    async fn get_columns(
        &self,
        handle: &ConnectionHandle,
        table: &str,
        database: &str,
        schema: Option<&str>,
    ) -> Result<(Vec<ColumnSchema>, Vec<String>), DriverError> {
        self.inner
            .get_columns(handle, table, database, schema)
            .await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query(handle, sql).await
    }

    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.inner.query_multi(handle, sql, limit).await
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_with_params(handle, sql, params).await
    }

    async fn execute_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        self.gate.pass().await;
        self.inner.execute_with_params(handle, sql, params).await
    }

    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.gate.pass().await;
        self.inner.execute(handle, sql).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.inner.begin_transaction(handle).await
    }

    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.inner.begin_read_snapshot(handle).await
    }

    async fn has_complete_foreign_key_catalog_visibility(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<bool, DriverError> {
        self.inner
            .has_complete_foreign_key_catalog_visibility(handle)
            .await
    }

    async fn close_database(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<bool, DriverError> {
        self.inner.close_database(handle, database).await
    }

    async fn open_databases(&self, handle: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        self.inner.open_databases(handle).await
    }

    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.commit(tx).await
    }

    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.rollback(tx).await
    }

    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.inner.cancel_query(handle).await
    }

    async fn get_server_info(&self, handle: &ConnectionHandle) -> Result<ServerInfo, DriverError> {
        self.inner.get_server_info(handle).await
    }
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Source holds three rows the target lacks ⇒ three `Insert` blocks.
async fn gated_pair(prefix: &str) -> (Pair, Arc<Gate>, mpsc::UnboundedReceiver<()>) {
    let (tx, rx) = mpsc::unbounded_channel();
    let gate = Arc::new(Gate::new(tx));
    let held = gate.clone();
    let pair = super::jobs_contract::gated_pair(
        prefix,
        &[(1, "alice"), (2, "bob"), (3, "carol"), (4, "dave")],
        &[(1, "alice")],
        move |inner| Arc::new(GatedDriver { inner, gate: held }) as Arc<dyn DatabaseDriver>,
    )
    .await;
    (pair, gate, rx)
}

// ---------------------------------------------------------------------------
// CM-44: a cancel sent mid-stage stops the writes still queued behind it
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cancelling_a_running_apply_stops_the_writes_the_executor_would_still_send() {
    let (pair, gate, mut reached) = gated_pair("cancel-in-flight").await;
    let preview = compare(&pair, None, None).await.unwrap();
    assert_eq!(preview.selection_revision, 1);

    confirm(
        &preview.plan_id,
        insert_selection(preview.selection_revision),
    );
    // One write per inserted row. Three rows at batch_size 2 ⇒ batch 0 carries
    // two statements, batch 1 carries the third. The cancel therefore lands
    // with a statement already executing and two still to come.
    let job_id = format!("cancel-in-flight-{}", uuid::Uuid::new_v4());

    let apply = async {
        submit_apply(
            pair.state(),
            ApplySpec {
                plan_id: preview.plan_id.clone(),
                selection_revision: preview.selection_revision,
                options: SyncOptions {
                    batch_size: 2,
                    ..SyncOptions::default()
                },
            },
            Some(job_id.clone()),
        )
        .await
    };
    let canceller = async {
        // Wait until a write is genuinely in flight before cancelling, so this
        // is a mid-stage cancel and not a pre-stage one.
        reached
            .recv()
            .await
            .expect("the apply Job must write to the target");
        assert!(
            cancel_job(&job_id).await,
            "cancel_job must report the cancel it just recorded"
        );
        tokio::time::sleep(SETTLE).await;
        gate.open();
    };

    let (result, ()) = tokio::join!(apply, canceller);
    let outcome = result.unwrap();

    assert_eq!(
        gate.writes(),
        1,
        "only the write that was already in flight may reach the target: the \
         pre-write checkpoint must refuse the rest of its own batch and every \
         later batch; host said: {}",
        message(&outcome)
    );
    assert_eq!(
        outcome.state,
        JobState::Cancelled,
        "a cancel delivered mid-stage must end the Job Cancelled"
    );
    assert_eq!(
        outcome.effect,
        EffectOutcome::RolledBack,
        "nothing was committed, so the effect is a rollback, not a partial apply"
    );
    assert_eq!(
        outcome.committed, 0,
        "the in-flight batch was rolled back, so no row counts as committed"
    );
    assert_eq!(
        pair.target.open_transaction_count(),
        0,
        "the cancelled batch lease must be released before the Job reports"
    );
    assert!(
        message(&outcome).starts_with("execute cancelled"),
        "the reason must be the apply stage's own ApplyFailure::cancelled, \
         got: {}",
        message(&outcome)
    );
}

// ---------------------------------------------------------------------------
// CM-44: a cancel that arrives before the Job record does still stops the run
// ---------------------------------------------------------------------------

/// The window can hit Cancel on an id the backend has never seen — the user
/// closes the compare step and cancels while the apply Job is still being
/// accepted. There is no Job record to write `cancel_requested` into yet, so
/// the intent lives only in the pre-Job window registry, and `submit_apply` is
/// the only thing that can carry it forward.
///
/// This is the production half of CM-44. The legacy statement path has its own
/// CM-44 test (`commands::sync::tests::cancel_data_sync_stops_execute_before_start`),
/// but that path reads the window flag directly and never enters `drive`, so it
/// would stay green with the carry-forward deleted. Mutation testing is what
/// caught that.
#[tokio::test]
async fn a_cancel_that_arrives_before_the_job_exists_still_stops_the_apply() {
    let (pair, gate, _reached) = gated_pair("cancel-before-job").await;
    let preview = compare(&pair, None, None).await.unwrap();
    assert_eq!(preview.selection_revision, 1);

    confirm(
        &preview.plan_id,
        insert_selection(preview.selection_revision),
    );
    let job_id = format!("cancel-before-job-{}", uuid::Uuid::new_v4());

    // No Job record exists for this id yet, so the kernel half of cancel_job
    // reports NotFound and only the window registry accepts the intent.
    assert!(
        cancel_job(&job_id).await,
        "the window registry must accept a cancel for an id it has not seen"
    );

    let outcome = submit_apply(
        pair.state(),
        ApplySpec {
            plan_id: preview.plan_id.clone(),
            selection_revision: preview.selection_revision,
            options: SyncOptions {
                batch_size: 2,
                ..SyncOptions::default()
            },
        },
        Some(job_id),
    )
    .await
    .unwrap();

    assert_eq!(
        gate.writes(),
        0,
        "submit_apply must carry the pre-Job cancel into the Job record, so \
         the stage never runs and no statement reaches the target; host said: {}",
        message(&outcome)
    );
    assert_eq!(
        outcome.state,
        JobState::Cancelled,
        "a cancel that beat the Job must end it Cancelled, not Succeeded"
    );
    assert_eq!(
        outcome.effect,
        EffectOutcome::NotStarted,
        "nothing ran, so the effect is NotStarted, not a rollback"
    );
    assert_eq!(outcome.committed, 0);
    assert_eq!(
        pair.target.open_transaction_count(),
        0,
        "a Job that never started must never open a batch lease"
    );
    assert!(
        message(&outcome).starts_with("execute cancelled"),
        "the reason must name the cancel, got: {}",
        message(&outcome)
    );
}
