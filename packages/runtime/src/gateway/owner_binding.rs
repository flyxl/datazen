//! 会话归属绑定（网关层的落点）。
//!
//! ## 这个模块解决什么
//!
//! 执行网关是**唯一**能把「一个会话句柄」变成「一次驱动下发」的地方，因此三条验收项
//! 在这里必须同时成立：
//!
//! * **禁止配置 ID 回退**：`connectionId`（持久化配置 id）在类型层就**不能**指称一个会话。
//!   会话由 [`SessionHandle`] = `{dbSessionId, runtimeEpoch}` 命名，注册表 `locate()` 也只按
//!   `dbSessionId` 查表（`registry/registry.rs`）。拿配置 id 当句柄查表，必然落进
//!   [`RuntimeError::UnknownSession`] 这一个出口。
//! * **跨用户/组织资源访问**：调用方**不能观察资源是否存在**。
//! * **前端伪造 owner**：请求体里的身份字段**不能覆盖** `RequestContext`。
//!
//! ## 关键设计：拒绝的可见性由拒绝的**依据**决定
//!
//! 一个容易被忽略的事实是：**任何依赖「这条会话存在」的判定，其失败信息都是存在性预言机**。
//! `session_closed` 只有在会话存在且已关闭时才可能产生；`permission_denied` 只有在会话存在
//! 且归属不符时才可能产生。攻击者只要能区分这两者与 `session_not_found`，就能把别人的
//! `dbSessionId` 逐个探测出来。因此本模块把三条判定按依据分成两类：
//!
//! | 依据 | 是否泄露存在性 | 投影 |
//! |------|----------------|------|
//! | 会话不在表里（`UnknownSession`） | 否 | `RuntimeError::UnknownSession`（逐字） |
//! | 会话存在但**归属不符**（`OwnerRef` 比对） | **是** | **同一个** `RuntimeError::UnknownSession` |
//! | 会话存在、归属正确、但**策略撤销/能力不支持** | 否 | `PermissionDenied`（透明） |
//! | 会话存在、归属正确、但**预算耗尽/超时隔离** | 否 | 端口错误逐字上抛 |
//!
//! `tests/owner_binding.rs` 用**可执行断言**把这个表钉死：同一主体对「不存在的句柄」与
//! 「别人的句柄」拿到的是**逐字段相等**的 `GatewayError`，落盘投影 `to_persistable_json()`
//! 的 JSON 也**逐字节相等**，且两次的驱动 execute / cancel 调用次数都是 0。
//!
//! ## 顺序不可调换：先判归属，再判状态
//!
//! [`resolve_owned_session`] 的三步固定为
//! `session_view` → **授权** → **终态拒绝**。把终态拒绝放在授权**之前**是错的：
//! `reject_terminal_session` 会为「已关闭的他人会话」产出 `sessionClosed`，而对不存在的句柄
//! 产出的是 `sessionNotFound`——这正好是一条现成的存在性预言机。授权先过，归属不符的会话
//! 根本走不到终态分支。
//!
//! ## 边界：`WorkflowBlock` 与 `ClientSession` 恒拒
//!
//! [`OwnerRef`] 有四个变体，其中 [`OwnerRef::WorkflowBlock`]（只有 `runId`/`blockId`）与
//! [`OwnerRef::ClientSession`]（只有 `clientInstanceId`）**结构上就没有** `organizationId` /
//! `principalId`。`ClientSession` 的文档甚至写明 `ClientInstanceId`「**不是授权依据**」。
//! 对这两种 owner，本模块**一律拒绝**（fail-closed）：把「无法归属」当成「已归属」放行，
//! 等于让任何持有句柄的人执行别人的工作流块。要让它们可用，必须先在 `OwnerRef` 上补齐组织
//! 与主体字段——那是契约层的改动，不在网关内单方面决定。
//!
//! ## 接线要求：生产组装必须显式传 `OwnerMatchAuthorizer::shared()`
//!
//! 本模块是网关层的落点，但**它自己不会被自动用上**：网关的作者器是构造参数，
//! 不是默认实现（`gateway/mod.rs` 的 `ExecutionGateway::new(port, authorizer, store, clock)`
//! 里 `authorizer: Arc<dyn Authorizer>` 是**必填的位置参数，没有 `Default`**）。
//! 第一个把网关接进生产路径的人如果随手传了恒定放行的替身（`provenance.rs` 的
//! `AlwaysAllow`，它经 `gateway/mod.rs` 的 `pub use` 对外可达，**任何 crate 都拿得到**），
//! 编译**照过**、行为**静默失效**：归属闸门对所有人放行，这三条在运行期形同虚设，
//! 而 `tests/owner_binding.rs` 仍然全绿（它自己传的就是对的作者器）。
//!
//! 因此本轨把接线要求写成两条**可执行**的事实，而不只是一句提醒：
//!
//! 1. **本模块头就是要求**：生产接线必须显式传 `OwnerMatchAuthorizer::shared()`；
//!    不得传 `AlwaysAllow` / `AlwaysDeny` 家族。
//! 2. **守卫**：`tests/owner_binding.rs` 的 `production_wiring` 子模块扫描**全仓** Rust
//!    源码，剥掉注释与字符串内容、`#[cfg(test)]` 块与 import 后，只要任何**生产**文件里
//!    还出现 `AlwaysAllow` / `AlwaysDeny`，门禁立刻变红，并指名那个文件和那一行。
//!    今天扫描是空的（全仓命中全在测试与定义点），
//!    **第一个生产接线者**就是触发它的人。
//!    守卫自证不成立的方式有两处，都实测过红：
//!    (a) 合成反例——内存夹具，见 `production_wiring.rs` 模块头的「反证」一节；
//!    (b) **落在真实文件上的探针**——往 `packages/runtime/src/gateway/mod.rs` 的
//!    `pub(crate) mod testing_support;` 之后插入一行 `pub fn
//!    planted_authorizer_for_guard_bypass() -> … { …AlwaysAllow) }`，
//!    跑 `cargo test -p datazen-runtime --test owner_binding`，
//!    报 `packages/runtime/src/gateway/mod.rs`，退出码 101。
//!    （`request.rs` **不是**反例：实测它没有任何 `AlwaysAllow`/`AlwaysDeny` 命中。）
//!
//! 缓解程度要说准：`authorizer` 无默认值 ⇒ 漏传是**编译错误**（不会静默降级），
//! 唯一的静默形态是「传了个恒放行的替身」——也就是上面这条守卫要对住的那一种。
//!
//! ## 未闭合项登记（随本轨一起合并，删除本文件也不消失）
//!
//! **job 半边 = PARTIAL。** 验收要求是「U1 请求 owner 指向 U2 的 editor **或 job**」。
//! editor 半边本模块闭合；
//! job 半边**本层合不上**：`OwnerRef::Job` 只有 `organization_id` / `job_id` / `stage_id`，
//! **不含 `principal_id`**（`connection/types.rs`），同组织的 U1 与 U2 在该变体上**完全同形**，
//! `Authorizer` 的入参里也没有「本次请求被授权操作哪个 job」。
//!
//! **上层没有兜底。** `packages/application/src/identity_policy.rs` 的 `check_owner`
//! 常被当成这个缺口的补法，但实测它**全仓没有任何生产调用点**（只有定义本身、几处文档
//! 引用、以及它自己文件内的单测），`src-tauri/src/platform/identity.rs` 也不调用它；
//! 而且它比的是调用方**显式传入**的 `authorized_job` 与 `owner.job_id`，那是「这个 job 有没有
//! 被授权」而不是「这是不是同一个人」，与跨用户语义**不是同一件事**。本轨不修改
//! 该文件，也不把它算作覆盖。行为已由 `tests/owner_binding.rs` 的
//! `cm06_a_job_owner_carries_no_principal_so_same_organization_peers_pass_the_gate` 钉死。
//!
//! **六个接口的处置**（结论落在 `tests/owner_binding.rs` 文件头的表里）：
//! 执行 **CLOSED**；取消 **PARTIAL**；读取 / 关闭 **N/A**（网关动作面恰好只有
//! `Execute` 与 `Cancel`，由 `cm05_the_gateway_action_surface_is_exactly_execute_and_cancel`
//! 在编译期钉住）；订阅 / 下载 **不属于本轨**，归 P7。
//!
//! ## 契约债：仓库里有**两个同名** `OwnerRef`
//!
//! `crate::connection::OwnerRef`（`packages/runtime/src/connection/types.rs`）与
//! `datazen_platform_api::context::OwnerRef`（`packages/platform-api/src/context.rs`）
//! **变体名一模一样，字段类型不一样，且没有任何一侧校验另一侧**：
//!
//! | | 网关侧（本模块） | 应用层 |
//! |---|---|---|
//! | `Editor` | `organization_id` + `principal_id` + … | 只有 `client_instance_id` |
//! | `Job` | `stage_id: String` | `stage_id: crate::id::StageId` |
//! | `ClientSession` | 只有 `client_instance_id` | 多一个 `purpose: String` |
//!
//! 后果：应用层的 `check_owner` 在**结构上看不到组织与主体**（那两层的 `Editor`/`Job`
//! 都没有这两个字段），于是本模块的归属语义与它的语义可以**各改各的、互不报警**。
//! 本轨**不统一这两个类型**（跨 crate，会把契约改动混进网关改动），只把这条债登记在此，
//! 并在 `identity_policy.rs` 的 `check_owner` 旁留同样的标记。
//!
//! 另外 [`OwnerRef`] 本身在本轨是**冻结**的：给 `OwnerRef::Job` 补 `principal_id` 会同时
//! 改变契约层语义，必须由有权限的人显式决定，不能由网关单方面加字段。

