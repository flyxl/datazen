//! CM-04 / CM-05 / CM-06 的 H 层门禁：归属绑定必须在**公开 API** 这一层就成立。
//!
//! 刻意不复用 `tests/gateway_fixtures/mod.rs`，也刻意不复用
//! `src/gateway/testing_support.rs`：这里只用
//! `datazen_runtime::gateway::*` + 冻结 DTO + `datazen_runtime::registry::SessionPort`
//! 自己搭一套。理由同上——集成测试能碰到的东西比 `--lib` 少，它能独立证明的
//! 东西也不同；一旦共用夹具，「集成测试验证了真实公开 API」这件事就失真了。
//!
//! 夹具里的会话全是**合成**数据，不含任何凭据或真实连接信息，ID 一律带 `-ob` 后缀。
//!
//! ## 为什么端口是「按句柄脚本化」的
//!
//! `gateway_fixtures::RecordingPort` 的 `set_view` 是**全局**的：整个端口只有
//! 一个视图，换句柄只能连视图一起换。这做不了 CM-05 —— CM-05 要的恰恰是
//! **同一时刻并存**「U1 的会话」「U2 的会话」「不存在的句柄」三种世界，让网关
//! 依次撞上去。所以这里自己写一个按 `db_session_id` 查表的端口：查得到就返回
//! 那个会话，查不到就返回 `UnknownSession`，**与
//! `SessionRegistry::session_view` → `locate()` 的行为逐条对齐**
//! （终态会话仍然返回 `Ok`，这一点也照抄）。
//!
//! ## 「不存在」与「不归我」必须完全无法区分
//!
//! CM-05 的核心断言是「不能观察资源存在性」。在本层，这条断言可以被钉成
//! **可执行**的字节级事实，因为 [`GatewayError::to_persistable_json`] 是审计落盘
//! 形态，**刻意不包含任何句柄**（`request.rs` 原话），而
//! [`RuntimeError::reason`] 对 `UnknownSession` 返回稳定字面量 `"unknownSession"`、
//! 把载荷丢掉。于是
//!
//! | 提问 | 解析层投影 |
//! | --- | --- |
//! | 句柄不存在 | `{"kind":"runtime","reason":"unknownSession","apiCode":"sessionNotFound"}` |
//! | 句柄存在但归属不符 | 同上，逐字节相同 |
//!
//! 这不是「隐藏一下」的口头承诺，而是测试里的一次 `assert_eq!`。
//!
//! ## CM-05 六个接口的处置（不编接口凑数）
//!
//! CM-05 的步骤列了「读取 / 执行 / 关闭 / 取消 / 订阅 / 下载」六个接口。本层
//! 网关 [`GatewayAction`] 只有 `Execute` 与 `Cancel` 两个取值——下面
//! `cm05_the_gateway_action_surface_is_exactly_execute_and_cancel` 用一个**穷尽
//! `match`** 把这一点做成编译期事实（少一个变体编不过，多一个变体也编不过）。
//! 六个接口里真正落在这层的只有两个，其余必须如实归属，不得凭空捏造。
//! 处置词只有四个：**`CLOSED`（闭合）/ `PARTIAL`（部分闭合，登记剩余缺口）/
//! `N/A`（H 层根本没有这个承接口，不是「漏测」）/ `P7`（归 P7 轨，CM-61 / CM-64）**。
//!
//! | CM-05 接口 | 归属层 | 处置 | 依据 |
//! | --- | --- | --- | --- |
//! | 执行 | 网关 `accept` / `dispatch` | **`CLOSED`** | `cm05_*`、`cm06_*`：归属不符时 `execute_calls() == 0`，投影与不存在逐字节相同 |
//! | 取消 | 网关 `cancel` | **`PARTIAL`** | 驱动次数已钉（`cancel_calls() == 0`，且有真 owner 的正向对照 `== 1`）；但**存在性预言机未闭合**，见下方 ⚠️ |
//! | 读取 | 无网关接口；`session_view` 是端口读路径，只被授权器读，不对外暴露 | **`N/A`** | `cm05_the_gateway_action_surface_is_exactly_execute_and_cancel` 编译期钉死动作面 |
//! | 关闭 | 无网关接口（`SessionPort::close_session` 由注册表演给生命周期管理方，**不经网关鉴权**） | **`N/A`** | 同上；CM-05 的「关闭」在 H 层没有承接口，**不得记作已覆盖** |
//! | 订阅 | 不存在（订阅属 `ArtifactStore`） | **`P7`** | 验收文档 CM-61（`connection-management.md`）已豁免 |
//! | 下载 | 同上 | **`P7`** | 验收文档 CM-64 同上 |
//!
//! ⚠️ **`PARTIAL` 的剩余缺口：取消接口残留一条存在性预言机。**
//! `gateway/mod.rs` 的 `cancel()` 里，执行记录查表发生在授权**之前**：所以
//! 「`executionId` 根本不存在」得到 `CancelFailed("unknownExecution")`
//! （`api_code()` 故意返回 `None`），而「`executionId` 存在但会话不归我」得到
//! `UnknownSession`（`sessionNotFound`）。两个变体、两个对外码，攻击者据此可判定该
//! `executionId` 是否存在。
//!
//! **本轨不改，理由是它有主人**：修它要动 `CancelFailed` 的对外语义，而
//! `tests/gateway_contract/cancel.rs`、`registry_audit.rs`、`registry_cancel.rs`、
//! `p3_session_port_contract.rs` 四份**别的轨**的门禁正逐条钉死该语义（含
//! `api_code()` 返回 `None`、`NotOnTheWire`、`UNKNOWN_EXECUTION` 三处），单轨改动会
//! 同时打穿它们。**归属裁定留给集成时统一裁决**，本轨只登记、不声称闭合。

