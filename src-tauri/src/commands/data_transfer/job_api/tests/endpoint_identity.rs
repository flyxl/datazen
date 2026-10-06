//! Endpoint identity on the Data Transfer Job path.
//!
//! Before the fix both endpoints were pinned to one constant `TRANSFER_SERVICE_KEY`
//! and two placeholder connection ids, so copying `public.users` between two
//! different physical databases collided on `("data-transfer", "public.users")`
//! and was refused as a self-overwrite — a regression against the direct exec path.
//! These tests pin the observable contract instead of the constant: identity comes
//! from the user's real `ConnectionConfig`, the same object name on two distinct
//! endpoints is accepted, and the same endpoint read *and* written is still refused.

use std::sync::{Arc, Mutex};

use datazen_platform_api::id::{ConnectionId, OrganizationId, PrincipalId};
use datazen_platform_api::ports::budget::ResourceClass;
use datazen_runtime::budget::{BudgetConfig, BudgetLedger};
use datazen_runtime::job::{
    detect_endpoint_overlap, EndpointRef, EndpointRole, JobError, MultiEndpointPermits,
};

use crate::commands::data_transfer::job_api::endpoint_identity::identify;
use crate::db::{ConnectionConfig, SslMode};

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

fn refs(source: &ConnectionConfig, target: &ConnectionConfig, objects: &[&str]) -> Vec<EndpointRef> {
    let objects: Vec<String> = objects.iter().map(|name| (*name).to_string()).collect();
    let source_identity = identify(source);
    let target_identity = identify(target);
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
        identify(&source).service_key,
        identify(&elsewhere).service_key
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
    let identity = identify(&source);

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
            connection_id: identify(&target).connection_id,
            service_key: identify(&target).service_key,
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