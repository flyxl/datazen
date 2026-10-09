//! The read side of a transfer Job handle.
//!
//! The frontend client declares `getJob(jobId)` and `listJobs(filter)`, and its
//! polling loop depends on `getJob` resolving — but the host had no command under
//! either name, so both methods reached a missing IPC handler and the id an apply
//! Job returns could never be read back. These tests pin the two contracts that
//! make those two client methods work: the wire shape the client's parser accepts,
//! and narrowing that the caller is entitled to expect.
//!
//! Two of the three filters the client can send are not the repository's filters.
//! `JobFilter` carries states, owner, a counter cursor and a limit; the in-memory
//! repository honours states and owner and silently drops the other two, and it
//! has no notion of a job kind at all. So `kind` and `limit` are applied here,
//! host-side, and each of them gets its own assertion below.

use datazen_platform_api::dto::execution::EffectOutcome;
use datazen_platform_api::dto::job::{JobProgress, JobState, JobView};

use super::super::queries::{read_job, read_jobs};
use super::super::runtime::{APPLY_KIND, PREPARE_KIND};
use super::super::{admit_apply, prepare_data_transfer_job_impl, TransferPrepareJobView};
use super::{
    apply_and_wait, apply_request_from, direct_job, mock_options, prepare_request, sql_file_job,
    TestAppState,
};

/// Serialize a view the way the IPC layer does and hand back the JSON object.
fn wire(view: &JobView) -> serde_json::Map<String, serde_json::Value> {
    serde_json::to_value(view)
        .expect("a Job view is plain serializable data")
        .as_object()
        .expect("a Job view serializes as an object")
        .clone()
}

fn ids(views: &[JobView]) -> Vec<String> {
    views
        .iter()
        .map(|view| view.job_id.as_str().to_string())
        .collect()
}

/// `getJob` must return a payload the client can parse without repairing it.
///
/// The client's parser rejects the whole view when `jobId`, `kind`, `state`,
/// `createdAt` or `updatedAt` is missing or mistyped, and it only accepts
/// camelCase keys — a snake_case field would simply not be found. So this asserts
/// the exact key set the parser reads, and asserts the two timestamps are real
/// instants rather than opaque strings: the parser hands them to `Date.parse`,
/// which rejects a string it cannot read.
#[tokio::test]
async fn get_job_returns_a_payload_the_client_parser_accepts() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-wire-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-wire-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    let applied = apply_and_wait(&test.state, apply_request_from(&prepared))
        .await
        .expect("the apply Job must run");

    let view = read_job(&test.state, &applied.job_id)
        .await
        .expect("a finished Job is still readable by its id");
    let object = wire(&view);

    for key in [
        "jobId",
        "kind",
        "state",
        "executionIds",
        "artifactIds",
        "createdAt",
        "updatedAt",
        "cancelRequested",
        "progress",
    ] {
        assert!(
            object.contains_key(key),
            "the client reads `{key}`; without it the whole view is rejected: {object:?}"
        );
    }
    for key in [
        "job_id",
        "kind_id",
        "execution_ids",
        "artifact_ids",
        "created_at",
        "updated_at",
        "cancel_requested",
        "effect_outcome",
        "pending_verification_reason",
    ] {
        assert!(
            !object.contains_key(key),
            "`{key}` is the Rust spelling; the client only looks for camelCase, \
             so publishing it tells the caller nothing"
        );
    }
    for key in ["createdAt", "updatedAt"] {
        let raw = object[key].as_str().unwrap_or_else(|| {
            panic!("the client accepts a finite number or a parseable string: {key}={object:?}")
        });
        chrono::DateTime::parse_from_rfc3339(raw).unwrap_or_else(|error| {
            panic!("`{key}` is not an instant the client can read: {error}")
        });
    }
    assert_eq!(
        object["state"],
        serde_json::json!("succeeded"),
        "the client matches the state against a fixed camelCase vocabulary"
    );
    let progress = object["progress"]
        .as_object()
        .expect("progress is an object of counters");
    for key in ["read", "converted", "attempted", "committed", "unknown"] {
        assert!(
            progress.contains_key(key),
            "the client reads progress.{key} and defaults it to zero when absent: {progress:?}"
        );
        // Presence is not the contract; the *type* is. `platform-api`'s `Counter`
        // is a `u64` serialized with `collect_str` (CM-01: a JSON number would
        // lose precision above 2^53), so each counter travels as a decimal
        // string. Asserting only `contains_key` is what let this drift: the key
        // was always there, so the test stayed green while every counter the
        // client read was silently rejected by `toCounter` and defaulted to 0.
        assert!(
            progress[key].is_string(),
            "`{key}` must travel as a decimal string, not {:?} — a bare number \
             loses u64 precision, and a client that only accepts numbers reads \
             every counter as 0: {progress:?}",
            progress[key],
        );
        let raw = progress[key].as_str().expect("checked above");
        assert!(
            !raw.is_empty() && raw.bytes().all(|b| b.is_ascii_digit()),
            "`{key}` must be a plain decimal integer, not `{raw}`: {progress:?}"
        );
        assert!(
            raw.parse::<u64>().is_ok(),
            "`{key}` must fit the u64 the kernel counts in: `{raw}`"
        );
    }
    // Type is necessary but not sufficient: a wire form the client cannot read
    // and a wire form that always says "0" look the same to a shape-only test.
    // This run read rows through to the target, so at least one counter has to
    // have actually moved — prove the values travel, not just their spelling.
    let committed = progress["committed"].as_str().expect("checked above");
    assert_ne!(
        committed, "0",
        "the apply committed rows, so `committed` cannot travel as 0 — if it does, \
         the payload has lost the value and only its shape survives: {progress:?}"
    );
}

