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
3. ~~`hostRejected` 不在 `ApiErrorCode` 里，`fold_exit` 返回 `ExitProjection::NotOnTheWire`~~ → **⚠️ 本条裁定有误，2026-10-05 由 registry Coder 当面纠正，协调者认领。** 冻结的 `packages/runtime/src/connection/error.rs:136` 写死 `ProviderError::HostRejected(_) => ApiErrorCode::InvalidArgument`，故 `fold_exit(Provider(HostRejected))` 实际返回 **`Code(InvalidArgument)`，是上线路**，`NotOnTheWire` 只留给 `CancelFailed` / `InvariantBroken` / `ProtocolError` / `SqlError` / `Timeout` / `ResourceLost`（`epoch.rs:129/132/144/149`）。教训见文末「冻结面事实必须当场读码核实」。Coder 按「不改行为」保留生产代码、只把**真正成立**的性质（`"hostRejected"` 这个 ExecutionErrorCode 写法不是任何 ApiErrorCode 的线格式字面量）钉成断言——**处理正确**。
4. `HostRejected -> InvalidArgument`（上线）与 `RuntimeEpochMismatch -> SessionNotFound`（经 `fold_exit` 归一）的刻意分歧 → 已标注有用例，无动作。
5. `RegistryAuditEntry` 只能序列化不能反序列化（存 `&'static str`） → **本轮不动**，gateway 今天无消费者。将来若需要，方向是 registry 另提供 `String` 侧 DTO，**不是**解冻 `connection/**`。

### registry 轨第 1 轮交付 → 协调者对 7 条待裁定逐条已答（2026-10-05）

Coder `cc6b4cab` 交付 3 提交 `6f5027eee` / `b51b03622` / `b57efab2e`，HEAD `b57efab2e`（基点 `060053afb`），工作区干净。自报 lib `272 passed`（= 223 + 49）、build 0 warning、fmt 0 字节、边界门禁 0 violation、集成 7+6+9+10+11 = 43 例全绿。Coder **7 条全部不自行裁定、上交协调者**——纪律正确。

**规模**：19 个 Rust 文件共 7231 行，`actor.rs` 734 行为最大，**全部 ≤ 800 行**。actor 内单测 1581 + 集成 1204 + 夹具 579。

| # | 事项 | 裁定 |
| --- | --- | --- |
| 1 | 协调者原裁定 #4 与实现不符 | **协调者认领错误**，见上节已订正。Coder 处理正确，不追责 |
| 2 | `emit` 给每种审计条目都填 `capability_versions` | **接受**。统一从 `state.physical` 快照填是自洽的，无需改行为 |
| **3** | `release()` 不可判定时登记表行 + 额度双泄漏 | **判为缺陷 R-01，必须修**（详见下） |
| 4 | `invalidate_worker` 成功分支发两条 `SessionInvalidated` | **交 Tester 取证后定**：两条逐字 dump，若除自增 id 外全等 ⇒ R-02（低）；若承载不同事实 ⇒ 保留但改名。协调者不预判 |
| 5 | 重复登记只在 `write_table().insert` 处被拒（物理 open 已发生） | **接受现行为**。额度确实退回，open 后随即关闭，是 TOCTOU 的诚实代价，优于 check-then-act。**但必须钉住「刚 open 的物理资源确实被关闭」**，用后端 open/close 计数对账 |
| 6 | 简报与仓库实况不符（23 条非 14 条；`mod.rs` 已声明；`cancel.rs` 文档表格写了一条从未存在的分支顺序） | **按实况处理正确**。Coder 选择改文档而非改行为，且只重写表格匹配代码、零行为变更——已要求 Tester 独立验证「确实零行为变更」 |
| 7 | 工作树 `AGENTS.md` 仍是旧版「不写进度台账」 | **按新版执行**。`progress.md` **由协调者在合并时删除**，Coder 不删（Tester 需读它作线索） |
| 8 | 首轮门禁指纹不可用 | **教训成立，已收录文末** |

**变异归属**：Coder 因根 `AGENTS.md`「同一棵工作树不得同时被提交方与验证方使用」把 CM-72 / CM-24 / D-02 三项变异上交，**判断正确**。变异归验证方，已并入第 1 轮 Tester 简报。Coder 无需补做。

**R-01 判为缺陷的依据**（协调者读码得出，已要求 Tester 独立复核）：

- `registry.rs:563-564` 注释自述：「关闭无论走到 `Closed` 还是 `Lost`，会话都已从登记表注销，额度都该归还——`Lost` 丢的是物理资源，不是额度账」。
- 但 `close_registered` 的 `Err` 臂（`:570`）只正确覆盖了 `CloseRejected`（关闭**未发生** ⇒ 留行留额度，正确），却把 `release()` 在跑完 §9.4 四步**之后**返回的 `SessionLost`（关闭**已发生**）一并吞掉 ⇒ 不调 `forget()`。
- 此时 actor 侧已彻底拆解：`release.rs:112-123` 令绑定失效、`physical = None`、`deferred.clear()`、状态 `Lost`。
- 证据**已经**由 `release.rs:130-140` 的 `Outcome::Undecided` 审计条目留住 ⇒ 留行不留任何额外证据，只漏一行 + 一个额度位，而调用方**无任何回收路径**。
- **代码与自己声明的意图相悖 ⇒ 缺陷，不是「刻意留证据」。** 修复方向：区分两类错误，`CloseRejected` 保留行与额度，后置 `SessionLost` 必须仍 `forget()`；**返回给调用方的错误不得改变**（调用方依赖 `SessionLost`）。

R-01 若经 Tester 复核成立且未修，本轮判 `FAIL`。

**registry 分支基线缺口**：其基点 `060053afb` **不含 gateway**，故 `gateway_contract` 在该树上不存在（不是 0）。合并后应为 lib **377**（main 328 + registry 49）、`gateway_contract` **51**。已要求 Tester 在自建 `--detach` 树里做 `git merge --no-ff main` 试跑，预期冲突面只有 `packages/runtime/src/lib.rs`（gateway 加了 `pub mod gateway;`）。

### registry 轨第 1 轮验收 → FAIL（Tester `a92775f2`）

**2 blocker + 1 确认缺陷 + 1 确认低缺陷。** Coder 树首尾指纹一致（`b57efab2e` / 0 / `da39a3ee`），变异全在自建 detach 树、收尾 3 棵全清 EXIT=0。**Coder 全绿自报不可信，但也未欺瞒**——它确实跑了清单内的全部命令，问题出在清单。

**BLOCKER-1 —— 分支改爆自己的冻结契约套件**

```
cargo test -p datazen-runtime --test p3_session_port_contract   → EXIT=101
error[E0053]: method `cancel_execution` has an incompatible type for trait
  --> packages/runtime/tests/p3_session_port_contract.rs:75:1
     expected `CancelReceipt`, found `ExecutionState`
```

基线 `060053afb` 实测 `test result: ok. 10 passed` EXIT=0。**10/10 绿 → 编译不过。**

**BLOCKER-2 —— 合并 main 产出整棵编译不过的树，且零冲突**

`git merge --no-ff main` MERGE_EXIT=0、`--diff-filter=U` 为空、`lib.rs:19/23` 两个 `pub mod` 都在，但**每个二进制 EXIT=101**：

```
error[E0308]: mismatched types
   --> packages/runtime/src/gateway/mod.rs:441:37
441 |  record.last_state = state_after;
    | expected `ExecutionState`, found `CancelReceipt`
   （另有 :446:41、:447:13；disposition_from_port_state 在 gateway/cancel.rs:185）
```

**协调者预判的冲突面（`src/lib.rs` 的 `pub mod gateway;`）是错的。真正的冲突面是语义破坏，`git merge` 看不见。** 合并后 377 / 51 因此是**不可达**，不是数字错。

**同一根因**：分支改了共享接缝 `SessionPort::cancel_execution` 的返回类型 `ExecutionState → CancelReceipt`（`registry/port.rs:96`），**冻结基线测试与 gateway 轨两个消费者都没跟着改**。已核实：`SessionPort` 5 个方法里**只有它改了签名**，其余 4 个未动。

**协调者裁定（本轮唯一设计决定）**：

- `SessionPort::cancel_execution` **回退为 `Result<ExecutionState, RuntimeError>`**。冻结测试写死了签名（`:104-113`）与返回态（`:318-323` 断言 `CancelRequested | Cancelled`），**冻结契约是仲裁者**。实测 `git diff --name-only 060053afb..b57efab2e -- .../p3_session_port_contract.rs` 输出为空 ⇒ **Coder 没有把契约改成迎合实现，这是对的，不得回改冻结测试。**
- `CancelReceipt` **不消失**：`registry.rs:609` 的 facade 方法本就返回 `Result<CancelReceipt, RuntimeError>`，保持不变；trait 实现内部调 facade 再投影 `Ok(receipt.state)`。D-01 由此**真关**（真 DTO、真逻辑、导出、facade 可达）；trait 是**内部接缝**，不是 §4 的 11 个跨边界端口，内部接缝可以保持窄。
- D-01 的 **gateway 侧消费 = D-03，仍 OPEN**，是 gateway 的活（`cancel.rs:185` 1 函数 + `mod.rs:441/446/447` 3 处）。**registry 禁碰 `gateway/**`**，D-03 排在 registry 合并后另派。

**缺陷清单**：

| 编号 | 位置 | 结论 |
| --- | --- | --- |
| B-1 / B-2 | `registry/port.rs:96` | 签名回退；facade 不动 |
| **R-01** | `registry.rs:567-569` | **Tester 独立复核确认成立**。链路：`registry.rs:567-569`（sink）← `actor.rs:472` 原样转发 ← `release.rs:127-133` 已 `invalidate_bindings()`/`physical=None`/`state=Lost` ← `release.rs:141` 返回 `Err(SessionLost)`。`registry.rs:562-564` 注释自述相反意图。留行不留证据（`release.rs:134` 的 `Outcome::Undecided` 已留），只漏一行 + 一个额度位，调用方无回收路径。**修法**：`CloseRejected` 保留行与额度，后置 `SessionLost` 仍 `forget()`；**调用方可见错误不得改变** |
| R-02 | `release.rs:182-187` + `actor.rs:621-628` | Tester 取证：两条 `AuditFacts` **逐字段全等**（`AuditFacts::none()` 在 `actor.rs:700-710` 已置 `handle_count: 0`），仅 `emit()` 自增 `id` 不同 ⇒ 确认为重复，删其一 |

**本轮全绿的部分（同样要记在台账里）**：§3 门禁全部复现；`+49` 对账**精确**（`223 → 272`，`registry::` 命名 lib 测试 `3 → 52`）；§5 静态 6 项全过（禁改区 0 命中、最大文件 `actor.rs` 734 ≤ 800、全仓 0 个 U+FFFD）；**4 条指定变异全杀、10/10 阴性对照绿**（CM-72 / CM-24 / D-02 / D-01）；§8 审计 drain 为**读而不消费**，且**有鉴别力**（改成消费式会让 7 条测试变红）⇒ 非空洞。

**Tester 主动标记的诚实空白**（未确认，不得当结论用）：R-02 是**源码结构比对**非运行时 dump；裁定 5 的**后端 open/close 计数未测**（重复登记的物理连接是否真被关闭，仍无连接泄漏证明）；裁定 2 未穷举；裁定 6 的零行为变更未独立验证；配额守恒只验**串行**未验并发。

**Tester 的一条覆盖提醒**：CM-24（伪造绑定）与 D-02（epoch 不一致）**只被 `--lib` 单测杀掉**，对应集成二进制全程绿。看着像重复、实际不是，**不得删除**。

**协调者认领的流程错误**：给 Coder 的门禁清单**只有 `--lib` + 5 个本轨二进制 + build/fmt/边界，漏了 12 条冻结基线**，所以 BLOCKER-1 从未进入 Coder 视野。Coder 如实跑了清单内全部命令、如实上报，**责任在清单不在执行**。本轮起门禁清单全文下发，12 条基线一条不落。

### registry 轨修复轮 1 → READY_FOR_TEST，Tester 第 2 轮已派

新 HEAD **`cd7aa906e7`**（修复在 `23bf4a46d`，台账在 `cd7aa906e`）。协调者已独立核实：STATUS=0、`git diff HEAD | shasum` = `da39a3ee`（空树）；`git diff --name-only 060053afb..HEAD -- p3_session_port_contract.rs connection/ gateway/` **输出为空** ⇒ 冻结测试与禁改区未被迎合；trait 签名确已回正为 `Result<ExecutionState, RuntimeError>`（`port.rs:96`）。

**四条缺陷的声称修法（Coder 自报，第 2 轮 Tester 待验）**：

- **B-1/B-2** —— trait 签名回正；`CancelReceipt` 三字段落到 `SessionRegistry::cancel_registered` 具名入口，`cancel_execution` 只做 `receipt.state` 投影。`port.rs:145` 加**编译期钉子** `frozen_port_cancel_shape`。**须验钉子是真闸门还是装饰**（改回签名是否真编译不过）。
- **R-01** —— `close_registered` 按失败形状分流：`CloseRejected`（派发前拒绝、物理资源未动）留行留额度；`SessionLost`（§9.4 四步跑完、物理已关）**必须 `forget()`**；错误原样传调用方。
- **R-02** —— 删 `invalidate_worker` 里的重复 emit，改由 `release` 单点发出；称守卫已排除 `physical.is_none()` 提前返回、`RollbackAndClose` 也不给 `CloseRejected`，故无审计空洞。
- **D-01** —— 三字段回执保留在 `registry/receipt.rs`，`mod.rs:73` 导出，经 `cancel_registered` / `cancel_execution_bound` 可达。gateway 侧投影仍是 **D-03**。

**Coder 主动报告的「覆盖迁移」（第 2 轮重点核查）**：CM-73 `宿主登记数与后端确认数对不上时墓碑不得报已关闭` 原经 `registry.session_view` 读登记表墓碑，R-01 修好后行被摘除、集成面读不到（与成功关闭后同形），**断言被移到** `actor/tests/release.rs:49-60`。**须独立判定：覆盖是平移了，还是被稀释成了更容易通过的东西。**

**Coder 主动上交的未关闭项**：actor 消失（`finish()`/`ActorGone`）后登记表会永久保留其行与额度，`close_registered` 够不到 `SessionClosed`。**与 R-01 同族（额度泄漏），须判公开 API 可达性**——不可达=潜在缺陷，可达=当前缺陷。

**Coder 自报门禁（第 2 轮必须独立复现，不采信）**：冻结基线 12 支全 EXIT=0；`--lib` 272；build 0 warning；fmt 0；边界 0；5 集成二进制 7/6/9/**11**/11；合并演练（对 main `158e058e0`）EXIT=0 零冲突、合并树 `--lib` **377**、`gateway_contract` **51**、19 支二进制逐支 EXIT=0。

### registry 轨第 2 轮验收 → PASS（Tester `7758907f`）+ 协调者独立复现

Tester 判 **PASS**，8 项条件全满足。**协调者未采信自报，自建 detached 树复跑了核心门禁**（第 1 轮翻车恰在此项）：

```
git worktree add --detach /tmp/dz-verify-registry cd7aa906e7   ADD_EXIT=0
git merge --no-ff main（main=448e073f1）                          MERGE_EXIT=0
git ls-files -u | wc -l                                           0（零冲突）
--lib              test result: ok. 377 passed; 0 failed; … finished in 0.21s   EXIT=0
18 支集成二进制    NONZERO_EXIT_COUNT=0
  gateway_contract          51    p3_session_port_contract     10
  registry_lifecycle 7  registry_execution 6  registry_cancel 9  registry_release 11  registry_audit 11
  budget_cm65_* 8/2/6  budget_cm66 7  directory_* 12/6/7/10  resource_* 4/7/6
```

**「19 支」的口径差异已查清，不是漏跑**：合并树 `tests/*.rs` 确为 **18** 个，`Cargo.toml` 无显式 `[[test]]` 段即目标全量；Tester 的 19 = 18 集成 + `--lib`。**第 1 轮那类「零冲突 + 整树 EXIT=101」的语义破坏已消失。**

**四条缺陷均已真修且反向验证变红**：

| | 修法（Tester 复核） | 钉住它的测试 |
| --- | --- | --- |
| B-1 | `port.rs:94-98` 签名回正 `Result<ExecutionState, RuntimeError>` | `registry_cancel.rs:156-175` 经 `Arc<dyn SessionPort>` 驱动 |
| B-2 | 合并树 19/19 全 EXIT=0 | 见上表 |
| R-01 | `close_registered` 的 `Err` 臂按失败形状分流：`if matches!(error, RuntimeError::SessionLost(_)) { self.forget(&record.db_session_id); }`，错误原样返回未改写 | `registry_release.rs:303-348`，**重新登记一次并观测额度落到 `SESSION_LIMIT-1`**——证明额度真被归还，而非仅行消失 |
| R-02 | 单点发出确认，`invalidate_worker` 无 `SessionInvalidated`、只有 `QuotaHeldStale` | `registry_release.rs:435-445` 断言 `len == 1` + `runtime_epoch` |

**CM-73 覆盖迁移 —— Tester 裁定「平移，未稀释」**，判据是可复核的硬证据：`git diff 6f5027eee..HEAD -- actor/tests/release.rs` **输出为空** ⇒ :49-60 是**逐字节未改动的既有代码**，不是被改弱的重写；它同时断言 `state == Lost` **与** `transaction_state == Unknown` 两半，且对 M4 变异（把状态压平成 `SessionState::Closed`）**变红**（`--lib` `FAILED. 270 passed; 2 failed`）。覆盖现按两个属主各守一半：actor 保墓碑可读，登记表断行已摘（`registry_release.rs:271` 改断 `session_view(...).is_err()`）。**Coder 的论据在实质上成立。**

**Coder 上交的「actor 消失后行与额度泄漏」—— Tester 判 LATENT，非当前缺陷，并订正了我自己的初步看法**（我先前倾向「可达 ⇒ 当前缺陷」）。依据是类型结构而非运行时观察：`SessionRecord:74-80` 按值持有 `actor: SessionActor`，而 `SessionActor` 持有 `exec_tx` 的 `UnboundedSender`；run loop 只在 `exec_rx.recv()` 返回 `None` 时退出，即**所有 sender 全被 drop**。行在表里 ⇒ registry 自己就是活着的 sender ⇒ actor 不可能退出 ⇒ `finish()`/`ActorGone` 对已登记会话**结构上不可达**。另：「行被摘 ⇒ 额度已结」在**全部 2 处**摘除点成立（`forget():446`→`release():448`；`invalidate_worker:470`→`hold_stale():476`），`evict_idle_at:435` 转调 `forget()`，陈旧侧有公开出口 `confirm_worker_quarantined`。

