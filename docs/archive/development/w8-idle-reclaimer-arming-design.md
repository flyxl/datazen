# W8 IdleReclaimer 武装设计：共享内核复用 + server 接线（TTL 策略与 revoke 触发点裁决）

> **设计日期**：2026-11-06。**基线**：main `0d1185a8`（#195 刀4 result-cell 合入点，分支 `w8-idle-reclaimer-design` 逐行核实）。
> **输入**：[w8-mutation-knife0-1-design.md](w8-mutation-knife0-1-design.md) §1.5 风险 3 与 [web-ready-plan.md](web-ready-plan.md) §11 2026-11-05 台账条目（「server 武装 IdleReclaimer + TTL 策略……登记为后续刀」）+ 用户拍板方向（2026-11-06：IdleReclaimer 能力必须共享层实现，策略+调度内核下沉共享层，两形态各写薄适配器，TTL/策略参数分开配置）。
> **性质**：纯设计文档，零产品代码。本文每个断言指回真实 `file:line`（在基线 `0d1185a8` 上逐行核实，非转录台账）。
> **验证基线**：server 全量本地验证须 `cargo test -p datazen-server --features mcp,use-case-contracts`（w8-handoff §3 纪律 3）。

---

## 0. 一句话结论与一个重要的事实修正

**结论**：本刀 = `crates/datazen-server` 三处小改动（lease.rs 策略注入 + 新增一个回收驱动器件 + config 两个环境变量键）+ bootstrap 一处装配；**共享层（`crates/datazen`）零改动、桌面（`datazen-desktop`）零改动**。

**事实修正（先于一切设计）**：用户拍板方向是「策略+调度内核下沉共享层」。逐行核实后的结论是——**这一步已经完成了**。`IdlePolicy`、`IdleReclaimer`、`reclaim_idle()` 全部住在共享 crate：`crates/datazen/src/execution/application/orchestration/idle.rs`（经 `orchestration/mod.rs:17` 与 `execution/application/mod.rs` 双层 re-export 挂到 `datazen::execution::application::IdlePolicy/IdleReclaimer`）。桌面的 `crates/datazen-desktop/src/platform/adapter.rs` 不是实现者，而是**薄适配器**：只做策略覆盖（`adapter.rs:511-514`）、起停（`adapter.rs:663,724-753`）、退出停车（`bootstrap/run.rs:640-656`）——这正是 host-layer-inventory §1「宿主只做形态专有的事」（`host-layer-inventory.md:13-38`）判据下的合法形状。刀0 台账原话「桌面有、server 未武装」（`web-ready-plan.md:181`）指的正是：**共享件已就位、桌面已接线、server bootstrap 没有 spawn 这个驱动器**。因此本刀不「下沉」，只「武装」。

---

## 1. 背景与现状（四条证据，逐条落锚）

### 1.1 证据一：共享内核已存在且自足

`crates/datazen/src/execution/application/orchestration/idle.rs`（358 行）：

| 件 | 位置 | 语义 |
| --- | --- | --- |
| `IdlePolicy` | `idle.rs:56-61` | `deadline: Option<Duration>` + `detached_grace: Option<Duration>` + `scan_interval: Duration`；`None` 是**配置事实**不是计数器为 0（与 `ai::domain::budget`/`resource` 同一门规，`idle.rs:52-55`） |
| `IdlePolicy::default()` | `idle.rs:63-71`（常量 `:36-38,49`） | 期限 **30 分钟**（§9.2）+ 摘挂载宽限 60 秒（与资源池 `IDLE_POOL_TTL_SECONDS = 60` 同口径，`runtime/resource/mod.rs:284`）+ 扫描间隔 3 分钟 |
| 策略构造器 | `idle.rs:74-106` | `disabled()` / `with_deadline(deadline, scan_interval)` / `with_detached_grace(Option)` / `with_scan_interval`（各自只改自己那份，`idle.rs:329-356` 四条钉子） |
| `reclaim_idle()` | `idle.rs:191-229` | 一轮回收的完整序：`sink.reclaim_expired()` → `reclaim_receipts()` → `registry.reclaim_idle_resources()` → `registry.evict_idle_at(clock.now_ms())` → 逐个被驱逐会话做**应用层清账**：`live.revoke`（先作废借用）→ 事务闸/unknown 清理 → `sessions.remove` → `directory.release(handle, Closed)` → `sink.invalidate`；`interrupted_borrows` 留可观测痕迹（`idle.rs:134-137,218-225`） |
| `start_idle_reclaimer(self: &Arc<Self>)` | `idle.rs:235-253` | **应用层自有**的 tokio 周期循环：`sleep(scan_interval)` → 翻旗检查 → `reclaim_idle()` → 有驱逐打 `tracing::info!(count)`（`idle.rs:247-249`）。宿主不需要（也不应该）再挂定时器 |
| `IdleReclaimer` 句柄 | `idle.rs:139-172` | `stop()`（下一轮循环边界退出）+ `is_finished()`（可测性投影，`idle.rs:151-164` 注释写明为什么必须暴露）+ `Drop`（stop + abort，幂等） |
| `arm_detached_grace` | `idle.rs:263-282` | detach 后把期限收到「现在 + 宽限」；由共享用例 `detach_session` 内部调用（`orchestration/operations.rs:482-497`），宿主不直接碰 |

**建会话侧的期限写入也在共享层、且 server 上已经在生效**：`open_session` 组装 `OpenRequest` 时写 `idle_deadline_ms: self.idle_deadline_ms()`（`orchestration/mod.rs:699-711`），`new_with_parts` 的实例内缺省策略就是 `IdlePolicy::default()`（`orchestration/mod.rs:226`）。这意味着 **server 现状的病灶不是「没写期限」而是「没人扫描」**——期限已随每个会话写入 registry（绝对单调毫秒），但 server 进程里没有任何东西调用过 `evict_idle_at`，于是「建好就废」的旧病以新形态存在：期限写着、驱动器缺席。这正是 idle.rs 模块头描述的「回收器**建好了，却一次也够不着会话**」的镜像（`idle.rs:1-6`）。

