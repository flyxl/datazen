//! `EventSink` 端口的进程内内存实现（WR-D5 前置刀）。
//!
//! 团队服务首版是单实例（API + admin + worker 同进程），本实现即 v1 形态的
//! 事件后备：[`MemoryEventSink::publish`] 把信封写进每流一个的环形回放窗口
//! 并唤醒等待者，[`MemoryEventSink::subscribe`] 返回一个阻塞迭代器，先按
//! 私有游标重放窗口，再实时等待追加。
//!
//! ## 与 WR-D5 SSE 适配的对接点
//!
//! 本模块只做「事件事实的中继」；[web-server-host §8] 的 SSE 语义里，
//! 以下职责**全部留在 SSE 适配层**，不在端口内：
//!
//! * **Origin 校验 / CSRF 替代 / 登录与 L6 授权**（§8.1）：端口不认识身份，
//!   `ctx` 参数不参与任何过滤（有行为钉子钉住这一点）；
//! * **重订阅协议**（§8.2）：`Last-Event-ID` / `?afterSequence=` →
//!   `subscribe(after_sequence)` 的翻译，以及把 subscribe 的 `Err` 翻译成
//!   `streamResetRequired` 帧（见下文「缺口语义」）；
//! * **分页 / 单响应大小限制**（web-ready-plan WR-D5 行）：适配层按帧数/字节
//!   限额从迭代器取数；
//! * **心跳与超时、权限周期复核**（§8.3/§8.4）：订阅迭代器会**无限期阻塞**
//!   等下一条事件，端口不内建超时；适配层需自行驱动 15s keepalive
//!   （如 axum 的 `KeepAlive`）并在独立路径上周期复核登录/权限；
//! * **慢消费者处置**（§8.2「先发 streamResetRequired，再关闭连接」）：迭代器
//!   终止（返回 `None`）即适配层断流的触发点，见下文。
//!
//! [web-server-host §8]: ../../../../docs/architecture/platform/web-server-host.md
//!
//! ## 容量与淘汰
//!
//! 每流环形窗口 [`DEFAULT_RING_CAPACITY`]（1024 条，§8.2「每 stream 环形缓冲，
//! 容量首版建议值 1024 条（来自部署配置）」），超限淘汰**最旧**。部署侧要换
//! 容量时用 [`MemoryEventSink::with_capacity`] 承接配置值。注意 §8.2 刻意把
//! 两个对象分开：这里的窗口是 **Last-Event-ID 回放缓冲**；「每订阅投递队列
//! 256 条 / 1 MiB」是 SSE 层自己的对象。本端口没有每订阅者私有队列——订阅者
//! 只持一个游标，从共享窗口按需克隆，慢消费者以「落出窗口」的形式暴露
//! （见缺口语义），快消费者不受影响。
//!
//! ## 缺口语义（本实现钉死；端口注释未逐条定义的部分在此登记）
//!
//! * 序列从 **1** 起、严格连续；`after_sequence` 是「交付从 after+1 开始」的
//!   补偿位点；`None ≡ Some(0)`（端口测试钉住：没有它就从 0 开始）。
//! * **订阅时**缺口检出，两种情形都返回 `Err(PortError::NotFound)`：
//!   1. `after + 1` 超前于下一条未发布序号——含「进程重启后窗口清零」
//!      （§8.2）与「流从未有过事件但请求了非零位点」；
//!   2. `after + 1` 早于窗口最老保留序号——请求的历史已被淘汰
//!      （§8.2 窗口过期；`None ≡ Some(0)` 同样适用）。
//!
//!   选「错误」而非「合成事件」是保守裁决：窗口里的信封必须是真实发布过的
//!   事实，本实现不伪造 `StreamResetRequired`——合成它需要编造 sequence，
//!   会污染 CM-55 的「sequence 连续」契约。`PortError` 没有缺口专用变体
//!   （已登记为端口语义缺口），借用 `NotFound`：请求的回放位点也是一种
//!   「不存在的资源」。**SSE 适配层必须把 subscribe 的任何 Err 都映射成
//!   streamResetRequired + 关流（§8.2 窗口过期），不要把它当 404。**
//! * **交付时**缺口检出（订阅者落后、游标被淘汰出窗口）：迭代器没有错误
//!   通道，以 `None` 终止，不静默跳号续读。适配层断流 → 客户端带
//!   Last-Event-ID 重连 → 命中订阅时缺口 → streamResetRequired → 客户端按
//!   §8.2 用快照接口重建。
//! * **未知 stream_id 的 subscribe 不报不存在**：按「空窗口 + 等实时」建立
//!   订阅——SSE 先连、首事件后到的竞态必须能成立；流的存在性与归属校验在
//!   适配层（§8.1 L6）。但对该流请求非零位点仍按超前缺口报错：这正是
//!   「进程重启后窗口清零」场景的正确出口。
//!
//! ## 订阅生命周期与并发模型
//!
//! * 订阅 = 共享窗口的 `Arc` + 私有游标。**端口没有取消方法**：适配层丢弃
//!   迭代器即断开，窗口不受影响（重连补偿位点的语义正需要它存续）。
//! * 端口同样没有「流终结/回收」方法，本实现不回收窗口：每流内存上界是
//!   容量常量，窗口总数随进程内出现过的 streamId 增长（v1 单实例可接受，
//!   已登记为缺口；「进程重启后回放窗口清零」由进程生命周期天然保证）。
//! * 并发模型沿用 `jobs/engine/memory_repository.rs` 的先例：`std::sync`
//!   锁 + 短临界区，async fn 内**不持锁跨 await**（发布/订阅的临界区只有
//!   「追加一条」与「克隆一条」）。不用 tokio channel/watch 的理由：订阅
//!   产物是**阻塞** `Iterator`，tokio 接收端必须 `blocking_recv()`，在运行时
//!   worker 上下文里会 panic；`Mutex + Condvar` 对调用上下文无假设，SSE 层
//!   把它放到 `spawn_blocking` 或专用线程泵都成立。发布后 `notify_all`：
//!   多订阅者都要收到同一条，`notify_one` 会丢投。每流独立 `Condvar`，
//!   发布只唤醒本流等待者，避免跨流惊群。

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};

