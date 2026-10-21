use super::*;
use crate::model::{
    Endpoint, TransferMode, TransferOptions, TransferRunRequest, WriteMode,
};
use crate::resume::{ResumeTableProgress, TransferResumeCheckpoint};

fn job() -> TransferJob {
    TransferJob {
        source: Endpoint {
            db_session_id: "src".into(),
            database: "source_db".into(),
            schema: Some("public".into()),
        },
        target: Some(Endpoint {
            db_session_id: "tgt".into(),
            database: "target_db".into(),
            schema: Some("public".into()),
        }),
        sql_file_target: None,
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        tables: vec![],
        options: TransferOptions::default(),
    }
}

#[test]
fn schema_fingerprint_is_order_independent_and_changes_with_schema() {
    let first = fingerprint_schemas(vec![("a".into(), None), ("b".into(), None)]).unwrap();
    let second = fingerprint_schemas(vec![("b".into(), None), ("a".into(), None)]).unwrap();
    assert_eq!(first, second);
    let changed = fingerprint_schemas(vec![
        ("a".into(), None),
        (
            "b".into(),
            Some(TableSchema {
                table_name: "b".into(),
                columns: vec![],
                primary_keys: vec![],
                indexes: vec![],
                foreign_keys: vec![],
                check_constraints: vec![],
                table_options: Default::default(),
            }),
        ),
    ])
    .unwrap();
    assert_ne!(first, changed);
}

#[test]
fn disabled_mappings_are_excluded_from_the_plan_fingerprint_scope() {
    let mut plan_job = job();
    plan_job
        .tables
        .push(crate::model::TableMapping::auto("users"));
    plan_job.tables.push({
        let mut mapping = crate::model::TableMapping::auto("archived");
        mapping.enabled = false;
        mapping
    });

    let users_schema = TableSchema {
        table_name: "users".into(),
        columns: vec![],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    let mut schemas: HashMap<String, TableSchema> = HashMap::new();
    schemas.insert("users".into(), users_schema.clone());
    schemas.insert(
        "archived".into(),
        TableSchema {
            table_name: "archived".into(),
            ..users_schema.clone()
        },
    );

    let scoped = fingerprint_schemas(participating_tables(&plan_job).map(|table| {
        (
            table.source_table.clone(),
            schemas.get(&table.source_table).cloned(),
        )
    }))
    .unwrap();
    let enabled_only = fingerprint_schemas(vec![("users".into(), Some(users_schema))]).unwrap();
    assert_eq!(scoped, enabled_only);

    // A schema change on a disabled relation cannot alter the immutable
    // snapshot, because that relation is outside the execution scope.
    schemas.insert(
        "archived".into(),
        TableSchema {
            table_name: "archived-v2".into(),
            ..TableSchema {
                table_name: "archived".into(),
                columns: vec![],
                primary_keys: vec![],
                indexes: vec![],
                foreign_keys: vec![],
                check_constraints: vec![],
                table_options: Default::default(),
            }
        },
    );
    let after_disabled_change = fingerprint_schemas(participating_tables(&plan_job).map(|table| {
        (
            table.source_table.clone(),
            schemas.get(&table.source_table).cloned(),
        )
    }))
    .unwrap();
    assert_eq!(scoped, after_disabled_change);
}

#[test]
fn expired_plan_is_rejected_and_consumed_plan_is_one_shot() {
    let store = TransferPlanStore::new();
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let source = HashMap::new();
    let target = HashMap::new();
    let id = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &source,
            &target,
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    let claimed = store.claim(&id).unwrap();
    assert_eq!(claimed.id, id);
    assert!(store.claim(&id).is_err());

    let expired = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &source,
            &target,
            false,
            Duration::ZERO,
        )
        .unwrap();
    assert!(store.peek(&expired).is_err());
}

#[test]
fn run_request_rejects_client_replacement_payloads() {
    let request = serde_json::from_value::<TransferRunRequest>(serde_json::json!({
        "planId": "opaque-plan",
        "job": { "source": {}, "target": {}, "tables": [] },
    }));
    assert!(request.is_err());
}

