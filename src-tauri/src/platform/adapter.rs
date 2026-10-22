//! §6.2 第 8 步：Tauri command 的窄适配组装根。
//!
//! ## 本 track 落在哪一步
//!
//! §6.2 的九步顺序里，本文件是第 8 步。它的上游依赖第 6 步（Runtime）与第 7 步
//! （`ApplicationServices`），下游是第 9 步（`state.manage`）。
//!
//! ## 为什么不放 no-op 实现
//!
//! 给 13 个方法写空壳（返回空 Vec、返回假的 `SessionView`）会让
//! `ApplicationServices` 看起来已接线，前端与 E2E 会据此以为功能可用，
//! 而实际上一旦有人调用就会拿到**没有 owner、没有 epoch、没有幂等**的会话。
//! 静默的空壳比显式报错危险得多，所以本文件一个空壳都不写。
//!
//! ## 保留的窄适配
//!
//! 本文件同时是"保留现有 IPC 外观"（:77）的落点：
//! 组装出的 [`CallScope`] 只把**身份**加到旧命令需要的位置，
//! 命令名、入参形状、返回类型一律不动。

use std::sync::Arc;

use datazen_application::error::{ApiError, ApiErrorCode};
use datazen_application::frontend_bridge::{self, BridgeInjection, FrontendBridge};
use datazen_application::ApplicationServices;
use datazen_driver_api::CommandCatalog;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::{
    id::{Counter, RequestId},
    ports::profile::ProfileRepository,
    target::{CanonicalTarget, ExecutionTarget},
};

use super::handles::SessionHandleRegistry;
use super::identity::{DesktopIdentity, OwnerIntent};

/// 组装进度。用来把"当前缺哪一步"变成可读的常量，而不是散落在注释里。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssemblyState {
    /// 身份与 owner 组装已就绪，第 6、7 步尚未注入。
    IdentityReady,
    /// 第 6 步（Runtime）已注入，第 7 步尚未注入。
    AwaitingApplicationServices,
    /// §6.2 九步全部落地。
    FullyWired,
}

impl AssemblyState {
    /// §6.2 中**尚未**落地的步骤，按顺序。供日志与启动自检打印，避免"为什么不能用"。
    ///
    /// 每项的第二个元素是"补上这一步之后"的 [`AssemblyState`]。
    pub fn missing_steps(self) -> &'static [(&'static str, AssemblyState)] {
        match self {
            AssemblyState::IdentityReady => &[
                ("§6.2-6 Runtime", AssemblyState::AwaitingApplicationServices),
                (
                    "§6.2-7 ApplicationServices",
                    AssemblyState::AwaitingApplicationServices,
                ),
            ],
            AssemblyState::AwaitingApplicationServices => &[(
                "§6.2-7 ApplicationServices",
                AssemblyState::AwaitingApplicationServices,
            )],
            AssemblyState::FullyWired => &[],
        }
    }
}

/// 一次 IPC 调用的窄适配上下文。
///
/// **不含**会话、不含句柄、不含权限判定结果 —— 那些要么还没接线（第 6/7 步），
/// 要么属于应用层职责。本结构只携带身份，且身份由 adapter 组装而非来自入参。
#[derive(Debug)]
pub struct CallScope {
    context: RequestContext,
}

impl CallScope {
    pub fn context(&self) -> &RequestContext {
        &self.context
    }

    /// 取出 `requestId` 供日志对账。UUID，不含机密。
    pub fn request_id(&self) -> &RequestId {
        &self.context.request_id
    }
}