### 第 2 轮遗留（不阻塞合并，随合并带走）

**两个 low，Tester 只报未修**：

- **D-R2-1（low，文档失真）** `registry/port.rs:70-74` / `:136-143` —— 编译期钉子 `frozen_port_cancel_shape` 的文档**夸大覆盖范围**。Tester 实测：只动钉子时 `cargo build -p datazen-runtime` = **EXIT=0** `Finished dev profile in 2.50s`，而 `cargo test --lib` = EXIT=101（E0599 + E0308）。**准确表述：该钉子只闸住 test-profile 构建，闸不住 `cargo build`/check/release。** 修法：把形状钉子提到 `#[cfg(test)]` 之外（它只需要生产 trait），或改正文档。
- **D-R2-2（low，潜在）** `registry/registry.rs:463-473` —— `invalidate_worker` 用 `let _ = actor.control(…)` 丢弃投递结果，随后 :470 **无条件**摘行。若 actor 任务已死，物理资源永不回滚且无重试句柄存活。窄（registry 生产代码无 panic 族），且可经 `QuotaHeldStale`/`Undecided` 观察。修法方向：摘行前先接住投递结果。**注**：Tester 对 actor 不可退出的结构性论证同样适用于此处，此项大概率与 D-R2-2 的可达性判断同源，**合并后可一并复核**。

**Tester 明确标注未确认的项（不得当结论用）**：`--lib` 连绿 6 次在 12.5% 偶发率下自然概率仅约 **46%**，**既不能证明偶发已消失、也不能证明其存在**，第 1 轮那条 `finished in 30.01s` 超时特征始终未复现；CM-24 伪造绑定**仅由 `handles.rs` 单测杀掉**，`--test registry_cancel` 返回 0 红，**集成面无对应用例**（冻结端口测试按构造无 cancelHandle，读作分层正确，但确实缺集成覆盖）；CM-72 本轮变异得 3 红、第 1 轮报 7 红，**变异强度不同、两者都能杀，未对账**；**只跑了 `-p datazen-runtime`**，未跑其他 crate 测试，**未跑 clippy**。

**Coder 台账中须随 `progress.md` 一并转出的活口裁定项**（该文件按 AGENTS.md 于合并时删除）：

1. **冻结面同名物**：`connection::port::CancelReceipt`（2 字段，缺 `state`）与 `registry::CancelReceipt`（3 字段）同名不同形。registry 未把前者转进 `registry` 命名空间，避免同名物同时进入一个 prelude。**待裁定**：Wave 2 收敛前者（需解冻 `connection/**`），还是两处并存并在调用点显式区分。
2. **`ExecutionState::as_str()` 不存在**（`connection/execution.rs` 只有 `ExecutionErrorCode::as_str`/`EffectOutcome::as_str`/`TruncationReason::as_str`）。`connection/**` 禁改，故 `registry/audit.rs` 私有映射了一份，用 `state_literals_match_the_serde_wire_casing` 钉在 serde 输出上防漂移。**若解冻 `connection/**`，这份映射应上提。**
3. **`RegistryAuditEntry` 只能序列化、不能反序列化**（存 `&'static str`，线上字面量必须与调用方逐字同源，不能退化成可随手写错的 `String`）。**代价**：gateway 若要从线上 JSON **重建**审计条目，需裁定解冻方向——允许 registry 提供 `String` 侧 DTO，或给 `connection/**` 加字面量常量。**这是 D-03 的隐藏前置。**
4. `hostRejected` 不在 `ApiErrorCode`、两处 `fold_exit` 刻意分歧 —— 已裁定接受，无需再议。
5. **D-03 仍 OPEN**（gateway 侧）—— **⚠️ 上面第 5 条的架构顾虑已被证伪，见下方「D-03 定案」一节**。

### ✅ registry 轨已合并并清理（main `7fb290394`）

```
git merge --no-ff feature/p3-registry   MERGE_EXIT=0   UNMERGED=0
合并规模（不含 hub.md）  22 files changed, 8284 insertions(+), 56 deletions(-)
git rm progress.md → 7fb290394  「交付即销毁，结论已转入 hub.md」
```

**台账已销毁**：`git ls-files | grep -E '^(progress|hub)\.md$'` 只剩 `hub.md`（P3 退出时删）。历史仍可追溯（台账本就允许提交入库）。

**main 上的合并门禁，首尾指纹证明期间无人动过**：

```
HEAD_START=7fb2903949fe…  STATUS_START=0
pnpm typecheck        TYPECHECK_EXIT=0
pnpm vitest run       VITEST_EXIT=0    Test Files  565 passed (565) / Tests  5920 passed (5920)
HEAD_END  =7fb2903949fe…  STATUS_END  =0
```

**清理**：`git worktree remove` ×2（registry 交付树 + 我的验证树）EXIT=0、`prune` EXIT=0、`git branch -d feature/p3-registry` EXIT=0、`/tmp/dz-target-p3-p3-registry` **909M** 已删、`git worktree list` 只剩 main、`git branch --list 'feature/*'` 为空。

**至此 P3 已合并 2 轨**：`p3-gateway`（`c8daaf7c0`）+ `p3-registry`（`8751f0fb4`）。

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

### 三条新纪律（2026-10-05，CM-74 交付过程暴露）

**① Tester detach 之后，Coder 不得 rebase / amend / force-push。** Tester 从某个**哈希**开树，不是从某个分支头。改写历史会换掉那个哈希，**Tester 手上的树被悄悄换底，而它验的是它以为的那份代码** —— 且它无从察觉，因为文件内容可能看起来一样。本轮 CM-74 Coder 在 `main` 前进后**主动不 rebase**，理由正是这个；实测 `git diff --stat 712b43012 HEAD -- packages/` 为空（只有 `progress.md` 变了 254 行），故 Tester 那轮门禁仍描述同一份代码，无需重跑。

**② 已被协调者或他人引用的提交，不得 amend。** 改写它等于让记录失效，而失效是静默的。同理，**台账里不许写死自身提交号**——自指值每次 amend 都变，记下来必是死值。正确写法：写死**代码**提交号，自身标为 `HEAD`。

**③ 台账在「待独立核实」的条目上用登记语气，不用断言语气。** 缺口还没被验证时，台账写「本条只登记缺口事实，**不预判**缺口是否成立」，否则下一个只读台账的人会把「**我说的**」当成「**查过的**」。台账与报告都是二手证据；两者都应显式标注哪些是亲验、哪些是转述。

附带一条通用后果：**本轨落后 `main` 2 个提交是安全的**（落后本身无碍），但**多轨并行时 `packages/runtime/Cargo.toml` 是高概率冲突点**（CM-70 已加 hmac/sha2/subtle，CM-60 可能再加依赖），合并演练时优先盯它。

### 当前权威基线（2026-10-05，`cargo test -p datazen-runtime`，20 个二进制）

**作废一个流传中的错数：`552`。** 该数是某轮 Coder 的**心算错误，不是任何工具的读数**，从未进过本文件，却已口头传给多轨。**以 `TOTAL=562` 为基线。**

| 二进制 | 基线 | CM-74 后 |
| --- | --- | --- |
| `src/lib.rs` | **382** | **384** |
| `budget_cm65_lifecycle` / `reservation` / `scheduling` | 8 / 2 / 6 | 8 / 2 / 6 |
| `budget_cm66` | 7 | 7 |
| `directory_attachment_ttl` / `no_disk` / `ownership` / `replacement` | 12 / 6 / 7 / 10 | 12 / 6 / 7 / 10 |
| `gateway_contract` | **51** | 51 |
| `p3_session_port_contract`（冻结） | **10** | 10 |
| `registry_audit` / `cancel` / `execution` / `lifecycle` / `release` | 11 / 9 / 6 / 7 / 11 | 11 / 9 / 6 / 7 / 11 |
| `resource_replacement` / `return_to_pool` / `rotation_and_disable` | 4 / 7 / 6 | 4 / 7 / 6 |
| Doc-tests | 0 | 0 |
| **TOTAL** | **562** | **564** |

差 `+2` 恰为 CM-74 的两条新测试，其余 19 支逐项一致 ⇒ CM-74 轨未删改任何测试。**注意 registry 五支的顺序是 `audit/cancel/execution/lifecycle/release`，不是 `cancel/audit/…`** —— 顺序写错会让下一个接手的人套错基线。

**★ 本表在 `9090c123f` 复核，仍然是 main 的当前基线，一个数都不用改**：`--lib` **384**、TOTAL **564**、20 个二进制，逐条与本会话在 `6c6f9f240` 的实测吻合（`bash-1770` C 段 / harness-split 合并彩排）。

**★ 不要再「顺手订正」下面这些数，它们不是过期值，是钉在提交上的历史读数**：
- `:604` / `:637` 的 `--lib` **380** —— 标题就是「Coder 自报门禁（**待 Tester 独立复核，自报数不作为验证**）」，上方表格逐条钉着 `afa22c7a6` / `141c8abf9`。
- `:775` 的 `--lib` **382** —— 标题是「变异实测（Tester 独立 detached 树，**负控先行**）」，是那次变异的负控读数。
- `platform-development-plan.md:341` 的 `--lib` **139** —— 原文自带「计数与退出码**测于 `5b7c5b49c`**」。

**带测量点/提交号的数字是关于那个提交的事实，永远不会「过期」；不带锚点的当前断言才会骗人。** 判据是「这句话在主张什么」，不是「这个数还等不等于今天的数」。

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
- **冻结面事实必须当场读码核实，不能凭记忆下发裁定**（2026-10-05 新增，代价真实）。协调者裁定 #4 断言 `HostRejected` 不上线、`fold_exit` 返回 `NotOnTheWire`；registry Coder 读 `connection/error.rs:136` 后发现冻结写死 `HostRejected -> InvalidArgument`，该错误**确实上线**，裁定与实现相反。Coder 未自行改动生产代码，只把**真正成立**的性质钉成断言并上交——这是正确处置。**教训**：凡裁定涉及冻结面的具体映射/取值/返回路径，下发前必须 `grep` 一次原文并在裁定里附上文件:行号；错裁定已就地划改并保留原文，不静默删除。
- **未提交状态不存在可靠的工作区内容指纹**（2026-10-05 新增）。registry Coder 首轮门禁指纹失效：`git write-tree` 写的是**索引**，未 `git add` 的新文件不在其中；`git diff` 只覆盖已跟踪文件。拿任一者与 `HEAD^{tree}` 比必然对不上（当日 21 个新文件未入库）。**结论**：门禁证据一律取「**已提交 + 干净树 + 重跑**」那一轮——工作区干净时，跑的内容按构造就是提交进去的内容。
- **「同一棵工作树不得同时被提交方与验证方使用」会让 Coder 主动上交变异任务，这是正确行为不是偷懒**（2026-10-05 确认）。Coder 发现自己既是提交方又被指派变异，两份指示冲突，选择**上交裁定而非绕过**——须在简报里明确把该轨的 CM/条款变异整体划给 Tester，否则会僵持。
- **改共享 trait 的方法签名，必须当场预演它的编译后果**（2026-10-05 新增，代价是 2 个 blocker）。registry 改了 `SessionPort::cancel_execution` 的返回类型，**冻结契约测试与已合并的 gateway 轨同时编译不过**，而 `git merge` **零冲突、EXIT=0**、`lib.rs` 两个 `pub mod` 都干净在位 ⇒ **语义破坏对 `git merge` 完全不可见**。两个后果：① 门禁清单**必须含全部冻结基线**，"本轨全绿"不等于能编译；② 预估冲突面不可靠，**合并试跑是唯一可靠手段**，且要跑**每一个**二进制而不是抽查。
- **冻结测试是仲裁者，实现必须向它让步**（2026-10-05 新增）。发现冻结测试挡住实现时，正确顺序是**回退实现**，不是改测试迎合——后者会摧毁它作为独立仲裁者的全部价值。判定冻结性的硬证据：`git diff <base>..<HEAD> -- <冻结测试路径>` 输出为空（未被改动）＋ 基线实测计数绿。
### D-03 定案（合并后复核：我上一条裁定是错的，已推翻）

我在上文两处写的结论**均不成立**，事实如下（全部可复跑）：

**① 「会新增 gateway → registry 的依赖方向」—— 证伪。** `gateway/mod.rs:52` 早就是 `use crate::registry::SessionPort;`（`:191` `port: Arc<dyn SessionPort>`），`gateway → registry` **本已存在**；反向 `grep -rn "crate::gateway" packages/runtime/src/registry/` **无输出** ⇒ 依赖严格单向。D-03 不需要任何新的解冻或依赖决策。

**② 「删掉 `connection::port::CancelDisposition`、复用 registry 的」—— 方向反了。** 规范所有者是 **`connection/port.rs:248`**，registry 只是转出（`registry/mod.rs:61-64` 与 `receipt.rs:35` 两处 `pub use`，注释写明「已在 `connection::port` 定型（三态 + 线上字面量）」）。它被 `registry/actor.rs:693`、`registry/backend.rs:128`、`resource/cleanup.rs:14`、`resource/harness.rs:19`、`resource/journey_rotation.rs:11`、`connection/testing/fake_resource/ops.rs:26` **六处生产/夹具代码**依赖。**真正重复的是 gateway 那一份**（`gateway/cancel.rs:86-110`：三变体 + `as_str()` 三字面量与 `connection/port.rs:257-259` **逐字相同**）。D-03 该删的是 gateway 的副本。

**③ 顺带订正一个容易踩的坑：`CancelOutcome` ≠ `CancelReceipt`，不要合并。**

| | 字段 |
| --- | --- |
| `gateway/cancel.rs:118-121` `CancelOutcome` | `execution_id` + `disposition` + `state` + **`observed_at_nanos`** |
| `registry/receipt.rs:52-62` `CancelReceipt` | `execution_id` + `disposition` + `state` |

gateway 的 `CancelOutcome` 是 registry 回执的**严格超集**，§7.6 要求的三个字段它本来就全有（`execution_id` 是入参、`disposition` 来自 `disposition_from_port_state`、`state` 是端口返回值）。**D-01 的 gateway 侧消费在语义上已被满足**，不需要为它新增对 registry 具名入口的调用，也不该把 `CancelOutcome` 改成 `CancelReceipt` —— 那会丢掉网关自己的观测时刻。

**D-03 的实际修法（已定案，随下轨执行）**：删 `gateway/cancel::CancelDisposition`，改 `use crate::connection::port::CancelDisposition;`（与 gateway 取 `ExecutionState`/`ExecutionId` 的来源一致，**不新增任何依赖方向**），同步 `gateway/mod.rs:78` 与 `facade_support.rs:16` 的转出与 gateway 侧全部引用。风险只剩一个真实项：**两份 `as_str()` 是逐字相同的字面量拷贝，未来任一侧改动会造成线上字面量漂移**，消除重复即消除该风险。

---

## P3 退出门禁 CM 对账（只读静态审计）

派一个**不编译、不跑测试**的只读审计代理（同机有 Coder 在占 CPU），逐条核对退出门槛集合的判据与实际断言。

### 这份审计能证明什么、不能证明什么

**只能证明「断言存在 / 不存在」，不能证明任何断言是绿的。** 全程未执行 `cargo test/build/clippy/fmt`、`vitest`、`typecheck`。

因此：`COVERED` = 判据要求的行为在测试代码里能读到对应断言，**不等于该断言当前通过**；`PARTIAL`/`MISSING` = 可信的**否定**结论（断言确实不存在），这一类结论反而比 `COVERED` 更硬。

另记两条**证据卫生**：
- `platform-development-plan.md:345-349` 里的 `--lib` 139 passed 是提交 `5b7c5b49c` 的历史数据，**已过期**，不得再被引用为 CM-73/CM-74 转绿依据。
- `connection-management.md:1340` 要求：拿不到真实测试环境时必须报告为「未验证」。

### 判据集合口径更正（两处）

1. 我下简报时写的「41 行」是错的。实际 = CM-02～07（6）+ CM-20～32（13）+ CM-37～39（3）+ CM-54～56（3）+ CM-60（1）+ CM-61～74（14）= **40 行**。
2. **审计代理自己的收尾摘要计数 20/18/2 也是错的。** 我按它的明细表逐行重数，得到 **19 / 20 / 1**，合计 40：

```
COVERED = 19   PARTIAL = 20   MISSING = 1   （求和 = 40）
```

它错在哪：收尾摘要的 PARTIAL 清单**漏了 CM-64、CM-70**，COVERED 清单**漏了 CM-21**，并把 CM-22 按 H 半 / D 半**重复计成两行**，同时把 CM-27/28 的隧道子项另算成第二个 MISSING。**明细表可信，摘要计数不可信** —— 这是本轮第二次「汇总与明细不一致，以后者为准」。

### 严格计数（以明细表为准）

| 状态 | 条数 | 编号 |
| --- | --- | --- |
| COVERED | **19** | 20, 21, 23, 24, 25, 26, 29, 30, 38, 55, 62, 63, 65, 66, 67, 68, 69, 72, 73 |
| PARTIAL | **20** | 02, 03, 04, 05, 06, 07, 22, 27, 28, 31, 37, 39, 54, 56, 60, 61, 64, 70, 71, 74 |
| MISSING | **1** | 32 |

审计代理首轮把 CM-74 判成 COVERED，随后**自查降为 PARTIAL** —— 这个自我修正本身是可信度信号，保留；但它没同步修正自己的汇总数字。

`CM-22` 记 PARTIAL：H 半（精确取消控制路径不排队）有断言，**D 半（真实驱动精确取消）本轮不判**，属 D 层。

### 四个必答问题的结论

**① CM-73 / CM-74 是否转绿？—— 不能声称转绿。** 判据 `:1317` 自陈「重构完成后必须转绿，**断言不得删除**」，审计确认断言**仍在仓库里、未被删、未被 `#[ignore]`**。这是必要条件不是充分条件。转绿判定只能由一个**允许跑测试**的代理在干净工作树上给出，并按工作树纪律**首尾各记录一次 HEAD 与工作区 sha**。

三点本体全部有断言：
- (a) **句柄在物理释放前注销** —— `harness/tests.rs:547` journal 严格等于 `["handle closed", "resource Closed", "permit -1"]`；`registry_release.rs:71` 句柄在**登记时的那个资源**上终结，才关当前物理资源。
- (b) **事务状态** —— `registry/actor/tests/release.rs:19` 同时断言 `state == Lost` **且** `transaction_state == Unknown`，绝不报 Active。
- (c) **物理资源与 session ID 同步失效** —— `harness/tests.rs:446` 断言恢复资源的 resource_id 与 db_session_id **双双不同**；`:490` 是负控（宿主若在恢复资源上重登记旧句柄，测试必须响）。

