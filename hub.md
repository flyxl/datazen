# hub.md — 数据浏览 PRD 落地：跨轨协调台账

> 集成分支：`feature/data-browsing-prd`（worktree `.worktrees/datazen-data-browsing-prd`）
> 基线：`main` = `3bb26cf8f`（已并入，merge 零冲突）
> 创建：2026-10-21。**交付时必须删除本文件**（AGENTS.md「进度台账」纪律）。

## 波次规划（按文件冲突面 + 硬依赖）

| Wave | 轨道 | 分册 | 触碰面 | 依赖 |
| --- | --- | --- | --- | --- |
| 1 | `grid-selection` | 01（Step 1–3 + Step 10 抽取） | `src/stores/tableData/*`、`useGridSelection`、`TablePendingChangesBar.tsx`、`TableView.tsx`、`tableDataStore.ts` | — |
| 1 | `insert-backend` | 04 的 **Rust 侧**（驱动契约 + 宿主计划/执行 + 测试） | `packages/driver-api/*`、`src-tauri/src/commands/{data.rs,data_session.rs,data/insert_plan.rs}` | — |
| 2 | `grid-cell` | 01 的 GridCell / VirtualBody / ARIA 段 | `src/components/DataTable/*` | Wave 1 `grid-selection` |
| 2 | `dirty-cells` | 06 | `CellRenderer.tsx`、`VirtualBody.tsx`、`tableDataStore.ts` | Wave 1 `grid-selection` |
| 2 | `insert-frontend` | 04 的前端草稿行 | `tableDataStore.ts`、`src/stores/tableData/draftInserts.ts` | Wave 1 两条 |
| 3 | `keyboard` / `clipboard` | 02 / 03 | 键盘与剪贴板 | Wave 2 |
| 4 | `editors` / `filters` / `count` | 05 / 08 / 09 | 编辑器族 / 筛选 / 行数三态 | Wave 3（08 的 `query_executor` 目录化是 09 的硬前置） |
| 5 | `result-grid` / `fk-nav` / `keyset` | 10 / 07 / 11 | 结果网格 / 外键 / 键集 | Wave 4 |

## 状态

| 轨道 | 波次 | 状态 | 提交 | 门禁实测 | 备注 |
| --- | --- | --- | --- | --- | --- |
| `grid-selection` | 1 | **第 3 轮修复中** | `afb21edff` + 本轮新增 | 第 2 轮 TEST_FAILED（2 新问题，4 缺陷全闭环）；6533 例中 1 红 = GS2-D2 | 缺陷循环第 3 轮 |
| `insert-backend` | 1 | **测试中**（Tester `e104b765`） | `93032e47c` / `2562908fb` / `2d0bf5664` | driver-api 377 passed；host 1395 passed / 9 ignored；data.rs 757 行 | 编码代理自报；待 Tester 独立复测 |

## 协调者裁定记录

| # | 事项 | 裁定 | 落地动作 |
| --- | --- | --- | --- |
| A1 | 分册 01 实装需要 `grid.selection.*` 三个码，但 00 契约 §8.2 的闭集里没有该域（§8.1 却把域指派给 01） | **判契约有缺口，不改实现**：按 §8.1 的登记门禁由协调者补登记 | `941761463` 把 §8.2 从 42 码扩到 45 码（43–45 归属 01），同步域映射表与 12-testing 的三处「42」 |
| A2 | `rowsCoveredBySelection(state)` 单参无法表达 row 模式 | 接受实现的 `GridSelection & { selectedRows?: ReadonlySet<number> }` 退化形（参数个数未变） | 待 Wave 5 前回写契约 §4.2.1 签名说明 |
| A3 | `normalizeCellRange` 取双参 + 未知列返回 `null`（§3.3 与 Step 1/§9.1 冲突） | 按 Step 1/§9.1 的显式要求（返回 `null`）为准，§3.3 为笔误 | 待回写 §3.3 |
| A4 | `gridEventGuards.ts` 未单独建文件（守卫私有在 hook 内） | 本波接受；**Wave 2 的 `grid-cell` 必须外提**，否则 DataTable 复用不了 | Wave 2 简报必列 |
| A5 | 未接 `DataTable`/`VirtualBody`（无高亮、无 `aria-activedescendant`、无 `role="rowheader"`） | 符合本波范围（Wave 2 `grid-cell` 负责） | Wave 2 简报必列 |
| A6 | `TableView` 落到 640 行（优于 A5 的 ≤710），因为连事务生命周期一起搬 | 接受，且这正是 01 点名的抽法 | 回写 01 §1.1 A5 与 00 §9.2 账本 |


