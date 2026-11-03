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

use datazen::shared::error::ApiError;

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

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
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
    let store = std::sync::Arc::new(
        AdminStore::open(&config.db_path)
            .map_err(|source| ServerError::Serve(format!("admin store: {source}")))?,
    );
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
    // ── 装配（§2.3）：policy / 登录会话 / L5 / SSE / OIDC —— 依赖闭包
    // 全部从 config + store 出发，不隐式构造全局单例。
    let secret_bytes = config.server_secret.as_bytes().to_vec();
    let policy = std::sync::Arc::new(
        crate::policy::PolicyService::new(store.clone(), secret_bytes.clone())
            .map_err(|source| ServerError::Serve(format!("policy: {source}")))?,
    );
    let clock: std::sync::Arc<dyn datazen::shared::clock::Clock> =
        std::sync::Arc::new(datazen::shared::clock::SystemClock);

    // ── WR-D6/D7 服务面：审计、作业仓储、提交令牌、产物库、预算协调器
    // 与 operations/transfer 两域服务。全部从 config + store 出发（§2.3）。
    let audit = std::sync::Arc::new(crate::storage::ServerAudit::new(store.clone()));
    let jobs = std::sync::Arc::new(crate::storage::ServerJobs::new(
        store.clone(),
        std::sync::Arc::new(datazen::jobs::domain::time::PortJobClock::new(
            datazen::shared::clock::SystemClock,
        )),
        900, // claim 租约首版起点（15 分钟；与内核内存实现同义）
    ));
    let issuer = std::sync::Arc::new(
        crate::execution::submission_token::ServerSubmissionTokenIssuer::new(
            secret_bytes.clone(),
            clock.clone(),
        )
        .map_err(|source| ServerError::Serve(format!("submission token issuer: {source}")))?,
    );
    let artifacts = std::sync::Arc::new(
        crate::storage::ServerArtifacts::open(store.clone(), &config.artifact_root)
            .map_err(|source| ServerError::Serve(format!("artifact store: {source}")))?,
    );
    let budget = std::sync::Arc::new(crate::execution::budget::ServerBudgetCoordinator::new(
        config.budget_node_limit,
        config.budget_organization_limit,
        datazen::shared::id::OrganizationId::new(config.organization_id.clone()),
    ));
    let quota_gate = std::sync::Arc::new(crate::operations::QuotaGate::new(
        budget,
        audit.clone() as std::sync::Arc<dyn datazen::shared::audit::AuditSink>,
    ));

    // 事件后备：单实例 v1 的 MemoryEventSink 同时承载 SSE 订阅与撤销传播
    // （WR-D6 EventSinkBridge 近似语义，见 revocation.rs 模块文档的登记）。
    let memory_sink: std::sync::Arc<dyn datazen::execution::application::EventSink> =
        std::sync::Arc::new(
            datazen::execution::application::event_sink_memory::MemoryEventSink::new(),
        );
    let revocation = std::sync::Arc::new(crate::operations::RevocationService::new(
        store.clone(),
        std::sync::Arc::new(crate::operations::revocation::EventSinkBridge::new(
            memory_sink.clone(),
            datazen::shared::id::StreamId::new("control-plane"),
            datazen::shared::id::RuntimeEpoch::new("server-v1"),
        )),
        audit.clone() as std::sync::Arc<dyn datazen::shared::audit::AuditSink>,
    ));
    let ttl_sweeper = std::sync::Arc::new(crate::operations::TtlSweeper::new(
        artifacts.clone(),
        audit.clone() as std::sync::Arc<dyn datazen::shared::audit::AuditSink>,
        datazen::shared::id::OrganizationId::new(config.organization_id.clone()),
        crate::operations::TtlSweepConfig {
            interval_seconds: config.artifact_ttl_sweep_secs,
        },
    ));
    // 后台清扫：JoinHandle 随 drain 语义（WR-G3）接管优雅停机。
    let _ttl_worker = tokio::spawn(async move { ttl_sweeper.run_forever().await });
    let _config_cas = std::sync::Arc::new(crate::operations::ProfileConfigCas::new(store.clone()));
    let _revocation = revocation; // L6/中间件接线随执行域路由点亮（check_session_permission）
    let task_center = std::sync::Arc::new(crate::operations::TaskCenter::new(jobs.clone(), 50));
    let gate = std::sync::Arc::new(QuotaPreSubmitGate {
        quota: quota_gate.clone(),
    });
    let submission = std::sync::Arc::new(crate::transfer::SubmissionService::new(
        issuer.clone(),
        jobs.clone(),
        gate,
        audit.clone(),
        clock.clone(),
    ));
    let uploads = std::sync::Arc::new(crate::transfer::UploadService::new(
        artifacts.clone(),
        audit.clone(),
        clock.clone(),
    ));
    let downloads = std::sync::Arc::new(crate::transfer::DownloadService::new(
        artifacts.clone(),
        audit.clone(),
        clock.clone(),
    ));
    let domains = http::router::DomainState {
        issuer,
        submission,
        uploads,
        downloads,
        tasks: task_center,
    };

    // WR-F2：MCP streamable HTTP（feature 门控）。认证器 = 登录会话
    // （与 L2 同源的 peek 校验面）；租约源 v1 fail-closed——执行后端随
    // WR-F7 落地，此前 runtime 依赖工具如实答上游不可用。
    #[cfg(feature = "mcp")]
    let mcp_state = {
        let authenticator = CookieSessionAuthenticator {
            sessions: crate::storage::LoginSessions::new(store.clone()),
        };
        Some(crate::mcp::runtime::build_http_state(
            std::sync::Arc::new(authenticator),
            std::sync::Arc::new(FailClosedLeaseSource), // WR-F7 真租约源待 policy 适配器（见下）
            Vec::new(), // 连接白名单：空表 = 全拒绝（安全默认；部署配置随 F7 接入）
            datazen::execution::domain::permission::McpPermissionMode::SafeWrite,
        ))
    };
    #[cfg(not(feature = "mcp"))]
    let mcp_state = None;

    // L5（§9.1）：参数来自部署配置；构造非法（容量 0 等）拒绝启动。
    let rate_limit = {
        let limiter = crate::http::middleware::ratelimit::RateLimiter::new(
            crate::http::middleware::ratelimit::RateLimitConfig {
                default: crate::http::middleware::ratelimit::BucketParams {
                    capacity: config.rate_default_capacity,
                    refill_per_second: config.rate_default_refill_per_second,
                },
                strict: crate::http::middleware::ratelimit::BucketParams {
                    capacity: config.rate_strict_capacity,
                    refill_per_second: config.rate_strict_refill_per_second,
                },
                max_entries: config.rate_max_entries,
            },
            std::sync::Arc::new(crate::http::middleware::ratelimit::SystemClock),
        )
        .map_err(|message| ServerError::ConfigInvalid(format!("ratelimit: {message}")))?;
        Some(crate::http::middleware::ratelimit::RateLimitState {
            limiter: std::sync::Arc::new(limiter),
        })
    };

    // SSE（WR-D5）：origin 白名单取 public_origin（§8.1）；事件后备是
    // MemoryEventSink（单实例 v1 形态）；周期复核走标准实现。
    let sse = config.public_origin.as_ref().map(|origin| {
        let sse_config = crate::http::sse::SseConfig {
            allowed_origins: vec![origin.clone()],
            keepalive_interval: std::time::Duration::from_secs(config.sse_keepalive_secs),
            max_backlog_frames: config.sse_max_backlog_frames,
            ..crate::http::sse::SseConfig::default()
        };
        crate::http::sse::SseState {
            registry: std::sync::Arc::new(crate::http::sse::EventStreamRegistry::new(&sse_config)),
            sink: memory_sink.clone(),
            recheck: std::sync::Arc::new(crate::http::sse::StandardRecheck::new(
                crate::storage::LoginSessions::new(store.clone()),
                policy.clone(),
                clock.clone(),
            )),
            config: std::sync::Arc::new(sse_config),
            now_unix: std::sync::Arc::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|elapsed| elapsed.as_secs())
                    .unwrap_or(0)
            }),
        }
    });

    // OIDC 登录面（§4）：三件套齐备才挂载；缺省 = 未启用（/api/v1/auth/*
    // 落到 fallback 501，部署形态显式）。
    let oidc = match (&config.oidc_issuer, &config.oidc_client_id) {
        (Some(issuer), Some(client_id)) => {
            let oidc_config = crate::auth::oidc::OidcConfig {
                issuer: issuer.clone(),
                client_id: client_id.clone(),
                client_secret: config.oidc_client_secret.as_ref().map(|value| {
                    crate::auth::oidc::ClientSecret {
                        value: value.clone(),
                        auth_style: crate::auth::oidc::ClientAuthStyle::Post,
                    }
                }),
                organization_id: config.organization_id.clone(),
                public_origin: config.public_origin.clone().unwrap_or_default(),
                callback_path: "/api/v1/auth/callback".into(),
                scope: "openid profile".into(),
                post_login_redirect: "/".into(),
                http_timeout: std::time::Duration::from_secs(5),
                discovery_ttl: std::time::Duration::from_secs(300),
                jwks_ttl: std::time::Duration::from_secs(300),
                clock_skew_leeway: std::time::Duration::from_secs(60),
                state_ttl: std::time::Duration::from_secs(600),
                pending_capacity: 1_024,
                session_idle_seconds: config.session_idle_seconds,
                session_absolute_seconds: config.session_absolute_seconds,
            };
            Some(std::sync::Arc::new(
                crate::auth::oidc::OidcRelyingParty::new(
                    oidc_config,
                    store.clone(),
                    policy.clone(),
                    crate::storage::LoginSessions::new(store.clone()),
                    std::sync::Arc::new(crate::auth::oidc::SystemClock),
                )
                .map_err(|message| ServerError::ConfigInvalid(format!("oidc: {message}")))?,
            ))
        }
        _ => None,
    };

    let app = http::router::build_router_with_auth(http::router::AuthDeps {
        store: store.clone(),
        policy,
        server_secret: secret_bytes,
        idle_ttl_seconds: config.session_idle_seconds,
        rate_limit,
        sse,
        oidc,
        domains: Some(domains),
        mcp: mcp_state,
    })?;

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

