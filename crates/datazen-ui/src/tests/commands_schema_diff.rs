//! Independent audit for T377 (the commands/schemaDiff.ts port).
//!
//! Tester-written: every assertion comes from re-reading
//! `packages/frontend/src/commands/schemaDiff.ts`, the domain client
//! (`packages/backend-client/src/schemaDiffClient.ts`) and the React wire
//! tests (`commands/__tests__/schemaDiff.test.ts`): the pure helpers, the
//! MethodMap payload shapes (key-for-key, `undefined` → key absent on the
//! wire), the tagged-enum requirement folding (in and out), and the
//! submit→poll→resolve machine's effect sequence, cadence and exact error
//! wording. The machine is driven through its public effect API — no
//! fabricated async, no self-reported state.

use serde_json::json;

use crate::commands::schema_diff::{
    DESTRUCTIVE_CONFIRM_TOKEN, DeployStatusVm, ExecuteDeployParamsVm, PlanRequirementKindVm,
    PlanRequirementVm, PreparePlanParamsVm, PrepareUnifiedParamsVm, RollbackCompletenessVm,
    SchemaDiffConfigJson, SchemaDiffEffect, SchemaDiffJobBus, SchemaDiffJobRun,
    SchemaDiffJobRunOutcome, SchemaDiffJobUpdateVm, SchemaDiffMethod, SchemaDiffPlanVm,
    SchemaDiffProfileVm, SchemaDiffRunKind, SchemaDiffSubmissionTokenVm, StatementRiskVm,
    cancel_deploy_request, compare_table_schemas_request, delete_profile_request,
    dialect_supports_transactional_ddl, execute_deploy_request, export_plan_sql,
    get_job_details_request, get_profiles_request, list_jobs_request, plan_has_destructive,
    prepare_plan_request, prepare_unified_plan_request, rollback_completeness_counts,
    save_profile_request, verify_recovery_request, with_submission_token,
};
use crate::commands::schema_diff::wire::{ColumnTypeOverrideVm, normalize_job_details};
use crate::views::data_transfer::data_transfer_attached_jobs::JobState;
use crate::views::schema_diff::schema_diff_object_identity::{
    SchemaDiffObjectIdentityVm, SchemaDiffObjectKindVm,
};

fn statement(sql: &str, risk: StatementRiskVm, requires_tx: bool) -> crate::commands::schema_diff::wire::PlanStatementVm {
    crate::commands::schema_diff::wire::PlanStatementVm {
        sql: sql.to_owned(),
        risk,
        rollback_sql: None,
        summary: String::new(),
        requires_transaction: Some(requires_tx),
    }
}

/// The React test's `samplePlan`.
fn sample_plan() -> SchemaDiffPlanVm {
    SchemaDiffPlanVm {
        table: "users".to_owned(),
        tables: vec!["users".to_owned()],
        source_dialect: "postgres".to_owned(),
        target_dialect: "mysql".to_owned(),
        same_dialect: false,
        statements: Vec::new(),
        warnings: Vec::new(),
        requirements: Vec::new(),
        rollback_completeness: RollbackCompletenessVm { complete: true, missing: Vec::new() },
        plan_id: None,
        type_suggestions: None,
    }
}

/// The React test's `TEST_TOKEN` (a `const` cannot call `to_owned`).
fn test_token() -> SchemaDiffSubmissionTokenVm {
    SchemaDiffSubmissionTokenVm {
        idempotency_key: "test-key".to_owned(),
        expires_at: "2999-01-01T00:00:00Z".to_owned(),
    }
}

// ---- the pure helpers ---------------------------------------------------------

#[test]
fn destructive_flags_destructive_and_rewrite_only() {
    let mut plan = sample_plan();
    assert!(!plan_has_destructive(&plan));
    plan.statements = vec![statement("CREATE TABLE t()", StatementRiskVm::Additive, false)];
    assert!(!plan_has_destructive(&plan));
    plan.statements = vec![statement("DROP TABLE t", StatementRiskVm::Destructive, false)];
    assert!(plan_has_destructive(&plan));
    plan.statements = vec![statement("ALTER TABLE t", StatementRiskVm::Rewrite, false)];
    assert!(plan_has_destructive(&plan));
}