## 已知跨轨约束（派发时必须重申）

1. **`TableView.tsx` 基线 803 行已越 800 红线** —— `grid-selection` 的 Step 10 抽取是后续所有前端轨的前置。
2. **`query_executor` 目录化归属冻结给 08**（`packages/feature-core/src/services/query_executor/`），其他轨不得动。
3. **`00-contracts.md` 是唯一契约源**：坐标系、`CellWrite` 三态、驱动新增方法签名、42 个错误码、行数账本都在那里。
4. 同一 worktree 不得同时被提交方与验证方使用（AGENTS.md）。

## 待裁定（等 Testers 结论后统一处理，勿提前改文档）

`insert-backend` 报的 6 处「与分册不符」，需在 Tester 判定后逐条定性（可接受 = 范围外 / 会返工 = 假完成），并把**定性结论回写**到 `04-insert.md` 与 `00-contracts.md`：

| 事项 | 分册要求 | 实际实现 | 倾向 |
| --- | --- | --- | --- |
| a | `CellWrite` 三态 + `build_insert_sql(&[(&str, CellWrite)]) -> String` | `build_insert_statement(table, &[(&str, Option<Value>)]) -> Result<BoundSqlStatement>`，与 update/delete 同一绑定参数 seam | **可接受**：主干已统一为绑定参数，回退到拼字符串会破坏参数化写路径 |
| b | `PendingRowInsert` + `canonicalize_inserts` + preview 第 4 参数 | 复用 `PendingRowChange.insert_marked`（`false` 不序列化 → 无插入的计划指纹逐字节不变），IPC 不变 | **可接受**：换来向后兼容，代价是 DTO 语义承载两个概念 |
| c | V-1..V-8 列元数据校验（draft 唯一/未知列/NOT NULL 无默认/schema 不可用/自增 warning） | 未做（本链路无列元数据入口） | **判会返工**：`grid.insert.unknownColumn` 等 4 个码因此无生产者，需后续轨补 |
| d | Step 6 RETURNING 回填 + 降级刷新 | 只有契约默认 `None` + 转发，宿主未调用 | 可接受（本轨未要求），Wave 2 前端须按「无回填」降级 |
| e | INSERT `affected == 0` 记 warning | 从严：`affected == 1`，否则失败回滚 | 需裁定：严格更好，但会让「0 行插入的幂等场景」变失败 |
| f | W-3 行身份键集 == `CachedColumns.primary_keys` 的漏列检查 | 未做（只做空/NULL 守卫） | 属 Step 3，记入 Wave 2 |

另需在合流前**回写**：`src/lib/tableChanges.ts` 的 TS 镜像（`inserts` / `insertMarked` / `operation: 'insert'`）——本轨按任务未改，Wave 2 `insert-frontend` 第一件事。

## grid-selection 缺陷循环记录

**第 1 轮（Tester `7037c93a`）→ TEST_FAILED，4 缺陷**

| ID | 级别 | 缺陷 | 裁定 |
| --- | --- | --- | --- |
| GS-D1 | P1 | 拖拽中 Escape 不结束会话，`pointerUp` 复活已取消选区（Escape 分支未清 `sessionRef`） | 立即修 |
| GS-D2 | P2 | IME 组合期 Escape 清空选区；`isComposingEvent` 不存在 | 立即修，并把 `gridEventGuards.ts` 由 Wave 2 **提前**到本轨外提 |
| GS-D3 | P2 | 裸 `reloadPanel` 失败不清三件套（§7 #19） | **同意扩大范围**：失败窗口是同一缺陷的第四个入口，契约已同步为「四处收敛点」 |
| GS-D4 | P1 | row 模式不可达：`applyRowModeTransition` 生产零调用者，行号槽走 `selectRow` 只写 `selectedRows` | 本轨必修（01 Step 4 的交付内容） |

**第 1 轮还改了 3 处契约**（协调者直接改，不等实现让步）：

1. §8.2 补登记 `grid.selection.*` 三码（42 → 45）；
2. §4.2.1 `rowsCoveredBySelection` 的 `GridSelectionState.selectedRows` 由可选改**必填**（可选会让 row 模式静默返回空集）；
3. §4 新增第四条不变式：**`mode` 是行集唯一权威，写入必须成对**（`none` → 三件一起清；`cell`/`row` → 清空 `selectedRows`，行集只能由 `applyRowModeTransition` / `setRowModeSelection` 与 mode 成对写入；「只改 mode 保留行集」非法）。

**第 2 轮**：原 Tester `7037c93a` 复测中（重点：考题是否被改、4 修复能否被独立新用例重现、并发失败不得误清新选择、`applyGridSelection` 新语义是否提前破坏 03/04/05 的调用面）。

