# 进度台账 — 轨 `feature/p3-reset-verdict`

基准 `feat/p2-p3-p4-completion` @ `f5a33b7dc`。工作树
`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/datazen-p3-reset-verdict`。
`CARGO_TARGET_DIR=/tmp/cargo-p3-reset-verdict`。
提交 `25c15d205`（code）—— 本文件随后同分支提交。

## 目标（一句话）

宿主侧物理端口把驱动的复位判决压扁成 `Result<(), ResourceError>`，`Clean` 与 `Discard`
都落进 `Ok(())`；`manager.rs` 归池分支因此把驱动明说「不能复用」的资源塞回空闲池。
本轨让判决原样传到归池决策，并按三态（Clean→归池 / Discard→关闭 / Err→隔离）处理。

## 状态

**完成**（缺陷已修、门禁全绿、失败证据已实测、已提交）。

## 改动清单（文件:行 → 改了什么）

| 文件 | 行 | 改动 |
| --- | --- | --- |
| `packages/runtime/src/resource/cleanup.rs` | 230-239 | 新增 `ReturnReason::DriverDiscardedReset`（阻断项码 `driverDiscardedReset`） |
| " | 252 | `reason_code()` 增补该格 |
| " | 256-263 | 新增 `ReturnReason::is_fault()`：只把「预期内的正常答复」排除出故障 |
| " | 515-565 | 新增 `pub enum ResetDisposition { Clean, Discard }` + `admits_reuse()` + `disposition_code()` |
| " | 577-591 | `PhysicalTransport::reset` 签名改为 `Result<ResetDisposition, ResourceError>`，文档写全三态 |
| `packages/runtime/src/resource/manager.rs` | 22 | use 列表引入 `ResetDisposition` |
| " | 358-386 | 新增 `refuse_park_after_discarded_reset()`：归池遇 `Discard` ⇒ 关闭，`info` 记账 |
| " | 410-427 | 关闭分支三态语义的判定依据（§9.4 / §6.5 / §7.5 第 5 步） |
| " | 430-451 | 关闭分支（`needs_reset`）的 `Discard` 分支：不改判、不谎称已完成 |
| " | 468-474 | 归池分支「判决必须原样传下去」的判据注释 |
| " | 479-487 | 归池分支三态分派 |
| `packages/runtime/src/resource/mod.rs` | 84 | 公开面再导出 `ResetDisposition` |
| `packages/runtime/src/resource/harness.rs` | 25 | use 引入 `ResetDisposition` |
| " | 90-98 | `Fault` 新增 `ResetDiscard` 一格（与 `Reset` 即 `Err` 分开） |
| " | 189-210 | `RecordingTransport::reset` 三态：`ResetDiscard` 事件照常入账并回 `Discard` |
| `packages/runtime/src/resource/journey.rs` | 423-525 | 新旅程 `a_discarded_reset_refuses_the_idle_park_and_closes_the_resource`（7 组断言） |
| " | 527-581 | 新旅程 `a_discarded_reset_during_a_close_does_not_block_the_close_but_keeps_the_handles_visible`（4 组断言） |
| `packages/runtime/tests/resource_return_to_pool.rs` | 20-22 | use 列表补 `CleanupDisposition` + `ResetDisposition` |
| " | 62-66, 85-91 | `Journal` 新增 `reset_discards` + `discard_reset()` 注入 |
| " | 131-144 | `reset` 替身三态 |
| " | 534-625 | 新集成测试 `a_discarded_reset_never_parks_the_resource_in_the_idle_pool`（7 组断言） |
| `packages/runtime/tests/resource_support/mod.rs` | 90-94, 128-134, 205-222 | 共享替身同步：`reset_discarded` + `discard_reset()` + 三态 `reset` |
| `packages/runtime/tests/cm28_concurrent_release.rs` | 124-126, 290-297 | `BudgetPort::reset` 改签名，一律报 `Clean`（该文件数的是关闭） |
| `packages/runtime/tests/resource_replacement.rs` | 18-20, 88-92 | `ScriptedTransport::reset` 改签名，报 `Clean` |
| `packages/runtime/tests/resource_rotation_and_disable.rs` | 18-20, 109-112 | 同上 |
| `packages/runtime/tests/tunnel_arch_support/mod.rs` | 26-28, 152-158 | 同上（`Err` 仍是故障注入） |