### 1.2 证据二：桌面已武装（薄适配器形状）

`crates/datazen-desktop/src/platform/adapter.rs`：

- 句柄槽：`idle_reclaimer: std::sync::Mutex<Option<datazen::execution::application::IdleReclaimer>>`（`adapter.rs:138-145`，注释写明「归应用层所有，宿主只负责在退出时停它」）。
- 策略覆盖：`IdlePolicy::default().with_scan_interval(15s)`（`adapter.rs:505-514`）——**期限保持 §9.2 的 30 分钟不动**，只把扫描加密到 15 秒，理由（摘挂载宽限 60 秒不能被 3 分钟扫描拖成 4 分钟）逐字在注释里（`adapter.rs:505-510`）。
- 启动时点：`services` 注入之后（`adapter.rs:659-663`）——让「装配已 FullyWired」与「回收器已在跑」不出现反序窗口。
- 接线可观测：`log_runtime_wiring` 打 `live_connections` / `idle_reclaim_enabled` 两个配置事实，回收器缺席时 `tracing::warn!`（`adapter.rs:709-717`）。
- 停车：`RunEvent::ExitRequested` → `adapter.stop_idle_reclaimer()`（`bootstrap/run.rs:640-656`；只翻标志不 abort，让进行中的一轮 `reclaim_idle` 跑完，理由在 `run.rs:649-653` 注释）。

### 1.3 证据三：server 未武装，且它的实例归属在租约源

`crates/datazen-server` 全 crate 检索 `start_idle_reclaimer` / `with_idle_policy`：**零调用**（`start_idle_reclaimer` 全仓四个命中文件里没有 datazen-server）。server 的 runtime 实例出生在租约源慢路：`ServerRuntimeLeaseSource::resolve()` 在 cell（`current: Mutex<Option<Arc<RuntimeConnectionUseCases>>>`，CM-63）内现构造实例（`crates/datazen-server/src/execution/lease.rs:156-198`），构造后只接活连接端口（`with_live_source` → 慢路 `with_live_connections`，`lease.rs:127-134,191-195`）——策略保持实例缺省 `IdlePolicy::default()`，而扫描器从未被 spawn。

租约源的 `revoke()`（`lease.rs:144-153`）置空 cell、下次 resolve 重建；模块头「吊销语义」（`lease.rs:21-26`）原文：旧实例整体作废——旧会话按既有 `SessionLost` 语义失效（等价「从未存在」的 `SessionNotFound`；结果落共享 sink 不受影响）；**v1 只暴露 API，bootstrap 不接线**。

### 1.4 证据四：server 侧「会话目录」的真实现状

bootstrap 实际接线的会话目录是**共享层**的 `InMemorySessionDirectory`（`bootstrap.rs:381-395` 构造租约源时 new、`:402-405` 经 `lease_source.session_directory()` 交给 sessions 域）——**不是** WR-D4 交付的 `ServerSessionDirectory`（`crates/datazen-server/src/execution/session_directory.rs`：`DEFAULT_IDLE_TTL_SECS = 3600` + `sweep_expired` 全套，`:53-55,236-272`；检索确认它只出现在自己的钉子测试与 g1/g1_w1 集成测试里，产线装配未用）。共享目录自带 TTL 机器（`DeadlineKind::{SessionIdle,TransactionIdle,DisconnectGrace}`，`runtime/directory/ttl.rs`、arm 入口 `runtime/directory/lifecycle.rs:281-308`），但产线**无人 arm、无人 sweep**——目录模块头明文「把清扫做成周期性任务……是**调用方的义务**，而不是本模块可以替你兜底的实现细节」（`runtime/directory/mod.rs:28-33`）。本刀与此的关系见 §4.3。

---

## 2. 目标与非目标

### 2.1 目标

| # | 目标 | 判据 |
| --- | --- | --- |
| G1 | server 的 runtime 会话有 TTL 驱动器：空闲到期的会话被回收，物理连接/预算额度随之释放 | 建会话 → 过期 → registry 摘行、应用层四账（sessions/directory/sink/live）清干净、backend 关物理、BudgetPortBridge 归还许可 |
| G2 | 内核唯一：策略+调度复用共享层 `orchestration/idle.rs`，两形态零分叉实现 | server 适配器只写「驱动循环 + 策略注入」，不复制任何到期判定/清账逻辑 |
| G3 | 桌面行为零变化 | `IdlePolicy::default()` 30 分钟不动（`idle.rs:36,63-71`）；adapter 策略覆盖/起停/退出停车一行不改 |
| G4 | TTL/策略参数分形态配置 | server 走 `ServerConfig` 环境变量（deploy 维度）；桌面维持代码常量（§9.2 产品语义）——两个口径不混用 |
| G5 | 回收留痕（观测性） | 每轮回收的驱逐数/interrupted borrows 进 `tracing`；装配状态进启动日志（桌面 `adapter.rs:709-717` 的 server 镜像） |
| G6 | 吊销安全：回收器跟随「当前实例」而不是绑死某一个 | `revoke()` 后回收器扫描的是新 resolve 出的实例，旧实例不被回收器的 Arc 钉在空中 |

### 2.2 非目标（明确不做与理由）

