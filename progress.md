# p3-registry 进度

> 本文件是 `feature/p3-registry` 分支的**过程台账**，由协调者 `session-29a41067` 要求落盘
> （会话重启过一次、brief 被清空，需要一个落进分支的进度来源）。
> P3 验收通过后由协调者统一删除。它不是设计文档，结论仍以代码与测试为准。

## 状态

**Coder 已交付，等待验证（READY_FOR_TEST）**

基点 `060053afb`（feat(p3-budget): merge）。工作树 `.worktrees/datazen-p3-registry`。
集成测试 5 个二进制共 43 个用例全绿，lib 单测 272 个全绿，四道门禁全绿。

## 已完成

- [x] **D-01 冻结面补齐**：`CancelReceipt { executionId, disposition, state }` 定义在
      `registry/receipt.rs`，三态处置从 `connection::port::CancelDisposition` **转出**而非重定义。
      → `registry::receipt` 内 7 个用例（含 `receipt_serializes_with_the_frozen_camel_case_keys`）。
- [x] **D-02 统一出口**：`fold_exit(ExitFact) -> ExitProjection` 是纯函数、全覆盖；
      `ProviderError::RuntimeEpochMismatch` 折叠为 `ApiErrorCode::SessionNotFound`（不泄露「已被替换」）。
      → `registry::epoch::fold_exit_is_total_and_pairwise_consistent_over_both_fact_spaces`（9 × 18 = 162 组）。
      集成侧另有 `会话层九个变体逐一有确定结论`（9 个会话层变体逐字钉住对外码）。
- [x] **§4.5 CM-72 审计**：取消**处置**字面量（`requested`/`unsupported`/`alreadyFinished`）与
      执行**终态**字面量（`queued`…`cancelled`）逐字不相交；`outcome` 与 `executionState`
      分属两个键；取消条目构造函数**不接收** `effectOutcome`，结构上杜绝覆盖。
      → `cancel_dispositions_never_read_like_execution_states`、
        `control_disposition_and_execution_state_live_in_separate_fields`、
        `successful_cancel_never_overwrites_the_recorded_effect_outcome`、
        `audit_entries_never_carry_credentials_or_physical_handles`；
      集成侧 → `取消成功不改写数据效果只留处置`、`执行终态是唯一带效果判定的条目`。
- [x] **§9.4 宿主侧归池前检查**：`ready_to_return_to_pool` 数的是**宿主自己的账**，
      driver 报 Clean 不构成放行条件。
      → `host_side_check_forces_handle_finalization_before_close`；
      集成侧 `registry_release.rs` 10 个用例（四步顺序、跨资源分组、多批次、不可判定）。
- [x] **§6.5 句柄登记 + CM-24 取消绑定**：`HandleRegistry` 两张表互不干扰；
      伪造绑定**在任何后端调用之前**被拒且不留下脏映射。
      → `forged_binding_is_rejected_and_leaves_no_dirty_mapping`、
        `terminal_execution_does_not_invalidate_registered_handles`。
- [x] **§6.3 actor 邮箱**（`registry/actor.rs` + `actor/{cancel,exec,release}.rs`）：
      exec 进串行队列（`SETTLED_CAP = 16`），ctrl（取消/关闭）走旁路，
      队列满时拒绝并给出可判读的 `InvariantBroken`，不留半开的登记。
      754 / 208 / 203 行，均在 800 行以内。
- [x] **§4.1/§4.2/§6.4/§7.x 登记表**（`registry/registry.rs`）：登记表是宿主唯一的账
      （登记、投影、世代校验、空闲驱逐、worker 失效、取消入口、执行提交）。
      `invalidate_worker` 走控制旁路，不受 exec 队列堵塞影响。
- [x] **D-01 落位**：`registry/port.rs` 的 `cancel_execution` 返回三字段 `CancelReceipt`，
      两个签名（trait 与 `impl`）锁步改完；`FakeSessionPort` 同步。
- [x] **§12 CM-58（部分）**：只出「剩余配额」查询口（`remaining_quota` / `stale_quota_for`），
      **不实现 Job**（归 gateway 轨道）。→ 集成侧 `额度只由登记表账本回答`。