## 待 Wave 2 承接的债（本轨明确不做）

| 债 | 承接轨 |
| --- | --- |
| `TableView.test.tsx` 的冻结 store mock 无 `setRowModeSelection` | Wave 2 加行号槽用例前必须先补 mock（本轨无权改该文件） |
| `[data-dt-surface]` / `[data-dt-selected]` / `role="rowheader"` / 选区摘要 testid | Wave 2 `grid-cell`（E2E `table-cell-selection.ts` 的 `surfaceWired` 守卫会在接线后自动激活） |
| `gridEventGuards.ts` 已有，但 DataTable 尚未消费 | Wave 2 `grid-cell` |

## 协作约定（用户 2026-10-21 定）

**同一轨的复测一律唤醒原 Tester，不新起实例。** 原 Tester 手上已有上一轮的缺陷清单、自己写的考题与对实现的怀疑点，复测的针对性远高于新实例；新实例只剩「重读整个轨」的开销。

- 例外：原 Tester 实例不可恢复（会话丢失）时才新起。
- 协调者已按此取消过一次新实例（`7ebdb8e9` 第 2 轮复测），并删除它留下的 4 个未完成测试文件。
- 编码代理侧同理沿用「resume 原 Coder」，两条规则合起来即：**一条轨只有一对 coder/tester，贯穿它的全部修复循环。**

## grid-selection 第 2 轮结论（原 Tester 唤醒，考题 60 例未被改）

**4 个缺陷全部闭环**（D1/D2/D3/D4 用全新用例逐条转绿，`git diff 35cde046e..HEAD -- <7 个测试文件>` = 0 行）。判失败只剩 2 条**由新契约引出**的问题：

| ID | 级别 | 问题 | 裁定 |
| --- | --- | --- | --- |
| GS2-D1 | P3 | §4.2.1 要求 `selectedRows` **必填**且类型名 `GridSelectionState`，实装仍是 `GridSelectionWithRows` + `selectedRows?` + `?? EMPTY_ROW_SET` 兜底 → 按冻结签名传裸 `GridSelection` 时 typecheck 全绿、row 模式静默返回空集 | 必改（字段必填 + 改名 + 去兜底 + 让 tsc 指路改调用点） |
| GS2-D2 | P2 | `deleteRows` 直接写 `selectedRows`，与 cell 选区并存 → 违反「mode 是行集唯一权威」 | 必改（删除不改变选择语义，行集由 `rowIdentityAnchors` 承担） |

**Tester 提出的一条待裁定，我已拍板并写进契约**：`mode:'row'` + **空**行集是**合法**状态（反选到 0 行、再点已选行取消都会产生）；契约禁止的是「`none`/`cell` + **非空**行集」。不要为「空行集」加退回 `none` 的逻辑——那会让「取消最后一行后 Shift 连选」失去锚点。契约提交 `ab8d0ffd7`。

**Tester 的另一条方法论值得记**：它为 GS2-D1 故意**不写** `@ts-expect-error` 红测，理由是那会让 `pnpm typecheck` 变红成 TS2578，与真实构建错误不可区分。改为「源码行 + 现行编译通过」作证据，并要求编码代理自证「裸调用必报 TS 错误」后还原。

## 契约与实现反复校正的顺序（教训）

本轨已出现 3 次「Tester 报偏差 → 复盘发现是契约错 → 改契约 → 再让实现对齐」：错误码闭集缺口、`rowsCoveredBySelection` 签名、row 模式空行集。**根因是分册写契约时没跑过真实调用面**。Wave 2 起，凡涉及跨册签名冻结，先在本轨写一个最小调用点再冻。

## 暂停令（用户 2026-10-21）

**当前在跑的子代理跑完后，暂停一切后续调度**，先等契约审计代理 `efc3921d` 出结论，再决定下一步。

| 在跑 | 角色 | 跑完后做什么 |
| --- | --- | --- |
| `94a74ec3` | `grid-selection` Coder（修 GS2-D1/D2） | ✅ 已交付 `8e4075059`，**复测按暂停令挂起** |
| `36973778` | `insert-backend` Coder（修 IB-D1..D5） | **只收结果，不派复测** |
| `efc3921d` | 契约审计（T1 全册冻结物 / T2 跨册引用 / T3 数字） | 出了结论再统一决定 |

**暂停的原因**：本轨已 4 次因「契约写错」返工，若继续「修完就复测」的循环，等于在错误的契约上反复验证。审计结论可能会**改动验收标准本身**，此时推进复测就是白跑。审计完成后按结论重排：先改契约 → 再让两个 Coder 对齐 → 最后一次性复测。

