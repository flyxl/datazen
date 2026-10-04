//! A4：超过记录保留期删除记录，再重放令牌——不得再执行。
//!
//! 这一条是协调方点名的**反面用例**所在：光说「删除后重放不执行」是不够的，
//! 必须同时证明「令牌还没过期、但授予已经没了」这个窗口是**关着的**。
//! 否则 `token_gate 通过 → 账本 Miss → 真的执行一次`，正好是 CM-70 禁止的那件事。

use crate::gateway_fixtures as fx;
use crate::{err, token_reason, write_once};
use datazen_runtime::gateway::{token_digest, RETENTION_AFTER_EXPIRY_NANOS};

/// 扫到「过期 + 保留期」之后，账本里那条记录真的没了。
#[tokio::test]
async fn the_sweep_actually_deletes_the_ledger_record() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;

    let scope = fx::request_with_key(fx::REVISION, &token).scope();
    assert!(h.store.peek(&scope).is_some(), "受理后账本里必须有记录");
    assert_eq!(h.store.delete_calls(), 0);

    // 扫在有效期之内：此时令牌还活着，绝不能删。
    let before_expiry = fx::TOKEN_EXPIRES_AT_NANOS - 1;
    h.harness.clock.advance(before_expiry);
    let early = h.harness.gateway.sweep_idempotency_retention(before_expiry);
    assert_eq!(early.retired, 0, "未过期不得删除");
    assert_eq!(early.ledger_deleted, 0);
    assert!(early.refused.is_empty(), "正常数据下不变量不该被报警");
    assert!(h.store.peek(&scope).is_some(), "未过期记录必须还在");
    assert_eq!(h.store.delete_calls(), 0);

    // 扫过「过期 + 保留期」。
    let now = fx::TOKEN_EXPIRES_AT_NANOS + RETENTION_AFTER_EXPIRY_NANOS;
    let sweep = h.harness.gateway.sweep_idempotency_retention(now);
    assert_eq!(sweep.retired, 1);
    assert_eq!(sweep.ledger_deleted, 1, "账本记录必须真的删掉");
    assert_eq!(h.store.delete_calls(), 1);
    assert!(
        h.store.peek(&scope).is_none(),
        "扫完之后账本里不该还留着这条记录"
    );
}

/// 删完记录再重放 ⇒ 拒绝，且不下发。
#[tokio::test]
async fn a_replay_after_the_record_was_deleted_never_executes() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;
    assert_eq!(h.harness.port.execute_calls(), 1);

    let now = fx::TOKEN_EXPIRES_AT_NANOS + RETENTION_AFTER_EXPIRY_NANOS;
    let sweep = h.harness.gateway.sweep_idempotency_retention(now);
    assert_eq!(sweep.ledger_deleted, 1);

    // 墓碑是有界的：令牌自己也过期之后，再扫一次就把它剪掉，且不删任何东西。
    // 这一步把「墓碑不会无限堆积」钉住，也把下面的拒绝理由从「退役」换成「过期」。
    let second = h.harness.gateway.sweep_idempotency_retention(now);
    assert_eq!(second.ledger_deleted, 0, "记录已经没了，不该再删");
    assert!(
        second.tombstones_pruned == 1 && second.refused.is_empty(),
        "令牌自身到期后墓碑必须被剪掉：{second:?}"
    );

    // 钟必须一起推到清扫那一刻：清扫时刻与闸门时刻都取同一个时钟，
    // 否则就成了「用未来的钟删记录、用过去的钟验收」的自欺。
    h.harness.clock.advance(now);

    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await);
    assert_eq!(
        token_reason(&error),
        "submissionTokenExpired",
        "墓碑此时已随到期剪掉，由过期判定接手"
    );
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "删完记录之后重放令牌绝不能落到驱动"
    );
}

