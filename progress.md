# p3-gateway 进度

> 本文件由协调者要求落盘（会话重启导致上下文丢失 + `/tmp` brief 被清空）。
> P3 验收通过后由协调者统一删除。

## 状态

**实现完成，门禁全绿，已提交（见文末提交记录）。**
- in-lib 单元测试 **全绿**：323 passed（= 223 基线 + 100 新增），EXIT=0。
- 集成测试二进制 `gateway_contract` **全绿**：44 passed，EXIT=0。
- `cargo build` 0 warning；`cargo fmt --check` 退出 0 且输出为空；边界检查 0 violation / 0 error。

## 已完成

- [x] CM-60 两段单调计时骨架 `timing.rs`（`OverheadProbe` / `FixedClock` / `OverheadSamples`）
      → `fixed_clock_only_moves_when_advanced`、`sample_is_none_until_both_boundaries_are_marked`、
      `fake_sql_time_is_excluded_from_both_segments`、`queueing_before_acceptance_is_excluded`、
      `repeated_marks_keep_the_first_boundary`、`a_backwards_reading_degrades_to_zero_instead_of_wrapping`、
      `p95_is_computed_over_per_request_totals`、`empty_samples_have_no_percentile`
- [x] CM-61/62 来源与权限 `provenance.rs`（`ExecutionSource` / `Authorizer` / `RequestPrincipal`）
      → `kind_literals_round_trip`、`persistable_json_carries_no_runtime_handles`、
      `an_empty_source_id_is_not_persistable`、`background_sources_are_exactly_job_and_workflow_block`、
      `denial_carries_action_and_stable_reason`
- [x] CM-54 幂等账本 `idempotency.rs`（`IdempotencyLedger` / `RequestFingerprint` / `InMemoryIdempotencyStore`）
      → `an_empty_or_blank_key_is_not_usable`、`the_same_key_in_two_sessions_does_not_collide`、
      `the_same_key_in_two_epochs_does_not_collide`、`fingerprint_separates_command_and_input_and_revision`、
      `a_field_separator_prevents_concatenation_collisions`、`miss_then_hit_returns_the_same_execution_id`、
      `reusing_a_key_for_a_different_request_is_a_conflict_not_a_replay`、
      `an_unreadable_record_is_never_downgraded_to_miss`、`a_write_failure_surfaces_as_a_write_error`、
      `store_len_tracks_distinct_scopes`
- [x] CM-55 事件交付 `events.rs`（`EventStore` / `EventDisposition`）
      → 15 个单测，见文件
- [x] §7.6 / D-01 取消绑定与三态 `cancel.rs`
      → `disposition_literals_match_the_architecture_map`、
      `a_live_state_maps_to_requested_and_a_terminal_one_to_already_finished`、
      `the_port_mapping_can_never_produce_unsupported`、
      `an_epoch_mismatch_is_refused_before_touching_the_port`、
      `a_session_mismatch_is_refused_before_touching_the_port`、
      `a_resource_binding_mismatch_is_refused_on_both_sides`、
      `an_unknown_execution_never_becomes_a_cancel_error_free_success`、
      `an_unsupported_driver_is_not_downgraded_to_a_session_wide_cancel`、
      `a_cancel_outcome_never_carries_a_runtime_handle`、
      `only_succeeded_failed_and_cancelled_count_as_terminal`、`cancel_authorization_carries_the_frozen_source`
- [x] §7.2 受理请求与受理回执 `request.rs`（`ExecutionRequest` / `GatewayAcceptance` / `GatewayError`）
      → 10 个单测，见文件
- [x] 单元测试夹具 `testing_support.rs`（`#[cfg(test)]`，不与集成测试共享）
- [x] `gateway/mod.rs` 门面 `ExecutionGateway`：`accept` / `dispatch` / `cancel` / `apply_event` /
      `recover_from_snapshot` / `overhead_p95`（已编译通过）
- [x] `packages/runtime/src/lib.rs` 增加唯一一行 `pub mod gateway;`
- [x] 门面级用例拆成三个文件（受 800 行上限约束）：
      `facade_support.rs`（399 行，`FakePort` / `Harness` / 替身作者，**全部 `pub(crate)`**）、
      `facade_tests.rs`（493 行，§3.1 受理 22 例）、
      `cancel_event_tests.rs`（528 行，§3.2 取消 8 例 + §3.3 事件 11 例）