/// `listJobs` must narrow by state and by kind, because the repository cannot.
///
/// The repository's `list` honours states but has no kind notion, and the client
/// filters by kind on its own side — so a host that passed a kind straight through
/// would answer with every Job in the process and let the UI discard them. Each
/// assertion below uses only ids this test itself created, so the shared
/// process-wide repository cannot make them pass or fail by accident.
#[tokio::test]
async fn list_jobs_narrows_by_state_and_by_kind() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-list-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-list-target").await;

    // A prepare Job is a terminal `dataTransferPrepare`; an apply Job admitted
    // here is a `queued` `dataTransferApply`. Same host, same moment, different
    // kind and different state — which is what makes them usable as each other's
    // negative control.
    let prepared: TransferPrepareJobView =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    let admitted = admit_apply(&test.state, apply_request_from(&prepared))
        .await
        .expect("the apply Job must be admitted");
    let apply_id = admitted.view.job_id.clone();

    // The Job is read while it is still owed a write. Listing it before anything
    // drives it is the whole point: this is the window a caller polls in, and a
    // Job that is invisible here is a Job that cannot be watched or cancelled.
    let queued_applys = read_jobs(
        &test.state,
        Some(vec![JobState::Queued]),
        Some(APPLY_KIND.to_string()),
        None,
    )
    .await
    .expect("listing must not fail");
    let prepare_jobs = read_jobs(&test.state, None, Some(PREPARE_KIND.to_string()), None)
        .await
        .expect("listing must not fail");

    let queued_ids = ids(&queued_applys);
    let prepare_ids = ids(&prepare_jobs);
    assert!(
        queued_ids.contains(&apply_id),
        "the Job this test admitted is queued and is an apply Job: {queued_ids:?}"
    );
    assert!(
        !prepare_ids.contains(&apply_id),
        "an apply Job must never appear in the prepare listing: {prepare_ids:?}"
    );
    assert!(
        prepare_ids.contains(&prepared.job_id),
        "the review Job this test created is a prepare Job: {prepare_ids:?}"
    );
    assert!(
        !queued_ids.contains(&prepared.job_id),
        "a finished prepare Job must not appear in the queued apply listing: {queued_ids:?}"
    );

    admitted
        .drive
        .expect("an admitted apply still owes a write")
        .finish()
        .await
        .expect("the detached write must report its verdict");

    // The admitted Job has since finished, so the same id drops out of a `queued`
    // listing and shows up under `succeeded`. A `listJobs` that ignored the state
    // filter would still answer `queued` here.
    let succeeded_applys = read_jobs(
        &test.state,
        Some(vec![JobState::Succeeded]),
        Some(APPLY_KIND.to_string()),
        None,
    )
    .await
    .expect("listing must not fail");
    assert!(
        ids(&succeeded_applys).contains(&apply_id),
        "a finished apply Job belongs in the succeeded listing"
    );
    let none_are_queued = read_jobs(
        &test.state,
        Some(vec![JobState::Queued]),
        Some(APPLY_KIND.to_string()),
        None,
    )
    .await
    .expect("listing must not fail");
    assert!(
        !ids(&none_are_queued).contains(&apply_id),
        "a job that has already finished must not be reported as queued"
    );

    // The counters are the other thing the read side owes the caller. The
    // repository writes them once as zero and has no port to change them, so
    // without the merge both listing and reading would report a finished copy
    // as having moved nothing. This is the assertion that dies with the merge
    // removed, so it belongs to the module that owns `view()` rather than to a
    // test that happens to read the same Job.
    let listed = succeeded_applys
        .iter()
        .find(|view| view.job_id.as_str() == apply_id)
        .expect("the finished apply is in the listing it was just matched in");
    let single = read_job(&test.state, &apply_id)
        .await
        .expect("a finished Job is still readable by its id");
    assert_ne!(
        single.progress,
        JobProgress::default(),
        "getJob must publish the counters the run accumulated: {:?}",
        single.progress
    );
    assert_eq!(
        listed.progress, single.progress,
        "the two read paths must not disagree about a Job's counters"
    );
}

