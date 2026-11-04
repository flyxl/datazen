//! WR-F7：执行结果发布钩子的服务端装配（`ResultPublisher` 的实现件）。
//!
//! [`crate::execution::driver_exec::ServerDriverExecBackend`] 的
//! `set_publisher` 钩子（与桌面同形状：块行 + 空块 = 完整语句摘要 + 上限）
//! 由主线在 bootstrap 用本模块的 [`execution_result_publisher`] 接上：
//!
//! - sink 是**跨 resolve 共享**的 [`RuntimeResultSink`]——租约源每次
//!   `resolve()` 现构造 `RuntimeConnectionUseCases`（可能被吊销，禁止缓存），
//!   但结果必须落进**准入执行时的那一个 sink**；共享实例由
//!   `RuntimeConnectionUseCases::new_with_parts` 注入（本波新增构造器），
//!   发布钩子与租约源持有同一个；
//! - 分发顺序与桌面 `platform/adapter.rs` 的接线逐字同构：
//!   export 绑定优先（`publish_export_output` 的 `Ok(None)` = 已被绑定
//!   消费）→ 载荷带 `source` 字段走语句摘要通道 → 其余走通用输出通道。
//!
//! 未接线 = fail-closed（driver_exec 的 execute 在 publisher 缺席时拒绝，
//! 不静默吞结果）——本模块的存在让「接线」成为一行 bootstrap 代码。

use std::sync::Arc;

use datazen::execution::application::orchestration::RuntimeResultSink;
use datazen::execution::runtime::connection as rc;

use crate::execution::driver_exec::ResultPublisher;

/// 构造可 `set_publisher` 的发布钩子。`sink` 必须与租约源注入的是同一实例
/// （bootstrap 装配纪律，见模块头）。
pub fn execution_result_publisher(sink: RuntimeResultSink) -> ResultPublisher {
    Arc::new(move |id, output| {
        let sink = sink.clone();
        Box::pin(async move {
            let output = match sink.publish_export_output(&id, output).await {
                Ok(None) => return Ok(()),
                Ok(Some(output)) => output,
                Err(_) => {
                    return Err(rc::ProviderError::ProtocolError(
                        "export publication rejected".into(),
                    ))
                }
            };
            let rejected =
                || rc::ProviderError::ProtocolError("result publication rejected".into());
            match output.get("source").cloned() {
                Some(source) => {
                    match serde_json::from_value::<datazen::execution::domain::StatementResultSource>(
                        source,
                    ) {
                        Ok(source) => sink
                            .publish_statement_output(&id, output, source)
                            .map_err(|_| rejected()),
                        Err(_) => Err(rc::ProviderError::ProtocolError(
                            "invalid result source".into(),
                        )),
                    }
                }
                None => sink.publish_output(&id, output).map_err(|_| rejected()),
            }
        })
    })
}
