//! 唯一 composition root（team-server §2.3）。
//!
//! 组装顺序是设计裁决（§2.3 的 A → N 步），D1 只走到真实存在的步：
//!
//! ```text
//! A 解析子命令（main.rs）→ B logging::init → C config::load 校验
//! → M http::router 装配中间件链 → N 监听、暴露 /healthz /readyz 与 /api/v1
//! ```
//!
//! D–L 步（管理库、SecretProvider、DriverRegistry、BudgetCoordinator、
//! SessionDirectory、ExecutionGateway、ArtifactRepository、PolicyService）
//! 是 WR-D2/D4 的交付物；它们落地时**插入到本函数的对应位置**，而不是
//! 另起组装点。任何一步失败即拒绝启动（fail-closed），错误可诊断但不
//! 泄密。

use std::process::ExitCode;

use crate::config::ServerConfig;
use crate::error::ServerError;
use crate::http;
use crate::logging;

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

/// 服务循环：装配 Router、绑定监听、优雅停机。
async fn run(config: ServerConfig) -> Result<(), ServerError> {
    let app = http::router::build_router();

    let listener = tokio::net::TcpListener::bind(&config.listen_addr)
        .await
        .map_err(|source| ServerError::Listen {
            addr: config.listen_addr,
            source,
        })?;

    tracing::info!(addr = %config.listen_addr, "datazen-server listening (skeleton: healthz/readyz live, /api/v1 arrives with WR-D2+)");

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