### `grid-selection` 第 3 轮交付（复测挂起，未验证）

| commit | 内容 |
| --- | --- |
| `3aa80b32c` | 固化第 2 轮 Tester 的 23 例（3 文件 / 571 行，零改动） |
| `5e1ce0419` | GS2-D2：`deleteRows` 不再写行集（712 → 713 行），删除定位仍归 `rowIdentityAnchors` |
| `8e4075059` | GS2-D1：`GridSelectionState.selectedRows` 改**必填**、去 `?? EMPTY_ROW_SET` 兜底、类型改名；5 处调用点由 tsc 指路修完 |

自报：627 文件 / 6575 例通过、typecheck EXIT=0、`TableView 642`、`tableDataStore 713`。
自证：裸 `GridSelection` 调用 → `TS2345`（探针文件已删、未进提交）。
对 Tester 测试的改动已披露 diff：只给参数补 `selectedRows` 字段，断言与期望值零改动（符合我的授权口径）。

**未验证**——按暂停令等契约审计结论，审计可能改动验收标准本身。

遗留（待我决定）：`src/stores/__tests__/tableDataStore.test.ts:533` 的用例名「deleteRows selects indices then stages deletes」已过时（断言只查 `pendingChanges`，仍绿）；属冻结历史测试，改名与否等合流时一并处理。

## 架构规则（用户 2026-10-21 定，优先级高于既有分册设计）

> 新增功能都要同时支持 desktop 与 web。两端共享的逻辑必须放进共享 crate（`packages/feature-core` 等）；`src-tauri` 只保留 **command 适配层**——类似 web 的 controller：只做必要校验，然后把请求转发到 domain service（desktop 与 web 共用层）。

已核实的 crate 依赖事实（`Cargo.toml`）：

| crate | 依赖 | 承载能力 |
| --- | --- | --- |
| `packages/feature-core` | `datazen-runtime`(:13) / `datazen-driver-api`(:14) / `datazen-platform-api`(:16) / `datazen-application`(:12) | **能**承载「计划 + 守卫 + 指纹 + 经 runtime 执行」→ 数据浏览 Rust 侧的落点 |
| `packages/application` | 仅 `datazen-platform-api`(:14) | 不依赖 runtime，放不了执行 |
| `packages/runtime` | — | 只放 session/资源/预算原语 |
| `packages/backend-client` | TS 侧传输无关契约 | web/desktop 共用的 IPC port |

**对 Wave 1 的直接影响**：`insert-backend` 把计划构建/指纹/守卫/执行全放进了 `src-tauri/src/commands/**`，违反新规则。已给该轨追加「搬迁提交」要求：逻辑搬进 `packages/feature-core/src/data_browsing/`（`change_set.rs` / `write_exec.rs`），`src-tauri` 降为「取值 → 必要校验 → 调 feature-core → 映射 CommandError」，且 **IPC 形状、serde 形状、指纹结果、错误文案一个字都不能变**。

**对后续波次的约束**：分册 `08/07/10/09/04/11` 都有把逻辑放 `src-tauri` 的描写（引用密度 16/13/10/10/10/6），必须逐册按新规则重定落点。已把「T0 分层落点审计」作为最高优先级插进契约审计代理（`efc3921d`）。

**Wave 2 起的分册写作规范**（待审计确认后固化）：每个新增 Rust 逻辑都必须在分册里写清「共享层落点 + src-tauri 适配层的职责边界」，写不出共享层落点的，一律视为设计未完成。

## 契约审计结论（代理 `efc3921d`，13 份 / 13,122 行对照代码）

**结构健康**，但「现状事实」与「实施规则」是两套不同步文档。最危险的不是某条写错，而是 **§1.x 的现状事实过期后仍被下游当实现依据**——3 条 P1 全源于此。

| 类别 | 数量 | 代表 |
| --- | --- | --- |
| T0 分层落点（新规则） | 12 行 | 6 处需整体搬家：04(≈680+290)、07(≈180)、08(≈440)、09(≈120)、10(≈380)、11(≈850) |
| T1 00 册冻结物 | 15 条 | P0：`FilterCondition.value` 写成 `Option<Value>` 还自称 R1 合规（实装是 `Value`，约 25 处构造 / 30 处读取） |
| T2 分册跨册引用 | 11 条 | P0：01:236 仍用旧 `rowsCoveredBySelection(GridSelection)` 签名 |
| T3 数字 | 12 条 | 6 处过期：`reuse.rs 1159→1344`、`query_executor 811→47`、TableView 720→642 |
| 契约自相矛盾 | 6 条 | C-1 写缝、C-2 §9.1↔§9.2 路径、C-3 legacy 函数、C-4 value 形状、C-5 INSERT 豁免、C-6 #7 触发条件 |

