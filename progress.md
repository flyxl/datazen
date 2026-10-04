# p3-cm32-tunnel-refcount

分支 `feature/p3-cm32-tunnel-refcount` · 基线 `21e27e4ebc6c05c21c69b14e30fe3248891c2c5b`

目标：CM-32 **隧道引用与并行任务（H）** —— 前置：两个 session/Job 共用同版本隧道；步骤：关闭第一个，继续第二个；第二个结束。断言：第一步不关闭隧道；最后一个引用释放才关闭；隧道失败传播给依赖资源。
（`docs/architecture/platform/connection-management.md:1057-1061`）

## 状态

- [x] 强制前置：CodeGraph 探索 `packages/runtime` 最近概念
- [x] 映射与设计（本文件下一节）
- [x] 协调者裁定（三条全部批准，实现授权）
- [x] 实现
- [x] 门禁与 `READY_FOR_TEST`

## 裁定落地结果

裁定三条全部照办，与上文设计的差异只有一处（下文「设计变更」已注明）：

| 裁定项 | 结论 | 代码事实 |
| --- | --- | --- |
| ① 方向 | 新建 `packages/runtime/src/tunnel/`；宿主 `TunnelKind`/`Tunnel` 枚举不进 runtime；共享身份 = `TunnelSpec` 全等值；单轨 | `tunnel/` 9 个文件；台账只按 `TunnelSpec` 索引，不读 `PoolKeyGeneration`/`CacheRevision` |
| ② 不碰 `src-tauri` | 桌面 `NetworkProvider` 实现是**后续独立轨**；CM-32 是 H 层，P3 出门条件不依赖它 | `src-tauri` 0 处改动；可实现性改由 `tests/tunnel_refcount_contract.rs` 在**进程外**证明 |
| ③ CM-27 / CM-28 | **部分覆盖**，不得标记为已覆盖；剩余项列成可直接领取的清单（见下） | 见「CM-27 / CM-28 剩余项」一节 |

### 设计变更（一处）

上文「拟增文件」表里的 `tunnel/bridge.rs` **没有建**。原因：F2（失败传播）需要的只是「把 `dependents` 交给调用方」，而 `TunnelLedger::report_failure(&spec) -> Vec<LeaseId>` 直接返回依赖清单就够，不值得为一个签名单独开一个模块。归还侧同理，`ResourceManager ↔ TunnelLedger` 的桥没有落地成代码，理由见下一节。

### ② 的硬性要求：归还接线是 (a)，不是「可选」

裁定明确要求二者择一，本轨取 **(a) 已接线**，并且用测试钉住「一次归还 = 恰好一次 release」：

- 接线点：`TunnelLedger::return_resource(&LeaseId) -> Option<TunnelRelease>`。它按**资源**找条目、从 `dependents` 里摘掉这一条、然后走**与 `release` 完全相同的 `drain` 路径**。找不到 ⇒ `None` 且零副作用。
- 台账**刻意不提供**「只摘依赖、不释放引用」的出口（设计期存在的 `detach_dependent` 已删除）——那正是让计数比依赖清单多 1 的漏斗。
- 一条资源对同一条隧道**只允许持有一份引用**，重复 acquire 拒绝为 `TunnelError::AlreadyHeld`（`tunnelAlreadyHeld`）。没有这条，一次归还只减 1，多出来的那份永远没人还。
- 对应测试：`journey_return.rs` 5 条 + `tunnel_refcount_contract.rs::one_return_releases_exactly_once_however_many_times_it_is_repeated` + `journey_single_counter.rs::single_counter_algebra_holds`。
- `ResourceManager` **刻意未改**：改 `ResourceManager::new` 或 `LeaseRecord` 会动到 382 条既有基线测试。桌面接线属于裁定 ② 的后续轨。

## 交付面（9 个新文件 + `lib.rs` 一处）