#[test]
fn source_scope_fingerprint_changes_when_recordset_changes() {
    let mut first = job();
    first.tables.push({
        let mut mapping = crate::model::TableMapping::auto("users");
        mapping.recordset = Some(crate::model::TransferRecordset {
            order_by: Some("id".into()),
            start: None,
            end: None,
            tuple_range: None,
            limit: Some(10),
        });
        mapping
    });
    let first_fingerprint = filter_fingerprint(&first).unwrap();
    first.tables[0].recordset.as_mut().unwrap().limit = Some(20);
    assert_ne!(first_fingerprint, filter_fingerprint(&first).unwrap());
    first.tables[0].recordset.as_mut().unwrap().limit = Some(10);
    first.tables[0].recordset.as_mut().unwrap().start =
        Some(crate::model::TransferRecordsetBound {
            value: serde_json::json!(2),
            inclusive: true,
        });
    assert_ne!(first_fingerprint, filter_fingerprint(&first).unwrap());

    let recordset = first.tables[0].recordset.as_mut().unwrap();
    recordset.order_by = None;
    recordset.start = None;
    recordset.tuple_range = Some(crate::model::TransferRecordsetTupleRange {
        columns: vec!["tenant".into(), "sequence".into()],
        start: Some(crate::model::TransferRecordsetTupleBound {
            values: vec![serde_json::json!("a-雪"), serde_json::json!("-3")],
            inclusive: false,
        }),
        end: None,
    });
    let tuple_fingerprint = filter_fingerprint(&first).unwrap();
    first.tables[0]
        .recordset
        .as_mut()
        .unwrap()
        .tuple_range
        .as_mut()
        .unwrap()
        .start
        .as_mut()
        .unwrap()
        .values[1] = serde_json::json!("-2");
    assert_ne!(tuple_fingerprint, filter_fingerprint(&first).unwrap());
    first.tables[0]
        .recordset
        .as_mut()
        .unwrap()
        .tuple_range
        .as_mut()
        .unwrap()
        .start
        .as_mut()
        .unwrap()
        .values[1] = serde_json::json!("-3");
    first.tables[0]
        .recordset
        .as_mut()
        .unwrap()
        .tuple_range
        .as_mut()
        .unwrap()
        .columns = vec!["sequence".into(), "tenant".into()];
    assert_ne!(tuple_fingerprint, filter_fingerprint(&first).unwrap());
}