- [x] **门面** `registry/mod.rs`：8 个子模块 + 转出。**转出而非重定义** `SessionView` /
      `SessionHandle` / `CancelDisposition`；`registry` 命名空间里**不**转出
      `connection::port::CancelReceipt`（见遗留第 1 条）。
- [x] **集成测试**：`tests/registry_fixtures/mod.rs`（脚本化后端）+ 5 个二进制
      `registry_lifecycle` 7 / `registry_execution` 6 / `registry_cancel` 9 /
      `registry_release` 10 / `registry_audit` 11 = 43 个用例。

## 未做 / 有意不做

- [ ] **逐 CM 变异实验（CM-72 / CM-24 / D-02）** —— **归验证方**。
      台账原文把变异列在本轨「进行中」，但根 `AGENTS.md`「同一棵工作树不得同时被提交方与
      验证方使用」把变异划给验证方，而提交方（本实例）正是跑门禁和 `git add` 的那一方。
      两份指示冲突，**本轮不做**，请协调者确认归属。
- [ ] **§12 CM-58 的 Job**：不在本轨范围（见上）。

## 门禁实测

门禁运行期间工作树没人动过（首尾指纹逐字相同，见下）。

| 命令 | EXIT | 结论行（逐字） |
| --- | --- | --- |
| `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 272 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` |
| `cargo build -p datazen-runtime` | 0 | `Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 1.38s`（0 warning） |
| `cargo fmt -p datazen-runtime --check` | 0 | 输出 0 字节 |
| `node scripts/check-platform-crate-boundaries.mjs` | 0 | `[check-platform-arch] PASS — 22 workspace member(s) classified, 6 rule(s) evaluated over 26 crate(s), 1 rule×subject combo(s) vacuous: 0 violation(s), 0 error(s), 3 advisory(ies)` |

集成测试（**必须逐个二进制跑**；`cargo test --tests` 会 `error[E0382]`，禁止使用）：

| 二进制 | EXIT | 结论行（逐字） |
| --- | --- | --- |
| `registry_lifecycle` | 0 | `test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_execution` | 0 | `test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_cancel` | 0 | `test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_release` | 0 | `test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |
| `registry_audit` | 0 | `test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s` |

五个二进制全部 0 warning。

### 首尾指纹（证明门禁期间没人动过）

提交前那一轮（HEAD `060053afb`，23 个未暂存条目）：

| 项 | 起 | 止 |
| --- | --- | --- |
| `git rev-parse HEAD` | `060053afb54c1ae69ea498004d8b53dc9d1d57bd` | `060053afb54c1ae69ea498004d8b53dc9d1d57bd` |
| `git status --porcelain -uall` 行数 | 23 | 23 |
| `git diff \| shasum`（仅已跟踪文件） | `10bd4654efad95640b94c6e4e7b368c45a653e69` | `10bd4654efad95640b94c6e4e7b368c45a653e69` |

提交后按已提交 HEAD 又跑完整一遍（下面的结论行都出自这一轮）：

| 项 | 起 | 止 |
| --- | --- | --- |
| `git rev-parse HEAD` | `b51b03622d8c775853ca6c48382216a81f50f9ef` | `b51b03622d8c775853ca6c48382216a81f50f9ef` |
| `git status --porcelain -uall` 行数 | **0** | **0** |
| `git diff HEAD \| shasum` | `da39a3ee5e6b4b0d3255bfef95601890afd80709` | `da39a3ee5e6b4b0d3255bfef95601890afd80709` |

工作区干净 ⇒ **门禁跑的就是提交进去的那份内容**，不需要靠树对象比对来推断。

> **★ 自纠第 22 条**：第一轮我记了 `git write-tree` 当「工作区内容指纹」，提交后拿它和
> `HEAD^{tree}` 一比就对不上。原因：`git write-tree` 写的是**索引**，而当时 21 个新文件
> **还没 `git add`**，索引里根本没有它们；同一条 `git diff` 也只覆盖已跟踪文件。
> 于是那组指纹只能证明「已跟踪的两个文件没被人动过」，**证明不了那 21 个未跟踪文件的内容没变**。
> ⇒ 未提交时**不存在**可靠的工作区内容指纹；正确做法是**先提交，再在干净树上重跑门禁**。

lib 基线 **223**（硬下限，只许升）。当前 **272 = 223 + 49**。
（上一版台账写的 `251` 是子模块刚落地时的快照，actor/registry/集成用例落地后已作废。）

## 变异证据

| CM | 变异内容 | 变红的用例 | 逐字失败行 |
| --- | --- | --- | --- |
| CM-72 | — | — | **本轮未做**，归验证方（见「未做」） |
| CM-24 | — | — | **本轮未做**，归验证方 |
| D-02 | — | — | **本轮未做**，归验证方 |

## 自纠记录（自己写的用例把自己写红了，结论一并留档）

1. **两处断言写反了**
   - `state_literals_never_collide…` 断言 `Outcome::Succeeded("succeeded")` 与
     `ExecutionState::Succeeded("succeeded")` 不相交——**这是错的**，冻结枚举本来就让它们同串。
     真正的 CM-72 约束是「两者落在不同的键上」。已改成两个用例：处置↔终态字面量不相交
     （成立）、`outcome`↔`executionState` 分键（成立，并显式断言 `succeeded` 上二者确实重合）。
   - 随后又犯一次同类错：为了证明「二者确实重合」写了 `assert_ne!`，红。已改成 `assert_eq!`。
2. **`CapabilityVersions::is_version_only` 判据是反的**：原实现「为空则 true」，
   任何真实版本号都判 false。已改为「非空、≤32 字符、不含空白与 `/:@=?#`」，
   并补反例（DSN、空串必须判 false）。