use async_trait::async_trait;

use crate::execution::application::{
    ConnectionEventEnvelope, EventSequence, EventSink, EventSubscription,
};
use crate::shared::context::RequestContext;
use crate::shared::error::PortError;
use crate::shared::id::StreamId;

/// 每 stream 回放窗口容量：§8.2「每 stream 环形缓冲，容量首版建议值 1024 条
/// （来自部署配置）」。这是 Last-Event-ID 回放缓冲的规格值；要承接部署配置
/// 时改用 [`MemoryEventSink::with_capacity`]，不要就地改本常量。
const DEFAULT_RING_CAPACITY: usize = 1024;

/// 单个流的共享窗口：环形缓冲 + 唤醒器。
struct StreamWindow {
    /// 窗口状态锁。临界区只有「追加一条并淘汰」与「按游标克隆一条」。
    ring: Mutex<RingState>,
    /// 与 `ring` 配对的唤醒器（每流独立，发布只唤醒本流等待者）。
    signal: Condvar,
}

/// 窗口状态。环形缓冲里的信封 sequence 严格连续：
/// `oldest_sequence() ..= next_sequence - 1`。
struct RingState {
    /// 下一次 publish 将分配的 sequence（首条事件为 1）。
    next_sequence: u64,
    ring: VecDeque<ConnectionEventEnvelope>,
    capacity: usize,
}

impl RingState {
    /// 窗口内最老保留事件的 sequence；窗口为空（尚无事件）时等于
    /// `next_sequence`（此时任何 `cursor < oldest` 都意味着位点超前，
    /// 语义与「淘汰后落出窗口」一致，不需要特判）。
    fn oldest_sequence(&self) -> u64 {
        self.next_sequence - self.ring.len() as u64
    }
}

