//! `[tester]` Independent re-verification of the W3-B key-tree budget contract
//! (`redis-tree-backend`). Written against the *published* surface only
//! (`command_definitions()` + `execute_command()`), never against crate
//! internals: the in-crate suite (`src/ops_tree_scan/tests.rs`) drives the
//! ledger through a scripted connection double, so it structurally cannot see
//! (a) the command input parsing or (b) the payload built in
//! `commands_exec_dispatch.rs`. This file owns exactly that seam, plus the
//! live-server probes the `## 留待 R 回归` list needs and that a double can
//! never substitute for.
//!
//! Offline (part of the default gate):
//! * every input name the dispatch arm parses is declared in the schema, and
//!   `budget` declares no bounds at all — the ladder is clamped in the driver,
//!   never rejected in validation;
//! * `key_probe` is a peer of `get_key` (permission / category / access level).
//!
//! Live Redis (`#[ignore]`, run by R with `--ignored` and
//! `DATAZEN_TEST_REDIS_URL`):
//! * each frozen reply field is present on the *real* serialised payload;
//! * `scan_keys`' `dbSize` and `dbsize` are the same number under both
//!   spellings (append-only: the old name is a projection, not a second value);
//! * `budget: 0` behaves exactly like an absent budget (the frozen sentence),
//!   while an explicit tier is honoured verbatim.

use std::sync::Arc;

use datazen_driver_api::{
    required_access_level, validate_command_input, CommandAccessLevel, ConnectionConfig,
    ConnectionHandle, DatabaseDriver, DriverCommandDefinition, SslMode,
};
use datazen_driver_redis::RedisDriver;
use serde_json::{json, Value};

fn definition(command: &str) -> DriverCommandDefinition {
    RedisDriver::new()
        .command_definitions()
        .into_iter()
        .find(|def| def.id == command)
        .unwrap_or_else(|| panic!("'{command}' must be registered"))
}

fn props(def: &DriverCommandDefinition) -> Vec<String> {
    def.input_schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|m| m.keys().cloned().collect())
        .unwrap_or_default()
}

fn has(props: &[String], name: &str) -> bool {
    props.iter().any(|p| p == name)
}

// ---------------------------------------------------------------------------
// Offline: declaration surface
// ---------------------------------------------------------------------------

/// Every parameter the dispatch arm reads must appear in the schema, or Wave 4
/// cannot see it exists. This is the *input* mirror of the payload freeze.
#[test]
fn test_tester_every_parsed_input_name_is_declared() {
    let cases: [(&str, &[&str]); 4] = [
        (
            "scan_keys",
            &[
                "dbIndex",
                "pattern",
                "cursor",
                "count",
                "keyType",
                "withMemory",
                "noTtlOnly",
                "budget",
            ],
        ),
        (
            "list_children",
            &[
                "dbIndex",
                "prefix",
                "cursor",
                "count",
                "sep",
                "noTtlOnly",
                "keyType",
                "withMemory",
                "budget",
            ],
        ),
        ("count_matching", &["dbIndex", "pattern", "budget"]),
        ("key_probe", &["dbIndex", "key"]),
    ];
    for (command, expected) in cases {
        let declared = props(&definition(command));
        for name in expected {
            assert!(
                has(&declared, name),
                "{command} must declare '{name}': {declared:?}"
            );
        }
    }
}