3. **`ProviderError` 是 18 个变体不是 17**：provider 事实表本来就齐，是期望值写错。已改 18/162。
4. **两处「看起来显然」的错误断言**：`json.contains("disposition")` 当成 snake_case 证据
   （`disposition` 本就是单词）；`json.contains('_')` 扫全串（值 `exec_1` 里本来就有下划线）。
   已改成逐键集合断言 `["disposition","executionId","state"]`。
5. **`CancelReceipt` 的 `Deserialize` 实际编译不过**：字段是 `&'static str`，
   手写 `Deserialize` 要求 `'de: 'static`，`from_str`/`from_value` 都编译不过
   （"implementation of Deserialize is not general enough"）。
   没有为此把线上字面量降级成 `String`（那会丢掉「审计侧与调用方逐字同源」），
   改为在值树上逐键断言。→ 见「遗留」第 5 条。
6. **「四个用例死锁」的诊断是错的，已撤回**。当时四个 actor 用例挂在 60s 上限，
   我判成 tokio 调度死锁。实际上 `run_actor` 把 `state.in_flight.take()` 走了，
   于是 `cancel()` 里的 `in_flight_here` **永远返回 false**，取消分支根本走不到——
   是生产路径缺陷，不是调度问题。修法：`in_flight` 留在 state 里，actor 循环把两个
   `.await` 点（`bind_rx`、`join`）从 `Option` 里 `take()` 进局部变量再进 `select!`
   （tokio 的借用规则：`select!` 各分支 future 全部存活期间，分支体内不能再借用同一个局部）。
7. **`biased;` 加在关闭的 `mpsc` 接收端前会饿死整个循环**：第一分支永远 `Ready(None)`，
   循环自旋不退出。修法：直接 `drop` 掉接收端，不要重新武装它。
   另一条同源纪律：**绝不再次 poll 一个已经返回过 `Ready` 的 `JoinHandle`**，用
   `Option<JoinHandle>` + `std::future::pending()` 兜底。
8. **一个从未编译过的用例，红的原因是别处**：T10（墓碑）红在 `release.rs:50` 的
   `SessionClosed(..)`。真实原因是墓碑写出的条件挂错了键（按关闭成功与否判定，
   而不可判定的那次关闭本来就**不会**成功），改成挂 `undecidable.is_some()`。
   结论：**任何用例的红都要从新鲜编译重新定位，不能沿用旧的行号图**。
9. **继承了前一个实例 46056 字节的 rustfmt 欠账**，`cargo fmt -p datazen-runtime --check`
   一开始就 EXIT=1。跑了一次 `cargo fmt -p datazen-runtime` 归零。
   注意：此后任何对带中文文档注释的文件的编辑都会**再次**让 `--check` 变红，必须重跑。