**关键事实**：§8.2 的 45 个错误码里，**只有 1 个有真实生产者**（`grid.commit.conflictingIntents`，且触发条件与契约不符）；`grid.selection.*` 三码零生产者（越界一律 `clampCellCoord → null`）。

## 协调者裁定（已写进契约或已下发）

| 编号 | 裁定 |
| --- | --- |
| T0-11 | **补 port，不裁 desktop-only**；G-5 要求每条新 IPC 在 `MethodMap` + `BackendClient` 登记，分册 §6 列四元组 |
| C-5 / T1-13 | **INSERT 不豁免** `affected == 1`；0 行走「幂等成功」独立分支讨论 |
| C-3 / T2-7 | `validate_legacy_pk_columns` 等**不存在**，整节迁移要求作废；空/NULL 守卫**新建**（Wave 1 已做），W-3 列集相等**仍是缺口** |
| T1-3 / C-4 | `FilterCondition.value` 保持 `Value`（改 Option 破 R1） |
| C-6 / T1-12 | §8.2 #7 触发条件补「或新增行同时被标记删除」；「删除+列写入」校验**尚未实现** |
| T2-1 | 01 签名改 `GridSelectionState` |
| T3 | 行数一律引用 §9.2 账本，禁止手写估值 |

**已落地**：`ae06bdb44` 改完 `00-contracts`（T0-1 + G-4 + G-5 + 16 条 T1 + 3 条 T3 + 归属补登记）。分册 01–12 由代理 `3e3ae460` 按审计清单修订中。

## 审计给出的方法论（待固化进 AGENTS.md）

1. 「现状事实」必须带**可执行探针**（`rg -n 'fn xxx' path` + 实测输出），无输出就写「不存在（待新增）」。
2. 冻结签名只允许「一处定义 + 引用」，分册禁止复制代码块，只写 `见 §5.1`；落地册须在同一提交里对齐。
3. 每条冻结契约登记**生产者 + 消费方**；§8.2 加「生产者」列，空着就是未实现。
4. 行数只允许脚本产出，文档引用账本。
5. **新 Rust 逻辑三问门禁**（G-4）：desktop/web 都要？能否端口注入而非吃 `AppState`？宿主只剩「取值→校验→转发→映射错误」？有一问答不上就落 feature-core。

## `insert-backend` 第 2 轮交付（含架构搬迁，复测中）

| commit | 内容 |
| --- | --- |
| `a970584a9` | 固化 Tester 20 例（未改动） |
| `7841b589c` | **分层搬迁**：`packages/feature-core/src/data_browsing/{change_set,insert_plan,row_identity,write_exec,mod,tests}.rs`；`src-tauri/commands/data.rs` **761→263**、`data_session.rs` **删除（333→0）**、新增宿主端口 `data/table_columns.rs`(132) |
| `497e9de11` | IB-D3：MySQL 空列 `INSERT INTO t () VALUES ()`；新增 `packages/drivers/mysql/tests/insert_defaults.rs` |
| `9bc3f613a` | TS 镜像 `inserts?` / `insertMarked?` / `operation:'insert'` |

自报：FC 321 / HOST 1406(9 ignored) / API 382 / MYSQL 188 / SQLITE 5 / MYSQL_DEFAULTS 2，`FMT_EXIT=0`、`TSC_EXIT=0`。
自证：反向依赖 grep 0 命中；宿主内已无 `build_row_change_plan`/`execute_plan`/`changes_fingerprint` 定义；DTO 块与搬迁前**逐字节相同**、13 条 serde 属性未改、36 个基线用例 assert 行零变化、指纹逐字节用例仍在。

**待复测验证的关键风险**：`data_session.rs` 整文件删除——事务归属（`manual_handle` 校验 / `defersCommit`）、错误文案是否在 `write_exec.rs` 里**等价**存在；测试搬家有没有丢断言（`data/tests.rs` 1203→396，迁出的必须逐条在 feature-core 重现）。

**V-3 已知缺口**：`ColumnSchema` 无 generated/identity 标记，无法判定"不可写列"，`grid.insert.noWritableColumn` 本轮不做（不臆造规则）。

## `insert-backend` 第 2 轮复测：**TEST_PASSED**（`9bc3f613a`）

搬迁被独立验证为**真搬**，不是壳：