impl StreamWindow {
    fn new(capacity: usize) -> Self {
        Self {
            ring: Mutex::new(RingState {
                next_sequence: 1,
                // 预分配封顶 64 槽：VecDeque 会按需增长到 capacity。窗口是
                // 每流一个的，若每流开局就押上 1024 个信封槽（约 1MB），短命
                // 流一多就是纯浪费；增长到上限的过程由 publish 自然的
                // push_back 完成，无额外裁决。
                ring: VecDeque::with_capacity(capacity.min(64)),
                capacity,
            }),
            signal: Condvar::new(),
        }
    }
}

/// 锁获取一律从毒锁恢复：本实现的单条信封操作不会留下半更新状态
/// （push/pop/克隆都是独立完整的），持锁 panic 后复用互斥量数据，
/// 比让整个流的订阅者永久卡死更符合「中继」的角色。
/// 与 `memory_repository` 的 `poisoned.into_inner()` 同款；
/// 迭代器一侧没有错误通道，因此全模块统一这一种取锁方式。
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `EventSink` 的进程内内存实现。线程安全；可跨 tokio 任务共享（`Arc`）。
pub struct MemoryEventSink {
    /// 以 streamId 为键的窗口表。条目在首次 publish/subscribe 时惰性建立，
    /// 之后随进程存续（端口没有流终结方法，见模块文档）。
    streams: Mutex<HashMap<StreamId, Arc<StreamWindow>>>,
    /// 新建窗口的容量（`with_capacity` 可覆盖规格默认值）。
    capacity: usize,
}

impl Default for MemoryEventSink {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryEventSink {
    /// 规格默认容量（§8.2 建议值 1024）。
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_RING_CAPACITY)
    }

    /// 以部署配置的容量构建。容量 0 意味着每条事件发布即淘汰，任何订阅都
    /// 立即落出窗口——那是配置错误而不是运行时状态，直接 fail fast。
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity >= 1, "event replay window capacity must be >= 1");
        Self {
            streams: Mutex::new(HashMap::new()),
            capacity,
        }
    }

    /// 取（或惰性建立）某流的窗口。表锁只保护查表与插入，
    /// 之后的所有工作都在该流自己的锁上进行。
    fn window_for(&self, stream_id: &StreamId) -> Arc<StreamWindow> {
        let mut streams = lock(&self.streams);
        streams
            .entry(stream_id.clone())
            .or_insert_with(|| Arc::new(StreamWindow::new(self.capacity)))
            .clone()
    }
}

#[async_trait]
impl EventSink for MemoryEventSink {
    /// 把信封写进其流对应的环形窗口并唤醒全部等待者。
    ///
    /// * sequence 由本实现按流分配（从 1 起严格单调递增）：传入信封上的
    ///   `sequence` 是调用方的占位值，**一律覆盖**——发布侧不拥有序号的
    ///   决定权，否则「不丢不重」无从谈起。
    /// * 发布者身份（`ctx`）不参与路由与过滤：端口不做鉴权（SSE 层 §8.1），
    ///   内存实现也不留审计投影。
    async fn publish(
        &self,
        ctx: &RequestContext,
        envelope: ConnectionEventEnvelope,
    ) -> Result<EventSequence, PortError> {
        let _ = ctx;
        let stream_id = envelope.stream_id.clone();
        let window = self.window_for(&stream_id);
        let mut ring = lock(&window.ring);

        // u64 序号空间在事件场景下不会耗尽（每纳秒 1 条也要 584 年）；
        // 真到达时宁可显式失败也不静默回绕破坏单调性。
        assert!(
            ring.next_sequence != u64::MAX,
            "event sequence space exhausted for stream {stream_id}"
        );
        let sequence = EventSequence::new(ring.next_sequence);
        let mut stored = envelope;
        stored.sequence = sequence;
        ring.ring.push_back(stored);
        while ring.ring.len() > ring.capacity {
            ring.ring.pop_front();
        }
        ring.next_sequence += 1;
        drop(ring);

        // 先改状态后唤醒：等待者在锁内复查条件，不存在「检查完、睡下前」
        // 丢通知的窗口（等待是原子释放锁的）。
        window.signal.notify_all();
        Ok(sequence)
    }

