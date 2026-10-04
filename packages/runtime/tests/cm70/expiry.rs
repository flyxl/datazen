//! A3：有效期内重发同/不同输入；过期重发不再执行。
//!
//! 「不再执行」的判据是**端口下发给驱动的次数**没涨，不是「没报错」——
//! 后者会把「拒了但还是跑了」放过去。

use crate::gateway_fixtures as fx;
use crate::{err, token_reason, write_once};
use datazen_runtime::connection::{CommandCall, Counter, RuntimeError};
use datazen_runtime::gateway::{ExecutionRequest, GatewayError};

/// 前置：写入已接受且响应丢失——即客户端没拿到回执就重发。
#[tokio::test]
async fn a_replay_inside_the_validity_window_returns_the_same_receipt() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);

    let first = write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;
    assert_eq!(h.harness.port.execute_calls(), 1, "首次必须真的下发");

    let reads_before = h.store.read_calls();
    assert_eq!(
        reads_before, 1,
        "受理查一次账本，派发按 executionId 走、不再查"
    );

    // 时钟在有效期内往前挪一格：令牌没过期，账本记录还在。
    h.harness.clock.advance(1_000);
    let replay = h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await
        .expect("有效期内重发应当拿到原回执");

    assert_eq!(replay.execution_id().as_str(), first, "同 receipt");
    assert!(replay.is_replay(), "回执必须自报是重发，而不是冒充首受理");
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "重发不得二次下发：驱动那边只能收到一次"
    );
    assert_eq!(h.store.read_calls(), reads_before + 1, "重发只查一次账本");
    assert_eq!(h.store.write_calls(), 1, "重发不得二次写入账本");
}

/// 有效期内、同一个令牌、不同输入 ⇒ 冲突。
///
/// 与上一条配对，证明去重不是「只看键在不在」——键相同但语义不同必须显式冲突，
/// 否则一个被篡改的重发会悄悄替换掉首次的执行语义。
#[tokio::test]
async fn a_replay_inside_the_window_with_different_input_conflicts() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;

    let mut conflicting = fx::request_with_key(fx::REVISION, &token);
    conflicting.call = CommandCall {
        command: "execute".to_owned(),
        input: serde_json::json!({ "sql": "drop table cm70_target" }),
    };

    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), conflicting)
        .await);
    assert!(
        matches!(error, GatewayError::IdempotencyConflict { .. }),
        "同令牌不同输入必须冲突，实际是 {error:?}"
    );
    assert_eq!(h.harness.port.execute_calls(), 1, "冲突的输入不得下发");
}

/// 过期重发 ⇒ 拒绝，且**不下发**。
#[tokio::test]
async fn an_expired_token_is_refused_and_never_dispatched() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;
    assert_eq!(h.harness.port.execute_calls(), 1);

    let views_before = h.harness.port.session_view_calls();

    // 恰好越过有效期一秒。
    h.harness.clock.advance(fx::TOKEN_EXPIRES_AT_NANOS);
    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await);
    assert_eq!(token_reason(&error), "submissionTokenExpired");
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "过期重放绝不能落到驱动：这是 CM-70 的主断言之一"
    );
    assert_eq!(
        h.harness.port.session_view_calls(),
        views_before,
        "令牌闸门在会话投影之前就该把过期重放挡下来"
    );
}

/// 过期重放**碰不到账本**。
///
/// 「拒绝」和「拒绝得足够早」是两回事：若过期重放走到了账本查询，
/// 一个读不出来或写入失败的账本就可能把这次重放变成一次新执行。
#[tokio::test]
async fn an_expired_replay_never_reaches_the_ledger() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;
    let reads_before = h.store.read_calls();

    h.harness.clock.advance(fx::TOKEN_EXPIRES_AT_NANOS);
    let _ = h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await;

    assert_eq!(
        h.store.read_calls(),
        reads_before,
        "过期重放不得查询账本：一旦 Miss 就会重新执行"
    );
}

/// 过期重放的落点是**拒绝**，不是「受理成一次新写入」。
///
/// 这条钉住拒绝语义本身——若将来有人把过期改成「当成没记录重新受理」，
/// 它会立刻红。
#[tokio::test]
async fn an_expired_replay_is_not_downgraded_to_a_fresh_acceptance() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;

    h.harness.clock.advance(fx::TOKEN_EXPIRES_AT_NANOS);
    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await);
    assert_eq!(token_reason(&error), "submissionTokenExpired");
    assert!(
        !matches!(error, GatewayError::IdempotencyConflict { .. }),
        "过期不是冲突：冲突意味着账本还认得这条记录"
    );
}

/// 没接令牌层的网关不拦过期——证明上面的拒绝确实来自令牌闸门，
/// 而不是碰巧被别的东西挡住了。
#[tokio::test]
async fn a_gateway_without_the_token_layer_does_not_check_expiry() {
    let h = fx::ready_harness();
    h.clock.advance(fx::TOKEN_EXPIRES_AT_NANOS);
    // 未接令牌层时，键就是普通字符串，过期与否无人过问。
    h.gateway
        .accept(&fx::principal(), fx::request(fx::REVISION))
        .await
        .expect("未接令牌层时不存在过期判定");
}

