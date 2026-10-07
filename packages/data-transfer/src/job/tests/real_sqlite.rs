//! 真引擎夹具：`GatedSqlite`（真 `SqliteDriver` 的直通装饰器）+ 真 SQLite 文件。
//!
//! 由 `job/tests/kernel_cancel.rs` 的两条 round-trip 用例驱动；它们是
//! `data-transfer` 侧唯一让真 SQL、真事务、真回滚参与内核取消验收的地方。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use datazen_driver_api::*;
use datazen_driver_sqlite::SqliteDriver;

use super::kernel_cancel::freeze_body;
use super::*;

// ===========================================================================
// ① 真驱动 round-trip
//
// 上面两条用例里跑的是 `tests.rs` 的 `FakeDb`：它**从不解析 SQL**，引号写死 `"`、
// 占位符写死 `?`，commit / rollback 是往 `Vec` 里 push。真引擎能抓住 fake 抓不住的
// 四类东西：
//
//   (a) fake 收下、真 SQLite 拒绝的 SQL；
//   (b) fake 的事务语义与真原子回滚分家；
//   (c) `max_bound_parameters` 参与的分批算错；
//   (d) 引号 / 占位符方言漂移。
//
// 所以这一节把整条链路换成真 `SqliteDriver` + 真文件。落地形态（结论，不是顺嘴）：
//
//   **真实数据库服务端：不需要。** SQLite 是内嵌引擎，进程内打开一个文件就是
//   一个完整数据库；仓库里已经有先例
//   `packages/drivers/sqlite/tests/migration_transfer_journey.rs`——真驱动打真文件、
//   普通 `#[tokio::test]`、不读环境变量、不连网络、不起容器。造一个 live PG/MySQL
//   环境需要 .env 凭据 + 外部服务，仓库没有这个设施，停下来报告才是正确答案，
//   而不是因为"跑不起来"就退回假驱动然后宣称完成。
//
//   **真实驱动 crate 进程内联：这就是本节的形态。** 它能抓到上面四类里的
//   (a)(b)(c)(d) 全部四类——因为执行的确实是 SQLite。
//
// 但有一个硬事实必须先讲清楚，见 `GatedSqlite` 的两处"偏离"：产品今天**不允许**
// 任何非 postgres/mysql 驱动走有界分块数据管道，因此本夹具必须在两处说谎才能走完
// 数据阶段。这两处说谎的具体内容、为什么无害、以及驱动补齐能力后该删哪几行，都写
// 在代码里，也记在 `progress.md`。
// ===========================================================================

/// 拦第一批**目标**写入，位置与 `tests.rs` 的 `SlowGate` 完全一致：
/// `execute_with_params` 的第一行，也就是真引擎还没写下第一行的时候。
pub struct SqliteGate {
    pub entered: tokio::sync::Notify,
    pub opened: tokio::sync::Notify,
    first: AtomicBool,
}

impl SqliteGate {
    pub fn new() -> Self {
        Self {
            entered: tokio::sync::Notify::new(),
            opened: tokio::sync::Notify::new(),
            first: AtomicBool::new(false),
        }
    }

    /// 只拦第一次。后续批次放行，模拟"第一批写完才发现有人点了取消"。
    pub async fn hold(&self) {
        if self.first.swap(true, Ordering::SeqCst) {
            return;
        }
        self.entered.notify_one();
        self.opened.notified().await;
    }
}