7 处 `impl PhysicalTransport` 全部改到（`grep -rn "impl PhysicalTransport"` 核实，
仓库内无第 8 处；`tunnel/.../port_audit.rs:134` 只是注释提到）。

## 门禁（全部本轨实测，未采信任何他人数字）

| 门禁 | 命令 | 退出码 | 结论行（逐字） |
| --- | --- | --- | --- |
| A | `cargo fmt --all -- --check` | 0 | 无任何输出（0 行） |
| B | `cargo check --workspace --all-targets` | 0 | `Finished dev profile [unoptimized + debuginfo] target(s) in 9.21s`；`grep -cE "^error(\[\|:)"` = **0** |
| C | `cargo test -p datazen-runtime --no-fail-fast` | 0 | 43 条 `test result:` 全 `ok`；合计 **1004 passed / 0 failed / 0 ignored** |
| D | `cargo test -p datazen --lib` | 0 | `test result: ok. 1810 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 10.38s` |
| E | 新测试改动前失败 | 101 | 见下节，**三条独立证据** |
| F | `git status --porcelain` | — | 提交后仅 `?? progress.md`（本文件），无其他残留 |

首尾各记录一次 HEAD / 工作区 sha，证明运行期间没人动过：
起始 `HEAD=f5a33b7dc`、`git diff` sha `43b35a44…`、`git status --porcelain` sha `d01217a3…`；
提交后 `HEAD=25c15d205`、`git diff HEAD` 为空。

### 门禁 E：改动前失败证据（三条）

**证据 1（弱，编译失败）**：独立 detached worktree `/tmp/p3-reset-verdict-probe` @ `f5a33b7dc`
（与另外两个临时 worktree `/tmp/dz-*` 并存，均在 `/tmp`，未碰本工作树）。
把新测试逐字拷入基线，`cargo test -p datazen-runtime --test resource_return_to_pool`：

```
error[E0433]: failed to resolve: use of undeclared type `CleanupDisposition`
   --> packages/runtime/tests/resource_return_to_pool.rs:564:36
error[E0599]: no method named `discard_reset` found for struct `Arc<ScriptedTransport>`
   --> packages/runtime/tests/resource_return_to_pool.rs:541:15
error: could not compile `datazen-runtime` (test "resource_return_to_pool") due to 2 previous errors
```
EXIT=101。基线上 `ResetDisposition` 与 `CleanupDisposition` 都不存在 ⇒ 测试编译不过。
这只是「新符号确实缺失」，**不能**证明测试钉住了运行时行为。

**证据 2（强，运行期断言失败）**：同一棵基线树上搬入**已修的** `cleanup.rs` /
`manager.rs` / `mod.rs` / `harness.rs` / `journey.rs` / 测试文件，然后**只把三处
退回缺陷态**（端口签名、manager 两个分支、两处替身），即完整复现旧行为：

- `cleanup.rs:591` `fn reset(..) -> Result<(), ResourceError>`
- `manager.rs:479-486` 归池分支退回 `Ok(()) => session_reset_performed = true`
- `manager.rs:430-451` 关闭分支退回 `Ok(()) => session_reset_performed = true`
- `resource_return_to_pool.rs:131-144` 替身：`Discard` 分支退化成 `Ok(())`
- `harness.rs:189-210` 替身：`Fault::ResetDiscard` 退化成 `Ok(())`

`cargo test -p datazen-runtime --lib` ⇒ EXIT=101：