| 项 | 理由 |
| --- | --- |
| 跨进程/多实例协调（分布式回收） | 单实例 v1 的自洽前提（`budget.rs:1-7`「进程重启后额度归零是单实例形态的自洽前提」、`session_directory.rs:1-6` 同口径）；多实例归 P9 |
| 持久化回收状态 | 目录四张表「进程退出一起没了」（`runtime/directory/directory.rs:17`）、额度账内存形态（`budget.rs:1-7`）——回收状态没有可持久化的宿主 |
| `revoke()` 触发点接线 | 触发点逐个裁决见 §5；基线上不存在现实触发条件（lease.rs:24-26 + 刀0 §1.4 第 1 问） |
| 共享目录 `sweep_expired` 的周期驱动 | 目录期限在产线从未被 arm（§1.4）——扫了是空转；且 per-session 回收走的是 registry 侧 `evict_idle_at`，不是目录 sweep。登记开放问题 Q4 |
| 回收策略热更新（改 TTL 不重启生效） | 策略是装配期事实（桌面同款：`with_idle_policy` 只在装配路径调用）；env 配置天然随进程生灭。热更=新增一条无消费者的配置通道 |
| attach/detach 与摘挂载宽限的语义变更 | `detach_session` 已在共享用例内调 `arm_detached_grace`（`operations.rs:497`），server 的 detach 路由已接线（`http/domains/sessions.rs:79,278-303`）——宽限在 server 上**已经在生效**，本刀不碰语义，只继承 |
| 多组织分账 / 用户级额度 | P9（`budget.rs:25-30` 已登记） |
| web 前端任何改动 | 纯后端武装；客户端对「会话被回收」的感知就是既有 `SessionNotFound`/`SessionLost` 错误面（与显式 close 后再操作同码） |

---

## 3. 共享内核设计（落点与类型草图）

### 3.1 落点裁决：不新增、不下沉、不搬动

内核落点维持现状：`crates/datazen/src/execution/application/orchestration/idle.rs`，经 `datazen::execution::application::{IdlePolicy, IdleReclaimer, IdleReclamation}` 消费（re-export 链 `orchestration/mod.rs:17` → `application/mod.rs`）。理由：

1. **它已经是共享件**（§0 事实修正）：策略结构、到期判定、清账序列、驱动循环全部形态中立——文件里没有任何 Tauri/HTTP 词汇；「桌面」只出现在注释的动机叙述里（如 `idle.rs:44-46` 的槽位实测）。
2. **形态中立判据可过**：host-layer-inventory §1 的判据是「把宿主从 Tauri 换成 axum 还能过」。本刀给 server 写的适配器（§4.2）与桌面 adapter（§4.1）对内核的调用面完全同构——同判据直接成立，无需任何搬动来「证明」。
3. **规则单家**：到期判定的账目表在 `evict_idle_at`（`runtime/registry/registry/evict.rs:1-100`，五种答复逐格记账）、业务活动暂停在 `idle_activity.rs`、清账顺序在 `reclaim_idle`——三处都已各有其家，本刀一块都不复制。

### 3.2 内核类型草图（现状实录，非新增）

```rust
// crates/datazen/src/execution/application/orchestration/idle.rs（现状）
pub struct IdlePolicy {                     // :56-61
    deadline: Option<Duration>,             // None = 本部署不做空闲回收（配置事实）
    detached_grace: Option<Duration>,       // None = 摘挂载后不收紧
    scan_interval: Duration,
}
// default() = 30min / 60s / 3min（:63-71）；disabled()（:74-81）；
// with_deadline / with_detached_grace / with_scan_interval（:83-106）

pub struct IdleReclaimer {                  // :139-143
    stop: Arc<AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}
// stop()（幂等，循环边界退出）；is_finished()（测试可断言任务真退出）；
// Drop = stop + abort（兜底）

impl RuntimeConnectionUseCases {
    pub fn with_idle_policy(&self, policy: IdlePolicy) -> &Self;   // 覆盖语义 mod.rs:241-245
    pub fn idle_policy(&self) -> IdlePolicy;                       // 只读投影 mod.rs:246-249
    pub async fn reclaim_idle(&self) -> IdleReclamation;           // idle.rs:191-229
    pub fn start_idle_reclaimer(self: &Arc<Self>) -> IdleReclaimer; // idle.rs:235-253
}
```

### 3.3 时钟注入：严格复用现有模式（引用，不新造）

仓库现成的注入时钟链路，本刀逐环复用、零新增：

1. **端口**：`MonotonicClock`（`datazen::execution::runtime::gateway`，`lease.rs:187` 的 `Arc<dyn MonotonicClock>` 用法即证）。构造入口 `RuntimeConnectionUseCases::new_with_clock(..)`（`orchestration/mod.rs:155-169`）与全量装配 `new_with_parts(.., clock, ..)`（`mod.rs:179-186`）。
2. **生产实现**：`SystemRuntimeClock(pub Instant)`（`orchestration/mod.rs:90-95`，模块注释明写「pub 以便 server 形态的每调用现取装配与 new 共用同一时间语义」）。server 租约源已在慢路用它：`lease.rs:183-187`。
3. **同一把尺子纪律**：`RuntimeClock::now_ms()` 的注释（`mod.rs:96-102`）——写进 `OpenRequest.idle_deadline_ms`（`mod.rs:711`）与驱逐判定 `evict_idle_at(at_ms)`（`evict.rs:59`，时间由调用方注入、本模块不读 `Instant::now()`）必须是同一时钟、同一单位（单调纳秒截毫秒），「到期」才在测试里完全确定。
4. **测试假钟**：`datazen::execution::runtime::connection::testing::clock::FakeClock`，经适配器包成 `MonotonicClock` 注入——先例 `crates/datazen/tests/fixtures/p4_idle_activity.rs:11-34`（`setup_clock()` + `advance()`），整组到期/不到期/顺延断言（`:35-72` 等）都建立在它上面。

**server 烟测的时钟策略**：租约源的 `SystemRuntimeClock(Instant::now())` 是基点事实（每实例进程内单调，刀0 §1.5 风险 4 已钉「一进程一基点、单调语义不变」），本刀**不**给租约源加时钟注入口。烟测沿 `p3_live_port.rs:472-492` 的既有先例：真钟 + 短期限（deadline 50ms / scan 20ms 量级）+ 有界轮询——「CI 慢机器不误判，快机器不白等」的纪律照抄（`p3_live_port.rs:519` 注释）。需要逐毫秒确定性的到期判定钉子已由内核测试（FakeClock）覆盖，server 层只验「接线在、驱动在、跟着当前实例走」。