| 检查 | 结果 |
| --- | --- |
| 核心定义仍在 `src-tauri`？ | `grep fn build_row_change_plan\|fn execute_plan\|fn changes_fingerprint\|fn canonicalize_insert` → **0 命中** |
| feature-core 反向依赖宿主？ | `grep crate::commands\|use datazen::` → **0 命中**；`Cargo.toml` 无宿主 crate |
| `data_session.rs` | 树内 0、源码引用 0、`mod` 已删；能力对照仅 **29 行机械差异**（签名 + 错误类型 + 3 行 adapter 上移） |
| 行为零变化 | 指纹算法逐字未改；serde 14 条**多重集相等**；文案"旧有新无"仅 1 条 `#[path]` 常量；两个 command **逐字节相同** |
| 测试搬家 | 46 个 fn 逐条比对，`not found=[]`、断言差异 **0**（3 行 import 之外零新增） |
| IB-D5 变异自证 | Tester **独立重现**：删转发 → EXIT=101 `Unsupported(...)`；还原后 sha256 一致、diff 空 |

**关键判断**：Tester 明确说明"只有 2 条真落 feature-core，另 6 条去 `insert_tests.rs`"**不是缩水**——那 6 条构造 `TestAppState` 调 `preview_pending_changes_impl`，搬进 feature-core 就必然反向依赖宿主，正好违反本轮分层规则。它用更强的判据（46 个 fn 全落点断言逐条相同）覆盖了这个偏差。这个判断我认可。

**Tester 纠正了编码代理的一处自述**：V-2「未做」不成立——存在性检查已实装并带 `grid.insert.unknownColumn`；准确说法是「存在性已做、**可写性**未做」。V-3 理由成立但不完整（`ColumnSchema` 确无 generated 标记，但 `default_value`/`is_auto_increment` 可做保守版）。

## 第 3 轮修复（已下发 `36973778`）

| ID | 级别 | 内容 |
| --- | --- | --- |
| IB2-D4 | P2 | 错误码嵌入不一致：`affectedMismatch`/`stalePlan` **没有** `grid.*` 前缀（下游无法按码分支）；`unknownColumn` 对 UPDATE/DELETE 也发（契约限定草稿行） |
| IB2-D7 | P2 | 契约第 7 条前半未实装：`delete_marked` + 列写入现状只发 warning 并**静默丢弃列写入**，要求改为拒绝 |
| IB2-D2 | P3 | `let _ = facts.database_type.as_str();` 死代码 → 改为真正校验（多宿主时唯一防线） |
| IB2-D3 | P3 | 注释与实现不符（"before the schema read"）→ 改注释，不挪顺序（挪动会改变"schema 不可读时报哪个错"的既有行为） |

## 裁定：IB2-D1（web 通路）→ 单独立 Wave 级任务

`packages/backend-client` 的 `MethodMap` 里**一条表数据方法都没有**（`previewPendingChanges`/`commitPendingChanges` 基线即缺），也没有 `insertMarked`/`inserts` 类型镜像。这是**基线缺口，不是本轨回归**，但按用户的架构规则（desktop 与 web 必须同时支持）它必须在功能上线前补齐。

**新增 Wave 级任务 `web-transport`**（不挂在任何功能轨上）：
1. `packages/backend-client/src/transport.ts` 的 `MethodMap` 补表数据方法（`getTableData` / `previewPendingChanges` / `commitPendingChanges` / `estimateTableRows` / `resolveForeignKeyTargets` / 命名视图三命令 / `commitQueryResultCell`）；
2. `BackendClient` 接口补同名方法；
3. `packages/backend-client/src/types.ts` 补 `insertMarked` / `inserts` / `operation:'insert'` 等 wire 类型镜像；
4. 前端 `src/commands/*` 只经 BackendClient，禁止散落 `invoke`。

这条应该在 Wave 2 里与 `grid-cell` 并行（它只改传输层，不碰三个热文件）。

## 协作事故与规则收紧（2026-10-21）

我在 `1211a47bf` 用 `git add -A docs/prd` 提交时，分册修订代理 `3e3ae460` **正在同一棵工作树里写文件**，于是把它进行中的编辑一起扫进了 commit。代理明确指出这违反 AGENTS.md「同一 worktree 不得同时被提交方与验证方使用」。实测后果是好的（内容自洽，脏区只剩它之后写的 3 处收尾），但这是**运气**，不是流程。

**收紧后的规则**：
1. **有代理在跑的分支，我不提交**。只在该代理回报 `DONE`/`READY_FOR_*` 且工作区状态明确之后，才 `git add`。
2. 若必须并发，用 `git add <具体文件列表>` 而**不是** `-A`，把提交面钉死在自己改的文件上。
3. 文档类代理默认给它**独立 worktree**（用户此前已提过一次同类风险）。
4. 提交前后各记一次 `git status --short` 与 HEAD，与门禁前后记录同规矩。

