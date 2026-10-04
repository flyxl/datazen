# P3 Hub

> 集成分支的跨轨汇总。**临时进度文件**——P3 开发完成并通过验收后由协调者删除。
> 设计结论请写进 `docs/architecture/`，不要停留在此文件。

## 基准

- 集成分支：`main`
- 当前基点：`060053afb`（`feat(p3-budget): merge`）

## 波次划分

| Wave | 内容 | 状态 | 合并提交 |
| --- | --- | --- | --- |
| 0 | `registry/port.rs` 接缝（`SessionPort` trait + `SessionView` 投影） | ✅ 已合入 | — |
| 1 | `directory` / `resource` / `budget` 三轨并行 | ✅ 全部 TEST_PASSED 并合入 | `60f0ac6b9` / `2c7ceac6e` / `060053afb` |
| 2 | `registry` / `gateway` 两轨并行 | 🔄 Coder 进行中 | — |

Wave 2 之后无已定义的 Wave 3。剩余工作为 P3 退出门禁对账（CM-02～07、20～32、37～39、54～56、60、61～74），其中 CM-73 / CM-74 必须转绿。

## Wave 1 汇总

| 轨 | 合并提交 | 轮次 | lib 测试 | 集成测试 |
| --- | --- | --- | --- | --- |
| `directory` | `60f0ac6b9` | 2 | 172 | 12/7/10/6 |
| `resource` | `2c7ceac6e` | 1 | 185 | 7/4/6 |
| `budget` | `060053afb` | 1 | 162 | 8/2/6/7 |

main 合并后门禁：lib **223 passed / 0 failed**（=148+24+37+14）、build 0 warning、fmt 0 字节、平台边界 `0 violation(s), 0 error(s), 3 advisory(ies)`。

## Wave 2 轨道

| 轨 | 分支 | 工作树 | 状态 |
| --- | --- | --- | --- |
| `p3-registry` | `feature/p3-registry` | `.worktrees/datazen-p3-registry` | 🔄 Coder 进行中 |
| `p3-gateway` | `feature/p3-gateway` | `.worktrees/datazen-p3-gateway` | ✅ Coder 已 `READY_FOR_TEST`（`19545268d`）→ 🔄 Tester 验收中 |

各轨细节见对应分支的 `progress.md`。

### gateway 轨交付快照（2026-10-04）

- HEAD `19545268d`，基线 `060053afb`，工作区 0 行；提交链 `e36228175` → `2a1ba7ada` → `19545268d`
- 23 个改动路径全在允许范围；`lib.rs` 恰好一行 `pub mod gateway;`
- 自报：lib `323 passed`（= 223 + 100）、build 0 warning、fmt 0 字节、边界 PASS、`--test gateway_contract` `44 passed`
- **以上均为 coder 自报，未经复核。** Tester 须独立复跑并自行设计新变异，不得采信。

### gateway 轨三条待裁定 → 协调者已答

1. **D-01 归属**：早已裁定——`CancelReceipt` 由 **registry 轨**定义并从 `datazen_runtime::registry::` 导出（放 `registry/receipt.rs`，因 `connection/**` 在 registry 的禁改区），gateway 轨只消费、**禁碰 `registry/**`**。gateway 现用 `CancelDisposition` / `CancelOutcome` 前缀类型 + `cancel::disposition_from_port_state` 单点收敛，属**合并前的临时形态**；registry 落地后该函数改为透传，`CancelOutcome` 形状不变。Tester 需验证「收敛点确实只有一个出口」，否则合并时会长出第二份三态语义。
2. `GatewayError` 归属：确认为 gateway 轨自有错误枚举（`RuntimeError` 是冻结 DTO，§4 禁止加变体，且缺 `PermissionDenied` / 幂等核验变体）。`GatewayError::Runtime` 为透明变体，**绝不降级**。
3. `registry::SessionView` / `SessionHandle` 缺失：确认 registry 轨只做**再导出**（§3.2 #5）。gateway 现从定义处 `datazen_runtime::connection::…` 取用，registry 补上再导出后 import 机械换路径，语义不变。**这不是缺陷**，但若合并时需要大改则是阻塞项。