### 3.4 形状裁决：为什么 server 不能直接用 `start_idle_reclaimer`

`start_idle_reclaimer` 的循环体闭包持有 `Arc<RuntimeConnectionUseCases>`（`idle.rs:239-244` 的 `let service = Arc::clone(self)`）。桌面上这是正确形状——桌面 runtime 是进程级单例（`adapter.rs:498-504` 构造一次）。server 上这是**错误形状**，两个后果都不可接受：

1. **扫了个寂寞**：实例归属在租约源 cell（§1.3）。`revoke()`（未来任何触发点接线后）置空 cell、新请求 resolve 出新实例；被绑死的回收器仍在旧实例上扫描——旧实例的 registry 里是按 `SessionLost` 已作废的会话，扫它们没有任何价值。
2. **钉死旧实例**：cell 摘除后，消费者的 Arc 全部随请求结束释放，旧实例本可整体析构；回收器的 Arc 把它钉在空中永不释放——旧实例的实例私账（sessions/tokens/receipts，`mod.rs:208-229`）与 registry actor 全部滞留，恰是本刀要治的「只建不收」在实例粒度的重演。

**裁决**：server 薄适配器自建驱动循环（宿主侧 ~40 行），每 tick 经 `RuntimeLeaseSource::resolve()` 现取当前实例再调 `runtime.reclaim_idle()`。这同时满足：「每调用现取，禁止缓存」禁令约束消费者（`lease.rs:7-12`）的纪律延伸到回收器；tick 内 Arc 只在栈上存活，吊销后下一 tick 自动跟随新实例（G6）。

**为什么不在共享层做「租约感知驱动器」**：`RuntimeLeaseSource` 端口是 server crate 的件（`lease.rs:50-53`）。共享层要驱动它就得反向定义一个最小端口（如 `IdleScanTarget::current()`）并让 server 实现一个跳板——为省宿主侧 40 行，在共享层多出一个只有一个消费者的端口，违反本仓「端口必须有真实第二消费者才上提」的保守惯性。**登记**：若第三形态（如独立 worker 进程）出现，再评估上提（开放问题 Q5）。

---

## 4. 两个形态适配器

### 4.1 桌面：现状即目标形态，零代码改动

桌面 adapter 已经是判据下的薄适配器（§1.2 五个锚点）。本刀对 `datazen-desktop` **零改动**；它的回归面由既有测试钉住：

- 默认策略 30 分钟：`idle.rs:290-298`（`default_policy_is_wired_with_a_thirty_minute_deadline`）。
- 驱动器生命周期（真的回收、真的能停、不泄漏 task）：`crates/datazen/tests/p3_live_port.rs:472-492,502-519`。
- 到期/顺延语义：`p4_idle_activity.rs` 全组（FakeClock 注入）。

唯一随本刀变化的是注释层面的认知：`idle.rs:1-6` 模块头写的是「建好就废→接线即生效」的桌面叙事，本刀落地后可在该模块头补一行「server 形态的接线见 `w8-idle-reclaimer-arming-design.md` / `datazen-server::execution::idle_reclaimer`」——属文档同步，不改行为。

### 4.2 server：三个改动点 + 一个装配点

#### 改动点 1：lease.rs 策略随实例装配（镜像 W8 刀2 的 `with_live_source` 模式）

```rust
// crates/datazen-server/src/execution/lease.rs（新增字段 + builder + 慢路接线）
pub struct ServerRuntimeLeaseSource {
    // …既有字段不动（lease.rs:71-87）…
    /// 空闲回收策略（装配期注入；缺省 = IdlePolicy::default()，与今日实例内
    /// 缺省逐字节一致——本字段本身是零行为变化）。
    idle_policy: datazen::execution::application::IdlePolicy,
}

pub fn with_idle_policy(mut self, policy: IdlePolicy) -> Self { self.idle_policy = policy; self }

// resolve() 慢路：new_with_parts 之后、存 cell 之前（lease.rs:179-196 的
// with_live_connections 旁）：
if let Some(live) = &self.live_source { runtime.with_live_connections(live.clone()); }
runtime.with_idle_policy(self.idle_policy);   // ← 新增一行；覆盖语义，慢路重入无副作用
```

理由：策略是**实例**字段（`mod.rs:72` 的 `Mutex<IdlePolicy>`），而实例出生在慢路——宿主能摸到新生实例的唯一位置就是 `resolve()` 自己（`lease.rs:33-39` 对活连接端口的同款论证原文）。装配进 builder 后，吊销→重建的下一个实例**天然带策略**，与「外部 resolve 一次再接线会在重建窗口后退化」的反模式相对。缺省 `IdlePolicy::default()` 保证既有构造器与全部既有测试零改动、行为与基线逐字节一致。

#### 改动点 2：新件 `crates/datazen-server/src/execution/idle_reclaimer.rs`

```rust
//! server 形态的空闲回收驱动器：每 tick 经租约源现取**当前**实例再回收。
//! 语义参照共享内核 idle.rs 的桌面驱动器（start_idle_reclaimer），差别只有一处：
//! 不绑死实例（§3.4 的两个后果）。

pub struct ServerIdleReclaimer { /* stop: Arc<AtomicBool> + JoinHandle<()>，句柄形状镜像 IdleReclaimer：stop() 幂等 + is_finished() + Drop 兜底 */ }

impl ServerIdleReclaimer {
    /// interval 来自注入的 IdlePolicy::scan_interval()（装配期常量，循环内不重读）。
    pub fn spawn(lease: Arc<ServerRuntimeLeaseSource>, policy: IdlePolicy) -> Self {
        // loop {
        //   sleep(policy.scan_interval()).await;   // 先睡后扫：启动即扫一轮的窗口收益为负
        //   if stop { break; }
        //   let Ok(runtime) = lease.resolve().await else { warn; continue; };  // resolve 失败不致死循环
        //   let reclaimed = runtime.reclaim_idle().await;
        //   if !reclaimed.reclaimed.is_empty() { info!(count, "空闲回收：驱逐到期会话"); }
        //   for id in &reclaimed.interrupted_borrows { warn!(db_session_id = id.as_str(), ...); }
        // }
    }
    pub fn stop(&self);        // 幂等
    pub fn is_finished(&self) -> bool;
}
impl Drop { /* stop + abort，与共享 IdleReclaimer 同款兜底 */ }
```

