//! CM-70 断言「客户端不能用新键自动重试未知写入」。
//!
//! 围栏的键是 `(dbSessionId, 请求指纹)`——**没有** `idempotencyKey`、也没有令牌本身。
//! 这一点是全部要害：只要键里留一个可以随手换掉的东西，「换个新键重试」就能绕过去，
//! 而换新键正是最常见、最自动的重试姿势。所以下面每条用例都在**换键**，
//! 换完还要断言它没被受理、没被下发。
//!
//! 反过来也要有一条：围栏不能变成「这个会话整个锁死」。否则它挡住的就不只是
//! 自动重试，而是连本来毫不相干的新写入也一并拒掉——那不是幂等，是停机。

use crate::gateway_fixtures as fx;
use crate::{err, write_once};
use datazen_runtime::connection::{CommandCall, Counter, RuntimeError};
use datazen_runtime::gateway::{ExecutionRequest, GatewayError, RequestFingerprint};

/// 让账本读不出来一次：这就是「结局未知」的世界观起点。
async fn fence_once() -> fx::TokenHarness {
    let h = fx::unreadable_token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await);
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "账本读不出来就必须要求核验，实际 {error:?}"
    );
    assert_eq!(
        h.harness.port.execute_calls(),
        0,
        "读不出来的那次连回执都没拿到，更不该打到驱动上"
    );
    assert_eq!(h.store.write_calls(), 0, "读不出来时不允许写账本");
    h
}

/// 另一条语义写入（命令不同 ⇒ 指纹不同）。
fn other_write(key: &str) -> ExecutionRequest {
    ExecutionRequest::new(
        fx::session_handle(),
        Counter::new(fx::REVISION),
        CommandCall {
            command: "query".to_owned(),
            input: serde_json::json!({ "sql": "select 2" }),
        },
        key,
        fx::source(),
    )
}

#[tokio::test]
async fn a_brand_new_key_cannot_retry_a_write_whose_outcome_is_unknown() {
    let h = fence_once().await;

    // 账本此刻**读得动了**。客户端拿着一个全新的键（= 一枚全新的令牌）回来。
    h.store.recover_reads();
    let retry = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let writes_before = h.store.write_calls();

    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &retry))
        .await);
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "换新键自动重试未知写入必须被拒，实际 {error:?}"
    );
    assert_eq!(
        h.store.write_calls(),
        writes_before,
        "被围栏挡下就不该碰账本写"
    );
    assert_eq!(h.harness.port.execute_calls(), 0, "自动重试不许下发到驱动");
    assert_eq!(
        h.harness.gateway.execution_count().await,
        0,
        "被拒的受理不该留下执行记录"
    );
}

#[tokio::test]
async fn even_the_very_same_token_cannot_auto_retry_the_unknown_write() {
    let h = fence_once().await;
    h.store.recover_reads();

    // 连原键、原令牌都一样，也不许自动重试。
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let request = fx::request_with_key(fx::REVISION, &token);
    let fingerprint = request.fingerprint();
    assert!(
        h.harness
            .gateway
            .has_unverified_outcome(fx::SESSION, &fingerprint)
            .await,
        "第一次读不出来必须把这次写入记成结局未知"
    );

    let error = err(h.harness.gateway.accept(&fx::principal(), request).await);
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "用原令牌原键回来也算自动重试，实际 {error:?}"
    );
    assert_eq!(h.harness.port.execute_calls(), 0);
}

#[tokio::test]
async fn the_fence_does_not_block_an_unrelated_write_in_the_same_session() {
    let h = fence_once().await;
    h.store.recover_reads();

    // 同一会话、不同命令：跟那次结局未知的写入毫不相干，必须照常受理并下发。
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let id = write_once(&h, other_write(&token)).await;
    assert!(!id.is_empty(), "不相干的写入应当被受理");
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "围栏只挡同一条语义写入，不该把整个会话一起停掉"
    );
}