### gateway 轨第 1 轮验收 → `TEST_FAILED`（2026-10-04，已派第 1 轮修复）

**门禁自报全部被独立复现、逐字一致**：lib `323 passed`、build 0 warning、fmt 0 字节、边界 PASS、`gateway_contract` 44 passed。静态审查同样成立：7 个生产文件禁用构造 0 违规、最大 737 行、`invariants.rs` 是真结构闸门（`read_dir` 断言文件集恰为 11 个名字）、`lib.rs` diff 恰一行、`GatewayError::Runtime` 真透明透传、`ContextRevisionMismatch` 确带服务端 actual、`source` 无 setter、23 路径零越界。

**变异：15 个，14 红 1 存活。** coder 自报 9 条全部独立复现，Tester 另设计 N1–N4 四条新变异也全部被杀。

阻断缺陷：

| ID | 严重度 | 摘要 |
| --- | --- | --- |
| D-01 | 高 | `recover_from_snapshot` 用 `SessionWatermark::new` **重建**水位线 → 终态倒回 `Queued`、已观测行数清零、已应用序号重投会重复累加（CM-55 被恢复路径重新打开）。现有用例只断言 `needs_recovery`/`resubscribe_from`/`context_revision`，无一条断言 `last_state`/`observed_row_count` 被保留 |
| D-02 | 中高 | CM-61「来源计入指纹」**零测试覆盖**：`source` 整段删掉后 367 条全绿（M6 EXIT=0）。两条冲突用例都只改 `call.input`。而 `accept` 的 Hit 分支把**重发方** `request.source` 写进受理回执，规则一旦被重构掉，同 key 换来源重发会静默变 Replay：回执是新来源、执行是旧那条 |
| D-03 | 低 | 同一 crate 两个同名 `CancelDisposition`，变体与 `as_str()` 完全重复 |
| D-04 | 低 | `SourceKind::is_background()` 生产不可达，文档却声称描述网关处置规则 |
| D-05 | 低 | `EventStore::apply` 全程不读 `event.execution_id`，归属全靠门面路由（已由 G5 钉住当前契约） |
| D-07 | 撤销 | 根 `progress.md` 进入 23 路径 diff —— 原判「与 AGENTS.md 冲突」系误判，AGENTS.md 已改口径明文允许；详见「操作纪律要点」 |

**D-01 的落地代价已被核实为小**：`ExecutionState → CancelDisposition` 映射全仓只有 `gateway::cancel::disposition_from_port_state` 一处，`CancelOutcome` 只有 2 个构造点且都走它 ⇒ 换成 registry 的 `CancelReceipt` 只改一个函数 + 6 处调用点用例，**不构成大返工**。且 `connection::port::CancelDisposition` 不跨 `SessionPort` 边界，生产路径无真实 transport 产出它。

修复轮裁决：本轮 gateway **不做变异**（验证归 Tester）、**不碰 `CancelReceipt`**（等两轨都过验收后在合并时统一切换）、G1–G5 五条用例**按语义就地落位**而非新开 `gap_probe` 文件，若 `invariants.rs` 枚举了测试文件集则同步更新该断言。目标门禁 lib **328**、`gateway_contract` **49**。

### gateway 轨第 2 轮验收 → `PASS（有条件）`（2026-10-05，已派第 2 轮修复）

被测提交 `de4d45749`（代码 `53bcaeeff` + 纯台账 `de4d45749`）。Tester 全程在自建的 `git worktree add --detach` 树里做变异，收尾核验 coder 树 `HEAD=de4d45749 DIRTY=0`，未执行任何 `git add`，三棵验证树已 `git worktree remove` 清理。

**五条门禁（收尾在干净树重跑，首尾各打一次印）：**