要点逐条：

- **不加任何 server 特有闸**：回收前必须检查的东西内核全都有（§4.4 逐项列）。适配器加闸 = 复制规则，违反 G2。
- **resolve 失败不致死循环**：`resolve()` 当前实现不会失败（返回 `Result<_, String>` 是端口形状），但循环按端口契约处理 `Err`——warn + 下一 tick 重试，不 panic 不退出（server 共享 crate 的 panic 纪律：`scripts/check-shared-crate-panic-policy.mjs` 守的是共享层；宿主侧同样不主动 panic）。
- **日志对齐桌面口径**：`reclaimed.reclaimed` 非空打 info、`interrupted_borrows` 逐个 warn——与共享驱动器 `idle.rs:247-249` 与 `idle.rs:218-225` 同词汇，看板不需要两套词表。
- **为什么句柄类型不自建新的共享抽象**：`IdleReclaimer` 的字段私有、无公开构造器（`idle.rs:139-172`）；为复用一个 20 行句柄去给内核开构造器口，收益不抵 API 面扩大。宿主自建句柄（形状镜像）。

#### 改动点 3：config.rs 两个环境变量键（§6 详）

`DATAZEN_SERVER_RUNTIME_IDLE_TTL_SECS`（默认 1800）、`DATAZEN_SERVER_RUNTIME_IDLE_SCAN_SECS`（默认 60）。`TTL` 未设或设 0 → `IdlePolicy::disabled()`。命名带 `RUNTIME`：与登录会话的 `DATAZEN_SERVER_SESSION_IDLE_SECS`（`config.rs:98,126`，auth 域）显式区分——本仓「session」一词已有两个所指（登录会话 / runtime db 会话），配置键上不制造第三个歧义。

#### 装配点：bootstrap.rs

```rust
// crates/datazen-server/src/bootstrap.rs（serve 流程内，租约源构造之后）
let lease_source = …new_with_shared_parts(..).with_live_source(live_source)
    .with_idle_policy(idle_policy_from_config(&config));        // ← 改动点 1 接线

let idle_reclaimer = crate::execution::idle_reclaimer::ServerIdleReclaimer::spawn(
    lease_source.clone(), idle_policy_from_config(&config));    // ← 驱动器起跑
…
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await
    .map_err(…)?;
idle_reclaimer.stop();   // ← serve 返回（drain 完成）后停车；Drop 为兜底
```

- 时点对齐桌面纪律（services 后启动、无反序窗口）：回收器在租约源与全部消费者装配完之后、`axum::serve` 之前起跑。第一个请求到达时驱动器必然已在——server 没有「装配期窗口」，因为装配是同步顺序代码。
- 停车对齐 `shutdown_signal()` 语义（`bootstrap.rs:748-766`）：`with_graceful_shutdown` 让在飞请求排空后 serve 返回，此刻停车与桌面「退出时不一边关物理一边全表扫」（`run.rs:649-653`）同一条理由。既有 `ttl_worker` 的「停机随 drain 切片」登记（`bootstrap.rs:262-263`）不含本件——本件的停车走 serve 返回后的显式 `stop()`。
- 装配可观测：启动日志补一行 `runtime wiring: idle_reclaim_enabled / scan_interval / ttl_secs`（桌面 `adapter.rs:709-717` 的 server 镜像；server 反向语义——disabled 是**合法配置事实**，打 info 不打 warn；「策略开着但驱动器没起」才是不允许出现的状态，由装配顺序结构性排除）。

### 4.3 「会话目录清理」写实（与任务案名的对账）

任务案名「回收动作 = 租约 `revoke()` + 会话目录清理」需要在代码现实上对账成两件不同的事：

1. **per-session TTL 回收不经过 `revoke()`**。`revoke()` 是**实例级**开关（整个 cell 置空、全部会话一起作废）；TTL 回收是**会话级**的（只摘到期的那几个）。两者的唯一交点是 §5 的触发点裁决表——实例级吊销是独立的运维/演化动作，不挂在 TTL 扫描上。目录清理在会话级路径里**已经在内核中**：`reclaim_idle()` 对每个被驱逐会话调 `directory.release(&entry.handle, CloseDisposition::Closed)`（`idle.rs:209-216`），对实际接线的 `InMemorySessionDirectory` 生效（幂等、错纪元句柄不释放新纪元条目——该端口实现的既有纪律）。
2. **`ServerSessionDirectory`（WR-D4 v1 件）不在本刀路径上**。产线接线的是共享层 `InMemorySessionDirectory`（§1.4）；`ServerSessionDirectory` 的 `sweep_expired` + 3600s TTL 是一个**未接线**的平行实现。本刀不去驱动它（它不在装配图里），也不去驱动共享目录的 `sweep_expired`（期限从未被 arm，`runtime/directory/mod.rs:28-33` 的「调用方义务」在产线处于未履约状态，但履约它需要先回答「谁 arm `arm_session_idle`」——与 per-session 回收正交，登记 Q4）。

### 4.4 回收前的安全检查（全部已在内核，逐项对账）