/// Tauri 侧的 platform 适配根。
pub struct PlatformAdapter {
    identity: DesktopIdentity,
    handles: Arc<SessionHandleRegistry>,
    services: std::sync::OnceLock<Arc<ApplicationServices>>,
    runtime: std::sync::OnceLock<Arc<datazen_runtime::application::RuntimeConnectionUseCases>>,
    profiles: std::sync::OnceLock<Arc<super::repositories::DesktopProfiles>>,
    /// 执行身份解析器（`IdentityResolver` 端口的桌面实现）。
    ///
    /// 放在 `OnceLock` 里而不是构造参数：它是**装配期**决定的事（派生盐取自宿主
    /// 主密钥），而查询方在装配完成前拿不到它——与 `profiles` / `live` 同理。
    identities: std::sync::OnceLock<Arc<super::execution_identity::DesktopIdentityResolver>>,
    driver_registry: std::sync::OnceLock<Arc<datazen_runtime::driver_registry::DriverRegistry>>,
    target_metadata: std::sync::OnceLock<super::target_metadata::TargetMetadataCache>,
    target_metadata_gate: std::sync::Mutex<()>,
    /// 出向端口适配器。未接线是配置事实（`None`），不是故障。
    ///
    /// 单独存一份是为了让「接线用的那个实例」与「将来要查询绑定的那个实例」
    /// 是**同一个**：两个实例背后是两张 `by_session` 表，端口就会查不到
    /// 自己刚绑定的那一行。`inject` 之外不需要它，因此放在 `OnceLock` 里
    /// 而不是字段构造参数。
    live: std::sync::OnceLock<Arc<super::live_source::DesktopLiveConnectionSource>>,
    /// 空闲回收驱动器。**归应用层所有**，宿主只负责在退出时停它。
    ///
    /// 放在 adapter 上而不是 `AppState`：`inject` 一次、进程一份，停它的时机
    /// （进程退出）与 adapter 的生命周期天然重合，不需要再引入一个「谁来持有句柄」
    /// 的所有权问题。`IdleReclaimer::Drop` 本身就会 stop + abort，因此即使
    /// 漏了显式 `stop`，任务也不会泄漏——这里仍显式停，是为了让停止**发生在一个
    /// 确定的时刻**，而不是等某个不可预测的析构点。
    idle_reclaimer: std::sync::Mutex<Option<datazen_runtime::application::IdleReclaimer>>,
    artifacts: std::sync::OnceLock<Arc<datazen_runtime::artifact::FileArtifactRepository>>,
    native: std::sync::OnceLock<Arc<super::native_tool::deployment::NativeDeployment>>,
}

impl PlatformAdapter {
    /// 应用启动时组装一次。`client_instance_id` 由调用方（bootstrap）生成。
    pub fn new_launch(client_instance_id: &str) -> Result<Self, ApiError> {
        Ok(Self {
            identity: DesktopIdentity::from_desktop_user(client_instance_id)?,
            handles: Arc::new(SessionHandleRegistry::new()),
            services: std::sync::OnceLock::new(),
            runtime: std::sync::OnceLock::new(),
            profiles: std::sync::OnceLock::new(),
            identities: std::sync::OnceLock::new(),
            driver_registry: std::sync::OnceLock::new(),
            target_metadata: std::sync::OnceLock::new(),
            target_metadata_gate: std::sync::Mutex::new(()),
            live: std::sync::OnceLock::new(),
            idle_reclaimer: std::sync::Mutex::new(None),
            artifacts: std::sync::OnceLock::new(),
            native: std::sync::OnceLock::new(),
        })
    }

    /// 用显式身份组装（测试与打包形态走这条）。
    pub fn with_identity(identity: DesktopIdentity) -> Self {
        Self {
            identity,
            handles: Arc::new(SessionHandleRegistry::new()),
            services: std::sync::OnceLock::new(),
            runtime: std::sync::OnceLock::new(),
            profiles: std::sync::OnceLock::new(),
            identities: std::sync::OnceLock::new(),
            driver_registry: std::sync::OnceLock::new(),
            target_metadata: std::sync::OnceLock::new(),
            target_metadata_gate: std::sync::Mutex::new(()),
            live: std::sync::OnceLock::new(),
            idle_reclaimer: std::sync::Mutex::new(None),
            artifacts: std::sync::OnceLock::new(),
            native: std::sync::OnceLock::new(),
        }
    }

