# p3-cancel-cleanup 进度台账

> 轨道：P3 合并后的清理（gateway + registry 两条子轨的收尾）。
> 分支：`feature/p3-cancel-cleanup`，起点 `25771de51b25b43c5cd45e38818e0e21e3e38b31`。
> 工作树：`.worktrees/datazen-p3-cancel-cleanup`。
>
> **本文件是台账，不是交付物。验收合并时必须删除，不得存活到 `main`。**
> 三条缺陷的结论都已落到代码、注释与测试里，删掉本文件后结论仍可直接从代码读出。

## 一、三项任务的完成状态

| 任务 | 缺陷 | 状态 | 落点 |
| --- | --- | --- | --- |
| Task 1 | D-03：网关侧重复定义 `CancelDisposition` | ✅ 已完成 | `gateway/cancel.rs`、`gateway/mod.rs` 及两个测试模块 |
| Task 2 | D-R2-1：`registry/port.rs` 文档与钉子夸大了覆盖面 | ✅ 已完成 | `registry/port.rs` |
| Task 3 | D-R2-2：`invalidate_worker` 静默丢弃 `control()` 的结果 | ✅ 已完成 | `registry/registry.rs` + 新增 `registry/registry/tests.rs` |

### Task 1 — D-03：处置三态只有一个定义处

- 删除 `gateway/cancel.rs` 里那份重复的 `CancelDisposition` 枚举与 `impl`。
- 改为 `pub use crate::connection::port::CancelDisposition;`（`pub` 而非 `use`：`pub use` 私有导入是编译错误，而 `gateway::CancelDisposition` 这条公开路径必须留着）。
- 权威定义保留在冻结的 `connection/port.rs`（`:245-262`），**未改一个字节**。
- `is_requested()` 原来挂在重复枚举上；权威枚举**没有**这个方法，于是它变成网关侧的自由函数
  `pub fn is_requested(disposition: CancelDisposition) -> bool`——它表达的是网关对调用方的
  承诺边界（「`unsupported` 绝不能被读成已下发取消」），不属于冻结面的契约，所以既不写进
  `connection/port`，也不作为方法挂在那个类型上。
- 线上的字面量一个字都没变：权威 `as_str` 是 `pub const fn`，返回同样的
  `"requested" | "unsupported" | "alreadyFinished"`，且同样 derive 了
  `Serialize`/`Deserialize`，JSON 不变。
- 模块文档里那段失效的「网关侧类型一律以 `Gateway`/`Cancel` 前缀命名」理由被替换成
  `## D-03：处置三态只有一个定义处`，写明**为什么**前缀护栏已经不成立。
- `gateway::cancel::CancelOutcome` 保持 4 个字段（`execution_id` / `disposition` /
  `state` / `observed_at_nanos`），没有改成 `registry::CancelReceipt`。

### Task 2 — D-R2-1：钉子的覆盖面按实测重写

- **实测（本轨开工前测得）**：只动钉子、不动别的 → `cargo build -p datazen-runtime`
  **EXIT=0**，结论行 `Finished dev profile in 2.50s`；`cargo test -p datazen-runtime --lib`
  **EXIT=101**，报 E0599 + E0308。也就是说「形状由编译器保证」这句话**只对 test profile 成立**，
  而对外发行的二进制来自 `cargo build`。
- 处理：`frozen_port_cancel_shape` 移出 `#[cfg(test)]`，加 `#[allow(dead_code)]`，
  与同文件里既有的 `assert_object_safe` 同一套做法。代价为零：它没有调用点，链接器不会把它拉进去。
- 效果：现在 `cargo build` 与 `cargo test` 都会拦住它。文档写明这一点，并写明跨轨的最终裁判仍然是
  冻结的 `p3_session_port_contract`（那个只在 test 下编译）。
- 新增钉子测试 `the_cancel_shape_pin_holds_for_the_concrete_port_implementation`：钉子必须对
  **具体实现**和 `Arc<dyn SessionPort>` 两种持有形式都成立，且两条路径返回同一结果。

