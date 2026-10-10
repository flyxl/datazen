# progress.md — P3 reset capability gate

分支 `feature/p3-reset-capability`，base `8dae098cc`。
本文件是并行开发期进度台账（AGENTS.md 允许，验收合并时删除）。

## 状态：全部验收标准已满足

| 项 | 状态 |
| --- | --- |
| progress.md 首提交 | ✅ `19e27eec8` |
| 依赖方向论证 | ✅ 见下 |
| 设计定稿 | ✅ |
| 实现 | ✅ |
| 门禁 A–G | ✅ 全部实测 |

## Commits

| hash | 内容 |
| --- | --- |
| `19e27eec8` | progress 台账 |
| `e743ea2f9` | 能力闸进归池准入（`decide` + `HostConditionSnapshot` + 新 `ReturnReason`） |
| `5e4a2246d` | 三个新测试 + 报障定序修正 |
| `b6539b911` | 空池可观测性（info 日志 + `admissions_declined_for_reset` 计数） |

## 依赖方向论证（任务 1）

**没有新增依赖，且使用的是 runtime 自己那一份 `ResetForReuse`。**

事实：
1. `packages/runtime` **已经**依赖 `datazen-driver-api`（`packages/runtime/Cargo.toml:20`）。
   方向单向 `runtime → driver-api`，`driver-api` 的 `[dependencies]` 不含任何 datazen 成员。
   所以「若不依赖就加窄端口」这个分支不成立，无需新依赖。
2. 但**落点是 runtime 侧那份**：`packages/runtime/src/connection/capability.rs:65` 已有
   `ResetForReuse { Verified, Unsupported }`，:176 的 doc 原文就是「归池复用是否被驱动验证过。
   未验证时**一切复用路径必须走关闭**」——即任务书要的那个判据早已写在类型上，只是没被归池路径读。
3. `connection/types.rs:91-95` 明确记录同名物**刻意并存、不合并**（driver 契约 vs 宿主观测是两个维度）。
4. 两份 Default **相反**：driver-api 侧 `Unsupported`（失败安全）；runtime 侧
   `CapabilitySnapshot::defaults()` = `Verified`（完整快照初值）。这正是「不是同一份」的证据。

**为什么不新造窄端口 trait**：`FixedSessionNotPoolable` 已经示范了「非阻断项挂
`HostConditionSnapshot`、由 `CleanupPlan::of` 强制置位」的先例。能力与它完全同构
（同为「宿主侧已知的拒绝 reuse 的事实」，都不携带判定依据），再开一个端口 trait
是平行造第二套机制。

## 设计定稿

1. `ReturnReason::DriverResetUnsupported` 新变体，`reason_code() = "driverResetUnsupported"`。
   命名表达**能力拒绝**，不是运行时故障（对齐 `FixedSessionNotPoolable` 的「用途而非故障」先例）。
2. `HostConditionSnapshot` 新增第 6 字段 `reset_for_reuse: ResetForReuse`，
   配 `with_reset_for_reuse()` 建造器。
3. 闸门位置：经 **`first_blocker()` 同一条通道**，排在宿主四项与 `fixed_session` **之后**。
4. `release` 里归池前复位的三态 match 与 `needs_reset` 段**未动**。
5. `ResourceManager::new` 签名**未动**（另一条轨也在看这个构造点）；只加字段与构造初始化。

### 关键取舍：`Default` 为什么取 `Verified`（第一次提交取过 `Unsupported`，实测翻车）

首版实现把 `ResetForReuse::Default` 取 `Unsupported`（失败安全方向）。实测：
**`cargo test -p datazen-runtime` 从 1001 → 963 passed / 38 failed**。
根因：`HostConditionSnapshot` 派生 `Default`，27 处既有 `HostConditionSnapshot::default()`
调用点的裁决会一次性翻成 `Closed`——这不是本任务的语义变化，是凭空把别人用例改红。

改回 `Verified` 后**逐字节回到 1001/0**。理由（已写进代码注释）：
`Default` 的语义是「调用方什么都没供给时的初值」，它只应**退回改动前的旧行为**——
跑一次复位、发现不行、关闭。真正要防的「声明了不复位却被当成可归池」由供给侧显式给
`Unsupported` 负责，不是靠默认值兜。这与 driver-api 侧 `Default = Unsupported` 不矛盾：
那边是 driver **自己的**默认，这边是**宿主快照**的默认，作用域不同。

### 关键取舍 2：能力闸为什么排最后而不是最先

初版在 `decide()` 里**特判**能力（排在 `driver.is_clean()` 之前）。新测试 (c) 立刻判红：
`left: Some("driverResetUnsupported") right: Some("hostActiveExecution")`。
即「静态事实盖掉了更具体的运行时状态」，违反 `first_blocker` 的定序纪律
（活动执行 → 协议 → 句柄 → owner → 用途）。
改为经 `first_blocker` 通道排末位后，测试 (c) 通过。**是测试逼出了这个决定**，不是设计预想。

## 观测性（三个设计问题）

**Q1 现在有什么**：`idle_lease_count()`、`occupied_slots()`、`empty_pool_metadata_len()`
早在 `ResourceManager` 上，`CleanupReport` 带 blockers/`reason_code()`/
`physical_budget_released`。但全链路没有任何东西回答「**这次为什么没归池**」：
`release` 路径原有 4 条日志全是 `warn` 且只覆盖故障（复位未确认、复位后关闭失败、
关闭握手失败、隔离），常态的「不归池」零信令。

