//! `cm70_no_disk` 的「网关错误投影面」这一组测试。
//!
//! 单独成文件只有一个理由：`cm70_no_disk.rs` 已经贴着单文件 800 行的上限，而这一轮
//! 修复要在它里面补裁定注释（每处保留的插值都要写理由）。拆分的界线按**职责**划，
//! 不是按行数凑：这里四条测试问的都是同一个问题——**`GatewayError` 及其伴生记录，
//! 在被返回、被 Debug、被持久化之后，身上还带着多少令牌材料**。
//!
//! panic 文本的裁定（允许什么、禁止什么、为什么）写在父模块文件头的
//! `## panic 文本里能出现什么`，机器闸门在 `tests/cm70_panic_redaction_guard.rs`。
//! 本文件里的插值遵守同一套标准，下面逐处标注了保留理由。

use super::*;

use datazen_runtime::gateway::ExecutionRequest;

/// A4：同键、不同输入的冲突错误，两条投影里都没有令牌材料。
///
/// 这是 H-1 的核心用例。冲突错误**必然**是在持有调用方那枚令牌时产生的，
/// 看起来像是最容易顺手把 `key` 回显出去的地方；而回显出去等于把凭据写进 IPC 载荷。
/// 账本按 `(dbSessionId, runtimeEpoch, key)` 查，冲突只可能出在调用方自己的作用域里，
/// 所以回显回来的 `key` 恒等于调用方自己刚递进来的东西——零信息量、纯泄漏。
/// 因此错误里只留**标识符**：`existing`（已存在的那次执行）与
/// `incoming`（指纹十六进制），二者都不含令牌材料。
#[tokio::test]
async fn a_conflict_error_carries_identifiers_but_no_token_material() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);

    let first = fx::accept(&h.harness, fx::request_with_key(fx::REVISION, &token)).await;
    // 同一枚令牌、换一个输入 ⇒ 判据里那条「同 receipt、不同输入冲突」。
    let mut other = fx::request_with_key(fx::REVISION, &token);
    other.call.input = serde_json::json!({ "sql": "delete from t where 1=0" });
    // `{error:?}` 保留：`error` 是 `refused` 返回的 `GatewayError`，其字段已在构造点
    // （`src/gateway/mod.rs`）逐个核对——`incoming` 是 `incoming.as_str()` 的指纹，
    // `message` 由网关自建，不含签名后的令牌原文。
    let error = refused(
        h.harness.gateway.accept(&fx::principal(), other).await,
        "同键不同输入必须是冲突",
    );
    assert!(
        matches!(error, GatewayError::IdempotencyConflict { .. }),
        "{error:?}"
    );

    assert_both_projections_are_clean("冲突错误", &error, &token);

    // 干净不等于没信息：该给排障的人看的标识符必须还在。
    let json = error.to_persistable_json();
    assert_eq!(json["kind"], "idempotencyConflict");
    assert_eq!(json["existing"], first.as_str(), "必须指出冲的是哪一次执行");
    // `incoming` 是**被查的那份投影本身**，所以只回显长度、不回显内容：长度断言挡不住
    // 「长到不像指纹的凭据」，可回显内容挡不住任何东西。
    let incoming = json["incoming"].as_str().unwrap_or_default();
    assert_eq!(
        incoming.len(),
        16,
        "`incoming` 必须是指纹十六进制，实际长度不对"
    );
    assert!(!incoming.is_empty());
}

