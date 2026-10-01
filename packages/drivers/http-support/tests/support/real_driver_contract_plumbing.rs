//! Shared plumbing for the real-driver contract template: the only credential
//! source (the process environment), fixture bookkeeping, and the live-session
//! handle. Split out of `real_driver_contract.rs` to keep every file well under the
//! 800-line ceiling in AGENTS.md; it is `#[path]`-included, never compiled alone.

#![allow(dead_code, unused_imports)]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use datazen_driver_api::{
    ConnectionConfig, ConnectionHandle, DatabaseDriver, DriverError, QueryResult, SqlTarget,
    SslMode, StatementResult, Value,
};

use super::Dialect;

/// The only fixture prefix `fake-runtime-fixtures.md` §10.2 rule 1 accepts.
/// Live targets that are not prefixed are refused — fail closed, no escape hatch.
pub const FIXTURE_PREFIX: &str = "dz_fixture_";
/// Every dimension this contract must cover, with the `fn cmNN_` test that owns it.
///
/// `contract_matrix_covers_every_required_dimension` and
/// `every_required_dimension_owns_a_named_test` both read this table, so a dimension
/// cannot quietly drop out of the coverage.
pub const REQUIRED_DIMENSIONS: &[(&str, &str, &str)] = &[
    (
        "CM-08",
        "cm08_two_sessions_keep_independent_databases",
        "两个编辑器会话各自锁定自己的 database",
    ),
    (
        "CM-09",
        "cm09_session_scoped_state_stays_in_its_own_session",
        "会话级临时状态不可跨会话可见",
    ),
    (
        "CM-10",
        "cm10_script_order_is_preserved_and_final_target_is_last",
        "原始脚本顺序与最终上下文",
    ),
    (
        "CM-13",
        "cm13_cross_target_resolution_reads_the_requested_target",
        "限定名解析命中被请求的目标",
    ),
    (
        "CM-14",
        "cm14_text_containing_the_switch_keyword_never_switches",
        "字符串/注释里的切换关键字不触发切库",
    ),
    (
        "CM-17",
        "cm17_hand_written_transaction_rolls_back_cleanly",
        "手写事务旅程与回滚",
    ),
    (
        "CM-18",
        "cm18_failed_statement_leaves_the_next_operation_working",
        "失败语句后的可恢复性",
    ),
    (
        "CM-22",
        "cm22_precise_cancel_is_addressed_to_one_execution",
        "精确取消只作用于目标执行",
    ),
    (
        "CM-24",
        "cm24_cancel_never_falls_back_to_session_wide",
        "无精确取消时必须明确拒绝",
    ),
    (
        "CM-26",
        "cm26_failed_statement_resource_is_not_reused_blindly",
        "失败后的资源不复用",
    ),
    (
        "CM-48",
        "cm48_read_snapshots_are_shared_or_explicitly_refused",
        "快照共享或明确拒绝",
    ),
    (
        "CM-69",
        "cm69_closed_database_stops_being_reported_open",
        "关闭后的数据库不再报告为已打开",
    ),
];
// ---------------------------------------------------------------------------
// Process-environment helpers (the only credential source, §10.2 rule 5)
// ---------------------------------------------------------------------------

pub fn optional_env(key: String) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.trim().is_empty())
}

pub fn required_env(key: String) -> Result<String, String> {
    optional_env(key.clone()).ok_or_else(|| format!("`{key}` is missing or empty"))
}

pub fn raw_env(key: String) -> String {
    // A password may legitimately be empty, so it is read without the filter.
    std::env::var(key).unwrap_or_default()
}

pub struct Profile {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// Target A of the A/B pair (§10.2 rule 2).
    pub a: String,
    /// Target B — a *different* database holding a *different* marker.
    pub b: String,
}

