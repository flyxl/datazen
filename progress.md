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

**完成（断言 3）。** 缺陷 A 已修（7 条新用例全绿，runtime 1015/0、host lib 1810/0/6、
协议双门禁 EXIT=0）；缺陷 B 修正为零行为变化的安全网并集。门禁与失败证据见下两节。


## 改动清单（文件:行 → 改了什么）

| 文件 | 行 | 改了什么 |
|---|---|---|
| `packages/runtime/src/resource/cleanup.rs` | 132-140 | 新增 `HostConditionSnapshot::outstanding_handles()`：**唯一**的句柄口径，过滤 `closed` |
| " | 143-148 | `is_clear()` 改读该谓词（去掉 `const`）—— 已关闭的句柄不再挡归池 |
| " | 180-184 | `first_blocker()` 句柄那一格改读该谓词 |
| " | 221-227 | `all_blockers()` 句柄那一格改读该谓词 |
| " | 538-540 | `CleanupReport::from_plan` 的 `outstanding_handles` 改读该谓词（两份口径合一） |
| " | 79-99 | 字段文档写清「`closed` 的句柄不是未了义务」与过滤为什么放在读取侧 |
| `packages/runtime/src/resource/manager.rs` | 480 | `needs_reset` 改读同一谓词（四处口径合一） |
| `packages/runtime/src/resource/lease_manager.rs` | 459-486 | (2) 宿主条件改成「调用方声明 ∪ 登记册**此刻还剩**的句柄」；理由与「为什么关净后允许归池」写全 |
| `packages/runtime/tests/cm74_assert3_clean_verdict_with_registered_handle.rs` | 新文件 696 行 | 7 条 CM-74 断言 3 用例 |

**未新增任何 `ReturnReason` 变体。** 本轨只修口径，没有新的阻断项类型。

## 门禁（全部本轨实测，未采信任何他人数字）

首尾各记录一次 HEAD 与工作区 sha，证明运行期间没人动过：
起始 `HEAD=9d6c1145f`（代码提交）→ 收尾 `HEAD=9d6c1145f`、`git status --porcelain` 空。

| 门禁 | 命令 | 退出码 | 逐字结论行 |
|---|---|---|---|
| A | `cargo fmt --all -- --check` | 0 | 无任何输出（0 字节日志） |
| B | `cargo check --workspace --all-targets` | 0 | `Finished dev profile [unoptimized + debuginfo] target(s) in 16m 16s`；`grep -cE "^error"` = **0** |
| C | `cargo test -p datazen-runtime --no-fail-fast` | 0 | **1015 passed / 0 failed**（基线 `abcbb41d0` 实测 **1008**，差 7 = 本轨新增） |
| D | `cargo test -p datazen --lib` | 0 | `test result: ok. 1810 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 46.02s` |
| E1 | `node scripts/check-driver-protocol-compat.mjs --base abcbb41d0` | 0 | `[check-driver-protocol-compat] ok (base abcbb41d0; PROTOCOL_VERSION 5 -> 5, MIN_PROTOCOL_VERSION 1 -> 1, crate 0.0.11 -> 0.0.11).` |
| E2 | `node scripts/check-driver-protocol-release.mjs --base abcbb41d0` | 0 | `[check-driver-protocol-release] ok (base 9d6c1145f; ...; driver releases: 0)` |
| F | 新测试在基线 `abcbb41d0` 上跑 | 101 | 见下节 |
| G | `git status --porcelain` | — | 空；三个 codegen 均在 `.gitignore` 中（`.gitignore:69/99/72`），未 add 未删 |

## 失败证据（改动前 = 基线 `abcbb41d0`）

独立 detached worktree `/tmp/cm74-baseline-probe` @ `abcbb41d0`（跑完已 `git worktree remove --force`，
未碰本工作树），只把新测试文件逐字拷入，**不改任何生产代码**：