    /// Host export adapters retain the same store used by result publication.
    /// Save-dialog paths become owner-bound expiring capabilities in this store;
    /// frontend request paths are never accepted as an ExportSink.
    pub(crate) fn artifact_store(
        &self,
    ) -> Result<Arc<datazen_runtime::artifact::FileArtifactRepository>, ApiError> {
        self.artifacts.get().cloned().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "artifact storage is not wired",
            )
        })
    }
    pub(crate) fn native_tools(
        &self,
    ) -> Result<Arc<super::native_tool::deployment::NativeDeployment>, ApiError> {
        self.native.get().cloned().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::CapabilityUnsupported,
                "native deployment unavailable",
            )
        })
    }
    pub fn assembly_state(&self) -> AssemblyState {
        if self.services.get().is_some() {
            AssemblyState::FullyWired
        } else if self.runtime.get().is_some() {
            AssemblyState::AwaitingApplicationServices
        } else {
            AssemblyState::IdentityReady
        }
    }

    pub fn identity(&self) -> &DesktopIdentity {
        &self.identity
    }

    /// owner 作用域的 adapter 句柄登记。物理事务句柄由 Runtime actor 管理。
    pub fn handles(&self) -> &Arc<SessionHandleRegistry> {
        &self.handles
    }

    /// 组装本次 IPC 调用的身份上下文。这是 adapter 层唯一的身份入口。
    pub fn begin_call(&self) -> CallScope {
        CallScope {
            context: self.identity.next_request_context(),
        }
    }

    /// 组装带归属校验的调用上下文。归属不成立直接失败，不降级。
    pub fn begin_owned_call(
        &self,
        intent: &OwnerIntent,
    ) -> Result<(CallScope, datazen_platform_api::context::OwnerRef), ApiError> {
        let owner = intent.admit(&self.identity)?;
        Ok((self.begin_call(), owner))
    }

    /// 注入前端桥。转调 [`frontend_bridge::set_bridge_injection`]；重复注入返回 `false`。
    pub fn set_bridge_injection(injection: BridgeInjection) -> bool {
        frontend_bridge::set_bridge_injection(injection)
    }

    /// 取前端桥；未注入 ⇒ 明确报错（:84 条款二）。
    pub fn require_bridge(&self) -> Result<&'static Arc<dyn FrontendBridge>, ApiError> {
        frontend_bridge::require_bridge()
    }

    pub fn application_services(&self) -> Option<Arc<ApplicationServices>> {
        self.services.get().cloned()
    }

    /// 需要应用服务时调用；未注入 ⇒ 明确报错，不回落旧路径。
    pub fn require_application_services(&self) -> Result<Arc<ApplicationServices>, ApiError> {
        self.application_services().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::CapabilityUnsupported,
                "ApplicationServices 未注入（§6.2 第 6/7 步未落位）：\
                 启动装配尚未调用 PlatformAdapter::inject，或在注入完成前就发起了用例调用；\
                 此处不做回落，也不得改走旧共享 session",
            )
        })
    }
}