#[path = "owner_binding/support.rs"]
mod support;
#[path = "owner_binding/production_wiring.rs"]
mod production_wiring;

use datazen_platform_api::error::ApiErrorCode;
use datazen_runtime::connection::RuntimeError;
use datazen_runtime::gateway::{GatewayAction, GatewayError};
use std::sync::Arc;

use support::*;

// ---------------------------------------------------------------------------
// CM-06 的对照与护栏
// ---------------------------------------------------------------------------

/// CM-06 的对照实验：真 owner 仍然能受理。
///
/// 没有这一条，前面所有「拒绝」的断言都可以靠「网关什么都不许干」来蒙混过关。
#[tokio::test]
async fn cm06_the_real_owner_is_still_accepted() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let acceptance = gw
        .accept(
            &u1(),
            request_for(U1_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect("真 owner 必须被放行");

    assert_eq!(
        gw.execution_count().await,
        1,
        "真 owner 应当留下一条执行记录"
    );
    assert_eq!(store.writes(), 1, "真 owner 应当写入一条幂等账本");
    assert_eq!(
        port.execute_calls(),
        0,
        "受理只入队，不下发给驱动；驱动次数为 0 才是这一步该有的样子"
    );
    assert!(!acceptance.execution_id().as_str().is_empty());
}

/// CM-06 的对照实验：同组织、不同主体照样被拒。
///
/// `ORG_O1` 里同时住着 U1 和 U2，所以这一条证明的是**主体**比较真的在跑，
/// 而不是碰巧把整个组织都拒了。
#[tokio::test]
async fn cm06_a_peer_principal_in_the_same_organization_is_denied() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let error = gw
        .accept(
            &u1(),
            request_for(U2_EDITOR_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect_err("同组织的另一个主体必须被拒");

    assert_eq!(api_code(&error), Some(ApiErrorCode::SessionNotFound));
    assert_eq!(gw.execution_count().await, 0);
    assert_eq!(store.writes(), 0);
    assert_eq!(port.execute_calls(), 0);
}

/// CM-06：U2 指向 U1 的 editor 会话 —— 拒绝，且一个会话都不该被创建。
///
/// **这条以前和上一条逐字节相同**（都是 U1 指向 U2），等于把一个用例数了两遍。
/// 现在改成**镜像方向**：真 owner 是 U1，来路是 U2。「谁是被指的」与「谁是攻击者」
/// 换了位置，所以它测的是另一次真实请求，而不只是把 `14 passed` 的计数撑大一个。
#[tokio::test]
async fn cm06_naming_another_users_editor_is_refused() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let error = gw
        .accept(
            &u2(),
            request_for(U1_SESSION, editor_source(PRINCIPAL_U2, ORG_O1)),
        )
        .await
        .expect_err("指向他人 editor 的请求必须被拒");

    assert_eq!(api_code(&error), Some(ApiErrorCode::SessionNotFound));
    assert_eq!(gw.execution_count().await, 0, "被拒的请求不得留下执行记录");
    assert_eq!(store.writes(), 0, "被拒的请求不得写幂等账本");
    assert_eq!(port.execute_calls(), 0, "被拒的请求一个驱动调用都不产生");
}

/// CM-06 的**未闭合半边**：U1 指向 U2 的 job 会话。
///
/// 本层判不了，原因是**类型里就没有可比的主体**：
/// `OwnerRef::Job`（`packages/runtime/src/connection/types.rs:390-394`）的字段
/// 只有 `organization_id` / `job_id` / `stage_id`，不含 `principal_id`。同一个组织
/// 里的 U1 与 U2 在 `OwnerRef::Job` 上**完全同形**，`Authorizer` 拿到的入参里也没有
/// 「本次请求被授权操作哪个 job」这一项，因此任何实现在本层都只能按组织放行。
///
/// 这里把实际行为钉住，而不是假装它被拒了：同组织不同主体的请求**确实会穿过归属
/// 闸门抵达端口**（`execute_calls()` 从 0 变 1，且被替身端口以内部错顶出来）。一旦
/// 有人日后在 `OwnerRef::Job` 上补上主体并接上比较，这条会变红，提醒他把
/// `cm06_a_job_owner_carries_no_principal_so_same_organization_peers_pass_the_gate`
/// 的对偶断言补齐。
///
/// **上层没有兜底。** 仓库里唯一看起来能补这个缺口的
/// `packages/application/src/identity_policy.rs` 的
/// `check_owner(ctx, owner, authorized_job)`，实测**全仓没有任何生产调用点**——只有定义
/// 本身、几处文档引用、以及它自己文件内的单测，`src-tauri` 不调用它。所以它**不是**
/// 覆盖，CM-06 的 job 半边按 **`PARTIAL`** 登记，不是「上层已经挡住」。
/// 即便接上调用方也不对题：它比的是调用方**显式传入**的 `authorized_job` 与
/// `owner.job_id` 是否相等，那是「这个 job 有没有被授权」，不是「这是不是同一个人」，
/// 与 CM-06「U1 指向 U2 的 job」的跨用户语义不是同一件事。
/// 闭合条件是唯一的：给 `OwnerRef::Job` 补上主体字段并接上比较——那是契约层的改动，
/// 本轨**不单方面给 `OwnerRef` 加字段**，只把债登记在
/// `src/gateway/owner_binding.rs` 的模块头与 `identity_policy.rs` 的 `check_owner` 上。
#[tokio::test]
async fn cm06_a_job_owner_carries_no_principal_so_same_organization_peers_pass_the_gate() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    // 先钉住可闭合的那一半：组织不同 ⇒ 拒。
    let cross_org = gw
        .accept(
            &u1_of_o2(),
            request_for(U2_JOB_SESSION, editor_source(PRINCIPAL_U1, ORG_O2)),
        )
        .await
        .expect_err("跨组织的 job 归属必须被拒");
    assert_eq!(api_code(&cross_org), Some(ApiErrorCode::SessionNotFound));
    assert_eq!(port.execute_calls(), 0, "跨组织被拒时驱动调用必须是 0");

    // 再钉住闭合不了的那一半：同组织不同主体 ⇒ 穿过归属闸门，被受理。
    // 注意失败信息刻意不带 `{:?}`：受理回执里含句柄与执行 id，断言文本不得回显它们。
    let accepted = gw
        .accept(
            &u1(),
            request_for(U2_JOB_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await;
    assert!(
        accepted.is_ok(),
        "同组织的 job 会话在本层与 U2 的完全同形，归属闸门判不了；若本条转红，说明 OwnerRef::Job 的字段表变了"
    );
    assert_eq!(
        gw.execution_count().await,
        1,
        "同组织 peer 的请求确实被受理并入队（这是本轨登记的未闭合项，不是期望行为）"
    );
    assert_eq!(
        port.execute_calls(),
        0,
        "受理只入队不下发；驱动阻断点在 dispatch 前，与本条无关"
    );
}

/// CM-06 未闭合半边的**结构守卫**：钉住 `OwnerRef::Job` 的字段表。
///
/// 这条不是注释——有人给 `OwnerRef::Job` 加上 `principal_id` 时本测试立刻变红，
/// 变红信息即要求把上面的对偶断言补齐。字段表从 `types.rs` 原文抠出，不靠记忆。
#[test]
fn cm06_the_job_owner_variant_field_list_is_pinned() {
    let src = include_str!("../src/connection/types.rs");
    let anchor = "    Job {";
    let start = src
        .find(anchor)
        .expect("OwnerRef::Job 必须仍定义在 src/connection/types.rs")
        + anchor.len();

    let mut fields = Vec::new();
    for line in src[start..].lines() {
        let trimmed = line.trim();
        // 变体末尾是 `},`（后面还有别的变体）而不是 `}`，两种都要收。
        if trimmed == "}" || trimmed == "}," {
            break;
        }
        if let Some((name, _)) = trimmed.split_once(':') {
            fields.push(name.to_string());
        }
    }

    assert_eq!(
        fields,
        vec![
            "organization_id".to_string(),
            "job_id".to_string(),
            "stage_id".to_string(),
        ],
        "OwnerRef::Job 的字段表变了：若新增了 principal_id，H 层已具备拒绝「同组织另一个主体的 job」的条件，\n\
         请补上 cm06_a_job_owner_carries_no_principal 的对偶断言并更新本文件头的归属台账"
    );
}

/// CM-06：跨组织的 job 归属同样被拒（`OwnerRef::Job` 的组织维度）。
#[tokio::test]
async fn cm06_a_job_owned_by_another_organization_is_refused() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let error = gw
        .accept(
            &u1_of_o2(),
            request_for(U2_JOB_SESSION, editor_source(PRINCIPAL_U1, ORG_O2)),
        )
        .await
        .expect_err("跨组织的 job 归属必须被拒");

    assert_eq!(api_code(&error), Some(ApiErrorCode::SessionNotFound));
    assert_eq!(port.execute_calls(), 0);
}

/// CM-06：**跨组织**的 editor 归属被拒（`OwnerRef::Editor` 的组织维度）。
///
/// **这条以前是假的。** 它名叫「跨组织」，实测传进来的却是 `u1_of_o2()`——那个主体
/// **本身就属于 `ORG_O2`**，与 `O2_EDITOR_SESSION` 的归属组织**相同**。所以它真正测到的
/// 是「同组织不同主体」，和 `cm06_a_peer_principal_in_the_same_organization_is_denied`
/// 重复，而 `check_organization` 在 `Editor` 这条路上**一次都没被触发过**。
/// 全文件里当时没有任何一条跨组织 `Editor` 负例。
///
/// 现在拆成两条腿，把组织比较单独钉住：
///
/// 1. **同主体、跨组织**：`principal(PRINCIPAL_U2, ORG_O1)` 顶着 U2 的身份去要 `ORG_O2`
///    的会话——`check_principal` 会**通过**，只有 `check_organization` 能拒它。
///    这是本文件唯一一条「删掉组织检查就变红、删掉主体检查仍然红不了」的隔离用例。
/// 2. **真 owner 正向对照**：`u2_of_o2()` 必须受理。否则腿 1 可能只是
///    「`ORG_O2` 的会话全被拒」这种空绿。
#[tokio::test]
async fn cm06_an_editor_owned_by_another_organization_is_refused() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    // 腿 1：主体对、组织错 → 只可能栽在组织比较上。
    let error = gw
        .accept(
            &principal(PRINCIPAL_U2, ORG_O1),
            request_for(O2_EDITOR_SESSION, editor_source(PRINCIPAL_U2, ORG_O1)),
        )
        .await
        .expect_err("跨组织的 editor 归属必须被拒");
    assert_eq!(api_code(&error), Some(ApiErrorCode::SessionNotFound));
    assert_eq!(port.execute_calls(), 0);

    // 腿 2：O2 自己的会话对 O2 自己的主体必须通行（排除空绿）。
    // `accept()` 只入队不下发，所以驱动次数仍是 0；证据在执行记录与幂等账本上。
    let acceptance = gw
        .accept(
            &u2_of_o2(),
            request_for(O2_EDITOR_SESSION, editor_source(PRINCIPAL_U2, ORG_O2)),
        )
        .await
        .expect("O2 的真 owner 必须被受理");
    assert!(!acceptance.execution_id().as_str().is_empty());
    assert_eq!(gw.execution_count().await, 1, "真 owner 应当留下一条执行记录");
    assert_eq!(store.writes(), 1, "真 owner 应当写入一条幂等账本");
    assert_eq!(port.execute_calls(), 0);
}