/// The freeze says an oversized budget is clamped, not rejected. A `minimum`
/// in the schema would reject `0` (which the freeze defines as "derive it"),
/// and a `maximum` would reject `1M+1` — both turn a driver behaviour into a
/// validation error, which is exactly what the contract forbids.
#[test]
fn test_tester_budget_declares_no_bounds_and_zero_still_validates() {
    for command in ["scan_keys", "list_children", "count_matching"] {
        let def = definition(command);
        let budget = &def.input_schema["properties"]["budget"];
        assert_eq!(budget["type"], "integer", "{command}");
        assert!(
            budget.get("minimum").is_none(),
            "{command}: a schema minimum would reject budget 0, which the freeze defines as 'derive from DBSIZE'"
        );
        assert!(
            budget.get("maximum").is_none(),
            "{command}: a schema maximum would move the clamp out of the ledger"
        );
        let mut zero = json!({ "budget": 0 });
        if command == "list_children" {
            zero["prefix"] = json!("app:");
        }
        if command == "count_matching" {
            zero["pattern"] = json!("app:*");
        }
        assert_eq!(
            validate_command_input(&def, &zero),
            Ok(()),
            "{command}: budget 0 must reach the ledger"
        );
    }
}

/// `key_probe` rides `get_key`'s read口径 (freeze §`key_probe`): same
/// permission, same category, access level `Read`. A probe that fell to
/// `Mutate` would be blocked by SafeMode while still being read-only.
#[test]
fn test_tester_key_probe_is_a_get_key_peer() {
    let probe = definition("key_probe");
    let get_key = definition("get_key");
    assert_eq!(
        probe.permissions, get_key.permissions,
        "key_probe must share get_key's permission"
    );
    assert_eq!(probe.metadata.category, get_key.metadata.category);
    assert_eq!(
        required_access_level(&probe),
        CommandAccessLevel::Read,
        "an attribute probe never writes"
    );
    assert_eq!(
        required_access_level(&get_key),
        required_access_level(&probe),
        "and must be gated exactly like it"
    );
}

/// The freeze accepts `key_type` / `with_memory` / `no_ttl_only` aliases as well
/// as the camelCase names. Validation is structural, so an alias-only input
/// must not be rejected for "missing" the camelCase spelling.
#[test]
fn test_tester_snake_case_aliases_pass_validation() {
    let def = definition("scan_keys");
    assert_eq!(
        validate_command_input(
            &def,
            &json!({ "key_type": "hash", "with_memory": true, "no_ttl_only": true })
        ),
        Ok(()),
        "the aliases the freeze promises must survive validation"
    );
}

// ---------------------------------------------------------------------------
// Live Redis: payload freeze + budget semantics
// ---------------------------------------------------------------------------