use std::sync::Arc;

use datazen_platform_api::id::{OrganizationId, PrincipalId};

use crate::connection::{
    Counter, OwnerRef, RuntimeError, SessionHandle, SessionState, SessionView,
};
use crate::gateway::provenance::{
    AuthorizationDenial, Authorizer, ExecutionSource, GatewayAction, RequestPrincipal,
};
use crate::gateway::request::GatewayError;
use crate::registry::SessionPort;

/// 组织归属不符。稳定字面量，供审计聚合。
pub const OWNER_ORGANIZATION_MISMATCH: &str = "ownerOrganizationMismatch";
/// 主体归属不符。稳定字面量，供审计聚合。
pub const OWNER_PRINCIPAL_MISMATCH: &str = "ownerPrincipalMismatch";
/// owner 变体在结构上不携带组织/主体，无法判定归属。稳定字面量，供审计聚合。
pub const OWNER_KIND_UNATTRIBUTED: &str = "ownerKindUnattributed";

/// 会话归属授权器。
///
/// 这是**生产用**的 [`Authorizer`]，不是 `AlwaysAllow` 家族的测试替身：它按
/// [`SessionView::owner`] 的 `organization_id` 与 `principal_id` **逐字段**比对
/// [`RequestPrincipal`]，两者都必须成立。缺一个都不放行——只看 principal 会让同一个人
/// 跨组织拿到别的租户的会话，只看 organization 会让组织内任意成员拿到别人的编辑器会话。
///
/// 拒绝一律走 [`AuthorizationDenial::hidden`]，因为「归属不符」这件事本身就是存在性证据
/// （见模块头可见性表）。
#[derive(Debug, Clone, Copy, Default)]
pub struct OwnerMatchAuthorizer;