## 两条轨同时进入终局复测（2026-10-21）

| 轨 | HEAD | 复测代理 | 本轮修了什么 |
| --- | --- | --- | --- |
| `insert-backend` | `a0d903519` | `e104b765`（原 Tester，第 3 轮） | IB2-D4 错误码补 `grid.*` 前缀并收敛域；IB2-D7 删除+列写入改为**拒绝**（原先只发 warning 且静默丢弃列写入）；IB2-D2 `driver_type` 防线；IB2-D3 注释改事实 |
| `grid-selection` | `8e4075059` | `7037c93a`（原 Tester，第 3 轮） | GS2-D1 `selectedRows` 必填 + 类型改名 + 去兜底；GS2-D2 `deleteRows` 不再写行集 |

**给两个 Tester 的关键考点**：
- `insert-backend`：四条错误路径是否**都**带前缀且前缀在**最开头**、描述文案是否保留、`unknownColumn` 是否只在插入域；并要它给出**「零生产者错误码清单」**（45 码里还有哪些没有任何 throw 点）——这是本批最后一次能补齐的机会。
- `grid-selection`：编码代理**承认改过 Tester 的 2 个测试文件**（纯类型适配），要求逐行核「只补参数、断言零变化」；并要它**独立重建探针**证明裸 `GridSelection` 调用必报 TS2345。

## Wave 2 待派（终局复测通过、合流之后再动）

三轨并行，但要遵守「三个热文件每波只允许一条前端轨」：

| 轨 | 内容 | 与热文件的关系 |
| --- | --- | --- |
| `grid-cell` | `[data-dt-surface]` / `data-dt-selected` / `role="rowheader"` / 选区摘要 testid 接线 | **碰 `DataTable.tsx`**（热文件）→ 本波唯一前端轨 |
| `web-transport` | `MethodMap` + `BackendClient` + wire 类型镜像（`insertMarked`/`inserts`） | 只碰 `packages/backend-client`，可与 `grid-cell` 并行 |
| `insert-frontend` | 草稿行 UI + TS 镜像接线（依赖 04 后端已就绪） | 碰 `TableView.tsx` → **与 `grid-cell` 冲突，排到 Wave 3** |

## `grid-selection` 第 3 轮复测：**TEST_PASSED**（`8e4075059`）

| 验证项 | 结果 |
| --- | --- |
| GS2-D2 红测转绿 | 是；另加 6 例不变式（cell+删多行 / 空列表 no-op / row 模式删选中行 / row+空合法不退回 none / 四写点矩阵） |
| `deleteRows` 行集写入 | 全仓归零；生产写点全部成对（`connectionState` 初始 + `selectionActions` 六处 + `reset`） |
| GS2-D1 类型层 | 改名 + 必填 + 去兜底均确认；**独立探针**重现 TS2345；反向构造（`as`/`Partial`/`Omit`/`Pick`/`satisfies`/`Object.assign`）**NONE** |
| 永久类型钉 | 新增 `gridSelection.types.test.ts`（`@ts-expect-error`），并用机制探针证明「字段改回可选 → tsc 报 TS2578」⇒ 回归无法悄悄复活 |
| 考题完整性 | 代理只补了 5 个调用点的必填字段，断言/期望值/用例名**一字未动** |
| 门禁 | 629 文件 / 6583 例 EXIT=0；typecheck EXIT=0；`TableView 642`、`tableDataStore 713` |

**Tester 提出的 3 条硬化建议，我的处理**：
1. `setGridSelection({mode:'row', anchor})` 能造出「row + 非 null 几何」→ **要求实现归一化**（契约 I2 已明文要求几何为 null，公开 API 上的洞要堵），已下发第 4 轮；
2. `00` §4 推论末句「row+空是不一致态」与新增例外段自相矛盾 → **已改**（`7652b3d88`）；
3. `01` 的 I6 只列 `selectRow`/`toggleSelectAll`，漏了 `setRowModeSelection` → **已补**（同上）。

## 非本轨观察：脚本测试并发串扰（记着，不在本批修）

Tester 全量首跑遇到 `scripts/__tests__/driver-init-rustfmt.test.mjs` 失败：`ENOENT … packages/ui/src/__boundaryProbe__.ts`——该 probe 文件由 `scripts/__tests__/boundaryMutation.ts` **并发植入/删除**，属脚本测试之间的竞态。单跑该文件 8 passed、全量复跑全绿。