### Task 3 — D-R2-2：`control()` 的结果不再被静默丢弃

见下面第四节「可达性裁定」。

## 二、可达性裁定（Task 3 的结论）

`SessionRegistry::invalidate_worker` 现在对 `control()` 的三种答复分三种动作：

| `control()` 返回 | 含义 | 摘表项？ | 计入 `lost`？ |
| --- | --- | --- | --- |
| `Ok(true)` | 送达，且确属该 worker，会话已在 actor 里回滚关闭 | 是 | 是 |
| `Ok(false)` | **同样送达了**，答复是「不归这个 worker 管 / 已无物理资源」 | 是 | 是 |
| `Err(_)` | **没送达** | 否 | 否 |

**`Err` 在正常退出路径上不可达，理由是结构性的：**

1. `SessionRecord` **按值**持有 `actor: SessionActor`；`SessionActor` 自己持有
   `UnboundedSender<ExecCommand>` / `UnboundedSender<ControlCommand>`。
2. `run_actor` 的循环条件是 `while exec_open || state.in_flight.is_some()`，
   `exec_open` 只有在 `exec_rx.recv()` 读到 `None` 时才置否（`actor.rs:343`、`:368`），
   而 `None` 要求**所有** `ExecCommand` 发送端都被 drop。
3. `owned_by` 是**按值克隆**出 `SessionRecord` 的。克隆里的 `actor` 在 `.await` 期间
   就是一个活着的发送端——**与表里还有没有那一份无关**。
4. 因此摘表项只会多丢一份克隆（表里那份），丢不掉发送端 ⇒ **摘行不可能造成 `Err`**。

能造出 `Err` 的只剩 actor 任务异常终止（panic / abort 把 `exec_rx` 一起丢掉）。类型系统挡不住
这件事，**注释也挡不住**——所以修法是把「正常路径不可能」写成**分支**而不是注释：
`Err` 时保留表项、`warn!` 留下可查的痕迹、不计入 `lost`。

**旧代码的实际代价**（不是理论问题）：`let _ = ...` 之后无条件摘行，等于把「没送达」也记成
「已作废」。控制通道已经断了、物理资源还在别处活着，登记表却已经把这条记成作废，
额度随后被 `quota.hold_stale` 挂住等人来隔离确认——而一个已经没有控制通道的会话不会被隔离，
那份额度就**永久**挂住。

**可断言的不变量只有正向这一半**：只要还有一个 `SessionActor` 克隆活着（哪怕表项已经摘掉），
`is_closed()` 就是 `false`。反向（最后一个克隆被 drop 后 actor 是否收尾）**在内部结构上不可观测**，
因为调用 `is_closed()` 本身就需要握着一个发送端。所以钉子只钉正向那一半，不假装能钉反向。

**结构钉子**（`registry/registry/tests.rs`）：

- `row_owns_the_sender(&SessionRecord) -> &SessionActor`：能编译就说明 `actor` 字段确实按值持有
  发送端（不是 `Option`、不是弱句柄）——这是可达性论证「形状」的那一半。
- `摘表项不会让控制投递变成未送达_因为表项自己就是发送端`：登记 → 记下行 → 确认 actor 活着 →
  `invalidate_worker` 照常摘行并计入 `lost` → **摘行之后**再确认那份克隆仍是活着的发送端。
- `否定答复仍算送达_不归该worker管的会话照常摘行`：钉住三态表里的第二格，防止有人日后把
  `Ok(false)` 并进失败分支。

## 三、门禁实测

环境：`export CARGO_TARGET_DIR=/tmp/dz-target-p3-cancel-cleanup`；工作树
`.worktrees/datazen-p3-cancel-cleanup`。长输出落 `/tmp/p3cc-*.log`，退出码单独打印。

### 门禁 1 — 冻结基线 12 套件

命令：`cargo test -p datazen-runtime --test <name>`，逐个执行。