    /// 建立订阅：检缺口（订阅时）→ 返回从 `after_sequence + 1` 起交付的
    /// 阻塞迭代器。重放与实时共用同一条游标路径：迭代器首次轮询时，
    /// 窗口里 `cursor .. next_sequence` 的部分就是重放段。
    ///
    /// 缺口语义见模块文档；`None ≡ Some(0)`。
    async fn subscribe(
        &self,
        ctx: &RequestContext,
        stream_id: StreamId,
        after_sequence: Option<EventSequence>,
    ) -> Result<EventSubscription, PortError> {
        let _ = ctx;
        let window = self.window_for(&stream_id);
        let ring = lock(&window.ring);

        let after = after_sequence.map(|sequence| sequence.get()).unwrap_or(0);
        let first_needed = after.saturating_add(1);

        // 缺口检出 1/2：请求位点超前（从未发布到那里）。边界
        // `first_needed == next_sequence` 合法：无重放、纯实时等待。
        if first_needed > ring.next_sequence {
            return Err(PortError::NotFound(format!(
                "event replay gap on stream {stream_id}: after {after} is ahead of next sequence {}",
                ring.next_sequence
            )));
        }
        // 缺口检出 2/2：请求位点早于窗口最老保留序号（历史已淘汰）。
        let oldest = ring.oldest_sequence();
        if first_needed < oldest {
            return Err(PortError::NotFound(format!(
                "event replay gap on stream {stream_id}: after {after} precedes oldest retained {oldest}"
            )));
        }

        Ok(Box::new(WindowSubscription {
            window: Arc::clone(&window),
            cursor: first_needed,
        }))
    }
}

/// 一个订阅者：共享窗口 + 私有游标。多订阅者互不干扰——交付是「按各自
/// 游标从共享窗口克隆」，淘汰按窗口整体推进，谁落后谁落出窗口。
struct WindowSubscription {
    window: Arc<StreamWindow>,
    /// 下一条要交付的 sequence（严格连续；交付后 +1）。
    cursor: u64,
}

impl Iterator for WindowSubscription {
    type Item = ConnectionEventEnvelope;

    fn next(&mut self) -> Option<Self::Item> {
        let mut ring = lock(&self.window.ring);
        loop {
            // 交付时缺口检出（慢消费者落出窗口）：终止迭代而不是跳号续读。
            // 迭代器没有错误通道，None 是这里唯一能发出的「不可续」信号；
            // SSE 适配层据此断流，客户端重连时 subscribe 会以 Err 报告缺口
            // （缺口语义见模块文档）。
            let oldest = ring.oldest_sequence();
            debug_assert!(self.cursor <= ring.next_sequence);
            if self.cursor < oldest {
                return None;
            }
            if self.cursor < ring.next_sequence {
                let index = (self.cursor - oldest) as usize;
                let envelope = ring.ring[index].clone();
                self.cursor += 1;
                return Some(envelope);
            }
            // 无新事件：无限期阻塞等待。心跳/超时是 SSE 适配层的职责
            // （§8.3），端口不内建。与 `lock` 同口径地从毒锁恢复。
            ring = self
                .window
                .signal
                .wait(ring)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::execution::application::{ConnectionEvent, EventEnvelope};
    use crate::execution::domain::SessionHandle;
    use crate::shared::id::{
        ClientInstanceId, DbSessionId, OrganizationId, PrincipalId, RequestId, RuntimeEpoch,
    };

    fn context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-1"),
            PrincipalId::new("user-1"),
            None,
            ClientInstanceId::new("client-1"),
            RequestId::new("req-1"),
            None,
        )
    }

    fn other_context() -> RequestContext {
        RequestContext::new(
            OrganizationId::new("org-2"),
            PrincipalId::new("user-2"),
            None,
            ClientInstanceId::new("client-2"),
            RequestId::new("req-2"),
            None,
        )
    }

