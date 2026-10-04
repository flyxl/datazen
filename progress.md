# p3-cm74-release-order 进度台账

> 开发期台账，合并时必须删除（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。

## 目标

把 CM-74 的释放顺序做成**真正统一**：宿主关闭路径与驱动直连路径产出**同一条** journal 顺序
`handle closed → resource Closed → permit -1`，且 F10「驱动报 Clean 但句柄非空 → 关闭而非归池」
不得回归。

## 边界

- worktree：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p3-cm74-release-order`
- branch：`feature/p3-cm74-release-order`
- baseline：`8d7474a9a91cc9f3ccef9cfd9c03f9179c38be71`（`main`）
- `CARGO_TARGET_DIR=/tmp/dz-target-p3-cm74-release-order`
- 不改 `connection/port.rs`（冻结 API）、不改 `hub.md`、不改其他 worktree。

## 复验基线（逐字结论行）

`cargo test -p datazen-runtime`（全量，含 18 个集成二进制 + lib + doc-tests）

```
test result: ok. 382 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
EXIT=0
HEAD_AFTER=8d7474a9a91cc9f3ccef9cfd9c03f9179c38be71
DIRTY=0
```

20 个测试目标全绿：lib 382 + 集成 180 + doc-tests 0 = **562 passed / 0 failed**。

> **更正**：早前口头汇报的「552」是**算术错误**（把集成目标少算了 10），不是任何一次工具读数。
> 日志实测为 **TOTAL=562 / `--lib` 382**。以本行为准。
`packages/runtime/Cargo.toml` 无 `[[test]]` 段，二进制布局就是 `tests/*.rs` 18 个 + `src/lib.rs` + doc-tests。

## 关键事实（本轮查证）

### 1. 冲突不存在于两份文档之间，而存在于实现与 CM-74 之间

`connection-management.md:1324` 的 CM-74 **同一条断言子句表**里同时写着两件事：

- 句柄在物理资源关闭**之前**已在原 resource 上回滚/关闭并从 actor 注销；
- driver 返回 Clean 时若宿主仍有已登记句柄，宿主检查**必须失败**（§9.4）。

F10（`fake-runtime-fixtures.md:221`）对第二点只是回指 CM-74。所以「先注销再判归池」会让 CM-74
自己失败：**判据问的是「关闭开始前资源上挂没挂着句柄」，不是「注销之后还剩几个」。**
注销前快照不是绕过 CM-74 的取巧，它是 CM-74 的字面要求。

### 2. 协调者担心的 TOCTOU 不存在

`FakeResourceProvider::close_resource`（`ops.rs:503`）是**同步** `fn`（非 async），全部 slot 变更走
同一把 `Mutex`（`self.lock()`）。取快照 + 注销 + 置 `Closed` 可以放在**一次 `self.lock()` 临界区**内完成，
结构上没有观察窗口，也不必等锁。

### 3. 快照取在 `close_resource` 内部，优于接线 `CloseResourceRequest.registered_handles`

该字段今天是**惰性的**：`ops.rs:576` 读的是 `slot.registered_handles()`。
`harness/mod.rs:173` 传的已经是**注销之后**的值（宿主已先注销句柄），
`cm73.rs:155-156` 的 doc 也明说「传进去的值只是调用方的声明」。接线它会把负担推给调用方，
并且直接与 `FakeHarness::close` 的形状冲突。

### 4. 协调者问的「更简单构造 = 显式布尔标志」不成立

在这份代码里**快照本身就是那个显式标志**，只是取在正确的时刻。
新增一个持久布尔字段等于造出**第二个真相源**，会与 `slot.handles` 漂移，还得维护一条不变量。
快照不新增状态、不漂移、无额外不变量。建议**不做**。

### 5. 直接路径退化的确切原因

`FakeResourceScript::rollback_before_release` 在新建脚本上默认 `false`（`script.rs:354`），
所以 `ops.rs:540` 的注销分支在直连路径**整段被跳过**；`handle closed` 由
`ops.rs:629` 的 `reclaim_registered_handles_on_close` 补记——它排在归池判定与 permit 归还**之后**，
于是顺序退化成 `resource Closed → permit -1 → handle closed`。

### 6. 现存守卫（本次改动的风险清单）

| 守卫 | 位置 | 改动后 |
|---|---|---|
| F10 不得归池（直连路径） | `fake_resource/tests.rs:439` | 有快照 → 仍不归池 ✅ |
| 竞态资源不得归池 | `harness/cm73_threads.rs:252-260` | 竞态脚本 `rollback_before_release = true`，行为不变 ✅ |
| 空闲资源精确序列 `["Created","OpeningReady","ReturnedToPool","Closed"]` | `cm73_threads.rs:364` | 快照 = 0 → 仍归池 ✅ |
| F11 `CloseUnconfirmed` 不归还 permit、`resource_release: Pending` | `ops.rs:513-536` | 完全不动 ✅ |
| F12 orphan 路径 | `fake_resource/tests.rs:623` | orphan 不在 `slot.handles` 里，走原 `recover_orphans_on_close` ✅ |
| `registered_handle_ids_on` 只看 `Registered` 条目 | `harness/cm73.rs:178` | 新增的 `Closed` 条目不参与 ✅ |

已核查：全仓**没有**任何「close 之后读 `registered_handles()`」的断言。
`harness/tests.rs:179`、`fake_resource/tests.rs:634`、`cm73_threads.rs:228` 都在关闭**之前**读。

## 待裁定

**`connection-management.md` §7.4 第 6 项**（`setSessionContext` 的 `requiresReplacement`
候选资源 + 原子发布协议）**不在本轨**，需另开轨。此处记录以免遗漏。

## 计划（待用户裁定后再写实现）

单文件改动 `packages/runtime/src/connection/testing/fake_resource/ops.rs`（742 行，800 行上限，余量 ~58 行）：

1. 把 `ops.rs:538-582` 的「回滚注销」与「取归池前置」两段合并进**同一个 `self.lock()` 临界区**，
   次序固定为「取注销前快照 → 注销句柄 → 判归池」。
2. **句柄注销改为无条件**（这是 CM-74 的关闭语义）；`slot.transaction_state = None` **仍以
   `rolled_back` 为门**（那是回滚语义，不是关闭语义）。
3. 归池判定改读 `handles_before_close`（注销前快照），不再是注销后的 `registered_handles()`。
   `ReturnedToPool { registered_handles }` 也填这个实数，不再写死 `0`。
4. F11 `CloseUnconfirmed` 提前返回、`Closed` / `permit -1` 无条件记录，全部原样保留。
5. `reclaim_registered_handles_on_close` 保留（对 journal 侧孤儿仍是兜底），
   但其 doc 「正常路径走不到这里」需要按新事实改写。
6. 改写 `harness/tests.rs:539-545` 的**理由**注释：它记录的是已被用户驳回的建议
   （「驱动直连路径故意不同 / 不应该在那条路径上断言 CM-74」）。**断言不动**，只换理由。
7. 新增两条测试（放 `harness/tests.rs`，676 行；`fake_resource/tests.rs` 已 790 行，只剩 10 行余量）：
   - 驱动直连路径的 CM-74 次序钉子（`handle closed → resource Closed → permit -1`）；
   - F10 不得归池的定点钉子（挂句柄的资源**必须** `Closed`、`ReturnedToPool` 出现 0 次）。

## 实施结果（已按裁定完成）

改动落在三个文件，`ops.rs` 的 `close_resource` 仍是同步 `&self`，全部 slot 变更在**一把锁**内：

| 文件 | 改前 | 改后 | 上限 800 |
|---|---|---|---|
| `fake_resource/ops.rs` | 742 | **750** | ✅ 余 50 |
| `harness/tests.rs` | 676 | **800** | ✅ 正好触线（见下遗留项） |
| `journal/core.rs` | 518 | **523** | ✅ 余 277 |

1. 「取注销前快照 → 注销句柄 → 判归池」合并进同一个 `self.lock()` 临界区。
2. 句柄注销**无条件**（关闭语义）；`slot.transaction_state = None` 仍以 `rolled_back` 为门（回滚语义）。
3. 归池判定读 `handles_before_close`（注销**前**快照）；`ReturnedToPool { registered_handles }` 填该实数，不再写死 `0`。
4. F11 `CloseUnconfirmed` 提前返回（`ops.rs:512-536`）一字未动，仍不归还 permit。
5. `reclaim_registered_handles_on_close` 函数体未动，只按新事实改写 doc：正常路径走不到那里，它只兜 journal 侧残留，
   因此**不会掩盖宿主自己的句柄泄漏**——那条由 `leak_invariant_violations()` 在未关闭时就报出来。

### 新增断言与钉子对应关系

| 断言 | 钉它的测试 |
|---|---|
| 两条关闭路径产出**同一条** CM-74 次序 | `harness/tests.rs::the_driver_direct_close_releases_in_the_same_cm74_order`（新增，直连）<br>`harness/tests.rs::closing_a_resource_that_still_holds_a_handle_releases_in_the_cm74_order`（既有宿主路径，**函数名与 `vec![...]` 一字未动**） |
| §4.2 F10：挂句柄的资源**必须关闭而非归池** | `harness/tests.rs::a_resource_still_holding_a_handle_is_never_returned_to_the_pool`（新增，计 `ReturnedToPool` 出现**次数**）<br>`fake_resource/tests.rs::f10_a_clean_reset_does_not_license_returning_the_resource_to_the_pool`（既有注入版） |

### 变异验证（证明断言不是走过场）

| 变异 | 预期 | 实测 |
|---|---|---|
| `ops.rs` 回到基线 | 直连次序用例失败 | **FAILED**，实测 `["resource Closed","permit -1","handle closed"]` vs 期望 `["handle closed","resource Closed","permit -1"]` |
| 快照改到 `slot.handles.clear()` **之后**读（天真修法） | F10 两条都失败 | 新增用例 **FAILED**（`left: 1, right: 0`）；既有注入版 **FAILED**；`cm73_threads` 竞态用例 **FAILED** |
| 判据改读 `resource.registered_handles()`（`resolve()` 的前置快照） | 语义等价，不应失败 | 全部 ok —— **这一版不是真变异**，`resource` 是锁前克隆，值本就等于注销前快照 |

## 门禁实测

`CARGO_TARGET_DIR=/tmp/dz-target-p3-cm74-release-order`；每次运行首尾各记一次 HEAD 与工作区 sha。

五条守卫逐条单独跑（`--lib`），逐字结论行：

| 守卫 | 测试 | 结论行 |
|---|---|---|
| F10 | `f10_a_clean_reset_does_not_license_returning_the_resource_to_the_pool` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 383 filtered out; finished in 0.01s` |
| 竞态不得归池 | `a_real_eviction_thread_meets_a_real_thread_holding_a_transaction` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 383 filtered out; finished in 0.00s` |
| 精确序列 + 空闲归池 | `an_idle_resource_with_no_transaction_still_returns_to_the_pool` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 383 filtered out; finished in 0.00s` |
| F11 | `close_unconfirmed_keeps_the_permit_occupied_forever` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 383 filtered out; finished in 0.00s` |
| F11 注入接线 | `a_fault_only_fires_on_its_own_operation` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 383 filtered out; finished in 0.00s` |
| F12 orphan | `f12_a_handle_the_runtime_refuses_to_return_never_reaches_the_host_registry` | `test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 383 filtered out; finished in 0.00s` |

CM-74 三条：`closing_..._cm74_order` ok、`the_driver_direct_close_releases_in_the_same_cm74_order` ok、
`a_resource_still_holding_a_handle_is_never_returned_to_the_pool` ok。

全量 `cargo test -p datazen-runtime` → `EXIT=0`，**20 个二进制**（18 集成 + `src/lib.rs` + doc-tests），
`TOTAL=564 passed; 0 failed`。逐二进制结论行见下：

```
 1. unittests src/lib.rs ........ test result: ok. 384 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
 2. tests/budget_cm65_lifecycle   test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
 3. tests/budget_cm65_reservation test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
 4. tests/budget_cm65_scheduling  test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
 5. tests/budget_cm66             test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
 6. tests/directory_attachment_ttl test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
 7. tests/directory_no_disk       test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
 8. tests/directory_ownership     test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
 9. tests/directory_replacement   test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
10. tests/gateway_contract        test result: ok. 51 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
11. tests/p3_session_port_contract test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
12. tests/registry_audit          test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
13. tests/registry_cancel         test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
14. tests/registry_execution      test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
15. tests/registry_lifecycle      test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
16. tests/registry_release        test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
17. tests/resource_replacement    test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
18. tests/resource_return_to_pool test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
19. tests/resource_rotation_and_disable test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
20. Doc-tests datazen_runtime     test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

`cargo test -p datazen-runtime --lib` 复跑 → `test result: ok. 384 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s`，`EXIT=0`。

`cargo fmt -p datazen-runtime -- --check` → `FMT_CHECK=0`。

### `--lib` 实际运行次数：**17 次**

1 次 `--no-run` 编译检查 + 16 次真正执行。16 次 = 6 条守卫 + 3 条 CM-74 + 7 次变异验证
（基线变异 2 次、天真修法变异 3 次、非真变异 2 次）+ 收尾全量 `--lib` 复跑 1 次。
另有 1 次全量 `cargo test -p datazen-runtime`（内含 `--lib` 目标）。

### 与基线的差

基线复验日志 `/tmp/dz-cm74-baseline-all.log`：**20 个二进制，TOTAL=562**，`--lib` 382。
现为 **TOTAL=564**，`--lib` 384。差 **+2**，恰是两条新测试；其余 19 个二进制逐项一致，
证明本轨**没有删改或增删任何其他测试**。

## 遗留与待裁定

1. **`harness/tests.rs` 已到 800 行触线**（上限硬）。下次任何追加都会破线，建议下一个动这条共享
   测试基础设施的轨道开 `harness/tests_cm74.rs` 之类的子模块并 `mod` 出去。
2. **`fake_resource/tests.rs` 790 行，只剩 10 行余量**（本轨**未**触碰）。它是共享基础设施，
   下一个要往里加用例的轨道会立刻撞线——需要一次按职责拆分的清理，建议单开一轨。
3. **F11 provider 侧无直测**：`ops.rs:512-536` 的 `CloseUnconfirmed` 提前返回没有任何用例直接注入
   `FaultKind::CloseUnconfirmed` 打到 `close_resource`（全仓只有 `script.rs` 的接线自测和
   `journal/tests.rs` 的账目层用例）。该缺口是既有的、非本轨引入；补测需动 790 行的
   `fake_resource/tests.rs` 或另起文件，故并入第 2 项同批处理。
4. **`connection-management.md` §7.4 第 6 项**（`setSessionContext` 的 `requiresReplacement`
   候选资源 + 原子发布协议）**不在本轨**，需另开轨。此处记录以免遗漏。

## 提交点

| commit | 内容 |
|---|---|
| `712b43012` | 三处代码改动（`ops.rs` / `harness/tests.rs` / `journal/core.rs`） |
| `HEAD`（本台账提交） | 本台账，合并时按路径删除。**自指，其 SHA 每次 amend 都会变，故此处不写死**；权威提交点是 `712b43012`。 |

Tester 请在 `712b43012` 上用独立 `git worktree add --detach` 树做变异验证 ——
未提交状态没有可靠指纹，工作区内容不可作为证据。

### 提交后复跑（工作区干净态）

```
HEAD_BEFORE=<台账提交，SHA 自指随 amend 变动；代码提交为 712b43012>
SHA_BEFORE=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855   # 空 porcelain 的 sha，即工作区干净
cargo test -p datazen-runtime      -> FULL_EXIT=0, BINARIES=20, TOTAL=564, FAILED=0
cargo test -p datazen-runtime --lib -> LIB_EXIT=0
   test result: ok. 384 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
HEAD_AFTER=<同 HEAD_BEFORE：运行期间 HEAD 未变>
SHA_AFTER=e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
PORCELAIN_AFTER=0
```

CM-74 三条在提交后复跑仍全 ok。

## 状态

- [x] 复验基线（20 个二进制 / 562 全绿）
- [x] 独立评估协调者三个问题
- [x] 计划成文并获裁定
- [x] 实现（三个文件）
- [x] 六条守卫逐条单独复跑 + 逐字结论行
- [x] 变异验证（两条新断言均非走过场）
- [x] 门禁复跑：20 个二进制 / 564 全绿，`fmt --check` 干净