```
PRE  HEAD=de4d45749… STATUSLINES=0
G1 EXIT=0 | test result: ok. 328 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.21s
G2 EXIT=0 | test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
G3 EXIT=0 | WARNING_COUNT=0
G4 EXIT=0 | FMT_BYTES=0
G5 EXIT=0 | [check-platform-arch] PASS — 22 workspace member(s) classified, 6 rule(s) evaluated over 26 crate(s), 1 rule×subject combo(s) vacuous: 0 violation(s), 0 error(s), 3 advisory(ies)
POST HEAD=de4d45749… STATUSLINES=0
```

14 个回归基线逐个对应到二进制名精确匹配，无一回退（详见「基线不变量」）。

**非空洞性成立**：把 3 个新测试文件覆盖到 `19545268d` 的旧生产代码上 ⇒ `45 passed; 4 failed`。G1 红 `left: Queued / right: Succeeded`、G2 红 `left: 0 / right: 12`、G3 红 `left: Applied / right: IgnoredDuplicate { sequence: Counter(1) }`。`invariants` 那条红经 Tester 明确定性为 11 文件 vs 12 文件的机械差，**打折不计**；G4、G5 在旧码上绿符合预期（分别是覆盖缺口与非本轮缺陷），其效力由 M6/M4 证明。

**变异 7 条，6 条被杀，1 条存活。**

| 编号 | 变异点 | 结果 |
| --- | --- | --- |
| **M6**（上轮遗留） | `RequestFingerprint::of` 删掉 `source` 段 | **被杀** 契约 `48 passed; 1 failed`，红在 `idempotency::g4_…`，逐字「期望失败，实际成功」；lib 仍 328 绿 |
| M1 | `recover_from_snapshot` 退回「重建空水位」 | **被杀** lib `323 passed; 5 failed` + 契约 `46 passed; 3 failed` |
| M2 | 去掉 `context_revision` 单调守卫 | **被杀** 但**仅**由 1 条 `--lib` 用例杀，契约 49 全绿 ⇒ 契约层零覆盖 |
| M3 | `Conflict` 并进 `Hit` 分支 | **被杀** lib 1 红 + 契约 `47 passed; 2 failed` |
| M4 | `apply_event` 不看 `event.execution_id`，路由到首条记录 | **被杀** 契约红 `left: Gap{expected:Counter(3),received:Counter(7)} / right: Unbound` ⇒ G5 有真牙齿 |
| M5 | 幂等账本 `lookup` 的 `Err` 降级成 `Miss` | **被杀** lib `326 passed; 2 failed` + 契约 `48 passed; 1 failed`，三层全红 |
| **M7** | `accept` 的 Hit 分支回填伪造来源 | **存活** ⚠️ |

**M7 存活项（中危，纯覆盖缺口，当前无行为缺陷）—— 协调者已独立复核属实：**

- 代码事实：`mod.rs:233-237` 的 `IdempotencyLookup::Hit` 分支把 `&request.source` 传给 `GatewayAcceptance::replayed(...)`（定义在 `request.rs:192`）。
- 契约层对 source 的断言只有 `idempotency.rs:141`（构造冲突入参）与 `:151`（断言的是**账本里存的记录** `record(&h,&id).source()`）——**没有任何一条断言重发回执上的 source**。
- 当前为何不是活缺陷：`ExecutionSource` 只含可持久化字段，指纹覆盖 `serde(source.to_persistable_json())`，故「指纹相等 ⇒ 来源全等 ⇒ `&request.source` 与当初冻结的来源逐字节相同」；该前提已被 M6/G4 钉住。
- 为何仍须补：G4 钉的是**冲突规则**，不是**回执字段**。一旦将来有人既去掉指纹里的 `source`、又改掉 G4，回执会静默报出新来源而**一条用例都不会红**。

**第 2 轮修复范围（已派发，纯测试轮，`src/gateway/**` 生产代码一行不许改）**：M7 必做（补「重发回执 source 等于首次受理冻结来源」，并顺带钉住回执 execution_id）+ M2 副产物建议做（契约层补 revision 单调性用例，**不得改动已有的那条 `--lib` 用例**）。目标门禁 lib ≥ **328**、契约 ≥ **50**。

