//! 重复 release/close 的第一、二格：**物理**层的真实并发契约。
//!
//! 判据：
//!
//! ```text
//! **重复 release/close（H）**
//!
//! - 前置：同 Lease、多个关闭请求；cleanup 与 timeout 竞态。
//! - 步骤：并发释放 20 次，再重复查询 tombstone。
//! - 断言：driver close 至多一次有效关闭；预算不负数；隧道不多减引用；重复响应一致。
//! ```
//!
//! 本文件负责前两格（隧道那格在 `cm28_concurrent_tunnel.rs`）：
//! 「driver close 至多一次**有效**关闭」与「预算不负数 / 重复响应一致」。
//!
//! # 为什么必须新写，不能靠既有测试
//!
//! `tunnel_refcount_contract.rs` 里的
//! `one_return_releases_exactly_once_however_many_times_it_is_repeated`
//! 数的是**隧道**端口的 `close`，而且是**顺序** `for` 循环：它既数错了对象
//! （判据第一格的主语是 **driver close**，即 [`PhysicalTransport::close`]），
//! 也没有制造任何竞态。`src/tunnel/journey_single_counter.rs` 的
//! `single_counter_algebra_holds` 同理 —— 嵌套 `for` 的纯代数，没有线程。
//!
//! # 并发形状（是「同时抵达」，不是「多线程顺序」）
//!
//! 每个风暴都由 [`tokio::sync::Barrier`] 把 `CONCURRENCY` 个任务**同时**放行：
//! 20 个任务全部抵达闸门之前，谁也不许开始归还。所以：
//!
//! * 前置「同 Lease、多个关闭请求」是**真的**同时，而不是顺序重放；
//! * 观测快照一律取在**闸门之前**，因此 20 份快照是真正同时的读数；
//! * `ResourceManager` 拿的是 `&mut self`，它必须放在 `Mutex` 后面 ——
//!   **被测的性质正是「这份串行化恰好产生一次有效关闭」**，不是把并发绕开。
//!   锁绝不允许跨 `.await`：每个持锁段落都是独立的同步块。
//!
//! # 数的对象
//!
//! [`BudgetPort`] 是这两格唯一被数的对象：它实现公开的 [`PhysicalTransport`]，
//! 在 `close()` 里同时记 **关闭尝试数**、**关闭确认数**、**物理预算占用**与
//! 其**全程最小值**。全程最小值是「预算不负数」的证据 —— 它在端口的**每一个**
//! 出入口上就地采样，因此覆盖风暴**中间**，而不是只看终态。
//!
//! # 「有效关闭」的口径（判据的自证定义）
//!
//! 判据原文只写「driver close 至多一次**有效**关闭」，而复合词「有效关闭」全文仅此一处
//! 出现，且从未被定义（单字「有效」另有 10 处无关用法，如「仅在对应 provider/worker 有效」、
//! 「旧 session 有效」）。本轨采用的落字口径取自同族文档已经用过的词：
//!
//! * 恢复决策表的 `cleanup 未确认` 行：「保留预算占用/隔离资源；**确认关闭**
//!   或节点隔离后才核销」；
//! * 「close 未确认继续占预算；**确认关闭**后只核销一次」。
//!
//! 故 **「有效关闭」≡ 被确认的关闭**。
//!
//! **推导（为什么「至多一次」只能修饰「确认」）**：归池前检查逐字写着
//! 「任一失败都关闭，即使 driver 返回 Clean 也不能绕过宿主检查。reset 不支持、失败或
//! 超时直接关闭。」—— 关闭失败**不**终止关闭义务，所以关闭**尝试**本就允许多次。
//! 若「至多一次」约束的是尝试次数，它与那条归池前检查直接冲突；两条并列，唯一自洽的读法是
//! 「至多一次**被确认的**关闭」：20 个并发关闭请求里，第一个把关闭确认下来，其余请求
//! 只能看到墓碑并被拒绝。
//!
//! 同一口径也写在实现侧 `packages/runtime/src/resource/manager.rs` 的 `force_close`
//! 错误分支注释里 —— 台账与代码两侧必须同词，否则结论随台账删除而蒸发。
//!
//! # 为什么三条负向对照必须常驻（不要简化掉）
//!
//! **本条断言不能靠「单点变异变红」验收。** 「至多一次有效关闭」在 `release` 里被
//! **三道彼此独立的守卫重复兜住**，任何只拆掉其中一道的变异都打不红它：
//!
//! | 变异 | 目标 | 实跑 |
//! | --- | --- | --- |
//! | 删 `manager.rs` 归零分支的 `table.forget(lease_id)` | 20 路并发归还 | RED（`occupied_slots` `left: 20 / right: 0`），但**关闭计数断言 panic 次数 0** |
//! | 再废掉 `release` 的 `record.state != InUse` 守卫 | 同上 | RED 且同因，**关闭计数断言仍 panic 次数 0** |
//! | 在 `Closed` 分支里把 `transport.close` 多调一次 | 关闭计数 | **RED `left: 2 / right: 1`** —— 只有直接改调用点才打得红 |
//!
//! 结论（已落成注释，合并后仍可从代码读出）：**不变量的过度确定导致「错误形状断言」不能
//! 代替「数物理端口」**。输家早在守卫处翻车，根本走不到关闭调用点。因此本文件必须同时
//! 持有 (1) 数 driver 端口的 M3 型断言，(2) `negative_control_the_driver_port_counter_reaches_more_than_one_close`
//! 与 `negative_control_the_budget_detector_reports_an_over_release`。删掉任何一条，
//! 「关闭恰好发生 N 次」就退化成断言一个常量 —— `+= 1` 改成 `= 1` 不会有任何测试变红。
//! 同理，反向对照必须真能把计数推离期望值：**每轮重建探测器的负控是无效负控**，
//! 隧道那一侧的教训见 `cm28_concurrent_tunnel.rs` 末尾负控的注释。
//!
//! **不要把 `flavor = "multi_thread"` 简化掉。** 实测：并发释放的两个文件合计 **8 处**
//! `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`（本文件 5 处、
//! `cm28_concurrent_tunnel.rs` 3 处）全部降级成默认 `#[tokio::test]`，**10 条测试仍然
//! 全绿**（`EXIT=0`）—— 因为 `&mut self` 的串行化保证结论与线程数无关。这是断言稳健，
//! **不是假并发**：把下表「变异落点」那两行 `Barrier::new(N)` 塌成 `Barrier::new(1)`
//! 立刻 `EXIT=101`，7 条里 **3 条 FAILED**。**下面两类行号含义不同、绝不可互相顶替** ——
//! R2 与 R3 连续两轮都把其中一类当成了另一类；R3 的「当场核对」则是把 R2 的行号抄了回来。
//!
//! | 类别 | 含义 | 取值方式（可复现命令） | 本文件当前值 |
//! | --- | --- | --- | --- |
//! | **变异落点** | 你**动手改**的那两行 `Barrier::new(N)` | `grep -n 'Barrier::new'`（排除本注释块） | 公用 helper `release_storm` 内（其上无 `#[tokio::test]`）、测试 `repeated_tombstone_queries_…` 体内 |
//! | **panic 行** | rustc **报错打印**的断言行，改完去看哪儿。3 个 FAIL **全部**由 helper 那一处透传而来，`repeated_tombstone_queries_…` 体内的断言**仍通过** | 跑一次变异，读 `panicked at` | helper 体内那条断言、以及测试体内靠前的两条断言 |
//!
//! ★ **左值不是常数。** 「panic 行」列里 helper 体内那条与测试体内靠前的那条恒为 `left: 0 / right: 1`（本树 5/5 次）。
//! 靠后的那条是 `left: N / right: 20`，**N 随机器负载变化**：本树空闲负载下连跑 5 次全为
//! `19`；改头前在 R3 内容上连跑 5 次却得 8 / 10 / 14 / 18 / 19，两组唯一差别是负载。
//! **故 `19` 不得当定值**（R3 与 TA 记下的 `19` 只是各自那次负载下的抽样），**只可断言 `N < 20`**。
//! 闸门塌成 1 后第 1 份读数通过即开跑，其余 19 份有多少赶在归还完成前读完并不确定。
//! ★ 上面两列的具体取值对本文件**当前内容**成立。改动本文件任意一行都会让它们失效，
//! 届时按「取值方式」两列重测；**不要用「失效了多少行」倒推位移**，位移只能由
//! `git diff --numstat` 算，且核对时必须打印本文件 blob 以免核到旧副本。
//!
//! 保留 `multi_thread` 的唯一理由：让 20 份抵达读数与端口计数在**真实跨线程争用**下被压到。
//! 它确实钉不住 —— Rust 无法内省 flavor，没有任何测试能断言当前 runtime 的 flavor，
//! 所以这段注释是它唯一的存活形式。**唯一残留的机械防线**来自 tokio 自己：只删
//! `flavor` 而留下 `worker_threads` 会编译失败（``error: The `worker_threads` option
//! requires the `multi_thread` runtime flavor``）。想偷偷降级就必须把整条属性删干净 ——
//! 那是一处显眼的 diff，不是无声的退化。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use datazen_runtime::connection::port::CancelDisposition;
use datazen_runtime::connection::types::{ClientInstanceId, LeaseId};
use datazen_runtime::connection::{
    ConfigRevision, ConnectionId, ExecutionId, NamespaceTarget, OwnerRef, PoolKeyInputs, ResourceId,
};
use datazen_runtime::resource::{
    CleanupDisposition, CleanupReport, DriverCleanVerdict, HostConditionSnapshot, LeasePurpose,
    LeaseRequest, MonotonicSource, PhysicalTransport, ResourceError, ResourceManager,
};