/// CM-06 的运行期一半：**请求体里伪造的身份字段无法覆盖 `RequestContext`**。
///
/// 这里把 `ExecutionSource` 里可声明的组织/主体写成 U2 的值，而
/// `RequestPrincipal` 仍然是 U1。若实现信任请求体，U1 就能用一句 source 顶掉
/// 鉴权身份；实测必须判 U2 的会话为拒绝、判自己的会话为放行——**判定只看
/// `RequestPrincipal`，一次都不看 source 里的身份字段**。
#[tokio::test]
async fn cm06_a_forged_identity_in_the_body_cannot_override_the_principal() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    // source 谎称自己是 U2 / O1，principal 却是 U1。
    let forged = editor_source(PRINCIPAL_U2, ORG_O1);

    let error = gw
        .accept(&u1(), request_for(U2_EDITOR_SESSION, forged.clone()))
        .await
        .expect_err("source 谎称 U2 不得让 U1 通过 U2 的会话");

    assert_eq!(api_code(&error), Some(ApiErrorCode::SessionNotFound));

    // 同一份伪造 source 用来打 U1 自己的会话：放行。
    // 放行只说明 source 没有被拒（它是合法可持久化的），不代表它参与了判定；
    // 真正的证据是上面那条：伪造它并不能替 U1 打开 U2 的门。
    gw.accept(&u1(), request_for(U1_SESSION, forged))
        .await
        .expect("伪造 source 不会污染 U1 自己的会话");
}