| 套件 | EXIT | 逐字结论行 |
| --- | --- | --- |
| `budget_cm65_lifecycle` | 0 | `test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `budget_cm65_reservation` | 0 | `test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `budget_cm65_scheduling` | 0 | `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `budget_cm66` | 0 | `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `directory_attachment_ttl` | 0 | `test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `directory_no_disk` | 0 | `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` |
| `directory_ownership` | 0 | `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `directory_replacement` | 0 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `p3_session_port_contract` | 0 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `resource_replacement` | 0 | `test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `resource_return_to_pool` | 0 | `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `resource_rotation_and_disable` | 0 | `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |

`p3_session_port_contract` = **10**，与预期一致。

### 门禁 2 — lib 单测

命令：`cargo test -p datazen-runtime --lib`，EXIT=**0**

```
test result: ok. 380 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
```

基线 377 + 本轨新增 3 条（1 条 D-R2-1 钉子测试 + 2 条 D-R2-2 结构测试）= **380**，≥377。

### 门禁 3 — 轨道相关套件

| 套件 | EXIT | 逐字结论行 |
| --- | --- | --- |
| `gateway_contract` | 0 | `test result: ok. 51 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s` |
| `registry_cancel` | 0 | `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_release` | 0 | `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_audit` | 0 | `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_lifecycle` | 0 | `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_execution` | 0 | `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |

全部等于预期值（51 / 9 / 11 / 11 / 7 / 6），无一被下调。

### 门禁 4 — 构建

命令：`cargo build -p datazen-runtime`，EXIT=**0**

```
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.97s
```

`grep -cE '^warning'` = **0**。另做过一次 `cargo clean -p datazen-runtime` 后的强制重编
（`cargo test --lib --no-run`），warning 计数同样为 **0**。

### 门禁 5 — 格式

命令：`cargo fmt --check -p datazen-runtime`，EXIT=**0**，输出 0 行。

### 门禁 6 — crate 边界

命令：`node scripts/check-platform-crate-boundaries.mjs`，EXIT=**0**

```
[check-platform-arch] PASS — 22 workspace member(s) classified, 6 rule(s) evaluated over 26 crate(s), 1 rule×subject combo(s) vacuous: 0 violation(s), 0 error(s), 3 advisory(ies)
```

### 门禁 7 — HEAD 与工作区行数（跑门禁前后各一次）

| | `git rev-parse HEAD` | `git status --porcelain -uall \| wc -l` |
| --- | --- | --- |
| 跑门禁前 | `25771de51b25b43c5cd45e38818e0e21e3e38b31` | 7 |
| 跑门禁后 | `25771de51b25b43c5cd45e38818e0e21e3e38b31` | 7 |

HEAD 未变、工作区行数未变 ⇒ 跑门禁期间没有人动过这棵工作树。

## 四、冻结面未被触碰

`git status --porcelain -uall -- packages/runtime/src/connection packages/runtime/tests` 输出为空。
`packages/runtime/src/connection/**` 与 `packages/runtime/tests/p3_session_port_contract.rs`
一个字节都没改，也没有任何断言被放宽或数量被下调。

改动过的文件（全部在 `packages/runtime/src/` 内）：

| 文件 | 行数 |
| --- | --- |
| `gateway/cancel.rs` | 438 |
| `gateway/mod.rs` | 537 |
| `gateway/facade_support.rs` | 400 |
| `gateway/cancel_event_tests.rs` | 528 |
| `registry/port.rs` | 683 |
| `registry/registry.rs` | 727 |
| `registry/registry/tests.rs`（新增） | 217 |

`registry/registry.rs` 已贴到 727 行，所以 Task 3 的用例按 `registry/actor.rs` →
`registry/actor/tests.rs` 的既有分工放进独立子模块文件，而不是继续往里塞。

## 五、每处修复对应的钉子测试