```
---- resource::journey::a_discarded_reset_refuses_the_idle_park_and_closes_the_resource ----
thread '...' panicked at packages/runtime/src/resource/journey.rs:455:5:
驱动明确回答 Discard ⇒ §7.3「只有 Clean 才归池，其余关闭」：绝不归池

---- resource::journey::a_discarded_reset_during_a_close_does_not_block_the_close_but_keeps_the_handles_visible ----
thread '...' panicked at packages/runtime/src/resource/journey.rs:561:5:
Discard 没能证明复位 ⇒ 不得记成已完成

test result: FAILED. 497 passed; 2 failed; 0 ignored
```

`cargo test -p datazen-runtime --test resource_return_to_pool` ⇒ EXIT=101：

```
---- a_discarded_reset_never_parks_the_resource_in_the_idle_pool ----
thread '...' panicked at packages/runtime/tests/resource_return_to_pool.rs:568:5:
驱动明确回答 Discard ⇒ 不得归池：池里不许出现驱动明说不能复用的资源

test result: FAILED. 9 passed; 1 failed; 0 ignored
```

**三条新测试在缺陷态下全部失败，且失败点正是「资源进了池 / 谎称复位已完成」。**
这就是缺陷被真修掉的证据：同一份测试，缺陷态红、修复态绿（门禁 C）。
该缺陷态探针跑完后已 `git worktree remove --force` 删除，未影响本工作树。

**证据 3（计数反证）**：基线 `f5a33b7dc` 上 `cargo test -p datazen-runtime --no-fail-fast`
实测 **1001 passed**（EXIT=0）；本轨 **1004**。差的 3 个正是本次新增的三个测试
（2 个 `journey.rs` 单测 + 1 个 `resource_return_to_pool.rs` 集成测试），
无其他数量变化 —— 没有悄悄增删别的用例。

### 门禁 F 佐证

提交后 `git status --porcelain` 仅一行 `?? progress.md`。其余 10 个改动文件已全部进
`25c15d205`，未留在工作区。并行轨的 worktree 我未触碰
（`datazen-p2-handle-closer` / `datazen-p3-reset-capability` 工作区干净，
`datazen-p3-runtime-wiring` 的 2 个改动文件属该轨自己的在途工作，我全程未进入）。

## 端口签名选型：为什么自己定义，不复用 `driver-api`

`packages/driver-api/src/session.rs:441-449` 的 `ResetDisposition`（`Clean`/`Discard`
二值）与 `connection/port.rs:220-235` 的 `ResetOutcome`/`ResetDiscardReason`（带 reason）
是**两份不同的类型**，我两个都没复用，在 `resource/cleanup.rs:538` 自定了同名枚举。理由按候选逐个说：

**不复用 `driver-api::ResetDisposition`—— 依赖方向合法但边界不合法。**
`packages/runtime/Cargo.toml:22` 已有 `datazen-driver-api` 依赖（`PROTOCOL_VERSION`
用于 `connection/capability.rs:137`，`DatabaseDriver` 用于 `application/live.rs:68`），
所以「runtime 依赖 driver-api」这点本身**不违规**，这点我核实过。否掉它的理由是模块边界：
`resource/mod.rs:48-49` 写明物理端口是**本模块自己定义的注入式接缝**，
`connection/port.rs` 只冻结 DTO 与 `BudgetPort`，**没有**资源操作端口。而
`driver-api` 那个枚举是**驱动协议的冻结面** —— 它的增删受 `PROTOCOL_VERSION` 与协议
成员闸门治理（`docs/architecture/platform/driver-capability-migration.md` §6）。
共用符号等于让驱动协议任何一次演进顺着类型别名渗进 `resource/**` 的公开面：
驱动侧某天加一个 `ResetDisposition::Degraded`，`resource` 的公开 API 就被迫跟着动，
而 `resource` 根本不关心那个新值。**结论进得来，判定依据进不来** —— 这与本模块
既有的 `DriverCleanVerdict`（`cleanup.rs:21-45`，结论是私有 bool、依据一律不带）
是同一条惯例，不是我新发明的。

