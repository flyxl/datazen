# 进度台账 — 轨 `feature/cm74-handle-release`

基准 `feat/p2-p3-p4-completion` @ `abcbb41d0`（已含轨 1「reset 三态」与轨 `p3-reset-capability`
能力闸；本轨已 rebase 上去，HEAD `0774ec834`）。
工作树 `/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-cm74-handle-release`。
`CARGO_TARGET_DIR=/tmp/cargo-cm74`。

## 目标（一句话）

按 CM-74（`docs/architecture/platform/connection-management.md:1323-1338`）的五条断言写测试，
修掉它们暴露的真缺陷；只动归池检查行为与 `connection/testing/**`，不碰
`PhysicalTransport::reset` 签名、不做能力分流、不动 `drivers/**`、`driver-api/**`、
`application/**`、`registry/**`、文档措辞与 CI。

## 设计（先落账，再写码）

### 三条路径的夹具落点（CM-74 步骤的三条路径）

CM-74 步骤说「再分别走归池、setSessionContext 替换、closeSession 三条路径」。三条在
本仓库的既有接缝上是：

| CM-74 路径 | 夹具落点 | 说明 |
|---|---|---|
| 归池 | `FakeHarness::close`（走 `close_resource` 的归池判据） | `ClosePrecondition::returns_to_pool()` 五合取 |
| 替换 | `ResourceManager::begin_replacement` / `confirm_replacement`（`resource/adoption.rs`） | 候选 `published=false` ⇒ `accepts_execution` 必拒；提交后旧会话 `old_session_executable()==false` |
| closeSession | `FakeHarness::close` / `evict_idle_resource` | 资源关闭前先注销句柄 |

**关键事实（已核实）**：`connection/testing/**` 这套 `FakeHarness` 夹具**没有** setSessionContext
替换入口 —— 替换在 `resource/**` 侧（`ResourceManager::begin_replacement`）。所以
「替换路径」的行为断言落点是 `packages/runtime/tests/` 下基于 `ResourceManager` 的集成测试，
而不是 `FakeHarness` 里的 `invoke`。这是既有结构，不是我选的分法。

### 断言 3（最高优先）的实测方法

断言 3 是「driver 返回 Clean 时若宿主仍有已登记句柄，宿主检查必须失败（§9.4）」。
现有覆盖分三层，我要补的是**第三条路径上它是否真的被检查**：

1. `ClosePrecondition::returns_to_pool()` 的合取项 4 `measured_handles == 0`
   ——**已被** `close_cases::pooling_refuses_a_host_ledger_that_undercounts_the_registered_handles`
     钉住（实测 1 / 声明 0 ⇒ 不归池）。
2. `HostConditionSnapshot::unreleased_handles` ⇒ `first_blocker()` 给
   `UnreleasedSessionHandle` ——**已被** `resource_return_to_pool.rs::an_unreleased_session_handle_refuses_reuse_and_is_reset_before_the_close`
     钉住（driver Clean + 宿主报句柄 ⇒ `hostUnreleasedSessionHandle`）。
3. **缺口**：`LeaseManager::release`（`resource/lease_manager.rs:431`）这条**生产**路径上，
   「驱动 Clean + 宿主四项全清 + 登记册里有句柄」这一组合**没有测试**。
   危险在于：`lease_manager` 第 (1) 步会先 `closer.close(&handle)` 再 `deregister`，
   然后第 (2) 步用调用方给的 `host` 合 `merged.unreleased_handles`。
   若某处拿错了顺序或判据读了登记之后的余量，`ReturnedToPool` 就会被记下来。

   ⇒ 我的新测试：`LeaseManager::release` 上「登记了句柄 + 驱动 Clean + 宿主四项全清」，
   断 `!cleanup.returned_to_pool()` + `handles.deregistered` 非空（先关再摘）+ 物理顺序
   `Open → Reset → Close`。这条同时服务断言 1 与断言 3。