**② CM-60 基准 harness 存在吗？—— 不存在，两处文档说法均属实。** `src/connection/testing/bench.rs` `test -f` = NOT EXISTS；`src/bin/` 不存在；全仓 `bench` 只有两处注释。`fake-runtime-fixtures.md:71` 与 `platform-development-plan.md:128` 的「未实现」都是真的。已有的是**口径实现**不是 harness：`latency.rs` 的 nearest-rank p95 五例自测 + `gateway/timing.rs` 验证「两段按请求求和后再取 p95、不得相加两个 p95、不得剔除失败样本」。判据 `:1236` 要的 release build / 4 vCPU 8 GiB / 并发 8 / 预热 1000 / 5 轮×10000 / p95≤10ms 阈值判定 —— **一次可运行的基准都没有**。

### 基准 harness 的权威规格在 §15.3 `:857`，不在 CM-60 正文 `:1236`

`:1236` 只有一句压缩表述；**逐字可执行的规格在 `connection-management.md:857`（§15.3 测试范围与基准 harness）**：

> CM-60 使用 release 构建，4 vCPU/8 GiB、无数据库网络、单进程固定 fake command 10 ms，以**并发 8** 预热 **1000 请求**；每轮采集 **10000 个获准且未排队的请求**，运行 **5 轮**。附加耗时从 gateway 完成鉴权/参数校验开始到派发 driver，以及 driver completion 到 receipt/event 状态登记完成的两段单调时间之和，不包含 fake SQL、预算/actor 排队、网络传输。采用 nearest-rank p95（排序后第 **ceil(0.95*N)** 项），**每轮均须 ≤10 ms**；排队请求另报等待分位数，**不删失败样本，失败数单列**。

配套 `:859`：**journal 在每次建连/关闭/permit 变化时断言额度，无随机采样盲区**；注入连接持有、慢 consumer 和 drain 的压力部分**另运行，不混入非排队延迟样本**；保存环境、构建参数、原始计时与 journal 作为 CI artifact，**不新增仓库评审记录**。

四条易漏点：**① release 构建**（debug 下 p95 无意义）；**② 每一轮都须 ≤10 ms**，不是五轮取平均或最差；**③ nearest-rank 取下标 `ceil(0.95*N)-1`**，N=10000 即 9499，不用插值分位数；**④ 不删失败样本**，失败数单列。预热/轮次/样本是**规格不是可调参数**，慢也不许砍。

### 「为什么要 4 vCPU」—— 仓库没给理由，它也不是容量要求

全文搜索：4 vCPU/8 GiB 仅两处，**均为裸规格，零出处、零论证** —— `:857` 与 `fake-runtime-fixtures.md:548`。

它作用来自 `:859` 那句「**无随机采样盲区**」：既然要求逐次断言每一次额度变化，p95 也必须能从噪声里分辨真实回归。**没钉住环境跑出的 p95，报告的是机器忙不忙，不是代码退没退。** 所以「4」这个数字本身不重要，重要的是**可复现**。

⇒ 本机 `hw.ncpu=8` / `hw.memsize=16.0 GiB`，约 2× 于判据环境。**更强机器测出更小的数，不构成「在判据环境达标」的证据。** 报告必须同写两句：「8 vCPU / 16.0 GiB 下实测 p95 = X ms」**和**「未在判据指定的 4 vCPU / 8 GiB 复测」，**不得写「达标」**。macOS 无 `taskset`，核数钉不住 ⇒ 运行基准期间只许基准在跑（不同时重编译 runtime、不与其他轨抢核）。

**③ 哪些判据靠 sleep 猜顺序？—— 一个都没有。** `packages/runtime/src` 与 `packages/runtime/tests` 全树 grep `sleep|Duration::from_millis|Duration::from_secs|yield_now|interval(` **零命中**。所有时间推进走 `FakeClock::advance`，所有顺序由 `Barrier` 或 journal 单原子 `seq` 表达。两处「擦边」（`barrier/tests.rs:103-110`、`:413-418` 的 200 ms 轮询）**不违规** —— `:102` 已就地引用 §6.4，它们证明的是「状态位无变化」即**无进展**，不参与顺序推导。其余真实时间用法都只是失败兜底超时；`settle()`/`wait_until()` 的上界（512 次 yield）是**失败时的诊断信息**，不是兜底 sleep。

⇒ **`platform-development-plan.md:124` 的「race 测试使用 barrier/fake clock，不靠任意 sleep 猜顺序」这一条，在 `packages/runtime` 范围内成立。** 可直接引作门槛证据：`harness/tests.rs:1-12`、`cm73.rs:1-18`、`registry/actor/tests.rs:5`「没有基于时间推进的并发窗口」。

**④ 哪些推迟到 P9、不得计入本轮不完整？** 依据 `platform-development-plan.md:335/:337` 与 `connection-management.md:855`：

| 判据 | 推迟部分 | 本轮 |
| --- | --- | --- |
| CM-57 / CM-58 | 全条（W） | **不在门槛集合内** |
| CM-60 / CM-68 / CM-71 | WN 半 | 只判 H/W1 半，**WN 半不构成缺陷** |

⇒ **40 行门槛集合里没有任何一条可以整体推迟到 P9。** 被推迟的只有 CM-60/68/71 三条的 WN 半。CM-59（全 W）不在集合内。

### 三个阻塞性缺口

**① CM-32 的「隧道」概念在 `packages/runtime` 完全不存在。** `grep -rn "tunnel|Tunnel" packages/runtime/src packages/runtime/tests --include=*.rs` → **零命中**；`src/resource/` 下无任何隧道引用计数字段。判据 `:1323` 要求「两个 session/Job 共用同版本隧道；关第一个不影响第二个」。需裁定：**这是设计已演进掉的历史术语（功能已被 CM-67 的 PoolKey / 空闲池模型覆盖），还是仍是活的要求？** 前者则应改文档，后者则要新增实现。

**② `ArtifactStore` 全仓零实现。** `packages/platform-api/src/ports/artifact.rs:16` 明写「本节描述尚未实现的目标行为，当前**无任何 ArtifactStore 实现**」；`grep -rn "whitelist|白名单"` 与 `grep -rn "checkpoint|Checkpoint"` 在 `packages/runtime` 全树**零命中**。CM-61（落盘白名单含 artifact / 临时结果恢复）与 CM-64（产物配额与 256 MiB 上限）各有一半断言**根本写不出来** —— 这不是覆盖不足，是结构缺失。而 `platform-development-plan.md:337` 把 CM-61～64 的 **F** 断言划给 P4、**H** 断言划给 P3。需确认这部分 H 断言是否本应在 P4 或更后。

**③ CM-74 顺序口径冲突。** `harness/tests.rs:539-545` 的注释把「句柄先于物理关闭」**明确限定在宿主关闭路径**（`FakeHarness::close`），并声明驱动直连路径（`close_resource` / `reclaim_registered_handles_on_close`）顺序**故意不同**（`resource Closed → permit -1 → handle closed`），且不得为 CM-74 断言该路径。但判据 `:1324` 要求**三条路径**（归池 / setSessionContext 替换 / closeSession）统一。二者必须二选一：把顺序统一到所有路径，或把判据适用面收窄到宿主关闭路径。**这是 CM-74 判 PARTIAL 的唯一原因。**

附带：`setSessionContext` 替换路径在 registry 层 `grep` **零命中**，只有 `ResourceOp::ChangeContext`（`fake_resource/script.rs:44`）。若该路径根本没建模，CM-74 第三条路径要**新增实现**而非新增断言。最接近的替代证据 `registry_lifecycle.rs:248`（世代推进使旧请求失效）与 `resource/handles.rs` 的 `resource_replacement_invalidates_bindings` 单测，**都不等价**于判据要求的「替换路径上句柄在物理关闭前被终结并注销」。

另记一条口径提醒：CM-62 的规范化契约实际落在 **`driver-api` crate**（`packages/driver-api/src/namespace_tests.rs`）而非 `packages/runtime`。它算覆盖，但覆盖点在 runtime 之外，评审时别在 runtime 里找不到就误判成缺失。

---

## 轨 p3-cancel-cleanup（D-03 / D-R2-1 / D-R2-2）—— **已合并**（详见文末「合并记录」）

`READY_FOR_TEST`，已派 fresh Tester（**非 Coder**）。分支 `feature/p3-cancel-cleanup`，`25771de51 → e1cf4576`。

| 变更 | commit |
| --- | --- |
| D-03 删除网关侧重复 `CancelDisposition` | `dd386c3aa` |
| D-R2-1 形状钉子提出 `#[cfg(test)]` | `afa22c7a6` |
| D-R2-2 `invalidate_worker` 三态分支 + 结构测试 | `141c8abf9` |
| `progress.md` 台账（**合并时必须删除**） | `e1cf4576` |

### Coder 自报门禁（**待 Tester 独立复核，自报数不作为验证**）

12 套冻结基线全 EXIT=0 且 `p3_session_port_contract` **10** 未动；`--lib` **380**（基线 377 + 新增 3）；`gateway_contract` **51** 未下调；registry 五件 9/11/11/7/6；`build` EXIT=0 且 warning 0；`fmt --check` EXIT=0；边界脚本 0 violation / 3 advisory。门禁首尾 HEAD 与 status 各记一次（未提交态 7→7、提交后态 0→0），冻结面 `git status` 输出为空。

### Coder 自承的三处弱点 —— 已列为 Tester 必答项，不接受「已解决」的说法

**① `Ok(false)` 分类测试可能是人工造的。** Coder 自陈「`Ok(false)` 在登记表路径上近乎不可达，**我没能证伪**」，却仍写了 `否定答复仍算送达_不归该worker管的会话照常摘行`。若该测试是**直接调 actor** 造出来的，它保护的是「三态表的第二格写对了」，**不是**「登记表真会走到第二格」。这两种保护力差一个量级，必须分开记账。

**② `Err` 分支「结构性不可达」这条裁定方向上是加强而非削弱，但依据要独立核。** Coder 的四步证据链：`SessionRecord` **按值**持有 `actor: SessionActor`（自持 `UnboundedSender`）→ `run_actor` 仅在 `exec_rx.recv()` 得 `None` 时收尾 → `owned_by` 是**按值克隆**记录、克隆在 `.await` 期间就是活着的发送端 → 摘表项只会多丢一份克隆，**丢不掉发送端**，故摘行不可能造成 `Err`。

这条如果成立，D-R2-2 的 `Err` 分支就是**纯防御**（只剩 actor 异常终止能触发），价值在于把「正常路径不可能」写成**分支**而不是注释：旧代码 `let _ = …` + 无条件摘行的真实代价是——控制通道已断、物理资源仍在别处活着，登记表却记成「已作废」，额度随后被 `quota.hold_stale` 挂住，而没有控制通道的会话**不会被隔离** ⇒ **额度永久挂住**。Coder 还诚实指出「不计入 `lost`」正是为此。**若这条裁定被推翻，`Err` 就是活缺陷而非潜在缺陷**，严重度要重估。

**③ 「能编译即证明」的说法要打折。** `row_owns_the_sender(&SessionRecord) -> &SessionActor` 是类型层面的平凡转换，本身不构成不变量证明；真正有分量的是它**顺带**证明了 `actor` 按值持有。Coder 同时承认反向（最后一个克隆 drop 后 actor 是否收尾）**结构上不可观测**——因为调用 `is_closed()` 本身就需要握着一个发送端。这个自我限制是对的，不要给反向不变量发通行证。

### Coder 依分工未做变异实验

三个修复各配了钉子，但钉子本身**没被「故意破坏实现再跑一遍」验证过**。已要求 Tester 在独立 detached 树补做 6 组变异（M-A～M-F），其中 **M-A 是本轮最关键的一条**：改 `registry/port.rs` 的 `cancel_execution` 签名后，`cargo build -p datazen-runtime` 必须失败。修复前实测该命令是 **EXIT=0 `Finished dev profile in 2.50s`**（钉子只对 test profile 生效）；**若这次 dev 档位仍是 0，说明钉子根本没移出 `#[cfg(test)]`，D-R2-1 未真正修复。**

---

## Tester 第 1 轮结论：**FAIL（窄，但真实）**

Tester `73b5a60e`，对 `feature/p3-cancel-cleanup`（`25771de51` → `e1cf4576`）。**判定 D-03 = PASS、D-R2-1 = PASS、D-R2-2 = FAIL**。失败面被压缩到 D-R2-2 的**零回归保护**加一个名实不符的测试，生产逻辑本身被判定为**正确、不需重写**。

### 交付树完整性：前后完全一致

| 时点 | HEAD | `git status --porcelain -uall \| wc -l` | `git diff HEAD \| shasum` |
|---|---|---|---|
| 跑门禁前 | `e1cf457677942e0dc667b7e34a4e1de1d8260647` | `0` | `da39a3ee5e6b4b0d3255bfef95601890afd80709  -` |
| 跑完所有变异后 | 同上 | `0` | 同上 |

等于空树哈希 ⇒ **变异全程没有污染交付树**。`git diff --name-status 25771de51..HEAD` = 8 文件 / 595 增 / 44 删，**全部在 `packages/runtime/src/` 之下加 `progress.md`，无 `Cargo.toml`、无 `Cargo.lock`、无其它 crate**。

### 门禁：7 项全部 EXIT=0，与 Coder 自报**零差异**

12 项冻结基线逐项吻合（8/2/6/7/12/6/7/10/**10**/4/7/6）；`--lib` 两次均 `test result: ok. 380 passed; 0 failed; …`（基线 377，Coder 自报 380，实测 380）；`gateway_contract` 51；registry 五套 9/11/11/7/6；`cargo build -p datazen-runtime` **0 warnings**，`Finished \`dev\` profile [unoptimized + debuginfo] target(s) in 14.59s`；`cargo fmt --check` 0 行；边界检查 `PASS — 22 workspace member(s) classified, 6 rule(s) evaluated over 26 crate(s), … 0 violation(s), 0 error(s), 3 advisory(ies)`。

**冻结面**：`git diff 25771de51..HEAD -- packages/runtime/src/connection packages/runtime/tests | wc -c` = **0 字节**。`connection/port.rs` 在两端同为 577 行。

**行数**：最高 `registry/actor.rs` 734 / 上限 800。**生产路径新增裸 `unwrap()/expect()/panic!()/unsafe` = 0**；新增的 4 处 `expect()` 全在 `#[cfg(test)] mod tests;` 文件内。

**Flakiness 诚实声明**：两次 `--lib` 都绿，未重跑、未放宽超时。按该 fixture 约 12.5% 的自发红概率，**连续两次绿只相当于约 77% 置信，不等于确定**；再跑更多次属于禁止的掩蔽。

### 一个被推翻的历史结论

任务书里那条「dev 档位会开始编译测试专用项」的前提**部分错误**：`#[cfg(test)] mod tests` 块在 dev profile 下**仍然不编译**，真正被编进 dev 的只有自由函数 `frozen_port_cancel_shape`，其 `#[allow(dead_code)]` 干净地压住了告警。**但 M-A 的实测结论是决定性的**——改 `registry/port.rs:104` 的 `cancel_execution` 返回类型后：

- 修复前：`cargo build -p datazen-runtime` → **EXIT=0 `Finished dev profile in 2.50s`**（钉子只对 test profile 生效）
- 修复后：**EXIT=101**，`error[E0271]: expected \`Pin<Box<dyn Future<Output = Result<(), RuntimeError>> + Send>>\` … but it resolves to \`Result<ExecutionState, RuntimeError>\``，错误锚在 `registry/port.rs:160:6` —— 即 `frozen_port_cancel_shape` 返回类型的精确位置

⇒ **钉子确实走出了 `#[cfg(test)]` 并在门禁 dev 档位生效**，D-R2-1 的核心主张是**实证**的，不是声称的。

### D-03 的真实价值被量化出来了

M-C 只改 `connection/port.rs:257` 规范侧的字面量 `"requested"` → `"requestedMUT"`，`--lib` 红 6 个，其中**两个是 gateway 侧的**：`gateway::cancel::tests::disposition_literals_match_the_architecture_map` 与 `gateway::cancel_event_tests::a_live_execution_reports_requested_without_claiming_a_terminal_state`。**修复前 gateway 手里有一份逐字 `as_str()` 副本，这个改动不会跟着走，那两条 gateway 断言会照样绿着，而 gateway 的字面量已经悄悄漂离规范值。** 修复后全工作区只有一处真相 ⇒ 这是实测收益，不是化妆品级改动。

### D-R2-2：三个变异全部存活

负控 `/tmp/dz-mut-NC2` 在 `e1cf4576` 干净树上跑完整套件 **20 个二进制全 ok、0 FAILED**。在此对照下：

| ID | 注入 | 结论 |
|---|---|---|
| M-D | `registry.rs:522` `if !delivered { continue; }` → 也 `lost.push(record.db_session_id.clone())` | **SURVIVOR** |
| M-E | `registry.rs:509` `Ok(_) => true` → `Ok(true) => true,` + `Ok(false) => false,` | **SURVIVOR** |
| M-F | **整段回退**到修复前 `25771de51:461-473`（已核对与基线**字节一致**） | **SURVIVOR** |

**M-F 能完整撤销 D-R2-2 而 20 个二进制无一察觉** ⇒ 该修复当前**零回归保护**。（另有一处自我更正记录在案：M-D 第一次注入 anchor 计数为 0、`INJECTED` 从未打印，其后三次绿测跑在**未变异**的树上 —— 该次尝试作废，不计入杀/存活。计数更正：**实际注入 4 组变异，不是 6 组**，M-A 与 M-C 各跑两次属同树重跑。）

### 缺陷清单

- **D1（medium，合并阻塞）** D-R2-2 零回归保护。要求：一条**真的驱动 `invalidate_worker` 到非 `Ok(true)` 结果**的测试，同时断言三件事——行仍在登记表、返回 `lost` **不含**该 id、**未**发出 `QuotaHeldStale` 审计事件。
- **D2（medium）** `registry/registry/tests.rs:172-217` 名实不符：`:187-195` 断言的恰是**与测试名相反**的「行不被摘」（被 `owned_by` 的 `record.worker_id` 过滤短路，`control()` 压根不会被调），`:202-207` 的 `Ok(false)` 是**直接调 `actor.control(...)`** 造的、绕过 `invalidate_worker`。M-E 已实测证明它保护的那一格是裸的。
- **D3（low）** `row_owns_the_sender`（`tests.rs:119-122`）**在 Rust 可见性上不可能**钉住「`SessionActor` 按值持发送端」——其字段私有于 `registry::actor`（`actor.rs:161-165` 无 `pub`），而本文件在 `registry::registry::tests`，够不着 `ctrl_tx`。真正有分量的是 `:165-169` 的**行为**断言（摘行后按值克隆仍 `!is_closed()`）。需改文档，把功劳记给行为断言。
- **D4（合并义务）** `progress.md`（221 行）随合并必须删除。