| 检查 | 落点 | server 语义 |
| --- | --- | --- |
| 进行中执行不误伤 | dispatch 持计数 hold、暂停 idle deadline（`idle.rs:19-23` 模块头 + `idle_activity.rs:41-69` `begin_idle_activity`）；actor 侧扫描跳过在飞（`evict.rs` 的账目表 + `p4_idle_activity.rs:56-72` 行为钉） | 会话内执行（MCP `execute_in_session`、result-cell 写）走同一条 hold；tableData target 面（无会话）不受回收影响 |
| 执行完成后重新起算 | 最后一条真实业务操作完成后刷新期限（`idle_activity.rs:70-101` `finish_idle_activity`；只读/幂等重放/订阅/attach 不记活动，`idle.rs:21-23`） | 长轮询（target_runner 25ms 收集）在 hold 窗口内；「完成后 +30 分钟」从真实活动点起算 |
| 替换屏障不穿透 | `evict_idle_at` 对 `CloseRejected` 什么都没发生（摘表还额度都禁止，`evict.rs:76-77` 账目表） | 会话替换编排不受扫描干扰 |
| 活句柄残留不硬关 | backend `close` 对 `registered_handles != 0` 拒绝（`driver_exec.rs:723-728`「driver 说 Clean 不是放行条件」）、未排空/在飞 cancel 如实 `Undecidable` 并摘出向绑定（`driver_exec.rs:744-751`） | W8 刀2 的活句柄借出（`live_source`）借出的引用在回收时先被 `live.revoke` 作废（`idle.rs:200-204`，顺序注释「先作废借用，再摘应用层登记」）——宿主拿旧引用打请求得到诚实失败 |
| 送达不明的保守处置 | 「其余 Err」保留表项与额度、等显式处置（`evict.rs:88-96`） | 不凭空报「已回收」 |
| 物理关闭与预算归还 | registry actor 释放例程 → `backend.close`（真关，v1 每次新开物理连接、close 即真关，`driver_exec.rs:16` 头表）→ `ServerDriverExecBackend` 摘 `by_session` 绑定（`driver_exec.rs:771-780`）；预算许可按 `permit_id` 由 `BudgetPortBridge` 找回逐一归还（`budget_bridge.rs:5-8`） | 回收释放的额度与显式 close 同一条路径；节点/组织额度闸（`config.rs:146-147` 默认 64/32）因此被「死会话占坑」拖死的现症状随本刀缓解 |

**server 适配器零额外闸**是本设计的一个明确立场：桌面两年跑通的那条「hold → 到期 → 五格账目表 → 四账清账 → 物理关闭 → 预算归还」链路，server 只换驱动循环的宿主，不换规则。

---

## 5. revoke 触发点决策表（刀0 开放问题的逐项裁决）

刀0 留下的开放问题（`w8-mutation-knife0-1-design.md` §1.4 第 1 问、`web-ready-plan.md:181` 末句）：`revoke()` 的现实触发点（驱动注册表热替换 / 后端重建 / 手动运维等）。逐个裁决：

| # | 候选触发点 | 裁决 | 理由 |
| --- | --- | --- | --- |
| T1 | 驱动注册表热替换 | **保留（未定义即不触发）** | 基线上驱动目录是构建期 inventory 静态入表（`bootstrap.rs:336-343`：`DriverRegistry::new()` 懒形 + `driver-*` feature 选择性链接），没有热替换 API，也没有它的需求单。若未来出现：吊销前必须先跑一轮旧实例 `reclaim_idle()`（drain-then-revoke）或接受旧实例物理资源随析构兜底——这两个语义要当场裁决，不能预支 |
| T2 | 后端重建（`SessionBackend` 换实例） | **保留（未定义即不触发）** | backend 是长生命周期 Arc（`bootstrap.rs:344-351` 构造一次），配置漂移由每请求 CAS 拒绝（profile revision 闸 `resource_host.rs:210-214`）；「重建」在 v1 没有现实路径 |
| T3 | 配置漂移（TTL/策略/白名单变更） | **不触发** | 漂移拒绝已有更细粒度的闸（每请求 CAS）；实例整体作废是核武器级响应用错地方。TTL 变更随进程重启生效（§2.2 非目标） |
| T4 | 手动运维吊销（admin 口） | **保留（API 已暴露，等第一个消费者）** | `revoke()` 已是 `pub`（`lease.rs:148`），v1 不为它开 HTTP/admin 面——没有消费者的运维口是攻击面不是能力。接线时须同时答两件事：① 谁授权（policy 域动作）；② 旧实例物理清账语义（同 T1 的 drain-then-revoke 问题） |
| T5 | 回收器发现实例不可救药（如 registry 毒化） | **不设立** | 扫描对每种非成功答复保守记账（`evict.rs` 五格表），不存在「实例级不可救药」的判定点；造一个就多一个误触发面 |
| T6 | 未来多实例/worker 拓扑（P9）的实例退役 | **保留（随 P9 设计）** | 分布式吊销涉及 worker 失联/节点租约（`budget.rs:18-19` 的 `renew_node_lease` 失联语义已是它的雏形），不在单实例 v1 的语义域内 |

**共同纪律**：本刀回收器的形状（每 tick 现取当前实例）已经把「吊销后跟随新实例」做成了结构性事实——任何未来触发点接线时，回收器**零改动**。这是 §3.4 形状裁决在时间维度上的红利：触发点裁决可以继续悬置，而不会变成后续刀的阻塞债。

---

## 6. TTL 配置面

### 6.1 server：`ServerConfig` 环境变量（唯一配置入口）

配置纪律沿用 `operations/ttl.rs:8-10` 的登记原文：「`config.rs` 是唯一配置入口，本模块不读环境变量。新增键与默认值在报告登记」。

| 环境变量 | 类型 | 默认 | 语义 | 校验 |
| --- | --- | --- | --- | --- |
| `DATAZEN_SERVER_RUNTIME_IDLE_TTL_SECS` | u64 | `1800` | 业务空闲期限（秒）。**未设 = 1800；显式 `0` = `IdlePolicy::disabled()`** | `parse_seconds`（`config.rs:427-442`）的 ≥60s 下限适用于显式非零值；`0` 是特判在先的合法值 |
| `DATAZEN_SERVER_RUNTIME_IDLE_SCAN_SECS` | u64 | `60` | 扫描间隔（秒） | ≥60s；下限理由见 §7 R4 |