**这是既有缺陷，与本批任何轨无关**，但它会周期性把别人的门禁弄红（本批已踩到一次）。登记：`docs/bugs/` 候选——「脚本测试用共享 probe 路径导致并发竞态」。合流后的健全性校验如果再次踩到，就按 `docs/bugs/` 单文件规则登记。

## 两条轨的第 4 轮（收尾）已下发

| 轨 | HEAD | 本轮内容 | 复测代理 |
| --- | --- | --- | --- |
| `insert-backend` | `df51994b4` | 10 处既有拒绝点贴契约码（stalePlan ×5 / affectedMismatch ×1 / cell 三码 ×10）。自报「只加前缀，判定零改动」 | `e104b765` |
| `grid-selection` | `3c7edb6bf` | `applyGridSelection` 按 mode 归一化几何（堵住「row + 非 null 几何」的公开 API 洞） | `7037c93a` |

**协调者自己的改动**：`3c7edb6bf` 只把 `tableDataStore.test.ts:533` 的用例名从 `deleteRows selects indices then stages deletes` 改成 `...from explicit indices without touching the row set`——行集写入移除后旧名字已失真，而「名字说谎的测试」比没测试更坏。断言零改动，单文件 40 例 EXIT=0。

**我要求两个 Tester 本轮各盯一个易漏点**：
- `insert-backend`：「只改字符串」这条自述要从 diff 层与用例层双向证；且要给出**零生产者终盘清单**（码 → 归属 → 状态），写进合流报告。
- `grid-selection`：`applyGridSelection` 现在返回**共享常量** `ROW_MODE_GRID_SELECTION`，要查它会不会被 mutate 污染全局；这比「跑绿」重要——归一化很容易顺手把 `cell` 合法几何也清掉。

## 合流预检（两轨都还在最后一轮，先做静态检查）

两轨各自相对分叉点的改动文件**交集为空**：

```
comm -12 <(diff --name-only base..grid-selection) <(diff --name-only base..insert-backend)
→ (空)
```

`grid-selection` 13 个提交全在 `src/`（stores/hooks/lib/TableView/components 测试），`insert-backend` 11 个提交全在 `packages/` + `src-tauri/` + `packages/feature-core`。**合流不应产生内容冲突**；唯一的共享风险是 lockfile / 生成文件，本轮两轨都没碰。

两条轨都**落后于集成分支**（集成分支多出的是 docs + hub 提交），所以合流顺序：
1. `feature/data-browsing-prd` ← `feature/grid-selection`
2. `feature/data-browsing-prd` ← `feature/insert-backend`
3. 健全性校验（全量门禁：前端 `npx vitest run` + `pnpm typecheck`；Rust `--lib` 四包 + 各 `--test`；`cargo fmt --check`）
4. 删两个 feature 分支与 worktree，删两轨 `progress.md`
5. 派 Wave 2

## Wave 2 派发单（合流 + 健全性校验通过后立即执行）

三条轨并行，但要守住「三个热文件（`TableView.tsx` / `DataTable.tsx` / `tableDataStore.ts`）每波只允许一条前端轨」：

| 轨 | 分支 | 内容 | 热文件 |
| --- | --- | --- | --- |
| `grid-cell` | `feature/grid-cell` | `[data-dt-surface]` / `data-dt-selected` / `role="rowheader"` / 选区摘要 testid 接线；消费 01 已落地的 `rowsCoveredBySelection` + `gridErrors` | **碰 `DataTable.tsx`** → 本波唯一前端轨 |
| `web-transport` | `feature/web-transport` | `MethodMap` + `BackendClient` + wire 类型镜像（`insertMarked` / `inserts` / `operation:'insert'`）；前端只经 BackendClient | 只碰 `packages/backend-client`，可并行 |
| `insert-frontend` | `feature/insert-frontend` | 草稿行 UI + TS 镜像接线 | 碰 `TableView.tsx` → **与 `grid-cell` 冲突，排 Wave 3** |

**Wave 1 交给 Wave 2 的三条硬信息**：
1. `insert-backend` 已把 `inserts` / `insertMarked` / `operation:'insert'` 镜像进桌面 TS（`src/lib/tableChanges.ts`），但 **`packages/backend-client` 没有**——`web-transport` 要照抄形状，不要重新设计；
2. `grid-selection` 的 `deleteRows` **不再写行集**，`applyGridSelection` 已按 mode 归一化几何；`insert-frontend` 若要在删除后保留选中视觉，**必须走 `setRowModeSelection`**，不能自己拼 patch；
3. 45 码里现在有 8 个有生产者；`grid-cell` 若要消费某个码，先确认它在生产者清单里，否则前端分支永不触发。