#[test]
fn transactional_ddl_covers_postgres_and_sqlite_only() {
    for dialect in ["postgresql", "postgres", "SQLite", "POSTGRES"] {
        assert!(dialect_supports_transactional_ddl(dialect), "{dialect}");
    }
    for dialect in ["mysql", "mariadb"] {
        assert!(!dialect_supports_transactional_ddl(dialect), "{dialect}");
    }
}

#[test]
fn rollback_counts_derive_complete_and_missing() {
    let mut plan = sample_plan();
    plan.statements = vec![
        statement("A", StatementRiskVm::Additive, false),
        statement("B", StatementRiskVm::Destructive, false),
    ];
    plan.rollback_completeness = RollbackCompletenessVm {
        complete: false,
        missing: vec!["b".to_owned()],
    };
    let counts = rollback_completeness_counts(&plan);
    assert_eq!(counts.complete, 1);
    assert_eq!(counts.missing, 1);
}

#[test]
fn export_plan_sql_renders_header_and_semicolon_statements() {
    let mut plan = sample_plan();
    plan.tables = vec!["users".to_owned(), "orders".to_owned()];
    plan.source_dialect = "PostgreSQL".to_owned();
    plan.target_dialect = "MySQL".to_owned();
    plan.statements = vec![
        statement("ALTER TABLE users ADD COLUMN c int", StatementRiskVm::Additive, false),
        statement("DROP INDEX idx", StatementRiskVm::Destructive, false),
    ];
    let sql = export_plan_sql(&plan);
    assert!(sql.contains("-- Schema Diff Deploy plan"));
    assert!(sql.contains("-- tables: users, orders"));
    assert!(sql.contains("-- PostgreSQL → MySQL"));
    assert!(sql.contains("ALTER TABLE users ADD COLUMN c int;"));
    assert!(sql.contains("DROP INDEX idx;"));
    assert!(sql.ends_with(';'));
    // The header's trailing empty string leaves one blank line before the
    // first statement.
    assert!(sql.contains("-- PostgreSQL → MySQL\n\nALTER TABLE users"));
}

// ---- the payload shapers (wire-form, key-for-key) ------------------------------

#[test]
fn compare_forwards_endpoint_tables_and_schemas() {
    assert_eq!(
        compare_table_schemas_request("src-1", "tgt-1", "source.users", "target.users", Some("source"), Some("target")),
        json!({
            "sourceDbSessionId": "src-1",
            "targetDbSessionId": "tgt-1",
            "sourceTableName": "source.users",
            "targetTableName": "target.users",
            "sourceSchema": "source",
            "targetSchema": "target",
        })
    );
    // The `?? null` collapse: absent schemas ride as null, key present.
    assert_eq!(
        compare_table_schemas_request("src-1", "tgt-1", "t1", "t2", None, None),
        json!({
            "sourceDbSessionId": "src-1",
            "targetDbSessionId": "tgt-1",
            "sourceTableName": "t1",
            "targetTableName": "t2",
            "sourceSchema": null,
            "targetSchema": null,
        })
    );
}

#[test]
fn profile_payloads_and_the_token_merge() {
    assert_eq!(get_profiles_request(), json!({}));
    let profile: SchemaDiffProfileVm = serde_json::from_value(json!({
        "version": 1,
        "id": "profile-1",
        "name": "Production schema",
        "sourceConnectionId": "source",
        "targetConnectionId": "target",
        "sourceDatabase": "app",
        "targetDatabase": "app",
        "sourceSchema": "public",
        "targetSchema": "public",
        "tables": ["public.users"],
        "allowDestructive": false,
        "includeIndexes": true,
        "requireRollback": false,
        "typeOverrides": [],
        "createdAt": "2026-09-21T00:00:00.000Z",
        "updatedAt": "2026-09-21T00:00:00.000Z"
    }))
    .unwrap();
    let save = save_profile_request(&profile);
    assert_eq!(
        save,
        json!({
            "profile": {
                "version": 1,
                "id": "profile-1",
                "name": "Production schema",
                "sourceConnectionId": "source",
                "targetConnectionId": "target",
                "sourceDatabase": "app",
                "targetDatabase": "app",
                "sourceSchema": "public",
                "targetSchema": "public",
                "tables": ["public.users"],
                "allowDestructive": false,
                "includeIndexes": true,
                "requireRollback": false,
                "typeOverrides": [],
                "createdAt": "2026-09-21T00:00:00.000Z",
                "updatedAt": "2026-09-21T00:00:00.000Z"
            }
        })
    );
    // The domain client's token merge: `{ ...request, submissionToken }`.
    let save_with_token = with_submission_token(save, &test_token());
    assert_eq!(save_with_token["submissionToken"]["idempotencyKey"], "test-key");
    assert_eq!(delete_profile_request("profile-1"), json!({ "profileId": "profile-1" }));
}

