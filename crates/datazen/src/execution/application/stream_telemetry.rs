//! 流式查询的遥测聚合（B2 driver_command 域刀自宿主下沉）。
//!
//! 桌面宿主把驱动流事件经 `QueryStreamCallback` 端口送往 tauri IPC
//! `Channel`；web 宿主未来送往 SSE。无论哪种形态，宿主在流结束后都需要
//! 同两个数——**受影响行数**（`StatementEnd.rows_affected` 逐语句累计）与
//! **总耗时**（`Done.total_time_ms`，末值胜出）——用于记录查询历史与缓存
//! 失效判断。这份「先累计、再转发」的包装逻辑不依赖任何宿主类型，归
//! modules；事件**发射**本身（往哪送、怎么序列化）留宿主。
//!
//! 事件顺序契约：包装回调对驱动是透明的（每个事件先累计、后原样转发），
//! 因此驱动视角与用户回调视角看到的是同一串事件。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use datazen_driver_api::{QueryStreamCallback, QueryStreamEvent};

/// 一次流式执行的遥测累计器。
///
/// 在流开始前创建，经 [`StreamTelemetry::wrap`] 包住用户回调后交给驱动；
/// 流结束后读回 [`StreamTelemetry::rows_affected`] 与
/// [`StreamTelemetry::total_time_ms`]。
#[derive(Debug, Default)]
pub struct StreamTelemetry {
    rows_affected: Arc<AtomicU64>,
    total_time_ms: Arc<AtomicU64>,
}

impl StreamTelemetry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 把用户回调包成遥测回调：`StatementEnd.rows_affected` 逐语句累计，
    /// `Done.total_time_ms` 记末值，然后**原样转发**给用户回调。
    pub fn wrap(&self, user: QueryStreamCallback) -> QueryStreamCallback {
        let rows = Arc::clone(&self.rows_affected);
        let total_ms = Arc::clone(&self.total_time_ms);
        Arc::new(move |event| {
            match &event {
                QueryStreamEvent::StatementEnd {
                    rows_affected: Some(n),
                    ..
                } => {
                    rows.fetch_add(*n, Ordering::Relaxed);
                }
                QueryStreamEvent::Done { total_time_ms } => {
                    total_ms.store(*total_time_ms, Ordering::Relaxed);
                }
                _ => {}
            }
            user(event);
        })
    }

    /// 流结束后读回：各语句 `rows_affected` 之和（无计数的语句不计入）。
    pub fn rows_affected(&self) -> u64 {
        self.rows_affected.load(Ordering::Relaxed)
    }

    /// 流结束后读回：`Done.total_time_ms`（流未完成时为 0）。
    pub fn total_time_ms(&self) -> u64 {
        self.total_time_ms.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    //! 2026-10-31 随 B2 driver_command 域刀自宿主 `commands/driver_command/
    //! streaming.rs` 下沉（commands-layer-review §3.3）：累计规则是形态中立
    //! 的编排语义（换 axum 宿主原样通过），测试跟逻辑走。用户回调的透传
    //! 顺序在此一并钉住——驱动视角与用户视角必须看到同一串事件。

    use super::*;
    use std::sync::Mutex;

    fn collector() -> (QueryStreamCallback, Arc<Mutex<Vec<QueryStreamEvent>>>) {
        let seen: Arc<Mutex<Vec<QueryStreamEvent>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        (
            Arc::new(move |event| {
                sink.lock().unwrap().push(event);
            }),
            seen,
        )
    }

    #[test]
    fn sums_rows_across_statements_and_keeps_the_last_done_time() {
        let telemetry = StreamTelemetry::new();
        let (user, _seen) = collector();
        let wrapped = telemetry.wrap(user);

        wrapped(QueryStreamEvent::ExecutionStarted {
            execution_id: "exec-1".into(),
        });
        wrapped(QueryStreamEvent::StatementEnd {
            index: 0,
            rows_affected: Some(3),
            execution_time_ms: 10,
            truncated: false,
        });
        wrapped(QueryStreamEvent::StatementEnd {
            index: 1,
            rows_affected: None,
            execution_time_ms: 5,
            truncated: false,
        });
        wrapped(QueryStreamEvent::StatementEnd {
            index: 2,
            rows_affected: Some(4),
            execution_time_ms: 1,
            truncated: true,
        });
        // 末值胜出：中途的 Done 会被最后一个覆盖。
        wrapped(QueryStreamEvent::Done { total_time_ms: 40 });
        wrapped(QueryStreamEvent::Done { total_time_ms: 55 });

        assert_eq!(telemetry.rows_affected(), 7);
        assert_eq!(telemetry.total_time_ms(), 55);
    }

    #[test]
    fn forwards_every_event_to_the_user_callback_in_order() {
        let telemetry = StreamTelemetry::new();
        let (user, seen) = collector();
        let wrapped = telemetry.wrap(user);

        let events = vec![
            QueryStreamEvent::ExecutionStarted {
                execution_id: "exec-2".into(),
            },
            QueryStreamEvent::StatementStart {
                index: 0,
                sql: "SELECT 1".into(),
                columns: Vec::new(),
            },
            QueryStreamEvent::Rows {
                index: 0,
                rows: Vec::new(),
            },
            QueryStreamEvent::StatementEnd {
                index: 0,
                rows_affected: Some(0),
                execution_time_ms: 1,
                truncated: false,
            },
            QueryStreamEvent::Done { total_time_ms: 2 },
        ];
        for event in &events {
            wrapped(event.clone());
        }

        // `QueryStreamEvent` 不派生 PartialEq：经 Debug 形状比对（同一事件
        // 的 Debug 输出是确定的）。
        let observed = seen.lock().unwrap();
        assert_eq!(observed.len(), events.len(), "no event may be swallowed");
        let observed_shapes: Vec<String> =
            observed.iter().map(|event| format!("{event:?}")).collect();
        let expected_shapes: Vec<String> =
            events.iter().map(|event| format!("{event:?}")).collect();
        assert_eq!(
            observed_shapes, expected_shapes,
            "events must be forwarded verbatim and in order"
        );
    }

    #[test]
    fn a_stream_that_never_reaches_done_reports_zero_totals() {
        let telemetry = StreamTelemetry::new();
        let (user, _seen) = collector();
        let wrapped = telemetry.wrap(user);
        wrapped(QueryStreamEvent::StatementEnd {
            index: 0,
            rows_affected: Some(9),
            execution_time_ms: 7,
            truncated: false,
        });

        assert_eq!(telemetry.rows_affected(), 9);
        assert_eq!(telemetry.total_time_ms(), 0);
    }
}