/// 判据步骤：「并发释放 **20** 次」。
const CONCURRENCY: usize = 20;
/// 「再重复查询 tombstone」的重放轮数（每个查询者重复这么多次）。
const TOMBSTONE_REPLAYS: usize = 8;

// ---------------------------------------------------------------- 时钟替身

/// 手动推进的单调时钟：`advance` 之前的时间对台账而言根本没有发生过。
struct TestClock {
    nanos: AtomicU64,
}

impl TestClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            nanos: AtomicU64::new(0),
        })
    }

    fn advance(&self, amount: Duration) {
        self.nanos
            .fetch_add(amount.as_nanos() as u64, Ordering::SeqCst);
    }
}

impl MonotonicSource for TestClock {
    fn now_nanos(&self) -> u64 {
        self.nanos.load(Ordering::SeqCst)
    }
}

// ------------------------------------------------- 物理端口（被测对象）

#[derive(Debug, Clone, PartialEq, Eq)]
enum PhysicalEvent {
    Open(ResourceId),
    Reset(ResourceId),
    Close(ResourceId),
}

/// 端口在某一刻的读数。`min_outstanding` 是**全程**最小值，
/// 由端口在每次 `open` / `close` 上就地更新，所以它是并发窗口内的证据。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BudgetSnapshot {
    outstanding: i64,
    min_outstanding: i64,
    close_attempts: u64,
    close_confirmed: u64,
}

