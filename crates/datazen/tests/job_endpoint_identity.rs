//! P5 端点身份集成测试（CM-31）：端点重叠检测只按**物理位置键** `service_key`
//! 判定——不同物理端点放行、同一物理端点自覆盖拒绝、身份不可证明时 fail-closed。
//!
//! `connection_id` 不是重叠键：它是 §6.2 记账键（`ensure_service` 按它注册），
//! 一条保存连接可以按库覆盖成多个会话。见本文件 `cm31_*` 两个跨库用例。
//!
//! 2026-10-31（B1 测试归位）：桌面 `commands/data_transfer/job_api/tests/
//! endpoint_identity.rs` 整文件迁入（12 测）。那一组把身份从**真实
//! `ConnectionConfig`** 经 `services::migration_endpoint::identify` 导出、再喂进
//! 重叠检测与多端预算预留——与本文件既有 `cm31_*`（手写 service_key、直测内核
//! 规则）互补：这边证明规则本身，那边证明迁移路径上的身份真的来自用户配置而
//! 非常量。断言与测试名逐字保留；唯一改动是 mock 驱动导入路径
//! （`crate::testing::mock_driver` → 同源的 `datazen_driver_api::mock_driver`）。

mod support;

use std::sync::{Arc, Mutex};

use datazen::execution::application::budget::ResourceClass;
use datazen::execution::runtime::budget::{BudgetConfig, BudgetLedger};
use datazen::jobs::{
    detect_endpoint_overlap, EndpointRef, EndpointRole, JobError, MultiEndpointPermits,
};
use datazen::services::migration_endpoint::identify;
use datazen::shared::id::{ConnectionId, OrganizationId, PrincipalId};

use datazen_driver_api::mock_driver::{MockDriver, MockDriverOptions};
use datazen_driver_api::{ConnectionConfig, SslMode};

use support::conn;

/// 同名对象落在**两个不同**的物理端点上必须放行——这是数据迁移最常见的一步
/// （A 库的 public.users 拷到 B 库的 public.users）。
#[test]
fn cm31_same_object_on_two_different_endpoints_is_allowed() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["public.users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-bbbb".into(),
            objects: vec!["public.users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    assert!(
        detect_endpoint_overlap(&endpoints).is_ok(),
        "不同物理端点之间拷贝同名对象必须允许"
    );
}

/// 两个不同的连接配置指向同一台服务器（别名）时仍必须拒绝：service_key 是共享的，
/// 而按配置 id 分桶看不见自覆盖。
#[test]
fn cm31_alias_configs_sharing_one_service_are_still_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("别名自覆盖必须拒绝");
    let datazen::jobs::JobError::EndpointOverlap(message) = err else {
        panic!("应当是 EndpointOverlap");
    };
    // 错误必须点名两端与对象，否则用户无从判断该改哪一侧
    assert!(message.contains("`users`"), "{message}");
    assert!(message.contains(conn().as_str()), "{message}");
    assert!(message.contains("conn-2"), "{message}");
}

/// 同名对象落在**同一台服务器的不同库**上必须放行。
///
/// 这是本轨 D1 缺陷的反向断言：`connect_dedicated` 只覆盖 `effective_config.database`，
/// `effective_config.id` 仍是同一条持久化连接 id，因此「同一 connectionId」不等于
/// 「同一物理端点」。把它当重叠键会让合法的跨库搬运被一句事实错误的
/// EndpointOverlap 拒绝——这类键只能把 accept 变成 reject，永远不能把 reject 变回
/// accept，即对真实自覆盖零增益。
#[test]
fn cm31_one_connection_id_across_two_databases_is_allowed() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-staging".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-prod".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    assert!(
        detect_endpoint_overlap(&endpoints).is_ok(),
        "同一条连接配置下的两个不同数据库是不同物理端点，必须允许"
    );
}