### 对 Coder 三处自陈的独立判定（E 组）

- **E1「`Ok(false)` 的测试是人工造的」——成立。** Coder 的怀疑是对的，该测试并不支撑它名义上的保护。
- **E2「`Err` 是结构性不可达」——方向成立，四步结构证据逐条复核全部成立**，故裁定为**潜在（latent）而非活缺陷**。但 Tester 明确**没有**在运行期构造出 actor 的异常终止，**因此不能断言 `Err` 在生产中到底可达不可达**。
- **E3「形状钉子证明了 actor 按值持有发送端」——不成立，措辞被夸大。** 不是错，但会诱导后来维护者高估一个类型注解的分量。

### 本轮暴露出的一个结构性问题（需协调者裁定）

**M-F 只有 `Err` 格的测试能杀死。** 修复前的「无条件摘行」在 `Ok(true)` 与 `Ok(false)` 上的行为与修复后**完全一致**，所以把 `Ok(false)` 那格钉得再死，M-F 照样存活。⇒ **不解决「`Err` 能否在测试里真实造出来」，D1 就无法闭合。**

若结论是造不出来且必须改生产代码，则要在三者之间选一个：① 在 `registry/actor.rs` 加 `#[cfg(test)]` 构造器或把 actor 的 JoinHandle 暴露给测试；② 反过来证明 `Err` 是活路径（actor 内部 panic → `ctrl_rx` 被 drop → 表项还在 → `control()` 返回 `Err`）并据此写测试；③ 承认这一格只能靠文档承担，D-R2-2 降级。**已连同证据要求一并交回原 Coder 查证**，未擅自替其下结论。

### Tester 明列的未验证项（无一条按「通过」记）

① `Ok(false)`/`Err` 在生产中的可达性（未构造异常终止）；② `state.physical == None`（`release.rs:113`）且表项仍在时这条 `Ok(false)` 路径（未构造）；③ flakiness；④ `packages/runtime` 之外的 scope（未构建 `src-tauri`、drivers、整 workspace）；⑤ 非默认 cargo profile / feature；⑥ Coder 自报门禁日志的产生环境（未审计，改为全部自行重跑）；⑦ 除冻结契约外的跨轨交互。

---

## 三个待答问题的分析（**协调者分析，尚未经用户裁定**）

以下三题由我在交付记录成文时提出，答案取自仓库证据。**用户尚未就 CM-74 顺序口径与 P3 豁免范围作裁定**，故本节只记分析与证据，不记裁定；相关问题仍开放。

### Q1：CM-74 为何要求句柄**先于物理关闭**回滚并注销

判据 `connection-management.md:1324` 要求句柄在**原 resource 上**回滚/关闭并从 actor 注销，**且发生在物理资源关闭之前**；commit 必须落在旧 resource 上或明确失败，绝不落到新 resource 上。

**理由**：句柄是对「某个物理 resource 上事务状态」的引用。CM-73 的基线缺陷（`:1316-1318`）就是这件事的具体形态——`cleanup_idle_connections` 保留了 owner map，而 `session_transactions` 另行管理，于是 `get_session` 会**经由该 map 以同一个 dbSessionId 重连**，驱逐途中的一次 commit 就静默落到了新资源上；反过来，旧句柄则指向已死的资源却仍在报 `Active`，这正是 `:1316` 明令禁止的。**先注销**把这段歧义窗口关掉：此后任何迟到的 commit 都走「未登记句柄被拒绝」这条**已明确定义**的路。

**次序之所以要单独钉**：因为**集合式断言对次序完全失明**。`resource/harness/tests.rs:535-537` 自己写着——三件事都发生但次序错了，宿主就会在句柄还挂着的时候先释放预算占用，I1（permit 收支）与 I5（登记册收口）一起破，**而任何「都发生过」式的断言都照样是绿的**。故 journal 必须严格为 `["handle closed","resource Closed","permit -1"]`。

**我自己的更正（撤回原措辞）**：早先把本条冲突表述成「必须二选一」**不准确**。`permit -1` 排在最后是 harness 测试额外施加的 I1/I5 约束，**并非 `:1324` 所要求**；且 `:1323` 列的三条路径里含**归池**，归池根本不物理关闭 resource，所以 `:1324` 落在该路径上真正断言的是「句柄不得带着资源进池」，**与 `FakeHarness::close` 那条不是同一个断言**。

**硬冲突仍然存在**：在归池路径上提前回收，会让 §4.2 F10「句柄非空 → 关闭而非归池」失去输入（`registered_handles` 已被清零），F10 会被骗过。**这需要一次明确裁定，不能靠改写措辞消解。**

### Q2：`setSessionContext` 是干什么的，有实现吗

**用途**：在不拆掉会话的前提下切换会话的活动上下文/命名空间（等价于 `USE otherdb`，见 §7.4 `:554-566`）。三种模式（`:425`）：`inPlace`（驱动原地切换并被观测，再发布已确认的上下文）；`requiresReplacement`（无法原地切换——保留旧会话，另建一个**仅内部可见**的候选 resource 与候选记录，该候选**不登记进** SessionRegistry/SessionDirectory 也**不接**任何 execution/subscription，随后经 §12 内存目录提交协议原子发布，把旧会话标记 Closing、旧 resource 交给清理；**发布失败不得返回成功**，提交后的失败只能恢复同一张回执）；`unsupported`/`unknown` ⇒ 明确报错，**不许假成功**。返回 `ContextChangeReceipt { session, replacedSessionId, attachmentToken }`（`:564`）；带 `idempotencyKey` 重试返回**原回执**（`:566`）。

**分层状态**（纠正「没有实现」这种粗糙说法）：

| 层 | 状态 | 证据 |
|---|---|---|
| application 契约 | **齐备** | `application/src/sessions.rs:110` trait 方法；`dto/requests.rs:279` `SetSessionContextRequest { target, expected_context_revision }`；`ContextChangeReceipt`；`platform-api/src/dto/idempotency.rs:25` `IdempotentOperation::SetSessionContext` |
| 传输层 | **齐备** | `backend-client/src/client.ts:80`，HTTP `PUT /api/v1/sessions/{id}/context`（`system-overview.md:229`） |
| 驱动层 | **已建模** | 每个驱动必须申报 `namespace_switch`；`driver-api/capabilities.rs:843`、`session.rs:672` 测 inPlace vs requiresReplacement；elasticsearch/vector 申报 `Unsupported` 并附表列理由；victoriametrics 证明「**Measured absent, not merely undeclared**」；sqlite 记录了为何 `requiresReplacement` 会承诺一个做不到的切换 |
| **runtime 核心** | **缺** | `packages/runtime/src/registry` 对 `setSessionContext` **零命中**；唯一的上下文变更建模是测试夹具的 `ResourceOp::ChangeContext`（`fake_resource/script.rs:44`） |
| runtime 前置闸 | **已是活的** | `context_revision` 闸在 `gateway/mod.rs:250/336-341`，三个测试在 `facade_tests.rs:55/130/375` |

**精确说法**：*runtime 已经有 `setSessionContext` 的守门部件，但没有这个操作本身*。缺的恰是 `requiresReplacement` 的**候选 resource + 原子发布**协议，即 §7.4 第 6 条，也是最重的一块。`sessions.rs:5-6` 说明了 application 为什么不能编排它（依赖图里没有 `APP → RT`），实现属于组装层。

**对 P3 的后果**：CM-74 被判 PARTIAL 的**唯一原因**就是这条替换路径缺句柄终止/注销次序断言，而证据表明它需要的是**新实现，不是新断言**。

### Q3：`ArtifactStore` 是干什么的

**用途**：大批量结果的**有界落盘端口**，让结果字节不必全驻内存、也不必全靠事件流推送。它解决的具体问题在 `:588-590`：内存 ResultSink 是有上限的（每订阅事件队列 256 条 / 1 MiB，首版每 execution 未消费缓冲上限 8 MiB，无消费者等待 30 秒），溢出必须落盘。

**三条安全/正确性红线**（`platform-api/src/ports/artifact.rs:8-12`）：`create` 处绑定组织/属主 ACL；`export` 的 sink 是**受控目标**（对话框句柄 / 服务端授权路径），**拒绝任意服务端绝对路径**；TTL 过期 ⇒ `PortError::ArtifactExpired`，且**不得继续用缓存副本供应**。

**生命周期不变量**（`:18-27`）：序号有序分配；已发布块不可变；**写入中即可读**（`read_chunk` 只读已发布索引，`read_range` 只读已发布连续前缀，**绝不等待未来的字节**）；未知/待定索引或越界 ⇒ **参数错误，绝不返回「空块」**；`abort` 保留已发布块并标记 truncated；**写入方异常退出 ⇒ 恢复扫描标 truncated，绝不声称 complete**；正常导出只允许从 `complete` 出；`truncated` 需用户显式确认且保留截断标记。

**归属按目标而非按会话**（`artifact.rs:46-47`「结果归属按目标而非按会话,切会话不改变归属」，`ArtifactSpec.target_fingerprint`）——与 Q2 直接耦合：切库换了会话，产物仍属原目标。

**配额**（`:590`）：per-execution 上限必填，桌面默认 256 MiB、团队默认 1 GiB，仍受组织总产物额度限制。**这就是 CM-64 的 256 MiB**——写进了规范，却无任何东西执行。

**状态：零实现方、零调用方**（`artifact.rs:16`「本节描述尚未实现的目标行为,当前无任何 `ArtifactStore` 实现」；`platform-development-plan.md:128`「`ArtifactStore` 至今**零实现方、零调用方**」）。

**决定性一点**：这不只是「P3 还没做」。该端口在 `:76`（P1）就已定义，而**服务端实现 + schema 迁移被排期在 P7（`:204`）**。

⇒ **审计员留下的开放问题（「这些 H 断言该归 P4 还是更晚」）已有答案：`ArtifactStore` 本身属于 P7**，因此 CM-61（落盘白名单含 artifact / 临时结果恢复）与 CM-64（配额与 256 MiB）中依赖 ArtifactStore 的 H 断言**在 P3 结构上不可能具备**，应记为**显式 P3 退出门禁豁免**，而不是记作「覆盖不足」。

---

## 轨 p3-cancel-cleanup 合并记录（Tester 第 2 轮 **PASS**）

合并提交 `32c17bf5a`，收尾提交 `9a356cec8`（删除 `progress.md`）。

### 第 1 轮 FAIL 的根因与第 2 轮的闭合

| ID | 缺陷 | 第 2 轮闭合情况 |
|---|---|---|
| D1 | D-R2-2 零回归保护：M-D/M-E/M-F 全存活，M-F 是把该段整块回退到 `25771de51:461-473`，20 个二进制一个都不红 | **已闭合**，三个变异全死 |
| D2 | 用例 `否定答复仍算送达_不归该worker管的会话照常摘行` 名实相反，且用直接调 `actor.control(..)` 造 `Ok(false)`，绕过被测函数 | **已改正**，旧名全仓命中 0 |
| D3 | `row_owns_the_sender` 注释声称能钉住「`SessionActor` 按值持发送端」，但那些字段私有于 `registry::actor`，测试住在 `registry::registry::tests`，**Rust 可见性上够不着** | **已改正**，注释如实降级为「只能靠行为断言钉」 |

**D1 之前打不死的结构原因**：回滚后的实现与修好后实现在 `Ok(true)`/`Ok(false)` 上**逐字不可区分**，只有走 `Err` 分支的用例能杀 M-F。因此钉 `Ok(false)` 再硬也**关不掉** D1——必须补 `Err` 格。

### 三态表三格的造法（均不改生产代码）

| `control()` | 造法 | 杀掉 |
|---|---|---|
| `Ok(true)` | 假后端 `close ⇒ Closed`，登记后直接 `invalidate_worker` | — |
| `Ok(false)` | 假后端 `close ⇒ Undecidable`：`actor/release.rs:113` **先**把 `physical` 清空再于 `:141` 返回 `SessionLost`；`actor.rs:668-679` 的 `evict_idle` 用 `?` 抛掉、拿不到成功视图，于是 `registry.rs:423` 的 `evict_idle_at` **跳过摘行**，留下「资源已清空、行还在表里」；下次租约失效命中 `actor.rs:610` 的 `physical.is_none()` 早返回 | **M-E** |
| `Err(_)` | 假后端 `close ⇒ panic!`：§9.4 在 actor 任务内部 `await backend.close(..)`，`actor.rs:600` 是 `let _ = reply.send(invalidate_worker(state,&id).await);`——`.await` 在 `send` **之前**，展开必然把回执一起丢掉，`control()` 拿到 `Err` | **M-F**、**M-D** |

⇒ **Coder 第一轮「`Ok(false)` 近乎不可达」的断言是错的**，已连同 `registry.rs` 注释就地更正。这条路径是真实生产路径，不是夹具假象。

### 变异实测（Tester 独立 detached 树，负控先行）

负控：`cargo test -p datazen-runtime` **20 个二进制全 ok、0 failed、EXIT=0**（`--lib` 382）。

| 变异 | BUILD | 结论行 | TEST | 死于 |
|---|---|---|---|---|
| M-D | EXIT=0 | `FAILED. 381 passed; 1 failed` | **101** | Err 格 `tests.rs:321` |
| M-E | EXIT=0 | `FAILED. 381 passed; 1 failed` | **101** | **`Ok(false)` 格 `tests.rs:293`** |
| M-F | EXIT=0 | `FAILED. 381 passed; 1 failed` | **101** | Err 格 `tests.rs:321` |

M-F 插入片段经脚本核对与 `git show 25771de51:` **byte-identical（len=1008）**。M-E 死在 `Ok(false)` 格这一条最有价值：它排除了「该格其实走的是 `Ok(true)`、与 `Ok(true)` 格不可区分」这个疑虑。

### 合并安全性

`main` 自分支点 `25771de51` 只动过 `hub.md`，`.rs` 改动数 **0**；分支动 7 个 `.rs` + `progress.md`。**交集 0**，`MERGE_EXIT=0`，`unmerged=0`。合并树 `packages/runtime/src` 与被验收分支差异 **0 行**——即 Tester 的 20 个二进制全绿**恒等覆盖**合并树，无需重跑。冻结面 `git diff 25771de51..HEAD -- packages/runtime/src/connection packages/runtime/tests | wc -c` = **0**。

前端门禁（合并树实跑，首尾 HEAD/`diff HEAD` 逐字相同、DIRTY 首尾皆 0）：
```
TYPECHECK_EXIT=0
VITEST_EXIT=0   Test Files 565 passed (565)   Tests 5920 passed (5920)   Duration 96.18s
```

### Tester 自列的未验证项（不按「通过」记）

`--lib` 只跑 3 次全绿（12.5%/次 ⇒ ≈98% 置信，非确定，无重跑到绿）；变异树用独立 `CARGO_TARGET_DIR` 冷编译，未验证交付树自带 `target/` 产物；未复核 D-03/D-R2-1 语义；未审计 `audit.rs` 本身；未跑 clippy / host crate / e2e；`.env`/`.env.test` 本树内不存在，只查存在性与忽略状态，**从未打开内容**。

### 本轨遗留（未随合并消失）

- **CM-74 顺序语义未裁定**（见上文 Q1）。
- `Err` 格用例会在 stderr 留一行 `panicked at … driverCloseCallbackCrashedTheActorTask`，**是被测事件本身**；不得用 `std::panic::set_hook` 消音。
- `connection::port::CancelReceipt`（2 字段）vs `registry::CancelReceipt`（3 字段）同名异构，待 Wave 2 裁定。

---

## 第二批开工前的现状复核（协调者实测，2026-10-05）

派活前把五个缺口逐个挖到可施工级别，其中两条**推翻了之前的记述**。

### 更正一：CM-70 不是「验签/重放」

原文（`connection-management.md:1294`）是「**CM-70 过期幂等键与记录删除（H/W1）**」。之前记成「验签/重放 MISSING」是错误概括，据此派活会做错东西。

实测本轨**不是零基础**：`packages/runtime` 幂等命中 **294** 处，且 `connection/testing/clock.rs:417` 已有 `idempotency_token_expiry_is_24h_of_virtual_time`；`resource/journey_ledger.rs:124` 已有「同键恢复候选而非二次开资源」；`connection/testing/ids.rs:254` 有 `idempotency_nonce()`。缺的是过期后不执行、删记录后不执行、伪造 `issuedAt`/`keyVersion` 拒绝、owner 重启后旧令牌 SessionLost、receipt/token 不落盘这**五条窄面**。

**↑ 上一行末的「五条窄面」是协调者派活时的错误估计，已被该轨 Coder 的机械查询推翻，见下方「更正三」。**

### 更正三：CM-70 的实际范围远大于「补断言」

派活时协调者按「只补窄面」估算。`p3-cm70-idempotency-replay` 的 Coder 在工作树内做了机械对照表后回传，结论相反——**runtime 侧根本没有签名设施**：

| 机械查询 | 结果 |
| --- | --- |
| `issued_at` / `issuedAt` in `packages/runtime` | **0** |
| `key_version` / `keyVersion` in `packages/runtime` | **0** |
| `retained_until` / `retainedUntil` in `packages/runtime` | **0** |
| `impl SubmissionTokenIssuer` 全工作区 | **0**（`platform-api/src/ports/token.rs:63` 是**只有 trait、没有实现**的端口） |
| `IdempotencyStore`（`gateway/idempotency.rs:161-174`） | 只有 `read` / `write`，**无删除语义** |

⇒ 7 条断言实测：**已有 2（A1 同输入重发同 receipt、A2 不同输入冲突，均由 CM-54 账本 `gateway/mod.rs:236-248` 做实）、部分 1（A6 机制在 `IdempotencyLookup::Unreadable` 但断言缺）、缺 4（A3 过期不执行、A4 删记录后不执行、A5 伪造拒绝、A7 owner 重启 SessionLost）、缺且需新增钉子 1（A8 receipt/token 不落盘；先例 `tests/directory_no_disk.rs` 只覆盖 `src/directory/**`）**。

附带更正一条此前的误读：`connection/testing/clock.rs:417` 的 `idempotency_token_expiry_is_24h_of_virtual_time` **不是**已有的过期能力——它只测 `FakeClock` 计时器本身，**没有任何消费者**，是一条孤立的时间算术断言。

⇒ **实际工作量是「新建一层 runtime 签名提交令牌 + 保留期/删除 + owner epoch 绑定」**，属新增能力而非补断言。已要求该 Coder **先出施工方案（分层落点、既有 4 处 `IdempotencyRecord` 构造点的边界、是否该拆多轨、可独立验收的最小闭环），暂不写实现**。CM-70 的排期须按新增能力重估，不能按「补断言」估。