/// CM-06 的编译期一半：请求 DTO 上根本没有身份字段可伪造。
///
/// 这条是**源码结构**断言，不是注释。`ExecutionRequest` 的字段表由
/// [`execution_request_fields`] 从 `request.rs` 里原样抠出来，因此有人日后加上
/// `owner` / `principal_id` 之类的字段，这里立刻变红。
#[test]
fn cm06_the_request_dto_has_no_field_to_forge_an_identity_into() {
    let fields = execution_request_fields();

    assert_eq!(
        fields,
        vec![
            "handle".to_string(),
            "expected_context_revision".to_string(),
            "call".to_string(),
            "idempotency_key".to_string(),
            "source".to_string(),
            "resource_binding_id".to_string(),
        ],
        "请求 DTO 的字段表变了：新增字段必须先回答「它能被用来伪造身份吗」"
    );

    for token in ["owner", "principal", "organization", "identity", "org_id"] {
        assert!(
            !fields.iter().any(|f| f.contains(token)),
            "请求 DTO 上出现了身份字段 `{token}`：它能被请求体伪造"
        );
    }

    // 请求 DTO 上连 `Deserialize` 都没有 ⇒ 任何 JSON 请求体都绑不进来，
    // 「序列化时被静默丢弃」这条路同样不存在。
    assert!(
        !request_source().contains("impl<'de> serde::Deserialize<'de> for ExecutionRequest")
            && !request_source().contains("DeserializeOwned for ExecutionRequest"),
        "ExecutionRequest 有了反序列化实现：请证明它不接受身份字段，否则本测试的前提失效"
    );
}