装配映射：`(0, _) → IdlePolicy::disabled()`；`(_, s) → IdlePolicy::with_deadline(Duration::from_secs(ttl), Duration::from_secs(scan))`。detached grace 保持 `default()` 的 60 秒（`with_deadline` 的既有行为，`idle.rs:83-90`）——它由 detach 路由实际触发（§2.2 非目标第 6 行），不单独开配置键。

默认值论证：**30 分钟 = §9.2 同族**（`idle.rs:36` 的常量本源），与内核 `IdlePolicy::default()` 数值一致——部署不设键时，server 与桌面的期限语义一致，行为可预期；多用户场景若嫌激进，部署调大即可（这正是 G4 分开配置的意义）。**60 秒扫描 = 期限的 1/30**：最坏 overshoot 1 分钟（对比桌面 15s/30min 的 1/120；server 不需要桌面那么勤，因为 server 的动机是额度卫生不是 UI 槽位竞争，`idle.rs:42-46` 的桌面实测痛点在 server 侧以额度闸形式存在）。数值都是「首版配置起点而非协议常量」（`session_directory.rs:29-30` 同款口径），最终值见开放问题 Q1/Q2。

### 6.2 桌面：无配置面（保持）

桌面 TTL 的来源是 §9.2 产品语义（`idle.rs:36-38` 常量）+ 装配期扫描间隔覆盖（`adapter.rs:511-514`）。本刀不开桌面设置项：`ExecutionSettings`（`settings/domain.rs:122-134`）目前没有 idle 字段，加字段=序列化迁移+UI+双端同步，是另一个特性不是本刀的护栏。桌面行为零变化（G3）的完整性依赖这一点：**配置面不动，行为就没有可以漂移的通道**。

---

## 7. 风险与护栏

| # | 风险 | 评估 | 护栏 |
| --- | --- | --- | --- |
| R1 | **SessionLost 扇出**：会话被回收后，客户端持旧 handle 的后续请求全部 `SessionNotFound`（等价「从未存在」口径，lease.rs:22-24 同源语义） | 中。这是回收的**定义**而不是故障，但扇出体验要可预期 | ① 与显式 close 后再操作同码——客户端已有处理路径（关闭幂等回执重放 `operations.rs:426-447`）；② 回收留痕（info 日志 + `db_session_id` 字段）让「我的会话怎么没了」可事后归因；③ TTL 默认 30 分钟 + 每次真实业务活动刷新（`idle_activity.rs:75-77`），活跃用户实际撞不到 |
| R2 | **长查询被误回收** | 低。内核已有三重保护：dispatch hold 暂停期限（`idle_activity.rs:41-69`）、actor 在飞时扫描得 `Ok(None)`（`evict.rs:32` 账目表 + `p4_idle_activity.rs:56-72` 钉子）、完成到收尾窗口由 hold 覆盖（`idle.rs:22`） | 残留缺口如实登记：「事务开着但无在飞 dispatch」的会话在期限后会被回收（下一语句 SessionLost）——这是桌面既有语义（同口径同值），不是本刀新增；若 server 需要事务期保护，走 `TransactionIdle` 目录期限（`lifecycle.rs:281-288`，产线未接线）另立刀 |
| R3 | **多租户回收风暴**：一轮扫描驱逐大量会话，物理关闭/预算归还在同一 tick 内爆发 | 低-中。上界被三层限死：节点/组织额度闸（默认 64/32，`config.rs:146-147`）决定在飞物理连接总量——风暴规模 ≤ 当时死会话数 ≤ 64；`evict_idle_at` 逐 actor 串行（`evict.rs:63-97`），单会话关闭失败不阻塞不连锁；扫描间隔 ≥60s，风暴频率有下界 | 扫描间隔配置下限 60s（§6.1 校验）；观测先行（§G5）——真出现风暴先看日志分布再调参 |
| R4 | **扫描过勤反噬**：多用户下全表扫描本身是 O(会话数) 的 actor 往返 | 低 | 间隔下限 60s；内核扫描对未到期会话是 O(1) 答复（`Ok(None)` 无物理动作） |
| R5 | **回收器绑死旧实例**（§3.4 的两个后果在实现层复发） | 设计期已消除 | 每 tick `resolve()` 现取（G6）；t3 烟测（§8）把「revoke 后回收器跟随新实例」变成红灯可测 |
| R6 | **观测缺失**：回收静默发生，运维无法回答「为什么额度突然释放/会话为什么没了」 | 低 | G5：每轮 info（count）、interrupted borrows warn（与内核 `idle.rs:218-225,247-249` 同词汇）、装配日志（enabled/interval/ttl）。审计事件（对齐 TtlSweeper 的 `artifact_ttl_swept` 先例，`ttl.rs:94-104`）作为可选项登记 Q3 |
| R7 | **与登录会话 TTL 的概念混淆**：两个「session idle」并存（auth 域 8h / runtime 域 30min） | 低 | 配置键 `RUNTIME_` 前缀显式区分（§6.1）；两者语义独立：登录会话过期断的是身份，runtime 会话回收断的是物理额度——断开登录会话**不**触发 runtime 回收（回收只看业务空闲期限），反之亦然 |

---

## 8. 测试计划

### 8.1 内核裁决钉子（既有，零改动，回归守护）

| 钉子 | 位置 | 守什么 |
| --- | --- | --- |
| 默认策略 30 分钟 | `idle.rs:290-298` | G3 的期限面：`IdlePolicy::default()` 数值不动 |
| 策略构造器互不干扰 | `idle.rs:300-356`（五条） | TTL 配置映射（§6.1）依赖的构造器行为 |
| 到期/刷新/顺延全组（注入时钟） | `p4_idle_activity.rs`（FakeClock + `new_with_clock`，`:11-34` 装配） | 到期判定在同一把注入尺子上完全确定（§3.3） |
| 驱动器真回收 + 真能停 | `p3_live_port.rs:472-492,502-519` | `is_finished()` 可测纪律（server 句柄镜像它） |