### 更正二：CM-32 不是「历史术语」，是「**哪儿都没有**」

之前把 CM-32 的开放问题写成「历史术语 vs 活要求」二选一。实测推翻了这个框架：

| 位置 | 命中 |
| --- | --- |
| `connection-management.md` 内「隧道」 | **8 处**（`:25` `:575` `:676` `:817` `:1029` `:1031` `:1037` `:1057`） |
| 全仓 `docs/`+`packages/`+`src/`+`src-tauri/` | **85 文件** |
| `packages/runtime` | **0** |
| `src-tauri/src/tunnel/` | 3 文件（`mod.rs` `http_proxy.rs` `websocket.rs`） |
| **上述两处的 refcount/共享引用计数** | **0** |

⇒ 隧道在宿主是**实打实实现了的**（`resolve_tunnel_kind` / `start_for_connection` / `Tunnel` 枚举），但**共享引用计数语义哪儿都不存在**。所以既不是文档漂移（措辞仍在用），也不是已实现（语义缺失）。

⇒ **这是新出现的第三种可能，之前的二选一没覆盖：隧道引用计数该建在 `packages/runtime`（判据标 (H)，H = 部署无关 runtime 契约），还是建在 `src-tauri`（架构映射 `:25`/`:676` 把隧道归宿主）？** 前者要在 runtime 引入一个目前为零的隧道概念；后者要承认 CM-32 的 (H) 标签标错了层。**必须用户裁定，协调者不自行决定。**

连带 **CM-27（`:1029`「隧道引用正确」）、CM-28（`:1037`「隧道不多减引用」）的隧道子项同此裁定**，一并等。

### CM-60 拆两半

- **A 半（行为）**：100 逻辑 session / 1000 次操作与取消 / 总额度 20 + 控制预留 2 / 队列上限 32 / 两 worker / drain 一个 / 关池。可完全实现，不依赖硬件。
- **B 半（性能）**：release build 基准，`latency.rs` 的 nearest-rank p95 可复用（**全仓目前一次可运行的基准都没有**：`connection/testing/bench.rs` 与 `src/bin/` 均不存在）。

**环境偏差已实测**：本机 `hw.ncpu=8`、`hw.memsize=16.0 GiB`；判据要求 **4 vCPU / 8 GiB**。本机**更强**约 2 倍。已向 Coder 追加裁定：在更快机器上通过 p95≤10 ms 比在目标机器上通过**更保守**，不是宽松放行，但**仍不得记为「已在判据指定环境验证通过」**；实测环境必须逐字记录。

### 工具更正

本仓**存在 `.codegraph/`**，AGENTS.md 要求查询前先走 `codegraph explore`。本节调查用的是 grep——结论有效（给的是具体计数），但方法不合规，后续调查改用 CodeGraph 优先。

---

## CM-70 施工方案裁定（协调者复核后批准，附一条硬要求）

Coder 交回方案、协调者**逐条实测其五条事实断言**（自报不算验证），结果全部属实：

| 断言 | 实测 |
| --- | --- |
| 分层先例 | `trait SessionDirectory` 在 `platform-api/src/ports/session_directory.rs:96`；`impl … for InMemorySessionDirectory` 在 `runtime/src/directory/directory.rs:612` ⇒ **端口在上游、实现落 runtime 确为既有惯例** |
| `IdempotencyRecord` | 确为 **3 字段**：`execution_id` / `fingerprint` / `first_accepted_at_nanos` |
| `IdempotencyStore` | 确只有 `read` / `write`，**无删除语义** |
| `ExecutionGateway::new` | runtime **5** / src-tauri **0** / 其他 crate **0** ⇒ 改 `new` 签名的回归面判断属实 |
| `SubmissionToken` | 确只有 `{idempotency_key, expires_at}`，**无签名字段** ⇒ 签名须落在不透明串内部、无需 DTO 改造属实 |
| 加密依赖可离线 | `hmac 0.12.1` / `sha2 0.10.8` / `subtle 2.6.1` 确在 `Cargo.lock` **且** 本地 registry 有 `.crate` ⇒ **降级方案不触发**，走真 MAC |

批准：端口留 `platform-api` 不改、实现在 `packages/runtime`；`IdempotencyStore` 加**带默认方法体的 `delete`**（替身继承「拒绝」比假装删成功诚实，5 处既有实现零改动）；`IdempotencyRecord` **冻结不动**；**不拆多轨、只拆 commit**。

### 拆分理由（值得留档）

Coder 主动拒绝拆分，理由成立：「有 `token.rs` 单测全绿、但网关压根没调它」能过自己的测试而 CM-70 行为一点没变——这正是半成品形态。commit 1 = 令牌层 + 保留期/删除 + 接入闸门 + A3/A4/A5/A7；commit 2 = A6 围栏 + A8 不落盘钉子。A6 不能与 commit 1 并行（两者都改 `gateway/mod.rs` 的 `accept` 同一段，必冲突）。

### 裁定附加的硬要求：`retained_until ≥ expires_at`

Coder 方案称「删除记录之所以安全，是因为拒绝由令牌驱动」。**该推理不完整；缺了它 A4 不是没防住，而是删除动作本身制造漏洞。**

推演：删 grant 后重放 → 闸门验签 ✅ / 未过期 ✅ / epoch ✅（令牌自身签名字段完好）⇒ **放行** → 账本查重 `Miss` → **真的执行一遍**，正是 CM-70 禁止的。

不变量 **`retained_until ≥ expires_at`**（令牌先按自身签名字段过期，之后才允许删 grant）才使该推理成立。判据原文「超过记录保留期删除记录，再重放令牌」的步骤顺序蕴含此点，但方案**未将其断言化**——未断言的不变量等于不存在。

⇒ 追加要求：(a) 该不变量落成**代码**，非注释约定，保留期清理须先确认 `expires_at` 已过；(b) 补**否定用例**——令牌仍有效但 grant 已删除 → 重放必须拒绝执行，作为防止 `delete` 被提前调用的护栏；(c) 与 A4 正向用例同列 CM-70 断言面。

### 新依赖

`packages/runtime/Cargo.toml` **未声明** `hmac`/`sha2`/`subtle`，需新增 `[dependencies]`。要求：锁文件一致；若 `cargo build --workspace` warning 数变化，**如实记录增减**，不得报成「清零」或「无变化」而不查。

---

## 用户裁定（2026-10-05，三项）

| 议题 | 裁定 |
| --- | --- |
| ① CM-74 释放顺序 | **统一** |
| ② CM-32 隧道引用计数建在哪一层 | **runtime** |
| ③ CM-61 / CM-64 的 ArtifactStore H 断言 | **豁免** |

### ① 「统一」的实证障碍：两条判据无法靠挪位置同时满足

用户裁定「统一」。派活前协调者实测发现，**直接按 `:1324` 挪位置会让行为反**，必须说清楚：

- `:1324` 要求句柄「在物理资源关闭**前**已在原 resource 上回滚/关闭并从 actor 注销」。
- 但 §4.2 **F10**「句柄非空 → 关闭而非归池」把 `registered_handles` 的空/非空当作**归池判定的输入**。
- 现状 `close_resource`（`fake_resource/ops.rs:503`）把 `reclaim_registered_handles_on_close`（`:629`）排在归池判定**之后**，正是为了不违反 F10——若挪到判定之前，判定看到已注销干净的 `registered_handles == 0`，会把带句柄的资源**归池**而非关闭。

⇒ `:1324` 与 F10 不是「挪一下」能同时成立的。**唯一能真正统一的构造是让归池判定读注销前的快照**：先快照句柄登记册 → 注销句柄 → 关闭资源 → 释放许可。这样 F10 读到的是旧快照，仍判「关闭」；CM-74 的 `handle closed → resource Closed → permit -1` 顺序同时成立。

现状记录：宿主编排路径**已符合** `:1324`（`harness/tests.rs:588` 明写 `handle closed → resource Closed → permit -1`），**仅驱动直连路径不符合**。原「两条路径本就不必共享顺序」的协调者建议已被用户否决。

> 台账更正：本节此前把该测试文件记作 `packages/runtime/src/resource/harness/tests.rs`——**已失效**，合并后实际位于 `packages/runtime/src/connection/testing/harness/tests.rs`。以实测路径为准。

### ② 隧道建在 runtime

CM-32（及 CM-27 / CM-28 的隧道子项）的引用计数**建在 `packages/runtime`**，不是宿主 `src-tauri`。这意味着 runtime 要**新长出一个目前为零的隧道概念**。此前实测：宿主已有 `src-tauri/src/tunnel/` 3 文件 + `ssh_tunnel.rs`，`src-tauri/src` 内 927 处命中，但 `packages/runtime` 内**隧道命中 0**、两层**引用计数均为 0**。

### ③ CM-61 / CM-64 显式豁免

`ArtifactStore` 在 `platform-development-plan.md` 中零实现方、零调用方，服务端实现 + schema migration 在 **P7**。CM-61 / CM-64 的 H 断言记为 **P3 出口门禁显例豁免，理由指向 P7**，不得记为「覆盖不足」。此口径与 CM-70 一致：**要求的能力本阶段不存在时，要么建，要么显式豁免并指名由谁何时建。**

---

## CM-32 方案裁定（协调者复核后批准）

### 更正：协调者派活前提错误

派活时写「runtime 要新长出一个目前为零的隧道概念」——**错**。Coder 的机械查证推翻，协调者已逐条复验属实：

| 事实 | 实测 |
| --- | --- |
| `trait NetworkProvider` | `packages/platform-api/src/ports/network.rs:68`，冻结上游 |
| `TunnelSpec { route_ref, via_host, via_port }` | 已在端口，`derive(PartialEq, Eq)` |
| `ensure_tunnel` / `release_tunnel` | `:80` / `:87`，**均为 async** |
| 语义注释 | `:7`「共享隧道引用计数在端口内维护」；`:58`「同一 `TunnelSpec` 共享引用计数」；`:86`「归零才真正拆除；重复释放是幂等的」 |
| 端口自带锁定测试 | `:99` `tunnel_sharing_is_keyed_by_the_whole_spec_not_just_the_route` |
| `impl NetworkProvider for` | **0 处** |
| `.ensure_tunnel()` / `.release_tunnel()` 调用方 | **0 / 0** |
| `TunnelSpec` / `TunnelBinding` 使用 | **0** |
| runtime → platform-api | `packages/runtime/Cargo.toml:24`，F-04 合规 |
| 跨代隔离 | `resource/manager.rs:163` `ResourceError::PoolKeyRotated` |

⇒ **准确定位是「契约早已写死，缺实现方与调用方」，不是发明概念。** 原风险等级判断作废。

### 批准项

- **落点**：新建兄弟模块 `packages/runtime/src/tunnel/`，**不塞 `resource/`**（隧道寿命长于单条 `Lease`，塞进去会破坏 `ResourceError` 单一拒绝出口纪律）。宿主 `TunnelKind` / `Tunnel` 枚举**都不进 runtime**——镜像宿主枚举等于把部署侧传输形态漏进传输中立内核。
- **「同版本」= `TunnelSpec` 全等值**，与 `PoolKeyGeneration`（管物理连接复用资格）、`CacheRevision`（管慢结果回填）不同源。共享键比 PoolKey **更细**：同一 `networkRouteRevision` 内两条不同 `TunnelSpec` 仍各开隧道。
- **不碰 `src-tauri`**。理由：三条断言全是台账算术；`shared-boundaries-and-ports.md:590` 已写死隧道生命周期由桌面 `NetworkProvider` 实现承接（接缝期活）；本仓先例 `ResourceManager` / `ExecutionGateway::new` 在 `src-tauri` 均 **0 处**接线。桌面实现**另立轨**，P3 出口门禁不依赖（CM-32 是 H 层）。
- **CM-27 / CM-28 按「部分覆盖」处理，不许顺手标成已覆盖**。剩余项须写成可被后续轨直接接手的清单。

### 追加硬要求：唯一计数铁律

`TunnelBinding.ref_count` 字段注释明文「**仅供观测，不用于判断能否释放（释放以 `release_tunnel` 的调用为准）**」，而 `:7` 说计数「在端口内维护」。

⇒ **全系统只能有一个引用计数器**，归 `NetworkProvider` 实现（即 `TunnelLedger` 本身），**不得 ledger 数一套、transport 再数一套**。双重记账会让 CM-28「不多减引用」与 CM-27「许可归零」同时失效，且极难排查（两边各自都"对"）。

必测代数不变量：N 次 `ensure`（同 spec）+ M 次 `release` ⇒ `transport.close` **恰好 N−M 次**、计数**任何时刻 ≥0**、重复 `release` **不二次 close**。

### 追加：「集成」不许留成"可选"

`shared-boundaries-and-ports.md:216` 的「ResourceManager 归还时释放一次」二选一：**(a)** 接上并测「一次归还 = 恰好一次 release」；**(b)** 不接但在 `progress.md` 明确记为剩余项并指名后续轨。**二选一可以，「可选/看情况」不行。**

---

## CM-74 顺序统一方案裁定（批准）

### 更正：协调者的「两判据冲突」框架错误

上一节把 CM-74 表述成「`:1324` 与 F10 两条判据打架，必须二选一或找折中」——**错**。实测 `connection-management.md:1324` 的断言子句表**本身就同时写着两句**：

- 「句柄在物理资源关闭**前**已在原 resource 上回滚/关闭并从 actor 注销」
- 「driver 返回 `Clean` 时若宿主仍有已登记句柄，宿主检查**必须失败**（§9.4）」

F10（`fake-runtime-fixtures.md:221`）对第二句只是**回指 CM-69 / CM-73 / CM-74**。

⇒ 不存在文档冲突。**注销前快照不是绕过判据的折中，是 CM-74 这条断言的字面要求**——§9.4 问的是「关闭开始前资源上挂没挂着句柄」，不是「注销之后还剩几个」。上一节的「无法靠挪位置同时满足」框架作废。

### Coder 驳回协调者两个提案，驳得对（均已撤回）

- **显式布尔标志**：会造第二个真相源，与 `slot.handles` 漂移并需额外维护不变量。快照不新增状态、不漂移、无新不变量。
- **把快照喂给 `CloseResourceRequest.registered_handles`**：该字段今天**惰性**（`ops.rs:576` 读 `slot.registered_handles()`），`harness/mod.rs:173` 传的是注销后的值，`cm73.rs:155-156` doc 明说那只是「调用方的声明」。接线会把负担推给调用方，且与宿主路径形状冲突。**快照必须取在 `close_resource` 内部。**

技术前提已复验：`close_resource` 是同步 `pub fn`（`ops.rs:503`），快照+注销+判归池可放进**同一把 `Mutex` 的一个临界区** ⇒ 无观察窗口、无需等锁、无 TOCTOU。

### 批准的实施要点

合并临界区（快照 → 注销 → 判归池）；句柄注销改无条件，但 `slot.transaction_state = None` **仍以 `rolled_back` 为门**（关闭语义 ≠ 回滚语义）；归池读快照，`ReturnedToPool { registered_handles }` 填实数不再写死 `0`；F11 与 `Closed`/`permit -1` 记录原样不动。

### 批准改写 `:539` doc 注释

该注释记的正是已被撤回的建议，留着会误导后续实例。**只许改理由散文，断言一字不动**：保留前两句机制描述（仍正确），改动处留注释说明「该理由随 CM-74 统一裁定更新」，测试函数名与 `vec![...]` 断言不得动。

### 仓库隐患（另立清理项）

`fake_resource/tests.rs` 已 **790/800 行**，仅剩 10 行余量，且是全仓共享测试基础设施。后续任何轨往里加测试都会撞墙。本轨不动它。

### 附录：本轨不覆盖

CM-74 **§7.4 第 6 项**（`setSessionContext` 的 `requiresReplacement` 候选资源 + 原子发布协议）**不在本轨，需另开轨**。

---

## 轨 p3-cm74-release-order 合并记录（Tester `f39235cc` **PASS**）

合并于 main `07c77d406`（`--no-ff`），代码提交 `712b43012`，Tester 验收点同为该哈希。`progress.md` 已按纪律销毁并单独提交。三棵工作树（轨 / Tester / 演练）与 `/tmp/dz-target-{ta-cm74,mg-cm74}` 已清理，feature 分支已删。

### 合并演练（detached 树，跑满全部目标）

```
MERGE_EXIT=0   冲突文件 0 个
引入 4 个文件：ops.rs / harness/tests.rs / journal/core.rs / progress.md
TEST_EXIT=0    20 个目标全 ok    TOTAL passed = 564    FAILED 目标数 = 0
HEAD_BEFORE=HEAD_AFTER=29edb02c3    DIRTY=0
主分支门禁：typecheck EXIT=0；vitest EXIT=0
  Test Files  565 passed (565)
  Tests  5920 passed (5920)
```

### Tester 变异结论（M1/M2 真击杀，M3 非真变异）

| | 变异 | 结论 | 证据 |
| --- | --- | --- | --- |
| 负控 | — | 绿 | `EXIT=0 \| ok. 1 passed; 0 failed; 383 filtered out` |
| M1 | `ops.rs` 回退到 `712b43012^` | **KILLED** | `EXIT=101`；`the_driver_direct_close_releases_in_the_same_cm74_order` 失败，panic 逐字：`harness/tests.rs:645:5` `CM-74 要求两条关闭路径产出**同一条**顺序；句柄还挂着就归还预算占用，I1 与 I5 一起破`；right: `["handle closed","resource Closed","permit -1"]`，实际 `["resource Closed","permit -1","handle closed"]` |
| M2 | 快照挪到 `slot.handles.clear()` 之后 | **KILLED** | `EXIT=101 \| test result: FAILED. 381 passed; 3 failed; … finished in 30.02s`。三条同时中：`fake_resource::tests::f10_…` panic `fake_resource/tests.rs:530:5`「句柄未清空时禁止归池 —— 只能关闭。实际事件序列：["Created","OpeningReady","ReturnedToPool","Closed"]」；`a_resource_still_holding_a_handle_is_never_returned_to_the_pool` panic `harness/tests.rs:710:5` `left: 1   right: 0`；`cm73_threads::a_real_eviction_thread_meets_a_real_thread_holding_a_transaction` panic `cm73_threads.rs:257:9`。恢复后 `EXIT=0 \| ok. 384 passed` |
| M3 | 判据改读 `resource.registered_handles()` | **NOT KILLED — 非真变异，如实登记** | `M3_EXIT=0 \| ok. 384 passed` |