| 文件 | 行数 | 职责 |
| --- | --- | --- |
| `src/tunnel/mod.rs` | 74 | 模块头、re-export、「唯一计数铁律」「归还接线点」两节 |
| `src/tunnel/ledger.rs` | 375 | `TunnelLedger` / `TunnelState` / `TunnelLease` / `TunnelRelease` / `drain` |
| `src/tunnel/transport.rs` | 75 | `TunnelTransport` 接缝、`TunnelHandle`、`TunnelFault` |
| `src/tunnel/error.rs` | 93 | `TunnelError` + 稳定字面码 + `rejected()` 唯一拒绝出口 |
| `src/tunnel/harness.rs` | 238 | `RecordingTunnelTransport`（**刻意不持计数**）/ `TunnelHarness` |
| `src/tunnel/journey_sharing.rs` | 286 | 断言 1、2 + 共享键 + 直连 + 免隧道 + 路由版本 |
| `src/tunnel/journey_failure.rs` | 251 | 断言 3 + F1/F2/F3 |
| `src/tunnel/journey_single_counter.rs` | 274 | 唯一计数铁律的代数不变量 |
| `src/tunnel/journey_return.rs` | 191 | (a) 归还接线契约 |
| `tests/tunnel_refcount_contract.rs` | 439 | 进程外 `TunnelTransport` 实现 + 接缝形状上的三条断言 |

`packages/runtime/src/lib.rs` 唯一的既有文件改动：`pub mod resource;` 之后加了带文档注释的 `pub mod tunnel;`。

## 映射与设计

### 结论先行：契约已在冻结上游，本轨是「实现」不是「发明」

核对下来，本概念**不是**从零设计。`packages/platform-api/src/ports/network.rs`（166 行，冻结上游）已经把 CM-32 的词汇表和端口签名写好了：

- `TunnelSpec { route_ref, via_host, via_port }`，`#[derive(PartialEq, Eq)]`；
- `TunnelBinding { spec, ref_count }`，文档注释：「已建立的隧道绑定。**同一 `TunnelSpec` 共享引用计数**：计数降到 0 时隧道才真正拆除。」；
- `NetworkProvider::ensure_tunnel(ctx, spec) -> Result<TunnelBinding, PortError>`，注释：「共享引用：同一路由/隧道被多处使用时返回同一 binding 的引用计数句柄」；
- `NetworkProvider::release_tunnel(binding) -> Result<(), PortError>`，注释：「释放一次引用。引用计数归零才真正拆除；**重复释放是幂等的**」。

`packages/runtime/Cargo.toml` 已经依赖 `datazen-platform-api`（方向 runtime → platform-api，符合 platform-api F-04），且 `directory/commit.rs:23-24` 已经在用 `datazen_platform_api::ports::session_directory::{ReplacementOperation, SessionOwner}` —— 复用 platform-api 端口词汇表是本仓既成做法。

所以本轨的准确表述是：**把已经存在的 `NetworkProvider` 隧道共享引用契约，在 runtime 里落成一个可注入、可确定性测试的引用计数台账。**

「同版本隧道」的可判定形式就是 `TunnelSpec` 全等值；`network.rs:101-112` 已有一个测试锁定这一点（`tunnel_sharing_is_keyed_by_the_whole_spec_not_just_the_route`：同规格必须共享、跳板不同不得复用）。

### Q1 · runtime 隧道类型落点

**落点：新建兄弟模块 `packages/runtime/src/tunnel/`，不塞进 `resource/`。**

理由：`resource/` 的记账单位是**物理连接**，共享粒度是 `PoolKeyGeneration`；隧道是另一种物理资源，共享粒度是 `TunnelSpec`，而且它的寿命必须**比单条 Lease 长**（多 session/Job 共用一条隧道），与 `LeaseState` 状态机不是同一个生命周期。硬塞进 `resource/` 会让 `ResourceError` 变成两个层次的错误混在一起，破坏 `resource/mod.rs:89-95` 定的「单一拒绝出口 + 精确原因集」纪律。

`shared-boundaries-and-ports.md:216` 把「隧道引用」写在 `resource` 模块行 —— 本轨把它落成**集成点**而不是**归属**：`ResourceManager` 在归还路径上把隧道引用释放一次（对应 `connection-management.md` §7.5 步骤 6「释放隧道引用、核销预算只执行一次」），台账本身归 `tunnel/`。

宿主枚举 → runtime 类型的映射（这是本轨最关键的一条映射判断）：

| 宿主/上游 | 类型 | 变体数 | 在 runtime 里的角色 |
| --- | --- | --- | --- |
| `src-tauri/src/tunnel/mod.rs:25-29` `Tunnel` | 枚举 | **3 臂**（`Ssh`/`HttpProxy`/`WebSocket`） | **不映射**。它是宿主进程内的 RAII 句柄，是实现细节 |
| `driver-api/src/tunnel_types.rs` `TunnelKind` | 枚举 | **4 变体**（`None`/`Ssh`/`HttpProxy`/`WebSocket`） | **不进 runtime**。它只决定宿主 `TunnelTransport` 的哪种开法 |
| `platform-api/.../network.rs` `TunnelSpec` | 结构 | `{route_ref, via_host, via_port}` | **就是共享键**。runtime 直接复用，不镜像、不另造 |

