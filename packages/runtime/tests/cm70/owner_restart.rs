//! A7：open/context 的 owner 重启后，旧令牌一律 `SessionLost`。
//!
//! 令牌里绑着签发时的 `owner_runtime_epoch`。owner 重启会把会话的 epoch 推进一格，
//! 于是「上一次运行期签的令牌」在新运行期里指向一个已经不存在的 owner。
//! 这类令牌**不能**退回成一次全新写入——那正是「响应丢了之后客户端拿新键重试」
//! 想干的事，也是 CM-70 点名要禁的。因此这里收敛到 `RuntimeError::SessionLost`，
//! 和驱动侧「会话没了」的既有语义走同一条路，调用方不必再认第三种错误。

use crate::gateway_fixtures as fx;
use crate::{err, write_once};
use datazen_runtime::connection::{RuntimeError, SessionState};
use datazen_runtime::gateway::{GatewayError, SubmissionOperation};

/// 在指定 owner epoch 下签一张执行令牌。
fn issue_for_epoch(tokens: &datazen_runtime::gateway::SubmissionTokenGuard, epoch: u64) -> String {
    tokens
        .issue(
            SubmissionOperation::ExecuteInSession,
            Some(datazen_runtime::connection::Counter::new(epoch)),
            1_000,
            fx::TOKEN_TTL_NANOS,
        )
        .unwrap_or_else(|err| panic!("夹具期望签发成功，实际失败：{err:?}"))
}

/// owner 重启后，旧令牌重发 ⇒ `SessionLost`，且不下发。
#[tokio::test]
async fn an_old_token_after_the_owner_restarts_is_session_lost() {
    let h = fx::token_harness();
    let stale = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &stale)).await;
    assert_eq!(h.harness.port.execute_calls(), 1);

    // owner 重启：端口视图与客户端句柄的 epoch 一起推进。
    h.harness.port.set_view(Ok(fx::view(
        fx::SESSION,
        2,
        fx::REVISION,
        SessionState::Ready,
    )));

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_for_epoch(fx::REVISION, &stale, 2),
        )
        .await);
    match &error {
        GatewayError::Runtime(RuntimeError::SessionLost(message)) => {
            assert!(
                message.contains(fx::SESSION),
                "SessionLost 要指明是哪个会话：{message}"
            );
        }
        other => panic!("owner 重启后旧令牌必须 SessionLost，实际是 {other:?}"),
    }
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "旧令牌在重启后重发绝不能落到驱动"
    );
}

/// 重启后的**新**令牌照常受理——确认上一条不是因为「重启后什么都不认」。
#[tokio::test]
async fn a_token_issued_after_the_restart_is_accepted() {
    let h = fx::token_harness();
    h.harness.port.set_view(Ok(fx::view(
        fx::SESSION,
        2,
        fx::REVISION,
        SessionState::Ready,
    )));
    let fresh = issue_for_epoch(&h.tokens, 2);

    write_once(&h, fx::request_for_epoch(fx::REVISION, &fresh, 2)).await;
    assert_eq!(h.harness.port.execute_calls(), 1);

    // 同一条令牌照样重发 ⇒ 原回执。
    let again = h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_for_epoch(fx::REVISION, &fresh, 2),
        )
        .await
        .expect("新运行期内的重发应当拿到原回执");
    assert!(again.is_replay());
    assert_eq!(h.harness.port.execute_calls(), 1);
}

/// 旧令牌跨过「上下文推进」也不认。
///
/// 重启通常还伴随 context 重建；这里把上下文修订也推进一格，
/// 确认 `SessionLost` 不是被上下文闸门「顺手」挡掉的。
#[tokio::test]
async fn an_old_token_is_session_lost_even_when_the_context_moved_on() {
    let h = fx::token_harness();
    let stale = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &stale)).await;

    h.harness.port.set_view(Ok(fx::view(
        fx::SESSION,
        2,
        fx::REVISION + 1,
        SessionState::Ready,
    )));

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_for_epoch(fx::REVISION + 1, &stale, 2),
        )
        .await);
    assert!(
        matches!(error, GatewayError::Runtime(RuntimeError::SessionLost(_))),
        "上下文推进不许掩盖 owner 重启，实际是 {error:?}"
    );
    assert_eq!(h.harness.port.execute_calls(), 1);
}

/// 旧令牌换一把新幂等键照样拒。
///
/// 客户端在响应丢失后的本能动作是「换把新键重试」。令牌闸门排在第 0 步，
/// 所以这把新键根本走不到账本——这正是「不能用新键自动重试未知写入」在
/// owner 重启这条路径上的落点。
#[tokio::test]
async fn an_old_token_is_refused_even_when_replayed_under_a_fresh_key() {
    let h = fx::token_harness();
    let stale = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &stale)).await;
    let reads_before = h.store.read_calls();

    h.harness.port.set_view(Ok(fx::view(
        fx::SESSION,
        2,
        fx::REVISION,
        SessionState::Ready,
    )));
    let brand_new_key = fx::issue_token(&h.tokens, 1_000);
    assert_ne!(brand_new_key, stale, "必须真的是一把新签的键");

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_for_epoch(fx::REVISION, &brand_new_key, 2),
        )
        .await);
    assert!(
        matches!(error, GatewayError::Runtime(RuntimeError::SessionLost(_))),
        "换新键不能把 owner 重启这条路径绕过去，实际是 {error:?}"
    );
    assert_eq!(h.harness.port.execute_calls(), 1);
    assert_eq!(
        h.store.read_calls(),
        reads_before,
        "令牌闸门就该判掉：连账本都不该查"
    );
}

/// 会话视图根本不该被问——拒绝发生在第 0 步。
#[tokio::test]
async fn a_stale_token_never_reaches_the_port_at_all() {
    let h = fx::token_harness();
    let stale = fx::issue_token(&h.tokens, 1_000);
    let views_before = h.harness.port.session_view_calls();

    let error = err(h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_for_epoch(fx::REVISION, &stale, 2),
        )
        .await);
    assert!(
        matches!(error, GatewayError::Runtime(RuntimeError::SessionLost(_))),
        "实际是 {error:?}"
    );
    assert_eq!(
        h.harness.port.session_view_calls(),
        views_before,
        "令牌闸门必须排在会话查询之前"
    );
    assert_eq!(h.harness.port.execute_calls(), 0);
    assert_eq!(h.store.read_calls(), 0);
    assert_eq!(h.store.write_calls(), 0);
}

/// 对照：不重启时，同一条令牌只是普通重发。
#[tokio::test]
async fn without_a_restart_the_same_token_is_just_a_replay() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;

    let again = h
        .harness
        .gateway
        .accept(
            &fx::principal(),
            fx::request_for_epoch(fx::REVISION, &token, 1),
        )
        .await
        .expect("同一运行期内重放应当拿到原回执");
    assert!(again.is_replay());
    assert_eq!(h.harness.port.execute_calls(), 1);
}