10. **暂停时钟下 `tokio::time::timeout` 不是超时，是自动推进时间**：`#[tokio::test(start_paused = true)]`
    运行时，只要任务空闲且挂着定时器就自动跳时间，「等 10ms 以为在超时」和「真的超时」
    在这里完全无法区分。判据一律改成显式闸门（`mpsc`）+ `oneshot::try_recv()` 的三态。
11. **闸门释放必须写在放行它所阻塞的那些执行的同一个 `tokio::join!` 里**：
    两任务互相等待的纯会合死锁里，暂停时钟不会推进，只有一条「已经就绪并挂着定时器」的
    分支能解开它。
12. **`tokio::spawn` 出来的任务在当前任务让出前不会被 poll**：刚 spawn 就 `try_recv` 必然是
    `Empty`，于是「控制面被堵死」和「控制面还没被调度」长得一模一样。必须 poll-with-yield
    （夹具里的 `ask_until` / `wait_until` 数的是调度轮次，不是毫秒）。
13. **夹具把 `context_revision` 钉死成同一个值时，「版本推进」类断言是空的**。
    真相：后端上报的 `ResourceExecution.context_revision` 是**替换**会话版本，
    不是自增；夹具把它钉成 `2` 而请求也写 `2`，断言「推进」永远成立。已改用默认计划。
14. **`SessionRegistry` 的几个签名都会咬人**：`new` 的 `Arc<dyn SessionBackend>` **按值**收，
    而 `Arc::clone(&x)` 返回 `Self`、**不是**强制转换点（必须 `as Arc<dyn SessionBackend>`）；
    `epoch_of` 返回 `Result`；`session_view` 是 `SessionPort` 的方法（要 `use ...::SessionPort`）；
    `SessionRegistry` **不是** `Clone`（要包 `Arc`，跨任务时显式 clone 搬进去的句柄）。
15. **`cancel.rs` 的文档表格写的是一条从未存在的分支顺序**，代码是
    `check_epoch → 物理在不在 → 是不是本会话在飞 → 驱动支不支持取消 → 绑定对照`。
    根因不是笔误：**绑定对照物理上不可能早于句柄发布**——绑定值里的句柄是后端在执行途中
    才发布的，没人发布过就无从对照。文档已重写，并明确真正被守住的不变式是
    「伪造绑定不留痕」。**仅改文档，未改任何行为**。
16. **夹具的 `execute_calls()` 数的是「进了 execute」，不是「发布了取消句柄」**，
    两者在默认 `Immediate` 脚本下都 +1，于是断言「先执行后取消」的时序其实是空断言。
    为此加了独立的 `published_calls()` 计数器。
17. **`release()` 的 `finalized` 是跨批次求和的**，而夹具原来每次返回同一个常数，
    多批次的断言因此恒真。夹具加 `finalize_sequence`，按次给出不同结果。
18. **★ `drain_audit()` 不是「全部条目」，只是自上次 `pump()` 之后的增量**，而且
    `is_registered()` / `registered_ids()` / `session_view()` / `cancel_execution()`
    / `register_session()` 成功分支都会顺手 `pump()`。两条独立的坑都被集成用例钉住：
    ① 登记刚完成时增量窗口**已经是空的**（`register_session` 自己在成功分支收尾 pump 过）；
    ② 一条只读投影（`is_registered`）会把队列里没读的条目**吃掉**。⇒ **累积断言一律用
    `audit_log()`**，`registry_release.rs` 的 5 个失败用例全部属于这一类。
19. **★ `emit` 从 `state.physical` 统一填 `capability_versions`**，所以
    `ExecutionCompleted` 条目也带能力版本——我原本断言它是 `None`，红了。
    已改成断言它与登记那条**逐字相同**且是 version-only。
20. **★ `ProviderError::HostRejected → ApiErrorCode::InvalidArgument` 是在冻结层
    `connection/error.rs:136` 写死的**，不是 `registry` 折叠出来的。
    我原本断言 `fold_exit(Provider(HostRejected))` 返回 `NotOnTheWire`，红了。
    真正的「不上线路」约束是：`"hostRejected"`（`ExecutionErrorCode` 的写法）
    **不是任何 `ApiErrorCode` 的线格式字面量**，已把 20 个对外码逐个列出来钉住。
    （`CancelFailed` / `InvariantBroken` 返回 `NotOnTheWire` 那条结论不变，见遗留第 3 条。）