impl std::fmt::Debug for PlatformAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlatformAdapter")
            .field("identity", &self.identity)
            .field("injected", &self.services.get().is_some())
            .finish()
    }
}
impl PlatformAdapter {
    /// 注入宿主能力。
    ///
    /// `tunnels` 与 `known_hosts` 是**隧道相关的两个存储端口实例**，由组装根
    /// （`bootstrap/app_state.rs`）创建并传入：隧道定义走 `TunnelRepository`
    /// （桌面实现是存储层本身），known-hosts 走 `KnownHostsRepository`（桌面实现读
    /// `{appData}/ssh_known_hosts.json`）。会话后端需要它们来在建连时解引用
    /// `tunnelId` 并启动隧道。
    pub fn inject(
        &self,
        store: Arc<crate::store::DesktopRepository>,
        registry: Arc<datazen_runtime::driver_registry::DriverRegistry>,
        tunnels: Arc<dyn datazen_platform_api::ports::tunnel::TunnelRepository>,
        known_hosts: Arc<dyn datazen_tunnel::KnownHostsRepository>,
    ) -> Result<(), ApiError> {
        if self.services.get().is_some() {
            return Ok(());
        }
        let profiles = Arc::new(super::repositories::DesktopProfiles::new(store.clone()));
        let policy = Arc::new(super::policy::DesktopPolicy::new(store.clone()));
        // 执行身份解析端口（`IdentityResolver`）。派生盐取自宿主主密钥：安装级固定、
        // 跨重启稳定，客户端拿不到就算不出身份键。
        //
        // 取不到主密钥就**不**接线：这里失败关闭，而不是换一个可预测的常量盐。
        //
        // **失败半径要说准**：下面这个 `?` 让 `inject` **整体**返回 `Err`，于是
        // `profiles` / `identities` / `runtime` / `services` 等**一个都装不上**——
        // 缺席的不是「仅身份解析能力」，而是**整个平台装配**。启动装配
        // （`bootstrap/app_state.rs`）对这个错误只 `tracing::error!` 记一条，
        // 应用不崩，但此后所有依赖平台的调用都会**失败**：IPC 表面统一是
        // `NotConfigured`（`platform/error.rs` 把 `ServiceUnavailable` 与
        // `CapabilityUnsupported` 收敛到同一个码），内部 `ApiError` 码**多数**是
        // `ServiceUnavailable`——`runtime()` / `profiles()` / `live_source()` /
        // `artifact_store()` / `identity_resolver()` 这 5 个访问器都是它，
        // 受 `inject` 失败影响的访问器里，`require_application_services()` 与
        // `native_tools()` 返回 `CapabilityUnsupported`（另有 `require_bridge()` 与
        // `begin_owned_call()` 也返回该码，但它们不由 `inject` 决定）。
        let identities = Arc::new(
            super::execution_identity::DesktopIdentityResolver::for_desktop(
                store.clone(),
                profiles.clone(),
                policy.clone(),
            )
            .map_err(|error| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    format!("execution identity resolver unavailable: {error}"),
                )
            })?,
        );
        let backend = Arc::new(super::session_backend::DesktopSessionBackend::new_scoped(
            registry.clone(),
            store.clone(),
            16,
            self.identity.organization_id().clone(),
            self.identity.principal_id().clone(),
            tunnels,
            known_hosts,
        ));
        let native =
            super::native_tool::deployment::NativeDeployment::from_build(backend.physical_budget());
        let artifacts = Arc::new(
            datazen_runtime::artifact::FileArtifactRepository::open(
                store.data_dir().join("result-artifacts"),
            )
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "artifact storage initialization failed",
                )
            })?,
        );
        let live = Arc::new(super::live_source::DesktopLiveConnectionSource::new(
            registry.clone(),
            store.clone(),
            backend.clone(),
        ));
        let runtime = Arc::new(
            datazen_runtime::application::RuntimeConnectionUseCases::new(
                profiles.clone(),
                policy.clone(),
                backend.clone(),
            ),
        );
        // 桌面口径的回收节奏：业务空闲期限仍是 §9.2 的 30 分钟，但扫描要更勤。
        // `arm_detached_grace` 把「摘掉挂载的会话」的期限收到 60 秒
        // （`IdlePolicy::DEFAULT_DETACHED_GRACE`），而默认扫描间隔是 3 分钟——宽限最坏会被
        // 拖到 4 分钟，等于「已经没人用的会话」继续占着 2 槽物理额度（PostgreSQL 元数据一类
        // 就是数据池 + 控制池）。15 秒一跳让宽限名副其实，也让 §9.2 的 30 分钟期限到期后
        // 不再多留最多 3 分钟。
        runtime.with_idle_policy(
            datazen_runtime::application::IdlePolicy::default()
                .with_scan_interval(std::time::Duration::from_secs(15)),
        );
        // 出向端口在此接线。放在 `runtime` 构造之后、`services` 之前：端口是**宿主
        // 装配期**决定的事（与 `RuntimeConnectionUseCases::new` 的三个业务依赖分开），
        // 而 `services` 一旦 set 出去，`assembly_state()` 就报 FullyWired——在那之前
        // 端口必须已经能用，否则会有一个「看起来装好了、第一个借用就 NotWired」的窗口。
        runtime.with_live_connections(live.clone());
        self.live.set(live).map_err(|_| {
            ApiError::new(
                ApiErrorCode::ContextConflict,
                "live source already injected",
            )
        })?;
        let sink = runtime.result_sink();
        sink.with_artifact_store(artifacts.clone())?;
        self.artifacts.set(artifacts).map_err(|_| {
            ApiError::new(
                ApiErrorCode::ContextConflict,
                "artifact storage already injected",
            )
        })?;
        let export_sink = sink.clone();
        backend
            .set_export_output_authority(Arc::new(move |id| export_sink.has_export_output(id)))
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "export authority unavailable",
                )
            })?;
        backend
            .set_publisher(Arc::new(move |id, output| {
                let sink = sink.clone();
                Box::pin(async move {
                    let output = match sink.publish_export_output(&id, output).await {
                        Ok(None) => return Ok(()),
                        Ok(Some(output)) => output,
                        Err(_) => {
                            return Err(datazen_runtime::connection::ProviderError::ProtocolError(
                                "export publication rejected".into(),
                            ))
                        }
                    };
                    let result = match output.get("source").cloned() {
                        Some(source) => serde_json::from_value(source)
                            .map_err(|_| ApiError::invalid_argument("invalid result source"))
                            .and_then(|source| sink.publish_statement_output(&id, output, source)),
                        None => sink.publish_output(&id, output),
                    };
                    result.map_err(|_| {
                        datazen_runtime::connection::ProviderError::ProtocolError(
                            "result publication rejected".into(),
                        )
                    })
                })
            }))
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "result publisher unavailable",
                )
            })?;
        let target_metadata = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let target = super::target_metadata::resolver(target_metadata.clone());
        let services = Arc::new(ApplicationServices::new(
            target,
            datazen_application::identity_policy::IdentityPolicy::new(),
            runtime.clone(),
        ));
        self.runtime.set(runtime.clone()).map_err(|_| {
            ApiError::new(ApiErrorCode::ContextConflict, "runtime already injected")
        })?;
        self.profiles.set(profiles).map_err(|_| {
            ApiError::new(ApiErrorCode::ContextConflict, "profiles already injected")
        })?;
        self.identities.set(identities).map_err(|_| {
            ApiError::new(
                ApiErrorCode::ContextConflict,
                "identity resolver already injected",
            )
        })?;
        self.driver_registry.set(registry).map_err(|_| {
            ApiError::new(
                ApiErrorCode::ContextConflict,
                "driver registry already injected",
            )
        })?;
        self.target_metadata.set(target_metadata).map_err(|_| {
            ApiError::new(
                ApiErrorCode::ContextConflict,
                "target metadata already injected",
            )
        })?;
        self.services.set(services).map_err(|_| {
            ApiError::new(ApiErrorCode::ContextConflict, "services already injected")
        })?;
        // Publish only after the matching Runtime backend was fully injected:
        // no partial retry may bind native jobs to a different physical ledger.
        match native {
            Ok(native) => {
                let _ = self.native.set(native);
            }
            Err(_) => tracing::warn!("native deployment rejected; native operations unsupported"),
        }
        // 空闲回收驱动器：**应用层自己拥有**，宿主不再另挂定时器（这样就不会有
        // 「两个驱动器抢同一张 registry 表」的形状）。放在 services 之后启动，
        // 是为了让「装配已 FullyWired」与「回收器已在跑」不出现反序——反序时
        // 会出现一个会话已建、回收器还没起的窗口，而那段时间的空闲判定完全缺失。
        self.start_idle_reclaimer();
        self.log_runtime_wiring();
        Ok(())
    }

    /// 把两条出/入口的接线状态打成一行**可观测**的启动日志。
    ///
    /// ## 为什么必须打出来而不是留在「代码里看得出来」
    ///
    /// 「端口接没接上」在此前只有两种观测方式：读源码，或在生产环境撞一次
    /// `NotWired` 才知道。两者都不合格——前者不可执行，后者太晚。
    ///
    /// 这一行同时是三件「配置事实」的落点，每件都需要在看板上与故障区分开：
    ///
    /// 1. `live_connections`：出向端口是否接线。false 时**不是**错误，是「本部署
    ///    不给宿主执行网关取活连接」，`borrow_live_connection` 得到
    ///    `ServiceUnavailable`。把它打成 warn 会让每个未接线部署都刷屏。
    /// 2. `idle_policy`：空闲回收开关。false 时**不是**错误，是「本部署不做空闲
    ///    回收」，与 §9.2 的 `deadline == None` 是同一件事。
    /// 3. `idle_reclaimer`：扫描任务是否在跑。false 只有一种原因——`runtime()`
    ///    在 `start` 之前就失败了，那是**真故障**，因此这一项单独用 warn。
    ///
    /// ## 它不断言归池可达性
    ///
    /// 首版空闲池在真实驱动上恒不可达（15 个驱动全部
    /// `ResetForReuse::Unsupported`）。那是**正确行为**：§9.4 要求归池前检查与
    /// 驱动 `Clean` 双条件齐备，缺一半就不许把连接留在池里。所以这里不打
    /// 「pool reachable」这种会骗人的字段，也不为了让它变绿而谎报能力。
    fn log_runtime_wiring(&self) {
        let Ok(runtime) = self.runtime() else {
            tracing::warn!("runtime wiring 未落位：runtime 尚未注入，出向端口与空闲回收都不可用");
            return;
        };
        let reclaimer_running = match self.idle_reclaimer.lock() {
            Ok(slot) => slot.is_some(),
            Err(poisoned) => poisoned.into_inner().is_some(),
        };
        if reclaimer_running {
            tracing::debug!(
                live_connections = runtime.has_live_connections(),
                idle_reclaim_enabled = runtime.idle_policy().is_enabled(),
                idle_scan_interval_ms =
                    u64::try_from(runtime.idle_policy().scan_interval().as_millis())
                        .unwrap_or(u64::MAX),
                "runtime wiring: 出向端口与空闲回收均按配置落位（未接线/关闭均为配置事实，非故障）"
            );
        } else {
            tracing::warn!(
                live_connections = runtime.has_live_connections(),
                idle_reclaim_enabled = runtime.idle_policy().is_enabled(),
                "runtime wiring: 空闲回收扫描器未启动——这不是配置事实而是装配缺陷，\
                 会话将只建不回收，额度会随会话数单调下降"
            );
        }
    }

    /// 起空闲回收驱动器，句柄留在 adapter 里，进程退出时 [`Self::stop_idle_reclaimer`] 停它。
    ///
    /// 重复调用是安全的：第二个句柄会替换第一个，被替换那个在自己的 `Drop` 里
    /// stop + abort，所以不会留下第二个活着的扫描任务。
    fn start_idle_reclaimer(&self) {
        let Ok(runtime) = self.runtime() else {
            return;
        };
        let reclaimer = runtime.start_idle_reclaimer();
        match self.idle_reclaimer.lock() {
            // 取了锁再替换：`Drop` 里同样会拿这把锁，因此「旧句柄已停」与
            // 「新句柄已装」不会交叠。
            Ok(mut slot) => *slot = Some(reclaimer),
            Err(poisoned) => *poisoned.into_inner() = Some(reclaimer),
        }
        tracing::debug!("idle reclaimer started: runtime 自有的周期扫描器已接管空闲驱逐");
    }

    /// 停空闲回收驱动器。幂等；从未启动过时是空操作。
    ///
    /// 由 `RunEvent::ExitRequested`（`bootstrap/run.rs`）调用。只翻标志不 abort
    /// 扫描任务：`IdleReclaimer::stop` 让它在下一轮循环边界退出，正在进行中的
    /// 一次 `reclaim_idle()` 会跑完——那是必要的，因为半途打断会在 registry 与
    /// 应用层台账之间留下「驱逐了一半」的中间态。
    pub fn stop_idle_reclaimer(&self) {
        let taken = match self.idle_reclaimer.lock() {
            Ok(mut slot) => slot.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        if let Some(reclaimer) = taken {
            reclaimer.stop();
            tracing::debug!("idle reclaimer stopped");
        }
    }

    /// 出向端口适配器。未注入 ⇒ 明确报错，不回落旧路径。
    pub fn live_source(
        &self,
    ) -> Result<Arc<super::live_source::DesktopLiveConnectionSource>, ApiError> {
        self.live.get().cloned().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "live connection source not injected: 启动装配尚未调用 PlatformAdapter::inject",
            )
        })
    }
    pub fn runtime(
        &self,
    ) -> Result<Arc<datazen_runtime::application::RuntimeConnectionUseCases>, ApiError> {
        self.runtime
            .get()
            .cloned()
            .ok_or_else(|| ApiError::new(ApiErrorCode::ServiceUnavailable, "runtime not injected"))
    }
    pub fn profiles(&self) -> Result<Arc<super::repositories::DesktopProfiles>, ApiError> {
        self.profiles
            .get()
            .cloned()
            .ok_or_else(|| ApiError::new(ApiErrorCode::ServiceUnavailable, "profiles not injected"))
    }

    /// 执行身份解析器。未注入 ⇒ 明确报错，不回落旧路径。
    pub fn identity_resolver(
        &self,
    ) -> Result<Arc<super::execution_identity::DesktopIdentityResolver>, ApiError> {
        self.identities.get().cloned().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "identity resolver not injected: 启动装配尚未调用 PlatformAdapter::inject",
            )
        })
    }

    /// Resolve an explicit execution target against the connection's actual driver metadata.
    /// The caller passes the returned canonical namespace to the runtime so the validated target
    /// is the one used to open the resource and derive its pool identity.
    pub async fn resolve_execution_target(
        &self,
        ctx: &RequestContext,
        expected_config_revision: Counter,
        command: &str,
        target: &ExecutionTarget,
    ) -> Result<CanonicalTarget, ApiError> {
        let profile = self
            .profiles()?
            .get(ctx, target.connection_id.clone())
            .await
            .map_err(|_| ApiError::new(ApiErrorCode::ServiceUnavailable, "profile lookup failed"))?
            .ok_or_else(|| ApiError::new(ApiErrorCode::NotFound, "profile unavailable"))?;
        if !profile.enabled {
            return Err(ApiError::new(ApiErrorCode::NotFound, "profile unavailable"));
        }
        if profile.config_revision.counter() != expected_config_revision {
            return Err(ApiError::new(
                ApiErrorCode::ConfigRevisionMismatch,
                "connection configuration changed",
            ));
        }

        let registry = self.driver_registry.get().cloned().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "driver registry not injected",
            )
        })?;
        let driver = registry
            .get_sql_driver_by_name(&profile.driver_id)
            .await
            .ok_or_else(|| {
                ApiError::new(
                    ApiErrorCode::TargetUnsupported,
                    "connection driver unavailable",
                )
            })?;
        let provider = registry
            .resource_provider(&profile.driver_id)
            .await
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::TargetUnsupported,
                    "connection driver has no namespace metadata provider",
                )
            })?;

        let shape = super::target_metadata::namespace_shape(provider.namespace_shape());
        // Description only: the command catalogue is listed here, never executed.
        let catalog = datazen_driver_api::LegacyCommandCatalog::new(driver.as_ref());
        let mut commands = std::collections::HashMap::new();
        for definition in catalog.command_definitions() {
            if let Some(requirements) = catalog.target_requirements(&definition.id) {
                commands.insert(
                    definition.id,
                    super::target_metadata::command_requirements(requirements),
                );
            }
        }
        for (command, requirements) in super::target_metadata::host_metadata_command_requirements()
        {
            commands.insert(
                command.into(),
                super::target_metadata::command_requirements(requirements),
            );
        }
        let cache = self.target_metadata.get().cloned().ok_or_else(|| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "target metadata not injected",
            )
        })?;
        let _metadata_scope = self.target_metadata_gate.lock().map_err(|_| {
            ApiError::new(
                ApiErrorCode::ServiceUnavailable,
                "target metadata unavailable",
            )
        })?;
        cache
            .write()
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "target metadata unavailable",
                )
            })?
            .insert(
                target.connection_id.clone(),
                super::target_metadata::ConnectionTargetMetadata { shape, commands },
            );

        let resolved = self.require_application_services()?.target.compute(
            command,
            &target.connection_id,
            &target.namespace,
            target.object.as_ref(),
            &datazen_application::target::TargetSource::Request,
        );
        cache
            .write()
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "target metadata unavailable",
                )
            })?
            .remove(&target.connection_id);
        resolved
    }
}