/// `listJobs` must apply the limit, because the repository drops it.
///
/// The in-memory repository sorts by creation and never truncates, so a host that
/// forwarded `limit` into the filter would look like it honoured the request
/// while returning everything. `limit: 0` is the sharpest probe: it is legal, and
/// a host that ignores it returns the full list.
#[tokio::test]
async fn list_jobs_applies_a_limit_the_repository_never_enforces() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_source_config, source) = test.save_and_connect("p5ja-limit-source").await;
    let (_target_config, target) = test.save_and_connect("p5ja-limit-target").await;
    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(direct_job(&source, &target)))
            .await
            .expect("a direct-pair review should issue a plan");
    apply_and_wait(&test.state, apply_request_from(&prepared))
        .await
        .expect("the apply Job must run");

    let unlimited = read_jobs(&test.state, None, None, None)
        .await
        .expect("listing must not fail");
    assert!(
        unlimited.len() >= 2,
        "this test created a review Job and an apply Job: {}",
        unlimited.len()
    );

    let none = read_jobs(&test.state, None, None, Some(0))
        .await
        .expect("listing must not fail");
    assert!(
        none.is_empty(),
        "a limit of zero asks for nothing, and the repository ignores it: {} jobs came back",
        none.len()
    );

    let one = read_jobs(&test.state, None, None, Some(1))
        .await
        .expect("listing must not fail");
    assert_eq!(
        one.len(),
        1,
        "the oldest Job wins the single slot: {}",
        one.len()
    );
}

/// An SQL-file migration publishes its artifact id from the host, and the read
/// side has to keep publishing it.
///
/// Detaching the write means nothing reads the return value of the run that
/// emitted the SQL file, so the id is remembered on the host and merged into
/// every view of that Job. Without the merge the Job view would come back with an
/// empty `artifactIds` for a Job that really did emit a file — a silent loss of
/// the only handle to what was written.
#[tokio::test]
async fn a_sql_file_job_publishes_its_artifact_through_the_read_side() {
    let test = TestAppState::with_options(mock_options()).await;
    let (_config, source) = test.save_and_connect("p5ja-artifact-source").await;
    let dir = tempfile::tempdir().expect("temporary SQL output directory");
    let token = crate::data_transfer::sql_file::register_path(dir.path().join("transfer.sql"))
        .expect("register the SQL destination");

    let prepared =
        prepare_data_transfer_job_impl(&test.state, prepare_request(sql_file_job(source, token)))
            .await
            .expect("a SQL-file job must be reviewable");
    let applied = apply_and_wait(&test.state, apply_request_from(&prepared))
        .await
        .expect("a SQL-file job must clear the gate on the apply path too");
    assert_eq!(applied.state, JobState::Succeeded, "{:?}", applied.error);

    let emitted = applied
        .artifact_ids
        .iter()
        .find(|id| id.starts_with("transfer-sql-"))
        .cloned()
        .unwrap_or_else(|| {
            panic!(
                "the emitted SQL file must be published as a content-addressed \
                 artifact: {:?}",
                applied.artifact_ids
            )
        });

    let view = read_job(&test.state, &applied.job_id)
        .await
        .expect("a finished Job is still readable by its id");
    assert!(
        view.artifact_ids.iter().any(|id| id.as_str() == emitted),
        "the read side must publish the same artifact the apply view did: \
         the run is detached, so this is the only place it can come from: {:?}",
        view.artifact_ids
    );
    assert_eq!(view.state, JobState::Succeeded);
    // Zero here is the truthful value, not a lost mirror: the SQL-file stage
    // reports `JobProgress::default()` — it emits a file and counts nothing per
    // row — so a non-zero here would mean the host had invented counters.
    // Progress mirroring is proved on the database-target path, where the
    // handler really does accumulate.
    assert_eq!(
        view.progress,
        JobProgress::default(),
        "an emit-only stage counts no rows, so the mirrored counters must stay zero: {:?}",
        view.progress
    );
    assert_eq!(
        view.effect_outcome,
        Some(EffectOutcome::Completed),
        "the recorded effect outcome is part of the Job the client watches: {:?}",
        view.effect_outcome
    );
}
