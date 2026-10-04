//! §3.5 / CM-60：两段单调耗时——①授权与校验完成→派发驱动，②驱动完成→回执/事件登记，
//! 均不计入驱动往返本身。

use std::sync::Arc;

use crate::gateway_fixtures as fx;
use datazen_runtime::gateway::{AlwaysAllow, InMemoryIdempotencyStore, MonotonicClock};
// ───────────────── G §3.5 CM-60 ─────────────────

#[tokio::test(start_paused = true)]
async fn the_gateway_segment_excludes_the_driver_round_trip() {
    let port = Arc::new(fx::RecordingPort::new(fx::ready_view()).with_driver_nanos(5_000_000));
    let h = fx::harness_with(
        port,
        Arc::new(AlwaysAllow),
        InMemoryIdempotencyStore::shared(),
    );
    assert_eq!(h.clock.now_nanos(), 0);

    let id = fx::accept(&h, fx::request(fx::REVISION)).await;
    h.clock.advance(1_000);
    h.gateway
        .dispatch(&fx::principal(), &id)
        .await
        .expect("应当派发");

    let samples = h.gateway.overhead_samples().await;
    assert_eq!(samples.len(), 1);
    let sample = samples.as_slice()[0];
    // 段①＝授权与参数校验完成 → 派发驱动；不含驱动往返的 5ms。
    assert_eq!(sample.gateway_nanos, 1_000);
    assert!(sample.total_nanos() < 5_000_000);
    // 驱动往返仍然真实发生了（时钟已被驱动推快 5ms）。
    assert_eq!(h.clock.now_nanos(), 5_001_000);
    assert_eq!(h.gateway.overhead_p95().await, Some(1_000));
}

#[tokio::test(start_paused = true)]
async fn the_p95_is_a_nearest_rank_over_every_dispatch() {
    let (h, _store) = fx::counted_harness();
    for k in 1..=20_u64 {
        let mut request = fx::request(fx::REVISION);
        request.idempotency_key = format!("idem-contract-p95-{k}");
        // 20 条互不相同的写入。CM-70 之后**不能**只换键不换写：换新键重发同一条
        // 写入正是「用新键自动重试」，第一次派发一落地就把这次写入围起来了，第二轮
        // 起会被要求显式核验。这条用例量的是派发开销，20 个样本才是它的立身之本，
        // 所以这里改换写入内容而不是改验收目标。
        request.call.input = serde_json::json!({ "sql": format!("select {k}") });
        let id = fx::accept(&h, request).await;
        h.clock.advance(100 * k);
        h.gateway
            .dispatch(&fx::principal(), &id)
            .await
            .expect("应当派发");
    }

    let samples = h.gateway.overhead_samples().await;
    assert_eq!(samples.len(), 20);
    // 20 个样本、p95 ⇒ 秩 = ceil(0.95 * 20) = 19 ⇒ 1900。
    assert_eq!(samples.overhead_p95(), Some(1_900));
    assert_eq!(h.gateway.overhead_p95().await, Some(1_900));
}