impl OwnerMatchAuthorizer {
    /// 构造 `Arc`，便于注入 [`crate::gateway::ExecutionGateway`]。
    pub fn shared() -> Arc<Self> {
        Arc::new(Self)
    }
}

impl Authorizer for OwnerMatchAuthorizer {
    fn authorize(
        &self,
        principal: &RequestPrincipal,
        action: GatewayAction,
        view: &SessionView,
        _source: &ExecutionSource,
    ) -> Result<(), AuthorizationDenial> {
        // 先比组织、再比主体：顺序只影响拒绝理由的字面量，不影响放行结论
        // （两个条件是合取，必须全部成立）。先报组织不符，是为了让「跨租户」这一类
        // 攻击在审计里聚成一条，而不是被拆成 N 条 principalMismatch。
        match &view.owner {
            OwnerRef::Editor {
                organization_id,
                principal_id,
                ..
            } => {
                check_organization(organization_id, principal, action)?;
                check_principal(principal_id, principal, action)
            }
            OwnerRef::Job {
                organization_id, ..
            } => check_organization(organization_id, principal, action),
            // 结构上没有归属字段 ⇒ 无法判定 ⇒ 拒绝（模块头「边界」一节）。
            OwnerRef::WorkflowBlock { .. } | OwnerRef::ClientSession { .. } => {
                Err(AuthorizationDenial::hidden(action, OWNER_KIND_UNATTRIBUTED))
            }
        }
    }
}