```
$ cargo test -p datazen-runtime --test cm74_assert3_clean_verdict_with_registered_handle
BASELINE_A3_EXIT=101

test a_clean_driver_verdict_never_carries_a_registered_cursor_into_the_pool ... ok
test a_clean_driver_verdict_never_carries_a_registered_handle_into_the_pool ... ok
test a_handle_left_in_the_registry_never_parks_even_when_the_host_claims_clear ... ok
test a_handle_that_cannot_be_closed_never_parks_and_stays_retryable ... ok
test a_clean_driver_verdict_parks_after_the_registry_is_actually_clean ... ok
test a_lease_without_any_handle_still_parks_on_the_production_release_path ... ok
test a_closed_session_handle_no_longer_blocks_the_return ... FAILED

---- a_closed_session_handle_no_longer_blocks_the_return stdout ----
thread '...' panicked at ...cm74_assert3_clean_verdict_with_registered_handle.rs:355:5:
a closed handle is not an outstanding obligation: the lease must park

test result: FAILED. 6 passed; 1 failed; 0 ignored
```

**计数反证**（同一棵基线树，先带新测试再删掉它各跑一次）：
带新测试 ⇒ `1014 passed / 1 failed`；删掉新测试 ⇒ `1008 passed / 0 failed`。
差的 7 个正是本轨新增，没有悄悄增删别的用例。

## 变异验证（证明新测试真的在守东西）

独立 detached worktree `/tmp/cm74-mutation-probe` @ `9d6c1145f`，跑完已删。

| 变异 | 结果 |
|---|---|
| M1 `is_clear()` 退回 `unreleased_handles.is_empty()` | **未被抓到**（EXIT=0）。`first_blocker` 那条路先判了，`is_clear` 在这条用例上不是承重墙 |
| M2 `first_blocker()` 退回读 `unreleased_handles.first()` | **被抓到**（EXIT=101），`a_closed_session_handle_no_longer_blocks_the_return` 转红 |
| M3 `all_blockers()` 退回读 `&self.unreleased_handles` | **未被抓到**（EXIT=0）。它只进报表不进裁决，说明本轨对它的改动是**口径统一**而非修复 |
| M4 `lease_manager` 并集退回「只用调用方」 | **未被抓到**（EXIT=0）。证实这一处是**零行为变化的安全网**，与台账记录一致，不是修复 |

**诚实结论**：真正承重的是 `first_blocker()` 那一格（M2）。`is_clear` / `all_blockers` /
`lease_manager` 并集三处改动是为口径一致与未来防护，**不是本轮缺陷的修复点**。
这一点必须让协调者知道：本轨的净行为修复只有一处。

## 取舍与待裁决项

1. **缺陷 B 我第一版修错了**（详见台账「缺陷 B」节）。第一版把「关之前的登记快照」
   并进宿主条件，三条既有用例转红；查 §6.5/§9.4 后发现是我错、它们对。
   **教训**：既有用例转红时先查权威文本再判谁错，不要默认「新写的测试更对」。
2. **`is_clear()` 不再是 `const fn`**。已核实全仓无 const 依赖（`admits_to_pool` 自身
   无生产调用方）。若将来有人在 const 上下文用它，会得到编译错误而不是静默降级 —— 可接受。
3. **变异 M1/M3/M4 未被抓到** ⇒ 这三处改动的价值是**防止未来回归**，不是本轮修复。
   若协调者希望最小 diff，可以只保留 `first_blocker` 的改动与 7 条测试，
   但代价是 `is_clear` / `all_blockers` / `from_plan` 三处继续和 `first_blocker` 口径不一致
   —— 我选择保留统一，理由写在各处注释里。
4. **本轨只交付断言 3。** 四条后续落点见台账「本轨明确交回给后续的四条」表。
5. **零 `ReturnReason` 新增**，因此与 `p3-reset-capability` 轨的合流面只有
   `cleanup.rs::HostConditionSnapshot` 一个 impl 块内的三个方法体，且我改的是
   **句柄那一格**（第 3 格）、它改的是**末位能力格**，不同臂、不同行。
6. **`docs/` 一字未改** —— 文档说该做的（§9.4「§6.5 已登记句柄为空」）代码做到了，
   反倒是代码对「已关闭的句柄」多挡了一道，那是代码偏离文档，不是文档偏离代码。

## 遗留

无未完成项。断言 3 已转绿并附失败证据。