即：`TunnelKind` 是**宿主实现细节**，`TunnelSpec` 才是**共享身份**。runtime 若去镜像 `TunnelKind` 就等于把宿主的传输形态泄漏进传输中立内核，与 runtime 既定纪律（自带 `PhysicalTransport` 而不是 import `DatabaseDriver`）相悖。

`TunnelKind::None`（直连）在 runtime 侧的落法：不进台账 —— `Ledger::acquire(spec: Option<TunnelSpec>)`，`None` 表示不涉及隧道，调用方直接拿物理连接。这一条要在 API 上说清楚，否则 `None` 会被误当成「一条空的隧道」而白占一个 entry。

拟增文件（单文件均 < 800 行上限）：

| 文件 | 职责 | 对位既有物 |
| --- | --- | --- |
| `tunnel/mod.rs` | 模块头、re-export、cfg 门控 | `resource/mod.rs` |
| `tunnel/ledger.rs` | `TunnelLedger` 引用计数台账（核心） | `resource/table.rs` |
| `tunnel/transport.rs` | `TunnelTransport` 物理接缝 | `resource/cleanup.rs` 的 `PhysicalTransport` |
| `tunnel/bridge.rs` | `ResourceManager ↔ TunnelLedger` 的失败传播桥 | `resource/rotation.rs` |
| `tunnel/harness.rs` | `RecordingTunnelTransport` / `TunnelHarness` | `resource/harness.rs` |

测试：`packages/runtime/tests/tunnel_refcount.rs`（集成，三条断言）+ 各文件 `#[cfg(test)]` 单测。

台账条目形状：`entry { spec: TunnelSpec, state, refs: u32, dependents: IndexSet<LeaseId> }`。

### Q2 · 「同版本」是什么，与 `PoolKey` / CM-67 的关系

**CM-32 的「版本」= `TunnelSpec` 全等值**（`route_ref` × `via_host` × `via_port`）。它**不是** `PoolKeyGeneration`，**也不是** CM-67 的 cache 代次。三者的区别逐条：

| 概念 | 代数 | 回答什么问题 | 判据 | 与隧道的关系 |
| --- | --- | --- | --- | --- |
| `PoolKeyGeneration`（`lease.rs:58-85`） | 指纹(database+policy) × `credential_revision` × `network_route_revision` | **哪条物理连接**可以被谁复用 | `ResourceManager::current_pool_key`（`manager.rs:146-169`） | **没有隧道字段** |
| `CacheRevision`（`generation.rs`） | `PoolKeyGeneration` × generation × `config_revision` | **慢结果**能不能回填 | `admit_cache_fill`（`manager.rs:129-135`） | 无关（缓存代次，§9.6 明说「它不是 session 代次」） |
| **`TunnelSpec`**（platform-api） | route_ref × via_host × via_port | **哪条隧道**被多少持有者共用 | **本轨新增的 `TunnelLedger`** | **就是它** |

与 `network_route_revision` 的关系要说准（`connection-management.md:676`：`networkRouteRevision` = 「NetworkProvider 的路由/隧道/TLS 配置版本」）：

- `network_route_revision` 是**路由被换掉**的**粗粒度代号**；`TunnelSpec` 是**隧道当前这一份材料**的**细粒度身份**。
- 约束方向一（安全性已有保障）：`network_route_revision` 轮换后，`current_pool_key` 已经用 `PoolKeyRotated` 拒绝旧代申请（`manager.rs:157-165`），所以**跨代隧道不会被共享**。本轨不重复实现这道闸门。
- 约束方向二（必须由本轨实现）：**同一 `networkRouteRevision` 内，两条 `TunnelSpec` 不同的连接仍各自开隧道**。这是 `network.rs:109-112` 已锁定的行为，目前无人实现。

⇒ 结论：**共享键比 `PoolKeyGeneration` 更细**。`TunnelLedger` 只按 `TunnelSpec` 索引，**不读** `PoolKeyGeneration`；跨代安全性由既有 CM-67 代次闸门提供。两者不重复、不冲突、不互相派生。