#[derive(Debug)]
struct BudgetInner {
    events: Vec<PhysicalEvent>,
    /// 已开未关的物理资源数。**允许为负** —— 让它真的能变成负数，
    /// 否则「预算不负数」这条断言就只是一句恒真的话。
    outstanding: i64,
    min_outstanding: i64,
    issued: u64,
    live: Vec<ResourceId>,
    close_attempts: u64,
    close_confirmed: u64,
    /// `outstanding` 曾经为负的时刻描述。
    underflows: Vec<String>,
    /// 关了一条**没有对应开启**（或已经关过）的资源 —— 重复有效关闭的端口侧证据。
    unmatched_closes: Vec<String>,
}

/// 第一格与第二格唯一被数的对象。
///
/// 刻意**不**复用 `src/resource/harness.rs` 的替身：那种替身只记事件序列，
/// 计次要靠调用方回扫，既容易数错对象，也看不出「预算有没有真的被还回去」。
struct BudgetPort {
    inner: Mutex<BudgetInner>,
}

impl BudgetPort {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(BudgetInner {
                events: Vec::new(),
                outstanding: 0,
                min_outstanding: 0,
                issued: 0,
                live: Vec::new(),
                close_attempts: 0,
                close_confirmed: 0,
                underflows: Vec::new(),
                unmatched_closes: Vec::new(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, BudgetInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn sample(outstanding: i64, inner: &mut BudgetInner) {
        inner.outstanding = outstanding;
        if outstanding < inner.min_outstanding {
            inner.min_outstanding = outstanding;
        }
        if outstanding < 0 {
            inner
                .underflows
                .push(format!("physical budget went negative: {outstanding}"));
        }
    }

    fn snapshot(&self) -> BudgetSnapshot {
        let inner = self.lock();
        BudgetSnapshot {
            outstanding: inner.outstanding,
            min_outstanding: inner.min_outstanding,
            close_attempts: inner.close_attempts,
            close_confirmed: inner.close_confirmed,
        }
    }

    fn underflows(&self) -> Vec<String> {
        self.lock().underflows.clone()
    }

    fn unmatched_closes(&self) -> Vec<String> {
        self.lock().unmatched_closes.clone()
    }

    fn events(&self) -> Vec<PhysicalEvent> {
        self.lock().events.clone()
    }
}

impl PhysicalTransport for BudgetPort {
    fn open(&self, _request: &LeaseRequest) -> Result<ResourceId, ResourceError> {
        let mut inner = self.lock();
        inner.issued += 1;
        let resource_id = ResourceId::new(format!("res-{}", inner.issued));
        inner.live.push(resource_id.clone());
        inner.events.push(PhysicalEvent::Open(resource_id.clone()));
        Self::sample(inner.outstanding + 1, &mut inner);
        Ok(resource_id)
    }

    fn close(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        let mut inner = self.lock();
        inner.close_attempts += 1;
        match inner.live.iter().position(|live| live == resource_id) {
            Some(index) => {
                inner.live.remove(index);
                inner.close_confirmed += 1;
            }
            None => inner.unmatched_closes.push(format!(
                "close without a matching open: {}",
                resource_id.as_str()
            )),
        }
        inner.events.push(PhysicalEvent::Close(resource_id.clone()));
        Self::sample(inner.outstanding - 1, &mut inner);
        Ok(())
    }

    fn reset(&self, resource_id: &ResourceId) -> Result<(), ResourceError> {
        self.lock()
            .events
            .push(PhysicalEvent::Reset(resource_id.clone()));
        Ok(())
    }

    fn cancel(
        &self,
        _resource_id: &ResourceId,
        _execution_id: &ExecutionId,
    ) -> Result<CancelDisposition, ResourceError> {
        Ok(CancelDisposition::Requested)
    }
}

// ------------------------------------------------------------ 申请与台账构造

fn connection(id: &str) -> ConnectionId {
    ConnectionId::new(id.to_owned())
}

fn owner() -> OwnerRef {
    OwnerRef::ClientSession {
        client_instance_id: ClientInstanceId::new("client-it-cm28"),
    }
}

/// 短操作租约：用途允许归还池，所以归还裁决对这条租约真实成立。
fn request(connection_id: &ConnectionId, database: &str) -> LeaseRequest {
    LeaseRequest::new(
        PoolKeyInputs {
            connection_id: connection_id.clone(),
            config_revision: ConfigRevision::new(1),
            driver_id: "postgres".to_owned(),
            namespace: NamespaceTarget {
                database: database.to_owned(),
                catalog: "appdb".to_owned(),
                schema: "public".to_owned(),
                path: "appdb.public".to_owned(),
            },
            execution_identity_key: "owner-hash-cm28".to_owned(),
            policy_isolation_key: "policy-cm28".to_owned(),
        },
        owner(),
        LeasePurpose::ShortOperation,
    )
}

type SharedManager = Arc<Mutex<ResourceManager>>;

/// 锁绝不允许跨 `.await`，所以它是显式的短作用域辅助函数。
fn manager(shared: &SharedManager) -> MutexGuard<'_, ResourceManager> {
    shared
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn new_manager() -> (SharedManager, Arc<BudgetPort>) {
    let transport = BudgetPort::new();
    let clock = TestClock::new();
    let manager = Arc::new(Mutex::new(ResourceManager::new(transport.clone(), clock)));
    (manager, transport)
}

/// 申请一条租约（驱动判定不干净 ⇒ 归还必走关闭，绝不会走归还池）。
fn acquire_one(shared: &SharedManager, connection_id: &ConnectionId, database: &str) -> LeaseId {
    manager(shared)
        .acquire(&request(connection_id, database))
        .expect("a fresh acquire opens a new physical resource")
        .lease_id
}

/// 一次并发归还的**到达时刻**读数 + 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Arrival {
    occupied_slots: usize,
    outstanding: i64,
}

// ------------------------------------------------------------ 并发归还风暴

/// 启动 `lease_ids.len()` 个任务，用 [`tokio::sync::Barrier`] 让它们**同时**抵达归还点。
///
/// - 到达读数取在闸门**之前**：这些读数因此是真正同时的；
/// - 归还本身在独立的同步块里完成，锁不跨 `.await`；
/// - `even_retire` 打开时，奇数号任务改走 `retire`（超时强制关闭），
///   这就是判据前置点名的「cleanup 与 timeout 竞态」。
async fn release_storm(
    shared: &SharedManager,
    transport: &Arc<BudgetPort>,
    lease_ids: Vec<LeaseId>,
    even_retire: bool,
) -> Vec<(Arrival, Result<CleanupReport, ResourceError>)> {
    let gate = Arc::new(tokio::sync::Barrier::new(lease_ids.len()));
    let mut handles = Vec::with_capacity(lease_ids.len());

    for (index, lease_id) in lease_ids.into_iter().enumerate() {
        let shared = Arc::clone(shared);
        let transport = Arc::clone(transport);
        let gate = Arc::clone(&gate);
        let via_timeout = even_retire && index % 2 == 1;
        handles.push(tokio::spawn(async move {
            let arrival = Arrival {
                occupied_slots: manager(&shared).occupied_slots(),
                outstanding: transport.snapshot().outstanding,
            };
            // 全部任务抵达后才放行：这一刻之前没有任何一次归还开始。
            gate.wait().await;
            let outcome = {
                let mut guard = manager(&shared);
                if via_timeout {
                    guard.retire(&lease_id)
                } else {
                    guard.release(
                        &lease_id,
                        HostConditionSnapshot::default(),
                        DriverCleanVerdict::reported_unclean(),
                    )
                }
            };
            (arrival, outcome)
        }));
    }

    let mut results = Vec::with_capacity(handles.len());
    for handle in handles {
        results.push(
            handle
                .await
                .expect("a spawned release task never panics: the manager returns errors"),
        );
    }
    results
}

fn split_outcomes(
    results: &[(Arrival, Result<CleanupReport, ResourceError>)],
) -> (Vec<&CleanupReport>, Vec<&ResourceError>) {
    let mut winners = Vec::new();
    let mut losers = Vec::new();
    for (_, outcome) in results {
        match outcome {
            Ok(report) => winners.push(report),
            Err(error) => losers.push(error),
        }
    }
    (winners, losers)
}

/// 「重复响应一致」：所有失败响应的**逐字**形态必须完全相同。
fn assert_every_loser_is_identical(losers: &[&ResourceError], expected: usize) {
    assert_eq!(
        losers.len(),
        expected,
        "所有重复请求都必须失败，只允许一个赢家"
    );
    let first = losers
        .first()
        .expect("the list of losers is non-empty by the assertion above");
    for loser in losers {
        assert_eq!(
            *loser, *first,
            "重复的关闭请求必须给出逐字一致的拒绝，而不是时变答案"
        );
    }
}

/// 「driver close 至多一次有效关闭」+「预算不负数」的共同见证。
fn assert_driver_closed_exactly_once(transport: &BudgetPort, expected: u64) {
    let snapshot = transport.snapshot();
    assert_eq!(
        snapshot.close_attempts, expected,
        "driver close 至多一次有效关闭：关闭尝试数必须精确等于 {expected}"
    );
    assert_eq!(
        snapshot.close_confirmed, expected,
        "每一次关闭尝试都必须被端口确认；未确认的关闭不计为有效关闭"
    );
    assert_eq!(snapshot.outstanding, 0, "全部归还完成后物理预算必须归零");
    assert_eq!(
        snapshot.min_outstanding, 0,
        "预算在任何时刻都不得为负 —— 全程最小占用就是证据"
    );
    assert!(
        transport.underflows().is_empty(),
        "物理预算出现过负数：{:?}",
        transport.underflows()
    );
    assert!(
        transport.unmatched_closes().is_empty(),
        "端口被叫去关闭了一条没有对应开启的资源：{:?}",
        transport.unmatched_closes()
    );
}

// ------------------------------------------------ 断言一 / 前置一：同 Lease

/// 前置「同 Lease、多个关闭请求」+ 断言「driver close 至多一次有效关闭」。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_concurrent_returns_of_one_lease_close_the_driver_exactly_once() {
    let (shared, transport) = new_manager();
    let connection_id = connection("conn-cm28-same-lease");
    let lease_id = acquire_one(&shared, &connection_id, "appdb");
    assert_eq!(manager(&shared).occupied_slots(), 1);

    let results = release_storm(&shared, &transport, vec![lease_id; CONCURRENCY], false).await;

    // 20 个任务全部在闸门前读数：此刻归还还没开始，所以每一份都必须是 1 / 1。
    for (index, (arrival, _)) in results.iter().enumerate() {
        assert_eq!(
            arrival.occupied_slots, 1,
            "第 {index} 份到达读数必须看到那条唯一在用的租约"
        );
        assert_eq!(
            arrival.outstanding, 1,
            "第 {index} 份到达读数必须看到那条唯一占用的物理预算"
        );
    }

    let (winners, losers) = split_outcomes(&results);
    assert_eq!(winners.len(), 1, "同一条租约只允许一次有效归还");
    assert_eq!(winners[0].disposition, CleanupDisposition::Closed);
    assert!(winners[0].physical_budget_released);
    assert_every_loser_is_identical(&losers, CONCURRENCY - 1);
    assert!(
        matches!(losers[0], ResourceError::UnknownResource(_)),
        "后到的关闭请求只能看到租约行已摘除，拿到的是未知资源拒绝，实得 {:?}",
        losers[0]
    );
    assert_eq!(losers[0].reason(), "resourceUnknown");

    assert_driver_closed_exactly_once(&transport, 1);
    let events = transport.events();
    assert_eq!(events.len(), 2, "端口只应被开一次、关一次");
    assert!(matches!(events[0], PhysicalEvent::Open(_)));
    assert!(matches!(events[1], PhysicalEvent::Close(_)));
    assert_eq!(
        manager(&shared).occupied_slots(),
        0,
        "关闭确认后物理槽位必须被释放"
    );
}

// -------------------------------------------- 步骤：再重复查询 tombstone

/// 步骤「并发释放 20 次，**再重复查询 tombstone**」+ 断言「重复响应一致」。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_tombstone_queries_answer_identically_after_a_concurrent_storm() {
    let (shared, transport) = new_manager();
    let connection_id = connection("conn-cm28-tombstone");
    let lease_id = acquire_one(&shared, &connection_id, "appdb");