#[test]
fn prepare_plan_shapes_optional_keys() {
    let mut params = PreparePlanParamsVm {
        source_db_session_id: "src-2".to_owned(),
        target_db_session_id: "tgt-2".to_owned(),
        table_names: vec!["a".to_owned(), "b".to_owned()],
        allow_destructive: true,
        include_indexes: Some(true),
        ..Default::default()
    };
    assert_eq!(
        prepare_plan_request(&params),
        json!({
            "sourceDbSessionId": "src-2",
            "targetDbSessionId": "tgt-2",
            "tableNames": ["a", "b"],
            "allowDestructive": true,
            "includeIndexes": true,
        })
    );
    // An empty target-only list is OMITTED (the conditional spread); the
    // other optionals ride only when present.
    params.target_only_table_names = Some(Vec::new());
    let request = prepare_plan_request(&params);
    assert!(request.get("targetOnlyTableNames").is_none());
    params.target_only_table_names = Some(vec!["archive".to_owned()]);
    assert_eq!(
        prepare_plan_request(&params)["targetOnlyTableNames"],
        json!(["archive"])
    );
    // typeOverrides ride as the camelCase wire shape.
    params.type_overrides = Some(vec![ColumnTypeOverrideVm {
        table: "orders".to_owned(),
        column: "amount".to_owned(),
        target_type: "DECIMAL(12,2)".to_owned(),
    }]);
    assert_eq!(
        prepare_plan_request(&params)["typeOverrides"],
        json!([{ "table": "orders", "column": "amount", "targetType": "DECIMAL(12,2)" }])
    );
}

#[test]
fn prepare_unified_sends_exact_identities() {
    let params = PrepareUnifiedParamsVm {
        source_db_session_id: "source-session".to_owned(),
        target_db_session_id: "target-session".to_owned(),
        table_names: vec!["public.orders".to_owned()],
        target_table_names: Some(vec!["app.orders".to_owned()]),
        target_only_table_names: None,
        source_schema: Some("public".to_owned()),
        target_schema: Some("app".to_owned()),
        allow_destructive: false,
        include_indexes: Some(true),
        type_overrides: Some(vec![ColumnTypeOverrideVm {
            table: "orders".to_owned(),
            column: "amount".to_owned(),
            target_type: "DECIMAL(12,2)".to_owned(),
        }]),
        source_objects: vec![
            SchemaDiffObjectIdentityVm {
                kind: SchemaDiffObjectKindVm::Function,
                schema: Some("public".to_owned()),
                name: "calculate_total".to_owned(),
                signature: Some("integer, numeric".to_owned()),
                target_schema: None,
                target_name: None,
            },
            SchemaDiffObjectIdentityVm {
                kind: SchemaDiffObjectKindVm::Trigger,
                schema: Some("public".to_owned()),
                name: "audit_row".to_owned(),
                signature: None,
                target_schema: Some("public".to_owned()),
                target_name: Some("orders".to_owned()),
            },
        ],
        target_objects: vec![SchemaDiffObjectIdentityVm {
            kind: SchemaDiffObjectKindVm::Function,
            schema: Some("app".to_owned()),
            name: "calculate_total".to_owned(),
            signature: Some("integer, numeric".to_owned()),
            target_schema: None,
            target_name: None,
        }],
    };
    let request = prepare_unified_plan_request(&params);
    assert_eq!(request["sourceObjects"], json!([
        {
            "kind": "function",
            "schema": "public",
            "name": "calculate_total",
            "signature": "integer, numeric",
            "targetSchema": null,
            "targetName": null
        },
        {
            "kind": "trigger",
            "schema": "public",
            "name": "audit_row",
            "signature": null,
            "targetSchema": "public",
            "targetName": "orders"
        }
    ]));
    assert_eq!(request["targetObjects"], json!([
        {
            "kind": "function",
            "schema": "app",
            "name": "calculate_total",
            "signature": "integer, numeric",
            "targetSchema": null,
            "targetName": null
        }
    ]));
    // Unlike the table plan, an EMPTY target-only list rides as [] (the
    // unified branch passes it through unconditionally).
    let mut empty = params.clone();
    empty.target_only_table_names = Some(Vec::new());
    assert_eq!(
        prepare_unified_plan_request(&empty)["targetOnlyTableNames"],
        json!([])
    );
}