fn request_source() -> &'static str {
    include_str!("../src/gateway/request.rs")
}

/// 抠出 `pub struct ExecutionRequest { … }` 在括号深度 1 上的全部 `pub` 字段名。
fn execution_request_fields() -> Vec<String> {
    let src = request_source();
    let anchor = "pub struct ExecutionRequest {";
    let start = src
        .find(anchor)
        .expect("ExecutionRequest 必须仍定义在 src/gateway/request.rs")
        + anchor.len();

    let mut depth = 0i32;
    let mut fields = Vec::new();
    for line in src[start..].lines() {
        let trimmed = line.trim();
        if depth == 0 {
            if let Some(rest) = trimmed.strip_prefix("pub ") {
                if let Some((name, tail)) = rest.split_once(':') {
                    if !tail.is_empty() && !tail.starts_with(|c: char| c.is_uppercase()) {
                        fields.push(name.to_string());
                    }
                }
            }
        }
        depth += line.matches('{').count() as i32;
        depth -= line.matches('}').count() as i32;
        if depth < 0 {
            break;
        }
    }
    fields
}

/// CM-05 六个接口的归属台账：网关这一层的动作面**恰好**是 `Execute` 与 `Cancel`。
///
/// 穷尽 `match` 是编译期事实：少一个变体编不过，多一个变体也编不过。于是
/// 「读取 / 关闭 / 订阅 / 下载」不是本层漏测，而是本层根本没有对应闸门；
/// 其中订阅与下载属 `ArtifactStore`（CM-61 / CM-64，已豁免到 P7）。
#[test]
fn cm05_the_gateway_action_surface_is_exactly_execute_and_cancel() {
    fn label(action: GatewayAction) -> &'static str {
        match action {
            GatewayAction::Execute => "execute",
            GatewayAction::Cancel => "cancel",
        }
    }

    assert_eq!(label(GatewayAction::Execute), "execute");
    assert_eq!(label(GatewayAction::Cancel), "cancel");
}

