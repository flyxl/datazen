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

## 状态

**进行中**。设计已落账（上一节）。代码与门禁见下节，随进度更新。

## 改动清单（文件:行 → 改了什么）

_待填。_

## 门禁（全部本轨实测，未采信任何他人数字）

_待填。_

## 失败证据（改动前）

_待填。_

## 取舍与待裁决项

_待填。_