    /// 占位 sequence 恒为 0：发布侧必须覆盖它，重放出来的 sequence 因此
    /// 顺带证明了「序号由 publish 分配」。
    fn envelope(stream: &str, epoch: &str, reason: &str) -> ConnectionEventEnvelope {
        let payload = ConnectionEvent::StreamResetRequired {
            reason: reason.to_string(),
        };
        let mut envelope = EventEnvelope::new(
            StreamId::new(stream),
            EventSequence::new(0),
            RuntimeEpoch::new(epoch),
            payload,
        );
        envelope.session_handle = Some(SessionHandle::new(
            DbSessionId::new("sess-1"),
            RuntimeEpoch::new(epoch),
        ));
        envelope
    }

    /// 从订阅里取**恰好** n 条。第 n+1 次 next 会无限期阻塞，所以测试
    /// 永远不多取一条。
    fn drain(subscription: &mut EventSubscription, n: usize) -> Vec<ConnectionEventEnvelope> {
        (0..n)
            .map(|_| subscription.next().expect("窗口内应有事件"))
            .collect()
    }

    fn sequences(events: &[ConnectionEventEnvelope]) -> Vec<u64> {
        events.iter().map(|event| event.sequence.get()).collect()
    }

    async fn publish_reset(sink: &MemoryEventSink, stream: &str, reason: &str) -> EventSequence {
        sink.publish(&context(), envelope(stream, "epoch-1", reason))
            .await
            .expect("publish")
    }

    #[tokio::test]
    async fn publish_assigns_strictly_monotonic_sequences_per_stream_from_one() {
        let sink = MemoryEventSink::new();
        let stream = StreamId::new("stream-1");
        for expected in 1u64..=3 {
            let sequence = publish_reset(&sink, stream.as_str(), "tick").await;
            assert_eq!(sequence.get(), expected);
        }
        // 另一条流从 1 重新起算：序号是每流的，不是全局的。
        let other = publish_reset(&sink, "stream-2", "tick").await;
        assert_eq!(other.get(), 1);
    }