- [x] `FakePort` 支持注入驱动往返耗时（`with_driver_nanos` + `attach_clock`），
      让 CM-60 的「网关开销 vs 驱动往返」在同一根时间轴上真正分开
- [x] **修掉一个自己写出的真缺陷**：`apply_event` 原先按 `(dbSessionId, runtimeEpoch)`
      在 `HashMap` 里反查归属，同一会话并发两条执行时归属是概率性的。
      改为 `ExecutionEvent` 显式携带 `executionId`，归属只按它判定；
      epoch 校验保留（驱动重启复用旧 id 的残帧正是靠它认出来）。
      连带用例：新增 `an_event_from_a_later_epoch_is_ignored_even_with_the_right_execution_id`、
      `an_event_carrying_another_session_is_ignored_and_changes_nothing`，
      `two_executions_keep_independent_event_watermarks` 改成「事件指向 first 就不许动 second」。
- [x] `packages/runtime/tests/gateway_contract.rs` + `tests/gateway_contract/*.rs` 八个分节
      （§3.1 受理 12 例 / §3.2 取消 7 例 / §3.3 幂等 6 例 + 事件 8 例 / §3.4 来源 4 例 /
      §4 不变量 3 例 / 修订闸门 3 例 / §3.5 计时 2 例，共 44 例）
- [x] 集成夹具 `tests/gateway_fixtures/mod.rs`（`RecordingPort` / `Harness` /
      `UnreadableStore` / `FlippingAuthorizer` / `WriteFailingStore`）
- [x] **第二个自己写出的真缺陷（集成测试暴露）**：`dispatch()` 一进锁就先把
      `dispatched = true` 占位打上，闸门没过（会话丢失 / 权限撤销 / 修订号被改）时
      占位没收回去 ⇒ 一条**从没下发过**的执行自称已下发，调用方按 `alreadyDispatched`
      永远重试不了。改为「碰到驱动之前失败就归还占位，驱动自己失败则保留占位」，
      两种语义各有独立用例：`a_session_lost_between_accept_and_dispatch_is_not_downgraded`
      （闸门失败 ⇒ `is_dispatched() == false`）、
      `a_driver_failure_keeps_the_dispatch_reservation`（驱动失败 ⇒ 仍为 `true`）。
- [x] 集成测试暴露的 4 条**我自己写错**的期望（改的是测试，不是生产代码）：
      `CancelRequested` 不是终态，断言反了；重复分片**会消费序号**但不加行，水位应到 `huge + 1`；
      冻结 DTO 的 JSON 键回来是字典序（`serde_json::Map` 是 BTreeMap，本工程没开
      `preserve_order`），比的是键集合；`Job + 非空 id` 的来源**是可落盘的**，
      真正不可落盘的是空 `source_id`（补一条正面用例锁住前者）。

## 进行中

无。实现、变异实验、门禁实测均已跑完。

## 未开始

无（本轨道范围内）。性能门禁按 §3.5 明确不在本次范围。

## 门禁实测

基线（改动前，仅 `cargo test -p datazen-runtime --lib`）：

| 命令 | EXIT | 结论行（逐字） |
| --- | --- | --- |
| `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 223 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` |
| `cargo test -p datazen-runtime --lib`（六个子模块 + 门面骨架后） | 0 | `test result: ok. 282 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` |
| `cargo test -p datazen-runtime --lib`（门面三个测试文件拆完、修完事件归属后） | 0 | `test result: ok. 322 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` |
| `cargo test -p datazen-runtime --lib`（补下发占位归还语义后） | 0 | `test result: ok. 323 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` |
| `cargo test -p datazen-runtime --test gateway_contract` | 0 | `test result: ok. 44 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s` |

223 是**硬下限**，最终结果必须是 `223 + 新增 in-lib 用例数`。
当前 `323 = 223 + 100`（timing 8 + provenance 5 + idempotency 10 + events 15 + request 10
+ cancel 11 + 受理 22 + 取消 8 + 事件 10 + 驱动失败占位 1）。