/// `PlatformAdapter` 在 `AppState` 里的落位（§6.2 第 8 步 → 第 9 步 `state.manage`）。
///
/// 刻意**不是** `Option<PlatformAdapter>`：`Option` 会诱导调用方写
/// `state.platform.clone().unwrap_or_else(fallback)`，而那种 fallback 正是 :84
/// 禁止的"回落到旧共享 session"。这里用 `Result` 承载失败原因，组装不成功时
/// 每次取用都返回**同一个可定位的错误**。
///
/// 组装失败的真实场景只有一种：拿不到当前桌面用户（[`DesktopIdentity::from_desktop_user`]）。
/// 那种情况下宁可让依赖身份的调用失败，也不编造一个 `PrincipalId` 出来。
#[derive(Debug, Clone)]
pub struct PlatformEntry {
    inner: Result<Arc<PlatformAdapter>, ApiError>,
}

impl PlatformEntry {
    /// 组装成功。
    pub fn ready(adapter: PlatformAdapter) -> Self {
        Self {
            inner: Ok(Arc::new(adapter)),
        }
    }

    /// 组装失败。原因被保留下来，不降级、不吞掉。
    pub fn failed(err: ApiError) -> Self {
        Self { inner: Err(err) }
    }

    /// 启动期一次性组装：成功给 adapter，失败把原因装进 `PlatformEntry`。
    pub fn launch() -> Self {
        match PlatformAdapter::new_launch(super::identity::new_client_instance_id().as_str()) {
            Ok(adapter) => PlatformEntry::ready(adapter),
            Err(err) => {
                tracing::warn!(
                    "platform adapter 组装失败，依赖身份的 IPC 调用将明确失败: {}",
                    err.message
                );
                PlatformEntry::failed(err)
            }
        }
    }

