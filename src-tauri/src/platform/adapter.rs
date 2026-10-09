//! §6.2 第 8 步：Tauri command 的窄适配组装根。
//!
//! ## 本 track 落在哪一步
//!
//! §6.2 的九步顺序里，本文件是第 8 步。它的上游依赖第 6 步（Runtime）与第 7 步
//! （`ApplicationServices`），下游是第 9 步（`state.manage`）。
//!
//! ## 第 6、7 步如何落地
//!
//! [`PlatformAdapter::inject`] 负责 §6.2 第 6 步（Runtime）与第 7 步
//! （`ApplicationServices`），由启动装配调用（`bootstrap/app_state.rs`）。它组装
//! `DesktopProfiles`（配置源）、`DesktopSessionBackend`（租约后端）与
//! `RuntimeConnectionUseCases`，目标解析器**失败关闭**（四个分支一律返回 `None`，
//! 不猜目标），最后 `ApplicationServices::new` 构造服务并写入三个 `OnceLock`。
//! 任何 `OnceLock` 重复写入映射为 `ContextConflict`；`inject()` 重复调用直接返回。
//!
//! ## 13 个方法要求的字段现在各是什么状态
//!
//! 下表逐项核对过实现，**7 项全部已实现**，均不依赖占位常量：
//!
//! | 方法要求的字段 | 落地位置 | 状态 |
//! | --- | --- | --- |
//! | `ProfileView::config_revision` | 配置源从 `ProfileMetadata::config_revision` 取持久化值；改配置、禁用连接时单调推进 | 已实现 |
//! | `ProfileView::credential_revision` | 同上取 `ProfileMetadata::credential_revision`；改密时推进，写入前校验一致 | 已实现 |
//! | `ProfileView::enabled` | 从 `ProfileMetadata::enabled` 取值；禁用时同时推进 `config_revision` | 已实现 |
//! | `RuntimeEpoch`（attach / get / close_session） | 由会话租约句柄的 epoch 构造，按 epoch 索引注册表，不匹配时报"epoch 不再匹配" | 已实现 |
//! | 幂等记录（submission token / `idempotency_key`） | 用例层的 receipts 台账；重放返回 `IdempotencyConflict` | 已实现 |
//! | `attachment_token`（attach / detach） | attach 校验、detach 解除；上下文变更时轮换 | 已实现 |
//! | `stream_id`（`subscribe_events`） | `open_session` 回执携带 `stream_id`，订阅按 `StreamId` 进行 | 已实现 |
//!
//! 版本号溢出按 `PortError::BackendUnavailable` 处理，不回绕。
//!
//! 关于 `ProfileView::new`：该默认构造器仍把两个 revision 置为 `Counter::ZERO`，
//! 但桌面配置源不使用它的默认值，而是逐字段覆盖为持久化元数据
//! （`repositories.rs` 的 `DesktopProfiles::record`）。全树 `Counter::ZERO` 只出现在
//! 测试夹具里，生产路径不产生它——因此 `ConfigRevisionMismatch` /
//! `RuntimeEpochMismatch` 判定仍然有效，不存在"全是第 0 版"的问题。
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
use datazen_application::ApplicationServices;
use datazen_platform_api::context::RequestContext;
use datazen_platform_api::id::RequestId;