/// 换令牌 ⇒ 换键 ⇒ 换账本作用域（键域语义本身没变）。
///
/// **此处原为 CM-54 键域语义延续，经协调者裁定按判据 §CM-70 更新。**
/// 原用例叫 `a_different_token_is_a_different_write_not_a_replay`，断言「新令牌
/// 必被受理、驱动收到第二条写入」。该断言在同一条语义写入上**正是在肯定旧行为**：
/// 换个新键把同一条写入再跑一遍，正是判据禁止的「用新键自动重试」。所以断言的
/// 作用域被收窄到「**不同**的写入」——键域语义（键不同即作用域不同、账本不去重跨键
/// 的无关写入）原样保留，被 CM-70 改写的只有「同一条写入换个键」那半边，它现在归
/// `cm70/retries.rs` 的围栏用例管。
#[tokio::test]
async fn a_different_token_on_a_different_write_is_still_a_fresh_write() {
    let h = fx::token_harness();
    let first_token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &first_token)).await;

    // 换令牌、**并且**换写入内容：与上一条毫不相干，必须真受理、真下发。
    let second_token = fx::issue_token(&h.tokens, 2_000);
    let other = ExecutionRequest::new(
        fx::session_handle(),
        Counter::new(fx::REVISION),
        CommandCall {
            command: "query".to_owned(),
            input: serde_json::json!({ "sql": "select 2" }),
        },
        &second_token,
        fx::source(),
    );
    let replay = h
        .harness
        .gateway
        .accept(&fx::principal(), other)
        .await
        .expect("另一条令牌上的另一条写入，应当受理");
    assert!(
        !replay.is_replay(),
        "不相干的写入不该被账本当成重发，否则换个键就能骗过账本"
    );
    h.harness
        .gateway
        .dispatch(&fx::principal(), replay.execution_id())
        .await
        .expect("第二条写入应当派发得出去");
    assert_eq!(h.harness.port.executed_requests().len(), 2);
}

/// 同一个测试里被收窄掉的那半边，单独钉在这里，钉在判据的那句话上。
///
/// 同一条写入换新键 ⇒ 第一次已经真发过 SQL ⇒ 第二遍必须先核验；
/// 核验放行之后才发第二遍。这两条合起来才是完整语义：围栏不是永久禁令。
#[tokio::test]
async fn the_same_write_under_a_new_key_waits_for_verification_and_then_runs() {
    let h = fx::token_harness();
    let first_token = fx::issue_token(&h.tokens, 1_000);
    let fingerprint = fx::request_with_key(fx::REVISION, &first_token).fingerprint();
    write_once(&h, fx::request_with_key(fx::REVISION, &first_token)).await;
    assert_eq!(h.harness.port.executed_requests().len(), 1);

    let second_token = fx::issue_token(&h.tokens, 2_000);
    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_with_key(fx::REVISION, &second_token),
        )
        .await);
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "同一条写入换新键就是自动重试，必须先核验，实际 {error:?}"
    );
    assert_eq!(
        h.harness.port.executed_requests().len(),
        1,
        "核验之前一条 SQL 都不许再发"
    );

    // 核验后确认上次确实没生效，才放行第二遍。
    assert!(
        h.harness
            .gateway
            .resolve_unknown_outcome(fx::SESSION, &fingerprint)
            .await,
        "指纹对得上就应当解除了围栏"
    );
    let id = write_once(&h, fx::request_with_key(fx::REVISION, &second_token)).await;
    assert!(!id.is_empty());
    assert_eq!(
        h.harness.port.executed_requests().len(),
        2,
        "显式核验之后这次写入才真的该被执行"
    );
}

/// 令牌层自己不会把 RuntimeError 降级成静默受理。
#[tokio::test]
async fn a_runtime_error_on_the_far_side_is_still_transparent() {
    let h = fx::token_harness();
    h.harness
        .port
        .set_view(Err(RuntimeError::SessionLost("会话没了".to_owned())));
    let token = fx::issue_token(&h.tokens, 1_000);
    let error: GatewayError = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await);
    assert!(
        matches!(error, GatewayError::Runtime(_)),
        "端口错误原样透传，实际是 {error:?}"
    );
    assert_eq!(h.harness.port.execute_calls(), 0);
}

/// 令牌层没有接上时，请求构造依然合法。
///
/// 这条不是废话：`ExecutionRequest::new` 的第五个参数在 CM-70 里从「随便一个键」
/// 变成了「签名令牌串」，凡是还在塞普通字符串的地方都不该被本次改动打断。
#[test]
fn the_idempotency_key_field_still_takes_a_plain_string() {
    let request: ExecutionRequest = fx::request_with_key(fx::REVISION, "not-a-token");
    assert_eq!(request.idempotency_key, "not-a-token");
}