#[test]
fn deploy_request_keeps_conditional_keys_apart() {
    let mut params = ExecuteDeployParamsVm {
        target_db_session_id: "tgt-3".to_owned(),
        use_transaction: Some(true),
        confirm_destructive: Some(DESTRUCTIVE_CONFIRM_TOKEN.to_owned()),
        ..Default::default()
    };
    params.plan = Some(sample_plan());
    let request = execute_deploy_request(&params);
    assert_eq!(request["targetDbSessionId"], "tgt-3");
    assert_eq!(request["useTransaction"], true);
    assert_eq!(request["confirmDestructive"], "DEPLOY");
    // `undefined` optionals are absent on the wire (JSON.stringify drops).
    for key in ["jobId", "targetDatabase", "targetSchema", "profile", "planId", "selectionRevision"] {
        assert!(request.get(key).is_none(), "{key} must be absent");
    }
    // The plan body rides denormalized (tagged requirements, even empty).
    assert_eq!(request["plan"]["requirements"], json!([]));

    // The window's explicit-null convention: `endpoints.targetDatabase || null`
    // rides as null, key PRESENT; absent stays absent.
    params.target_database = Some(None);
    params.target_schema = Some(Some("reporting".to_owned()));
    let request = execute_deploy_request(&params);
    assert_eq!(request["targetDatabase"], serde_json::Value::Null);
    assert_eq!(request["targetSchema"], "reporting");

    // selectionRevision 0 is non-null in React (`!= null`) and rides.
    params.selection_revision = Some(0);
    assert_eq!(execute_deploy_request(&params)["selectionRevision"], 0);
    params.profile = Some(crate::commands::schema_diff::DeployProfileRefVm {
        id: "p1".to_owned(),
        revision: "r1".to_owned(),
    });
    assert_eq!(
        execute_deploy_request(&params)["profile"],
        json!({ "id": "p1", "revision": "r1" })
    );
}

#[test]
fn single_shot_payloads() {
    assert_eq!(list_jobs_request(), json!({ "states": null, "limit": 100 }));
    assert_eq!(get_job_details_request("job-42"), json!({ "jobId": "job-42" }));
    assert_eq!(cancel_deploy_request("job-42"), json!({ "jobId": "job-42" }));
    assert_eq!(
        verify_recovery_request("job-42", "tgt-9"),
        json!({ "jobId": "job-42", "targetDbSessionId": "tgt-9" })
    );
}

// ---- the tagged-enum folding ---------------------------------------------------

/// The React test's `mockAcceptedPrepare` details payload (tagged
/// requirements on the wire).
fn details_json(state: &str, prepared: Option<serde_json::Value>) -> serde_json::Value {
    json!({
        "details": {
            "job": {
                "jobId": "job-prepare-1",
                "kind": "schemaDiffPrepare",
                "state": state,
                "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 },
                "error": null,
                "artifactIds": []
            },
            "stateVersion": 2,
            "planId": "plan-1",
            "commitBoundaries": [],
            "domainResults": [],
            "recoveryTargets": []
        },
        "prepared": prepared,
        "planUnavailableAfterRestart": false,
        "deployResult": null
    })
}

fn envelope_json(plan: serde_json::Value) -> serde_json::Value {
    json!({
        "plan": plan,
        "planId": "plan-1",
        "selectionRevision": 1,
        "planVersion": 1,
        "handlerVersion": 1,
        "checkpointVersion": 1,
        "expiresAt": "2999-01-01T00:00:00Z",
        "recoveryPolicy": "readOnlyVerify"
    })
}