**静态审查结论**：生产代码 2270 行禁用构造 0 命中；U+FFFD 全仓 0；最大文件 543 行；`lib.rs` diff 恰一行；`060053afb..de4d45749` 全部在轨内，`connection/**`、`registry/**`、`Cargo.toml`、`docs/**` 各 0 命中；`GatewayError::Runtime` 真透明，5 类错误零重映射，契约层 6 条按变体精确等值断言；gateway ↔ barrier/FakeClock 零耦合（退出码 1）。

**两处如实记录的失准（非缺陷）**：

1. coder 称 `event_store_tests.rs` 是「纯搬家、一字未改」，实测 15 个原函数中 14 个逐字节相同、**1 个被刻意加强**（`a_snapshot_clears_the_gap_and_realigns_the_context_revision` 原断言的是缺陷行为 `Queued`，现改为断言 `Running` + `last_sequence()` + `row_count()` + 重复 chunk）、另新增 5 个。偏差方向是**加强**，且正好解释 323 + 5 = 328。判为措辞失准，**但后续台账必须区分「未改动 / 新增 / 修改」三类**，已写进第 2 轮 Coder 简报。
2. Tester 自己的变异脚本第一版写死了工作树路径，导致第一次「在 `19545268d` 上跑 M7」实际打到 `de4d45749` 那棵树，**该次读数无效**。发现残留 diff 后已 `git checkout --` 复原、改为环境变量传入，并在两棵树上各重跑一次；随后又在**干净树**上完整重跑五条门禁，确保头条证据未被污染。**验证方自纠入库留档。**

**D-06 确认不存在**：hub.md 缺陷表只有 D-01/02/03/04/05/07，全仓亦无此条目，Tester 未编造裁定。D-03 按协调者裁决推迟到合并时统一切 `CancelReceipt`（必须动 `connection/**`，属本轨禁改区）。D-07 维持撤销。D-01 / D-04 / D-05 判真关。

### gateway 轨第 3 轮验收 → `PASS`（2026-10-05）→ **已合并**

被测提交 `9872a8c5a`（父 `de4d4574`），本轮是**纯测试轮**：`git diff de4d4574..9872a8c5a -- packages/runtime/src/` = **0 行**。协调者独立复核与 Tester 一致（HEAD / 父 / 工作树 0 行脏 / `src/` 0 行 / `lib.rs` 1 行 / 两条新用例就位 / 契约 `51 passed` EXIT=0）。

**5 条门禁全绿**：lib `328 passed`、契约 `51 passed`、build 0 warning、fmt 0 字节、边界 `0 violation(s), 0 error(s)`。**14 个回归基线逐个对应二进制名，零回退。**

**4 条变异全部被杀（本轮唯一实质内容就是这 2 条新用例）**：

| 变异 | EXIT | 失败用例 | 逐字断言 |
| --- | --- | --- | --- |
| **M7** | 101 | `idempotency::a_replay_receipt_reports_the_source_frozen_at_first_acceptance` | `left: ExecutionSource { kind: Editor, source_id: "edt_tampered", organization_id: None, principal_id: None }` / `right: … source_id: "edt-contract", organization_id: Some(OrganizationId("org-contract")), principal_id: Some(PrincipalId("principal-contract"))` |
| **M7-变种**（`SourceKind::Job`，kind 与原值**不同**） | 101 | 同上 | `left: kind: Job, source_id: "job_tampered"` / `right: kind: Editor, source_id: "edt-contract"` |
| **M2-A**（守卫 → 无条件赋值） | 101 | `events::a_stale_snapshot_never_lowers_the_observed_context_revision` | `left: Counter(11)` / `right: Counter(30)` |
| **M2-B**（整条守卫语句**删除**） | 101 | 同上 | `left: Counter(30)` / `right: Counter(40)` |

**两条关键的交叉验证（Tester 自行追加，非 Coder 自证）：**