    #[tokio::test]
    async fn a_fresh_subscription_replays_everything_in_order_and_overrides_caller_sequences() {
        let sink = MemoryEventSink::new();
        let stream = StreamId::new("stream-1");
        for index in 1..=3 {
            let mut envelope = envelope("stream-1", "epoch-1", "tick");
            // 发布侧塞进来的占位值必须被覆盖（这里故意用非零占位）。
            envelope.sequence = EventSequence::new(999);
            envelope.payload = ConnectionEvent::StreamResetRequired {
                reason: format!("r{index}"),
            };
            sink.publish(&context(), envelope).await.expect("publish");
        }

        let mut subscription = sink
            .subscribe(&context(), stream.clone(), None)
            .await
            .expect("subscribe");
        let replayed = drain(&mut subscription, 3);
        assert_eq!(sequences(&replayed), vec![1, 2, 3], "全量重放且连续");
        for (index, event) in replayed.iter().enumerate() {
            assert_eq!(
                event.sequence.get(),
                index as u64 + 1,
                "调用方占位 sequence(999) 不得漏进窗口"
            );
        }

        // None ≡ Some(0)（端口测试钉住的缺省补偿位点）。
        let mut from_zero = sink
            .subscribe(&context(), stream, Some(EventSequence::new(0)))
            .await
            .expect("subscribe");
        assert_eq!(sequences(&drain(&mut from_zero, 3)), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn a_subscription_from_a_middle_revision_replays_only_the_tail() {
        let sink = MemoryEventSink::new();
        let stream = StreamId::new("stream-1");
        for index in 1..=5 {
            publish_reset(&sink, "stream-1", &format!("r{index}")).await;
        }

        let mut subscription = sink
            .subscribe(&context(), stream.clone(), Some(EventSequence::new(2)))
            .await
            .expect("subscribe");
        let replayed = drain(&mut subscription, 3);
        assert_eq!(sequences(&replayed), vec![3, 4, 5], "从 after+1 起不丢不重");
        assert_eq!(
            replayed[0].payload,
            ConnectionEvent::StreamResetRequired {
                reason: "r3".to_string()
            },
            "重放的是原事件，不是重新生成的"
        );

        // 边界：after = 最新已发布序号 → 无重放、纯实时等待（合法，不报缺口）。
        let live_only = sink
            .subscribe(&context(), stream, Some(EventSequence::new(5)))
            .await
            .expect("after=latest 是合法位点");
        drop(live_only);
    }

    #[tokio::test]
    async fn a_request_ahead_of_the_stream_is_a_gap_error() {
        let sink = MemoryEventSink::new();
        for index in 1..=3 {
            publish_reset(&sink, "stream-1", &format!("r{index}")).await;
        }
        // 超前：位点 4 尚不存在（下一条是 4，请求 after=4 即从 5 起交付）。
        let ahead = sink
            .subscribe(
                &context(),
                StreamId::new("stream-1"),
                Some(EventSequence::new(4)),
            )
            .await;
        assert!(
            matches!(ahead, Err(PortError::NotFound(_))),
            "超前位点必须报缺口（Err(NotFound)）"
        );

        // 新流 + 非零位点 = 同一种超前缺口（进程重启窗口清零的正解就是它）。
        let fresh_ahead = sink
            .subscribe(
                &context(),
                StreamId::new("stream-2"),
                Some(EventSequence::new(1)),
            )
            .await;
        assert!(matches!(fresh_ahead, Err(PortError::NotFound(_))));

        // 但新流 + None 合法：空窗口等实时（SSE 先连、事件后到的竞态）。
        let fresh_live = sink
            .subscribe(&context(), StreamId::new("stream-2"), None)
            .await;
        assert!(fresh_live.is_ok());
    }

    #[tokio::test]
    async fn an_evicted_revision_is_a_gap_error_even_for_none() {
        let sink = MemoryEventSink::with_capacity(4);
        let stream = StreamId::new("stream-1");
        for index in 1..=6 {
            publish_reset(&sink, "stream-1", &format!("r{index}")).await;
        }
        // 容量 4：窗口只剩 3..=6。

        let evicted = sink
            .subscribe(&context(), stream.clone(), Some(EventSequence::new(1)))
            .await;
        assert!(
            matches!(evicted, Err(PortError::NotFound(_))),
            "已淘汰的旧 revision 必须报缺口（Err(NotFound)）"
        );

        // None ≡ Some(0)，同样落在淘汰区。
        let none_evicted = sink.subscribe(&context(), stream.clone(), None).await;
        assert!(matches!(none_evicted, Err(PortError::NotFound(_))));

        // 恰好压着最老保留序号的位点合法，并从那里连续重放。
        let mut boundary = sink
            .subscribe(&context(), stream.clone(), Some(EventSequence::new(2)))
            .await
            .expect("after=2 交付从 3 起，恰好是窗口最老保留序号");
        assert_eq!(sequences(&drain(&mut boundary, 4)), vec![3, 4, 5, 6]);

        let mut tail = sink
            .subscribe(&context(), stream, Some(EventSequence::new(4)))
            .await
            .expect("subscribe");
        assert_eq!(sequences(&drain(&mut tail, 2)), vec![5, 6]);
    }

    #[tokio::test]
    async fn the_default_window_capacity_is_the_spec_ring_size() {
        let sink = MemoryEventSink::new();
        let stream = StreamId::new("stream-cap");
        for index in 1..=DEFAULT_RING_CAPACITY {
            publish_reset(&sink, "stream-cap", &format!("r{index}")).await;
        }
        // 第 1025 条发布把第 1 条挤出窗口。
        publish_reset(&sink, "stream-cap", "r-final").await;

        let first_evicted = sink.subscribe(&context(), stream, None).await;
        assert!(
            matches!(first_evicted, Err(PortError::NotFound(_))),
            "默认容量是 1024：发布 1025 条后，None(≡0) 的起点已被淘汰"
        );

        let mut tail = sink
            .subscribe(
                &context(),
                StreamId::new("stream-cap"),
                Some(EventSequence::new(1)),
            )
            .await
            .expect("after=1 交付从 2 起，仍在窗口内");
        assert_eq!(sequences(&drain(&mut tail, 3)), vec![2, 3, 4]);
    }

    #[tokio::test]
    async fn subscribers_do_not_interfere_and_a_laggard_falls_out_of_the_window() {
        let sink = MemoryEventSink::with_capacity(4);
        let stream = StreamId::new("stream-1");
        let mut fast = sink
            .subscribe(&context(), stream.clone(), None)
            .await
            .expect("subscribe");
        let mut slow = sink
            .subscribe(&context(), stream.clone(), None)
            .await
            .expect("subscribe");

        for index in 1..=3 {
            publish_reset(&sink, "stream-1", &format!("r{index}")).await;
        }
        // fast 全量消费；slow 一条都不取。
        assert_eq!(sequences(&drain(&mut fast, 3)), vec![1, 2, 3]);

        // 再发 3 条：窗口只剩 3..=6，slow 的游标（1）落出窗口。
        for index in 4..=6 {
            publish_reset(&sink, "stream-1", &format!("r{index}")).await;
        }
        // slow 不拖累 fast：fast 继续按自己的游标拿到 4..=6。
        assert_eq!(sequences(&drain(&mut fast, 3)), vec![4, 5, 6]);

        // slow 现在才轮询：交付时缺口 → 迭代器终止（None），绝不跳号续读。
        assert!(
            slow.next().is_none(),
            "落出窗口的订阅必须终止，不得静默跳号"
        );

        // fast 不受 slow 终止的影响。
        publish_reset(&sink, "stream-1", "r7").await;
        assert_eq!(sequences(&drain(&mut fast, 1)), vec![7]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn publish_and_subscribe_work_across_tokio_tasks_with_live_delivery() {
        let sink = Arc::new(MemoryEventSink::new());
        // 先订阅后发布：订阅建立时该流还没有任何事件。
        let mut subscription = sink
            .subscribe(&context(), StreamId::new("stream-live"), None)
            .await
            .expect("subscribe");

        let publisher = {
            let sink = Arc::clone(&sink);
            tokio::spawn(async move {
                let mut published = Vec::new();
                for index in 1..=5 {
                    let sequence =
                        publish_reset(&sink, "stream-live", &format!("live-{index}")).await;
                    published.push(sequence.get());
                }
                published
            })
        };

        // 阻塞迭代器在等待中被发布方唤醒，按序收到全部实时事件。
        let seen = drain(&mut subscription, 5);
        assert_eq!(sequences(&seen), vec![1, 2, 3, 4, 5]);
        assert_eq!(
            publisher.await.expect("publisher task"),
            vec![1, 2, 3, 4, 5]
        );
    }

    #[tokio::test]
    async fn subscriptions_are_not_scoped_by_the_request_context() {
        // 端口不做组织/主体过滤：归属与授权在 SSE 适配层（§8.1 L6）。
        // 用不同组织的 ctx 订阅，照样收到 org-1 发布的事件——把「不过滤」
        // 钉成显式行为，防止将来有人悄悄在这里加第二套鉴权。
        let sink = MemoryEventSink::new();
        publish_reset(&sink, "stream-1", "r1").await;

        let mut subscription = sink
            .subscribe(&other_context(), StreamId::new("stream-1"), None)
            .await
            .expect("跨组织 ctx 不改变端口行为");
        let seen = drain(&mut subscription, 1);
        assert_eq!(sequences(&seen), vec![1]);
    }

    #[tokio::test]
    async fn stale_epoch_events_are_delivered_untouched() {
        // 端口不静默丢过期纪元的事件：belongs_to 过滤发生在前端可见的一侧
        // （端口文档注释），中继必须原样送达。
        let sink = MemoryEventSink::new();
        sink.publish(&context(), envelope("stream-1", "epoch-old", "stale"))
            .await
            .expect("publish");

        let mut subscription = sink
            .subscribe(&context(), StreamId::new("stream-1"), None)
            .await
            .expect("subscribe");
        let seen = drain(&mut subscription, 1);
        assert_eq!(seen[0].runtime_epoch, RuntimeEpoch::new("epoch-old"));
        assert!(!seen[0].belongs_to(&RuntimeEpoch::new("epoch-new")));
    }
}