/// **协调方点名的反面用例**：令牌还新鲜，授予已经没了 ⇒ 仍然拒绝、仍然不下发。
///
/// 没有这条，上面那条就可能是假的安全：删除动作若发生得比过期早，
/// 令牌闸门三项全过（验签 / 未过期 / epoch 匹配），请求会一路掉到账本查询，
/// 拿到 `Miss` 然后**真的执行一次**。
#[tokio::test]
async fn a_live_token_whose_grant_is_already_gone_is_refused_not_executed() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;
    assert_eq!(h.harness.port.execute_calls(), 1);

    let digest = token_digest(&token);
    assert!(
        h.tokens.registry().grant(&digest).is_some(),
        "受理成功后必须有授予"
    );

    // 刻意在令牌**尚未过期**时撤掉授予——这正是要杀死的那种状态。
    let retire_at = 1_000 + fx::TOKEN_TTL_NANOS - 1;
    assert!(
        retire_at < 1_000 + fx::TOKEN_TTL_NANOS,
        "本用例的前提就是令牌还活着"
    );
    assert!(h.tokens.registry().force_retire(&digest, retire_at));
    assert!(
        h.tokens.registry().grant(&digest).is_none(),
        "授予必须已消失"
    );

    // 账本记录此刻**还在**——所以拒绝不可能来自账本，只能来自令牌闸门。
    let scope = fx::request_with_key(fx::REVISION, &token).scope();
    assert!(h.store.peek(&scope).is_some());

    h.harness.clock.advance(fx::TOKEN_TTL_NANOS - 2);
    let reads_before = h.store.read_calls();
    let error = err(h
        .harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await);
    assert_eq!(
        token_reason(&error),
        "submissionTokenRetired",
        "授予已删而令牌未过期时，必须由墓碑拦下"
    );
    assert_eq!(
        h.harness.port.execute_calls(),
        1,
        "这一条是 CM-70 的核心：窗口开着就等于重复执行了一次写入"
    );
    assert_eq!(
        h.store.read_calls(),
        reads_before,
        "墓碑必须拦在账本之前；一旦落到账本就是 Miss → 重新执行"
    );
}

/// 反面用例的对照：没撤授予时，同一条令牌照样受理。
///
/// 少了这条，上一条可能只是「反正都拒」的假阳性。
#[tokio::test]
async fn without_the_grant_being_retired_the_same_token_still_replays() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;

    h.harness.clock.advance(1_000);
    h.harness
        .gateway
        .accept(&fx::principal(), fx::request_with_key(fx::REVISION, &token))
        .await
        .expect("授予还在时重放应当拿到原回执");
    assert_eq!(h.harness.port.execute_calls(), 1);
}

/// 未过期时反复清扫，反复拒绝删。
///
/// 删了就开窗；这条把「反复清扫不会把记录提前删没」钉住。
#[tokio::test]
async fn repeated_sweeps_before_expiry_delete_nothing() {
    let h = fx::token_harness();
    let token = fx::issue_token(&h.tokens, 1_000);
    write_once(&h, fx::request_with_key(fx::REVISION, &token)).await;
    let scope = fx::request_with_key(fx::REVISION, &token).scope();

    for _ in 0..5 {
        let sweep = h
            .harness
            .gateway
            .sweep_idempotency_retention(fx::TOKEN_TTL_NANOS - 1);
        assert_eq!(sweep.retired, 0);
        assert_eq!(sweep.ledger_deleted, 0);
        assert!(h.store.peek(&scope).is_some());
    }
    assert_eq!(h.store.delete_calls(), 0, "一次都不许删");
}

/// 没接令牌层的网关清扫为空操作，而不是 panic。
#[tokio::test]
async fn a_gateway_without_the_token_layer_sweeps_into_nothing() {
    let h = fx::ready_harness();
    let sweep = h.gateway.sweep_idempotency_retention(u64::MAX);
    assert_eq!(sweep.retired, 0);
    assert_eq!(sweep.ledger_deleted, 0);
    assert_eq!(sweep.tombstones_pruned, 0);
    assert!(sweep.refused.is_empty());
}