/// 单文件 SQLite 的连接配置。形状照抄
/// `packages/drivers/sqlite/tests/migration_transfer_journey.rs`。
/// `role` 进 id，保证源、目标拿到**两个不同的会话句柄**——`pipeline.rs` 在
/// `source_handle.id == target_handle.id` 时直接判 "in-table resume requires
/// separate source and target database sessions"。
pub fn sqlite_config(path: &Path, role: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: format!("kernel-cancel-{role}"),
        name: format!("kernel cancel {role}"),
        database_type: "sqlite".into(),
        host: None,
        port: None,
        database: Some(path.to_string_lossy().into_owned()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 1,
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

/// 真 `SqliteDriver` 的直通装饰器：**三十个方法里有二十八个是逐字转发**，
/// 转发给的就是真的那个驱动；只有两个是本夹具自己实现的，都写在下面。
pub struct GatedSqlite {
    pub inner: SqliteDriver,
    /// `None` = 源端与引导 DDL；`Some` = 目标端第一批写入前关闸。
    pub gate: Option<Arc<SqliteGate>>,
}

#[async_trait]
impl DatabaseDriver for GatedSqlite {
    // ---- 下面两个是本夹具**故意**偏离真驱动的地方 ----

    /// **偏离 1 / 2：身份标签。**
    ///
    /// `resume::supports_chunk_driver` 硬写死 `postgresql | postgres | mysql`，
    /// `pipeline.rs` 用它决定"这个驱动有没有经过核验的有界分块事务契约"。
    /// 这里报 postgresql **只为过这一关**：真正生成 SQL 的是下面转发的真
    /// `SqliteDriver` 方法——引号、占位符、事务、参数绑定全是 SQLite 的，
    /// 执行它们的也是 SQLite 自己。说谎的只有这一个字符串。
    ///
    /// `SqliteDriver` 真的拿到快照契约之后，删掉这个方法即可回到真实身份。
    fn driver_type(&self) -> DatabaseType {
        "postgresql".into()
    }

    /// **偏离 2 / 2：稳定读快照。**
    ///
    /// trait 默认实现是
    /// `Err(DriverError::Unsupported("stable read snapshots are not supported by this driver"))`，
    /// `SqliteDriver` 没有覆写它；`pipeline.rs` 在读任何一行之前第一件事就是
    /// `begin_read_snapshot`，所以不补这一格，管道在拿到数据之前就返回
    /// "source driver could not open a stable read snapshot"。
    ///
    /// SQLite 的稳定读快照就是一个 BEGIN，这里转发到驱动自己的
    /// `begin_transaction`——它执行的是真的 `BEGIN` 语句。`connect` 建的池是
    /// `max_connections(1)`，BEGIN / 读 / COMMIT / ROLLBACK 必然落在同一条连接上，
    /// 语义成立。
    ///
    /// 这一格是**产品还缺的能力**，不是夹具多出来的花样：`SqliteDriver` 补上真
    /// `begin_read_snapshot` 之后，删掉这个方法即可。
    async fn begin_read_snapshot(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.inner.begin_transaction(handle).await
    }

    // ---- 以下二十八个逐字转发给真 SqliteDriver ----

    fn max_bound_parameters(&self) -> usize {
        self.inner.max_bound_parameters()
    }
    fn ddl_atomicity(&self) -> DdlAtomicity {
        self.inner.ddl_atomicity()
    }
    fn sync_family(&self) -> String {
        self.inner.sync_family()
    }
    fn dialect_notes(&self) -> Option<String> {
        self.inner.dialect_notes()
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
    async fn execute(&self, handle: &ConnectionHandle, sql: &str) -> Result<u64, DriverError> {
        self.inner.execute(handle, sql).await
    }
    async fn execute_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<u64, DriverError> {
        if let Some(gate) = &self.gate {
            gate.hold().await;
        }
        self.inner.execute_with_params(handle, sql, params).await
    }
    async fn cancel_query(&self, handle: &ConnectionHandle) -> Result<(), DriverError> {
        self.inner.cancel_query(handle).await
    }

    async fn query(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<QueryResult, DriverError> {
        self.inner.query(handle, sql).await
    }
    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.inner.query_with_params(handle, sql, params).await
    }
    async fn query_multi(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        self.inner.query_multi(handle, sql, limit).await
    }
    async fn query_stream(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        limit: Option<u32>,
        on_event: QueryStreamCallback,
    ) -> Result<(), DriverError> {
        self.inner.query_stream(handle, sql, limit, on_event).await
    }
    async fn explain(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
    ) -> Result<ExplainResult, DriverError> {
        self.inner.explain(handle, sql).await
    }

    async fn begin_transaction(
        &self,
        handle: &ConnectionHandle,
    ) -> Result<TransactionHandle, DriverError> {
        self.inner.begin_transaction(handle).await
    }
    async fn commit(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.commit(tx).await
    }
    async fn rollback(&self, tx: TransactionHandle) -> Result<(), DriverError> {
        self.inner.rollback(tx).await
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

    fn parameter_placeholder(
        &self,
        index: usize,
        data_type: Option<&str>,
    ) -> Result<String, DriverError> {
        self.inner.parameter_placeholder(index, data_type)
    }
    fn qualify_sql_target(
        &self,
        sql: &str,
        database: Option<&str>,
        schema: Option<&str>,
    ) -> Option<String> {
        self.inner.qualify_sql_target(sql, database, schema)
    }
    async fn physical_database_identity(
        &self,
        handle: &ConnectionHandle,
        database: &str,
    ) -> Result<Option<String>, DriverError> {
        self.inner
            .physical_database_identity(handle, database)
            .await
    }
    fn type_normalizer(&self) -> Option<Arc<dyn TypeNormalizer>> {
        self.inner.type_normalizer()
    }
    fn migration_capabilities(&self) -> Option<Arc<dyn MigrationCapabilities>> {
        self.inner.migration_capabilities()
    }
    fn migration_renderer(&self) -> Option<Arc<dyn MigrationRenderer>> {
        self.inner.migration_renderer()
    }
    async fn validate_schema_migration(
        &self,
        handle: &ConnectionHandle,
        target: SqlTarget<'_>,
    ) -> Result<(), DriverError> {
        self.inner.validate_schema_migration(handle, target).await
    }
    async fn validate_schema_migration_plan(
        &self,
        handle: &ConnectionHandle,
        target: SqlTarget<'_>,
        expected_target_schemas: &[TableSchema],
    ) -> Result<(), DriverError> {
        self.inner
            .validate_schema_migration_plan(handle, target, expected_target_schemas)
            .await
    }
}

/// 真引擎夹具：两个真文件、两个真驱动实例、一条真 handler。
pub struct RealSqlite {
    /// `Some` = 这条用例要走取消路径，第一批写入前会真停住等信号。
    /// `None` = 对照组，闸门根本不存在——否则闸门会永远等一个没人发的
    /// `opened`，用例挂在 60 秒超时上，而不是挂在断言上。
    pub gate: Option<Arc<SqliteGate>>,
    pub handler: DataTransferHandler,
    pub source_path: PathBuf,
    pub target_path: PathBuf,
    /// 目录必须在夹具活着期间留着，掉了文件就跟着没了。用 `Arc` 是为了让
    /// `run_through_kernel` 把它一起带回来——回读断言必须发生在临时目录
    /// 仍然存在的时候，否则 `count_rows` 会当场新建一个空库并数出 0 行。
    pub dir: Arc<tempfile::TempDir>,
}

/// 建真库：源文件 6 行，目标文件空表。
/// `create_new: false` ⇒ 管道不生成 DDL，目标表必须先在。
pub async fn real_sqlite(arm_gate: bool) -> RealSqlite {
    let dir = tempfile::tempdir().expect("sqlite temp dir");
    let source_path = dir.path().join("source.sqlite3");
    let target_path = dir.path().join("target.sqlite3");
    // `SqliteConnectOptions::new()` 的 `create_if_missing` 默认是 **false**：
    // sqlite 驱动不会自己建库，文件不存在就是 code 14。先建空文件，与
    // `packages/drivers/sqlite/tests/migration_transfer_journey.rs` 同一写法。
    std::fs::File::create(&source_path).expect("create empty source file");
    std::fs::File::create(&target_path).expect("create empty target file");

    let source = Arc::new(GatedSqlite {
        inner: SqliteDriver::new(),
        gate: None,
    });
    let gate = arm_gate.then(|| Arc::new(SqliteGate::new()));
    let target = Arc::new(GatedSqlite {
        inner: SqliteDriver::new(),
        gate: gate.clone(),
    });

    let ddl = "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT NOT NULL)";
    let source_handle = source
        .connect(&sqlite_config(&source_path, "source"))
        .await
        .expect("connect source");
    source
        .execute(&source_handle, ddl)
        .await
        .expect("create source table");
    for id in 1..=6i64 {
        source
            .execute_with_params(
                &source_handle,
                "INSERT INTO t (id, name) VALUES (?, ?)",
                &[Value::Integer(id), Value::String(format!("v{id}"))],
            )
            .await
            .expect("insert source row");
    }

    let target_handle = target
        .connect(&sqlite_config(&target_path, "target"))
        .await
        .expect("connect target");
    target
        .execute(&target_handle, ddl)
        .await
        .expect("create target table");

    let schema = schema_with_snapshot(&["id", "name"]);
    let handler = DataTransferHandler::apply(
        freeze_body(),
        vec![inspected_for(&["id", "name"])],
        HashMap::from([("t".to_string(), schema.clone())]),
        HashMap::from([("t".to_string(), schema)]),
        TransferEndpoints::Database {
            source_driver: source.clone(),
            source_handle,
            target_driver: target.clone(),
            target_handle,
            source_type: "postgresql".into(),
            target_type: "postgresql".into(),
        },
        None,
    );

    RealSqlite {
        gate,
        handler,
        source_path,
        target_path,
        dir: Arc::new(dir),
    }
}

/// 用**一个全新的、没有任何装饰的** `SqliteDriver` 数行数。
/// 不复用夹具里的实例，否则"回滚"这句话会被夹具自己的状态污染掉。
pub async fn count_rows(path: &Path) -> i64 {
    let driver = SqliteDriver::new();
    let handle = driver
        .connect(&sqlite_config(path, "verify"))
        .await
        .expect("connect for verification");
    let result = driver
        .query(&handle, "SELECT COUNT(*) FROM t")
        .await
        .expect("count rows");
    driver.disconnect(handle).await.ok();
    match result
        .rows
        .first()
        .and_then(|r| r.first())
        .and_then(|c| c.clone())
    {
        Some(Value::Integer(n)) => n,
        other => panic!("COUNT(*) came back as {other:?}, not an integer"),
    }
}
/// 真引擎的闸门等待上限，与 `kernel_cancel.rs` 的假驱动闸门同一量纲。
const GATE_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// 等真引擎真的停在第一批写入里。阻塞点同样在 `execute_with_params` 的第一行。
pub async fn await_gated_sqlite_write(gate: &SqliteGate) {
    assert!(
        tokio::time::timeout(GATE_WAIT, gate.entered.notified())
            .await
            .is_ok(),
        "the real data stage never reached the gated write"
    );
}