**提交前最终一轮（全部普通 cargo，target 目录 = 工作树 `target/`）**，
运行前 `HEAD=060053afb`，工作区指纹 `ee1707a5c60fc407e600bf04286658d783e36bfc8772bcc70cf46fcde458946e`，
运行后 HEAD 与指纹**逐字相同**（首尾各记一次，证明运行期间没人动过工作树）：

| 命令 | EXIT | 结论行（逐字） |
| --- | --- | --- |
| `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 323 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s` |
| `cargo build -p datazen-runtime` | 0 | 无输出（`warning` 行数 = 0） |
| `cargo fmt -p datazen-runtime --check` | 0 | 输出 0 字节 |
| `node scripts/check-platform-crate-boundaries.mjs` | 0 | `[check-platform-arch] PASS — 22 workspace member(s) classified, 6 rule(s) evaluated over 26 crate(s), 1 rule×subject combo(s) vacuous: 0 violation(s), 0 error(s), 3 advisory(ies)` |
| `cargo test -p datazen-runtime --test gateway_contract`（逐个二进制，未用 `--tests`） | 0 | `test result: ok. 44 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s` |

**提交后最终一轮（§5 要求「提交后再跑一遍」）**：
运行前 `HEAD=2a1ba7ada`、工作区 `git status --porcelain -uall` **0 行**、
工作区指纹 `2af41c4d26d5433339e24c0118216d9ee45f405806b1344259b768ee112210eb`，
运行后 HEAD 与指纹**逐字相同**：

| 命令 | EXIT | 结论行（逐字） |
| --- | --- | --- |
| `cargo test -p datazen-runtime --lib` | 0 | `test result: ok. 323 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s` |
| `cargo build -p datazen-runtime` | 0 | 无输出（`warning` 行数 = 0） |
| `cargo fmt -p datazen-runtime --check` | 0 | 输出 0 字节 |
| `node scripts/check-platform-crate-boundaries.mjs` | 0 | `[check-platform-arch] PASS — 22 workspace member(s) classified, 6 rule(s) evaluated over 26 crate(s), 1 rule×subject combo(s) vacuous: 0 violation(s), 0 error(s), 3 advisory(ies)` |
| `cargo test -p datazen-runtime --test gateway_contract` | 0 | `test result: ok. 44 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s` |

§4 不变量静态自查（脚本按 `#[cfg(test)] mod … {…}` 花括号配平剥掉测试块后统计，
11 个文件逐个核对）：

| 文件 | 行数 | `.unwrap(` | `.expect(` | `panic!(` | `unsafe ` | `#[allow` | `sleep(` | U+FFFD |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `cancel.rs` | 444 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `events.rs` | 737 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `idempotency.rs` | 543 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `mod.rs` | 530 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `provenance.rs` | 350 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `request.rs` | 489 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| `timing.rs` | 305 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |

`testing_support.rs`(91) / `facade_support.rs`(399) / `facade_tests.rs`(521) / `cancel_event_tests.rs`(528)
整体就是 `#[cfg(test)]` 代码，其中的 `panic!` 属测试断言，不在「生产路径」口径内。
集成测试分节最大 276 行，夹具 541 行，全部 ≤800。

过程中 4 条**我自己写错**的断言已改正（改的是测试，不是生产代码）：
缺口补订阅起点契约是「缺口前最后一个已应用序号」；
自称来源的帧其状态迁移本身合法、被丢掉的只有来源声明；
首帧没有前序序号因而不构成缺口；
冻结时钟下段②的登记耗时必为 0，算术证明留在 `timing.rs` 单测里。

首次编译一次性暴露并已修掉的问题（记录以免二次踩）：
`PreciseCancel` 不在 `crate::connection` 而在 `crate::connection::capability`；
`ClientInstanceId` / `EditorSessionId` 由 `datazen_platform_api::id` 再导出、不在 `connection` 的
`pub use types::{..}` 名单里；`OverheadProbe::mark_*` 取时钟引用而非纳秒；
`EventDisposition::mutated_state` 是 `self`（消耗值），不能在 `if` 里判断后再返回。

## 变异证据

同一棵工作树里既做变异又做提交（AGENTS.md 要求变异与提交分树）。这是我做不到的
前提限制下的折中，**逐条披露如下**：每次变异都先备份目标文件到系统 temp、跑完立刻还原，
并用 sha256 校验还原结果（10/10 还原校验 = OK）；原始日志留在 `/tmp/dz-gw-mutant-*.log`。
请验收方另开工作树独立复核这 10 条。

