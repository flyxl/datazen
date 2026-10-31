//! 唯一 composition root（team-server §2.3）。
//!
//! 组装顺序是设计裁决（§2.3 的 A → N 步）：
//!
//! ```text
//! A 解析子命令（main.rs）→ B logging::init → C config::load 校验
//! → D AdminStore 打开 + schema 版本检查（WR-D2 第一切片插入）
//! → M http::router 装配中间件链 → N 监听、暴露 /healthz /readyz 与 /api/v1
//! ```
//!
//! D1 只走到真实存在的步；`migrate` / `check-schema` 子命令随 WR-D2 第一
//! 切片落地（真实迁移执行器 + 只读版本报告）。E–L 步（SecretProvider、
//! DriverRegistry、BudgetCoordinator、SessionDirectory、ExecutionGateway、
//! ArtifactRepository、PolicyService）是 WR-D4 的交付物；它们落地时
//! **插入到本函数的对应位置**，而不是另起组装点。任何一步失败即拒绝
//! 启动（fail-closed），错误可诊断但不泄密。

use std::process::ExitCode;

use crate::config::ServerConfig;
use crate::error::ServerError;
use crate::http;
use crate::logging;
use crate::storage::AdminStore;

/// `migrate` 子命令的进程入口（WR-D2 第一切片）：打开管理库、执行全部
/// pending 的 expand 迁移、逐版本打印。空跑（无 pending）也是成功——
/// 幂等是执行器的承诺（`storage::tests` 钉住）。
pub fn migrate() -> ExitCode {
    let config = match ServerConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("datazen-server: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = logging::init(&config.log_filter) {
        eprintln!("datazen-server: {error}");
        return ExitCode::FAILURE;
    }

    let store = match AdminStore::open(&config.db_path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("datazen-server: {error}");
            return ExitCode::FAILURE;
        }
    };
    match store.migrate() {
        Ok(applied) => {
            if applied.is_empty() {
                println!(
                    "datazen-server: schema already at v{} (nothing to do)",
                    store
                        .schema_version()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| "?".into())
                );
            } else {
                for migration in &applied {
                    println!(
                        "datazen-server: applied v{} {} ({})",
                        migration.version, migration.name, migration.applied_at
                    );
                }
                println!(
                    "datazen-server: schema now at v{}",
                    store
                        .schema_version()
                        .map(|v| v.to_string())
                        .unwrap_or_else(|_| "?".into())
                );
            }
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("datazen-server: {error}");
            ExitCode::FAILURE
        }
    }
}

/// `check-schema` 子命令的进程入口：只读报告。落后 → 退出码 3（提示跑
/// `migrate`）；超前（库版本高于本二进制已知版本）→ 退出码 4，fail-closed：
/// 未知 schema 不得被旧二进制触碰。
pub fn check_schema() -> ExitCode {
    let config = match ServerConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("datazen-server: {error}");
            return ExitCode::FAILURE;
        }
    };

    let store = match AdminStore::open(&config.db_path) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("datazen-server: {error}");
            return ExitCode::FAILURE;
        }
    };
    let current = match store.schema_version() {
        Ok(current) => current,
        Err(error) => {
            eprintln!("datazen-server: {error}");
            return ExitCode::FAILURE;
        }
    };
    let target = crate::storage::migrations::MIGRATIONS
        .last()
        .map(|migration| migration.version)
        .unwrap_or(0);
    match current.cmp(&target) {
        std::cmp::Ordering::Equal => {
            println!("datazen-server: schema at v{current} (up to date)");
            ExitCode::SUCCESS
        }
        std::cmp::Ordering::Less => {
            println!(
                "datazen-server: schema at v{current}, target v{target} — run `datazen-server migrate`"
            );
            ExitCode::from(3)
        }
        std::cmp::Ordering::Greater => {
            eprintln!(
                "datazen-server: schema at v{current} is newer than this binary knows (v{target}); \
                 refusing to touch an unknown schema — upgrade the binary first"
            );
            ExitCode::from(4)
        }
    }
}

/// `serve` 子命令的进程入口（main.rs 调用）。
///
/// 同步外壳：加载配置、起日志，然后一次性 tokio runtime 跑服务循环。
/// 任一步失败即返回非零退出码——fail-closed。
pub fn serve() -> ExitCode {
    let config = match ServerConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("datazen-server: {error}");
            return ExitCode::FAILURE;
        }
    };

    if let Err(error) = logging::init(&config.log_filter) {
        eprintln!("datazen-server: {error}");
        return ExitCode::FAILURE;
    }

    let runtime = match tokio::runtime::Builder::new_multi_thread().enable_all().build() {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("datazen-server: runtime build failed: {error}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run(config)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            tracing::error!(%error, "server exited with failure");
            eprintln!("datazen-server: {error}");
            ExitCode::FAILURE
        }
    }
}

/// 服务循环：装配管理库（§2.3 的 D 步，WR-D2 第一切片插入）、Router、
/// 绑定监听、优雅停机。
async fn run(config: ServerConfig) -> Result<(), ServerError> {
    // D 步：管理库。serve 不自动迁移（migrate 是独立子命令）；schema 落后
    // 或超前都拒绝服务——fail-closed，不静默降级。
    let store = AdminStore::open(&config.db_path)
        .map_err(|source| ServerError::Serve(format!("admin store: {source}")))?;
    let current = store
        .schema_version()
        .map_err(|source| ServerError::Serve(format!("admin store: {source}")))?;
    let target = crate::storage::migrations::MIGRATIONS
        .last()
        .map(|migration| migration.version)
        .unwrap_or(0);
    if current != target {
        return Err(ServerError::Serve(format!(
            "admin schema at v{current}, this build expects v{target} — \
             run `datazen-server migrate` (or upgrade the binary)"
        )));
    }
    // 仓储适配器在域路由点亮时注入 Router（WR-D2 后续切片）；在此之前
    // store 在此保活，证明打开与版本检查是 serve 的真实前置。
    let _keep_store = store;

    let app = http::router::build_router();

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .map_err(|source| ServerError::Listen {
            addr: config.listen_addr,
            source,
        })?;

    tracing::info!(addr = %config.listen_addr, "datazen-server listening (healthz/readyz live; /api/v1 domains arrive with WR-D2/D4+)");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|error| ServerError::Serve(error.to_string()))?;
    Ok(())
}

/// 停机信号：Ctrl-C（SIGINT）。容器场景的 SIGTERM 接线随部署切片（WR-G）。
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received; draining");
}
