//! A8 的**输入侧**：请求 DTO 自己的 `Debug` 也不能把令牌印出来。
//!
//! 上一轮只堵了输出侧（`ExecutionRecord` → `RedactedExecuteRequest`）。输入侧
//! `ExecutionRequest.idempotency_key` 当时仍是**明文**：`format!("{req:?}")` 会把整串
//! 签名令牌连同 MAC 段逐字打印。派生 `Debug` 会一路走到叶子字段，只在**最外层**换一个
//! 包装类型是不够的——这是同一条教训的第二次出现，因此单独成文件盯这一层。
//!
//! 脱敏口径是**裁定**（收口轮定案）：只脱敏凭据字段，`call` 原样输出——它是排查幂等
//! 问题必需的负载（同一个键配了哪条 SQL、参数差在哪）。所以本用例同时断言 `call`
//! **仍然可见**：将来谁把「整个 `call` 也抹掉」当成修复方案，这条会红。

use crate::gateway_fixtures as fx;
use datazen_runtime::gateway::ExecutionRequest;

/// 签名令牌的段数与 MAC 段长度下限：用来证明「本用例断言的对象确实是一段签名令牌」，
/// 而不是一段随手写出来的普通字符串——否则「输出里没有 MAC 段」是空断言。
const TOKEN_SEGMENTS: usize = 4;
const MAC_HEX_MIN: usize = 32;

/// 请求自带的 SQL：负载必须原样出现在 `Debug` 里。
const PAYLOAD_SQL: &str = "select 1";

/// 把一段真正签发出来的令牌打出来，断言它既不在 `Debug` 里、MAC 段也不在。
fn assert_redacted(request: &ExecutionRequest, token: &str) {
    let segments: Vec<&str> = token.split('.').collect();
    assert_eq!(segments.len(), TOKEN_SEGMENTS, "前提：这确实是一段签名令牌");
    assert!(
        segments[3].len() >= MAC_HEX_MIN,
        "前提：最后一段确实是 MAC，不是空壳"
    );

    let rendered = format!("{request:?}");
    assert!(!rendered.contains(token), "请求 Debug 印出了令牌原文");
    assert!(
        !rendered.contains(segments[3]),
        "请求 Debug 印出了 MAC 段：只剩前两段同样能拿去撞库"
    );
    assert!(
        rendered.contains("<redacted>"),
        "idempotency_key 必须显式打成 <redacted>，而不是被整个字段省掉"
    );
}

/// `ExecutionRequest` 的 `Debug` 脱敏，但 `call` 保持可见。
#[tokio::test]
async fn the_input_request_debug_redacts_the_token_and_keeps_the_payload() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let request = fx::request_with_key(fx::REVISION, &token);

    assert_redacted(&request, &token);
    assert!(
        format!("{request:?}").contains(PAYLOAD_SQL),
        "call 是排查幂等必需的负载，脱敏口径只限凭据字段，不包括它"
    );
}

/// 同一个请求在**派发出去之后**再打一次，结论必须不变：脱敏不能只在某条路径上生效。
#[tokio::test]
async fn the_redaction_holds_for_a_request_that_was_actually_dispatched() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, fx::TOKEN_ISSUED_AT_NANOS);
    let request = fx::request_with_key(fx::REVISION, &token);
    let execution_id = fx::accept_and_dispatch(&h.harness, request.clone()).await;
    assert!(h.harness.port.execute_calls() > 0, "前提：这条写真的下发了");

    assert_redacted(&request, &token);
    // 记录公开可取（`execution`），拿它当输出侧对照：两边都必须没有令牌。
    let record = h
        .harness
        .gateway
        .execution(&execution_id)
        .await
        .expect("刚派发的写入必须能取回记录");
    let rendered = format!("{record:?}");
    assert!(!rendered.contains(&token), "执行记录 Debug 印出了令牌原文");
    assert!(
        !rendered.contains(token.split('.').next_back().expect("令牌有段")),
        "执行记录 Debug 印出了 MAC 段"
    );
}