M3 的无反应原因（Tester 独立给出，比 Coder 的说法更强）：`ops.rs:573` 的 `None` 分支**是可证明的死代码** —— 顶层 `resources` IndexMap 只在 `ops.rs:191` 插入、从不删除，故 `:549` 的 `get_mut` 恒为 `Some`；且 `resolve()`（`handles.rs:200-208`）在同一把锁下 `.cloned()`，而 `slot.handles` 在 `:508`→`:554` 之间无任何写入 ⇒ 两个表达式**可证明同值**。

M1 与 M2 的杀手**正交**：M1 只杀前者，M2 杀后者并连带另外两条。二者互不替代 —— 这是判两条用例非空跑的依据（前者虽无显式 `assert!(declared > 0)`，但其精确序列断言严格更强，要求句柄条目存在**且**排第一；后者有显式前置并数 `ReturnedToPool` 出现次数）。

### ★★ 假绿陷阱：`cargo test` 匹配不到任何测试时退出码是 **0**（本轮最重要的一条）

**错在我自己的简报里。** 协调者给 Tester 的守卫清单中把 `fake_resource/tests.rs:440` 的真名 `f10_a_clean_reset_does_not_license_returning_the_resource_to_pool` 写成了漏掉 `_the_` 的 `f10_a_clean_reset_does_not_license_returning_the_resource_to_pool`。Tester 发现后用真名重跑了全部九次。

协调者**自行复现**（第一次用错语法，见下）：

```
EXIT=0   an_idle_resource_with_no_transaction_still_returns_to_the_pool   ok. 1 passed; 0 failed   ← 真名
EXIT=0   f10_a_clean_reset_does_not_license_returning_the_resource_to_pool   ok. 0 passed; 0 failed   ← 错名，假绿
EXIT=0   this_test_name_does_not_exist_at_all_xyz                          ok. 0 passed; 0 failed   ← 乱写，也假绿
```

**由此成为常设纪律：任何过滤运行一律以 `passed` 计数判定，绝不以退出码判定。** 简报给出的测试名**不可直接采信**，必须先用 `-- --list` 取全限定名再跑。

附带一条语法事实：`cargo test -p X --lib --filter NAME` **不是合法 cargo 参数**，报 `error: unexpected argument '--filter' found` / `tip: to pass '--filter' as a value, use '-- --filter'`，**EXIT=1 且完全没有 `test result:` 行**。合法形式是位置参数 `cargo test -p X --lib NAME` 或 `cargo test -p X --lib -- --filter NAME`。协调者第一次验证时正是踩了这个坑，三个探针全返回 EXIT=1 —— 那是命令行报错，不是测试结果。

### ★ F11 `CloseUnconfirmed` 分支当前执行 **0 次**（缺口比原记录更严重）

`ops.rs:512-536` 的 F11 提前返回，在全部 384 条 `--lib` 用例中**一次都没执行过**。

- 全树唯一构造 `FaultKind::CloseUnconfirmed` 的地方是 `script.rs:343`，位于 `a_fault_only_fires_on_its_own_operation`（`script.rs:339`）内；该用例只 `FakeScript::new()`，**从不构造 provider、从不调 `close_resource`**。`script.rs:183` 只是 `label()` 映射。
- 那条名字极像保护的 `close_unconfirmed_keeps_the_permit_occupied_forever`（`journal/tests.rs:131-162`）**直接构造 `CommandJournal::default()`** 并调 `record_permit`/`record_resource_event`，**对 `ops.rs:512-536` 零保护**。

⇒ **「既有缺口可顺带覆盖」的说法作废**；台账口径应为「该分支当前 0 次执行」。这是既有缺口，非本轨引入；危险在于一个同名守卫**读起来像**覆盖。F11 维持独立窄轨（L4），**不得与拆分轨捆绑**。

### CM-74 判据 `connection-management.md:1324` 仍为 **PARTIAL**（合并 ≠ 转绿）

| 断言 | 覆盖 | 证据 |
| --- | --- | --- |
| 句柄在物理资源关闭前已在原 resource 上回滚/关闭并注销 | ✅ | 两条新用例 + M1/M2 击杀 |
| **前置条件（须含游标句柄）** | ❌ | 两条新用例都只用**事务句柄**，`handles.rs` 的**游标句柄从未出现** |
| **b** 提交落在旧 resource 或显式失败，绝不落新 resource | ⚠️ 部分 | 驱逐用例只断言「不得归池」，未断言提交落点；`setSessionContext` 未动 |
| **c** §9.4 driver 报 `Clean` 而宿主仍有句柄 ⇒ 宿主检查必须失败 | ⚠️ 未覆盖 | `FaultKind::CleanButPreconditionUnmet` 仅在 `fake_resource/tests.rs:461` 经 `ResourceOp::Reset` 注入、由 `ops.rs:478` `reset_resource` 处理；**`close_resource` 对它引用 0 次** ⇒ 关闭路径的故障注入变体未覆盖 |
| 「未登记句柄在归还时被拒」 | ✅（别处） | `harness/tests.rs:403`/`:357`/`f12_…`，但不在 CM-74 旅程内 |
| **e** actor 终止后不重建句柄，只能以新 `dbSessionId` 恢复 | ❌ | 全树无用例；`close_active_session` 仅出现在 `journal/tests.rs:474` |

### 更正：本轨对 `registered_handles` 的自述夸大（L1，协调者复核）

Coder 自述「`ReturnedToPool` 现在填真实数字而非硬编码 `0`」——**夸大**。协调者复核合并后的 `ops.rs`：该字段位于 `if drained && handles_before_close == 0`（`:592`）之内，故 `registered_handles: handles_before_close`（`:596`）在**每个发射点必然为 0**，与旧的硬编码 `0` **可观察等价**。

真实行为变更在**别处**：(a) 句柄注销由 `if rolled_back { … }` 改为**无条件**；(b) 归池判据由「注销**之后**重取」改为读**注销前**快照。**代码无需改动，但合并信息与任何复述都不得沿用「填真实数字」的措辞。**

### 未被冻结的推理（非缺陷，但当前不可证伪）

`ops.rs:545-551` 的注释断言「快照与注销同处一把锁 —— 中间没有观察者，也就没有 TOCTOU」。现有用例钉的是**取值**，不是**取点**：把快照挪出临界区**不产生任何测试反应**。安全性另有可证分析（同一把 `Mutex` + `slot.handles` 在 `:508`→`:554` 间无写入），但**该结论目前只存在于注释里**。

### 口径更正：测试目标数

此前记作「20 个二进制」，实测为 **19 个 `Running` + 1 个 `Doc-tests` = 20 个测试目标**。

### 偶发红如实记录

Tester 跑 `--lib` **8 次，8/8 全绿**；历史约 12.5% 的 barrier 偶发红在本轨**未复现**。原因是这两条新用例**单线程、无 barrier**，不属于那一类。**未做任何「重跑到绿」。**

### 新基线（main `07c77d406`，与 Coder 自报独立测量一致）

| | 基线 | CM-74 后 |
| --- | --- | --- |
| `src/lib.rs` | 382 | **384** |
| `gateway_contract` | 51 | 51 |
| `p3_session_port_contract`（冻结） | 10 | 10 |
| **TOTAL** | **562** | **564** |

delta `+2/+2` 精确。冻结面实测：`connection/port.rs` 空 diff、`fake_resource/tests.rs` 空 diff（790 未动）、`hub.md` 空 diff。`--features test-harness` 的 `cargo build` EXIT=0 且 warning 0 —— **不带该 feature 的 build 不覆盖被改文件**（改动在 `test-harness` 之后），别把前者的 0 warning 当成被改代码的 0 warning。

### ★ L3 硬阻塞：`harness/tests.rs` 已 **800/800**，零余量

CM-74 把该文件从 676 推到 **800**，正好撞上限。**任何 CM-74 后续用例落地前必须先拆分**（建议新开 `harness/cm74_release_order.rs`，照既有 `cm73_threads.rs` 的样子）。L5（判据补齐轨）因此被硬阻塞。

其余逼近上限：`fake_resource/tests.rs` 790/800（余 10）、`fake_resource/ops.rs` 750/800（余 50）。

---

## CM-32 验收点冻结，Tester 已派出

代码提交 `2836d346d`（11 个文件全 `.rs`，最大 439 行，`lib.rs` 38 行），diff 与宣称的完全一致。Tester `ee7b59f0` 在 `.worktrees/ta-cm32` 以 detached HEAD `2836d346d` 作业，**该哈希冻结：不得 amend / rebase / force-push**。

Coder 自报 `--lib` 405 / 全量 592 / 21 个二进制 —— **系对过期基线 `21e27e4eb` 测得**；CM-74 合入后 main 已是 `--lib` 384 / TOTAL 564。Tester 须自行实测真实数字，对不上就报告对不上。

预通知 Tester 的三处疑点：① `TunnelBinding.ref_count`「惰性」用例是否**可被杀**（判据改回读快照须变红）；② `close_calls() == 0` 是否在空实现下恒真（须有真会关闭的 spec 做负控）；③ `N < M` 与 `ref_count` 下探为负是否被钉死。

### Coder 交付后协调者独立复核（登记语气；最终裁决归 Tester）

冻结哈希实测未动：`2836d346de129f84bfcd184eb32f77dbfab44f46`。Coder 在 Tester 开树**之后**追加的两个 `progress.md` 提交都是纯文档，11 个 `.rs` 一字未碰。演练树 `af5baa65` 确认**未回流到分支**，分支 `main..HEAD=3` 且工作区干净。

**已实测成立的部分**：`ledger.rs` 释放决策（`:295-335`）为 `refs = refs.saturating_sub(1)` → `if refs > 0 { return }` → 归零才 `transport.close` 且恰好一次。`close_calls` 的出现位置**只有四处**（字段 `:143` / 初始化 `:151` / 自增 `:316` / 只读访问器 `:372-373`），**从不作为任何条件式的操作数** ⇒ 不参与释放判断。四个改引用的方法 `acquire(:174)` / `release(:245)` / `return_resource(:270)` / `report_failure(:340)` 全部 `&mut self`，只读访问器全部 `&self`。此层收窄是对的。

**已实测不成立的部分（「结构上装不下第二个计数」应降级）**：Coder 主张 `TunnelTransport` 方法全 `&self` ⇒ 包在 `Arc<dyn TunnelTransport>` 里的实现**在结构上装不下第二个计数器**。**该论证无效，且它自己的字段清单就是反证** —— `RecordingTunnelTransport` 已有 `journal: Mutex<Vec<TunnelEvent>>`，`HostTunnelTransport` 已有 `events: Mutex<Vec<&'static str>>`，两者都已在用内部可变性；再加一个 `count: Mutex<u32>` 完全编译通过且不违反 `&self` 契约。trait object 只保证拿不到 `&mut self`，而 `Mutex`/`Cell` 不需要 `&mut self`。

⇒ 「结构保证（改不动）」必须降级为「**经字段审计，当前不存在**（改得动，只是现在没人这么写）」。这两句在验收结论里含义完全不同。**待 Tester 裁决的关键问题：把两个 fake 各藏一个 `Mutex<u32>` 独立计数，现有测试会不会红？** 会红 ⇒ 铁律可证伪但只能靠测试钉；不会红 ⇒ 铁律根本没被钉住，是真缺口。

**这条与 CM-74 轨 Coder 对 `registered_handles` 的自述是同一类夸大**（那次复核结论同为「实测必然为 0」被写成「填真实数字」）。**同一 Coder 连续两轨在同一处放宽论证强度** —— 说明是表达习惯而非本轨偶发，后续各轨 Tester 一律不得采信「结构上/类型上保证 X」这类断言，必须要求可对质的代码依据。

---

## CM-32 Tester 判决：ACCEPT 可合并，但点名两处合并前必修

验收点 `2836d346d` 未动，首尾 `git status --porcelain` 的 sha 同为 `e3b0c442…`（空树），四门禁全绿，diff 纯增量 **11 文件 / +2301 / −0**。⇒ **不是干净 PASS**，已打回原 Coder `bffa94ba` 走 repair round 1，修完派**全新** Tester。

### 我提的三处裁决，Tester 全部独立复核

**① `close_calls` 穷尽性成立**：它自己 grep 得 L143/L151/L316/L372/L373，无一是任何条件式操作数；释放决策从不读该计数。

**② 唯一计数铁律的答案比「类型挡不住」更糟 —— 根本没被钉住**：

- 字段审计：全 `src/tunnel/**` + 契约测试中 `u32|u64|usize|i32|i64|AtomicUsize|Cell<|Arc<AtomicUsize>` 命中**恰好 2 处**，都在 `ledger.rs`（`refs: u32` `:100` / `close_calls: u64` `:143`）。
- **M1a**：给 `RecordingTunnelTransport` 加 `close_tally: Mutex<usize>` 并让 `close_calls()` 改读它 ⇒ **编译通过，全绿 405 + 7**。
- **M2（决定性）**：给 `TunnelEntry` 加一个**始终同步**的 `shadow: u32`，并让 **`drain()`（唯一释放决策点）改为按它决策** ⇒ **编译通过，全绿 405 + 7**。
- **M3**：同一个 shadow 不同步 ⇒ 红 `395 passed; 10 failed`。

⇒ **两个 fake 各藏一个独立计数器，现有测试不会红。测试钉的是「发散」，从不钉「唯一」。** 真正的约束是「决策从哪里读」（唯一输入 = `TunnelEntry.refs`；`TunnelTransport` 返回类型不携带计数），仍属**审计结论**而非结构保证。**登记为 CM-32 已知名洞**，另开跟进轨（可选修法：断言释放路径只读一个计数 / 让 `drain` 按值从单个私有方法取计数而非直接读结构体字段）。

**③ 论证强度分级**：Tester 进一步在 `transport.rs` 模块头找到**同一类夸大的第二处** —— 「`&self` ⇒ 结构上就无法维护任何内部引用计数」为假（`&self` 只拿不到 `&mut self`，而 `Mutex`/`Cell` 从不需要 `&mut self`；`TunnelTransport: Send + Sync`）。**同一 commit 内两处**，坐实是表达习惯。但 Tester 同时确认 **`progress.md` 本身诚实**：CM-27/CM-28 标注「已覆盖」逐条为真，标注「剩余」的确实未闭合（`TunnelFault` 只有 `{Open, Close}` ⇒ 握手中途失败**根本无法表达**；`CleanupDisposition::Closed` 在 `src/tunnel/*.rs` 中 `grep -c` 为 **0**；`Quarantined` 归属缺失；CM-28 ④ 20 并发归还、⑤ 并发预算）。**无夸大。**

### 我预警的第 1 处疑点：确认成立（协调者错，对）

`the_binding_snapshot_never_decides_whether_to_release` 末句 `assert_eq!(ledger.ref_count(&spec), Some(1))` —— **M4 把权威读换成冻结快照后仍然通过**（已单独确认确实执行，`1 passed; 0 failed; 404 filtered out`）。**两个来源在断言点恰好都等于 `1`，无法判别。** Tester 找遍全轨**没有它的独占杀手**。修法：第三次 acquire 让台账读到 `2`、首份快照仍为 `1`，`!=` 才可断言。

### Tester 撤回了自己第一轮的一个结论（更正要传下去）

它曾称两个 close 计数器语义不一致、关闭失败路径上 `close_calls == N−M` 会失配 —— **错**。假件 `close()` 在 journal **之前**就返回 `Err`，两者按构造只计成功；且 **M11 证明这份一致性被断言了**。**这是优点**，撤回而非发布。

### 协调者两处前提被 Tester 纠正

1. **「不带 `test-harness` 的构建不覆盖被改文件」对 CM-32 是反的**：`test-harness = []` 只闸 `connection::testing`；`pub mod tunnel;` 在 `lib.rs:31` **无条件**，`tunnel/mod.rs:51-53` 的 `error`/`ledger`/`transport` 未加 gate。裸 `cargo build -p datazen-runtime` **确实覆盖全部被改生产文件**。（该规则对 CM-74 的 `connection/testing/**` 仍成立，**不得推广**。）
2. Tester 自陈首轮 `--test-harness` 跑出 `Finished in 0.11s` **是缓存、什么都没证明**，是 `touch` 后强制重建才发现的。⇒ **「门禁输出秒完」本身就该怀疑是缓存。**

### 变异台账（每条前负控 `405 passed`；每次还原后 `touch`）

| ID | 变异 | 结果 | 真杀？ |
|---|---|---|---|
| M1a | fake 内藏 `Mutex<usize>` 计数 | 绿 405/7 | ❌ |
| M2 | `shadow: u32` 始终同步，`drain()` 据此决策 | 绿 405/7 | ❌ **决定性** |
| M3 | shadow 发散 | 红 395/10 + 契约 5 | ✅ |
| M4 | 权威读 → 冻结快照 | 单独跑 `1 passed` | 空转探针 |
| M6 | 二次 `transport.close()` | 红 395/10 + 契约 6 | ✅ |
| M7 | `saturating_sub`→`wrapping_sub` | 绿 405 | ❌ 下溢路径不可达 |
| M8 | 去掉 `AlreadyHeld` 守卫 | 红 **404/1** | ✅ **独占** |
| M9 | `return_resource` 只摘不解绑 | 红 400/5 | ✅ |
| M10 | 快照 `ref_count` 恒为 1 | 红 403/2 | ✅ |
| M11 | 台账计调用数而非成功数 | 红 **404/1** | ✅ **独占** |

我的疑点 ③ 已被 `single_counter_algebra_holds`（`acquires ∈ 1..=6`, `releases ∈ 1..=6`）**证伪**：`N < M` 已被覆盖。契约 ②「`ref_count` 恒不负」为**结构性成立但无断言支撑**（`u32` 无符号 ⇒ 负值不可表示，M7 证实下溢路径在全部测试中不可达）。

`--lib` 本轮实跑 **21 次**（累计 22 次）：13 绿（负控/还原/最终）、6 为故意变异、4 设计上就该绿（M1a/M2/M4/M7）。**未重跑刷绿**，12.5% barrier 竞态本轮未现。

### 留给 seam 轨的裁定项

`TunnelError::rejected()` 与 `as_runtime_error()` 在 `error.rs` 外**零调用方**（实测 `count_outside_error_rs=0`）⇒ 整个 error→`RuntimeError` 面未被行使。且 **`AlreadyHeld`（调用方 bug）当前映射到 `SessionQuarantined`** —— 一旦 `rejected()` 拿到第一个调用方，**双重 acquire 就会把健康会话隔离掉**。必须在 seam 阶段裁定。

---

## `p3-harness-split`：Coder 连续死 3 次在 0 提交，换人后一次交付

**前置实例 `0fd9e750` 三轮全部死在 0 提交**：R1 静默、R2 静默（`mod.rs` 已 361 行）、R3 吐出损坏的工具调用片段（`<]minimax[>[…`）。⇒ 判为**模型损坏而非偷懒**，设计抢救到 `/tmp/dz-harness-split-handoff.md`（复核为正确），换新 Coder `048ce930`。该实例中途在「Let me start a background build…」处截断，用「先跑一个测试再提交」的窄路径唤醒，**一次性交付全部三个测试**。