| 修复 | 钉子测试 |
| --- | --- |
| D-03 | `gateway::cancel::tests::disposition_literals_match_the_architecture_map`、`the_port_mapping_can_never_produce_unsupported`、`a_live_state_maps_to_requested_and_a_terminal_one_to_already_finished`；`gateway::cancel_event_tests::a_driver_without_precise_cancel_is_unsupported_and_never_touches_the_port`、`a_live_execution_reports_requested_without_claiming_a_terminal_state`、`an_execution_already_in_a_terminal_state_reports_already_finished` |
| D-R2-1 | `registry::port::tests::the_cancel_shape_pin_holds_for_the_concrete_port_implementation`（新增） |
| D-R2-2 | `registry::registry::tests::送达即作废_摘行并把额度挂成stale`（第一格）、`registry::registry::tests::否定答复仍算送达_已无物理资源的会话照常摘行`（第二格）、`registry::registry::tests::投递失败保留表项_额度不挂stale等重投`（第三格）、`registry::registry::tests::摘行丢不掉活着的发送端`（结构前提） |

既有行为不回归：`registry_release:403-465`（租约失效立即作废、额度扣住不外发、隔离确认后才归还）
与 `registry_audit:498-529`（额度只由登记表账本回答）两条既有集成用例原样通过，`lost`、
stale、隔离确认计数均未变。

## 六、遗留与待裁定项（第二轮复审后更新）

- **~~`Ok(false)` 的可达性没被证伪~~（已解决，见第七节）**：第一轮我判断「近乎不可达」，
  判断是**错的**。`evict_idle_at` 在 `SessionLost` 上跳过摘行，只留下「物理资源已清空、
  行还在表里」的记录——这正是第二格的入口。`registry.rs` 的注释与第二格用例都已改成这一事实。
- **反向不变量（最后一个克隆 drop 后 actor 是否收尾）结构上不可断言**，钉子只钉正向。
  这不是遗漏，是 tokio 无界通道 + 「调用 `is_closed()` 需要握着发送端」共同决定的。
- 本轨**没有做变异实验**（按分工不做）；第一轮 D-R2-2 的用例就是这么漏下了 M-D/M-E/M-F，
  验证方在独立工作树注入三个变异后 20 套件 0 失败。第三节用例按「每一格各杀一个变异」重写，
  但**钉子本身仍未被变异验证**，如需更强的证据应由验证方再补一次。
- `progress.md` 必须在验收合并时删除，不得存活到 `main`。

## 七、第二轮复审（Tester 判 FAIL：D-R2-2 零回归保护无效）

### 7.1 D1（medium，合并阻塞）三个变异全部存活，根因与改法

上一版用例只测了「表项还在不在」这一栏，而且第二格是**绕过 `invalidate_worker` 直接问
actor** 造出来的，测的其实是 actor 而不是登记表。于是三种把三态并成两态的改法全都活着：

| ID | 变异 | 上一版为何没杀掉 | 本轮由谁杀 |
| --- | --- | --- | --- |
| M-D | `if !delivered { continue; }` 同时 `lost.push(..)` | 没有任何用例断言「没送达就不进 `lost`」 | 第三格 `{counted_in_lost: false}` |
| M-E | 区分 `Ok(true)/Ok(false)`，把 `Ok(false)` 塞进失败分支 | 第二格没走 `invalidate_worker`，分类表根本没参与 | 第二格整条 `Classification` |
| M-F | 整块回滚成 `let _ = ..` + 无条件摘行 | 上一版没有 `Err` 格用例，而 M-F 在 `Ok` 两格上的行为与修好后**完全一致** | 第三格 `{row_kept: true, quota_held_stale: false}` |

改法是三格各一条用例，且每格用**一条 `assert_eq!` 打整个 `Classification` 结构**
（表项 / `lost` / stale 额度 / stale 审计四栏一起断言）——这样任一栏坏掉时，失败信息直接
点名坏的是哪一栏，而不是「行还在」这种单栏信号。

### 7.2 D2（medium）用例名与断言相反