**不复用 `connection::port::ResetOutcome`—— 它是会话面 DTO，且形状不对。**
`ResetOutcome` 带 `ResetDiscardReason { SessionStillExecuting, SessionStateAborted,
TemporaryObjectsPresent, HealthDegraded }`，四个 reason 是**判定依据**，正是上面那条
边界要求挡在门外的东西；宿主在此只做三态分派，不重算、也不搬运依据。另外
`connection/**` 是冻结的会话面，`resource/mod.rs:44-45` 写明本模块「只用不重定义」，
反向把会话面 DTO 拉进资源层端口签名，方向上虽不构成编译环，但会让两条本应独立的
演进线（会话面 DTO / 资源层裁决）绑死。`fake_resource/ops.rs:447` 那个 `ResetOutcome`
是**测试夹具内部**的消费，不构成生产代码的复用先例。

**自定的两个附带收益**：`admits_reuse()` 把「只有 Clean 可复用」变成可调用的谓词而不是
散落的 `matches!`；`disposition_code()` 给日志/指标一个不随文案漂移的稳定码
（`manager.rs:378` / `:436` 的 `reset_disposition = "discard"` 字段）。
`Debug+Clone+Copy+PartialEq+Eq` 派生与 driver-api 那份一致，测试替身可直接比较。

**待裁决（留给协调者）**：`packages/runtime` 已经依赖 `datazen-driver-api`，
所以「干脆共用符号、省一份平行类型」在**依赖图**上是说得通的。我选不共用是因为
模块边界与协议冻结面，不是因为有编译障碍。若集成时认为「两个同名枚举太容易混淆」，
替代方案是在 `resource` 内 `pub use` 一个**新名字**的别名而不是直接再导出同名符号
（现状是再导出同名 `ResetDisposition`，测试文件里 `use datazen_runtime::resource::…`
一眼能看出是 runtime 自己那份）。

## 关闭分支遇 `Discard` 的判断与依据

**结论：不改判。落点仍是 `Closed`，预算照常核销，但 `session_reset_performed` 保持
`false`，`outstanding_handles` 不清空。**

判断链条（三段，都写在 `manager.rs:410-427` 的注释里，代码只是它的执行）：

1. **这段复位的目的不是复用性，而是关闭前的收尾**（回滚旧事务、解除映射）。
   `needs_reset` 只在「还有未释放的会话级句柄」时为真，走到这条分支本身就意味着
   这条租约已经不可能回池了 —— 它要的是被关掉。
2. **关闭本来就不需要复用性。** §9.4 把「关闭」定为所有非 `Clean` 结果的落点；
   §7.5 第 5 步明写「任意用户 SQL session 首版直接关闭物理资源；**不复用未知会话状态**」。
   「不能证明已复位」在一条**反正要关**的路径上不构成额外阻断——资源照样关、预算照样释放。
3. **与 `Err` 那一格的区别是「不知道」与「知道不能复用」。** `Err` 改判成 `Quarantined`
   （握手从未确认 ⇒ 有待核验的义务，预算继续占用等运维核验，既有行为不变）；
   `Discard` 是确定答复，没有不确定性 ⇒ 没有隔离事由，改判成隔离反而会把
   「驱动按契约给的正常答复」升级成一次假的故障。

**但不许谎报。** `Discard` 下 `session_reset_performed` 必须是 `false`，
`outstanding_handles` 必须留着。§6.5 说「先在原 resource 上回滚/关闭句柄并从 actor
注销，确认后才释放资源」，而驱动没能证明那份回滚真发生过，
`CleanupReport::from_plan`（`cleanup.rs:439-447`）只在 `session_reset_performed` 为真时
清空句柄 —— 所以这里不置真，句柄就自然留在报表里。这是审计要的：
「回滚了」与「驱动机说它证明不了回滚」是两件事，报告不许替驱动写前者。
新旅程 `a_discarded_reset_during_a_close_does_not_block_the_close_but_keeps_the_handles_visible`
第 3、4 组断言钉的正是这两点。