fn check_organization(
    owner: &OrganizationId,
    principal: &RequestPrincipal,
    action: GatewayAction,
) -> Result<(), AuthorizationDenial> {
    if owner != principal.organization_id() {
        return Err(AuthorizationDenial::hidden(
            action,
            OWNER_ORGANIZATION_MISMATCH,
        ));
    }
    Ok(())
}

fn check_principal(
    owner: &PrincipalId,
    principal: &RequestPrincipal,
    action: GatewayAction,
) -> Result<(), AuthorizationDenial> {
    if owner != principal.principal_id() {
        return Err(AuthorizationDenial::hidden(
            action,
            OWNER_PRINCIPAL_MISMATCH,
        ));
    }
    Ok(())
}

/// 终态会话不是「已受理」。端口对 `Closed`/`Lost` 会话**返回 `Ok`**，
/// 漏掉这一步等于把一个已经关闭的会话的执行判成功。
///
/// 调用点必须排在 [`resolve_owned_session`] 的授权之后（模块头「顺序不可调换」）。
pub(crate) fn reject_terminal_session(view: &SessionView) -> Result<(), GatewayError> {
    match view.state {
        SessionState::Closed | SessionState::Closing => {
            Err(RuntimeError::SessionClosed(view.handle.db_session_id.as_str().to_owned()).into())
        }
        SessionState::Lost => {
            Err(RuntimeError::SessionLost(view.handle.db_session_id.as_str().to_owned()).into())
        }
        _ => Ok(()),
    }
}

/// 「这条句柄在本主体名下是否存在且可用」的**唯一**判定入口。
///
/// 三步固定顺序，理由见模块头：
///
/// 1. `session_view` —— 找不到 ⇒ `UnknownSession`（逐字上抛，调用方分不出与归属拒绝的区别）。
/// 2. **授权** —— 归属不符 ⇒ 隐藏拒绝，投影成**同一个** `UnknownSession`。
/// 3. **终态拒绝**（仅 `GatewayAction::Execute`）—— 归属已成立，此时才有资格告诉调用方
///    「你的会话已经关闭了」。`Cancel` 不做终态拒绝：一次已入队执行的取消请求，其终态信息
///    即使会话随后关闭仍然有意义。
/// 4. **CAS 修订号**（`expected_revision` 给了才查）——下发闸门的乐观并发复核。
pub(crate) async fn resolve_owned_session(
    port: &dyn SessionPort,
    authorizer: &dyn Authorizer,
    principal: &RequestPrincipal,
    action: GatewayAction,
    handle: &SessionHandle,
    source: &ExecutionSource,
    expected_revision: Option<Counter>,
) -> Result<SessionView, GatewayError> {
    // 步骤 1：会话投影。失败集的处理见 `match` 的逐分支注释。
    let view = match port.session_view(handle).await {
        Ok(view) => view,
        // 端口契约把 `session_view` 的失败集钉死为这一项（`registry/port.rs` 文档），
        // 逐字上抛：调用方看到的形状与端口完全一致，网关不得给端口错误换皮。
        Err(error @ RuntimeError::UnknownSession(_)) => return Err(GatewayError::Runtime(error)),
        // 实现细节补洞：actor 的回复通道在请求途中断掉时，`actor.rs` 的
        // `unwrap_or(Err(SessionClosed(..)))` 会吐出 `SessionClosed`——它不在文档失败集里，
        // 但同样是「这条会话存在」的证据。合并进 `UnknownSession`，否则一条已死 actor 的
        // 别人会话就会与不存在的句柄给出不同的错误码。
        Err(RuntimeError::SessionClosed(_)) => return Err(GatewayError::Runtime(unknown(handle))),
        // 其余端口错误逐字上抛：预算耗尽、会话隔离、会话丢失都不是存在性信号，
        // 且必须保留诊断价值——把它们一并抹成 `UnknownSession` 会让现场无法排障。
        Err(other) => return Err(GatewayError::Runtime(other)),
    };

    // 步骤 2：归属。隐藏拒绝与步骤 1 的失败投影成同一个错误，攻击者无从区分。
    if let Err(denial) = authorizer.authorize(principal, action, &view, source) {
        return Err(project_denial(denial, handle));
    }

    // 步骤 3：终态。只在归属已成立后才有资格暴露。
    if action == GatewayAction::Execute {
        reject_terminal_session(&view)?;
    }

    // 步骤 4：CAS 修订号（只有下发闸门给值）。同属归属成立之后的信息，
    // 因此不是存在性预言机——走到这里调用方已经证明自己持有这条会话。
    if let Some(expected) = expected_revision {
        if expected != view.context_revision {
            return Err(RuntimeError::ContextRevisionMismatch {
                expected: expected.get(),
                actual: view.context_revision.get(),
            }
            .into());
        }
    }

    Ok(view)
}