    let results = release_storm(
        &shared,
        &transport,
        vec![lease_id.clone(); CONCURRENCY],
        false,
    )
    .await;
    let (winners, losers) = split_outcomes(&results);
    assert_eq!(winners.len(), 1);
    assert_every_loser_is_identical(&losers, CONCURRENCY - 1);

    // 风暴之后：墓碑查询本身就是被重复并发发起的那一件事。
    let gate = Arc::new(tokio::sync::Barrier::new(CONCURRENCY));
    let mut handles = Vec::with_capacity(CONCURRENCY);
    for _ in 0..CONCURRENCY {
        let shared = Arc::clone(&shared);
        let lease_id = lease_id.clone();
        let gate = Arc::clone(&gate);
        handles.push(tokio::spawn(async move {
            gate.wait().await;
            let mut answers = Vec::with_capacity(TOMBSTONE_REPLAYS);
            for _ in 0..TOMBSTONE_REPLAYS {
                answers.push({
                    let guard = manager(&shared);
                    (
                        guard.lease(&lease_id).is_some(),
                        guard.occupied_slots(),
                        guard.lease_count(),
                    )
                });
            }
            answers
        }));
    }

    let mut first_answer = None;
    for handle in handles {
        let answers = handle
            .await
            .expect("a spawned tombstone task never panics: it only reads");
        for answer in answers {
            match &first_answer {
                None => first_answer = Some(answer),
                Some(first) => {
                    assert_eq!(*first, answer, "重复查询墓碑必须给出逐字一致的答案")
                }
            }
        }
    }