### Q3 · 引用计数归属，与宿主的关系

**归属**：runtime 的 `TunnelLedger`。一份 entry = 一个 `TunnelSpec` 的一个权威计数。

⚠️ 一条必须写进注释的陷阱：`TunnelBinding.ref_count` 的契约原文是「**仅供观测**，不用于判断能否释放（释放以 `release_tunnel` 的调用为准）」。所以 ledger **内部必须另持一个权威计数**，绝不能拿 binding 快照做判断 —— 否则重复释放必然多减引用（CM-28 的失败模式）。这条约束要同时钉在字段注释和一条单测里。

**本轨不碰 `src-tauri/`。** 三条理由：

1. 三条断言全部是**台账算术**。「最后一个引用释放才关闭」只有在 `transport.close` 的**调用计数**上可观测 —— 这正是 `PhysicalTransport` + `RecordingTransport`（`resource/harness.rs:105-216`）今天的做法。
2. 分工已由文档写死：`shared-boundaries-and-ports.md:590` ——「隧道生命周期通过 `NetworkProvider::ensure_tunnel` 的桌面实现承接」。桌面实现属于**接缝期**的工作，不是本轨。
3. 本仓先例成立：`ResourceManager` 在 `src-tauri` **0 处**接线（`grep -rn ResourceManager` 只命中 runtime 自身 + `platform-api/src/target.rs` 一条注释）；`ExecutionGateway::new` 在 `src-tauri` **0 处**、只在 runtime 夹具里 5 处。「runtime 模块零宿主接线即可验收」是已确立的模式。

**无宿主接线时的验证方式**：集成测试用 `TunnelHarness` + `RecordingTunnelTransport` 的 journal，断言 open/close 的**次数与顺序**；顺序判定用 `connection::testing::clock::FakeClock`，**不用 sleep**。

### Q4 · 「隧道失败传播给依赖资源」怎么建模

建模为**三个可注入的失败点**，都不需要真网络：

| 编号 | 注入点 | 语义 | 断言 |
| --- | --- | --- | --- |
| **F1** | `TunnelTransport::open` 返回 `Err(TunnelFault::Open)` | 开隧道失败 ⇒ **不落账**（refs 不增、entry 不建），依赖它的资源获取**整条失败**，而不是拿到指向死端口的 lease | entry 数不变；无 binding 产出；资源表行数不变 |
| **F2** | `TunnelLedger::report_failure(binding)`（隧道中途死亡 / 上游断） | entry 转 `Failed`，把 `dependents`（`IndexSet<LeaseId>`）交给 `ResourceManager` 走**隔离** | 依赖 lease 全部不再 `accepts_execution`；沿用 `CleanupDisposition::Quarantined` 既有语义（保留物理预算、不再发放）；**隧道 refs 不被误减**（引用方尚未释放） |
| **F3** | `TunnelTransport::close` 返回 `Err(TunnelFault::Close)` | 拆除失败 ⇒ entry 转 `Closing`/`Unconfirmed` 而**不是**被 forget | 重复 `release` 幂等；refs **不得降到 0 以下**；close **不得调用第二次** |

F2 是「传播给依赖资源」的主断言：隧道是**共享**资源，所以它死掉时受影响的不是某一个持有者，而是**全部登记过的依赖方**。这要求 entry 必须持有 `dependents: IndexSet<LeaseId>`，否则共享场景下无法枚举受影响方。

F3 是 CM-28「隧道不多减引用」的前置条件：refs 必须是无符号且做饱和检查，不能靠 debug 断言兜。

**错误出口**沿用 `resource/` 的既有纪律：`TunnelError` 自带 `reason()` 稳定字面码（`resource/mod.rs:127-144` 的形态），并用 `as_runtime_error()` 映射到**既有** `RuntimeError` 变体，**不新增**线路可见的拒绝形状。

### Q5 · 单轨还是拆

**单轨，可独立验收。**

- 拆不开：三条断言是**同一个状态机**。断言 3（失败传播）必须先有断言 2 的账本才写得出来；拆开必然留下一半「能验收但没验」的状态。
- 规模可控：5 个源文件合计约 750–900 行 + 1 个集成测试约 250 行，单文件均在 800 行上限内。
- 独立性成立：不改 `connection/port.rs`（冻结）、不改 `src-tauri`、不改任何现有测试；对既有文件的改动只有 `lib.rs` 加一行 `pub mod tunnel;`，以及 `resource/mod.rs` 可选的一行 re-export。