★ **这一条印证了技能里的死线规则，但要按现象而非次数判**：损坏/截断的收尾消息 = 「无进展」，应唤醒一次，换人与否再定。

目标：`harness/tests.rs` 当时 **800 行整、零余量**（CM-74 从 676 推到 800），任何 CM-74 后续用例落地前必须先拆（故 L5 被硬阻塞）。交付 `bf5060cea`，3 文件 / **+267 / −234**，`tests.rs` **800 → 573**（新 `cm74_release_order.rs` 207，`mod.rs` 361）。门禁全绿：根 `cargo fmt` EXIT=1 **仍是那个 gitignore codegen 缺件**（`src-tauri/src/driver_init.rs`，`.gitignore:69`），定向 `cargo fmt -p datazen-runtime -- --check` EXIT=0 —— **不要为此花一轮**。`--lib` 384 / TOTAL 564 / 20 目标 三数与基线**逐字相同**（只搬不增）。

### 搬迁纯度：协调者与 Coder 两侧都用机械证据，不是「看着像」

Coder 侧：快照 800 行原件，把原件 **532–719** 与新文件 20+ 逐行 diff ⇒ **188 行字节相同**（分段头 528–531 单独 diff 亦同）。Tester 侧用两套独立方法互证（括号感知提取器 + SHA-256；`sed` 行区间 + `diff -q`）⇒ **14 个函数体逐字节 IDENTICAL，FAIL=0**，函数名并集完全相同（丢失 0 / 新增 0），搬出的**恰好 3 个** CM-74 用例、留 11 个，`vec![…]` 4 处 / `panic!`·`expect` 消息 50 处 / 续行 `\` 3 处**一条不差**。

★ **这是纯重构轨唯一的成败判据**：编译过 + 门禁绿只证明语法活下来了；有人「顺手」改断言、改期望值、改 panic 文案，**任何门禁都不会红**。逐字节对拍才堵得住。

### Tester 抓出的自述不完整（如实登记）

Coder 报了 1 条断裂内链，Tester 实测**共 3 条**：

| 位置 | 链接 | 基线 | 归属 |
|---|---|---|---|
| `cm74_release_order.rs:27` | `` [`FakeHarness::close`] `` | 不断 | 本轨新引入，**Coder 已报** |
| `tests.rs:1` | `` [`FakeHarness`] `` | 不断 | **本轨新引入，Coder 未报**（搬走辅助函数后 `use super::{…}` 不再含它） |
| `tests.rs:7` | `` [`Barrier`] `` | **基线即断** | **既有，不归本轨** |

Tester 用一对变异证明了「沉默来源」：坏链接挂在新 `cfg(test)` 模块 ⇒ rustdoc warning **38 → 38，零命中**；同一坏链接挂到确实渲染的 `pub trait SessionPort` ⇒ **38 → 39** 且逐字报出。⇒ 沉默源于 **`#[cfg(test)]` 排除**，不是 rustdoc 宽松。**Coder「零构建信号」的理由成立**，严重度低，不阻塞合并；`[Barrier]` 单独排期。

★ **Tester 主动记录了自己一度造出的假证据**：DOC-link 阴性对照首轮用 `grep … | head -6` 当判据，`head` 的退出码掩盖了 grep 空结果，打印了一句并不存在的「对照成立」，且该轮变异根本没落盘。两条缺陷叠加差点产出假证据，已重做并改用 `if grep -q`。**这与本项目「逃逸码掩盖上游失败」的既有教训同源。**

提交描述「纯搬 3 个测试」未提三个辅助函数同时 `tests.rs → mod.rs`（函数体字节相同、仍在 3 文件内、行为无害），描述该更正。

---

## CM-32 repair round 1（`566cccd00`）与 round-2 判决：CONDITIONAL PASS

### repair round 1：两处必修，Coder 找到了我漏掉的两份副本

打回原 Coder `bffa94ba`。**他改对了我点的那一处，又自查出第二份我完全没提的副本**（`tunnel/mod.rs:47`）—— 同一假话在**一个提交内有两份**。契约二进制里 `tunnel_refcount_contract.rs:435-455` 也存在**同一种空转**，而我那一轮根本没提。⇒ **「同一 commit 内多副本」必须整目录扫，不能只按简报点改。**

- 必修 1：删假话，替换为「类型系统**挡不住**这件事（不要把纪律说成保证）」。全仓 grep「结构上就无法维护」「结构上装不下」= **0**。
- 必修 2：两份都加第三次 acquire（lib `journey_single_counter.rs:274`/`:308` 用 `assert_ne!`；契约二进制 `:435-455`）。杀点证明：M4 lib `402 passed; 3 failed`、M4 契约 `4 passed; 3 failed`、负控 `405 passed`。
- 门禁：`--lib` 405 / 契约 7 / 二进制 21 / TOTAL 592 / 定向 fmt 0。12 文件 / +2695 / −0。

### round-2 判决：行为缺陷 0，**合并阻断 1 项（纯注释）**

必修 2 判为**真修好且证明了判别力**，且 Tester 做了我没想到的一步：

- M-4a 换掉新断言的数据源 ⇒ `EXIT=101`、`FAILED. 0 passed; 1 failed`、panic 在 `:285` `left: Some(1) / right: Some(2)` ⇒ **真杀**。
- **负控：同一条变异打在修复前的 `2836d346d` 上 ⇒ `EXIT=0`、`ok. 1 passed`** ⇒ 旧断言扛住了它本该禁掉的那件事。**第一轮的空转被复现并关闭。**
- M-4b 三处权威读全换成快照 ⇒ panic 在 `:274` `left: Some(1) / right: Some(1)` ⇒ `assert_ne!` 是**活守卫不是空转**，且 RHS 是测试本地的，生产代码改不动。
- 契约二进制那份**不是盲抄**：用三次 return 把 refs 打到 0（lib 那份只 return 一次），另加 seam 级 `transport.closed()` 证据。

★ **我要求它设计的那条变异落地了，而且打在我预判的缝上**：M-NEW-1（给 `TunnelEntry` 加 `handed_out: u32`，只在 acquire 写、release 不写，再让 drain 的存活判据从 `entry.refs > 0` 改成 `entry.handed_out > 1`）—— 套件级**真杀，9 处失败**。但 **lib 那份必修 2 用例在它下面 `EXIT=0` 通过了**（它只 release 一次，refs 到不了 0），**`assert_ne!` 根本没触发**（2 ≠ 1）。⇒ **`assert_ne!` 自证的是「两个来源不同」，不是「决策读的是权威那个」。** 靠 `single_counter_algebra_holds` 与契约二进制兜住。根因是 lib 副本只放一次 return。登记为**低级文档注记，不阻塞** —— 套件整体是可靠的。

M2 复核**确认 Coder 自报为真**：加 `close_tally: Mutex<usize>` 且 `close_calls()` 改读它 ⇒ 编译通过、21/21 二进制全绿、`--lib` 405。Tester 的负控（让第二个计数器重复计数）**杀掉 10 个测试** ⇒ 是真洞，不是「没人读这个字段」。**精确形状：测试钉死了观测值的数值，没钉死它从哪个字段来。**

`progress.md:288` 的零调用方声明用**墓碑法**验证而非 grep（`rejected` / `as_runtime_error` 在别的类型上也有，grep 不可靠）：单墓碑编译零错；双墓碑唯一 E0599 在 `error.rs:91` **自身内部** ⇒ 外部确实零调用。**登记准确。**

### ★ 必修 1 被判「改对了但没改全」—— 这就是唯一阻断项

`transport.rs:12-21` 与 `mod.rs:48-52` 改写正确（引了两个真实反例，措辞是「靠字段审计与评审维持」，**没把纪律说成保证**）。**但第三处漏网**：`tunnel/ledger.rs:16-18` 把那句假话**逐字保留**（「所以这里用类型系统把第二份账**在编译期消灭**」「因此它**不可能**持有需要 `&mut self` 的内部计数」），**与同目录两个文件里的反驳直接打架**；`:19` 还把 `single_counter_algebra_holds` 指到了错的模块（实际 `tunnel::journey_single_counter`，不在 `tunnel::harness`）。

**协调者已实测复现并确认阻断**（`grep -rn "编译期消灭\|不可能持有\|类型系统.*消灭" packages/runtime/src/tunnel/` **只剩这一处** ⇒ 是漏扫不是扩散）。公平地说 `ledger.rs` 本轮没碰它，属既存假话，但上一轮必修项写的是「模块头」**复数**，只改两处不算做完。已派回原 Coder 做注释清扫（零可执行代码改动）。

★ **两轮下来这个模式已经很清楚：我的「点名式」复核天然漏副本。** 第一轮我点了 1 处（实际 2 处），第二轮我判「已改全」（实际还剩 1 处）。**注释里的断言必须按目录整扫，不能按简报点改** —— 已写进 CM-32 的收尾简报。

### 分支拓扑：合并会从 main 侧动到冻结文件（Tester 主动提的）

`merge-base(main, 566cccd00)` = `21e27e4eb`，main **不是**祖先 ⇒ 分支落后 main **12 个提交**（Tester 按旧 main 算成 9，不影响结论）。⇒ 合并后要按 **`git diff <merge-base> <merged-head>`** 判冻结面，**不能用 `git diff 07c77d406..566cccd00`**（会产出假违规报告）；且合并后的测试数增减是**拓扑产物**，不得据此重开 CM-32。

---

## CM-70 Tester 判决：门禁全绿但**不判 PASS**，两个 HIGH

Coder `9fb5b438`，验收点 `c6597b803`，20 文件 / **+4210 / −4**。四门禁全绿（`BUILD/BUILD_TH/TEST/DOC` 均 EXIT=0），**22 个测试目标逐个单跑全 EXIT=0**，TOTAL passed **638**，`--lib` 410（跑两遍首过全绿），定向 fmt 0。基线自洽：`main=890ace653` / `merge-base=7fa6630f0`，638−48−26=**564**。

**这是第三次「门禁全绿 ≠ 判据转绿」**（前两次：CM-32、CM-74）。

### 🔴 H-1：签名令牌明文整条落进审计投影（协调者独立复核成立）

链条逐环实测：`token.rs:336` 拼的是 `format!("{TOKEN_PREFIX}.{version}.{payload_hex}.{mac_hex}")` —— 含 nonce 与 MAC 的**完整签名令牌串**；`IdempotencyScope::key()`（`idempotency.rs:54`）原样返回它；`gateway/mod.rs:363` 塞进 `GatewayError::IdempotencyConflict { key: scope.key().to_owned(), .. }`；`request.rs:310-312` 的 `json!` 把它写进 `to_persistable_json()`，`thiserror` 的 `Display` 也原样带出。**普通客户端输入即可触发。**

★ **这直接证伪了本次新增 `SubmissionTokenRejected` 的设计理由**：同一个提交里 `SubmissionTokenRejected { reason: &'static str }` 只带静态原因串、干净；而**另一个变体把完整凭据整条铺在同一条审计通道上**。拒绝对拒绝原因写摘要的理由，在隔壁变体上被绕过。**CM-54 冻结保护的是冲突检测的语义，不含「可以往外泄凭据」的许可** —— 已签字授权修改。

已核实 `key` 字段**下游只有 `request.rs:312` 的 JSON 投影自己**，无外部消费者 ⇒ 删除成本低。

### 🔴 H-2：A6 围栏不覆盖「驱动中途丢」（协调者独立复核成立）

`grep -rn "unverified.insert" packages/runtime/src/` **全仓唯一**（`mod.rs:372`），且**只挂 `IdempotencyLookup::Unreadable` 分支**；而 `mod.rs:509` 是 `let receipt = self.port.execute_in_session(port_request).await?;` —— 驱动报错被 `?` 直接吞掉，**这里根本没有抬围栏的地方**。Tester 探针 5（「先真发 SQL 再返回 `RuntimeError::SessionLost`」）：`PROBE5_FENCE_COUNT=0`、`PROBE5_NEW_TOKEN_ACCEPTED=exe_db_session_contract_1`、`PROBE5_IS_REPLAY=false`、**`PROBE5_SQL_ISSUED_FINAL=2`** —— 同一条 SQL 真发了两遍。判据措辞无条件 ⇒ **未闭合**。已签字授权把围栏从「账本读不出来」扩到「已下发但结果未知」，并明确 `IdempotencyPersistFailed` **不得**动（探针 4 已证明该分支正确）。

### 🟡 M-1：未申报的 CM-54 公共面新增 —— 已签字通过

`IdempotencyStore::delete`（带默认实现）、`InMemoryIdempotencyStore::delete`、`IdempotencyLedger::forget()`、`RequestFingerprint: Hash`。纯新增、源码级向后兼容，`IdempotencyRecord` 恰好 3 字段逐字节未动。**M-2 `progress.md` 合并时删除。**

### Tester 推翻了我 3 条、确认了 6 处（逐条传下去）

**推翻**：① `#![allow(dead_code)]` 掩盖死码 —— **删掉后爆 29 条真实 `never used`**，allow 有实据合法（**判定依据是告警内容不是退出码：编译 `EXIT=0` 不代表没死码**）；② 我点名的变异配对 `a_grant_..._is_never_deleted` —— **作废**，它用 `with_retained_until_for_test` 自造违规数据、走不到生产推导式；真正被杀的是另外三个，**不变量本身确实成立**，但我给的因果链是错的；③ `reserve` 失败未抬围栏 —— 探针 4 证明设计正确，已撤回。

**确认**：`cm70_no_disk.rs:357` 真的格式化 `{keyring:?}`（比简报要求的更严）、`:134` 真的用活的 `read_dir`；`TokenKeyring` 原先派生 `Debug`（**会泄出原始签名密钥**）→ 改成手写脱敏实现；`invariants.rs` 是**加强**（断言体一字未改、`800` 字面量原样）；冻结面零 diff，`harness/tests.rs` sha256 两端相同。

### ★ 简报里的过期数字（口径更正，非缺陷）

测试目标 **22 非 21**（20 个集成二进制清单与简报完全一致，多出的是 `lib` 本身）；`07c77d406` 非本 HEAD 祖先；我说的「4 个生产文件」实为 **2 生产 + 2 仅测试**。

## CM-70 repair round 1 交付（`0a9a8f6e3`），待 Tester

HEAD `c6597b803` → `0a9a8f6e3`（append-only，`merge-base --is-ancestor` 校验通过），DIRTY=0，冻结面 6 处零 diff（含 `connection/session.rs`）。

**H-1 修法**：`IdempotencyConflict` 去掉 `key`，改 `{ existing: ExecutionId, incoming: String }`，`incoming` 换成 16 位十六进制 `RequestFingerprint`。Coder 给的因果链值得记：**账本按 `(dbSessionId, runtimeEpoch, key)` 查找，冲突只可能发生在调用方自己的作用域内，所以回显的 `key` 是把自己的凭据原样递回来 —— 既零信息量又可重放**。

**★ 第一轮测试看不见的第二层泄漏**：`ExecutionRecord` 手写的 `Debug` 脱敏了顶层 `idempotency_key`，但内层 `port_request: ExecuteInSessionRequest` 携带同一个令牌，而**它自己派生的 `Debug` 会完整打印**。测量点 `cm70_no_disk.rs:478`。加 `RedactedExecuteRequest` 在内层脱敏。教训固化：**脱敏只看最外层字段是不够的，必须逐层走到派生 `Debug` 的叶子**。

**H-2 修法**：围栏抬到 `dispatch` 里，`mark_dispatch_issued` 之后、`execute_in_session(...)` 之前，键为 `(db_session_id, fingerprint)`。**绝不以令牌为键** —— 以令牌为键的围栏什么也围不住（新键就是新令牌）。刻意不看驱动的返回值。

**旁支修复**：`gateway_contract/timing.rs` 的 p95 用例原先每轮只变 `idempotency_key`（同一条写、同一个指纹），正确的新围栏从第 2 轮起就拒绝它。改为变 `call.input`，保住 20 次真实派发。**这是正确修复不是削弱**，但它说明旧用例本来就在用「换令牌重发同一条写」来造负载。

Coder 自报门禁：`FMT_CHECK_EXIT=0 / BUILD_EXIT=0 / TEST_ALL_EXIT=0 / TARGETS=22 / TOTAL_PASSED=650 / TOTAL_FAILED=0`，`unittests 411`、`cm70_idempotency_replay 46`、`cm70_no_disk 13`。

**★ 但计数清单不可信**：收尾消息把 `registry_*` 写成 `10/11/9/6/7`、`p3_session_port_contract` 写成 `7`（基线 10）、`resource_*` 写成 4 个（基线 3 个）还把 `registry_audit 6` 单列。算术上 `410+1 + 41+5 + 7+6 = 650` 恰好闭合 ⇒ 是**列表错序而非测试减少**，但这必须由带目标名的表证实，已写进 Tester 简报。**又一次印证：子代理的汇总计数不是证据，明细行集才是。**

**★ 我自己也没有答案、已交给 Tester 的两个问题**：① 判据限定词是「**未知**写入」不能换新键重试，但围栏不看驱动返回值 —— 驱动真执行 SQL 后返回**明确**业务错误时结局已知未生效，围栏却已抬起、新键被永久拒。这是保守合规还是反向过度围栏？② 围栏 `HashSet` 永不清空，Coder 理由是「清空等于断言这次写没生效」—— 但明确的语法错误恰是一个可确定未生效的时点，「永不清」未必是唯一选项。

## CM-32 注释清扫 `7db1188c0` 与合并彩排（协调者已独立验完）

### 注释清扫

`git show --stat` = `ledger.rs` 12 行 / `transport.rs` 2 行，**9 插入 5 删除**。纯注释证明：`git diff 7db1188c0~1 7db1188c0 -U0 | grep -E '^[+-]' | grep -vE '^(\+\+\+|---)' | grep -vE '^[+-]\s*(//|$)'` → **完全为空**。`packages/runtime/src/tunnel/` 按目录整扫四类谎言措辞（「编译期消灭」「不可能持有」「类型系统.*消灭」「ensure 次数」）→ **全 0**。三处副本（`ledger.rs` / `transport.rs` / `mod.rs`）口径一致。

改写后的 `ledger.rs:12-23` 逐条对锚点核过、非谎：:13「全系统只能有一个引用计数器」；:14「`[`TunnelTransport`]` **不带**任何计数字段」；:15-16 双重记账会同时失效 CM-28/CM-27；:18「但这条纪律**不由类型系统保证**……`RecordingTunnelTransport` 的 `journal: Mutex<Vec<TunnelEvent>>` 就是活证据」（该字段确实存在）；:23「代数不变量由 `tunnel::journey_single_counter` 的 `single_counter_algebra_holds` 钉住」（该测试确实存在）。