1. **M7 对全部既有用例隐形。** M7 变异下跑 `--lib` = `328 passed` EXIT=0 —— 该变异对 328 条 lib 与 51 条契约中的其余 50 条**完全不可见**。上一轮 M7 存活的原因由此被独立确证：缺的就是这条新用例。
2. **M2-B 的归属判定成立。** 定向跑两个同名用例：`--lib` 那条受保护用例在 M2-B 下 **EXIT=0 变绿**，只有新用例第 (3) 段（更新快照 40 恢复后必须是 40）杀掉形态 B，红在 `events.rs:175` `Counter(30)` vs `Counter(40)`。形态 A 红在 `events.rs:164`（第 (2) 段）。

**Coder 的 M2-B 主张经限定后成立，未夸大**：M2-B 并非只被新用例捕获，`--lib` 整体跑另有 2 条变红（`event_store_tests.rs:273` `left: Counter(2)` / `right: Counter(42)`、`cancel_event_tests.rs:406` `left: Counter(1)` / `right: Counter(11)`）。Coder 原话是「受保护的 `--lib` 用例 `event_store_tests.rs:296` 在形态 B 下会绿」，对**那一条**准确。新用例的增量价值是把「陈旧快照不得拉低」与「新快照仍须抬升」绑进**同一条**用例，使形态 B 无处藏身。

**非空洞性实验的正确读法**：把新用例覆盖到父提交 `de4d4574` 的生产代码上，51 条**全绿**——因为本轮 `src/` diff 为 0，两提交生产代码完全相同，新用例在父提交上必然绿。这不是空洞，而是证明「覆盖操作良构 + 唯一的价值来源就是变异杀伤力」，§4 即其直接证据。

**静态审查 8 项全部成立**，其中两项值得记档：

- **既有 49 条未被削弱，且是逐字节证明**：父提交 `idempotency.rs` 正文 143 行在新文件中构成完整且逐字节相同的 143 行前缀；`events.rs` 父提交正文 250 行尾部逐字节连续出现（offset 172），唯一增量是插入的 47 行新用例块。本轮仅 3 条 `-` 且**全为 `use` 改写**（原名字全部保留），Rust 中改写既有行必同时产生 `-` 与 `+`，函数改名必产生 `-` 行 ⇒ 无改名、无删除、无削弱。计数印证 49 → 51。
- **新增 2 处 `.expect()`**：沿用该套件既有约定（父提交契约套件已有 23 处 `.expect(`、10 处 `panic!`、2 处 `unsafe`、1 处 `sleep(`），`AGENTS.md` panic 政策明确豁免 `#[cfg(test)]`。判为**沿用约定，非缺陷**；若要契约层零 `expect()`，应作全套件统一整改，不宜单挑本轮。

**Coder 自报两处披露均核验为合理不予追责**：(b) 首次 lib 红在既有 barrier 竞态（`connection/` 本轮 0 命中，夹具一行未动，不可能由本轮引入）；(c) 首次 fmt 1467 字节已改正，复跑 EXIT=0 / 0 字节。历史那次运行的失败清单本身已不可复放，Tester 明确标注「未能独立证实」——**这类不可复现项不计入结论，但也不隐瞒**。

**变异工具阴性对照**：每次 `restore + touch` 后复跑必回到 `51 passed` EXIT=0，仓库内 `tampered` 残留 0 ⇒ 证明确实重编译，未复用变异二进制。若未 `touch`，还原后应仍为红——该对照使 `touch` 纪律从「口头要求」变成**有证据的规则**。

**Tester 纪律**：Coder 树全程只读（首尾 HEAD 与 STATUSLINES 一致），变异全在自建 `--detach` 树、脚本经 `WT` 环境变量传入（无写死路径），收尾 `git worktree remove` 2 棵并 `prune`。**缺陷清单：无。**

### registry 轨待裁定 5 条 → 协调者已答（2026-10-04）