/// 同一 connectionId + **同一** service_key 的读写同名对象仍必须拒绝。
///
/// 与上一个用例成对：删掉 `connection_id` 这个重叠键，不能顺带把真正的自覆盖放行。
#[test]
fn cm31_same_service_key_on_one_connection_is_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: conn(),
            service_key: "transfer:sha256-aaaa".into(),
            objects: vec!["users".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("同库自覆盖必须拒绝");
    assert!(
        matches!(err, datazen::jobs::JobError::EndpointOverlap(_)),
        "{err:?}"
    );
}

/// fail-closed：端点给不出 `service_key`（身份不可证明）且同时存在 reader 与 writer
/// 时拒绝，不开放危险自覆盖。`connection_id` 非空**不再**能救它。
#[test]
fn cm31_unprovable_identity_with_writer_is_rejected() {
    let endpoints = vec![
        EndpointRef {
            connection_id: conn(),
            service_key: "   ".into(),
            objects: vec!["users".into()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: ConnectionId::new("conn-2"),
            service_key: "transfer:sha256-bbbb".into(),
            objects: vec!["orders".into()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let err = detect_endpoint_overlap(&endpoints).expect_err("身份不可证明必须拒绝");
    let datazen::jobs::JobError::EndpointOverlap(message) = err else {
        panic!("应当是 EndpointOverlap");
    };
    assert!(
        message.contains("unprovable"),
        "必须是身份不可证明这一条，而不是伪装成某个对象的自覆盖: {message}"
    );
}

// ------------------------------ 自桌面 `job_api/tests/endpoint_identity.rs` 归位（B1）
//
// Endpoint identity on the Data Transfer Job path.
//
// Before the fix both endpoints were pinned to one constant `TRANSFER_SERVICE_KEY`
// and two placeholder connection ids, so copying `public.users` between two
// different physical databases collided on `("data-transfer", "public.users")`
// and was refused as a self-overwrite — a regression against the direct exec path.
// These tests pin the observable contract instead of the constant: identity comes
// from the user's real `ConnectionConfig`, the same object name on two distinct
// endpoints is accepted, and the same endpoint read *and* written is still refused.

fn org() -> OrganizationId {
    OrganizationId::new("org-1")
}

/// Mirrors `job_api::runtime::ensure_endpoint_services`: every endpoint is
/// registered under its own connection id before the first Job over it.
fn ledger_with(endpoints: &[EndpointRef]) -> Arc<Mutex<BudgetLedger>> {
    let ledger = Arc::new(Mutex::new(BudgetLedger::new(BudgetConfig::team_default())));
    let mut guard = ledger.lock().expect("ledger is not poisoned");
    for endpoint in endpoints {
        guard.ensure_service(&endpoint.connection_id);
    }
    drop(guard);
    ledger
}

fn config(id: &str, host: &str, database: &str, schema: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: id.to_string(),
        name: id.to_string(),
        database_type: "PostgreSQL".to_string(),
        host: Some(host.to_string()),
        port: Some(5432),
        database: Some(database.to_string()),
        schema: Some(schema.to_string()),
        username: Some("app_user".to_string()),
        password: Some("not-a-real-secret".to_string()),
        ssl_mode: SslMode::Prefer,
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

/// `conn-source` on `db-a.example.com/app`, and `conn-target` on
/// `db-b.example.com/app` — the same schema-qualified table name on two servers.
fn cross_database_pair() -> (ConnectionConfig, ConnectionConfig) {
    (
        config("conn-source", "db-a.example.com", "app", "public"),
        config("conn-target", "db-b.example.com", "app", "public"),
    )
}

/// These fixtures always name an explicit host, so the mock needs no declared
/// default: identity resolution asks the driver only for what the config omits.
fn driver() -> Arc<MockDriver> {
    MockDriver::new("PostgreSQL", MockDriverOptions::default())
}

fn refs(
    source: &ConnectionConfig,
    target: &ConnectionConfig,
    objects: &[&str],
) -> Vec<EndpointRef> {
    let objects: Vec<String> = objects.iter().map(|name| (*name).to_string()).collect();
    let driver = driver();
    let source_identity = identify(source, &*driver);
    let target_identity = identify(target, &*driver);
    vec![
        EndpointRef {
            connection_id: source_identity.connection_id,
            service_key: source_identity.service_key,
            objects: objects.clone(),
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: target_identity.connection_id,
            service_key: target_identity.service_key,
            objects,
            role: EndpointRole::TargetWriter,
        },
    ]
}

/// The defect: same object name, two different physical databases.
#[test]
fn same_object_on_two_physical_endpoints_is_accepted() {
    let (source, target) = cross_database_pair();
    let result = detect_endpoint_overlap(&refs(&source, &target, &["public.users"]));
    assert!(
        result.is_ok(),
        "two physical endpoints are not one service: {result:?}"
    );
}

/// Identity is real: the endpoint carries the user's own persisted connection id.
#[test]
fn endpoint_identity_carries_the_real_connection_config_id() {
    let (source, target) = cross_database_pair();
    let endpoints = refs(&source, &target, &["public.users"]);
    assert_eq!(endpoints[0].connection_id, ConnectionId::new("conn-source"));
    assert_eq!(endpoints[1].connection_id, ConnectionId::new("conn-target"));
}

/// Identity is real *and* per-physical-location: not a fixed per-side constant.
#[test]
fn service_key_is_not_a_constant_and_never_holds_config_text() {
    let (source, target) = cross_database_pair();
    let endpoints = refs(&source, &target, &["public.users"]);
    assert_ne!(
        endpoints[0].service_key, endpoints[1].service_key,
        "distinct servers must not share a budget service"
    );
    for endpoint in &endpoints {
        for secret in ["db-a.example.com", "db-b.example.com", "app_user", "public"] {
            assert!(
                !endpoint.service_key.contains(secret),
                "service_key must be a digest, not an echo of the config"
            );
        }
    }
}

/// Two different Jobs over different endpoints must not share a key, or the
/// atomic reservation would either collide falsely or miss a real conflict.
#[test]
fn another_job_on_other_endpoints_does_not_inherit_this_services_key() {
    let (source, _) = cross_database_pair();
    let elsewhere = config("conn-elsewhere", "db-c.example.com", "app", "public");
    assert_ne!(
        identify(&source, &*driver()).service_key,
        identify(&elsewhere, &*driver()).service_key
    );
}

/// The safety property: one physical endpoint read and written for one object.
#[test]
fn same_endpoint_read_and_written_is_still_refused() {
    let mut alias = config("conn-alias", "db-a.example.com", "app", "public");
    alias.name = "Alias of source".to_string();
    let source = config("conn-source", "db-a.example.com", "app", "public");

    let endpoints = refs(&source, &alias, &["public.users"]);
    let error = detect_endpoint_overlap(&endpoints).expect_err("one server, one object: refuse");
    let JobError::EndpointOverlap(message) = error else {
        panic!("expected an endpoint-overlap refusal");
    };
    assert!(message.contains("`public.users`"), "{message}");
    assert!(message.contains("conn-source"), "{message}");
    assert!(message.contains("conn-alias"), "{message}");
}

/// Two configs pointing at one server, same object: the catastrophic case legacy
/// allows on the direct path is refused here, and the message names both sides.
#[test]
fn one_connection_id_reserved_on_both_sides_is_refused() {
    let source = config("conn-source", "db-a.example.com", "app", "public");
    let mut target = config("conn-target", "db-b.example.com", "app", "public");
    target.id = "conn-source".to_string();
    target.host = Some("db-a.example.com".to_string());

    let error =
        detect_endpoint_overlap(&refs(&source, &target, &["users"])).expect_err("same connection");
    assert!(matches!(error, JobError::EndpointOverlap(_)), "{error:?}");
}

/// The whole Job reservation, not just the detector: two endpoints, same object
/// names, must produce two permits under one atomic all-or-nothing claim.
#[test]
fn a_cross_database_job_reserves_both_endpoints_atomically() {
    let (source, target) = cross_database_pair();
    let endpoints = refs(&source, &target, &["public.users", "public.roles"]);
    let ledger = ledger_with(&endpoints);

    let permits = MultiEndpointPermits::reserve(
        Arc::clone(&ledger),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    )
    .expect("two distinct endpoints over two objects");
    assert_eq!(
        permits.permits().len(),
        2,
        "each physical endpoint reserves its own permit"
    );
}

/// The same run against one physical endpoint must be refused before any permit
/// is taken — the refusal, not the budget, is what protects the data.
#[test]
fn a_self_overwrite_job_takes_no_permit_at_all() {
    let source = config("conn-source", "db-a.example.com", "app", "public");
    let mut target = config("conn-target", "db-b.example.com", "app", "public");
    target.host = Some("db-a.example.com".to_string());

    let endpoints = refs(&source, &target, &["public.users"]);
    let ledger = ledger_with(&endpoints);
    let Err(error) = MultiEndpointPermits::reserve(
        Arc::clone(&ledger),
        &endpoints,
        ResourceClass::Job,
        &org(),
        &PrincipalId::new("user-a"),
        0,
    ) else {
        panic!("one physical endpoint, one object must be refused");
    };
    assert!(matches!(error, JobError::EndpointOverlap(_)), "{error:?}");

    let guard = ledger.lock().expect("ledger is not poisoned");
    assert_eq!(
        guard.granted_total(),
        0,
        "a refused run must hold no budget at all"
    );
}

/// A cross-schema move on one server is legitimate and must not be refused —
/// this is where a key that included only the host would over-reject.
#[test]
fn cross_schema_move_on_one_server_is_accepted() {
    let source = config("conn-source", "db-a.example.com", "app", "public");
    let target = config("conn-target", "db-a.example.com", "app", "archive");

    let result = detect_endpoint_overlap(&refs(&source, &target, &["users"]));
    assert!(result.is_ok(), "schema is part of the location: {result:?}");
}

/// An endpoint whose config names no location gets no `Service` key at all — the
/// digest of an all-`None` identity would make every such config collide. It still
/// carries its real connection id, so it stays distinguishable from other configs.
#[test]
fn an_unlocatable_config_keeps_its_connection_id() {
    let mut source = config("conn-source", "db-a.example.com", "app", "public");
    source.host = None;
    source.database = None;
    let identity = identify(&source, &*driver());

    assert_eq!(
        identity.service_key, "",
        "an unlocatable config must not digest into a key"
    );
    assert_eq!(
        identity.connection_id,
        ConnectionId::new("conn-source"),
        "the real config id is always carried"
    );
}

/// Fail-closed, defense in depth: an endpoint with *no* identity at all — no
/// service key and no connection id — cannot prove it is distinct from the other
/// side, so a run with a writer is refused rather than silently allowed. The host
/// path cannot reach this (a persisted `connectionId` is never blank), which is
/// exactly why the branch is kept.
#[test]
fn an_endpoint_with_no_identity_at_all_is_refused_against_a_writer() {
    let (_, target) = cross_database_pair();
    let endpoints = vec![
        EndpointRef {
            connection_id: ConnectionId::new("  "),
            service_key: "   ".to_string(),
            objects: vec!["orders".to_string()],
            role: EndpointRole::SourceReader,
        },
        EndpointRef {
            connection_id: identify(&target, &*driver()).connection_id,
            service_key: identify(&target, &*driver()).service_key,
            objects: vec!["orders".to_string()],
            role: EndpointRole::TargetWriter,
        },
    ];
    let error = detect_endpoint_overlap(&endpoints)
        .expect_err("identity unprovable plus a writer must fail closed");
    assert!(matches!(error, JobError::EndpointOverlap(_)), "{error:?}");
}

/// A reader-only run — the SQL-file shape — carries no writer, so even an
/// identity-less endpoint cannot refuse it.
#[test]
fn an_endpoint_with_no_identity_and_no_writer_is_accepted() {
    let endpoints = [EndpointRef {
        connection_id: ConnectionId::new("  "),
        service_key: "   ".to_string(),
        objects: vec!["orders".to_string()],
        role: EndpointRole::SourceReader,
    }];
    let result = detect_endpoint_overlap(&endpoints);
    assert!(result.is_ok(), "a read-only run writes nothing: {result:?}");
}