    assert_eq!(
        first_answer,
        Some((false, 0, 0)),
        "关闭确认之后墓碑必须稳定地回答「这条租约不存在、预算为零」"
    );
    assert_driver_closed_exactly_once(&transport, 1);
}

// ------------------------------- 前置第二半：cleanup 与 timeout 竞态（同一租约）

/// 前置「cleanup 与 timeout 竞态」：10 次 `release` 与 10 次 `retire` 同时打同一条租约。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cleanup_and_timeout_racing_on_one_lease_still_confirm_the_driver_close_once() {
    let (shared, transport) = new_manager();
    let connection_id = connection("conn-cm28-race");
    let lease_id = acquire_one(&shared, &connection_id, "appdb");

    let results = release_storm(&shared, &transport, vec![lease_id; CONCURRENCY], true).await;

    // 两条入口的到达读数必须一致：竞态发生前它们看见的是同一个世界。
    for (arrival, _) in &results {
        assert_eq!(arrival.occupied_slots, 1);
        assert_eq!(arrival.outstanding, 1);
    }

    let (winners, losers) = split_outcomes(&results);
    assert_eq!(
        winners.len(),
        1,
        "cleanup 与 timeout 打到同一条租约，只允许一个赢家"
    );
    assert_every_loser_is_identical(&losers, CONCURRENCY - 1);
    assert_driver_closed_exactly_once(&transport, 1);
    assert_eq!(manager(&shared).occupied_slots(), 0);
}