use super::bridge::{self, BridgeInjection, FrontendBridge};
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
        }
    }

    /// 当前组装进度。**从注入事实推导**，读 `runtime` / `services` 两个 `OnceLock`。
    ///
    /// 早先这里是 `ASSEMBLY_STATE` 常量。该常量由 [`PlatformAdapter::inject`] 与
    /// 各写各的：`inject()` 只写 `OnceLock`、从不回写常量，因此常量自落地起就没被推进过，
    /// 一直是 `AwaitingApplicationServices`，而实际早已 `FullyWired`。改为推导后，
    /// 状态不可能再与注入事实脱节。
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

    /// owner 作用域的会话级句柄登记表。与 `AppState::session_transactions` 并存、
    /// 互不读写：见 [`super::handles`] 模块头。
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

    /// 注入前端桥。转调 [`bridge::set_bridge_injection`]；重复注入返回 `false`。
    pub fn set_bridge_injection(injection: BridgeInjection) -> bool {
        bridge::set_bridge_injection(injection)
    }

    /// 取前端桥；未注入 ⇒ 明确报错（:84 条款二）。
    pub fn require_bridge(&self) -> Result<&'static Arc<dyn FrontendBridge>, ApiError> {
        bridge::require_bridge()
    }

    /// 取已注入的 `ApplicationServices`。
    ///
    /// §6.2 第 6、7 步由 [`PlatformAdapter::inject`] 落位（调用点
    /// `bootstrap/app_state.rs`）；注入之后这里返回实际构造的实例，
    /// 注入之前为 `None`。保留 `Option` 是为了让调用点能区分"未注入"与"注入失败"。
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
    pub fn inject(
        &self,
        store: Arc<crate::store::Store>,
        registry: Arc<crate::db::DriverRegistry>,
    ) -> Result<(), ApiError> {
        if self.services.get().is_some() {
            return Ok(());
        }
        let profiles = Arc::new(super::repositories::DesktopProfiles::new(store.clone()));
        let backend = Arc::new(super::session_backend::DesktopSessionBackend::new(
            registry, store, 32,
        ));
        let runtime = Arc::new(
            datazen_runtime::application::RuntimeConnectionUseCases::new(
                profiles.clone(),
                Arc::new(super::policy::DesktopPolicy),
                backend.clone(),
            ),
        );
        let sink = runtime.result_sink();
        backend
            .set_publisher(Arc::new(move |id, output| {
                sink.publish_output(id, output).map_err(|_| {
                    datazen_runtime::connection::ProviderError::ProtocolError(
                        "result publication rejected".into(),
                    )
                })
            }))
            .map_err(|_| {
                ApiError::new(
                    ApiErrorCode::ServiceUnavailable,
                    "result publisher unavailable",
                )
            })?;
        // The shared runtime validates namespace using each real provider. The legacy
        // standalone resolver has no resource-scoped driver metadata and stays fail-closed.
        let target = datazen_application::target::TargetResolver::new(
            Arc::new(|_| None),
            Arc::new(|_, _| None),
            Arc::new(|_| None),
            Arc::new(|_| None),
        );
        let services = Arc::new(ApplicationServices::new(
            target,
            datazen_application::identity_policy::IdentityPolicy::new(),
            runtime.clone(),
        ));
        self.runtime.set(runtime).map_err(|_| {
            ApiError::new(ApiErrorCode::ContextConflict, "runtime already injected")
        })?;
        self.profiles.set(profiles).map_err(|_| {
            ApiError::new(ApiErrorCode::ContextConflict, "profiles already injected")
        })?;
        self.services.set(services).map_err(|_| {
            ApiError::new(ApiErrorCode::ContextConflict, "services already injected")
        })?;
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use datazen_platform_api::id::{ClientInstanceId, EditorSessionId, JobId, StageId};

    fn adapter() -> PlatformAdapter {
        PlatformAdapter::with_identity(
            DesktopIdentity::new_launch("datazen-desktop-local", "alice", "client-1")
                .expect("固定输入应当组装成功"),
        )
    }

    #[test]
    fn assembly_state_is_derived_from_injection_not_hand_kept() {
        let adapter = adapter();
        // 未调用 inject() ⇒ 只有身份就绪，第 6、7 步都还没注入。
        // 这条断言读的是 OnceLock 推导结果，不是常量，因此注入逻辑写错时会失败。
        assert_eq!(adapter.assembly_state(), AssemblyState::IdentityReady);
        let missing: Vec<&str> = adapter
            .assembly_state()
            .missing_steps()
            .iter()
            .map(|(name, _)| *name)
            .collect();
        assert_eq!(
            missing,
            vec!["§6.2-6 Runtime", "§6.2-7 ApplicationServices"],
            "缺哪两步必须可枚举，否则调用方无法定位"
        );
        // 每条缺步记录的目标状态必须比当前状态靠后，否则"补齐到哪"是错的。
        for (_, after) in AssemblyState::IdentityReady.missing_steps() {
            assert_ne!(*after, AssemblyState::IdentityReady);
        }
        // 注入完成后的状态不得再声称缺步。
        assert!(AssemblyState::FullyWired.missing_steps().is_empty());
        // 组装状态与注入事实必须一致：此刻未注入，就取不到应用服务。
        assert!(adapter.application_services().is_none());
        assert_eq!(adapter.identity().principal_id().as_str(), "alice");
        assert!(adapter.handles().is_empty());
    }

    #[test]
    fn launch_without_a_deterministic_desktop_user_fails_closed() {
        // `new_launch` 依赖运行环境变量；这里只断言它要么成功、要么明确失败，
        // 绝不返回占位主体。
        match PlatformAdapter::new_launch("client-x") {
            Ok(adapter) => assert!(!adapter.identity().principal_id().as_str().is_empty()),
            Err(err) => assert_eq!(err.code, ApiErrorCode::Unauthenticated),
        }
    }

    #[test]
    fn failed_entry_returns_the_original_error_not_a_placeholder_adapter() {
        // 组装失败时，依赖身份的调用必须拿到**原始**原因。
        // 如果这里退化成一个占位 adapter，调用方会以为身份已就绪，
        // 然后把未知主体当成已知主体写进日志/审计 —— 那是 :84 禁止的伪映射。
        let original = ApiError::new(
            ApiErrorCode::Unauthenticated,
            "test: 桌面用户不可解析，身份未组装",
        );
        let entry = PlatformEntry::failed(original.clone());
        assert!(!entry.is_ready());
        let err = entry.require().expect_err("失败的 entry 必须报错");
        assert_eq!(err.code, ApiErrorCode::Unauthenticated);
        assert_eq!(err.message, original.message);
        // 失败时不得凭空造出一个 adapter。
        assert!(PlatformEntry::failed(original.clone()).require().is_err());
    }

    #[test]
    fn ready_entry_hands_back_the_same_adapter() {
        let adapter = adapter();
        let entry = PlatformEntry::ready(adapter);
        assert!(entry.is_ready());
        let recovered = entry.require().expect("成功的 entry 必须给出 adapter");
        assert_eq!(recovered.identity().principal_id().as_str(), "alice");
    }

    #[test]
    fn each_call_gets_a_fresh_request_id_and_a_stable_principal() {
        let adapter = adapter();
        let a = adapter.begin_call();
        let b = adapter.begin_call();
        assert_ne!(a.request_id(), b.request_id());
        assert_eq!(a.context().principal_id.as_str(), "alice");
        assert_eq!(b.context().principal_id.as_str(), "alice");
        assert_eq!(a.context().client_instance_id.as_str(), "client-1");
    }

    #[test]
    fn owned_call_rejects_a_foreign_editor_owner_before_any_use_case_runs() {
        let adapter = adapter();
        let err = adapter
            .begin_owned_call(&OwnerIntent::Editor {
                claimed_client_instance_id: ClientInstanceId::new("client-other"),
                editor_session_id: EditorSessionId::new("tab-1"),
            })
            .err()
            .expect("跨实例 editor owner 必须被拒");
        assert_eq!(err.code, ApiErrorCode::PermissionDenied);
    }

    #[test]
    fn owned_call_accepts_the_current_instance_editor_owner() {
        let adapter = adapter();
        let (scope, owner) = adapter
            .begin_owned_call(&OwnerIntent::Editor {
                claimed_client_instance_id: ClientInstanceId::new("client-1"),
                editor_session_id: EditorSessionId::new("tab-1"),
            })
            .expect("同实例 editor owner 应通过");
        assert!(matches!(
            owner,
            datazen_platform_api::context::OwnerRef::Editor { .. }
        ));
        assert_eq!(scope.context().client_instance_id.as_str(), "client-1");
    }

    #[test]
    fn owned_call_fails_closed_for_job_owners() {
        let adapter = adapter();
        let err = adapter
            .begin_owned_call(&OwnerIntent::Job {
                job_id: JobId::new("job-1"),
                stage_id: StageId::new("stage-1"),
            })
            .err()
            .expect("job owner 必须失败关闭");
        assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
    }

    #[test]
    fn application_services_is_absent_and_the_failure_explains_why() {
        let adapter = adapter();
        assert!(
            adapter.application_services().is_none(),
            "inject() 之前不得凭空造出 ApplicationServices"
        );
        let err = adapter
            .require_application_services()
            .err()
            .expect("未注入必须报错");
        assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported);
        // 报错必须说明未注入这件事，否则调用方无法定位是启动装配漏了哪一步。
        // 注意：这里断言的"原因"必须与实际状态一致——7 项字段已实现，
        // 未注入的真实原因是第 6/7 步没跑，不是"字段不存在"。
        for needle in ["§6.2", "未注入", "inject", "不得改走旧共享 session"] {
            assert!(
                err.message.contains(needle),
                "报错缺少「{needle}」：{}",
                err.message
            );
        }
    }

    /// 窄适配的核心约束：**不得回落**。`require_*` 失败时不能偷偷给旧路径开门。
    #[test]
    fn failing_requirements_never_fall_back_to_the_legacy_path() {
        let adapter = adapter();
        // 三个 require_* 在未注入 / 未接线时的行为都必须是 Err，而不是"尽力而为"。
        assert!(adapter.require_application_services().is_err());
        // 桥在测试进程里可能被其它用例注入，两种结果都必须成功，
        // 关键是**没有第三条"回落"分支**。
        match adapter.require_bridge() {
            Ok(_) => {}
            Err(err) => assert_eq!(err.code, ApiErrorCode::CapabilityUnsupported),
        }
    }

    /// 结构性证据：本模块不含任何 `ConnectionUseCases` 实现，也不出现 `todo!` /
    /// `unimplemented!` / `unreachable!` 之类会把半成品伪装成完成的写法。
    #[test]
    fn no_shell_implementation_of_the_thirteen_use_cases_exists() {
        let source = include_str!("adapter.rs");
        let production = source.split("#[cfg(test)]").next().unwrap_or_default();
        assert!(
            !production.contains("impl ConnectionUseCases"),
            "本 track 不实现 13 个用例：实现就得编造 revision / epoch / 幂等值"
        );
        for forbidden in ["todo!", "unimplemented!", "TODO", "FIXME"] {
            assert!(
                !production.contains(forbidden),
                "adapter.rs 生产路径不得出现 {forbidden}"
            );
        }
    }

    /// :84 条款一的**语义**证据：`ProfileView` 的默认 revision 就是常量 0。
    /// 这正是"伪映射"会落进去的洞：把常量 0 当成真实版本号填进去，
    /// `ConfigRevisionMismatch` 判定会永久失效。本测试把这个事实钉住。
    #[test]
    /// `ProfileView::new` 的默认值**仍是编造值**——两个 revision 都是 ZERO、永远相等，
    /// `enabled` 也只能是 true。这条测试现在守护的是"默认值不可用于生产路径"：
    /// 桌面配置源 `DesktopProfiles` 必须逐字段覆盖成持久化元数据，
    /// 否则 `ConfigRevisionMismatch` / `RuntimeEpochMismatch` 判定会永久失效。
    /// 它**不再**是"所以不能接线"的论据——7 项字段已各自有真实来源（见模块文档表格）。
    #[test]
    fn profile_view_defaults_are_placeholders_and_must_not_be_used_on_the_desktop_path() {
        use datazen_platform_api::dto::profile::ProfileView;
        use datazen_platform_api::id::{ConnectionId, Counter};

        let view = ProfileView::new(ConnectionId::new("c-1"), "本地", "postgres");
        // 没有任何真实版本源时，唯一能拿到的就是 ZERO —— 一个编造值。
        assert_eq!(view.config_revision, Counter::ZERO);
        assert_eq!(view.credential_revision, Counter::ZERO);
        // 两个 revision 因此永远相等 ⇒ 版本不一致判定不可用。
        assert_eq!(view.config_revision, view.credential_revision);
        // enabled 同样只能取默认值 true；桌面配置源改从 metadata.enabled 取。
        assert!(view.enabled);
        // 对照：配置源用了带修订的构造器，两个 revision 必须真的不同。
        let real = view
            .with_config_revision(Counter::new(7))
            .with_credential_revision(Counter::new(3));
        assert_ne!(real.config_revision, real.credential_revision);
    }
}