### 断言 1（句柄在物理资源关闭前已从 actor 注销）

既有覆盖：`cm74_release_order.rs` 两条钉 journal 次序
`handle closed → resource Closed → permit -1`。**缺口**：`lease_manager` 这条生产路径上没有
「关句柄那一刻物理层还没动过」的顺序证明（`lease_handle_registry.rs:182`
的 `handle_closes_happen_before_any_physical_reset_or_close` 是在 `FixedSession` 上做的，
且只断言 `count == 0`）。⇒ 新测试补 `ShortOperation` + 判顺序而非计数。

### 断言 2（commit 落在旧 resource 或明确失败，不出现在新 resource）

`FakeHarness` 有 `commit_session_transaction`，跨资源判负已有
`a_handle_from_another_resource_is_rejected`。**缺口**：**替换提交之后**（新 resource 已发布）
拿旧 resource 上的句柄提交，必须被拒且不得落在新 resource 上。
⇒ 新测试走 `ResourceManager`：`begin_replacement` → `confirm_replacement` →
旧句柄提交判负（`CandidateNotPublished` / `HostConditionUnmet`）+ 新 resource 上无 `Reset`/无句柄登记。

### 断言 4（未登记句柄被拒绝返回）

既有：`begin_session_transaction_unregistered` 造 orphan，`close_cases` 钉 orphan 回收。
**缺口**：未登记句柄**直接提交**（不是通过反例命令，而是宿主拿一个从未登记的
`handleId` 去调 commit）—— 这条在 `LeaseManager::release` 的视角下是
「句柄没关掉」，归还必须失败。⇒ 新测试：`NoHandleCloser` + 登记册空 ⇒ `TransportRefused`。

### 断言 5（actor 终止后不重建任何句柄，恢复只能以新 dbSessionId）

既有：`cm73.rs` 五步 + `tests.rs::the_eviction_race_recovers_on_a_fresh_resource_without_reusing_handles`。
**缺口**：**恢复之后**再用旧 `dbSessionId` 重新 acquire（而非新 id）会怎样。
⇒ 新测试：驱逐后拿旧 id 走恢复路径，必须拿到**新**的 dbSessionId（`FakeIds` 按序发号），
且新 resource 上零句柄登记。

## 拆分决定：**选项 B**（先只做断言 3），其余四条记入台账作为后续

### 理由（自判，不是执行上级安排）

我选 B 而不是 A 或 C，判据是**「缺口是不是真的」**与**「落点热不热」**两条一起看：

**1. 断言 3 大概率已经成立，我倾向用实测代替改码。**
`CleanupPlan::of`（`cleanup.rs:397`）→ `ReturnDecision::decide` → `host.first_blocker()`，
而 `first_blocker()` 的第 3 格正是 `unreleased_handles` ⇒
`ReturnReason::UnreleasedSessionHandle { .. }`（`cleanup.rs:146-155`）。
`Returned` 只在 `first_blocker()` 返回 `None` 时成立，也就是**四项全清**。
「driver 报 Clean」只让裁决走到 `first_blocker()`，它**不能**让任何一项宿主条件消失
（`decide` 的注释明写这句）。所以断言 3 在结构上成立。
⇒ 我的活是**写一条能杀死「读错数」这种变形的测试**，而不是改代码。

**2. 断言 3 不碰 harness，而其余四条都要碰。**
父代理给的落点表我逐条核对了：断言 1 的 journal 次序、断言 4 的登记校验、
断言 5 的恢复判负，全部要改 `connection/testing/harness/**` 或 `fake_resource/**`
（这正是原简报点名的「已有夹具，别重造」区域）。断言 3 只动 `resource/cleanup.rs::of`
这条判据与 `packages/runtime/tests/` 下基于 `ResourceManager` 的集成测试。