/// WR-F2：MCP HTTP 的登录会话认证器。校验面与 L2 中间件同源
/// （`peek`：撤销/绝对窗/空闲窗），互斥裁决同样只服务 Cookie 模式会话
/// （native/Bearer 凭据不经 cookie 复用，§4.6）。
#[cfg(feature = "mcp")]
struct CookieSessionAuthenticator {
    sessions: crate::storage::LoginSessions,
}

#[cfg(feature = "mcp")]
impl crate::mcp::http::SessionAuthenticator for CookieSessionAuthenticator {
    fn authenticate(
        &self,
        headers: &axum::http::HeaderMap,
    ) -> Option<crate::mcp::http::CallIdentity> {
        let session_id = crate::http::middleware::session_cookie(
            headers,
            crate::http::middleware::auth::SESSION_COOKIE,
        )?;
        let row = self
            .sessions
            .peek(
                &session_id,
                datazen::shared::id::Timestamp::new(crate::http::middleware::now_rfc3339()),
            )
            .ok()?;
        if row.mode != crate::storage::login_sessions::LoginSessionMode::Cookie {
            return None;
        }
        Some(crate::mcp::http::CallIdentity {
            organization_id: row.organization_id,
            principal_id: row.principal_id,
            request_id: crate::mcp::http::new_request_id(),
        })
    }
}