### 8.2 server 接线烟测（新增 `crates/datazen-server/tests/idle_reclaimer_wiring.rs`）

夹具沿 `cm63_lease_continuity.rs`（刀0 建立的真租约源装配模式）+ MockDriver/真 SQLite 管理库；真钟 + 短期限（先例 `p3_live_port.rs:472-492`）：

- **t1 到期回收全链**：建会话（deadline 50ms / scan 20ms）→ 有界轮询 `is_registered == false` → 断言 `tracked_session_rows() == 0`、`is_attached == false`、backend closed 计数 +1、预算快照归还（`ServerBudgetCoordinator::snapshot` 的 in_use 回落）。
- **t2 在飞不误伤**：MockDriver 阻塞的执行挂起 → 推过期限 → 未回收（closed == 0）→ 放行至终态 → 期限后再过一轮 → 回收（内核钉子的 server 级复证）。
- **t3 吊销后跟随新实例**（R5 红灯）：建会话 A → `revoke()` → resolve 新实例 → 建会话 B → 过期限 → **A 不可达（SessionNotFound）且 B 被回收**——回收器没有停在旧实例上。
- **t4 停车**：`ServerIdleReclaimer::stop()` → 有界轮询 `is_finished() == true`（不泄漏 task 的 server 版钉子）。
- **t5 disabled 配置面**：`TTL=0` → 驱动器不 spawn（或 spawn 后零驱逐 + 装配日志 enabled=false 二选一，施工时按 §4.2 装配日志的口径定案并钉住）。
- **t6 既有探针不改一字仍绿**：`table_data_routes.rs:757-787` fail-closed 探针、`cm63_lease_continuity.rs` 全组、`driver_exec_lease.rs` 的 resolve 语义钉（刀0 翻新版）——lease.rs 加字段+builder 不改任何既有签名，这一组是「零行为变化」的回归证明。

### 8.3 桌面行为回归（G3）

- 零改动 crate 的回归由 8.1 全组 + 桌面既有单测（`idle.rs` 内嵌 `#[cfg(test)]`）覆盖；不新增桌面测试。
- 手工验收（一次性）：桌面启动 → `log_runtime_wiring` 输出 `idle_reclaim_enabled=true` → 退出无 panic（既有路径，`run.rs:640-656` 未动）。

---

## 9. 施工切分

**建议一把刀**（分支 `w8-idle-reclaimer-arming`，单 PR）：

| 量 | 件 | 验收标准 |
| --- | --- | --- |
| S | `lease.rs`：`idle_policy` 字段 + `with_idle_policy` builder + 慢路一行接线 | 既有测试零改动全绿（t6 组）；新实例 `idle_policy()` 等于注入值 |
| S | `config.rs`：两键 + 解析 + `idle_policy_from_config` | `parse_seconds` 复用；`0 → disabled` 特判有钉 |
| M | `execution/idle_reclaimer.rs`（新件）+ `execution/mod.rs` 一行声明 | t1-t5 全绿；句柄形状与 `IdleReclaimer` 同纪律（stop 幂等 / is_finished / Drop 兜底） |
| S | `bootstrap.rs`：`with_idle_policy` 接线 + spawn + serve 返回后 stop + 装配日志 | 启动日志含 enabled/interval/ttl 三事实；SIGTERM 后日志见停车 |
| S | 文档：`web-ready-plan.md` §11 顶部条目（w8-handoff §3 纪律 5）+ `idle.rs` 模块头一行指向 | 台账闭合（刀0 遗留登记销账） |

**什么时候拆两把**：仅当评审推翻 §3.4 的形状裁决、要求把「租约感知驱动器」上提共享层（开放问题 Q5）——那时拆成「内核共享驱动器 + 桌面切换回归」与「server 接线」两把，前者必须带桌面全量回归。在当前裁决下拆两把只会把 S+M 切成两个互相等待的半成品。

**全量验证**：`cargo test -p datazen-server --features mcp,use-case-contracts --no-fail-fast`；桌面侧 `cargo test -p datazen-desktop --lib`（受影响面为 0，跑既有即可）；共享层零改动不触发 `cargo test -p datazen` 全量义务，但 t6 组会连带覆盖。

---

## 10. 开放问题（需要用户/产品拍板）

| # | 问题 | 本文的倾向 | 拍板影响面 |
| --- | --- | --- | --- |
| Q1 | **server 默认 TTL 值**：30 分钟（§9.2 同族，本文倾向）还是更长（如 60/120 分钟，多用户下更保守） | 30 分钟，部署可用 env 覆盖 | 只影响 `config.rs` 默认常量与文档登记表 |
| Q2 | **server 扫描间隔默认值**：60 秒（本文倾向，最坏 overshoot 1 分钟）还是沿内核 3 分钟 | 60 秒 | 同上 |
| Q3 | **回收是否落审计事件**（`runtime_session_reclaimed`，对齐 `artifact_ttl_swept` 先例） | v1 先 `tracing` 留痕；审计事件等第一个审计消费方出现再加（避免噪音行刷 `audit_events`） | 若加，`AuditPayload` 白名单四键封顶纪律下 session id 只进 tracing（`ttl.rs:16-18` 同款取舍） |
| Q4 | **共享目录 `sweep_expired` 的产线履约**：目录 TTL 机器（`mod.rs:28-33` 的调用方义务）在产线无人驱动；`ServerSessionDirectory`（3600s 形态）未接线 | 本刀不碰（与 per-session 回收正交，§4.3）；单独立刀回答「谁 arm / 谁 sweep / `ServerSessionDirectory` 归位还是退役」 | 独立排刀 |
| Q5 | **「租约感知驱动器」是否上提共享层** | 不上提（§3.4）；第三形态出现再议 | 若上提，施工切分改两把（§9） |
| Q6 | 事务中途空闲的会话是否需要 `TransactionIdle` 保护（R2 残留缺口） | 维持桌面既有语义（回收 = SessionLost），不为本刀扩语义 | 若要保护，走目录期限接线另立刀 |