## 取舍与待裁决项

1. **`Discard` 记 `info` 不记 `warn`。** 依据：首版 15 个 path 驱动（`ls packages/drivers`
   去掉 `http-support` 辅助 crate = 15）**无一**能在 `reset_resource` 给出 `Clean` ——
   7 个自己实现的全 `Ok(ResetDisposition::Discard)`（postgres/mysql/sqlite/redis/
   mongodb/hbase/vector，redis 在 `baseline` 非空时还直接 `Err`），
   8 个走 `LegacyResourceAdapter` 的默认体 `resource_adapter.rs:499-506`，注释写死
   "Always `Discard` … never `Clean`"。常态记 warn 会让真正的告警淹死。
   这条依据我写进了 `ReturnReason::is_fault()`（代码而非注释），并由
   `journey.rs:483` 断言钉住。
2. **`Fault::ResetDiscard` 与 `Fault::Reset` 分立两格**（`harness.rs:90-98`）。
   若合并，就再也测不出「宿主有没有区分不知道与知道不能复用」—— 这个测试能力本身
   比省一个枚举变体值钱。
3. **`ResetDisposition` 与 driver-api 那份同名**（见上一节待裁决）。
4. **未做能力分流。** `require_reset_for_reuse` 接进归池是 `feature/p3-reset-capability`
   的活（已在跑）。本轨的三态结构是它的前提：它拿到的已经是 `Ok(Clean)` / `Ok(Discard)` /
   `Err` 三格，不必再猜。我没有碰那一侧的任何文件。
5. **未改 `packages/drivers/**`、`packages/driver-api/**`、`packages/runtime/src/application/**`、
   `registry/**`、文档措辞、CI**（简报的明确不做项，全部遵守）。
6. **待合流**：集成基线已到 `e612cb0c1`（含轨 A 协议升级），本轨基于 `f5a33b7dc`，
   合并前需 rebase。预期冲突面：`packages/runtime/src/resource/cleanup.rs` 的端口签名
   与 `manager.rs` 归池分支 —— 若 `e612cb0c1` 也动过这两处，需人工核对三态分派没被盖掉。

## 遗留

无未完成项。下方复选框清单 11 项全部打勾，与代码状态一致。

## 核对清单（据实打勾）

- [x] 读完必读材料（AGENTS.md、`connection-management.md` §6.5/§7.3/§7.5/§9.3/§9.4、
      driver-api `session.rs:441-449`、`cleanup.rs`、`manager.rs` release）
- [x] 读 `progress.md` 全文并 `git diff` 逐项核实前任声称的完成项
      （11 项声称**全部属实**；`cleanup.rs:591` / `manager.rs:369` / `info` 级别 /
      关闭分支三态 / 5 组断言均核实无误；前任唯一未做的是门禁实测、失败证据、
      台账打勾与「后续待做」）
- [x] 前任越界补的 `CleanupDisposition` 导入已纳入我的代码责任重审：
      **必要**（新测试 `:579` 用 `CleanupDisposition::Closed` 做 `assert_eq!`）。
      同类遗漏已全查：`cargo check -p datazen-runtime --all-targets` EXIT=0，
      `grep -E "never used|unused"` 对本轨改动文件零命中。
- [x] 端口签名携带判决（自定 `ResetDisposition`，选型论证见上）
- [x] `manager.rs` 归池分支三态
- [x] `manager.rs` 关闭分支（`needs_reset`）遇 `Discard` 的判断
- [x] 7 处替身改签名（第 8 处不存在，已 grep 核实）
- [x] 新增三态区分测试 3 个 + 改动前失败证据（三条，含运行期断言失败）
- [x] 门禁 A–F 全部实测填入上表
- [x] `cargo test -p datazen-runtime` 1004 vs 基线 1001 的 3 个差额已说明来源
- [x] 已提交（`25c15d205`，未 push）。本文件随后单独提交。