pub fn profile() -> Result<Profile, String> {
    let contract = &crate::CONTRACT;
    let prefix = contract.env_prefix;
    let a = required_env(format!("{prefix}DATABASE"))?;
    Ok(Profile {
        host: optional_env(format!("{prefix}HOST")).unwrap_or_else(|| "127.0.0.1".into()),
        port: optional_env(format!("{prefix}PORT"))
            .and_then(|v| v.parse().ok())
            .unwrap_or(contract.default_port),
        user: optional_env(format!("{prefix}USER"))
            .unwrap_or_else(|| contract.default_user.into()),
        password: raw_env(format!("{prefix}PASSWORD")),
        b: optional_env(format!("{prefix}DATABASE_B")).unwrap_or_else(|| a.clone()),
        a,
    })
}

pub fn connection_config(profile: &Profile, database: &str, id: &str) -> ConnectionConfig {
    let contract = &crate::CONTRACT;
    ConnectionConfig {
        id: id.to_string(),
        name: format!("dz contract {database}"),
        database_type: contract.label.to_string(),
        host: Some(profile.host.clone()),
        port: Some(profile.port),
        database: Some(database.to_string()),
        schema: contract.default_schema.map(str::to_string),
        username: Some(profile.user.clone()),
        password: Some(profile.password.clone()),
        ssl_mode: SslMode::Prefer,
        connection_timeout: 5,
        max_pool_size: 5,
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

/// Never print a driver error verbatim: a server message can echo connection
/// details. Tests print the variant only.
pub fn err_kind(error: &DriverError) -> &'static str {
    match error {
        DriverError::ConnectionFailed(_) => "ConnectionFailed",
        DriverError::ConnectionTimeout => "ConnectionTimeout",
        DriverError::AuthenticationFailed(_) => "AuthenticationFailed",
        DriverError::QueryFailed(_) => "QueryFailed",
        DriverError::InvalidConfig(_) => "InvalidConfig",
        DriverError::TransactionError(_) => "TransactionError",
        DriverError::Unsupported(_) => "Unsupported",
        DriverError::NotSupported(_) => "NotSupported",
        DriverError::PoolExhausted => "PoolExhausted",
        DriverError::QueryExecutionNotFound(_) => "QueryExecutionNotFound",
        _ => "Other",
    }
}

pub fn row_text(cell: &Option<Value>) -> String {
    match cell {
        Some(Value::String(s)) | Some(Value::Timestamp(s)) => s.clone(),
        Some(Value::Integer(i)) => i.to_string(),
        Some(Value::Float(f)) => f.to_string(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Json(v)) => v.to_string(),
        Some(Value::Bytes(b)) => format!("{} bytes", b.len()),
        Some(Value::Null) | None => String::new(),
    }
}

pub fn first_cell(result: &QueryResult) -> String {
    match result.rows.first().and_then(|row| row.first()) {
        Some(cell) => row_text(cell),
        None => String::new(),
    }
}

pub fn rows_contain(result: &QueryResult, needle: &str) -> bool {
    result.rows.iter().flatten().any(|cell| row_text(cell).contains(needle))
}

pub fn statement_rows_contain(result: &StatementResult, needle: &str) -> bool {
    result.rows.iter().flatten().any(|cell| row_text(cell).contains(needle))
}

/// The first cell of the first row of one statement of a raw script.
pub fn statement_first_cell(result: &StatementResult) -> String {
    match result.rows.first().and_then(|row| row.first()) {
        Some(cell) => row_text(cell),
        None => String::new(),
    }
}

pub fn render(template: &str, name: &str) -> String {
    template.replace("{name}", name)
}

/// The CM-13 target shape: the explicitly requested database plus this driver's
/// own default schema. Blank/absent parts stay absent (`SqlTarget::nonblank`).
pub fn target_for(database: &str) -> SqlTarget<'_> {
    SqlTarget::new(Some(database), crate::CONTRACT.default_schema)
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Accumulates teardown SQL and runs it **after** the assertions, whether they
/// passed or failed. Teardown failure fails the test (§10.2 rule 4).
pub struct Fixture {
    pub tag: String,
    pub drops: Vec<String>,
}

impl Fixture {
    pub fn new(tag: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        Self {
            tag: format!("{tag}_{}_{stamp}", std::process::id()),
            drops: Vec::new(),
        }
    }

    /// A per-case unique object name carrying the dedicated prefix (§10.2 rule 3).
    pub fn name(&self, suffix: &str) -> String {
        format!("{FIXTURE_PREFIX}{}_{suffix}", self.tag)
    }

    pub fn on_drop(&mut self, sql: String) {
        self.drops.push(sql);
    }

    pub async fn finish<D: DatabaseDriver + ?Sized>(
        &mut self,
        driver: &D,
        handle: &ConnectionHandle,
    ) -> Result<(), DriverError> {
        // Reverse order: nested fixtures unwind before their parents.
        for sql in std::mem::take(&mut self.drops).into_iter().rev() {
            // Propagated: teardown must never silently pass.
            driver.execute(handle, &sql).await?;
        }
        Ok(())
    }
}

/// Writes the A/B marker rows demanded by `fake-runtime-fixtures.md` §10.2 rule 2:
/// identically named tables in both targets, **different** marker values.
pub async fn seed_marker<D: DatabaseDriver + ?Sized>(
    driver: &D,
    handle: &ConnectionHandle,
    dialect: &Dialect,
    marker: &str,
) -> Result<(), DriverError> {
    driver.execute(handle, dialect.create_marker).await?;
    driver
        .execute(handle, &dialect.insert_marker.replace("{marker}", marker))
        .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Live session
// ---------------------------------------------------------------------------

pub struct Live<D: DatabaseDriver> {
    pub driver: D,
    pub handle: ConnectionHandle,
    pub profile: Profile,
    pub dialect: &'static Dialect,
}

/// Prints the honest skip line: which dimension is therefore **unverified** and
/// why. No fake stands in for a real-protocol conclusion.
pub fn report_unverified(dimension: &str, reason: &str) {
    eprintln!(
        "⏭  {dimension} 未验证（{}）：{reason} — 该维度需要真实数据库，本次运行不产生任何真实协议结论。",
        crate::CONTRACT.label
    );
}

pub async fn open_live<D: DatabaseDriver>(driver: D, dimension: &'static str) -> Option<Live<D>> {
    if let Err(reason) = crate::CONTRACT.availability() {
        report_unverified(dimension, &reason);
        return None;
    }
    let profile = match profile() {
        Ok(profile) => profile,
        Err(reason) => {
            report_unverified(dimension, &reason);
            return None;
        }
    };
    let config = connection_config(&profile, &profile.b, "dz-contract-b");
    let handle = match driver.connect(&config).await {
        Ok(handle) => handle,
        Err(error) => {
            report_unverified(
                dimension,
                &format!("cannot reach the fixture target: {}", err_kind(&error)),
            );
            return None;
        }
    };
    Some(Live {
        driver,
        handle,
        profile,
        dialect: &crate::CONTRACT.dialect,
    })
}

impl<D: DatabaseDriver> Live<D> {
    pub async fn query(&self, sql: &str) -> Result<QueryResult, DriverError> {
        self.driver.query(&self.handle, sql).await
    }

    pub async fn execute(&self, sql: &str) -> Result<(), DriverError> {
        self.driver.execute(&self.handle, sql).await.map(|_| ())
    }

    pub async fn namespace(&self) -> Result<String, DriverError> {
        let result = self
            .query(&format!("SELECT {}", self.dialect.current_namespace))
            .await?;
        Ok(first_cell(&result))
    }

    /// An independent session opened on the *other* fixture target.
    pub async fn second_session(&self, id: &str) -> Result<ConnectionHandle, DriverError> {
        let config = connection_config(&self.profile, &self.profile.a, id);
        self.driver.connect(&config).await
    }
}
/// `packages/drivers`, found by name so the depth of the crate inside the
/// workspace never becomes a hidden assumption of the template.
pub fn drivers_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "drivers"))
        .expect("a driver crate must live at packages/drivers/<id>")
        .to_path_buf()
}

pub fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_rs(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// Every source file of the shared template, so a source-level guard scans the
/// whole template instead of only the file that happens to hold the test.
pub fn template_sources() -> Vec<PathBuf> {
    let dir = drivers_root().join("http-support/tests/support");
    [
        "real_driver_contract.rs",
        "real_driver_contract_plumbing.rs",
        "real_driver_contract_live.rs",
    ]
    .iter()
    .map(|name| dir.join(name))
    .collect()
}