**Q2 加了什么**：
- `release` 每次归池准入判一条 `tracing::info!`，带 `pooled` / `reason` /
  `disposition` / `admissions_declined`。用 `info` 而非 `warn`：没有任何东西出错。
- `ResourceManager::admissions_declined_for_reset()`：**只**数能力拒绝。
  与 `idle_lease_count()` 配对读才有信息量：两者都 0 = 健康；`admissions_declined` 非 0
  且 `idle` 恒 0 = 「这个连接族没有归池通道」是能力事实，不是调度异常。
- 刻意**不**伪造 `Verified` 声明，不让 stub 驱动假装干净。计数器刻意**不含**运行时失败
  （`hostResetUnconfirmed` / `driverUnclean`）——那是故障，混进来会污染能力信号。
- `cm60-bench` 未动（只读参考，P3 指标项不属本任务范围）。

**Q3 需要第三态吗：不需要。** 二值在能力声明阶段够了：`Verified` 的语义本来就是
「驱动有一个**已被验证的**reset-to-baseline 协议」。它是 driver 侧**可证明**的二元断言——
要么有被验证过的协议，要么没有。第三个值（`Unknown` / `PartiallyVerified`）会把
「没人验证过」塞进归池候选，而那正是 `Unknown` 在 `SessionScopedHandleSupport` /
`PreciseCancelSupport` 里被定为「一律不过」的原因——不可证明 ≠ 可归池。
首版 15 个驱动全 `Unsupported`，池为空是**事实**；给它一个显式计数与日志就够了，
不需要第三种取值。且 `Unknown` 会让 `admits_reuse()` 的语义从「声明过」退化成「没反对」。

## 待裁定项

1. **`needs_reset` 边界**：`Unsupported` + 仍有未释放句柄时，`release` 里那段「先回滚再关」
   仍会跑一次 `reset`（失败则隔离）。**刻意保留**：不能为省一次往返就在事务还开着时静默关闭。
   完全符合任务书「改动放 `CleanupPlan::of` / `ReturnDecision::decide` 一侧，别碰」的边界。
2. **单账审计名单**：新字段先被 `MANAGER_FIELDS` 闭合名单挡下（这正是该闸的设计意图）。
   已在名单内显式登记理由。另建隔离 worktree 验证：注入真 `AtomicU32` 第二本账仍会被
   判红（`多出 ["admissions_declined_for_reset", "cached_spare_refs"]`），闸没被我改松。
3. **`ReturnReason` 变体新增的影响面**：grep 确认其在 `packages/runtime/src/resource/` 外
   无序列化依赖（仅 `mod.rs` 再导出 + `manager.rs` 两处使用），新增不破坏外部契约。

## 门禁实测（最终 commit `b6539b911`，首尾 HEAD 一致）

| 门禁 | 命令 | 退出码 | 结论 |
| --- | --- | --- | --- |
| A | `cargo fmt --all -- --check` | **0** | 0 处 diff |
| B | `cargo check --workspace --all-targets` | **0** | 0 error |
| C | `cargo test -p datazen-runtime --no-fail-fast` | **0** | **1005 passed / 0 failed** |
| D | `cargo test -p datazen --lib` | **0** | **1810 passed / 0 failed / 6 ignored** |
| E | 见下 | — | (a) `reset_count()==0`；(b) 双闸完整 |
| F | 隔离 worktree 变异 | 见下 | 改动前 (a)(c) 实测失败 |
| G | `git status --porcelain` | — | 空；`src-tauri/Cargo.toml` / `Cargo.lock` 未动 |

基线（同 HEAD `8dae098cc` 实测，不是抄来的）：runtime **1001 passed / 0 failed**；
`datazen --lib` **1810 passed / 0 failed / 6 ignored**。与任务书给出的基线一致。

**E** — 测试 (a) 断言 `transport.reset` **调用次数 0**（不是断言最终状态）；
测试 (b) `a_verified_reset_capability_still_runs_the_full_two_gate_contract` 断言
`Open → Reset` 完整序列 + `session_reset_performed` + 复用后 driver 不干净仍被
`driverUnclean` 挡住（证明双闸独立）。

**F** — 隔离 worktree（`git worktree add --detach`，未污染本轨）把 `first_blocker` /
`all_blockers` 里的能力判定摘掉，其余源码全部保留：

```
test a_resource_whose_driver_declares_reset_unsupported_is_closed_without_a_reset_round_trip ... FAILED
test the_capability_blocker_is_reported_by_first_and_all_blockers ... FAILED
test result: FAILED. 10 passed; 2 failed

---- a_resource_... stdout ----
assertion `left == right` failed: §9.4 允许直接关闭，宿主没必要先跑一次注定失败的复位
  left: 1
 right: 0

---- the_capability_blocker_is_reported_by_first_and_all_blockers stdout ----
all_blockers 必须把能力阻断项一并报出来，不能只报第一条: ["hostActiveExecution"]
```

`left: 1 / right: 0` 是本任务的核心证据：改动前 `Unsupported` 资源**确实跑了一次**
注定失败的复位往返。另一次在 base commit `8dae098cc` 上直接跑新测试文件，失败为
编译期 `no method named with_reset_for_reuse`（行为证据由上面的变异实验补齐）。

## 遗留

无阻塞项。合并后本文件须随分支删除（AGENTS.md 纪律）。