// ------------------------------------------- 断言「预算不负数」：20 条租约同时归还

/// 断言「预算不负数」在**真正有压力**的形状上成立：20 条同时在用的租约被同时归还。
///
/// 同租约风暴里预算最多被打到 1，这里才是预算会被真正并发扣减的地方。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn twenty_leases_returned_concurrently_never_drive_the_physical_budget_negative() {
    let (shared, transport) = new_manager();
    let connection_id = connection("conn-cm28-budget");
    let lease_ids: Vec<LeaseId> = (0..CONCURRENCY)
        .map(|index| acquire_one(&shared, &connection_id, &format!("appdb-{index}")))
        .collect();

    assert_eq!(
        manager(&shared).occupied_slots(),
        CONCURRENCY,
        "归还之前预算必须是满的，否则这条测试没在施压"
    );
    assert_eq!(transport.snapshot().outstanding, CONCURRENCY as i64);

    let results = release_storm(&shared, &transport, lease_ids, false).await;

    for (index, (arrival, _)) in results.iter().enumerate() {
        assert_eq!(
            arrival.occupied_slots, CONCURRENCY,
            "第 {index} 份到达读数必须看到全部 {CONCURRENCY} 条租约都还在"
        );
        assert_eq!(
            arrival.outstanding, CONCURRENCY as i64,
            "第 {index} 份到达读数必须看到全部 {CONCURRENCY} 份物理预算都还占着"
        );
    }

    let (winners, losers) = split_outcomes(&results);
    assert!(losers.is_empty(), "20 条**互不相同**的租约各自都该归还成功");
    assert_eq!(winners.len(), CONCURRENCY);
    for report in winners {
        assert_eq!(report.disposition, CleanupDisposition::Closed);
        assert!(report.physical_budget_released);
    }

    // 每条租约一次有效关闭 —— 20 次，正好等于租约数，不多不少。
    assert_driver_closed_exactly_once(&transport, CONCURRENCY as u64);
    assert_eq!(manager(&shared).occupied_slots(), 0);
}