/// 隐藏拒绝 ⇒ 与「句柄不存在」**逐字段相同**的哨兵错误。
///
/// 哨兵的载荷用**请求方给出的** `dbSessionId`，与注册表 `locate()` 构造
/// `UnknownSession(db_session_id.to_string())` 的方式一致（`registry/registry.rs`）。
fn project_denial(denial: AuthorizationDenial, handle: &SessionHandle) -> GatewayError {
    if denial.is_hidden() {
        return GatewayError::Runtime(unknown(handle));
    }
    GatewayError::from(denial)
}

fn unknown(handle: &SessionHandle) -> RuntimeError {
    RuntimeError::UnknownSession(handle.db_session_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::testing_support as fx;
    use datazen_platform_api::id::DbSessionId;

    fn principal(principal_id: &str, organization_id: &str) -> RequestPrincipal {
        RequestPrincipal::new(
            PrincipalId::new(principal_id),
            OrganizationId::new(organization_id),
            DbSessionId::new("dbs_org_session_fixture"),
        )
    }

    fn source() -> ExecutionSource {
        ExecutionSource::new(
            crate::gateway::provenance::SourceKind::Editor,
            "edt_a",
            None,
            None,
        )
    }

    /// 归属成立（`principal_a` / `org_a`）的会话投影。
    fn owned_view() -> SessionView {
        fx::ready_view(
            DbSessionId::new("dbs_owner_binding"),
            Counter::new(1),
            Counter::new(1),
        )
    }

    #[test]
    fn the_true_owner_is_allowed_but_a_peer_principal_is_not() {
        let authorizer = OwnerMatchAuthorizer;
        let view = owned_view();

        assert!(authorizer
            .authorize(
                &principal("principal_a", "org_a"),
                GatewayAction::Execute,
                &view,
                &source()
            )
            .is_ok());

        // 同组织内的**他人**主体：组织过了、主体没过 ⇒ 拒。
        let peer = authorizer
            .authorize(
                &principal("principal_b", "org_a"),
                GatewayAction::Execute,
                &view,
                &source(),
            )
            .expect_err("同组织他人主体必须被拒");
        assert_eq!(peer.reason, OWNER_PRINCIPAL_MISMATCH);
        // 归属类判定必须隐藏存在性，否则就是一条现成的存在性预言机。
        assert!(peer.is_hidden(), "归属拒绝不得对调用方可见");
    }

    #[test]
    fn a_foreign_organization_is_denied_even_when_the_principal_string_matches() {
        let authorizer = OwnerMatchAuthorizer;
        let view = owned_view();

        // 主体字符串与 owner 相同，但组织不同 ⇒ 必须拒（只看主体会跨租户放行）。
        let cross_tenant = authorizer
            .authorize(
                &principal("principal_a", "org_b"),
                GatewayAction::Execute,
                &view,
                &source(),
            )
            .expect_err("跨组织必须被拒");
        assert_eq!(cross_tenant.reason, OWNER_ORGANIZATION_MISMATCH);
        assert!(cross_tenant.is_hidden());
    }

    #[test]
    fn a_job_owner_is_bound_by_organization_only() {
        let authorizer = OwnerMatchAuthorizer;
        let mut view = owned_view();
        view.owner = OwnerRef::Job {
            organization_id: OrganizationId::new("org_a"),
            job_id: datazen_platform_api::id::JobId::new("job_a"),
            stage_id: "stage_a".to_owned(),
        };

        // job owner 结构上没有 principal 字段 ⇒ 同组织放行是契约，不是不校验。
        assert!(authorizer
            .authorize(
                &principal("principal_b", "org_a"),
                GatewayAction::Execute,
                &view,
                &source()
            )
            .is_ok());

        let foreign = authorizer
            .authorize(
                &principal("principal_b", "org_b"),
                GatewayAction::Execute,
                &view,
                &source(),
            )
            .expect_err("跨组织必须被拒");
        assert_eq!(foreign.reason, OWNER_ORGANIZATION_MISMATCH);
        assert!(foreign.is_hidden());
    }

    #[test]
    fn owner_kinds_without_identity_fields_are_denied_outright() {
        let authorizer = OwnerMatchAuthorizer;
        let mut view = owned_view();

        view.owner = OwnerRef::WorkflowBlock {
            run_id: datazen_platform_api::id::RunId::new("run_a"),
            block_id: datazen_platform_api::id::BlockId::new("block_a"),
        };
        let block = authorizer
            .authorize(
                &principal("principal_a", "org_a"),
                GatewayAction::Execute,
                &view,
                &source(),
            )
            .expect_err("结构上无归属字段的 owner 不得放行");
        assert_eq!(block.reason, OWNER_KIND_UNATTRIBUTED);

        view.owner = OwnerRef::ClientSession {
            client_instance_id: datazen_platform_api::id::ClientInstanceId::new("cli_a"),
        };
        let client = authorizer
            .authorize(
                &principal("principal_a", "org_a"),
                GatewayAction::Execute,
                &view,
                &source(),
            )
            .expect_err("结构上无归属字段的 owner 不得放行");
        assert_eq!(client.reason, OWNER_KIND_UNATTRIBUTED);

        assert!(block.is_hidden() && client.is_hidden());
    }

    /// `session_view` 的失败集被端口契约钉死为 `UnknownSession` 一项（外加 actor 通道断开时
    /// 实现方补出的 `SessionClosed`）。这条守卫把那份契约变成机器可执行的事实：一旦实现方
    /// 开始从 `session_view` 抛别的错误，`resolve_owned_session` 的「全部存在性信号已归一」
    /// 就不再成立，测试必须红。
    #[test]
    fn the_session_view_contract_declares_exactly_one_failure_value() {
        let port_src = include_str!("../registry/port.rs");
        let doc = doc_comment_before(port_src, "async fn session_view")
            .expect("守卫空转：在 registry/port.rs 里找不到 session_view 的文档注释");

        let mut values: Vec<String> = Vec::new();
        let mut rest = doc.as_str();
        while let Some(at) = rest.find("RuntimeError::") {
            rest = &rest[at + "RuntimeError::".len()..];
            let name: String = rest
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            assert!(
                !name.is_empty(),
                "守卫空转：`RuntimeError::` 后面没有标识符"
            );
            if !values.contains(&name) {
                values.push(name);
            }
        }
        assert_eq!(
            values,
            vec!["UnknownSession".to_owned()],
            "session_view 的文档失败集不再只有 UnknownSession，resolve_owned_session 的存在性归一覆盖不全"
        );
    }

    /// 取 `needle` 之前**紧邻**的整块 `///` 文档注释。
    ///
    /// 逐行向前回溯，只认连续的 `///` 行（允许块内的空行），碰到任何其它行就停。
    /// 找不到返回 `None`，由调用方决定是否判为守卫空转。
    fn doc_comment_before(src: &str, needle: &str) -> Option<String> {
        let at = src.find(needle)?;
        let before = &src[..at];
        let lines: Vec<&str> = before.lines().collect();
        let mut start = lines.len();
        while start > 0 {
            let trimmed = lines[start - 1].trim();
            if trimmed.is_empty() || trimmed.starts_with("///") {
                start -= 1;
            } else {
                break;
            }
        }
        // 全篇都是文档（没有终止行）说明锚点找错了位置，同样判为空转。
        if start == 0 || lines[start..].iter().all(|line| line.trim().is_empty()) {
            return None;
        }
        Some(lines[start..].join("\n"))
    }
}