#[test]
fn job_details_folds_tagged_requirements() {
    let plan = json!({
        "table": "users",
        "tables": ["users"],
        "sourceDialect": "postgres",
        "targetDialect": "mysql",
        "sameDialect": false,
        "statements": [],
        "warnings": [],
        "requirements": [
            { "backfill": { "table": "users", "column": "status", "reason": "Populate existing rows before enforcing NOT NULL." } },
            { "unsupported": { "operation": "users.meta", "reason": "Operation is not supported" } }
        ],
        "rollbackCompleteness": { "complete": true, "missing": [] }
    });
    let details = normalize_job_details(details_json("succeeded", Some(envelope_json(plan)))).unwrap();
    assert_eq!(details.details.state, JobState::Succeeded);
    assert_eq!(details.state_version, 2);
    let envelope = details.prepared.expect("envelope");
    assert_eq!(envelope.plan_id, "plan-1");
    assert_eq!(envelope.selection_revision, 1);
    assert_eq!(envelope.recovery_policy, "readOnlyVerify");
    assert_eq!(
        envelope.plan.requirements,
        vec![
            PlanRequirementVm {
                kind: PlanRequirementKindVm::Backfill,
                table: "users".to_owned(),
                column: "status".to_owned(),
                reason: "Populate existing rows before enforcing NOT NULL.".to_owned(),
            },
            PlanRequirementVm {
                kind: PlanRequirementKindVm::Unsupported,
                table: "users".to_owned(),
                column: "meta".to_owned(),
                reason: "Operation is not supported".to_owned(),
            },
        ]
    );
}

#[test]
fn requirement_split_and_rejoin_round_trip() {
    // The React round-trip test: backfill, bare-table unsupported and
    // dotted unsupported survive normalize + denormalize without losing
    // table/column identity.
    let plan = json!({
        "table": "users",
        "tables": ["users"],
        "sourceDialect": "postgres",
        "targetDialect": "mysql",
        "sameDialect": false,
        "statements": [],
        "warnings": [],
        "requirements": [
            { "backfill": { "table": "users", "column": "status", "reason": "populate first" } },
            { "unsupported": { "operation": "users", "reason": "table unavailable" } },
            { "unsupported": { "operation": "users.id", "reason": "column unavailable" } }
        ],
        "rollbackCompleteness": { "complete": true, "missing": [] }
    });
    let details = normalize_job_details(details_json("succeeded", Some(envelope_json(plan)))).unwrap();
    let prepared = details.prepared.expect("envelope");
    assert_eq!(prepared.plan.requirements[1].column, "");
    assert_eq!(prepared.plan.requirements[2].column, "id");
    // The deploy request unfolds them back into the tagged wire shapes.
    let request = execute_deploy_request(&ExecuteDeployParamsVm {
        target_db_session_id: "tgt".to_owned(),
        plan: Some(prepared.plan.clone()),
        ..Default::default()
    });
    assert_eq!(request["plan"]["requirements"], json!([
        { "backfill": { "table": "users", "column": "status", "reason": "populate first" } },
        { "unsupported": { "operation": "users", "reason": "table unavailable" } },
        { "unsupported": { "operation": "users.id", "reason": "column unavailable" } }
    ]));
}

#[test]
fn deploy_result_and_progress_decode() {
    let mut payload = details_json("succeeded", None);
    payload["details"]["job"]["state"] = json!("failed");
    payload["details"]["job"]["error"] = json!("boom");
    payload["details"]["job"]["progress"]["read"] = json!("3");
    payload["details"]["job"]["progress"]["committed"] = json!(2);
    payload["deployResult"] = json!({
        "status": "mixed",
        "executedCount": 1,
        "statementCount": 2,
        "errors": ["e1"],
        "statementResults": [{ "index": 0, "sql": "S", "ok": false, "error": "e1" }]
    });
    let details = normalize_job_details(payload).unwrap();
    assert_eq!(details.details.state, JobState::Failed);
    assert_eq!(details.details.error.as_deref(), Some("boom"));
    // Counters keep their wire forms (string vs number) until readJobCounter.
    assert_eq!(
        details.details.progress.read,
        Some(crate::views::data_transfer::data_transfer_attached_jobs::JobCounter::Text(
            "3".to_owned()
        ))
    );
    let result = details.deploy_result.expect("deploy result");
    assert_eq!(result.status, DeployStatusVm::Mixed);
    assert_eq!(result.executed_count, 1);
    assert_eq!(result.statement_count, 2);
    assert_eq!(result.statement_results[0].sql, "S");
    assert!(!result.statement_results[0].ok);
}