**3. 五条一起做治不了我的毛病，反而放大它。**
本会话已连续 4 次中断、每次都零提交。一次交付五条 = 五个可能出错的推理链条 +
harness 与两个已合流轨的潜在冲突全部堆在同一次提交里。断言 3 单独一条：
- 落点最小（`cleanup.rs::of` + 一个新集成测试文件）；
- 与 `p3-reset-capability` 轨的冲突面最小 —— 那轨改的是 `first_blocker`/`all_blockers`
  **末位**（`reset_for_reuse`），我改的是**句柄那一格**（第 3 格），同一函数但不同臂；
- 最可能成活（预期零代码改动，只加测试）。

**4. A 的并行前提不成立。**
A 要求两轨不碰同一文件。但断言 3 的集成测试要写 `HostConditionSnapshot::with_unreleased_handle`，
而这正是 `resource_return_to_pool.rs` / `lease_transitions_matrix.rs` / `tunnel_budget_pairing.rs`
三个文件共用的构造；断言 1/4/5 那条轨一旦也要写 `LeaseManager::release` 的句柄用例，
两边都会落到 `packages/runtime/tests/`。真实冲突比父代理估的更近。

### 边界（本轨只做这些）

- **改**：`packages/runtime/tests/cm74_assert3_clean_verdict_with_registered_handle.rs`（新文件）
- **可能改**：`packages/runtime/src/resource/cleanup.rs`（仅在实测发现判据真被绕过时）
- **不碰**：`connection/testing/**`、`resource/lease_manager.rs`、`resource/manager.rs`、
  `resource/mod.rs` 的导出面、`PhysicalTransport::reset` 签名

### 本轨明确交回给后续的四条

| 断言 | 后续落点（已核实存在，但本轨不碰） |
|---|---|
| 1 | `resource/lease_manager.rs:447-455` 的 close→deregister 次序 + `connection/testing/harness/cm74_release_order.rs` |
| 2 | `resource/adoption.rs::confirm_replacement` + `packages/runtime/tests/resource_replacement.rs` |
| 4 | `session_cmds.rs:138-141` 的 `SessionNotFound` + `lease_manager.rs:449` 的 `?` 中断 |
| 5 | `harness/cm73.rs` 五步 + `harness/tests.rs:437` 的判负 |

## 实测结果：**断言 3 不成立，两个真缺陷**

「先写测试当实验」直接跑出两条失败（基线 `abcbb41d0` 上 `cargo test -p datazen-runtime
--test cm74_assert3_clean_verdict_with_registered_handle`，EXIT=101，
`6 tests` 中 2 条 FAILED）。

### 缺陷 A（严重）：已关闭的句柄仍把租约挡在池外，`is_clear()` 与报表口径互相矛盾

`HostConditionSnapshot::is_clear()`（`cleanup.rs:115`）判 `unreleased_handles.is_empty()`，
**不看 `closed` 标志**；而同一文件 `CleanupReport::from_plan`（`cleanup.rs:502-510`）
在填 `outstanding_handles` 时**过滤掉** `!handle.closed`。两份口径对同一个快照给出
不同答案：

- `is_clear()`：`[closed=true 的句柄]` ⇒ **不清** ⇒ 拒绝归池；
- `from_plan`：同一个快照 ⇒ `outstanding_handles == []` ⇒ 报表说「没有未了句柄」。

后果是**永久泄漏**：宿主已经把它的事务提交/回滚掉（句柄 `closed = true`，按
`SessionHandleRef` 的契约这是合法终态，`connection/execution.rs` 的 `ExecutionCompletion`
正是这样上报的），却因为一条死句柄永远无法再进空闲池。每次短操作都新开物理连接、
关掉，池恒空。这与 §9.3「归还到 idle pool 不释放物理连接预算」配套的池化收益
整体失效，且**没有任何一个既有测试覆盖过 `closed = true` 的输入**
（`grep -rn "\.closed = true" packages/runtime/tests` 只有本轨新写的那一处）。