#[tokio::test]
async fn only_an_explicit_verification_reopens_the_write() {
    let h = fence_once().await;
    h.store.recover_reads();

    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let fingerprint = fx::request_with_key(fx::REVISION, &token).fingerprint();
    assert_eq!(
        h.harness.gateway.unverified_outcome_count().await,
        1,
        "恰好一次未知写入"
    );

    // 核验了库、确认那次写没落，才轮到解除围栏。
    assert!(
        h.harness
            .gateway
            .resolve_unknown_outcome(fx::SESSION, &fingerprint)
            .await,
        "对得上指纹的解除必须报告确实解除了什么"
    );
    assert!(
        !h.harness
            .gateway
            .resolve_unknown_outcome(fx::SESSION, &fingerprint)
            .await,
        "再解除一次就该是空的：解除不是幂等动作，而是状态迁移"
    );
    assert!(
        !h.harness
            .gateway
            .has_unverified_outcome(fx::SESSION, &fingerprint)
            .await
    );

    let id = write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;
    assert!(!id.is_empty());
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "核验之后这条写入才真的该被执行一次"
    );
}

#[tokio::test]
async fn the_fence_is_keyed_by_the_session_and_the_write_not_by_the_key() {
    let h = fence_once().await;

    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let fenced = fx::request_with_key(fx::REVISION, &token).fingerprint();
    let unrelated = other_write(&token).fingerprint();

    assert!(
        h.harness
            .gateway
            .has_unverified_outcome(fx::SESSION, &fenced)
            .await
    );
    assert!(
        !h.harness
            .gateway
            .has_unverified_outcome(fx::SESSION, &unrelated)
            .await,
        "指纹不同的写入不在这道围栏里"
    );
    assert!(
        !h.harness
            .gateway
            .has_unverified_outcome("db_session_别的", &fenced)
            .await,
        "别的会话里的同名指纹也不在这道围栏里"
    );
    assert_eq!(h.harness.gateway.unverified_outcome_count().await, 1);
}

/// 围栏状态对调用方可见，是为了让「这次被挡是因为上一次结局未知」这句话能被说出来。
#[tokio::test]
async fn an_unknown_outcome_never_silently_downgrades_into_a_fresh_write() {
    let h = fence_once().await;
    h.store.recover_reads();

    // 反复自动重试（每次换新键），一次都不许变成「新建一条写入」。
    for round in 0..3 {
        let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
        let error = err(h
            .harness
            .gateway
            .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
            .await);
        assert!(
            matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
            "第 {round} 次自动重试必须仍被拒，实际 {error:?}"
        );
    }
    assert_eq!(h.store.write_calls(), 0, "三轮重试一条记录都不许写");
    assert_eq!(h.harness.port.execute_calls(), 0);
    assert_eq!(h.harness.gateway.execution_count().await, 0);
}

/// 指纹是围栏的身份依据，所以它必须稳定、可复算：换一枚令牌不换指纹。
#[test]
fn the_fingerprint_ignores_the_token_but_not_the_write() {
    let a = fx::request_with_key(fx::REVISION, "cm70.1.aaaa.bbbb").fingerprint();
    let b = fx::request_with_key(fx::REVISION, "cm70.1.cccc.dddd").fingerprint();
    assert_eq!(a, b, "幂等键换了不该改掉指纹，否则围栏等于按 key 记");
    let c = other_write("cm70.1.aaaa.bbbb").fingerprint();
    assert_ne!(a, c, "命令换了必须改掉指纹，否则围栏会误伤别的写入");
    let _: RequestFingerprint = a;
}

// ───────────────── 受理时围栏之外的第二个抬栏点：下发 ─────────────────
//
// 判据的前置是「写入已接受且响应丢失」。响应有两条丢法：
//   ① 驱动报错——SQL 可能已经落库，只是没人知道；
//   ② 驱动成功、回执在路上丢了——落库了，只是提交者不知道。
// ② 在网关这一侧根本不可观测：驱动把回执交回来了，是**再往后**丢的。
// 只在 ① 的错误分支抬围栏，就等于给 ② 留了「自动重试」的口子，而自动重试
// 恰恰是判据明令不许的那件事。下面两条用例分别钉住 ① 和 ②。

