use super::*;
use datazen_driver_api::{MySqlViewMetadata, ObjectKind};
fn config() -> ConnectionConfig {
    serde_json::from_value(serde_json::json!({"id":"target","name":"target","databaseType":"postgresql","host":"localhost","port":5432,"database":"test","connectionTimeout":30,"maxPoolSize":3})).unwrap()
}
fn plan() -> SchemaDiffPlan {
    serde_json::from_value(serde_json::json!({"table":"t","tables":["t"],"sourceDialect":"postgresql","targetDialect":"postgresql","sameDialect":true,"statements":[],"warnings":[],"requirements":[],"rollbackCompleteness":{"complete":true,"missing":[]}})).unwrap()
}
#[tokio::test]
async fn reviewed_plan_rejects_client_mutation_wrong_target_and_replay() {
    let mut plan = plan();
    let config = config();
    let handle = ConnectionHandle {
        id: "session".into(),
        pool_id: "pool".into(),
    };
    freeze(
        &mut plan,
        "session".into(),
        &handle,
        &config,
        vec![],
        config.database.clone(),
        Some("public".into()),
    )
    .await;
    let mut tampered = plan.clone();
    tampered.warnings.push("client modification".into());
    assert!(consume(
        &tampered,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    assert!(consume(
        &plan,
        "other",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    let mut changed = config.clone();
    changed.database = Some("other_database".into());
    assert!(consume(
        &plan,
        "session",
        &handle,
        &changed,
        changed.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    changed = config.clone();
    changed.read_only = true;
    assert!(consume(
        &plan,
        "session",
        &handle,
        &changed,
        changed.database.as_deref(),
        Some("public"),
    )
    .await
    .is_err());
    assert!(consume(
        &plan,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("public"),
    )
    .await
    .is_ok());
    assert!(consume(
        &plan,
        "session",
        &handle,
        &config,
        config.database.as_deref(),
        Some("archive"),
    )
    .await
    .is_err());
}
#[test]
fn unified_source_connection_fingerprint_rejects_database_and_schema_changes() {
    let source = config();
    let snapshot = ReviewedSourceSnapshot {
        session: "source-session".into(),
        pool: "source-pool".into(),
        identity: identity(&source),
        database_scope: source.database.clone(),
        schema_scope: source.schema.clone(),
        table_snapshots: vec![],
        object_snapshots: vec![],
        table_dependency_catalog: vec![],
    };
    assert!(validate_source_connection_snapshot(
        &snapshot,
        "source-session",
        "source-pool",
        &source
    )
    .is_ok());

    let mut changed = source.clone();
    changed.database = Some("other_database".into());
    assert!(validate_source_connection_snapshot(
        &snapshot,
        "source-session",
        "source-pool",
        &changed
    )
    .is_err());

    let mut changed = source;
    changed.schema = Some("archive".into());
    assert!(validate_source_connection_snapshot(
        &snapshot,
        "source-session",
        "source-pool",
        &changed
    )
    .is_err());
}

#[test]
fn self_target_is_independent_of_user_and_persisted_connection_id() {
    let a = config();
    let mut b = a.clone();
    b.id = "other".into();
    b.username = Some("other_user".into());
    assert!(same_endpoint(&a, &b));
    b.database = Some("other_database".into());
    assert!(!same_endpoint(&a, &b));
}

#[test]
fn physical_database_scope_detects_aliases_and_fails_closed_when_unknown() {
    use PhysicalDatabaseScope::{Different, Same, Unknown};
    let a = config();
    let mut alias = a.clone();
    alias.id = "alias".into();
    alias.host = Some("db-alias.example".into());
    assert_eq!(
        physical_database_scope(
            &a,
            &alias,
            Some("postgresql:cluster:app"),
            Some("postgresql:cluster:app"),
            None,
            None,
            None,
            None,
        ),
        Same
    );

    let mut pg_alias = a.clone();
    pg_alias.database_type = "postgres".into();
    assert!(same_endpoint(&pg_alias, &a));
    assert_eq!(
        physical_database_scope(
            &pg_alias,
            &a,
            Some("postgresql:cluster:app"),
            Some("postgresql:cluster:app"),
            None,
            None,
            None,
            None,
        ),
        Same
    );

    alias.schema = Some("archive".into());
    assert_eq!(
        physical_database_scope(
            &a,
            &alias,
            Some("postgresql:cluster:app"),
            Some("postgresql:cluster:app"),
            Some("public"),
            Some("archive"),
            None,
            None,
        ),
        Different
    );
    assert_eq!(
        physical_database_scope(
            &a,
            &alias,
            Some("postgresql:cluster:app"),
            Some("mysql:cluster:app"),
            None,
            None,
            None,
            None,
        ),
        Different
    );
    assert_eq!(
        physical_database_scope(
            &a,
            &a,
            Some("postgresql:cluster:app"),
            Some("postgresql:cluster:app"),
            Some("public"),
            Some("archive"),
            None,
            None,
        ),
        Different
    );
    assert_eq!(
        physical_database_scope(&a, &alias, None, None, None, None, None, None),
        Unknown
    );
}

#[test]
fn sqlserver_physical_scope_allows_only_proven_distinct_schemas() {
    use PhysicalDatabaseScope::{Different, Same, Unknown};

    let mut source = config();
    source.database_type = "sqlserver".into();
    source.database = Some("DataZen".into());
    source.schema = Some("dbo".into());
    let identity = "sqlserver:server:13:prod-instance:database:23";

    let same_schema = source.clone();
    assert_eq!(
        physical_database_scope(
            &source,
            &same_schema,
            Some(identity),
            Some(identity),
            Some("dbo"),
            Some("dbo"),
            Some("sqlserver:schema-id:1"),
            Some("sqlserver:schema-id:1"),
        ),
        Same
    );

    let mut other_schema = source.clone();
    other_schema.schema = Some("sales".into());
    assert_eq!(
        physical_database_scope(
            &source,
            &other_schema,
            Some(identity),
            Some(identity),
            Some("dbo"),
            Some("sales"),
            Some("sqlserver:schema-id:1"),
            Some("sqlserver:schema-id:2"),
        ),
        Different
    );

    // Collation can make distinct spellings resolve to the same schema_id.
    assert_eq!(
        physical_database_scope(
            &source,
            &source,
            Some(identity),
            Some(identity),
            Some("dbo"),
            Some("DBO"),
            Some("sqlserver:schema-id:1"),
            Some("sqlserver:schema-id:1"),
        ),
        Same
    );
    assert_eq!(
        physical_database_scope(
            &source,
            &source,
            Some(identity),
            Some(identity),
            Some("dbo"),
            Some("审计"),
            Some("sqlserver:schema-id:1"),
            None,
        ),
        Same
    );

    assert_eq!(
        physical_database_scope(
            &source,
            &other_schema,
            Some(identity),
            Some("sqlserver:server:13:prod-instance:database:24"),
            Some("dbo"),
            Some("sales"),
            None,
            None,
        ),
        Different
    );
    assert_eq!(
        physical_database_scope(
            &source,
            &other_schema,
            None,
            None,
            Some("dbo"),
            Some("sales"),
            None,
            None,
        ),
        Unknown
    );
}

#[test]
fn physical_scope_uses_mysql_identity_before_case_foldable_names() {
    use PhysicalDatabaseScope::{Same, Unknown};
    let mut source = config();
    source.database_type = "mysql".into();
    source.database = Some("AppDb".into());
    let mut target = source.clone();
    target.database = Some("appdb".into());

    assert_eq!(
        physical_database_scope(
            &source,
            &target,
            Some("mysql:server:canonical-app-db"),
            Some("mysql:server:canonical-app-db"),
            None,
            None,
            None,
            None,
        ),
        Same
    );
    assert_eq!(
        physical_database_scope(&source, &target, None, None, None, None, None, None),
        Unknown
    );
}

#[test]
fn unified_target_dependency_catalog_revalidation_detects_stale_and_incomplete_data() {
    let view = SchemaObjectIdentity {
        kind: ObjectKind::View,
        schema: Some("public".into()),
        name: "account_view".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let accounts = SchemaObjectIdentity::table(Some("public"), "accounts");
    let reviewed = vec![SchemaObjectDependencySnapshot {
        identity: view.clone(),
        dependencies: Some(vec![accounts.clone()]),
        type_dependency_usages: None,
        sequence_dependency_usages: None,
    }];
    let reordered = vec![SchemaObjectDependencySnapshot {
        identity: view.clone(),
        dependencies: Some(vec![accounts.clone(), accounts.clone()]),
        type_dependency_usages: None,
        sequence_dependency_usages: None,
    }];
    assert!(validate_object_dependency_catalog(&reviewed, &reordered).is_ok());

    let changed_dependency = vec![SchemaObjectDependencySnapshot {
        identity: view.clone(),
        dependencies: Some(vec![SchemaObjectIdentity::table(
            Some("public"),
            "archived_accounts",
        )]),
        type_dependency_usages: None,
        sequence_dependency_usages: None,
    }];
    assert!(validate_object_dependency_catalog(&reviewed, &changed_dependency).is_err());
    let source_table = SchemaObjectIdentity::table(Some("public"), "typed_accounts");
    let custom_type = SchemaObjectIdentity {
        kind: ObjectKind::Type,
        schema: Some("public".into()),
        name: "account_state".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let reviewed_type_usage = vec![SchemaObjectDependencySnapshot {
        identity: source_table.clone(),
        dependencies: Some(vec![custom_type.clone()]),
        type_dependency_usages: Some(vec![
            crate::object_identity::TypeDependencyUsage {
                dependency: custom_type.clone(),
                usage: crate::object_identity::TypeDependencyUsageKind::ColumnType,
                column_name: Some("state".into()),
            },
        ]),
        sequence_dependency_usages: None,
    }];
    let changed_type_usage = vec![SchemaObjectDependencySnapshot {
        identity: source_table,
        dependencies: Some(vec![custom_type.clone()]),
        type_dependency_usages: Some(vec![
            crate::object_identity::TypeDependencyUsage {
                dependency: custom_type,
                usage: crate::object_identity::TypeDependencyUsageKind::Expression,
                column_name: None,
            },
        ]),
        sequence_dependency_usages: None,
    }];
    assert!(validate_object_dependency_catalog(&reviewed_type_usage, &changed_type_usage).is_err());

    let sequence = SchemaObjectIdentity {
        kind: ObjectKind::Sequence,
        schema: Some("public".into()),
        name: "orders_id_seq".into(),
        signature: None,
        target_schema: None,
        target_name: None,
    };
    let owner = SchemaObjectIdentity::table(Some("public"), "orders");
    let sequence_review = vec![SchemaObjectDependencySnapshot {
        identity: sequence.clone(),
        dependencies: Some(vec![owner.clone()]),
        type_dependency_usages: None,
        sequence_dependency_usages: Some(vec![
            crate::object_identity::SequenceDependencyUsage {
                sequence: sequence.clone(),
                owner_table: owner.clone(),
                column_name: "id".into(),
                usage: crate::object_identity::SequenceDependencyUsageKind::OwnedBy,
            },
        ]),
    }];
    let changed_sequence_usage = vec![SchemaObjectDependencySnapshot {
        identity: sequence.clone(),
        dependencies: Some(vec![owner.clone()]),
        type_dependency_usages: None,
        sequence_dependency_usages: Some(vec![
            crate::object_identity::SequenceDependencyUsage {
                sequence,
                owner_table: owner,
                column_name: "legacy_id".into(),
                usage: crate::object_identity::SequenceDependencyUsageKind::OwnedBy,
            },
        ]),
    }];
    assert!(validate_object_dependency_catalog(&sequence_review, &changed_sequence_usage).is_err());
    assert!(validate_object_dependency_catalog(&reviewed, &[]).is_err());
    assert!(validate_object_dependency_catalog(
        &reviewed,
        &[SchemaObjectDependencySnapshot {
            identity: view.clone(),
            dependencies: None,
            type_dependency_usages: None,
            sequence_dependency_usages: None,
        }]
    )
    .is_err());
    assert!(validate_object_dependency_catalog(
        &reviewed,
        &[
            SchemaObjectDependencySnapshot {
                identity: view.clone(),
                dependencies: Some(vec![accounts.clone()]),
                type_dependency_usages: None,
                sequence_dependency_usages: None,
            },
            SchemaObjectDependencySnapshot {
                identity: view,
                dependencies: Some(vec![accounts]),
                type_dependency_usages: None,
                sequence_dependency_usages: None,
            },
        ]
    )
    .is_err());
}

#[test]
fn unified_table_catalog_revalidation_requires_same_exact_identities() {
    let reviewed = vec![SchemaObjectIdentity::table(Some("public"), "accounts")];
    assert!(validate_table_identity_catalog(&reviewed, &reviewed).is_ok());
    assert!(validate_table_identity_catalog(
        &reviewed,
        &[SchemaObjectIdentity::table(Some("archive"), "accounts")]
    )
    .is_err());
    assert!(validate_table_identity_catalog(&reviewed, &[]).is_err());
}

#[test]
fn postgres_names_can_prove_distinct_database_or_schema_without_identity() {
    use PhysicalDatabaseScope::Different;
    let source = config();
    let mut different_database = source.clone();
    different_database.database = Some("another_db".into());
    assert_eq!(
        physical_database_scope(
            &source,
            &different_database,
            None,
            None,
            None,
            None,
            None,
            None,
        ),
        Different
    );

    let target = source.clone();
    assert_eq!(
        physical_database_scope(
            &source,
            &target,
            None,
            None,
            Some("public"),
            Some("archive"),
            None,
            None,
        ),
        Different
    );
}
#[test]
fn target_snapshot_detects_structure_changed_after_review() {
    let old = TableSchema {
        table_name: "t".into(),
        columns: vec![],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    assert!(validate_snapshot("t", &old, &old).is_ok());
    let mut current = old.clone();
    current.primary_keys.push("id".into());
    assert!(validate_snapshot("t", &old, &current).is_err());
    current = old.clone();
    current.indexes.push(datazen_driver_api::IndexInfo {
        name: "external".into(),
        columns: vec!["id".into()],
        is_unique: false,
        is_primary: false,
        index_type: "btree".into(),
    });
    assert!(validate_snapshot("t", &old, &current).is_err());
}

#[test]
fn pg_target_snapshot_accepts_qualified_prepare_name_and_bare_deploy_name() {
    let empty_missing = TableSchema {
        table_name: "public.missing".into(),
        columns: vec![],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    let mut deployed_missing = empty_missing.clone();
    deployed_missing.table_name = "missing".into();
    assert!(validate_snapshot("public.missing", &empty_missing, &deployed_missing).is_ok());

    let existing = TableSchema {
        table_name: "public.child".into(),
        columns: vec![datazen_driver_api::ColumnSchema {
            name: "parent_id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    let mut unchanged = existing.clone();
    unchanged.table_name = "child".into();
    assert!(validate_snapshot("public.child", &existing, &unchanged).is_ok());

    let mut changed = unchanged.clone();
    changed
        .foreign_keys
        .push(datazen_driver_api::ForeignKeyInfo {
            name: "fk_child_parent".into(),
            columns: vec!["parent_id".into()],
            referenced_table: "public.parent".into(),
            referenced_columns: vec!["id".into()],
            on_update: "NO ACTION".into(),
            on_delete: "NO ACTION".into(),
            deferrability: datazen_driver_api::ForeignKeyDeferrability::NotDeferrable,
        });
    assert!(validate_snapshot("public.child", &existing, &changed).is_err());

    let mut wrong_schema = unchanged.clone();
    wrong_schema.table_name = "archive.child".into();
    assert!(validate_snapshot("public.child", &existing, &wrong_schema).is_err());

    let mut wrong_table = unchanged;
    wrong_table.table_name = "other_child".into();
    assert!(validate_snapshot("public.child", &existing, &wrong_table).is_err());

    let mut unqualified_reviewed = existing.clone();
    unqualified_reviewed.table_name = "child".into();
    let unexpectedly_qualified = TableSchema {
        table_name: "archive.child".into(),
        columns: vec![],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    assert!(validate_snapshot("child", &unqualified_reviewed, &unexpectedly_qualified).is_err());
}

#[test]
fn test_tester_target_snapshot_detects_check_constraint_changed_after_review() {
    let old = TableSchema {
        table_name: "t".into(),
        columns: vec![],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![datazen_driver_api::CheckConstraint {
            name: "t_positive_id".into(),
            expression: "id > 0".into(),
        }],
        table_options: Default::default(),
    };
    assert!(validate_snapshot("t", &old, &old).is_ok());
    let mut current = old.clone();
    current.check_constraints[0].expression = "id >= 0".into();
    assert!(validate_snapshot("t", &old, &current).is_err());
}

#[test]
fn target_only_snapshot_detects_table_disappearing_after_review() {
    let reviewed = TableSchema {
        table_name: "archive".into(),
        columns: vec![datazen_driver_api::ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            comment: None,
            is_primary_key: true,
            is_auto_increment: false,
        }],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };
    let disappeared = TableSchema {
        table_name: "archive".into(),
        columns: vec![],
        primary_keys: vec![],
        indexes: vec![],
        foreign_keys: vec![],
        check_constraints: vec![],
        table_options: Default::default(),
    };

    assert!(validate_snapshot("archive", &reviewed, &reviewed).is_ok());
    let error = validate_snapshot("archive", &reviewed, &disappeared).unwrap_err();
    assert!(error.contains("Target schema changed for archive"));
}

#[test]
fn target_object_snapshot_detects_definition_or_identity_changes() {
    let old = SchemaObjectSnapshot::view(Some("public"), "active_users", "SELECT id FROM users");
    assert!(validate_object_snapshot(&old, &old).is_ok());
    let mut changed = old.clone();
    changed.definition = "SELECT id, email FROM users".into();
    assert!(validate_object_snapshot(&old, &changed).is_err());
    changed = old.clone();
    changed.schema = Some("other".into());
    assert!(validate_object_snapshot(&old, &changed).is_err());
}

#[test]
fn reviewed_mysql_view_snapshot_detects_creation_semantic_changes() {
    let metadata = MySqlViewMetadata {
        algorithm: "UNDEFINED".into(),
        definer: "migrator@localhost".into(),
        security_type: "DEFINER".into(),
        check_option: "NONE".into(),
        character_set_client: "utf8mb4".into(),
        collation_connection: "utf8mb4_0900_ai_ci".into(),
        has_explicit_column_list: false,
    };
    let reviewed = SchemaObjectSnapshot::view(Some("source_db"), "item_view", "SELECT 1")
        .with_mysql_view_metadata(metadata.clone());
    assert!(validate_object_snapshot(&reviewed, &reviewed).is_ok());
    let changed = SchemaObjectSnapshot::view(Some("source_db"), "item_view", "SELECT 1")
        .with_mysql_view_metadata(MySqlViewMetadata {
            security_type: "INVOKER".into(),
            ..metadata
        });
    assert!(validate_object_snapshot(&reviewed, &changed).is_err());
}

#[test]
fn test_tester_sequence_snapshot_detects_catalog_ddl_and_identity_changes() {
    let reviewed = SchemaObjectSnapshot::sequence(
            Some("public"),
            "orders_id_seq",
            "CREATE SEQUENCE \"public\".\"orders_id_seq\" AS bigint INCREMENT BY 1 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE;",
        );
    assert!(validate_object_snapshot(&reviewed, &reviewed).is_ok());
    let changed = SchemaObjectSnapshot::sequence(
            Some("public"),
            "orders_id_seq",
            "CREATE SEQUENCE \"public\".\"orders_id_seq\" AS bigint INCREMENT BY 2 MINVALUE 1 MAXVALUE 9223372036854775807 START WITH 1 CACHE 1 NO CYCLE;",
        );
    assert!(validate_object_snapshot(&reviewed, &changed).is_err());
    let wrong_identity =
        SchemaObjectSnapshot::sequence(Some("other"), "orders_id_seq", &reviewed.definition);
    assert!(validate_object_snapshot(&reviewed, &wrong_identity).is_err());
}
#[tokio::test]
async fn test_tester_concurrent_deploy_consumes_exactly_once() {
    let mut plan = plan();
    let cfg = config();
    let handle = ConnectionHandle {
        id: "concurrent".into(),
        pool_id: "concurrent_pool".into(),
    };
    freeze(
        &mut plan,
        "concurrent".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        cfg.schema.clone(),
    )
    .await;
    let (a, b) = tokio::join!(
        consume(
            &plan,
            "concurrent",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            cfg.schema.as_deref()
        ),
        consume(
            &plan,
            "concurrent",
            &handle,
            &cfg,
            cfg.database.as_deref(),
            cfg.schema.as_deref()
        )
    );
    assert_ne!(a.is_ok(), b.is_ok());
}
#[tokio::test]
async fn test_tester_pool_change_rejected_without_consuming_valid_plan() {
    let mut plan = plan();
    let cfg = config();
    let handle = ConnectionHandle {
        id: "pooltest".into(),
        pool_id: "original".into(),
    };
    freeze(
        &mut plan,
        "pooltest".into(),
        &handle,
        &cfg,
        vec![],
        cfg.database.clone(),
        cfg.schema.clone(),
    )
    .await;
    let changed = ConnectionHandle {
        id: "pooltest".into(),
        pool_id: "replacement".into(),
    };
    assert!(consume(
        &plan,
        "pooltest",
        &changed,
        &cfg,
        cfg.database.as_deref(),
        cfg.schema.as_deref()
    )
    .await
    .is_err());
    assert!(consume(
        &plan,
        "pooltest",
        &handle,
        &cfg,
        cfg.database.as_deref(),
        cfg.schema.as_deref()
    )
    .await
    .is_ok());
}