### CM-27 / CM-28 的隧道子项覆盖声明

**明确不随本轨自动闭合。** 逐条：

- **CM-27「隧道引用正确」= 部分覆盖。** 本轨覆盖 F1（隧道 stage 失败 ⇒ 引用不落账/必释放）与「隧道开了但后续 stage 失败 ⇒ 回滚释放」这两处的引用正确性。CM-27 的完整断言还含 permit/socket/handshake/init/register **全阶段矩阵**，以及「可确认关闭的资源许可归零」「未确认关闭进入隔离」——**不在本轨**，仍挂在 CM-27。
- **CM-28「隧道不多减引用」= 部分覆盖。** 本轨覆盖隧道引用计数自身的重复释放幂等与不低于 0（F3）。CM-28 的主断言（20 次并发归还、driver close 至多一次有效关闭、预算不负数、重复响应一致）**不在本轨**，仍挂在 CM-28。

若协调者认为这两条应整体收口，需把 CM-27/CM-28 的非隧道部分一并纳入，本轨范围将显著扩大 —— 请在裁定中一并给出。

> **裁定结果**：维持「部分覆盖」，不扩范围。剩余项已拆成可领取清单，见文末「CM-27 / CM-28 剩余项」。

### CM-27 / CM-28 剩余项（后续轨可直接领取，不在 CM-32）

裁定 ③ 要求「不得标记为已覆盖」，并把剩余项交成可直接领取的清单：

**CM-27「隧道引用正确」剩余**（本轨已覆盖：F1 开隧道失败不落账、隧道开成后回滚释放）

1. permit / socket / handshake / init / register **全阶段失败矩阵**下隧道引用的正确回滚。
2. 「可确认关闭的资源许可归零」——`CleanupDisposition::Closed` 与隧道引用核销的配对断言。
3. 「未确认关闭进入隔离」——`Quarantined` 下隧道引用与物理预算的归属规则。

**CM-28「隧道不多减引用」剩余**（本轨已覆盖：重复释放幂等、refs 不低于 0、close 至多一次）

4. 20 次并发归还下的 driver close 至多一次有效关闭。
5. 并发归还下预算不负数、重复响应一致。

两条的隧道侧均已由 `tunnel/` 覆盖，剩余项都是**非隧道**部分，领取时不需要碰 `tunnel/`。

## 门禁实测

工作树：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p3-cm32-tunnel-refcount`
分支 `feature/p3-cm32-tunnel-refcount` · **交付提交 `b111b026149a08f12442f3cf99131318fd4b1186`**（`2836d346d` 实现 + `b111b0261` 台账）
基线 `21e27e4ebc6c05c21c69b14e30fe3248891c2c5b` · `CARGO_TARGET_DIR=/tmp/dz-target-p3-cm32-tunnel-refcount`

### 基线口径（不要把旧的 382/405 搬过来）

本轨开发期间 `main` 合入了 CM-74，`packages/runtime` 的基线数字已经变了。四组数字口径不同，必须分清：

| 口径 | 树 | `--lib` | 全量 | 二进制 |
| --- | --- | --- | --- | --- |
| ① 本轨开发基线（**已过期**） | `21e27e4e` | 382 | 562 | 20 |
| ② 交付树（旧基线 + 本轨） | `b111b026` | **405** = 382+23 | **592** = 562+30 | 21 |
| ③ 当前 `main`（含 CM-74，无本轨） | `07c77d40` | **384** | **564** | 20 |
| ④ **`main` + 本轨（交付后真实形态）** | 演练树 `af5baa65` | **407** = 384+23 | **594** = 564+30 | 21 |

④ 是在一次性 detached 工作树里 `git merge main` 演练得到：**0 冲突**，我的分支未被改写，演练树已删除。**验收应以 ④ 为准**——两侧基线都从 384/564 起算，新增 23 条 `--lib` 测试 + 1 个 7 条测试二进制。

### 逐字结论行

交付树 `b111b026`（口径 ②）：

| 命令 | 退出码 | 逐字结论行 |
| --- | --- | --- |
| `cargo test -p datazen-runtime --lib` | `0` | `test result: ok. 405 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s` |
| `cargo test -p datazen-runtime --test tunnel_refcount_contract` | `0` | `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `cargo test -p datazen-runtime`（21 个二进制） | `0` | 21 × `test result: ok.`，`passed=592 failed=0`，告警 0 条 |
| `cargo fmt -p datazen-runtime -- --check` | `0` | 无输出 |