#[test]
fn unknown_wire_enum_spellings_are_errors() {
    let payload = details_json("exploded", None);
    assert!(normalize_job_details(payload).is_err());
}

// ---- the submit→poll→resolve machine -------------------------------------------

#[test]
fn prepare_run_journey_publishes_and_polls_to_terminal() {
    let plan = json!({
        "table": "users", "tables": ["users"],
        "sourceDialect": "postgres", "targetDialect": "mysql", "sameDialect": false,
        "statements": [], "warnings": [],
        "requirements": [{ "backfill": { "table": "users", "column": "status", "reason": "r" } }],
        "rollbackCompleteness": { "complete": true, "missing": [] }
    });
    let mut run = SchemaDiffJobRun::begin(
        SchemaDiffRunKind::Prepare,
        SchemaDiffMethod::PreparePlan,
        prepare_plan_request(&PreparePlanParamsVm {
            source_db_session_id: "src".to_owned(),
            target_db_session_id: "tgt".to_owned(),
            table_names: vec!["users".to_owned()],
            allow_destructive: false,
            ..Default::default()
        }),
    );
    // 1) token first
    let effects = run.on_token(&test_token());
    assert_eq!(effects.len(), 1);
    match &effects[0] {
        SchemaDiffEffect::Call { method, request } => {
            assert_eq!(*method, SchemaDiffMethod::PreparePlan);
            assert_eq!(request["sourceDbSessionId"], "src");
            assert_eq!(request["tableNames"], json!(["users"]));
            assert_eq!(request["submissionToken"]["idempotencyKey"], "test-key");
        }
        other => panic!("expected Call, got {other:?}"),
    }
    assert!(!run.is_done());

    // 2) accepted: publish + the FIRST getJobDetails fires immediately
    let effects = run.on_accepted_json(json!({
        "jobId": "job-prepare-1", "kind": "schemaDiffPrepare",
        "state": "queued", "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    }));
    assert!(matches!(effects[0], SchemaDiffEffect::Publish(SchemaDiffJobUpdateVm::Accepted(_))));
    match &effects[1] {
        SchemaDiffEffect::Call { method, request } => {
            assert_eq!(*method, SchemaDiffMethod::GetJobDetails);
            assert_eq!(request["jobId"], "job-prepare-1");
        }
        other => panic!("expected Call, got {other:?}"),
    }

    // 3) a running receipt publishes + re-arms the 400ms wait
    let effects = run.on_details_json(1_000, details_json("running", None));
    assert!(matches!(effects[0], SchemaDiffEffect::Publish(SchemaDiffJobUpdateVm::Details(_))));
    match effects.get(1) {
        Some(SchemaDiffEffect::Wait { until_ms }) => assert_eq!(*until_ms, 1_400),
        other => panic!("expected Wait 400ms, got {other:?}"),
    }
    assert_eq!(run.next_wake_ms(), Some(1_400));
    // ticks before the deadline do nothing; at the deadline the poll fires.
    assert!(run.tick(1_399).is_empty());
    let effects = run.tick(1_400);
    assert_eq!(effects.len(), 1);
    assert!(matches!(
        &effects[0],
        SchemaDiffEffect::Call { method: SchemaDiffMethod::GetJobDetails, .. }
    ));

    // 4) terminal: publish, resolve to the prepared envelope
    let effects = run.on_details_json(1_800, details_json("succeeded", Some(envelope_json(plan))));
    assert!(matches!(effects[0], SchemaDiffEffect::Publish(SchemaDiffJobUpdateVm::Details(_))));
    assert!(run.is_done());
    match run.result().expect("settled") {
        Ok(SchemaDiffJobRunOutcome::Prepared(envelope)) => {
            assert_eq!(envelope.plan_id, "plan-1");
            assert_eq!(envelope.plan.requirements.len(), 1);
            assert_eq!(envelope.plan.requirements[0].kind, PlanRequirementKindVm::Backfill);
        }
        other => panic!("expected Prepared, got {other:?}"),
    }
}

#[test]
fn prepare_run_error_wording_is_react_verbatim() {
    // failed with an error reason
    let mut run = SchemaDiffJobRun::begin(
        SchemaDiffRunKind::Prepare,
        SchemaDiffMethod::PreparePlan,
        json!({}),
    );
    let _ = run.on_token(&test_token());
    let _ = run.on_accepted_json(json!({
        "jobId": "j1", "kind": "schemaDiffPrepare", "state": "queued",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    }));
    let mut payload = details_json("failed", None);
    payload["details"]["job"]["error"] = json!("boom");
    let _ = run.on_details_json(0, payload);
    assert_eq!(
        run.result().expect("settled").as_ref().expect_err("error"),
        "Schema Diff plan job ended in failed (boom)"
    );

    // failed WITHOUT an error: the planUnavailable fallback reason
    let mut run = SchemaDiffJobRun::begin(
        SchemaDiffRunKind::Prepare,
        SchemaDiffMethod::PreparePlan,
        json!({}),
    );
    let _ = run.on_token(&test_token());
    let _ = run.on_accepted_json(json!({
        "jobId": "j2", "kind": "schemaDiffPrepare", "state": "queued",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    }));
    let _ = run.on_details_json(0, details_json("cancelled", None));
    assert_eq!(
        run.result().expect("settled").as_ref().expect_err("error"),
        "Schema Diff plan job ended in cancelled (planUnavailable)"
    );

    // the restart-recovery branch
    let mut payload = details_json("failed", None);
    payload["planUnavailableAfterRestart"] = json!(true);
    let mut run = SchemaDiffJobRun::begin(
        SchemaDiffRunKind::Prepare,
        SchemaDiffMethod::PreparePlan,
        json!({}),
    );
    let _ = run.on_token(&test_token());
    let _ = run.on_accepted_json(json!({
        "jobId": "j3", "kind": "schemaDiffPrepare", "state": "queued",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    }));
    let _ = run.on_details_json(0, payload);
    assert_eq!(
        run.result().expect("settled").as_ref().expect_err("error"),
        "PlanUnavailableAfterRestart: compare again before applying"
    );
}

#[test]
fn apply_run_resolves_the_deploy_receipt_or_errors() {
    let mut run = SchemaDiffJobRun::begin(
        SchemaDiffRunKind::Apply,
        SchemaDiffMethod::ExecuteDeploy,
        execute_deploy_request(&ExecuteDeployParamsVm {
            target_db_session_id: "tgt".to_owned(),
            plan: Some(sample_plan()),
            use_transaction: Some(true),
            ..Default::default()
        }),
    );
    let _ = run.on_token(&test_token());
    let _ = run.on_accepted_json(json!({
        "jobId": "job-apply-1", "kind": "schemaDiffApply", "state": "queued",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    }));
    let mut payload = details_json("succeeded", None);
    payload["details"]["job"]["kind"] = json!("schemaDiffApply");
    payload["deployResult"] = json!({
        "status": "committed", "executedCount": 1, "statementCount": 1,
        "errors": [], "statementResults": []
    });
    let _ = run.on_details_json(0, payload);
    match run.result().expect("settled") {
        Ok(SchemaDiffJobRunOutcome::Deployed(result)) => {
            assert_eq!(result.status, DeployStatusVm::Committed);
            assert_eq!(result.executed_count, 1);
        }
        other => panic!("expected Deployed, got {other:?}"),
    }

    // no receipt: the apply wording
    let mut run = SchemaDiffJobRun::begin(
        SchemaDiffRunKind::Apply,
        SchemaDiffMethod::ExecuteDeploy,
        json!({}),
    );
    let _ = run.on_token(&test_token());
    let _ = run.on_accepted_json(json!({
        "jobId": "j4", "kind": "schemaDiffApply", "state": "queued",
        "progress": { "read": 0, "converted": 0, "attempted": 0, "committed": 0, "unknown": 0 }
    }));
    let mut payload = details_json("cancelled", None);
    payload["details"]["job"]["kind"] = json!("schemaDiffApply");
    let _ = run.on_details_json(0, payload);
    assert_eq!(
        run.result().expect("settled").as_ref().expect_err("error"),
        "Schema Diff apply job ended in cancelled"
    );
}

#[test]
fn transport_failures_reject_the_run() {
    let mut run = SchemaDiffJobRun::begin(
        SchemaDiffRunKind::Apply,
        SchemaDiffMethod::ExecuteDeploy,
        json!({}),
    );
    run.on_call_error("job not found".to_owned());
    assert_eq!(
        run.result().expect("settled").as_ref().expect_err("error"),
        "job not found"
    );
}

// ---- the observer bus -----------------------------------------------------------

#[test]
fn bus_publishes_in_subscribe_order_and_unsubscribes() {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut bus = SchemaDiffJobBus::default();
    assert!(bus.is_empty());
    let a = {
        let seen = std::rc::Rc::clone(&seen);
        bus.subscribe(std::rc::Rc::new(move |update| {
            seen.borrow_mut().push(format!("a:{}", update_kind(update)));
        }))
    };
    let b = {
        let seen = std::rc::Rc::clone(&seen);
        bus.subscribe(std::rc::Rc::new(move |update| {
            seen.borrow_mut().push(format!("b:{}", update_kind(update)));
        }))
    };
    assert_eq!(bus.len(), 2);
    let plan = json!({
        "table": "t", "tables": ["t"], "sourceDialect": "a", "targetDialect": "b",
        "sameDialect": false, "statements": [], "warnings": [],
        "rollbackCompleteness": { "complete": true, "missing": [] }
    });
    let details = normalize_job_details(details_json("running", Some(envelope_json(plan)))).unwrap();
    bus.publish(&SchemaDiffJobUpdateVm::Details(details));
    assert_eq!(*seen.borrow(), ["a:details", "b:details"]);
    assert!(bus.unsubscribe(b));
    assert!(!bus.unsubscribe(b), "second delete is false, like Set.delete");
    bus.publish(&SchemaDiffJobUpdateVm::Details(
        normalize_job_details(details_json("running", None)).unwrap(),
    ));
    assert_eq!(*seen.borrow(), ["a:details", "b:details", "a:details"]);
    assert_eq!(bus.len(), 1);
    assert!(bus.unsubscribe(a));
    assert!(bus.is_empty());
}

fn update_kind(update: &SchemaDiffJobUpdateVm) -> &'static str {
    match update {
        SchemaDiffJobUpdateVm::Accepted(_) => "accepted",
        SchemaDiffJobUpdateVm::Details(_) => "details",
    }
}

// ---- the clipboard config format -------------------------------------------------

#[test]
fn config_json_v2_parses_with_optional_sections_absent() {
    let config: SchemaDiffConfigJson = serde_json::from_value(json!({
        "version": 2,
        "sourceConnectionId": "source",
        "targetConnectionId": "target",
        "tables": ["public.users"],
        "allowDestructive": false
    }))
    .unwrap();
    assert_eq!(config.version, 2);
    assert_eq!(config.tables, vec!["public.users".to_owned()]);
    assert!(config.target_only_tables.is_none());
    assert!(config.source_objects.is_none());
    assert!(config.include_indexes.is_none());
    // The optional sections ride when present.
    let config: SchemaDiffConfigJson = serde_json::from_value(json!({
        "version": 2,
        "sourceConnectionId": "source",
        "targetConnectionId": "target",
        "tables": [],
        "allowDestructive": true,
        "includeIndexes": true,
        "requireRollback": true,
        "sourceObjects": [{ "kind": "view", "schema": "public", "name": "v1" }],
        "targetOnlyTables": ["archive"]
    }))
    .unwrap();
    assert_eq!(config.include_indexes, Some(true));
    assert_eq!(config.require_rollback, Some(true));
    assert_eq!(config.target_only_tables.as_deref(), Some(["archive".to_owned()].as_slice()));
    let objects = config.source_objects.expect("objects");
    assert_eq!(objects[0].name, "v1");
    assert_eq!(objects[0].kind, SchemaDiffObjectKindVm::View);
}
