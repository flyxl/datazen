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

use datazen::settings::SettingsRepository;
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
        budget.clone(),
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
            datazen::shared::id::RuntimeEpoch::new(crate::http::sse::SERVER_RUNTIME_EPOCH),
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
    // DomainState 装配后移到 session_directory 定义之后（execution 域路由
    // 刀：令牌签发的会话绑定操作需要目录核对归属，见 router.rs 的
    // `handler_issue_token`）。

    // WR-F7 执行栈（无 feature 门控）：真租约源三件套全部就位——policy
    // 适配器（policy_adapter.rs，本波解除阻塞项）+ `storage::ServerProfiles`
    // + `driver_exec::ServerDriverExecBackend`（预算桥 + 出站闸 + 结果发布
    // 钩子同源接线）。驱动工厂按部署构建切片点亮（W8-a P1）：`driver-*`
    // per-driver feature 选择性链接驱动 crate，`deploy-drivers` 聚合一次
    // 全量；缺省（无 driver-*）目录为空表，任何连接目标显式答「驱动不可用」。
    let vault = std::sync::Arc::new(
        crate::execution::secret_provider::ServerSecretVault::new(
            store.clone(),
            secret_bytes.clone(),
        )
        .map_err(|source| ServerError::Serve(format!("secret vault: {source}")))?,
    );
    let connections_repo = std::sync::Arc::new(crate::storage::ServerConnections::new(
        store.clone(),
        vault.clone(),
    ));
    let profiles_repo = std::sync::Arc::new(crate::storage::ServerProfiles::new(store.clone()));
    // 出站白名单：config 注入（空表 = 全拒，fail-closed）。
    let network = std::sync::Arc::new(
        crate::execution::network_provider::ServerNetworkProvider::new(
            config.egress_allowed_hosts.clone(),
        ),
    );
    let egress: crate::execution::driver_exec::EgressGate = {
        let network = network.clone();
        std::sync::Arc::new(move |config: &datazen_driver_api::ConnectionConfig| {
            match &config.host {
                Some(host) => network
                    .whitelist_check(&format!("tcp://{host}"))
                    .map_err(|error| error.to_string()),
                None => Ok(()),
            }
        })
    };
    // 物理预算桥：coordinator 的同一实例（quota_gate 的份额共存）。
    let budget_bridge =
        std::sync::Arc::new(crate::execution::budget_bridge::BudgetPortBridge::new(
            budget.clone(),
            datazen::shared::id::OrganizationId::new(config.organization_id.clone()),
        ));
    // 驱动注册表：inventory 懒形（`DriverRegistry::new()`）。驱动 crate 由
    // `driver-*` per-driver feature 选择性链接（`driver_link.rs` 的 extern
    // crate 闭包），链接即经 `datazen_driver_api::inventory::submit!`（inventory）
    // 入表；不开任何 driver-* 时目录为空表——与 v1 的 `with_factories([])`
    // 同一语义，任何连接目标照旧显式答「驱动不可用」（W8-a P1）。执行后端
    // 与（mcp 构建下的）目标机器共享同一实例。
    let driver_registry =
        std::sync::Arc::new(datazen::execution::runtime::driver_registry::DriverRegistry::new());
    let backend = std::sync::Arc::new(crate::execution::driver_exec::ServerDriverExecBackend::new(
        driver_registry.clone(),
        connections_repo.clone(),
        profiles_repo.clone(),
        datazen::connection::domain::ConnectionScope::new(config.organization_id.clone()),
        budget_bridge,
        Some(egress),
    ));
    // W8 刀2：活连接出向端口适配器——backend 尚未被租约源移动，clone 一份
    // 给它（`by_session` 反向索引随 backend.open 同帧写入，两份 Arc 必须
    // 指向**同一** backend 实例；连接目录与 registry 同实例既有纪律）。
    let live_source = std::sync::Arc::new(
        crate::execution::live_source::ServerLiveConnectionSource::new(
            backend.clone(),
            connections_repo.clone(),
            driver_registry.clone(),
            datazen::connection::domain::ConnectionScope::new(config.organization_id.clone()),
        ),
    );
    // W8 刀2：SchemaCache 全局一个实例（缓存键第一维 connection_id，天然按
    // 连接分桶；TTL 300s 与桌面同口径；失效接线随写半边的后续刀定）。
    let schema_cache = std::sync::Arc::new(datazen::schema::cache::SchemaCache::new());
    // 结果发布钩子与租约源共享同一 RuntimeResultSink（publisher.rs 的装配
    // 纪律）。
    let result_sink = datazen::execution::application::orchestration::RuntimeResultSink::default();
    backend
        .set_publisher(crate::execution::publisher::execution_result_publisher(
            result_sink.clone(),
        ))
        .map_err(|error| ServerError::Serve(format!("result publisher: {error}")))?;
    let execution_policy = std::sync::Arc::new(
        crate::execution::policy_adapter::ServerExecutionPolicy::new(
            policy.clone(),
            store.clone(),
            clock.clone(),
        ),
    );
    let lease_source = std::sync::Arc::new(
        crate::execution::lease::ServerRuntimeLeaseSource::new_with_shared_parts(
            profiles_repo.clone(),
            execution_policy,
            backend,
            result_sink,
            std::sync::Arc::new(
                datazen::execution::runtime::directory::InMemorySessionDirectory::new(),
            ),
        )
        // W8 刀2：活连接端口随实例装配——慢路构造的每个新 runtime 实例
        // 构造后立即接线；吊销→重建自动重接（lease.rs 模块头「活连接端口
        // 随实例装配」）。
        .with_live_source(live_source)
        // 空闲回收策略随实例装配（IdleReclaimer 武装刀）：config 两键 →
        // `IdlePolicy` 的唯一映射在 `config::idle_policy_from_config`；
        // 慢路每个新实例 `with_idle_policy` 接线，吊销→重建天然跟随。
        .with_idle_policy(crate::config::idle_policy_from_config(
            config.runtime_idle_ttl_secs,
            config.runtime_idle_scan_secs,
        )),
    );
    // 空闲回收驱动器起跑（设计档 §4.2 装配点）：租约源与全部消费者装配
    // **之后** spawn（无反序窗口）；每 tick 现取当前实例（吊销跟随）。
    // `axum::serve` 的 graceful drain 完成后 stop（Drop 为兑底）。
    let idle_reclaimer = crate::execution::idle_reclaimer::ServerIdleReclaimer::spawn(
        lease_source.clone(),
        crate::config::idle_policy_from_config(
            config.runtime_idle_ttl_secs,
            config.runtime_idle_scan_secs,
        ),
    );
    tracing::info!(
        idle_reclaim_enabled = config.runtime_idle_ttl_secs != 0,
        idle_ttl_secs = config.runtime_idle_ttl_secs,
        idle_scan_secs = config.runtime_idle_scan_secs,
        "runtime wiring: idle reclaimer configured"
    );
    // 执行域路由的用例面：租约委托适配器（每调用现取，与 MCP 同一租约源）。
    let lease_cases: std::sync::Arc<
        dyn datazen::execution::application::use_cases::ConnectionUseCases,
    > = std::sync::Arc::new(
        crate::execution::lease_use_cases::LeaseConnectionUseCases::new(lease_source.clone()),
    );
    // sessions 域可见性闸的目录 = 租约源内部**同一**实例（跨 resolve 共享；
    // 用例的 open/get 都登记/查询它——域文件模块头的接线登记落地）。
    let session_directory: std::sync::Arc<dyn datazen::execution::SessionDirectory> =
        lease_source.session_directory();

    // WR-D6/D7 域路由态（jobs / submission-tokens / artifacts）+ 执行域路由
    // 刀新增的用例面（issue-token 的会话绑定操作经它签发，与运行时令牌表
    // 同一实例闭环）。
    let domains = http::router::DomainState {
        issuer: issuer.clone(),
        submission,
        uploads,
        downloads,
        tasks: task_center,
        sessions: lease_cases.clone(),
    };

    // 服务端设置存储：ai-host 域、（mcp 构建下的）目标机器与 driverGateway
    // 过渡域共享同一实例。
    let settings_store = std::sync::Arc::new(crate::storage::ServerSettingsStore::scoped(
        &config.organization_id,
        store.clone(),
    ));

    // WR-F7 目标元数据切片：`ResourceExecutionHost` 端口 server 实现
    // （`execution::resource_host`）。租约源与 MCP / sessions 域同一实例
    // （共享 sink + 会话目录）；驱动注册表与执行后端同一实例。消费者：
    // （mcp 构建下的）`TargetMachine` 与 driverGateway 过渡域（恒开）。
    let resource_host = std::sync::Arc::new(
        crate::execution::resource_host::ServerResourceExecutionHost::new(
            connections_repo.clone(),
            profiles_repo.clone(),
            driver_registry.clone(),
            settings_store.clone(),
            lease_source.clone(),
            &config.organization_id,
        ),
    );

    // W8-a P2：tableData 域的执行半边——`TargetCommandRunner` 端口 server
    // 实现（`execution::target_runner`）。host/lease 与 driverGateway、
    // （mcp 构建下的）目标机器同一实例（bootstrap 既有纪律）；执行面授权
    // 语见 target_runner 模块头（P3 收口点登记在案）。
    let table_target_runner: std::sync::Arc<dyn datazen::table_browsing::api::TargetCommandRunner> =
        std::sync::Arc::new(crate::execution::target_runner::ServerTargetRunner::new(
            resource_host.clone(),
            lease_source.clone(),
        ));

    // SSE（WR-D5，W8-b 起提前到域路由装配之前）：origin 白名单取
    // public_origin（§8.1）；事件后备是 MemoryEventSink（单实例 v1 形态）；
    // 周期复核走标准实现。`Option` = publicOrigin 缺席时不挂事件端点；
    // driverGateway 流投递桥与挂载端点共享同一实例（registry/sink 同源）。
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

    // WR-C2 §6.1–§6.4 读侧域路由（薄绑定；挂载归 router）。
    let domain_routers = vec![
        crate::http::domains::settings::settings_router(
            crate::http::domains::settings::SettingsState {
                store: store.clone(),
            },
        ),
        crate::http::domains::identity::identity_router(),
        crate::http::domains::connections::connections_router(
            crate::http::domains::connections::ConnectionsState {
                profiles: profiles_repo.clone(),
                issuer: issuer.clone(),
            },
        ),
        crate::http::domains::sessions::sessions_router(
            crate::http::domains::sessions::SessionsState {
                // W8-a 接线刀起 tableData 域共享同一用例面（estimate 的
                // get_session），此处改 clone。
                sessions: lease_cases.clone(),
                directory: session_directory.clone(),
            },
        ),
        // execution 域路由（契约 §6.10；执行域路由收口刀）：会话内执行 /
        // 目标执行 / 执行视图 / 取消登记——用例面与 sessions 域同一实例
        // （租约委托适配器），目录与 sessions 域同一实例（同一可见性事实）。
        crate::http::domains::execution::execution_router(
            crate::http::domains::execution::ExecutionState {
                sessions: lease_cases.clone(),
                directory: session_directory.clone(),
            },
        ),
        // WR-F1 服务端半边（三缺口合拢）：secrets 域（ServerSecretVault 的
        // 受控解析/存储 + 审计）与 ai-host 域（执行设置读 + 目标执行
        // fail-closed 登记）。挂载走 domain_routers 统一通道，认证链
        // L2–L6 与 CSRF 覆盖由 router 的 merge 循环保证。
        crate::http::domains::security::security_router(
            crate::http::domains::security::SecurityState {
                vault: vault.clone(),
                audit: audit.clone(),
            },
        ),
        crate::http::domains::ai_host::ai_host_router(crate::http::domains::ai_host::AiHostState {
            settings: settings_store.clone(),
            organization_id: config.organization_id.clone(),
            // 执行域路由收口刀：目标执行宿主与 MCP 目标机器 / driverGateway
            // 同一实例（共享驱动注册表 / 设置存储 / 租约源）。
            host: resource_host.clone(),
        }),
        // T25：组织记忆由服务端组织角色治理。桌面本地没有 OrgAdmin，
        // 因此不把管理员语义伪造到本地 store；成员只读/审计走此域。
        crate::http::domains::org_memory::org_memory_router(
            crate::http::domains::org_memory::OrgMemoryState {
                store: store.clone(),
                policy: policy.clone(),
            },
        ),
        // WR-C3 §6.7–6.9 tableData 域（W3 刀）：命名视图三端点真实现
        // （v9 `named_views` 表）。W8-a 接线刀：读三端点（reads /
        // read-sql-previews / row-count-estimates）501 → 真用例接线——
        // profiles/connections 按请求组织读取（SessionIdentity.ctx 真主体
        // 语义），directory/sessions 与 sessions 域同一实例；W8 刀3：
        // 变更二端点（change-plan-previews / change-commits）点亮，`lease`
        // 是 commit 写半边单次 resolve 的源（与 MCP / sessions 域同一
        // 实例）；W8 刀2：FK 二 + 值搜索 + origin 四读端点点亮（`lease`
        // 兼活句柄借出源 + `schema_cache` 目录缓存读通路径）；其余二端点
        // fail-closed（501，逐端口收窄原因）。认证链 L2–L6 与 CSRF 由
        // merge 循环保证。
        crate::http::domains::table_data::table_data_router(
            crate::http::domains::table_data::TableDataState {
                store: store.clone(),
                registry: driver_registry.clone(),
                runner: table_target_runner.clone(),
                profiles: profiles_repo.clone(),
                connections: connections_repo.clone(),
                directory: session_directory.clone(),
                sessions: lease_cases.clone(),
                lease: lease_source.clone(),
                schema_cache: schema_cache.clone(),
            },
        ),
        // WR-C2 §6.9 driverGateway 过渡域（四方法；闸序与目标机器同一
        // resource_host 实例；目录 = sessions 域同一实例）。W8-b：统一事件
        // 面（SSE 同一 registry/sink）、审计端口与 ephemeral 会话注册表
        // 同源装配。
        crate::http::domains::driver_gateway::driver_gateway_router(
            crate::http::domains::driver_gateway::DriverGatewayState {
                registry: driver_registry.clone(),
                host: resource_host.clone(),
                directory: session_directory.clone(),
                settings: settings_store
                    .read_settings()
                    .await
                    .map(|shared| shared.execution)
                    .unwrap_or_else(|_| crate::storage::default_shared_settings().execution),
                sse: sse.clone(),
                audit: audit.clone() as std::sync::Arc<dyn datazen::shared::audit::AuditSink>,
                clock: clock.clone(),
                ephemeral: std::sync::Arc::new(
                    crate::execution::ephemeral::EphemeralSessions::new(),
                ),
                now_unix: std::sync::Arc::new(|| {
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or(0)
                }),
            },
        ),
    ];

    // WR-F2：MCP streamable HTTP（feature 门控；执行栈已在上方装配）。
    #[cfg(feature = "mcp")]
    let mcp_state = {
        let authenticator = CookieSessionAuthenticator {
            sessions: crate::storage::LoginSessions::new(store.clone()),
        };
        // 目标机器复用上方共享的 resource_host（驱动注册表/设置存储/租约
        // 源同一实例）。
        let execution_settings = settings_store
            .read_settings()
            .await
            .map(|shared| shared.execution)
            .unwrap_or_else(|_| crate::storage::default_shared_settings().execution);
        let stateless = std::sync::Arc::new(
            crate::mcp::stateless_exec::StatelessToolExec::new(
                std::sync::Arc::new(crate::mcp::stateless_exec::ServerRedactedConnections::new(
                    connections_repo.clone(),
                    &config.organization_id,
                )),
                std::sync::Arc::new(crate::mcp::stateless_exec::EmptyWorkflowCatalog),
                Vec::new(), // 连接白名单：空表 = 全拒绝（安全默认；部署配置随 settings 切片接入）
            )
            .with_target_machine(crate::mcp::stateless_exec::TargetMachine {
                host: resource_host,
                settings: execution_settings,
            }),
        );
        Some(crate::mcp::runtime::build_http_state(
            std::sync::Arc::new(authenticator),
            lease_source,
            Vec::new(),
            datazen::execution::domain::permission::McpPermissionMode::SafeWrite,
            Some(stateless),
        ))
    };
    #[cfg(not(feature = "mcp"))]
    let mcp_state = None;
    #[cfg(not(feature = "mcp"))]
    let _ = (&resource_host, &lease_source); // 非 mcp 构建由 driverGateway 域与 sessions 域消费

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
        domain_routers: Some(domain_routers),
    })?;

    // WR-E5：静态资源同源服务（配置了 `web_root` 才挂）。SPA fallback →
    // `index.html`；`/api/` 与 `/mcp` 不受影响（fallback 只接未被域路由
    // 认领的路径，且 api_or_not_found 在 fallback 链先注册——ServeDir 挂
    // 在更外层的 fallback 上，仅承接 404 的非 API 路径）。SSE 缺失 Origin
    // 的同源校验在 D5 已逐字落地（§8.1），同源静态面是其前提形态。
    let app = match &config.web_root {
        Some(root) => {
            if !root.is_dir() {
                return Err(ServerError::Serve(format!(
                    "web_root '{}' is not a directory",
                    root.display()
                )));
            }
            let index = root.join("index.html");
            app.fallback_service(
                tower_http::services::ServeDir::new(root)
                    .not_found_service(tower_http::services::ServeFile::new(index)),
            )
        }
        None => app,
    };

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
    // serve 返回（drain 完成）后停车：空闲回收驱动器在下一轮循环边界退出
    // （幂等）；`idle_reclaimer` 离开作用域时 Drop 兜底（stop + abort）。
    idle_reclaimer.stop();
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

/// 停机信号：SIGINT（Ctrl-C）与 SIGTERM（容器编排的标准停机路径，WR-G3
/// 接线完成）。两条信号先到先得；drain 语义（在飞请求排空）由
/// `with_graceful_shutdown` 承接，后台清扫任务的停机随 drain 切片。
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
    tracing::info!("shutdown signal received; draining");
}

/// WR-D6 QuotaGate 适配为 WR-D7 的 `PreSubmitGate`（`transfer` 模块文档
/// 的接缝裁决）。闸门只读无副作用：重放路径同样穿过它，因此提交本身
/// 不占用持久额度——占位检查容量后立即归还（执行期额度随 ExecutionGateway
/// 接线收取）。拒绝用 wire 词汇原样透传，transfer 侧零翻译。
struct QuotaPreSubmitGate {
    quota: std::sync::Arc<
        crate::operations::QuotaGate<crate::execution::budget::ServerBudgetCoordinator>,
    >,
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