1. `connection::port::CancelReceipt`（2 字段，缺 `state`）与 registry 的 `CancelReceipt`（3 字段，§7.6）同名不同形 → **两处都保留，不解冻 `connection/**`，registry 也不转出前者**。registry 现有做法（不转出，避免同名物同时进 prelude）正确。记入文档批次遗留。
2. `ExecutionState::as_str()` 不存在、registry 私有映射并用 `state_literals_match_the_serde_wire_casing` 钉住 → **接受现状**，解冻后再上提。
3. `hostRejected` 不在 `ApiErrorCode` 里，`fold_exit` 返回 `ExitProjection::NotOnTheWire` → **接受**。不许给 `ApiErrorCode` 加取值（`platform-api` 冻结面）；不许把宿主缺陷伪装成调用方可修正的派发前拒绝。
4. `HostRejected -> InvalidArgument` 与 `RuntimeEpochMismatch -> SessionNotFound` 的刻意分歧 → 已标注有用例，无动作。
5. `RegistryAuditEntry` 只能序列化不能反序列化（存 `&'static str`） → **本轮不动**，gateway 今天无消费者。将来若需要，方向是 registry 另提供 `String` 侧 DTO，**不是**解冻 `connection/**`。

### 协调者已裁定（各轨不得重新讨论）

1. §3.2 #5 registry **重导出** `SessionView`/`SessionHandle`，绝不重新定义。
2. gateway 模块路径是 `packages/runtime/src/gateway/`（`execution` 已被 `connection/execution.rs` 占用）。
3. 真实命令形状 `CommandCall { command: String, input: serde_json::Value }`。
4. tokio 不是禁用依赖（F-03 黑名单 = `axum`/`actix-web`/`warp`/`tonic`）。
5. D-05：`SessionPort` 返回 `RuntimeError` 而非 `PortError` 是有意分层，文档中出现 0 次；§4 的 11 个 port 全是跨边界端口。
6. D-01：必须用真实 `CancelReceipt` DTO 补齐，不得在 Wave 2 就地私造结构体绕过去。
7. D-02：epoch 不匹配必须只留一个对外错误码出口。

### 冻结面缺口分配

- **D-01** 归 `registry`：改 `registry/port.rs`、定义并导出 `CancelReceipt`。因 `connection/**` 在禁改区，`CancelReceipt` 落在 `registry/receipt.rs` 而非 `connection/execution.rs`。
- `gateway` 只消费 registry 导出的 `CancelReceipt`，**禁碰 `registry/**`**。

### 文件冲突面

`packages/runtime/tests/common/` 属目录轨、`tests/support/` 属预算轨，均已在 main 上。Wave 2 两轨只能新建 `tests/registry_fixtures/` 与 `tests/gateway_fixtures/`，不得触碰前两个目录，否则合并必冲突。

## 基线不变量（只许涨不许跌）

**以下数字以 gateway 验收方在 `060053afb` 基线上的独立复跑为准**（原记录 `12/7/10/6`、`7/4/6` 是转写错误，已作废）。逐条对应到二进制名，不再用「目录名 + 顺序」这种会串位的写法。

- main lib：**223**（硬地板）
- `budget_cm65_lifecycle` **8**、`budget_cm65_reservation` **2**、`budget_cm65_scheduling` **6**、`budget_cm66` **7**
- `directory_attachment_ttl` **12**、`directory_no_disk` **6**、`directory_ownership` **7**、`directory_replacement` **10**
- `resource_replacement` **4**、`resource_return_to_pool` **7**、`resource_rotation_and_disable` **6**
- `p3_session_port_contract` **10**

> 教训：**基线数字必须带产生它的命令 + 逐字结论行**。写成「目录 + 四个数」会被下一个接手的人读成另一种顺序，而 `rev-parse` / `is-ancestor` 全部放过。

## 待办与遗留