`否定答复仍算送达_不归该worker管的会话照常摘行` 的 187-195 行断言行**没被摘**（与名字相反）：
`owned_by` 已按 `record.worker_id` 筛过，别 worker 的行根本不会走到 `control()`；
202-207 行则是直接调 `actor.control(..)` 造 `Ok(false)`，绕过了被测函数。
已删除该用例，改为真正走 `invalidate_worker` 的第二格（见 7.4）。

### 7.3 D3（low）钉子注释吹过了头

`row_owns_the_sender` 只能钉住「`SessionRecord` 有一个名为 `actor`、类型为 `SessionActor`、
按值存放的字段」；`SessionActor` 内部的发送端字段对 `registry::actor` 私有，本模块（测试住在
`registry::registry::tests`）按 Rust 可见性**够不着**。注释已改成如实陈述，并把
「表项 ⇒ 活着的发送端」这份功劳归给行为断言 `摘行丢不掉活着的发送端`。

### 7.4 三格的构造（均不改生产代码，已实测）

| `control()` | 构造 | 实测 |
| --- | --- | --- |
| `Ok(true)` | 登记后直接 `invalidate_worker`（`ConfirmedCloseBackend::close` ⇒ `Closed`） | 绿 |
| `Ok(false)` | `UndecidableCloseBackend`（`close` ⇒ `Undecidable`）：先 `evict_idle_at(IDLE_DEADLINE_MS)`，`release` 写过 `physical = None` 才返回 `SessionLost`，`evict_idle_at` 收不到成功视图就跳过摘行；再 `invalidate_worker`，actor 判 `physical.is_none()` ⇒ `Ok(false)` | 绿 |
| `Err(_)` | `CrashingCloseBackend`（`close` ⇒ `panic!`）：§9.4 在 actor 任务**内部**直接 `await backend.close(..)`，回调一崩就穿过 `release` → `handle_control` → `run_actor`，在 `reply.send(..)` 之前丢掉回执，`control()` 拿到 `Err`。tokio 捕获任务 panic，`spawn_actor` 又丢弃了 `JoinHandle`，所以测试进程不受影响 | 绿 |

第三格额外做了一次**重投**：第二次 `invalidate_worker` 返回空且行仍在（若 actor 还活着，
它会回 `Ok(true)` 把行摘掉），随后才断言 `is_closed()`——`ctrl_rx` 已丢 ⇒ 同一个 future 里的
`exec_rx` 必然一起丢，所以这一步不依赖任何调度时序，是无竞态的。
**没有**用 `std::panic::set_hook` 去消音（进程级遮蔽）。

### 7.5 本轮门禁（首尾 HEAD 一致，未被验证方触碰）

```
HEAD=e1cf457677942e0dc667b7e34a4e1de1d8260647（跑门禁前）→ 同上（跑门禁后）
cargo fmt --check -p datazen-runtime                          EXIT=0，输出 0 行
cargo build -p datazen-runtime                                EXIT=0，warning 计数 0
cargo test -p datazen-runtime --lib   （第 1 次）             EXIT=0，382 passed
cargo test -p datazen-runtime --lib   （第 2 次）             EXIT=0，382 passed
node scripts/check-platform-crate-boundaries.mjs              EXIT=0，0 violation(s), 0 error(s), 3 advisory(ies)
```

18 个冻结套件逐个跑，`EXIT=0`，计数与第一轮逐字一致：
`p3_session_port_contract`=10、`gateway_contract`=51、`registry_{cancel,release,audit,lifecycle,execution}`=9/11/11/7/6、
`budget_cm65_{lifecycle,reservation,scheduling}`=8/2/6、`budget_cm66`=7、
`directory_{attachment_ttl,no_disk,ownership,replacement}`=12/6/7/10、
`resource_{replacement,return_to_pool,rotation_and_disable}`=4/7/6。

冻结面 `git diff 25771de51..HEAD -- packages/runtime/src/connection packages/runtime/tests | wc -c` = **0**。
本轮只改两个文件：`registry/registry.rs`（**仅注释**，733 行）、`registry/registry/tests.rs`（347 行）。