`main` 基线树 `07c77d40`（口径 ③）：

```
cargo test -p datazen-runtime   EXIT=0
test result: ok. 384 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
20 个二进制，passed_total=564，failed=0，告警 0 条，日志内 'tunnel' 出现 0 次
```

合并演练树 `af5baa65`（口径 ④）：

```
cargo test -p datazen-runtime   EXIT=0
test result: ok. 407 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
21 个二进制，passed_total=594，failed=0，告警 0 条
cargo fmt -p datazen-runtime -- --check   FMT_EXIT=0
```

### 两个提交各自独立绿

- **`2836d346d`（实现）单独检出跑门禁**：`EXIT=0`，`--lib` `405 passed; 0 failed`，全量 `passed_total=592`，21 个二进制，告警 0 条。该提交**不含** `progress.md`（`git ls-files progress.md` = 0）。
- **`b111b0261`（台账）**只新增根目录 `progress.md`，不经过 Rust 工具链，门禁结果与上一提交逐字一致。
- 20 个既有测试二进制在两种口径下都保持绿（`gateway_contract` 51、`p3_session_port_contract` 10 不变），新增第 21 个。
- 最大文件 `tests/tunnel_refcount_contract.rs` 439 行，全部 < 800 行上限。

### 运行次数如实记账

`--lib` 在本轨**实际执行 14 次**：13 次单独 `cargo test -p datazen-runtime --lib`，加 1 次包含在整包 `cargo test -p datazen-runtime` 里。第 1–9 次在实现期收敛（第 9 次绿），第 10–12 次在 (a) 接线落地后收敛，第 13 次为交付前单独复跑，第 14 次为整包门禁内的复现。第 10–11 次的失败是真实回归，不是环境噪声：

- 第 10 次 `404 passed; 1 failed` —— 新加的 `AlreadyHeld` 守卫与 `journey_single_counter.rs::independent_specs_never_share_a_count` 冲突：该测试原先用**同一个** `holder` 对同一 spec 反复 acquire 来把计数撑到 3。守卫是对的（一条资源一条引用，否则一次归还只减 1），测试改成 N 个不同持有方，并把归零改走 `return_resource` 交叉顺序。
- 第 11 次 `404 passed; 1 failed` —— 上次改动自身的断言写错（把「递减后的剩余数」写成了恒等于 0），改为按 spec 分别维护期望剩余值。

没有出现「跑到绿为止」的循环：第 12 次绿之后只复跑确认（第 13、14 次），结论一致。整包门禁共执行 2 次，两次结论一致（`EXIT=0`，`passed=592 failed=0`，告警 0 条）。

基线重测阶段（本轨交付前）**另加 3 棵一次性 detached 工作树**，各跑 1 次整包门禁（`07c77d40` 基线、`af5baa65` 合并演练、`2836d346` 单提交核验），`--lib` 各隐含 1 次。合并演练在 scratch 树里 `git merge main`，**我的分支未被 rebase、未被 amend**，演练树与三棵工作树事后全部 `git worktree remove --force` 清理。

### 树未被并发改动

整包门禁运行前后各记录一次 HEAD 与工作区 sha，两次完全相同：

```
BEFORE  HEAD 21e27e4ebc6c05c21c69b14e30fe3248891c2c5b
BEFORE  git status --porcelain | sha256sum = b8af2d81a404b64c33e4ea6554fd9fc573fd4bbefbbb185a701e83e389973024
AFTER   HEAD 21e27e4ebc6c05c21c69b14e30fe3248891c2c5b
AFTER   git status --porcelain | sha256sum = b8af2d81a404b64c33e4ea6554fd9fc573fd4bbefbbb185a701e83e389973024
```

未触碰的文件（逐一核对）：`packages/platform-api/src/ports/network.rs`、`packages/runtime/src/connection/port.rs`、`hub.md`、`src-tauri/**`、`docs/**`。

提交后本轨工作树 `git status --porcelain -uall` 为 **0 条**，验证方可直接在 detached 树上开作业面。

## 独立验收结论（Tester `ee7b59f0`）