/// 把「每条真实拒绝路径」的错误挨个查一遍。
///
/// 只造一个错误去查，等于只查了构造出来的那个形状；真正的风险是**某条具体路径**
/// 顺手把请求里的东西拼进了消息文本。所以这里把真实令牌真的送过每一条拒绝路径，
/// 再检查它返回的那个错误——令牌材料要真的一路活到投影里才会被抓到。
#[tokio::test]
async fn no_rejection_path_ever_carries_token_material() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let harness = &h.harness;
    let accept = |request: ExecutionRequest| async move {
        harness.gateway.accept(&fx::principal(), request).await
    };

    // ① 伪造 MAC：改掉 MAC 段的一位（不重算签名）。
    let parts: Vec<&str> = token.split('.').collect();
    let mut tampered = parts[3].to_owned();
    let last = tampered.pop().expect("MAC 段非空");
    tampered.push(if last == '0' { '1' } else { '0' });
    // `forged_mac` 是凭据（MAC 段的十六进制），`parts` 是同一枚真令牌的四段——两者都绝不
    // 进 panic 文本；下面的 `{forged:?}` 指的是另一个东西。
    let forged_mac = format!("{}.{}.{}.{}", parts[0], parts[1], parts[2], tampered);
    let forged = refused(
        accept(fx::request_with_key(fx::REVISION, &forged_mac)).await,
        "伪造 MAC 必须被拒",
    );
    // `{forged:?}` 保留：`forged` 是 `refused` 返回的 `GatewayError`，不是那枚伪造令牌
    // （伪造出来的是 `forged_mac`，它的十六进制绝不能进 panic 文本）。`SubmissionTokenRejected`
    // 的载荷是 `&'static str`，且这条断言的立意正是「拒绝理由不许把令牌咽下去」——
    // 删掉 `{forged:?}` 反而让失败时看不出网关到底返回了什么。
    assert!(
        matches!(forged, GatewayError::SubmissionTokenRejected { .. }),
        "{forged:?}"
    );
    assert_both_projections_are_clean("伪造 MAC 的拒绝", &forged, &token);

    // ② 未知 keyVersion：版本号在签名覆盖范围内，所以改它同样对不上签名。
    let unknown_version = parts[..3].join(".") + ".99." + parts[3];
    let error = refused(
        accept(fx::request_with_key(fx::REVISION, &unknown_version)).await,
        "未知 keyVersion 必须被拒",
    );
    assert!(
        matches!(error, GatewayError::SubmissionTokenRejected { .. }),
        "{error:?}"
    );
    assert_both_projections_are_clean("未知 keyVersion 的拒绝", &error, &token);

    // 先把这条写入**真的下发一次**：下面两条拒绝路径都以「已经下发」为前提，
    // 没下发过就没有记录，既撞不出冲突，也立不起围栏。
    fx::accept_and_dispatch(&h.harness, fx::request_with_key(fx::REVISION, &token)).await;
    assert_eq!(h.harness.port.execute_calls(), 1, "确有一次真正下发");

    // ③ 同键不同输入。
    let mut different = fx::request_with_key(fx::REVISION, &token);
    different.call.input = serde_json::json!({ "sql": "update t set a=1" });
    // `{conflict:?}` 同上：变体是 `IdempotencyConflict`，字段是指纹与 `ExecutionId`。
    let conflict = refused(accept(different).await, "同键不同输入必须是冲突");
    assert!(
        matches!(conflict, GatewayError::IdempotencyConflict { .. }),
        "{conflict:?}"
    );
    assert_both_projections_are_clean("冲突", &conflict, &token);

    // ④ 围栏：换新键把同一条写入再发一次。这是另一枚令牌，所以它自己的两段凭据
    //    也要一起查——最可能顺手当回显的就是它。
    let retry = fx::issue_token(&h.tokens, h.harness.clock.now_nanos());
    let error = refused(
        accept(fx::request_with_key(fx::REVISION, &retry)).await,
        "结局未知的写入换新键重试必须先核验",
    );
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "{error:?}"
    );
    assert_both_projections_are_clean("围栏", &error, &retry);
    assert_both_projections_are_clean("围栏", &error, &token);

    // ⑤ 过期。单独一只夹具：把时钟推过 TTL 会顺手搅到上面几条。
    let late = fx::token_harness();
    let expired = fx::issue_token(&late.tokens, 1_000);
    // 推进 TTL 再多 1_000（签发点是 1_000），让「现在」确实越过 expiresAt。
    late.harness.clock.advance(fx::TOKEN_TTL_NANOS + 2_000);
    let error = refused(
        late.harness
            .gateway
            .accept(
                &fx::principal(),
                fx::request_with_key(fx::REVISION, &expired),
            )
            .await,
        "过期令牌必须被拒",
    );
    assert!(
        matches!(error, GatewayError::SubmissionTokenRejected { .. }),
        "{error:?}"
    );
    assert_both_projections_are_clean("过期令牌的拒绝", &error, &expired);
}

/// 账本读不出来那条拒绝路径：`Unreadable` 不是「没这条记录」，而是「说不准」。
#[tokio::test]
async fn the_unreadable_ledger_rejection_carries_no_token_material() {
    let h = fx::unreadable_token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let error = refused(
        h.harness
            .gateway
            .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
            .await,
        "账本读不出来必须要求核验，而不是当成没这条记录",
    );
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "{error:?}"
    );
    assert_both_projections_are_clean("账本读不出来", &error, &token);
}

/// 执行记录的手写 `Debug` 是公开面（`ExecutionGateway::execution` 返回记录本身），
/// 所以令牌字段必须在里面被抹掉，而不是「恰好没人打印它」。
#[tokio::test]
async fn the_execution_record_debug_does_not_print_the_token() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let id = fx::accept(&h.harness, fx::request_with_key(fx::REVISION, &token)).await;

    let record = h
        .harness
        .gateway
        .execution(&id)
        .await
        .expect("受理过就查得到记录");
    // `rendered` 是**正被怀疑**装了凭据的渲染文本，`token` 本身是凭据——两者都由
    // `assert_no_token_material` 挡住，绝不进 panic 文本。
    let rendered = format!("{record:?}");
    assert_no_token_material("执行记录的 Debug", &rendered, &token);
    assert!(
        rendered.contains("<redacted>"),
        "令牌字段应当被显式抹掉而不是悄悄消失"
    );
    // 记录里剩下的东西是排障要用的，不该被一起抹掉。
    assert!(rendered.contains(id.as_str()), "记录里本来就有 executionId");
}