| CM | 变异内容 | 变红的用例 | 逐字失败行 |
| --- | --- | --- | --- |
| CM-54 | `gateway/mod.rs` 幂等命中时不再回放原 `executionId`，改拼一个新 id（等价于换 key 重跑） | EXIT=101，1 例：`idempotency::a_resend_returns_the_same_execution_id_and_writes_once` | `thread '…a_resend_returns_the_same_execution_id_and_writes_once' panicked at packages/runtime/tests/gateway_contract/idempotency.rs:26:5:`<br>`assertion \`left == right\` failed`<br>`left: ExecutionId("exe_db_session_contract_0")`<br>`right: ExecutionId("exe_mutant_exe_db_session_contract_0")` |
| CM-55 | `gateway/events.rs` 删掉 epoch 精确相等校验（陈旧 epoch 的帧不再被丢弃） | EXIT=101，1 例：`gateway::cancel_event_tests::a_stale_epoch_event_is_ignored_and_leaves_the_record_alone` | `thread '…a_stale_epoch_event_is_ignored_and_leaves_the_record_alone' panicked at packages/runtime/src/gateway/cancel_event_tests.rs:236:5:`<br>`assertion \`left == right\` failed`<br>`left: Applied`<br>`right: IgnoredStaleEpoch { bound: Counter(1) }` |
| CM-56 | `gateway/idempotency.rs` 账本读失败被降级成 `Miss`（于是会另写一条并再跑一次） | EXIT=101，1 例：`idempotency::an_unreadable_record_requires_verification_and_writes_nothing` | `thread '…an_unreadable_record_requires_verification_and_writes_nothing' panicked at packages/runtime/tests/gateway_contract/idempotency.rs:105:18:`<br>`期望要求核验，实际 IdempotencyPersistFailed { message: "存储不可写" }` |
| CM-60 | `gateway/timing.rs` 段①终点由「紧贴端口调用之前」改成「驱动返回之后」（把驱动往返算进网关开销） | EXIT=101，1 例：`gateway::timing::tests::fake_sql_time_is_excluded_from_both_segments` | `thread '…fake_sql_time_is_excluded_from_both_segments' panicked at packages/runtime/src/gateway/timing.rs:223:9:`<br>`assertion \`left == right\` failed: 段① = 鉴权/校验完成 → 派发`<br>`left: 10000002`<br>`right: 2` |
| CM-61 | `gateway/mod.rs` 让事件自带的 `declared_source` 反过来覆盖执行记录上的请求来源 | EXIT=101，1 例：`gateway::cancel_event_tests::a_terminal_event_updates_the_recorded_state_but_never_the_source` | `thread '…never_the_source' panicked at packages/runtime/src/gateway/cancel_event_tests.rs:307:5:`<br>`left: ExecutionSource { kind: Job, source_id: "job-rogue", organization_id: None, principal_id: None }`<br>`right: ExecutionSource { kind: Editor, source_id: "edt-facade", … }` |
| CM-62 | `gateway/mod.rs` 下发前不再复查权限（受理时那一次授权被当成够用） | EXIT=101，1 例：`gateway::facade_tests::a_permission_revoked_between_accept_and_dispatch_blocks_execution` | `thread '…blocks_execution' panicked at packages/runtime/src/gateway/facade_tests.rs:361:18:`<br>`权限被撤销后绝不能下发` |
| §3.1 乐观闸门 | `gateway/mod.rs` 下发前不再重比 `expected_context_revision` | EXIT=101，1 例：`gateway::facade_tests::a_context_revision_change_between_accept_and_dispatch_is_refused_before_execution` | `thread '…is_refused_before_execution' panicked at packages/runtime/src/gateway/facade_tests.rs:385:18:`<br>`revision 变化后绝不能真正执行` |
| §3.1 受理不被降级 | `gateway/mod.rs` 下发前闸门失败时不再归还「已下发」占位（我的第二个缺陷原样复现） | EXIT=101，1 例：`gateway::facade_tests::a_session_lost_between_accept_and_dispatch_is_not_downgraded` | `thread '…is_not_downgraded' panicked at packages/runtime/src/gateway/facade_tests.rs:293:5:`<br>`assertion failed: !record_of(&h, &id).await.is_dispatched()` |
| §3.2 取消绑定 | `gateway/cancel.rs` `verify_binding` 开头直接 `return Ok(())`（三道绑定全不校验） | EXIT=101，1 例：`gateway::cancel_event_tests::a_binding_on_a_different_epoch_is_refused_before_the_port_is_touched` | `thread '…refused_before_the_port_is_touched' panicked at packages/runtime/src/gateway/cancel_event_tests.rs:128:24:`<br>`绑定不一致绝不能取消成功，实际 CancelOutcome { … disposition: Requested, state: CancelRequested, … }` |