`SessionHandleRef.closed` 的语义写在 `connection/session.rs:42`：「必须……登记到
session actor；未登记 = 非法」，而 `closed` 字段是给「已自行关闭、登记还挂着」用的
对称另一半（`lease_manager.rs:621-624` 的 `deregister_handle` 文档明写这句）。
所以「closed 的句柄不是未了义务」是本类型的既有契约，`is_clear` 漏掉它才是缺陷。

**修法**：新增 `outstanding_handles()`（过滤 `closed`）谓词，`is_clear()` / `first_blocker()`
/ `all_blockers()` / `manager.rs` 的 `needs_reset` 四处统一改读它。`is_clear()` 是
`const fn`，改成读迭代器方法后**不再是 const** —— 已核实全仓无调用方依赖 const
（`grep -rn "is_clear()"` 只有 `admits_to_pool` 与两处文档提及；`admits_to_pool` 本身
也只有文档引用，无生产调用点）。

### 缺陷 B（第一版修法被自己推翻，记录这个弯路）

`LeaseManager::release`（`lease_manager.rs:447-472`）第 (2) 步旧写法是
`merged.unreleased_handles = host.unreleased_handles.filter(!closed)`
—— 只用调用方声明的那一份，`self.handles` 里真实存在的句柄对**裁决**不可见。
第一版我据此判它是缺陷，改成「并集关**之前**的登记快照」。

**然后三条既有用例当场转红**：

| 用例 | 断言 |
|---|---|
| `lease_handle_registry.rs:102` | "once the registry is clean the lease may go back to the idle pool" |
| `lease_transitions_matrix.rs:570` | "a closer that can close the handle lets the return through" |
| `lease_handle_registry.rs:177` | 重试后 `released.pool.park` |

**我判这三条对、我错**，依据是两份权威文本而不是用例数量：

1. §6.5 逐字：「淘汰、替换或隔离时，**先在原 resource 上回滚/关闭句柄并从 actor 注销，
   确认后才释放资源**」。关掉 + 摘登记 = 登记册已清空 = §9.4 那一项归池前置
   **已满足**。此时仍不放行，等于让一条彻底收尾过的租约永远回不了池 ——
   短操作每次新开物理连接再关掉，池恒空，池化收益整体失效。
2. 我自己的缺陷 A 修法就是这个理由：`outstanding_handles()` 过滤 `closed`
   的论证是「宿主把它提交/回滚掉之后，它就不再是一项未了义务」。
   同一条理由在这里一字不差地成立。我一边过滤 `closed`、一边又把关掉的句柄
   并回去，是自相矛盾。

**所以 B 不是缺陷，「以调用方为准」这一侧的真正缺口是**：登记册**此刻还剩**
（关不掉、没摘掉）的句柄，必须并入宿主条件。旧写法漏了这个，
于是 `NoHandleCloser` 之外还有一条缝 —— 但那条缝被 (1) 的 `?` 提前返回堵住了，
所以旧写法在既有用法下恰好不出错。**修正后的并集因此是零行为变化的安全网**，
不是修复：它让「本层是登记册唯一权威」这句话成为代码而不是注释。

**这一条的教训记入取舍节**：三条既有用例转红时，先查权威文本再判谁错。
我第一反应是「它们钉的是缺陷」，查完 §6.5/§9.4 发现是我错。

## 状态

**进行中**。缺陷 A 已修（7 条新用例全绿，1015/0）；缺陷 B 修正为安全网并集。
门禁见下节，随进度更新。
## 状态

**进行中**。两个缺陷已实测并定位；修法与证据见上一节。门禁见下节，随进度更新。

## 改动清单（文件:行 → 改了什么）

_待填。_

## 门禁（全部本轨实测，未采信任何他人数字）

_待填。_

## 失败证据（改动前）

_待填。_

## 取舍与待裁决项

_待填。_