fn standalone_config(url: &str) -> ConnectionConfig {
    let authority = url
        .trim_start_matches("redis://")
        .trim_start_matches("rediss://");
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h.to_string(), p.parse::<u16>().unwrap_or(6379)),
        None => (authority.to_string(), 6379),
    };
    ConnectionConfig {
        id: "tester-tree-backend".into(),
        name: "tester-tree-backend".into(),
        database_type: "redis".into(),
        host: Some(host),
        port: Some(port),
        database: Some("0".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::default(),
        connection_timeout: 10,
        max_pool_size: 2,
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

/// Connect a real Redis, as the host would.
#[allow(dead_code)]
async fn live() -> (Arc<dyn DatabaseDriver>, ConnectionHandle) {
    let url = std::env::var("DATAZEN_TEST_REDIS_URL").expect("set DATAZEN_TEST_REDIS_URL");
    let driver: Arc<dyn DatabaseDriver> = Arc::new(RedisDriver::new());
    let handle = driver
        .connect(&standalone_config(&url))
        .await
        .expect("live redis must accept the connection");
    (driver, handle)
}

#[allow(dead_code)]
async fn ask(
    driver: &Arc<dyn DatabaseDriver>,
    handle: &ConnectionHandle,
    command: &str,
    input: Value,
) -> Value {
    match driver.execute_command(handle, command, input).await {
        Ok(result) => {
            eprintln!(
                "[payload] {command} = {}",
                serde_json::to_string(&result).unwrap()
            );
            result.data
        }
        Err(err) => panic!("{command} failed: {err}"),
    }
}

/// Every field the frozen budget contract promises for `scan_keys`, on the real payload —
/// including that the legacy `dbSize` spelling and the appended `dbsize` carry
/// the same number (one DBSIZE read, two names, append-only contract).
#[tokio::test]
#[ignore = "requires a live Redis (DATAZEN_TEST_REDIS_URL); see ## 留待 R 回归"]
async fn test_tester_scan_keys_payload_matches_the_freeze() {
    let (driver, handle) = live().await;
    let payload = ask(
        &driver,
        &handle,
        "scan_keys",
        json!({ "dbIndex": 0, "pattern": "*", "count": 10 }),
    )
    .await;

    for field in [
        "cursor",
        "keys",
        "dbSize",
        "consumed",
        "truncated",
        "dbsize",
        "exact",
    ] {
        assert!(
            payload.get(field).is_some(),
            "scan_keys reply lost frozen field '{field}': {payload}"
        );
    }
    assert_eq!(
        payload["dbSize"], payload["dbsize"],
        "dbSize is the established projection of the same DBSIZE read, not a second value"
    );
    assert!(payload["exact"].is_boolean() && payload["truncated"].is_boolean());
    if let Some(first) = payload["keys"].as_array().and_then(|a| a.first()) {
        for field in ["key", "keyType", "ttl", "size", "preview"] {
            assert!(
                first.get(field).is_some(),
                "KeyEntry lost frozen field '{field}': {first}"
            );
        }
    }
}

/// `list_children` keeps `children` + `cursor` and gains the budget trio; a
/// `key` child carries the full `ChildEntry` field set, `memBytes` being
/// nullable rather than absent.
#[tokio::test]
#[ignore = "requires a live Redis (DATAZEN_TEST_REDIS_URL); see ## 留待 R 回归"]
async fn test_tester_list_children_payload_matches_the_freeze() {
    let (driver, handle) = live().await;
    let payload = ask(
        &driver,
        &handle,
        "list_children",
        json!({ "dbIndex": 0, "prefix": "" }),
    )
    .await;
    for field in ["children", "cursor", "consumed", "truncated", "dbsize"] {
        assert!(
            payload.get(field).is_some(),
            "list_children reply lost frozen field '{field}': {payload}"
        );
    }
    for child in payload["children"].as_array().unwrap_or(&Vec::new()) {
        match child["kind"].as_str() {
            Some("folder") => {
                for field in ["kind", "prefix", "count"] {
                    assert!(child.get(field).is_some(), "folder lost '{field}': {child}");
                }
            }
            Some("key") => {
                for field in ["kind", "key", "keyType", "ttl", "logicalLen"] {
                    assert!(
                        child.get(field).is_some(),
                        "key child lost '{field}': {child}"
                    );
                }
                assert!(
                    child.get("memBytes").is_some(),
                    "memBytes must stay present as null when withMemory was not asked: {child}"
                );
            }
            other => panic!("unknown child kind {other:?} in {child}"),
        }
    }
}

/// `count_matching` answers the four-field census object, and `key_probe`
/// answers all four fields for a key that is not there (nulls present, not
/// omitted) — the shape Wave 4's `n+` and sidebar render from.
#[tokio::test]
#[ignore = "requires a live Redis (DATAZEN_TEST_REDIS_URL); see ## 留待 R 回归"]
async fn test_tester_count_and_probe_payloads_match_the_freeze() {
    let (driver, handle) = live().await;

    let count = ask(
        &driver,
        &handle,
        "count_matching",
        json!({ "dbIndex": 0, "pattern": "*" }),
    )
    .await;
    for field in ["count", "truncated", "consumed", "dbsize"] {
        assert!(
            count.get(field).is_some(),
            "count_matching lost '{field}': {count}"
        );
    }
    assert_eq!(
        count["count"], count["dbsize"],
        "freeze: `*` answers count == dbsize"
    );
    assert_eq!(count["consumed"], json!(0), "freeze: `*` spends no COUNT");

    let probe = ask(
        &driver,
        &handle,
        "key_probe",
        json!({ "dbIndex": 0, "key": "datazen:tester:absent-key-9f2c" }),
    )
    .await;
    assert_eq!(
        probe,
        json!({ "exists": false, "type": null, "ttlMs": -2, "memoryBytes": null }),
        "a missing key must answer with every frozen field present"
    );
}

/// The budget sentence as an observable fact: `budget: 0` buys the same scan as
/// no budget at all (derived tier), while an explicit tier is honoured to the
/// unit. This is the end-to-end proof of "缺失或 0 ⇒ min(1M, max(50k, dbsize×2))"
/// — the ledger's own unit test pins the *opposite* for `Some(0)` and is
/// unreachable through the command path (see redis-tree-backend-BUG-001).
#[tokio::test]
#[ignore = "requires a live Redis (DATAZEN_TEST_REDIS_URL); see ## 留待 R 回归"]
async fn test_tester_zero_budget_equals_absent_budget_over_the_wire() {
    let (driver, handle) = live().await;

    let absent = ask(
        &driver,
        &handle,
        "scan_keys",
        json!({ "dbIndex": 0, "pattern": "tester-absent-prefix-*", "count": 1 }),
    )
    .await;
    let zero = ask(
        &driver,
        &handle,
        "scan_keys",
        json!({
            "dbIndex": 0,
            "pattern": "tester-absent-prefix-*",
            "count": 1,
            "budget": 0
        }),
    )
    .await;
    assert_eq!(
        zero["consumed"], absent["consumed"],
        "budget 0 must be the same action as no budget (derived tier), never a 1-unit scan"
    );
    assert!(
        zero["consumed"].as_u64().unwrap_or(0) > 1,
        "the derived tier is at least DEFAULT_TREE_BUDGET's first round: {zero}"
    );

    let explicit = ask(
        &driver,
        &handle,
        "scan_keys",
        json!({
            "dbIndex": 0,
            "pattern": "tester-absent-prefix-*",
            "count": 1,
            "budget": 1
        }),
    )
    .await;
    assert_eq!(
        explicit["consumed"].as_u64(),
        Some(1),
        "an explicit 1-unit tier must be honoured verbatim (no scaling): {explicit}"
    );
}

/// Real-cluster proof that the per-key batches are *addressed*, not merely
/// well-typed: a page whose keys live on several shards can only complete
/// without a `CROSSSLOT` if every batch names one key's own slot. The
/// connection double implements the same trait, so it can never show this.
#[tokio::test]
#[ignore = "requires a live Redis Cluster (DATAZEN_TEST_REDIS_CLUSTER_URL); see ## 留待 R 回归 9a"]
async fn test_tester_cluster_page_never_crosses_slots() {
    let url = std::env::var("DATAZEN_TEST_REDIS_CLUSTER_URL")
        .expect("set DATAZEN_TEST_REDIS_CLUSTER_URL");
    let mut options = serde_json::Map::new();
    options.insert("topology".into(), json!("cluster"));
    options.insert("clusterNodes".into(), json!([url]));
    let config = ConnectionConfig {
        options: Some(options),
        ..standalone_config(&url)
    };
    let driver: Arc<dyn DatabaseDriver> = Arc::new(RedisDriver::new());
    let handle = driver
        .connect(&config)
        .await
        .expect("live cluster seed must accept the connection");

    let payload = ask(
        &driver,
        &handle,
        "scan_keys",
        json!({ "dbIndex": 0, "pattern": "*", "count": 50 }),
    )
    .await;
    assert!(
        payload.get("keys").is_some(),
        "a cluster page must answer, not fail: {payload}"
    );
    let probe = ask(
        &driver,
        &handle,
        "key_probe",
        json!({ "dbIndex": 0, "key": "datazen:tester:absent-on-cluster-9f2c" }),
    )
    .await;
    assert_eq!(
        probe["exists"],
        json!(false),
        "probe reached its shard: {probe}"
    );
}
