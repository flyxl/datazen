//! `ExecutionGateway` 的集成契约用例。
//!
//! 刻意只经由 `datazen_runtime::gateway` / `datazen_runtime::registry` 的**公开 API**
//! 驱动网关：不在测试里复用 `src/gateway/**` 的内部测试支撑，也不碰私有字段。
//! 因此这里红了就代表对外契约真的破了，而不是内部实现挪了个地方。
//!
//! 用例按 §3 的主题分文件，单个文件都远低于 800 行上限：
//! - `acceptance.rs` §3.1 受理入口顺序、回执≠结果、拒绝不得静默降级为「已受理」
//! - `idempotency.rs` §3.3 幂等（CM-54/55/56）
//! - `revision.rs` §3.1 乐观并发闸（`expectedContextRevision`）
//! - `provenance.rs` §3.4 授权与来源（CM-61/62）
//! - `cancel.rs` §3.2 取消三态与绑定校验（D-01）
//! - `events.rs` §3.3 事件投递（陈旧 epoch / 乱序 / 空洞 / 大计数）
//! - `timing.rs` §3.5 CM-60 两段单调耗时
//! - `invariants.rs` §4 冻结 DTO、错误类型与源码级不变量
//!
//! 全部为单测试二进制：`gateway_fixtures` 只编译一次，因此不存在「另一半夹具在本
//! 二进制里没人调用」的 `dead_code` 噪音。

mod gateway_fixtures;

#[path = "gateway_contract/acceptance.rs"]
mod acceptance;
#[path = "gateway_contract/cancel.rs"]
mod cancel;
#[path = "gateway_contract/events.rs"]
mod events;
#[path = "gateway_contract/idempotency.rs"]
mod idempotency;
#[path = "gateway_contract/invariants.rs"]
mod invariants;
#[path = "gateway_contract/provenance.rs"]
mod provenance;
#[path = "gateway_contract/revision.rs"]
mod revision;
#[path = "gateway_contract/timing.rs"]
mod timing;

use datazen_runtime::connection::{CommandCall, ExecutionId, RuntimeError};
use datazen_runtime::gateway::{ExecutionRecord, ExecutionRequest, GatewayError};
use gateway_fixtures as fx;

/// 把 `Ok` 变成一条带上下文的失败断言。
fn err<T>(result: Result<T, GatewayError>) -> GatewayError {
    match result {
        Ok(_) => panic!("期望失败，实际成功"),
        Err(error) => error,
    }
}

/// 取出门关原样透传的 `RuntimeError`；若被换成了别的错误类型就直接炸。
fn runtime_err(error: &GatewayError) -> &RuntimeError {
    error
        .runtime()
        .unwrap_or_else(|| panic!("期望 RuntimeError 透传，实际是 {error:?}"))
}

async fn record(h: &fx::Harness, id: &ExecutionId) -> ExecutionRecord {
    h.gateway.execution(id).await.expect("执行记录应当存在")
}

/// 一个语义不同的第二请求：换幂等键**且**换命令。
fn other_request() -> ExecutionRequest {
    let mut request = fx::request(fx::REVISION);
    request.idempotency_key = "idem-contract-other".to_string();
    request.call = CommandCall {
        command: "execute".to_string(),
        input: serde_json::json!({ "sql": "select 2" }),
    };
    request
}