/// WR-F7 真租约源（`crate::mcp::runtime::ServerRuntimeLeaseSource::new`）
/// 待 Wave4 接线：阻塞项 = modules `execution::application::policy::
/// PolicyService` trait 适配器（server `policy::PolicyService` 未实现该
/// trait，见 `driver_exec.rs` 登记表）。v1 维持 fail-closed：runtime
/// 依赖工具如实答不可用。
#[cfg(feature = "mcp")]
struct FailClosedLeaseSource;

#[cfg(feature = "mcp")]
#[async_trait::async_trait]
impl crate::mcp::runtime::RuntimeLeaseSource for FailClosedLeaseSource {
    async fn resolve(
        &self,
    ) -> Result<std::sync::Arc<datazen::execution::application::RuntimeConnectionUseCases>, String>
    {
        Err("driver execution lease wiring lands with the WR-F7 policy adapter; \
             runtime-backed MCP tools are unavailable in this build."
            .to_string())
    }
}

/// WR-F2：runtime 租约源的 server v1 形态——执行后端（驱动/会话物理
/// 资源）随 WR-F7 落地；此前如实报错，runtime 依赖工具 fail-closed。
#[cfg(feature = "mcp")]
struct ServerRuntimeLeaseSource;

#[cfg(feature = "mcp")]
#[async_trait::async_trait]
impl crate::mcp::runtime::RuntimeLeaseSource for ServerRuntimeLeaseSource {
    async fn resolve(
        &self,
    ) -> Result<std::sync::Arc<datazen::execution::application::RuntimeConnectionUseCases>, String>
    {
        Err("driver execution backend is not assembled yet (WR-F7); \
             runtime-backed MCP tools are unavailable in this build."
            .to_string())
    }
}

/// 停机信号：Ctrl-C（SIGINT）。容器场景的 SIGTERM 接线随部署切片（WR-G）。
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown signal received; draining");
}

/// WR-D6 QuotaGate 适配为 WR-D7 的 `PreSubmitGate`（`transfer` 模块文档
/// 的接缝裁决）。闸门只读无副作用：重放路径同样穿过它，因此提交本身
/// 不占用持久额度——占位检查容量后立即归还（执行期额度随 ExecutionGateway
/// 接线收取）。拒绝用 wire 词汇原样透传，transfer 侧零翻译。
struct QuotaPreSubmitGate {
    quota: std::sync::Arc<crate::operations::QuotaGate<crate::execution::budget::ServerBudgetCoordinator>>,
}

#[async_trait::async_trait]
impl crate::transfer::PreSubmitGate for QuotaPreSubmitGate {
    async fn check_submit(
        &self,
        ctx: &datazen::shared::context::RequestContext,
        _submission: &datazen::execution::application::VerifiedSubmission,
    ) -> Result<(), ApiError> {
        let request = datazen::execution::application::budget::BudgetRequest::new(
            ctx.organization_id.clone(),
            datazen::shared::id::ConnectionId::new("job-submission"),
            datazen::execution::application::budget::BudgetPurpose::BackgroundJob,
        );
        match self.quota.gate(ctx, request).await {
            Ok(admission) => {
                let _ = self
                    .quota
                    .release(
                        &admission.permit,
                        datazen::execution::application::budget::ReleaseOutcome::Unused,
                    )
                    .await;
                Ok(())
            }
            Err(rejection) => Err(rejection.to_api_error()),
        }
    }
}