21. **`open_and_publish` 的重复登记只在 `write_table().insert` 处被拒**，
    也就是**物理资源已经开出来之后**。额度确实退回，但后端 `open` 调用已经发生了一次。
    没有为此改行为（冻结在登记表语义里），列入遗留请协调者裁定。

## 遗留 / 待裁定

1. **冻结面同名物**：`connection::port::CancelReceipt`（2 字段，缺 `state`）与本模块的
   `CancelReceipt`（3 字段，§7.6 形状）**同名不同形**。后者是 §7.6 要求的那个。
   本模块**没有**把前者转出到 `registry` 命名空间，避免两个同名物同时出现在一个 prelude 里。
   → 请协调者裁定：Wave 2 是否收敛 `connection::port::CancelReceipt`（那需要解冻 `connection/**`），
   还是保留两处并在调用点显式区分。
2. **`ExecutionState::as_str()` 不存在**（`connection/execution.rs` 只有 `ExecutionErrorCode::as_str`、
   `EffectOutcome::as_str`、`TruncationReason::as_str`）。`connection/**` 禁改，故在
   `registry/audit.rs` 私有映射了一份，并用 `state_literals_match_the_serde_wire_casing`
   把它逐字钉在 serde 输出上，防漂移。→ 若解冻 `connection/**`，应把这份映射上提。
3. **`hostRejected` 不在 `ApiErrorCode` 里**：它是 `ExecutionErrorCode` 的取值，
   属于另一个命名空间。因此 `RuntimeError::CancelFailed` / `InvariantBroken`
   （`api_code()` 返回 `None`）无法诚实地折叠成一个 `ApiErrorCode`。
   `fold_exit` 为此返回 `ExitProjection::NotOnTheWire { reason }`（取消回执仍走
   `unsupported` 正常返回），避免把宿主缺陷伪装成调用方可修正的派发前拒绝。
   → 若上游希望它们也上到线上码，需要给 `ApiErrorCode` 加取值（`platform-api` 冻结面）。
4. **`ProviderError::HostRejected(_) -> ApiErrorCode::InvalidArgument`** 与
   `RuntimeEpochMismatch -> SessionNotFound` 两处刻意分歧已在 `fold_exit` 文档中标注并被用例覆盖。
5. **`RegistryAuditEntry` 只能序列化、不能反序列化**：因为它存 `&'static str`（见自纠第 5 条）。
   这是有意的取舍——线上字面量必须与调用方逐字同源，不能退化成可随手写错的 `String`。
   代价：gateway 轨道若要从线上 JSON **重建**审计条目，需要协调者裁定解冻方向
   （要么允许 registry 提供一个 `String` 侧的 DTO，要么给 `connection/**` 加字面量常量）。
6. **★ `release()` 不可判定时的回收路径不存在（生产缺陷，未自行修改）**：
   不可判定分支已经跑完 §9.4 四步（句柄已终结、宿主检查已判否、资源已关），却返回
   `Err(SessionLost(..))`。`close_registered` 只在 `Ok` 时忘记登记行，因此这一条路上
   **既不摘行也不退额度**——每次不可判定释放泄漏一行 + 一个额度位，且会话已被移出
   可用集合，调用方没有任何办法回收它。请协调者裁定这是刻意的「留证据」还是缺陷。
7. **`invalidate_worker` 的成功分支发了两条 `SessionInvalidated` 审计条目**（同一会话）。
   是重复发出，不是两次不同的判定。请协调者裁定是补一条断言还是删掉多出的那次。
8. **简报与仓库实况不一致之处（均按实况处理，未擅自改简报）**：
   - 简报称工作树有 14 个未提交条目，实为 **23** 个（含本轮新增）。
   - 简报称 `mod.rs` 尚未声明 `actor` / `registry`，实为已声明且已通过门禁。
   - 简报把 CM 变异实验列在本轨，根 `AGENTS.md` 把变异划给验证方（见「未做」）。
   - 本工作树里的 `AGENTS.md` 仍写着「不写进度台账」，而集成分支（`637fb5824`）的新版
     `AGENTS.md` 与协调者都要求写。本文件按新版执行，请在合并时与新版一并处置。