// ------------------------------------------------------------ 负向对照（必须能红）

/// 负向对照一：driver close 的计数**不是**天生等于 1。
///
/// 如果这个对照跑不过，上面所有「至多一次」的断言就只是在断言一个常量。
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn negative_control_the_driver_port_counter_reaches_more_than_one_close() {
    let (shared, transport) = new_manager();
    let connection_id = connection("conn-cm28-negative");
    let lease_ids: Vec<LeaseId> = (0..3)
        .map(|index| acquire_one(&shared, &connection_id, &format!("appdb-{index}")))
        .collect();

    let results = release_storm(&shared, &transport, lease_ids, false).await;
    let (winners, losers) = split_outcomes(&results);
    assert!(losers.is_empty());
    assert_eq!(winners.len(), 3);
    assert_eq!(
        transport.snapshot().close_confirmed,
        3,
        "三条不同租约各自关闭一次 ⇒ 计数必须能走到 3，证明它不是一个恒为 1 的常数"
    );
}

/// 负向对照二：「预算不负数」这条探测器**真的会红**。
#[test]
fn negative_control_the_budget_detector_reports_an_over_release() {
    let transport = BudgetPort::new();
    assert!(transport.underflows().is_empty());
    transport
        .close(&ResourceId::new("res-never-opened"))
        .expect("the port reports the anomaly in its ledger, not as a refusal");
    assert!(
        !transport.underflows().is_empty(),
        "关一条从未开启的资源必须让预算探测器报警"
    );
    assert_eq!(
        transport.snapshot().min_outstanding,
        -1,
        "全程最小占用必须真的能取到负值"
    );
    assert!(
        !transport.unmatched_closes().is_empty(),
        "无对应开启的关闭必须被单独记名"
    );
}

/// 兜底：时钟替身不参与并发路径，但 `MonotonicSource` 是必实现的口子，顺手钉住它。
#[test]
fn the_shared_clock_is_a_monotonic_source() {
    let clock = TestClock::new();
    assert_eq!(clock.now_nanos(), 0);
    clock.advance(Duration::from_secs(61));
    assert_eq!(clock.now_nanos(), 61_000_000_000);
}