### ★ 两次都栽在同一处：注释里的断言必须按目录整扫

那句谎在 `tunnel/ledger.rs:16-18` **活过了 CM-32 的 repair round 1 和 round 2 两轮**才被我发现 —— 因为我给 Coder 的文件清单里没有它。**第二次仍是我的清单问题**。固化：注释清扫的输入必须是**目录**，不能是任何人的文件清单。

### 合并彩排（detached，`5e6ed8a14`）

`merge-base(main=6c6f9f240, 7db1188c0)` = **`21e27e4eb`**，分支落后 main **15** 个提交、领先 **5** 个。`MERGE_EXIT=0`、**无 CONFLICT**。逐二进制：**21 个目标、0 failed、TOTAL 594、`--lib` 407**，无 `error[]`、无 `panicked at`，运行期间 HEAD 与工作区 sha 前后一致。

### ★ 三项拓扑证明（都实测，不推断）

1. **那 2 处「冻结面 diff」不是 CM-32 造成的。** 相对 `21e27e4eb` 看到 `connection/testing/journal` 与 `fake_resource` 各 1 处。分支侧在**这两个路径上的改动为空**；main 侧改动是 `fake_resource/ops.rs` 与 `journal/core.rs`。**误报源是拿 `main..branch-head` 比，而不是 `<merge-base> <merged-head>`。**
2. **合并引入的内容 == CM-32 分支自身内容。** `git diff 21e27e4eb 7db1188c0` 与 `git diff 6c6f9f240 5e6ed8a14` 的文件清单**逐条一致**，各 12 文件 / +2699 / −0。合并零额外。
3. **`--lib` 407−405 的 +2 是拓扑产物，不是回归。** main 自身 `--lib` 实测 **384**；CM-32 净贡献 = 407 − 384 = **23**；分支自测的 405 建立在 main 侧 lib=382 的旧树上。**合并后 `--lib` 无任何目标减少** —— 21 个目标逐条比对，只有第 1 项 `unittests` 407 与分支侧 405 不同，其余 20 项完全一致。

教训固化：**测试计数的变化必须先用「合并树计数 − 各侧计数」对账，再讨论是不是回归。** 直接把差值当回归，是把拓扑成本误记成代码缺陷。

### 合并后仍然存在的已知洞（不改代码，但必须入台账）

`transport.rs` 那句「释放决策只读本文件的 `refs`」**今天是真的**，但测试钉的是**观测值的数值、不是它从哪个字段读来**：给 `TunnelTransport` 加一个冗余 `close_tally: Mutex<usize>` 计数器，21 个二进制全绿、`--lib` 仍 405（负控制才有 10 处失败）。`journey_single_counter.rs:274` 的 `assert_ne!` 同理 —— 它证明「两个来源不同」，不证明「决策读了权威来源」。另有两处接缝风险：`TunnelError::rejected()` / `as_runtime_error()` **全仓零调用方**（墓碑验证），以及 `AlreadyHeld → SessionQuarantined` 的归属未定。**合并后 CM-32 判据仍记 PARTIAL，不是 COVERED。**

## `p3-harness-split` 合并全绿，但 CM-74 判据 `connection-management.md:1324` 仍是 PARTIAL

**第三次同形事件**：CM-74 合并时 `MERGE_EXIT=0`、20 个目标 0 失败、`--lib` 384、main 门禁全绿 —— 判据 `:1324` 一条都没转绿。**这是「PASS ≠ 判据转绿」的第三例，与 CM-74 Tester 当初的拒绝、CM-32 Tester 点名两处必修同源。**

`:1324` 五条款的落地证据（全仓实测，非读报告）：

| 条款 | 状态 | 证据 |
| --- | --- | --- |
| a 句柄在物理关闭前已回滚/关闭并从 actor 注销 | ✅ | `cm74_release_order.rs` 三个测试：`closing_a_resource_that_still_holds_a_handle_releases_in_the_cm74_order`、`the_driver_direct_close_releases_in_the_same_cm74_order`（宿主路径与驱动直连路径**对拍**）、`a_resource_still_holding_a_handle_is_never_returned_to_the_pool`（断言 `closed == 1`，恰好一次） |
| b commit 落在旧 resource 或明确失败 | ✅ | 同上，`assert_no_leak()` |
| c driver 返回 Clean 但宿主仍有登记句柄时检查必须失败 | 🟡 | `CleanButPreconditionUnmet` 在 `src/` 有 6 处，但**无独立 CM-74 用例** |
| d 未登记句柄被拒绝返回 | 🟡 | `AlreadyHeld` 6 处，未见 CM-74 专属用例 |
| e actor 终止后不重建句柄、恢复只能新 dbSessionId | ❌ | `harness/` 下 `actor.*terminat` **0 命中** |

**外加一个至今 0 次执行的代码路径**：`fake_resource/ops.rs:513` 的 F11 分支（`FaultKind::CloseUnconfirmed` 穿过 `close_resource`）。`FaultKind::CloseUnconfirmed` 只在 `script.rs:343` 定义、`ops.rs:513` 被消费，**全仓没有任何测试构造它并走 `close_resource`** ⇒ 这段早返回代码**在全部 384 个 `--lib` 测试里执行 0 次**。它是未被测试覆盖的真实分支，不是死代码，**不得当作冗余删除**。

**这正是 CM-32 那次同一个病根的第三次发作**：合并门禁证明「改动没弄坏已有的东西」，判据要求「补上还不存在的东西」。二者是不同的命题。固化：**每条轨的合并结论必须逐条回填判据原句，而不是回填门禁退出码。**

## CM-70 repair round 1 Tester 判决：CONDITIONAL PASS，一个阻塞 + 两个必修

被测 `0a9a8f6e3` / 基线 `c6597b803`，独立 worktree `.worktrees/ta-cm70r1`，门禁首尾 HEAD 一致、工作区干净，全程未读 `.env` 内容。报告 `/tmp/cm70_logs/TA_R1_REPORT_cm70.md`。**阻塞项与两个必修项我逐条独立复核过，不是转述。**

**D1（阻塞）— `TOTAL_FAILED=0` 作为稳定门禁被推翻。** Tester 连跑 25 次 `cm70_no_disk`：HEAD **7 绿 / 18 红（72%）**，基线 **24 绿 / 1 红（4%）**。机制在 `tests/cm70_no_disk.rs:181 struct PrivateTempRoot` / `:193 std::env::set_var("TMPDIR", …)` / `:204-208 impl Drop` 还原 —— **`TMPDIR` 是进程级全局**，同一二进制内两个用例并行互相踩，炸在 `:277 entries().is_empty()` 与 `:166 list_entries`。
**归因我认**：`git diff c6597b803 0a9a8f6e3 -- tests/cm70_no_disk.rs` 里 `set_var` **零命中** ⇒ 机制存量、Coder 未引入；基线 7 用例 → HEAD 更多用例，**竞态窗口是被本提交放大的**。但门禁不能这么合：25 次里红 18 次的「全绿」不是证据。**又一次印证：单次全绿与可复现的全绿是两回事。**

**M1（Medium）— H-1 只关了一半。** `packages/runtime/src/gateway/request.rs:36-37` `#[derive(Debug, Clone, PartialEq)] pub struct ExecutionRequest`，其 `pub idempotency_key: String` 就是那把令牌；`grep "impl.*Debug for ExecutionRequest"` **零命中** ⇒ `format!("{req:?}")` 逐字打印。Tester 实测 `LEAKED=true HAS_MAC_SEGMENT=true LEN=232`。
**这是 `:1314` 那条教训的下一层**：那里是「`ExecutionRecord` 脱敏了顶层 `idempotency_key`，内层 `port_request` 派生 `Debug` 明文」；这次是 **`ExecutionRequest` 本身**（最外层、**输入侧**）仍是派生。derive 存量，但洞与本轨同源、同一文件、同一把令牌，而 `cm70_no_disk.rs:675` 只守护输出侧，**输入侧零测试覆盖**。固化：**脱敏要逐层走到最外层那个派生 `Debug`，且输入侧与输出侧各要一条对称断言。**

**M2（Low）** `RedactedExecuteRequest` 只脱敏 `idempotency_key`，`call`（含 `serde_json::Value`）原样输出。

**四项待裁全部 PASS / 非缺陷：**

| 项 | 裁定 | 关键证据 |
| --- | --- | --- |
| (a) H-1 令牌回显 | **PASS** | `key` 从变体 / `Display` / `to_persistable_json` 三处全消失；`cm70_no_disk.rs:713` 无 `key` 字段仍编译（编译期佐证）。**令牌按构造不可从 16-hex 指纹反推**（FNV-1a 原像显式不含 `idempotencyKey`）。残余仅 payload oracle 与 64 位碰撞，**非凭据泄漏** |
| (b) H-1 二层泄漏 | **PASS（带残余）** | 基线实证：`RedactedExecuteRequest` **ABSENT_AT_BASELINE**、基线 `ExecutionRecord` 为 `#[derive(Debug, Clone)]`、守护测试 **TEST_ABSENT_AT_BASELINE** ⇒ 泄漏在 `c6597b803` 确实开放且当时无守护，HEAD 修复有效。`src/` 286 个派生类型中 35 个带凭据字段，除 M1 外均在冻结面或无 `{:?}` 打印点 |
| (c) H-2 围栏 | **PASS** | 位置逐字 `mod.rs:558-588`（`mark_dispatch_issued` 之后、`execute_in_session` 之前）、键为 `(dbSessionId, 指纹)`；**PROBE5 复测 `AFTER_DISPATCH SQL_ISSUED_FINAL=1 FENCE_COUNT=1`、`AFTER_RETRY 1/1`（2→1、0→1）**；`git diff … \| grep PersistFailed` 无命中；`expiry.rs` 已就地写明裁定理由；`retries.rs:290-294` 冻结为常驻回归 |
| (d) Q1 | **符合判据的保守实现，Low 非缺陷** | 判据「不能用新键自动重试**未知**写入」是**禁止性**条款，放行不是它的要求；「已知」在网关侧不成立（网关只有 `RuntimeError`，拿不到驱动内部先后知识）；反向错误代价不对称；逃生舱 `resolve_unknown_outcome` 存在且有专测。**不改行为，仅补一句文档澄清** |
| (d) Q2 | **Coder 理由属实，判为技术债** | `connection/execution.rs:97-118` 的 `EffectOutcome` 存在且文档明写与 `ExecutionErrorCode` 正交，但**没穿过网关端口**（`ExecutionReceipt` 无 `effect_outcome` 字段）；`ProviderError::SqlError(String)` 是扁平变体，把「计划期拒绝（确定未生效）」与「执行中途失败（可能部分生效）」压成同一形态 ⇒ **今天驱动返回值确实无法确定性证明未生效**。代价递增（`unverified` 只在 `resolve_unknown_outcome` 里 `remove`，会话越长累积越大），判为**可接受但属技术债**。解法：给 `ExecutionReceipt` 接上 `effect_outcome`，在 `NotStarted/RolledBack` 时撤围栏，而非永不清。**排跟进轨，不阻塞本轮** |

**变异测试四方向全部按预期**（每次改完 `git diff --stat` 证明落地、判定用 `grep -q`、还原后 `touch` 全部 `src/**/*.rs` **和** `tests/**/*.rs`）：V1（围栏退回 `accept` 的 `Unreadable` 分支）🔴 `43 passed; 3 failed`，并**逐字复现原缺陷** `TA_V1_AFTER_RETRY SQL_ISSUED_FINAL=2 FENCE_COUNT=0`（对照 HEAD 的 `=1/=1`）⇒ 因果性双向证明；V2（围栏键 `fingerprint`→令牌）🔴 `43 passed; 3 failed`；NEG（只改注释空白）🟢 `46 passed; 0 failed` + `ONLY_COMMENT_LINES_CHANGED`；H-1（`incoming` → `scope.key()`）🔴 `cm70_no_disk 11 passed; 2 failed`，**正好打中两条专用泄漏断言**（11 条确实通过，已排除 0 命中假绿）。**这一组是本轮最硬的证据：每个修复都有真实 kill，且配了负控。**

**计数**：22 目标 TOTAL 638 → 650，**无任何目标下降**，仅 `lib 410→411`、`cm70_idempotency_replay 41→46`、`cm70_no_disk 7→13` 三处按预期变化。`TARGETS=22 / TOTAL_PASSED=650` 属实；**但「`TOTAL_FAILED=0`」作为稳定门禁被推翻**。

**两处交接勘误（照 Tester 的更正，我 `:1320`/`:1322` 的旧表述作废）**：
1. `grep "unverified.insert"` 在 HEAD **零命中** —— 已重构进 `GatewayState::raise_unknown_outcome`（`mod.rs:269` 定义、`:433` 调用、`:579` 注释）。后续一律用 `raise_unknown_outcome`。
2. **Q1 探针方法学**：Tester 的探针首版只 `accept` 不 `dispatch` 重试，量到 `=1` **差点误判「没被拦」**；**SQL 是 `dispatch` 才下发**。修正后 V1 侧才复现 `=2`。任何涉及围栏的回归测试，重试必须打在 `dispatch` 上。

**结构债**：`gateway/mod.rs` 已 **799 行**（AGENTS.md 建议上限 800）。本轮只改 `request.rs` / `tests/`，不动它；但 **Q2 跟进轨动手前必须先拆模块**，否则必然超限。

## CM-32 注释清扫第三轮：`mod.rs:50` 是第三份副本，而我下错了禁令

Tester `a4c17d92` 对 `7db1188c0` 判 **PASS**（文档清扫的合并阻塞项解除），同时报了一条 LOW 不精确，我**独立复核为真**：`ledger.rs:293` 的 `if matches!(entry.state, TunnelState::Closing | TunnelState::Unconfirmed)` 在 `:294-298` 直接 `return`，而 `entry.refs` 要到 `:302`（`debug_assert`）/`:306`（`saturating_sub`）/`:308` 才读 ⇒ `drain()` 的决策**同时读 `state` 和 `refs`**，而 `ledger.rs:22` 说「释放决策只读本文件的 `refs`」、`transport.rs:28` 说「释放决策的唯一输入是 `TunnelEntry::refs`」，**字面上都被同一个 `drain()` 证伪**。这不是谎（`state` 是终态守卫、不是第二本账，唯一计数铁律完好），但字面不准确，且**同一句话住两个文件**。

### 我的问题前提错了，被 Tester 推翻

我问「要不要把『测试钉的是数值、不是字段来源』这个洞写进头注释」以补诚实度。Tester 判 **PASS，不要加**，四条理由：① 该事实已逐字写在 `transport.rs:33-37`（连 `close_tally: Mutex<usize>` 实证都点名了），`mod.rs:51` 与 `ledger.rs:22` 都指向它，三份头一跳可达；② 头注释从未声称编译器保证，写的是「靠字段审计与评审维持」，**M-2 正是那句话成真的预测**；③ 加它违反 AGENTS.md「缺陷结论…最终必须变成代码、测试和 `docs/` 正式文档里的事实」—— 会把一个**可修的洞冻成永久架构断言**；④ 复刻 Finding A 的四份漂移。**裁定采纳，不加。**
★ 可推广：**裁决的不只是代码，一个 Coder/Tester 可以推翻协调者*问题*的前提。判问题的前提，和判它的答案同等重要。**

### ★ 同一句话住三个文件、只清了两处 —— 第三次复发

round 3 我指示改 `ledger.rs:22` + `transport.rs:28`，并**明确说「`mod.rs` 不动，`:48` 说的是另一句，没有此缺陷」**。Coder 扫目录后发现 `mod.rs:50-51`「真正的约束是「释放决策只读 `refs`，[`TunnelTransport`] 的返回值不携带计数」」**正是第三份副本**。我复核为真：`:48` 确实成立，但 `:50` 才是缺陷本身。**Coder 遵守禁令、只登记不改，是正确处置；错的是我的清单。**

`:1336` 早就记过「注释清扫的输入必须是目录，不能是任何人的文件清单」，本文也早有「Tester 独立扫 `packages/` + `src-tauri/` + `docs/` ⇒ 0 命中」的先例 —— 但**四份视角、三种漏法**：我点名的清单漏（round 1、round 2、round 3 各一次）、Coder 的目录 grep 抓到、Tester 逐句读又漏。
**固化（这条替换 `:1336` 的旧表述）**：审计单位不是「我怀疑的那一句话」，而是**整个模块头**。验收标准 = **把三份头逐行读完、枚举每一条事实性断言（含你觉得「当然成立因而没写出来」的隐含断言）、每条给出当场读到的 `文件:行号` 锚点**，**不是**「我说的那几句改对了」。round 4 据此派单。

**round 3 交付 `c96252426`（协调者已独立验完）**：非注释改动 **0 行**（我复跑 `git diff -U0 | grep -vE '^(\+\+\+|---)' | grep -vE '^[+-][[:space:]]*(//|$)'` 得 0），改动仅 `ledger.rs` + `transport.rs`；剔除 `^\s*//!` 后父子**逐字节相同**（`431dfaa4…4e57` / `c094055e…8484d2` 两侧一致）；门禁 `--lib` **405**、**21 目标 / TOTAL 592 / 0 failed / 0 警告**，三数全不变；冻结面 diff **为空**。Coder 对 `transport.rs:39`「释放路径只读一个计数」的判断我**认** —— 那是**候选修法**的描述、且限定了「一个计数」，不假；但我授权它换词，让目录级 `只读|唯一输入|唯一来源` 扫描能回到 0，否则这条不变式就不可检查了。

### M-2 洞的归属：立编号跟进轨 `CM-32-FU1`

Tester 提出的合并前最后一项行动：M-2 只登记在 `progress.md:287` 与 `transport.rs:33-37`，而 **`progress.md` 合并时必删** ⇒ 轨消失后唯一记录是一条**无主**的代码注释。
**裁定：立编号跟进轨 `CM-32-FU1`（负责人＝协调者，排期在 CM-32 合并之后），本轮不关闭。**
- **不写进 `docs/`** —— 那正是 Tester 驳回我「该加」那条问题时用的同一个理由：会把可修的洞写成永久架构事实。
- `transport.rs:33-37` 是**代码**且完整陈述了洞与实证 ⇒ AGENTS.md「台账消失后结论必须仍然能从代码和测试读出来」**已满足**。缺的只是**排期责任人**，由 FU1 提供。
- FU1 内容（Tester 已列两个候选，本轮不裁决）：为「释放路径只经由唯一计数」补断言 / 让 `drain` 按值从单个私有方法取计数而非直读结构体字段。
