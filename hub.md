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

## 轨 p3-cancel-cleanup（D-03 / D-R2-1 / D-R2-2）—— 已交付，待独立验收

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