| 项 | 状态 |
| --- | --- |
| Wave 2 文档批次：`shared-boundaries-and-ports.md` §3.2 第 6 行 `execution` → `gateway`；记录 `SessionView`/`SessionHandle` 重导出而非重定义的规则；将 `SessionPort` 登记为内部接缝（非 §4 的 11 个跨边界端口） | 待 Wave 2 合并后由协调者执行 |
| P3 退出门禁对账（CM-02～07、20～32、37～39、54～56、60、61～74） | 待 Wave 2 验收后 |
| `cargo build --workspace` 仍有 58 条 warning（55 diagnostics + 3 cargo summary）。**不得**记作已清除 | 未处理 |
| `datazen-driver-redis` 的 `approximate_constant` ×2 在禁改区 | 已知遗留 |
| 候选门禁 `cargo clippy -p datazen-platform-api -p datazen-driver-api -p datazen-application -p datazen-ai-api --lib -- -D warnings`（已验证安全）可关闭一度藏过真实 lint 的范围缺口 | 未执行 |
| CM-72 是唯一零引用条目 | Wave 2 registry 轨负责 |
| 每驱动 `case_rules` 对 `Unknown` 的真值研究（sqlite 大小写不敏感导致 `Users`/`users` 两种缓存身份） | 用户裁定记为 P2 遗留 |
| `connection::testing::barrier` 夹具竞态（见下「夹具竞态裁定」） | 已定性为 Wave 1 既有缺陷；**不阻塞** Wave 2 合并，待独立小修 |
| M7：`GatewayAcceptance` 重发回执的 `source` 字段无用例钉住（M7 变异存活） | 已派第 2 轮修复补断言；属 D-02 的后半段 |
| M2 副产物：`context_revision` 单调性仅由 1 条 `--lib` 用例钉住，契约层零覆盖 | 同批在第 2 轮补契约用例 |

## 夹具竞态裁定（2026-10-05，协调者）

gateway 第 2 轮整改中，coder 主动披露 lib 门禁两次红在**同一处既有用例**：
`connection::testing::barrier::tests::await_drain_wakes_when_the_clock_crosses_the_deadline`
（`tests.rs:246` / `:249`），两次结论行均为 `327 passed; 1 failed; finished in 30.02s`。
该文件属 gateway 轨禁改区，coder 拒绝越界修，交回协调者。协调者独立复核如下。

**定性：Wave 1 既有缺陷，非 gateway 引入，不构成 gateway 合并阻塞项。**

| 证据 | 命令 | 逐字结果 |
| --- | --- | --- |
| 用例在 Wave 1 基线上已存在且**逐字相同** | `git show 060053afb:packages/runtime/src/connection/testing/barrier/tests.rs` | 与当前工作树同一段文本 |
| gateway 轨从未碰过该目录 | `git log 060053afb..feature/p3-gateway -- packages/runtime/src/connection/testing/` | **0 行** |
| gateway 侧零引用 | `grep -rn "DrainBarrier\|FakeClock\|testing::barrier\|testing::clock" src/gateway tests/gateway_contract tests/gateway_fixtures` | **0** |
| 引入者 | `git log --diff-filter=A -- …/barrier/tests.rs` | `2334b7d3a fix(fake-runtime): F 编号对齐文档、barrier 拆分、真实竞态与 CM-74 顺序钉住` |

**根因（可从代码直接读出，coder 的分析与此一致）：**

1. `FakeClock::arm`（`clock.rs:169-182`）以 `armed_at_nanos = state.nanos`（**当前假时间**）记起点。
2. `arm_drain_and_check` 在 `drain.rs:312` 被调用，位于 `await_drain` 内部，而 `await_drain`
   由 `tests.rs:246` 的 `std::thread::spawn` 在**另一个线程**执行。
3. `tests.rs:246` spawn 之后，`tests.rs:248` **不等等待线程装填 timer** 就 `clock.advance(10s)`。
4. 线程若未跑到 `drain.rs:312`，`armed_at` 被记成已被推进后的 10s，期限在本次 `advance` 中被跳过，
   此后不再有推进 ⇒ 等待线程永久阻塞。