**一条无效变异（如实自报）**：§3.2 第一版变异只把 `verify_binding` 里的
**会话 id** 一道改成 `if false && …`，而目标用例走的是 **epoch** 那一道，
结果 EXIT=0、0 例变红 —— 这次变异根本没碰到语义，不算证据。换锚点（函数开头直接
`return Ok(())`）后才有上表第 9 行那条结果。

**一次假红（如实自报，且比假绿更危险）**：补跑 CM-54/CM-56 两条变异时，我的还原脚本
用 `shutil.copy2`，它**连 mtime 一起还原**到变异前的时间戳；cargo 按 mtime 判新鲜度，
于是源码看起来比变异时编出来的产物还旧，**不重编、直接复用了带变异的测试二进制**。
表现是门禁复跑时 `gateway_contract` 43 passed / 1 failed，失败行与 CM-56 变异逐字相同 ——
看起来像"代码没还原干净"，其实字节已还原（sha256 相等）。
处置：`touch` 全部网关源码与测试文件强制重编后再跑门禁，44 passed / EXIT=0。
教训：**只校验内容哈希不够，还必须校验 mtime 或直接 `touch`**；这一条也请验收方照做。

注：上表「逐字失败行」里的 `thread '…'` 是把用例名省略成 `…` 之后的原文，
日志未作任何改写；完整逐字文本在 `/tmp/dz-gw-mutant-*.log`。
CM-54/CM-56 两行的行号是在 `cargo fmt` 重排 `tests/` 之后**重跑变异**得到的，
与交付树当前行号一致（CM-54 仍为 `idempotency.rs:26:5`，CM-56 因格式化后移为 `:105:18`）。

## 遗留 / 待裁定

1. **D-01 我需要 registry 的 `CancelReceipt`，协调者需裁定。**
   `registry/port.rs` 的 `cancel_execution` 目前返回 `ExecutionState`，不是架构文档 §7.6 规定的
   `CancelReceipt { executionId, disposition, state }`。契约文档已预告 registry 轨道会改签，
   但那条轨道不在本工作树内。当前处理方式（**不改 `registry/**`、**不私造同名 `CancelReceipt`**）：
   - 网关侧类型一律加前缀命名（`CancelDisposition` / `CancelOutcome`），与 registry 正式类型不撞名；
   - 三态映射收敛在 `cancel::disposition_from_port_state` 一处，registry 落地后只需把它换成透传，
     `CancelOutcome` 形状不动；
   - `Unsupported` 只能由网关拒绝下发产生（`PreciseCancel::Unsupported` 来自调用方，
     因为 `SessionPort` 没有能力查询方法），端口状态映射**不可能**返回 `Unsupported`。
2. `GatewayError` 是网关自有错误枚举：`RuntimeError` 是冻结 DTO，§4 禁止新增变体，
   且它没有 `PermissionDenied` / 幂等核验变体。`GatewayError::Runtime` 为透明变体，绝不降级。
3. **`registry::SessionView` / `registry::SessionHandle` 在基线 `060053afb` 上不存在。**
   §10 已裁定 registry 只做再导出，但基线 `registry/mod.rs` 里只有
   `pub use port::SessionPort;` 一条（`port.rs` 用 `use crate::connection::{…}` 私引 DTO）。
   当前处理方式：不 import 任何他轨内部类型，也**不私造同名类型**，统一从冻结 DTO 的
   **定义处** `datazen_runtime::connection::…` 取用。registry 补上再导出后，
   测试里的 import 需要改路径（纯机械替换，语义不变）——请协调者知悉这条改名成本。