/// 把一次写入真正送进驱动，让它以 `SessionLost` 收场（丢法①）。
///
/// 返回实际交给驱动的 SQL 条数——判据要数的是「发出去几遍」，
/// 不是「网关受理了几遍」：受理成功而驱动报错的那次，SQL 同样已经出去了。
async fn dispatch_into_a_lost_driver(h: &fx::TokenHarness, token: &str) -> usize {
    let id = fx::accept(&h.harness, fx::request_with_key(fx::REVISION, token)).await;
    h.harness
        .port
        .set_execute(Err(RuntimeError::SessionLost("dbse_after_sql".to_owned())));
    let error = err(h.harness.gateway.dispatch(&fx::principal(), &id).await);
    assert!(
        matches!(
            error,
            GatewayError::Runtime(RuntimeError::SessionLost { .. })
        ),
        "驱动应当如实报出会话已丢，实际 {error:?}"
    );
    assert_eq!(
        h.harness.port.executed_requests().len(),
        1,
        "夹具必须真的把请求交给驱动，否则这条用例什么也证明不了"
    );
    1
}

/// 回归：驱动已经发了 SQL 才报 `SessionLost`，换新键自动重试必须发不出第二遍。
///
/// 这是本轮实测里被判为「探针 5 测量失真」的那条：当时量到
/// `SQL_ISSUED_FINAL=2`，因为围栏只在受理期。错不在测量，在实现——
/// 而断言形状（数发出去的 SQL 而不是数受理次数）是对的，所以这里沿用。
#[tokio::test]
async fn a_new_key_cannot_resend_the_sql_after_the_driver_lost_the_session() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let issued_before = dispatch_into_a_lost_driver(&h, &token).await;

    // 客户端拿着全新的键（＝全新的令牌）自动重试。
    let retry = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &retry))
        .await);
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "驱动已经可能写成功，换新键自动重试必须先核验，实际 {error:?}"
    );
    assert_eq!(
        h.harness.port.executed_requests().len(),
        issued_before,
        "被围栏挡下就不该再发一次 SQL"
    );
}

/// 丢法②：驱动成功但回执没到提交者。此时网关这边看起来一切正常，
/// 所以围栏必须在**下发那一刻**就抬起来，不能等报错。
#[tokio::test]
async fn a_new_key_cannot_resend_the_sql_after_a_successful_but_unheard_write() {
    let h = fx::token_harness();
    let id = write_once(
        &h,
        fx::request_with_key(
            fx::REVISION,
            &fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS),
        ),
    )
    .await;
    assert!(!id.is_empty());
    assert_eq!(h.harness.port.executed_requests().len(), 1);

    // 提交者没听到回执，于是自动换键重试一次——判据明令不许自动重试。
    let retry = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &retry))
        .await);
    assert!(
        matches!(error, GatewayError::IdempotencyVerificationRequired { .. }),
        "回执丢失不等于写入没发生，自动重试必须先核验，实际 {error:?}"
    );
    assert_eq!(
        h.harness.port.executed_requests().len(),
        1,
        "同一条语义写入绝不许发第二遍 SQL"
    );
    assert_eq!(h.harness.port.execute_calls(), 1);
}

/// 围栏的另一面：同键重发仍然回放（不要求核验），因为账本里有那条记录，
/// 重放给的是同一条执行，不是新写入。围栏只拦「新键 + 结局未知」。
#[tokio::test]
async fn the_same_token_still_replays_after_the_fence_was_raised_by_dispatch() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let id = write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;

    let replay = fx::accept(&h.harness, fx::request_with_key(fx::REVISION, &token)).await;
    assert_eq!(
        replay.as_str(),
        id,
        "同键重发必须回放同一条执行，不该被自己的围栏挡住"
    );
    assert_eq!(h.harness.port.executed_requests().len(), 1);
}

/// 抬栏点在下发那一刻 ⇒ 「还没下发的受理」不该留围栏：受理了但调用方还没下发、
/// 就被取消或放弃的写入，不该让后来同一语义写入永远做不了。
#[tokio::test]
async fn accepting_without_dispatching_does_not_raise_the_fence() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let id = fx::accept(&h.harness, fx::request_with_key(fx::REVISION, &token)).await;
    assert!(!id.is_empty(), "受理本身应当成功");
    assert_eq!(h.harness.port.execute_calls(), 0, "没下发就不该碰驱动");
    assert_eq!(
        h.harness.gateway.unverified_outcome_count().await,
        0,
        "受理但没下发＝请求没出网关，抬围栏会误伤一次真正没跑过的写入"
    );
}