    pub fn is_ready(&self) -> bool {
        self.inner.is_ok()
    }

    /// 取 adapter。组装失败时原样返回 `ApiError`（code + 可定位文案）。
    pub fn require(&self) -> Result<&Arc<PlatformAdapter>, ApiError> {
        self.inner.as_ref().map_err(|err| err.clone())
    }

    /// 当前组装进度。组装失败或 `inject()` 之前指向 §6.2 尚未落地的步骤，不假装已完成。
    pub fn assembly_state(&self) -> AssemblyState {
        match self.inner.as_ref() {
            Ok(adapter) => adapter.assembly_state(),
            Err(_) => AssemblyState::IdentityReady,
        }
    }
}

// 用例搬出本文件：adapter.rs 同时被 `include_str!` 当成整篇文本扫描，生产半边必须自己就落在
// 800 行以内。`#[path]` 让 `super::` 与 `include_str!("adapter.rs")` 的解析基准都不变，
// 扫描到的仍是同一段生产代码。
#[cfg(test)]
#[path = "adapter_tests.rs"]
mod tests;

#[cfg(test)]
impl PlatformAdapter {
    pub(crate) fn replace_native_fixture(&mut self, manifest: &str) -> Result<(), ApiError> {
        let replacement = self
            .native_tools()?
            .fixture_manifest(manifest)
            .map_err(|_| ApiError::invalid_argument("synthetic native deployment rejected"))?;
        self.native.take();
        self.native
            .set(replacement)
            .map_err(|_| ApiError::invalid_argument("synthetic native fixture unavailable"))
    }
}