5. 兜底是 `barrier/mod.rs:41` 的 `BLOCK_REAL_TIME_BUDGET = Duration::from_secs(30)`（真实单调时刻，
   与 `FakeClock` 无关）。耗尽后 `wait_until` 交还 `Err(guard)`，调用点带 `file:line` panic
   ⇒ 失败耗时 **30.02s**，与实测精确吻合。

**修法方向（留档，待独立小修时执行；两处择一或并用）：**

- 让等待线程先装填再推进时钟（在 spawn 后加装填握手，而不是靠线程调度碰运气）；
- 或让 `arm` 按**已流逝假时间**结算起点，而不是拿当前假时间当起点。

**纪律：** 不得用「多跑几次」或调大 30 秒预算掩盖。`30s` 预算的价值正在于此条文档
（`barrier/mod.rs:34-46`）明写的「把挂死变成指名道姓的失败」；调大它等于把这条兜底拆掉。


## Wave 1 遗留建议（不阻塞，记录备查）

- `resource/src/adoption.rs` 命名不在 §3.2 词表内，建议合并/重命名为 `replacement.rs`
- CM-37「权限不跨缓存」覆盖偏薄
- resource 用内联 fixture，directory 用 `test-harness` dev-deps，两者不一致
- resource 的 CM-68/69/73 缺真实驱动 E2E
- D-2：`FORBIDDEN` 子串表漏掉 `tempfile` / `fs::copy` / alias import（已接受为低危）

## 操作纪律要点

- 同一棵工作树不得同时被提交方和验证方使用；Tester 的变异必须在 `git worktree add --detach` 的独立工作树里做。
- **还原变异必须验 mtime，不能只验内容哈希。** `shutil.copy2` 会连带还原 mtime，cargo 判定源文件「新鲜」、**不重编译、直接复用被变异的测试二进制** → 门禁复跑出现与变异毫不相干的假红/假绿。还原后用 `os.utime` 强制 touch 全部 `src/**/*.rs` 再跑。
- **提交方不得在同一棵树上做变异**（coder 上轮违反过，只靠临时备份 + sha256 + HEAD 指纹兜底）。验证证据的可信度不能靠提交方的自报。
- 门禁重跑首尾各记录一次 HEAD 与工作区 sha。
- 长输出（测试命令）落系统临时目录再取结论行，退出码单独打印。
- 绝不读取或打印 `.env` / `.env.test` 内容（程序读文件合法，内容回显进上下文才违法）。
- 编译产物放各自 worktree 内 `target/`，合并后随 worktree 清理。
- **每轨分支根目录的 `progress.md` 与集成分支的 `hub.md` 已获 AGENTS.md 明文授权**（见 AGENTS.md「进度台账：开发期间允许，交付即销毁」）：开发期间允许、可提交入库，**验收合并时必须删除，不得存活在 `main`**。gateway 验收方把它列为 D-07 一节，属误判，现予撤销。
- 断言一旦红，先怀疑实现而不是改断言——「自己把用例写红」的自纠记录是这一轮最有价值的产出之一。
- **「跑一次门禁就绿」对任何轨都不足以作为证据**（2026-10-05 新增）。barrier 夹具竞态使 `cargo test -p datazen-runtime --lib` 存在约 **12.5%** 的偶发红概率。Tester 三点实测：基线 `060053afb` 5/40 = 12.5%、`19545268d` 2/15 = 13.3%、`de4d45749` 2/15 = 13.3%，三次失败逐字同形。按 13% 概率连过 6 次的自然概率约 **43%** ⇒ 任何「我连跑 N 次全绿」的自报都不构成证据，需要连跑到统计上说得过去，或用变异实验/非空洞性证明代替。
- **变异脚本不得写死工作树路径。** 第 2 轮 Tester 的脚本第一版写死路径，导致一次「在旧提交上跑变异」的读数无效；须参数化并在多棵树上各跑一次，收尾核验 `POST DIRTY=0`。
- **「纯搬家 / 一字未改」这类描述要逐函数核对。** coder 报 15 个函数一字未改，实测 1 个被刻意加强、另加 5 个。方向虽是加强，但下次台账须区分「未改动 / 新增 / 修改」三类。