#[test]
fn sql_file_target_dialect_is_bound_to_the_immutable_plan() {
    let store = TransferPlanStore::new();
    let source = datazen_driver_api::mock_driver::MockDriver::new("postgresql", Default::default());
    let target = datazen_driver_api::mock_driver::MockDriver::new("mysql", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "sqlFile".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![DdlPreviewItem {
            source_table: "users".into(),
            target_table: "users".into(),
            ddl: "CREATE TABLE `users` (`id` INT)".into(),
            kind: crate::Table,
            depends_on: vec![],
        }],
        write_plans: vec![],
        warnings: vec!["mysql".into()],
        can_execute: true,
        block_reason: None,
    };
    let mut sql_job = job();
    sql_job.target = None;
    sql_job.sql_file_target = Some(crate {
        file_token: "opaque-file".into(),
        database_type: Some("mysql".into()),
        database: None,
        schema: None,
        encoding: None,
        compression: None,
    });
    let id = store
        .issue_with_ttl(
            sql_job.clone(),
            &preview,
            source.as_ref(),
            target.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    sql_job.sql_file_target.as_mut().unwrap().database_type = Some("postgresql".into());
    let stored = store.peek(&id).unwrap();
    assert_eq!(stored.target_driver_type, "mysql");
    assert_eq!(
        stored
            .job
            .sql_file_target
            .as_ref()
            .and_then(|target| target.database_type.as_deref()),
        Some("mysql")
    );
    assert_eq!(
        stored
            .sql_file_structure
            .as_ref()
            .and_then(|statements| statements.first())
            .map(|statement| statement.ddl.as_str()),
        Some("CREATE TABLE `users` (`id` INT)")
    );
    assert_eq!(
        stored.target_scope_fingerprint.as_deref(),
        target_scope_fingerprint(&stored.job).unwrap().as_deref()
    );
    let mut changed_scope = stored.job.clone();
    changed_scope.sql_file_target.as_mut().unwrap().database = Some("other_catalog".into());
    assert_ne!(
        stored.target_scope_fingerprint,
        target_scope_fingerprint(&changed_scope).unwrap()
    );
    let mut changed_format = stored.job.clone();
    changed_format.sql_file_target.as_mut().unwrap().compression =
        Some(crate::Gzip);
    assert_ne!(
        stored.target_scope_fingerprint,
        target_scope_fingerprint(&changed_format).unwrap()
    );
}

#[test]
fn test_tester_run_request_rejects_all_client_owned_execution_payloads() {
    for field in ["sql", "ddl", "mapping", "rows"] {
        let mut payload = serde_json::Map::new();
        payload.insert("planId".into(), serde_json::json!("opaque-plan"));
        payload.insert(field.into(), serde_json::json!([]));
        let request =
            serde_json::from_value::<TransferRunRequest>(serde_json::Value::Object(payload));
        assert!(request.is_err(), "client field {field} must be rejected");
    }
}

#[test]
fn resume_checkpoint_is_opaque_single_flight_and_consumed_on_success() {
    let store = TransferPlanStore::new();
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let id = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    store.claim(&id).unwrap();
    let token = store
        .create_checkpoint(&id, vec!["users".into()], Vec::new())
        .unwrap();
    assert_ne!(token, id);
    let (_, checkpoint) = store.peek_checkpoint(&token, &id).unwrap();
    assert_eq!(checkpoint.selected_tables, vec!["users"]);
    assert!(store.claim_checkpoint(&token, &id).is_ok());
    assert!(store.peek_checkpoint(&token, &id).is_err());
    store
        .update_checkpoint(&token, vec!["users".into()], false)
        .unwrap();
    let (_, checkpoint) = store.peek_checkpoint(&token, &id).unwrap();
    assert_eq!(checkpoint.completed_tables, vec!["users"]);
    store.claim_checkpoint(&token, &id).unwrap();
    store.update_checkpoint(&token, Vec::new(), true).unwrap();
    assert!(store.peek_checkpoint(&token, &id).is_err());
}

#[test]
fn chunk_checkpoint_binds_progress_and_claims_the_token_once() {
    let store = TransferPlanStore::new();
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let plan_id = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    store.claim(&plan_id).unwrap();
    let token = store
        .start_chunk_checkpoint(
            &plan_id,
            vec!["users".into(), "orders".into()],
            Vec::new(),
            ResumeTableProgress {
                source_table: "users".into(),
                target_table: "customer".into(),
                key_columns: vec!["tenant_id".into(), "id".into()],
                chunk_size: 250,
                source_fingerprint: "source-digest-users".into(),
                cursor: None,
                rows_seen: 0,
            },
        )
        .unwrap();
    assert!(store.plans.lock().unwrap()[&plan_id]
        .active_until
        .is_some_and(|until| until > Instant::now()));
    store
        .advance_checkpoint_table(
            &token,
            "users",
            vec![datazen_driver_api::Value::Integer(8), datazen_driver_api::Value::Integer(21)],
            250,
        )
        .unwrap();
    store
        .prepare_checkpoint_table(
            &token,
            ResumeTableProgress {
                source_table: "orders".into(),
                target_table: "orders_archive".into(),
                key_columns: vec!["order_id".into()],
                chunk_size: 250,
                source_fingerprint: "source-digest-orders".into(),
                cursor: None,
                rows_seen: 0,
            },
        )
        .unwrap();
    store
        .advance_checkpoint_table(&token, "orders", vec![datazen_driver_api::Value::Integer(44)], 3)
        .unwrap();
    store
        .update_checkpoint(&token, vec!["users".into()], false)
        .unwrap();

    let (_, checkpoint) = store.peek_checkpoint(&token, &plan_id).unwrap();
    assert_eq!(checkpoint.resume_tables.len(), 2);
    assert_eq!(checkpoint.resume_tables["users"].target_table, "customer");
    assert_eq!(
        checkpoint.resume_tables["users"].key_columns,
        vec!["tenant_id".to_string(), "id".to_string()]
    );
    assert_eq!(checkpoint.resume_tables["users"].chunk_size, 250);
    assert_eq!(checkpoint.resume_tables["users"].rows_seen, 250);
    assert!(matches!(
        checkpoint.resume_tables["users"]
            .cursor
            .as_ref()
            .and_then(|v| v.first()),
        Some(datazen_driver_api::Value::Integer(8))
    ));
    assert_eq!(checkpoint.resume_tables["orders"].rows_seen, 3);

    assert!(store.claim_checkpoint(&token, &plan_id).is_ok());
    assert!(store.claim_checkpoint(&token, &plan_id).is_err());
}

#[test]
fn test_tester_concurrent_checkpoint_claim_allows_exactly_one_and_prevents_replay() {
    let store = TransferPlanStore::new();
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let plan_id = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    store.claim(&plan_id).unwrap();
    let token = store
        .start_chunk_checkpoint(
            &plan_id,
            vec!["users".into()],
            Vec::new(),
            ResumeTableProgress {
                source_table: "users".into(),
                target_table: "users_copy".into(),
                key_columns: vec!["id".into()],
                chunk_size: 2,
                source_fingerprint: "source-digest-users".into(),
                cursor: None,
                rows_seen: 0,
            },
        )
        .unwrap();
    // Model the interruption path: execution releases a resumable checkpoint
    // back to Available before a later run can claim it.
    store.update_checkpoint(&token, Vec::new(), false).unwrap();

    let store = std::sync::Arc::new(store);
    let start = std::sync::Arc::new(std::sync::Barrier::new(3));
    let first = {
        let store = std::sync::Arc::clone(&store);
        let start = std::sync::Arc::clone(&start);
        let token = token.clone();
        let plan_id = plan_id.clone();
        std::thread::spawn(move || {
            start.wait();
            store.claim_checkpoint(&token, &plan_id).is_ok()
        })
    };
    let second = {
        let store = std::sync::Arc::clone(&store);
        let start = std::sync::Arc::clone(&start);
        let token = token.clone();
        let plan_id = plan_id.clone();
        std::thread::spawn(move || {
            start.wait();
            store.claim_checkpoint(&token, &plan_id).is_ok()
        })
    };

    start.wait();
    let first_claimed = first.join().unwrap();
    let second_claimed = second.join().unwrap();
    assert_ne!(
        first_claimed, second_claimed,
        "exactly one concurrent claim wins"
    );
    assert!(
        store.claim_checkpoint(&token, &plan_id).is_err(),
        "a claimed checkpoint token cannot be replayed"
    );
}

#[test]
fn changed_source_digest_rejects_resume_and_session_invalidates_token() {
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let plan_id = global_store()
        .issue(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
        )
        .unwrap();
    global_store().claim(&plan_id).unwrap();
    let mut first =
        TransferCheckpointSession::new(plan_id.clone(), vec!["users".into()], Vec::new(), None);
    first
        .prepare_table(
            "users",
            "users_copy",
            &["id".into()],
            100,
            "digest-before-change",
        )
        .unwrap();
    first
        .advance_table("users", vec![datazen_driver_api::Value::Integer(100)], 100)
        .unwrap();
    let token = first.token().unwrap();
    first.finish(Vec::new(), true).unwrap();

    global_store().claim_checkpoint(&token, &plan_id).unwrap();
    let mut resumed = TransferCheckpointSession::new(
        plan_id.clone(),
        vec!["users".into()],
        Vec::new(),
        Some(token.clone()),
    );
    assert!(resumed
        .prepare_table(
            "users",
            "users_copy",
            &["id".into()],
            100,
            "digest-after-source-mutation",
        )
        .is_err());
    assert!(resumed.is_invalidated());
    assert!(global_store().peek_checkpoint(&token, &plan_id).is_err());
    assert_eq!(resumed.finish(Vec::new(), true).unwrap(), None);
}

#[test]
fn checkpoint_ttl_is_independent_from_preview_expiry_and_expires_explicitly() {
    let store = TransferPlanStore::new();
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let plan_id = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    store.claim(&plan_id).unwrap();
    {
        let mut plans = store.plans.lock().unwrap();
        plans.get_mut(&plan_id).unwrap().expires_at = Instant::now() - Duration::from_secs(1);
    }
    let token = store
        .create_checkpoint(&plan_id, vec!["users".into()], Vec::new())
        .unwrap();
    let checkpoint = store.checkpoints.lock().unwrap()[&token].clone();
    assert!(checkpoint.expires_at > Instant::now());
    {
        let mut checkpoints = store.checkpoints.lock().unwrap();
        checkpoints.get_mut(&token).unwrap().expires_at = Instant::now() - Duration::from_secs(1);
    }
    assert!(store.peek_checkpoint(&token, &plan_id).is_err());

    let replacement = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    assert_ne!(replacement, plan_id);
    assert!(!store.checkpoints.lock().unwrap().contains_key(&token));
}

#[test]
fn run_request_accepts_only_opaque_resume_token_field() {
    let request = serde_json::from_value::<TransferRunRequest>(serde_json::json!({
        "planId": "opaque-plan",
        "resumeToken": "opaque-checkpoint",
    }))
    .unwrap();
    assert_eq!(request.resume_token.as_deref(), Some("opaque-checkpoint"));
}

fn tester_claimed_chunk(store: &TransferPlanStore) -> (String, String) {
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let plan_id = store
        .issue_with_ttl(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
            Duration::from_secs(60),
        )
        .unwrap();
    store.claim(&plan_id).unwrap();
    let token = store
        .start_chunk_checkpoint(
            &plan_id,
            vec!["users".into()],
            Vec::new(),
            ResumeTableProgress {
                source_table: "users".into(),
                target_table: "users_copy".into(),
                key_columns: vec!["id".into()],
                chunk_size: 2,
                source_fingerprint: "source-digest-users".into(),
                cursor: None,
                rows_seen: 0,
            },
        )
        .unwrap();
    (plan_id, token)
}

#[test]
fn test_tester_checkpoint_state_matrix_rejects_wrong_owner_regression_and_replay() {
    let store = TransferPlanStore::new();
    let (plan_id, token) = tester_claimed_chunk(&store);
    store.update_checkpoint(&token, Vec::new(), false).unwrap();

    assert!(store
        .peek_checkpoint(&token, "different-plan")
        .expect_err("opaque checkpoint must remain bound to its plan")
        .to_string()
        .contains("does not belong"));
    assert!(store
        .claim_checkpoint(&token, "different-plan")
        .expect_err("a mismatched plan cannot claim the token")
        .to_string()
        .contains("does not belong"));
    assert!(store
        .update_checkpoint(&token, Vec::new(), false)
        .expect_err("only an in-flight checkpoint can be released")
        .to_string()
        .contains("not in flight"));

    store.claim_checkpoint(&token, &plan_id).unwrap();
    assert!(store
        .advance_checkpoint_table(
            &token,
            "users",
            vec![datazen_driver_api::Value::Integer(1), datazen_driver_api::Value::Integer(2)],
            2,
        )
        .expect_err("cursor width cannot diverge from the saved key")
        .to_string()
        .contains("primary-key order"));
    store
        .advance_checkpoint_table(&token, "users", vec![datazen_driver_api::Value::Integer(3)], 3)
        .unwrap();
    assert!(store
        .advance_checkpoint_table(&token, "users", vec![datazen_driver_api::Value::Integer(2)], 2)
        .expect_err("checkpoint progress cannot move backwards")
        .to_string()
        .contains("primary-key order"));
    assert!(store
        .prepare_checkpoint_table(
            &token,
            ResumeTableProgress {
                source_table: "users".into(),
                target_table: "users_copy".into(),
                key_columns: vec!["id".into()],
                chunk_size: 2,
                source_fingerprint: "changed-source-digest".into(),
                cursor: None,
                rows_seen: 0,
            },
        )
        .expect_err("a changed source fingerprint cannot consume prior progress")
        .to_string()
        .contains("changed since the checkpoint"));

    store.update_checkpoint(&token, Vec::new(), false).unwrap();
    store.claim_checkpoint(&token, &plan_id).unwrap();
    store
        .update_checkpoint(&token, vec!["users".into()], true)
        .unwrap();
    assert!(store.peek_checkpoint(&token, &plan_id).is_err());
}

#[test]
fn test_tester_expired_checkpoint_is_removed_when_claimed() {
    let store = TransferPlanStore::new();
    let (plan_id, token) = tester_claimed_chunk(&store);
    store
        .checkpoints
        .lock()
        .expect("checkpoint lock")
        .get_mut(&token)
        .expect("new checkpoint")
        .expires_at = Instant::now() - Duration::from_secs(1);

    assert!(store
        .claim_checkpoint(&token, &plan_id)
        .expect_err("expired opaque tokens are not claimable")
        .to_string()
        .contains("resume token expired"));
    assert!(!store.checkpoints.lock().unwrap().contains_key(&token));
}

#[test]
fn test_tester_checkpoint_store_fails_closed_on_expiry_and_corruption_boundaries() {
    let store = TransferPlanStore::new();
    let (plan_id, token) = tester_claimed_chunk(&store);

    assert!(store
        .create_checkpoint("missing-plan", Vec::new(), Vec::new())
        .expect_err("a checkpoint cannot be created without its immutable plan")
        .to_string()
        .contains("unknown or has expired"));

    {
        let mut plans = store.plans.lock().expect("plan lock");
        plans.get_mut(&plan_id).expect("active plan").active_until =
            Some(Instant::now() + Duration::from_secs(60));
        store
            .checkpoints
            .lock()
            .expect("checkpoint lock")
            .get_mut(&token)
            .expect("checkpoint")
            .expires_at = Instant::now() - Duration::from_secs(1);
        let mut checkpoints = store.checkpoints.lock().expect("checkpoint lock");
        super::retain_live_checkpoints(&mut checkpoints, &plans, Instant::now());
        assert!(checkpoints.contains_key(&token));
    }
    assert!(store
        .peek_checkpoint(&token, &plan_id)
        .expect_err("an expired token is not peekable even while its execution lease is live")
        .to_string()
        .contains("resume token expired"));

    {
        let mut plans = store.plans.lock().expect("plan lock");
        plans.get_mut(&plan_id).expect("active plan").active_until =
            Some(Instant::now() - Duration::from_secs(1));
        let mut checkpoints = store.checkpoints.lock().expect("checkpoint lock");
        super::retain_live_checkpoints(&mut checkpoints, &plans, Instant::now());
        assert!(!checkpoints.contains_key(&token));
        let plan = plans.get_mut(&plan_id).expect("active plan");
        plan.expires_at = Instant::now() - Duration::from_secs(1);
    }
    assert!(store
        .create_checkpoint(&plan_id, Vec::new(), Vec::new())
        .expect_err("an expired execution without a live lease cannot create a token")
        .to_string()
        .contains("transfer plan has expired"));
    store.finish_execution(&plan_id, false);
    assert!(store
        .create_checkpoint(&plan_id, Vec::new(), Vec::new())
        .expect_err("a consumed plan cannot mint another checkpoint")
        .to_string()
        .contains("was not claimed"));
    assert!(store.renew_execution("missing-plan", None).is_err());
    assert!(store.renew_execution(&plan_id, None).is_err());
    assert!(store.claim_checkpoint("missing-token", &plan_id).is_err());
    assert!(store
        .advance_checkpoint_table("missing-token", "users", Vec::new(), 0)
        .is_err());

    let (second_plan_id, second_token) = tester_claimed_chunk(&store);
    {
        let mut checkpoints = store.checkpoints.lock().expect("checkpoint lock");
        checkpoints
            .get_mut(&second_token)
            .expect("checkpoint")
            .token = "tampered-token".into();
    }
    assert!(store
        .peek_checkpoint(&second_token, &second_plan_id)
        .expect_err("the opaque token is bound to its registry key")
        .to_string()
        .contains("registry entry is invalid"));
    assert!(store
        .claim_checkpoint(&second_token, &second_plan_id)
        .expect_err("a tampered token cannot be claimed")
        .to_string()
        .contains("registry entry is invalid"));
    store
        .checkpoints
        .lock()
        .expect("checkpoint lock")
        .get_mut(&second_token)
        .expect("checkpoint")
        .token = second_token.clone();

    assert!(store
        .advance_checkpoint_table(&second_token, "other-table", Vec::new(), 0)
        .expect_err("progress must already exist for the source table")
        .to_string()
        .contains("state disappeared"));
    store
        .update_checkpoint(&second_token, Vec::new(), false)
        .expect("release the simulated interrupted execution");
    let expected = ResumeTableProgress {
        source_table: "users".into(),
        target_table: "users_copy".into(),
        key_columns: vec!["id".into()],
        chunk_size: 2,
        source_fingerprint: "source-digest-users".into(),
        cursor: None,
        rows_seen: 0,
    };
    assert!(store
        .prepare_checkpoint_table(&second_token, expected)
        .expect_err("available checkpoints cannot be prepared as in-flight")
        .to_string()
        .contains("no longer active"));
    {
        let mut plans = store.plans.lock().expect("plan lock");
        let plan = plans.get_mut(&second_plan_id).expect("second plan");
        plan.state = PlanState::Executing;
        plan.expires_at = Instant::now() - Duration::from_secs(1);
        plan.active_until = None;
    }
    assert!(store
        .peek_checkpoint(&second_token, &second_plan_id)
        .expect_err("an expired plan without an active lease cannot be resumed")
        .to_string()
        .contains("transfer plan has expired"));
    assert!(store
        .renew_execution(&second_plan_id, Some(&second_token))
        .expect_err("an available checkpoint is not renewable as an active execution")
        .to_string()
        .contains("no longer in flight"));
}

#[test]
fn test_tester_unfinalized_checkpoint_session_drop_fences_resume_token() {
    let driver = datazen_driver_api::mock_driver::MockDriver::new("fixture", Default::default());
    let preview = TransferPreview {
        plan_id: String::new(),
        pairing_path: "direct".into(),
        mode: TransferMode::Data,
        write_mode: WriteMode::Insert,
        ddl: vec![],
        write_plans: vec![],
        warnings: vec![],
        can_execute: true,
        block_reason: None,
    };
    let plan_id = global_store()
        .issue(
            job(),
            &preview,
            driver.as_ref(),
            driver.as_ref(),
            &HashMap::new(),
            &HashMap::new(),
            false,
        )
        .expect("issue immutable plan");
    global_store().claim(&plan_id).expect("claim plan");
    let mut session =
        TransferCheckpointSession::new(plan_id.clone(), vec!["users".into()], Vec::new(), None);
    session
        .prepare_table(
            "users",
            "users_copy",
            &["id".into()],
            2,
            "source-digest-users",
        )
        .expect("create first chunk checkpoint");
    let token = session.token().expect("opaque checkpoint token");
    drop(session);
    assert!(global_store()
        .peek_checkpoint(&token, &plan_id)
        .expect_err("dropping an unfinished session must fence its token")
        .to_string()
        .contains("unknown, consumed, or expired"));
}

#[test]
fn test_tester_expired_inflight_checkpoint_is_removed_when_released() {
    let store = TransferPlanStore::new();
    let (_plan_id, token) = tester_claimed_chunk(&store);
    store
        .checkpoints
        .lock()
        .expect("checkpoint lock")
        .get_mut(&token)
        .expect("in-flight checkpoint")
        .expires_at = Instant::now() - Duration::from_secs(1);

    assert!(store
        .update_checkpoint(&token, Vec::new(), false)
        .expect_err("an expired in-flight token cannot become available again")
        .to_string()
        .contains("expired during execution"));
    assert!(!store.checkpoints.lock().unwrap().contains_key(&token));
}

#[test]
fn test_tester_invalidated_session_cannot_renew_or_advance_without_token() {
    let mut session =
        TransferCheckpointSession::new("nonexistent-plan", vec!["users".into()], Vec::new(), None);
    session.invalidate();

    assert!(session
        .renew()
        .expect_err("an invalidated session cannot renew its execution lease")
        .to_string()
        .contains("invalidated"));
    assert!(session
        .advance_table("users", vec![datazen_driver_api::Value::Integer(1)], 1)
        .expect_err("a chunk cannot advance before the opaque checkpoint exists")
        .to_string()
        .contains("was not created before writing"));
}