// ---------------------------------------------------------------------------
// CM-04
// ---------------------------------------------------------------------------

/// CM-04：`executeInSession` 拿到一个 `profileId`（配置 id）当会话句柄用。
///
/// `SessionHandle` 是个结构体，装的是 `dbSessionId` + `runtimeEpoch`，
/// 「配置 id 回退成会话 id」在类型上就没有通路；即便有人拿字符串硬凑过来，
/// 解析层也只会回答「没有这个会话」，且一个驱动调用都不产生。
#[tokio::test]
async fn cm04_a_profile_id_passed_as_a_session_handle_is_not_found() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let error = gw
        .accept(
            &u1(),
            request_for(PROFILE_ID, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect_err("配置 id 冒充会话句柄必须得到 SessionNotFound");

    assert_eq!(
        api_code(&error),
        Some(ApiErrorCode::SessionNotFound),
        "CM-04 要求的不可见配置错误，就是 §13 的 sessionNotFound"
    );
    assert_eq!(port.execute_calls(), 0, "driver execute 次数必须为 0");
    assert_eq!(gw.execution_count().await, 0);
    assert_eq!(store.writes(), 0);

    // 对照：把同一个配置 id 换回一个真实存在的会话句柄，投影立刻不同。
    // 少了这一条，「一切投影都相同」也可以靠「什么都看不见」蒙混过关。
    let ok = gw
        .accept(
            &u1(),
            request_for(U1_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect("真句柄必须放行");
    assert!(!ok.execution_id().as_str().is_empty());
}

// ---------------------------------------------------------------------------
// CM-05
// ---------------------------------------------------------------------------

/// CM-05 的核心断言：**「不存在」与「不归我」逐字节无法区分**。
///
/// 两条请求分别用**同一个未知句柄**（`ABSENT_SESSION`）和 U2 的会话句柄发给 U1。
/// 两条的对外投影必须完全一样，且驱动执行次数都必须是 0。
#[tokio::test]
async fn cm05_absent_and_foreign_handles_are_byte_identical() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let absent = gw
        .accept(
            &u1(),
            request_for(ABSENT_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect_err("未知句柄必须被拒");
    let foreign = gw
        .accept(
            &u1(),
            request_for(U2_EDITOR_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect_err("他人会话必须被拒");

    assert_eq!(
        projection(&absent),
        projection(&foreign),
        "「不存在」与「不归我」的对外投影必须逐字节相同，否则就是存在性预言机"
    );
    assert_eq!(
        projection(&absent),
        serde_json::json!({
            "kind": "runtime",
            "reason": "unknownSession",
            "apiCode": "sessionNotFound",
        })
    );
    assert_eq!(port.execute_calls(), 0);
    assert_eq!(gw.execution_count().await, 0);
    assert_eq!(store.writes(), 0);

    // 负对照：投影本身**是**敏感的——换一个原因，投影必须跟着变。
    // 少了这一条，上面的 `assert_eq!` 可能只是因为「所有错误长得都一样」而成立。
    let conflict = gw
        .accept(&u1(), request_with_revision(U1_SESSION, REVISION + 5))
        .await
        .expect_err("版本对不上必须被拒");
    assert_ne!(
        projection(&conflict),
        projection(&absent),
        "投影若对不同原因也相同，上面的相等断言就没有意义"
    );
    assert_eq!(api_code(&conflict), Some(ApiErrorCode::ContextConflict));
}

/// CM-05：取消一个属于他人的执行 —— 归属不符，驱动取消次数必须是 0。
///
/// 先由真 owner U2 受理一次（产生一条真实的执行记录），再让 U1 拿着这个
/// `executionId` 去取消：记录存在、绑定也对得上，唯一的拦截点是归属比较。
#[tokio::test]
async fn cm05_cancelling_a_foreign_execution_never_reaches_the_driver() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let acceptance = gw
        .accept(
            &u2(),
            request_for(U2_EDITOR_SESSION, editor_source(PRINCIPAL_U2, ORG_O1)),
        )
        .await
        .expect("U2 是真 owner，受理应当成功");
    let execution_id = acceptance.execution_id().clone();

    let error = gw
        .cancel(&u1(), cancel_request(&execution_id, U2_EDITOR_SESSION))
        .await
        .expect_err("U1 无权取消 U2 的执行");

    assert_eq!(
        api_code(&error),
        Some(ApiErrorCode::SessionNotFound),
        "取消路径上的归属不符必须与执行路径投影一致"
    );
    assert_eq!(port.cancel_calls(), 0, "driver cancel 次数必须为 0");
    assert_eq!(port.execute_calls(), 0);

    // 对照：真 owner 自己取消不被归属闸门拦住（这里会被端口替身挡下，
    // 证明它确实走到了端口，而不是又被归属比较拦了一次）。
    let owner_error = gw
        .cancel(&u2(), cancel_request(&execution_id, U2_EDITOR_SESSION))
        .await
        .expect_err("替身端口不提供取消");
    assert!(
        matches!(
            owner_error,
            GatewayError::Runtime(RuntimeError::InvariantBroken(_))
        ),
        "真 owner 必须穿过归属闸门抵达端口（失败文本刻意不回显错误值：错误里带句柄）"
    );
    assert_eq!(port.cancel_calls(), 1);
}

/// CM-05：**跨组织**归属与不存在被折叠成同一个投影。
///
/// 同样**修过名**：以前这条也传 `u1_of_o2()`（本身属于 `ORG_O2`），所以它是
/// 「同组织不同主体」不是「跨组织」。现在两个请求都换成 `&u1()`（属于 `ORG_O1`）
/// 打 `ORG_O2` 的会话，对象才是真的跨组织；「不存在」那一腿用同一个主体，
/// 于是两腿之间**只有归属这一项不同**，投影相等才是有意义的断言。
#[tokio::test]
async fn cm05_a_cross_organization_owner_looks_identical_to_an_absent_session() {
    let (port, store) = world();
    let gw = gateway(Arc::clone(&port), Arc::clone(&store));

    let absent = gw
        .accept(
            &u1(),
            request_for(ABSENT_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect_err("未知句柄必须被拒");
    let foreign = gw
        .accept(
            &u1(),
            request_for(O2_EDITOR_SESSION, editor_source(PRINCIPAL_U1, ORG_O1)),
        )
        .await
        .expect_err("跨组织的会话必须被拒");

    assert_eq!(projection(&absent), projection(&foreign));
    assert_eq!(port.execute_calls(), 0);
}