Tester 在 detached `2836d346d` 上验收：**ACCEPT，可合并**。diff 纯增量（11 文件 / +2301 / **−0**），冻结面全空，四个门禁全绿。**但点名两处合并前必须修，故不是干净 PASS**，本轨据此进入 repair round 1（见下）。

### repair round 1 修的两处

1. **`transport.rs` 模块头的「结构上无法维护计数」是假话，已改写。**
   Tester 反证：`&self` 只排除 `&mut self`，而 `Mutex`/`Cell` 是内部可变性，本就不需要 `&mut self`；`TunnelTransport: Send + Sync` 下放 `Mutex<usize>` 完全合法。反例就在本轨夹具里 —— `RecordingTunnelTransport.journal: Mutex<...>` 与 crate 外宿主 `HostTunnelTransport.events: Mutex<...>`。
   改后口径：约束**不由类型系统保证**，由字段审计保证；真实约束是「释放决策只读 `TunnelEntry.refs`」与「`TunnelTransport` 的三个返回值都不携带计数」。同口径的错误表述在 `mod.rs:47` 一并改正（原先是同一句假话的第二个落点）。测试数不变。
2. **`the_binding_snapshot_never_decides_whether_to_release` 判别力不足，已补。**
   原用例末句 `assert_eq!(ledger.ref_count(&spec), Some(1))` 杀不掉「把权威读换成冻结快照」（M4）：该断言点上两个来源**恰好都等于 1**，Tester 确认 M4 下仍 `1 passed; 0 failed`，且全轨找不到它的独占杀手。
   修法（采纳 Tester 意见）：**第三次 acquire** 把台账推到 3、`first_lease.binding.ref_count` 冻结在 1，释放一次后台账读 `Some(2)`；并加 `assert_ne!` **自证两来源在断言点上不相等**，防止同类空过再次悄悄回来。同样的空过也存在于契约二进制 `tests/tunnel_refcount_contract.rs:435`，一并按同一手法修好。原有的 `!released.closed` 与「快照冻结性」断言保留不动。

### 已知名洞（登记，本轮**不修**，留给跟进轨）

- **唯一计数铁律被 M2 证伪。** 变异：给 `RecordingTunnelTransport` 加 `close_tally: Mutex<usize>` 并让 `close_calls()` 改读它 —— **编译通过，405 + 7 全绿**。即不变式「全系统只有一份计数」当前**由审计保证，不由编译器保证**。两个候选修法（断言释放路径只读一个计数 / 让 `drain` 按值从单个私有方法取计数而非直读结构体字段）留待跟进轨裁决。该事实已写进 `transport.rs` 模块头，不只活在台账里。
- **`AlreadyHeld`（调用方 bug）当前映射到 `SessionQuarantined`**；`TunnelError::rejected()` 与 `as_runtime_error()` 在 `error.rs` 外**零调用方**（Tester 实测 `count_outside_error_rs=0`）。一旦 `rejected()` 拿到第一个调用方，双重 acquire 就会**把健康会话隔离掉**。属 seam 阶段裁定项，本轮不动，已登记。

### Tester 纠正的两条前提（记下来，别再沿用旧说法）

1. 「不带 `test-harness` 的构建不覆盖被改文件」——**对 CM-32 是反的**。`test-harness = []` 是空 flag，只闸住 `connection::testing`；`pub mod tunnel;` 在 `lib.rs:31` **无条件**，`tunnel/mod.rs` 的 `error`/`ledger`/`transport` 均未加 gate。裸 `cargo build -p datazen-runtime` **确实覆盖全部被改生产文件**。（该规则对 CM-74 的 `connection/testing/**` 仍然成立，别推广。）
2. 两个 close 计数器「语义不一致、关闭失败路径上 `close_calls == N−M` 会失配」——**该结论已由 Tester 自行撤回**。实际是假件在 journal 之前就 `Err`，两者按构造一致，且 M11 证明这份一致性被断言钉住。**这是优点，不要去「修」一个不存在的问题。**

### CM-27 / CM-28 覆盖声明经复核为诚实

Tester 逐条复核：标「已覆盖」的为真；标「剩余」的**确实仍未闭合**，包括 `TunnelFault` 只有 `{Open, Close}` 故握手中途失败无法表达、CM-27 ② `CleanupDisposition::Closed` 在 `src/tunnel/*.rs` 中 `grep -c` 为 **0**、`Quarantined` 归属缺失、CM-28 ④ 20 并发归还与 CM-28 ⑤ 并发预算。**无夸大。**
