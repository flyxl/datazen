//! The gate shared by every migration test.
//!
//! Nine migration tests across PostgreSQL and MySQL sat behind a static
//! `#[ignore]` for their whole life. That attribute is neither a pass nor a
//! failure — cargo counts it under `ignored` and exits 0 — so each of them was
//! reportable as green while never having run. Worse, deleting the attribute
//! naively would have converted a quiet skip into a CI panic, because every one
//! of them opens with `std::env::var("MIGRATION_TEST_DATABASE").expect(...)`.
//!
//! This module is the way out of that bind: read the configuration from the
//! **process environment only** (no `.env` file is opened, parsed, or sourced),
//! and when it is absent *report the dimension unverified* — a skip by default,
//! a failure under `DATAZEN_CONTRACT_REQUIRE_LIVE=1`. Same tests, same
//! assertions; the difference is that "green" can now be made to mean
//! "verified".
//!
//! Credentials are never interpolated into a message. A missing key is named, a
//! present one is not — so a stray print of a failure reason cannot leak a
//! password into a CI log.

use datazen_driver_api::ConnectionConfig;

#[path = "live_gate.rs"]
mod live_gate;

/// Reports one migration dimension as unverified; see [`live_gate`] for why.
///
/// `dead_code` is allowed because this file is compiled into each migration
/// test binary separately, and not every suite has a connect-failure path to
/// report. A `#[deny(warnings)]` crate must not turn that into eight copies of
/// the same warning.
#[allow(dead_code)]
pub fn unverified(label: &str, dimension: &str, reason: &str) {
    live_gate::unverified_or_fail(label, dimension, reason, live_gate::strict_live());
}

/// The env prefix a given family reads.
///
/// Both families ran under the same `MIGRATION_TEST_*` names, which cannot
/// work now that they are no longer `#[ignore]`d: one `cargo test -p pg -p
/// mysql` puts every test binary in one environment, so a single set of names
/// would point the MySQL suites at the PostgreSQL fixture. PostgreSQL keeps the
/// original prefix so an existing `MIGRATION_TEST_*` block still works; MySQL
/// gets its own.
fn env_prefix(database_type: &str) -> &'static str {
    match database_type {
        "mysql" | "mariadb" => "MYSQL_MIGRATION_TEST_",
        _ => "MIGRATION_TEST_",
    }
}

/// Reads the migration fixture configuration from the process environment.
///
/// Returns `None` after reporting the dimension unverified, which is the normal
/// path on a machine with no fixtures: the caller's `return` then skips. Under
/// strict mode the report has already panicked, so that `return` is
/// unreachable — the dimension is a failure, not a skip.
///
/// `also_allowed` lists **extra exact names** a given suite has always accepted
/// beyond the `dz_mig_` prefix (`datazen_sync_src`, `datazen_e2e`,
/// `datazen_test`). They are per-suite on purpose: these tests create and drop
/// schemas, so widening the refusal uniformly would either delete a real gate or
/// open one somewhere it was never granted. `dz_mig_` itself is enforced here
/// for everyone.
///
/// A non-fixture database is a hard failure, never a skip. "Unverified" is the
/// right word for a missing server; it is the wrong word for a guard that has
/// just caught a configuration that would destroy a developer's real data.
pub fn require_config(
    label: &str,
    dimension: &str,
    database_type: &str,
    id_prefix: &str,
    also_allowed: &[&str],
) -> Option<ConnectionConfig> {
    let strict = live_gate::strict_live();
    let pre = env_prefix(database_type);
    let database_key = format!("{pre}DATABASE");

    let Some(database) = env_var(&database_key) else {
        live_gate::unverified_or_fail(
            label,
            dimension,
            &format!("{database_key} is not in the process environment"),
            strict,
        );
        return None;
    };

    assert!(
        database.starts_with("dz_mig_") || also_allowed.contains(&database.as_str()),
        "refuse to run migration tests against a database that is not a disposable fixture: \
         {database_key} must start with `dz_mig_` (or be one of {also_allowed:?}). \
         These tests create and drop schemas, so a working copy would lose real data."
    );

    // Named one at a time on purpose: "credentials are missing" is unactionable,
    // "MIGRATION_TEST_HOST is missing" is a one-line fix.
    let missing = |key: &str| {
        live_gate::unverified_or_fail(
            label,
            dimension,
            &format!("{key} is not in the process environment"),
            strict,
        );
    };

    let host_key = format!("{pre}HOST");
    let Some(host) = env_var(&host_key) else {
        missing(&host_key);
        return None;
    };
    let user_key = format!("{pre}USER");
    let Some(user) = env_var(&user_key) else {
        missing(&user_key);
        return None;
    };

    let port_key = format!("{pre}PORT");
    let port = match env_var(&port_key) {
        Some(raw) => match raw.parse::<u16>() {
            Ok(port) => port,
            Err(_) => {
                live_gate::unverified_or_fail(
                    label,
                    dimension,
                    &format!("{port_key} is not a valid port number"),
                    strict,
                );
                return None;
            }
        },
        None => match database_type {
            "mysql" | "mariadb" => 3306,
            _ => 5432,
        },
    };

    Some(ConnectionConfig {
        id: format!("{id_prefix}-{}", unique_token()),
        name: "migration test".into(),
        database_type: database_type.into(),
        host: Some(host),
        port: Some(port),
        database: Some(database),
        schema: None,
        username: Some(user),
        // Absent, not empty, is the signal that this suite needs no password.
        // Reading it without an emptiness filter preserves an intentionally
        // blank credential instead of silently substituting a default.
        password: Some(env_var(&format!("{pre}PASSWORD")).unwrap_or_default()),
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 3,
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
    })
}

/// Connection ids must be distinct per run: they key the driver's connection
/// registry, so a reused id can hand one test a handle another test is already
/// closing. pid plus nanosecond nonce needs no dependency and is enough — two
/// runs of one suite within the same nanosecond are not a real scenario, and
/// pid alone collides across processes.
fn unique_token() -> String {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("{}_{}", std::process::id(), nonce)
}

/// Reads one variable, treating an empty value as absent.
///
/// An empty `MIGRATION_TEST_DATABASE=` is a half-filled config block, not a
/// request to use the database named "". Treating it as absent keeps the
/// dimension reportable as unverified instead of failing to connect to nothing.
fn env_var(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}
