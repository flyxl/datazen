# T05 测试报告：网格与详情接线（cell / aside / 草稿行）

- 被测对象：`packages/frontend/src/components/DataTable/CellRenderer.tsx`（新增 `presentationPlan` prop）+ `GridCell.tsx`（转发）+ `VirtualBody.tsx`（`planByColumn` 查表）+ `DraftRow.tsx`（已写入格子查表）+ `DataTable.tsx`（`presentationSnapshot` / `presentationSource` / `presentationRegistry` props + `useValuePresentation` + 模块级默认快照）+ `DetailPanel.tsx`（同款 hook + `FieldRow` 展示路径）+ `packages/frontend/src/windows/connection/TableView.tsx`（转发）+ `ContentViewDrawers.tsx`（转发 + source 派生）+ 自带测试 `CellRenderer.presentation.test.tsx`（7 项）、`DataTable.presentation.test.tsx`（7 项）、`DetailPanel.presentation.test.tsx`（9 项）
- 分支：`feature/issue-32-value-presentation`（HEAD `c5a7a035d`，作者 issue-32-agent；T05 两提交：`800dcdfc5` feat + `c5a7a035d` test，基底 `b621bcd45`；验证开始时工作区干净）
- 测试角色：tester 独立验证，未修改任何被测代码或测试
- 依据：`docs/archive/development/issue-32-value-presentation-design.md` §7 §10-A、T04 `presentationPlan.ts` 与 `useValuePresentation.ts`、T03 `timestampContribution.ts`
- 日期：2026-11-12

## 1. 测试环境与命令

环境：Node v24.22.1，vitest 4.1.10（仓库自带），tsc 7.0.2 / tsrs 0.18.0，系统时区 UTC（全部探针均显式传入时区与语言，不依赖机器默认值）。对抗性探针运行于独立临时项目 `/tmp/probe-t05/`（通过 `node_modules` 符号链接复用仓库依赖，`environment: jsdom`，被测源码以绝对路径导入，不污染仓库工作区；探针按任务要求用后删除——`/tmp` 在任务中途被环境清理，探针文件与原始日志随之清除，最终 `git status` 复核仓库除本报告外无任何改动；探针结果以运行输出为准，见 §2.2）。

环境说明：本机 `PATH` 中无 `pnpm` 二进制（与 T02/T03/T04 报告相同情况）；G3 改用 `npx -y pnpm@12.18.0 <script>` 调用，版本号实测 12.18.0，属等价调用，不影响门禁结论。

| 命令 | 用途 |
|------|------|
| `npx vitest run packages/frontend/src/components/DataTable packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx --retry=0` | G1：T05 相关包单测（排除根配置 retry 掩盖侥幸通过） |
| `npx vitest run packages/frontend/src/windows/connection --retry=0` | G2：connection 窗口全量（含 TableView / ContentViewDrawers / ResultTableView / ProcessListView） |
| `npx -y pnpm@12.18.0 typecheck` / `npx -y pnpm@12.18.0 typecheck:tsc` | G3：双引擎类型门禁（tsrs / 权威 tsc；root / scripts / pack-ep 三程序） |
| `node scripts/check-test-module-naming.mjs` | G4：测试命名门禁 |
| `npx vitest run <G1 路径> --retry=0 --coverage` | G5：六个 DataTable 文件覆盖率（v8 报告；TableView / ContentViewDrawers 不在 coverage include 清单内，见 §2.1 注） |
| `git show 800dcdfc5` 全 diff + grep 审读 | G6：改动范围、`any` / 跳过断言、编辑与 NULL 分支、QuickLookDialog、默认快照只读性 |
| `/tmp/probe-t05` 探针项目（17 项，P1–P10） | 对抗性行为验证（PRD 锚点 grid/aside 逐字一致、BUG-003 不回归、默认关闭、编辑原值与取消往返、草稿行编辑态、计划缺失/禁用/非法值回退、跨库 source 一致性、注册注销响应、样式与截断、其他调用点） |
| `npx vitest run …/ExplainPanel.test.tsx …/ResultTableView.test.tsx …/ProcessListView.test.tsx --retry=0` | P10 补充抽查（ExplainPanel 不在 G1/G2 路径内，单独跑） |

探针项目结构：`probes/t05-adversarial.test.tsx`（P1–P6、P7b、P8–P9，共 14 项）+ `probes/t05-drawers.test.tsx`（P7a-table、P7a-query、P7c，共 3 项）+ `vitest.config.ts`（root 指回工作区，复用仓库 setup 与 tauri mock）。探针属一次性验证资产，位于系统临时目录，不进入仓库；各项方法已在下表内描述到可复现程度。

## 2. 测试项清单

### 2.1 既有验证（仓库门禁）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| G1 | T05 相关包单测全量 | `npx vitest run packages/frontend/src/components/DataTable packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx --retry=0` | ✅ 59 文件 **748/748 通过**（含新增 CellRenderer.presentation 7、DataTable.presentation 7、DetailPanel.presentation 9，与申报数量一致；退出码 0；耗时 108s） |
| G2 | connection 窗口全量 | `npx vitest run packages/frontend/src/windows/connection --retry=0` | ✅ 113 文件 **1195/1195 通过**（与申报一致；退出码 0；耗时 249s） |
| G3 | 类型门禁（tsrs 单引擎 + 权威 tsc） | `pnpm typecheck` / `pnpm typecheck:tsc`（经 npx pnpm 等价调用） | ✅ 两者均 OK（root / scripts / pack-ep 三程序；tsrs 0.18.0-ts7.1.0-dev.20260929、tsc 7.0.2；退出码均为 0） |
| G4 | 测试命名门禁 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2564 TS 文件、8 wdio spec 树；退出码 0） |
| G5 | 六个 DataTable 文件覆盖率 | `npx vitest run <G1 路径> --retry=0 --coverage` | ✅ 六文件四项指标全部 ≥80%（见下表明细）。测试本身 748/748 通过；进程退出码 1，原因唯一是全局阈值（All files 16.16 / 18.04 / 13.57 / 17.19，未跑全量 include 清单所致，与 T04 报告的已知现象相同），不是测试失败、不是六文件未达标 |
| G6 | 改动范围与源码审读 | `git show 800dcdfc5` 全 diff（619 行）+ grep | ✅ 通过（见 §2.3） |

> G5 明细（v8 报告；`% Stmts / % Branch / % Funcs / % Lines`）：
>
> | 文件 | % Stmts | % Branch | % Funcs | % Lines | 未覆盖行（语句级，取自 coverage-final.json） |
> |------|---------|----------|---------|---------|----------|
> | `CellRenderer.tsx` | 100 | 97.61 | 100 | 100 | 无未覆盖语句；唯一未覆盖分支为 :93（`loadCandidates` 三元的既有分支，与 T05 无关） |
> | `DataTable.tsx` | 93.68 | 95.45 | 85.71 | 93.67 | 574、575、658、826、955、1049（均非 T05 新增行；T05 新增行 :609–626、:979 全覆盖） |
> | `DetailPanel.tsx` | 88.63 | 88.57 | 85.71 | 90.12 | 141、225–227、285、313–314、334–335、341（中键 QuickLook、QuickLookDialog 关闭、InlineFieldEditor 内部分支，均为既有行；T05 新增行 :221（`presented` 求值）与 :268–274（`<pre>` 三元）全覆盖） |
> | `DraftRow.tsx` | 100 | 85.18 | 87.5 | 100 | 无未覆盖语句；未覆盖分支 80–87、131–132（既有行） |
> | `GridCell.tsx` | 82.6 | 90.21 | **80** | 82.6 | 268–269、341、346（中键 QuickLook、FK 下拉按钮，均为既有行）。⚠️ Funcs 恰为 80.0，踩线通过，见 O-6 |
> | `VirtualBody.tsx` | 90.83 | 88.52 | 100 | 93.39 | 374–377、670–671、699、702、705（均非 T05 新增行；T05 新增行 :546、:758、:772 全覆盖） |
>
> 注：`TableView.tsx` 与 `ContentViewDrawers.tsx` 不在根 vitest 配置的 coverage include 清单内（清单只含 `windows/connection/` 下的 ConnectionPage / ObjectBrowser / PrivilegeView 三文件），故无单文件覆盖率数字；其转发行由 G2 的 1195 项全量覆盖。`coverage/` 输出目录由本次运行生成，已确认被 gitignore 覆盖，工作区无残留。

### 2.2 对抗性探针（/tmp/probe-t05，独立于 coder 自带测试编写）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| P1 | PRD 锚点经 DataTable + DetailPanel | 同一 snapshot/registry/source 下 `2026-01-15T14:30:00Z` + iso-tz/Asia/Shanghai；grid 与 aside 各取精确字符串比对 | ✅ 通过。grid = `2026-01-15T22:30:00+08:00`；aside 第 1 个 `<pre>` 与 grid 逐字相等（`toBe`，不只是包含） |
| P2 | BUG-003 不回归 | 同一 snapshot 下 `epoch_ms=1705316400123`（custom unix-ms 覆盖）与 `qty=42`，grid/aside 各取精确字符串 | ✅ 通过。grid 与 aside 的 epoch_ms 均为 `2024-01-15T19:00:00.123+08:00`（含 `.123`）；qty 两处均为 `42` |
| P3 | 默认关闭 | 无 snapshot 的 DataTable + DetailPanel | ✅ 通过。grid created_at = `2026-01-15T14:30:00.000Z`（旧 formatTimestamp）、epoch_ms = `1705316400123`（number 分支原整数）；aside created_at = `2026-01-15T14:30:00Z`、epoch_ms = `1705316400123`（旧 formatCell 原串） |
| P4 | 编辑原值与取消往返 | （a）CellRenderer `isEditing` + text plan：input 值断言 + rerender `isEditing={false}`；（b）DetailPanel editable：首个字段 input 初值 → 改值 → Escape → 断言恢复原值且 `onFieldEdit` 未调用 → rerender 非编辑态；（c）NULL 行 grid/aside | ✅ 通过。（a）编辑态 input 值为 `2026-01-15T14:30:00Z`，不含呈示文本；退出编辑后恢复 `2026-01-15T22:30:00+08:00`。（b）aside 编辑框初值为原值；改值后 Escape 恢复原值、`onFieldEdit` 未调用；退出编辑后 `<pre>` 恢复呈示文本。（c）NULL 两处均为 `NULL` |
| P5 | 草稿行编辑态 | DataTable `draftRows` + `editingDraft={draft-a/created_at}`（已写入时间格子进入编辑） | ✅ 通过。草稿编辑格内为 EditableCell，input 值为原值 `2026-01-15T14:30:00Z`，不含呈示文本。另复核 coder 已覆盖的已写入呈示 / 未写入 DEFAULT（见 §3） |
| P6 | 计划缺失/回退 | （a）registry 空（未注册贡献）+ enabled snapshot；（b）`enabledRendererIds: []` + 已注册贡献；（c）非法值 `not-a-date` + enabled snapshot，并与无 snapshot 渲染逐字对照 | ✅ 通过。（a）grid/aside 走原有分支（`2026-01-15T14:30:00.000Z` / `2026-01-15T14:30:00Z`，epoch 整数），不崩溃。（b）默认路由列回退旧展示；custom 覆盖列仍呈示 `2024-01-15T19:00:00.123+08:00`——这是有意设计（custom bypass enabled，见 O-1；首轮探针曾误判为失败，已按代码注释 `presentationPlan.ts:339` 与 T04 P5 修正探针期望）。（c）非法值两处均为 `not-a-date`，与无 snapshot 渲染逐字相同 |
| P7 | Source 一致性 | 见 P7a-table / P7a-query / P7b / P7c 四项 | ⚠️ 见 §4：P7a-table 暴露 **BUG-005（严重级）**；P7c 暴露 **BUG-006（轻微级）**；其余两项通过 |
| P7a-table | 跨库抽屉（真实组件） | 种子 `useTableDataStore`（`emptyTableState` + 列/行/`detailRowIndex: 0`）；`ContentViewDrawers` 挂真实 DetailPanel，表面板 `database='analytics'`，`currentDatabase='shop'`，快照带 analytics 键的 epoch_ms 覆盖；贡献注册进全局 registry（抽屉不接受 registry prop，生产亦用全局单例） | ❌ **BUG-005**：aside 内 `aside pre` 数量为 0，三个字段全是编辑框，值分别为原值 `2026-01-15T14:30:00Z` / `1705316400123` / `42`——T05 接在 `<pre>` 分支上的呈示在表 aside 里完全不可达（详见 §4） |
| P7a-query | 只读查询结果抽屉（真实组件） | 种子 `usePanelStore` queryExec（无 `source` / 无 `runtimeBinding` 的结果，`resultDetailRowIndex: 0`）+ mock `queryCommands.resolveResultOrigin`；query 面板 + shop 键快照 | ✅ 通过。aside `<pre>`：created_at = `2026-01-15T22:30:00+08:00`（默认路由呈示）；epoch_ms = `1705316400123`（表覆盖因 source=undefined 不命中）；qty = `42` |
| P7b | source=undefined 时覆盖不可用 | DataTable + DetailPanel 不传 `presentationSource`，快照带表覆盖 | ✅ 通过。created_at 两处仍走默认路由呈示 `2026-01-15T22:30:00+08:00`；epoch_ms 两处均为整数 `1705316400123` |
| P7c | 两者皆缺的键分歧（计划层） | `buildColumnPresentationPlans`：aside 口径 source（database 缺省）vs grid 口径 source（database `''`），同一份 database 缺省的覆盖 | ❌ **BUG-006**：aside 侧命中（`present(1705316400123)` = `{kind:'text', text:'2024-01-15T19:00:00.123+08:00', interpretation:'unix'}`），grid 侧不命中（`{kind:'unchanged'}`，status 非 ready）。详见 §4 |
| P8 | 注册变更响应 | DataTable 挂载后 `registry.unregister(TIMESTAMP_CONTRIBUTION_ID)` 再 rerender（同一 registry 对象、同一 snapshot） | ✅ 通过。注销前 `2026-01-15T22:30:00+08:00`；rerender 后恢复 `2026-01-15T14:30:00.000Z`，epoch 恢复整数——plans 随 contributionRevision 失效重建 |
| P9 | 样式与 tooltip、截断 | （a）CellRenderer + 200 字假 plan：span 类、title、textContent 长度；（b）与无 plan 的 datetime 分支逐类比对；（c）DetailPanel aside `<pre>` 类 | ✅ 通过。（a）呈示 span 含 `truncate font-mono text-xs`，`title` 为原值字符串；200 字文本完整保留在 DOM 中——**没有 120 字 JS 截断**，只有 CSS `truncate`（与 datetime 分支逐类完全相同）。（b）aside `<pre>` 无 `truncate` 类，含 `whitespace-pre-wrap`——长呈示文本在 aside 换行而非截断（见 O-5） |
| P10 | 其他 DataTable 调用点无回归 | 调用点 grep（无一处传 presentation props，TableView 除外）+ G2 全量 + 单独抽查 ExplainPanel / ResultTableView / ProcessListView 三文件 | ✅ 通过。三文件 17/17 通过（退出码 0）；G2 1195 项全量覆盖 windows/connection 内调用点。无 props 时默认快照（时间列 disabled），行为与改动前一致 |

> 探针过程说明：17 项中首轮 2 项失败均为**探针自身缺陷**，已修正探针、代码未动：P6b 误把 custom bypass enabled 当作回退失败（该 bypass 是 T04 确立的有意设计，见 O-1）；P7a-query 首轮未向全局 registry 注册贡献（抽屉不接受 registry prop，生产亦用全局单例），致计划缺失而显示原值，补注册后通过。修正后 17/17 通过；除 P7a-table 与 P7c 暴露的两 bug 外，被测代码在全部探针下无失败。

### 2.3 契约类型严格性审读（G6）

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| T1 | 改动范围 | `git show 800dcdfc5 --stat` / `--numstat` | ✅ 8 文件、272 行增 / 2 行删，全部在 T05 范围内（CellRenderer 27+、DataTable 61+、DetailPanel 83+/2-、DraftRow 10+、GridCell 9+、VirtualBody 14+、ContentViewDrawers 43+、TableView 25+），无无关改动。2 行删均在 DetailPanel：`FieldRow` 单行签名拆为多行 1 行 + `<pre>` 内容行替换 1 行，均为机械改动 |
| T2 | any 泄漏 / 断言跳过验证 | grep 新增行 `any`、新测试 `it.skip/test.skip/.todo(/xit(/xdescribe` | ✅ 新增行中 `any` 0 处（唯一命中为注释词 "anything"）；新测试 0 处跳过 |
| T3 | 编辑分支与 NULL 分支无改动 | 全 diff 审读 | ✅ `CellRenderer` 的 `isEditing` 返回块与 `value === null || value === undefined` 返回块均为上下文行，一字未动；呈示求值在两者之后、family 分支之前（`:129`）。`DetailPanel` 的 `isNull` 判定与 `InlineFieldEditor` 的 `toEditString(value)` 初始化均未动 |
| T4 | QuickLookDialog 仍传原值 | 审读 + grep | ✅ `DetailPanel.tsx:280-285` 的 `QuickLookDialog` 仍传 `value={value}`（原值）；`GridCell` 的 quicklook 仍传 `rowIndex/columnName`，未改动 |
| T5 | 默认快照只读性 | grep `DEFAULT_PRESENTATION_SNAPSHOT` 全仓 | ✅ 两处模块级常量（DataTable `:438`、DetailPanel `:61`）只在 `??` 右侧被读取，无任何写操作。附注：两文件各持一个实例，并非同一个共享对象（见 O-4）；两者均不被修改，行为等价 |
| T6 | 申报语义逐条核验 | 逐条对照 diff 与源码（见 §3） | ✅ 第 1–4 条申报全部成立；第 5 条偏差成立且正确（跨库场景），但附带 BUG-006 的残余分歧（见 §4） |

## 3. 设计符合性逐条结论

对照设计文档（§7 / §10-A 相关项）与 coder 申报：

**Coder 申报核验**

| 条目 | 结论 |
|------|------|
| 1. CellRenderer：presentationPlan 在编辑/NULL 之后、family 之前求值；text 结果以 datetime 分支同款式样渲染、title 为原值；unchanged/无 plan 走原有分支；编辑与 NULL 不动 | ✅ 符合（diff `:129`；P9 逐类比对与 datetime 分支完全相同；P4/P6 验证编辑/NULL/unchanged；G6-T3 确认分支未动） |
| 2. GridCell/VirtualBody/DraftRow 逐级透传 planByColumn；DraftRow 仅已写入格子查表，DEFAULT 占位与 EditableCell 不动 | ✅ 符合（diff 逐级转发；P5 验证草稿编辑态原值；coder 自带测试验证已写入呈示/未写入 DEFAULT） |
| 3. DataTable/DetailPanel 各自调 useValuePresentation，模块级默认快照共享只读；无 props 时默认快照（时间列 disabled），行为与改动前一致；DetailPanel 的 hook 在 `!open` 早返之前 | ✅ 符合（hook 调用：DataTable `:619`、DetailPanel `:100`；早返在 `:107` 之后；DataTable 在 `:609` 前无早返；P3 验证默认关闭；G6-T5 确认只读） |
| 4. TableView：connectionId 缺省时 presentationSource 为 undefined（不伪造空串）；ContentViewDrawers：仅 table/view 面板派生 source，query 等为 undefined | ✅ 符合（TableView `:1271-1280` 三元；ContentViewDrawers `:305-335` 两分支 + else undefined；P7b/P7a-query 验证 query 侧行为） |
| 5. 有意偏差：aside 的 database 取 `activePanel.database ?? currentDatabase ?? undefined`，而非任务书字面的 `currentDatabase ?? undefined` | ✅ 偏差成立且正确（见下方评审）。但附带 BUG-006：两者皆缺时 grid 得 `''`、aside 得 `undefined`，键不一致 |

**第 5 条偏差评审（重点）**

coder 的理由成立，评审通过。传递链实测：

- Grid 侧：`PanelContentRenderer.tsx:230`（表）/`:319`（视图）传给 TableView 的是 `database={panel.database ?? currentDatabase ?? ''}`，即面板 pin 住自己的库；TableView `:1275` 把该 prop 原样放进 `presentationSource.database`。
- Aside 侧：`ContentViewDrawers.tsx:321`（表）/`:328`（视图）取 `activePanel.database ?? currentDatabase ?? undefined`，与 grid 侧除末端回退外完全一致。
- 若 aside 只用 `currentDatabase`：用户把会话当前库切到另一库、而面板仍 pin 住原库时，grid source 为原库、aside source 为新库，覆盖键对不上——coder 指出的问题真实存在。P7a-table 的对照探针（覆盖键在 `shop` 上时 aside 不命中）反向证实：aside 确实没有用 `currentDatabase`。
- 因此偏差正确：跨库场景下 grid/aside 的 source 三元组一致，覆盖键相同。

唯一残余分歧见 BUG-006：两者皆缺时 grid 为 `''`（PanelContentRenderer 的 `?? ''` 是既有行为，TableView 的 `database` prop 类型即 `string`），aside 为 `undefined`——两端仍不对齐。该分歧不否定偏差本身，只需一处收敛（见 §4 修复建议）。

**§7 共享展示管线**

| 条目 | 结论 |
|------|------|
| 面板层共享 controller/hook，列计划经 props 注入 DataTable 与 DetailPanel，组件不直接读 Store 或 IPC | ✅ 符合（两组件各调 `useValuePresentation`，计划经 props 逐级下发；T05 未新增 Store/IPC 读取。附注：生产快照尚未接通，见 O-2） |
| 修改 CellRenderer：编辑分支继续接收原值；NULL 优先；之后尝试展示计划，再走原有 bool/number/datetime/json 等默认分支；BIGINT 在默认数字分支返回前可被显式时间计划处理 | ✅ 符合（P2/P4/P6 验证；BIGINT custom 覆盖在 number 分支前命中） |
| 修改 DetailPanel：非编辑态使用同一展示结果；InlineFieldEditor 进入编辑时从原值初始化，取消恢复展示，提交仍使用既有类型校验；不把格式化文本写进行数据或 pending changes | ⚠️ **部分符合，见 BUG-005**。组件级（DetailPanel 单独渲染、非编辑态）全部成立；但生产主流程（表/视图面板经 ContentViewDrawers）中 aside 永远处于编辑态，`<pre>` 展示分支不可达，"取消恢复展示"亦无从发生——设计假定的"非编辑态"在该流程不存在 |
| 草稿行和查询结果网格接入同一逻辑 | ✅ 基本符合。草稿行已接入（已写入格子呈示、未写入 DEFAULT、编辑态原值，P5 + coder 测试）；查询结果网格：ResultTableView 不传 presentation props，走默认快照（旧行为），source=undefined 时时间列仍走默认路由（P7b/P7a-query）。查询结果的表覆盖不可用符合设计（无来源） |
| 计划缓存键至少包含来源身份、列元数据版本、配置修订、贡献修订、locale、localTimeZone | ➖ 范围外（T04 已验证通过；T05 未动缓存键。附注：BUG-006 的键分歧正源于缓存键与覆盖键共用的 source 表达不一致） |
| 扩展异常由宿主统一隔离；普通非法值返回 unchanged，不作为扩展异常 | ✅ 符合（P6c：非法值回退旧分支；T04 已验证 prepare 异常隔离；T05 接线未引入新异常面——P6a 空 registry 不崩溃） |
| 日志记录贡献与原因码，不记录原始行值 | ➖ 未接线（T04 已记录 O-7；T05 未新增日志调用，不构成缺口） |

**§10-A 验证清单（阶段 A，与 T05 相关项）**

| 条目 | 结论 |
|------|------|
| cell/aside 默认一致和编辑原值 | ⚠️ **组件级符合、生产表流程不符合，见 BUG-005**。组件级：同一 snapshot 下 grid/aside 逐字相同（P1/P2）、编辑原值（P4）、默认关闭（P3）。生产表流程：grid 呈示、aside 显示原值输入，两处不一致 |
| 验证重复 ID、旧卸载句柄、激活失败清理 | ➖ 范围外（T01 已验证通过；T05 未重测注册中心本身。P8 验证注销后计划失效重建） |

## 4. Bug 列表

| 编号 | 标题 | 级别 | 状态 |
|------|------|------|------|
| BUG-005 | 表/视图 aside 永远编辑态，T05 的 aside 呈示分支在生产主流程不可达 | 严重级 | 已关闭（2026-11-12 回归验证通过，见 §7） |
| BUG-006 | 两者皆缺时 grid 的 database 为 `''`、aside 为 `undefined`，覆盖键不一致 | 轻微级 | 已关闭（2026-11-12 回归验证通过，见 §7） |

**BUG-005 — 表/视图 aside 呈示分支不可达（严重级）**

- 位置：`packages/frontend/src/windows/connection/ContentViewDrawers.tsx:435-442`（`editable` 恒真 + `onFieldEdit` 恒传）× `packages/frontend/src/components/DataTable/DetailPanel.tsx:256-257`（`editable && onCommit` 时永远渲染 `InlineFieldEditor`）× `DetailPanel.tsx:268-274`（呈示只在 `<pre>` 分支被查阅）。
- 复现输入：任一表面板（如 `database='analytics'` 的 orders 表）+ 任意启用快照（如 native iso-tz / Asia/Shanghai，epoch_ms 有 custom unix-ms 覆盖）+ 打开行详情抽屉。
- 实际输出：抽屉内三个字段全是编辑框（`aside pre` 数量为 0），值分别为原值 `2026-01-15T14:30:00Z` / `1705316400123` / `42`；同一快照下 grid 显示 `2026-01-15T22:30:00+08:00` / `2024-01-15T19:00:00.123+08:00`。探针 P7a-table 实测。
- 正确应为：设计 §7"非编辑态使用同一展示结果"与 §10-A"cell/aside 默认一致"——aside 应显示与 grid 相同的呈示文本，编辑时才显示原值；取消后恢复展示。
- 根因：生产唯一一处 DetailPanel 调用点（ContentViewDrawers `:425`，全仓 grep 仅此一处生产调用）对表/视图面板恒传 `editable={activePanel?.type !== 'query' || …}`（恒真）与 `onFieldEdit={handleDetailFieldEdit}`（恒有），故 `FieldRow` 永远走 `InlineFieldEditor` 分支；T05 把呈示只接进了 `<pre>` 分支，该分支在表/视图流程中永不渲染。`InlineFieldEditor` 自身亦无"展示态"：它永远渲染一个值为原值（`toEditString(value)`）的 input，`editing` 状态只改样式；Escape 后仍显示原值 input，不存在"取消恢复展示"的状态。
- 为什么 coder 自带测试没抓住：三份自带测试均单独渲染 DetailPanel，且默认不传 `editable`（非编辑态），只覆盖了 `<pre>` 分支；没有一处经真实 ContentViewDrawers 挂载、也没有覆盖表面板 `editable` 恒真的生产接线。
- 影响：T05  headline 目标在主流程未达成——用户在表上看到 grid 时间已格式化、aside 时间仍是原值；且"编辑原值、取消恢复展示"的设计语义在表 aside 上不存在（取消后仍是原值输入）。只读查询结果 aside 不受影响（该路径 `editable=false`，P7a-query 验证呈示正常），故 bug 限于表/视图面板。
- 修复建议（供 coder / 协调方定夺，不改被测代码）：首选按设计补齐 aside 的展示态——`FieldRow` 在字段未被聚焦编辑时渲染呈示文本（`<pre>` 分支），聚焦进入编辑时才切为原值 input，取消/失焦恢复展示；或由协调方裁决修订设计（明确表 aside 恒为编辑态、呈示仅用于只读 aside），则 BUG-005 关闭为"设计修订"，T05 收尾不受阻，但 §10-A 的"cell/aside 默认一致"须同步改写。无论哪种裁决，都应补一条经真实 ContentViewDrawers 挂载表面板的回归测试（断言 aside 在有快照时显示呈示文本或按裁决显示原值），避免组件级测试再次掩盖生产接线。
- 归属说明：缺陷不在 T05 引入的呈示逻辑本身（计划构建与 `<pre>` 分支均正确），而在 T05 的接线点选择与生产调用点的既有恒编辑态之间的错配；T01–T04 不受影响。

**BUG-006 — 两者皆缺时 grid/aside 的 database 表达不一致（轻微级）**

- 位置：`packages/frontend/src/windows/connection/TableView.tsx:1271-1280`（`database` 取自 prop）× `PanelContentRenderer.tsx:230` / `:319`（`database={panel.database ?? currentDatabase ?? ''}`）× `ContentViewDrawers.tsx:321` / `:328`（`database: activePanel.database ?? currentDatabase ?? undefined`）。
- 复现输入：database 缺省的覆盖（如 SQLite 类无库概念的连接：面板无 database、会话当前库为 null）+ `buildColumnPresentationPlans` 分别以 grid 口径（database `''`）与 aside 口径（database 缺省）求计划。
- 实际输出：aside 侧命中（`present(1705316400123)` = `{kind:'text', text:'2024-01-15T19:00:00.123+08:00', interpretation:'unix'}`），grid 侧不命中（`{kind:'unchanged'}`，status 非 ready）。探针 P7c 实测。根因为 BUG-004 修复后缺省（`null`）与空串（`''`）键不同，而两端恰好各取其一。
- 正确应为：同一表在 grid 与 aside 的 source 三元组完全相同（第 5 条偏差的评审结论），覆盖键相同。
- 影响：现实中窄——仅当面板 database 与会话当前库同时缺失时触发；但该场景在无库概念的连接上可达，且 T06 持久化后覆盖键即成为持久化身份，一端存、一端读即会串用或漏用。
- 修复建议（供 coder / 协调方定夺，不改被测代码）：收敛一处，二选一：(a) TableView 的 `presentationSource` 取 `database: database || undefined`（空串归一为缺省，与 aside 一致，且不伪造空串，符合 BUG-004 原则）；(b) 抽屉侧改取与 grid 完全相同的已解析值（需把 PanelContentRenderer 的解析结果透传，改动面大）。推荐 (a)，一行改动，并补一条两者皆缺的对照回归断言。注意：不要反向把 aside 改为 `?? ''`——那会重新引入缺省/空串碰撞。
- 归属说明：分歧由 T05 两端各自的派生引入（grid 沿用 PanelContentRenderer 既有的 `?? ''`，aside 新写 `?? undefined`）；T01–T04 的键逻辑本身无误。

## 5. 观察项（非缺陷，供 coder / 后续任务参考）

- **O-1（custom 覆盖 bypass enabled 是有意设计，非缺陷）**：`enabledRendererIds` 为空时，默认路由列（created_at）正确回退旧展示，但 custom 覆盖列（epoch_ms）仍呈示。代码注释 `presentationPlan.ts:16` 与 `:339-340` 明确写明"显式 custom 覆盖 bypass enabled 门禁"，T04 报告 P5 亦已锁定该语义。评审：分工可接受。提醒 T07 设置 UI：全局开关文案须说明"已逐列指定的 custom 覆盖不受全局开关影响"，否则用户会误以为禁用即全关。
- **O-2（生产快照尚未接通，T05 接线目前只在测试中可达）**：`ContentView` 既不向 `PanelContentRenderer` 传 `presentationSnapshot`，也不向 `ContentViewDrawers` 传——`PanelContentRenderer` 甚至没有该 prop。生产环境下 TableView 与抽屉永远走模块级默认快照（时间列 disabled，旧行为），T05 的真实快照通路要等 T06 持久化链路接通才生效。这是设计分工（T06 供快照），非缺陷；但"整页挂载未覆盖"：没有任何测试经 ContentView 整页挂载验证快照透传，建议 T06 落地时补一条整页级回归。
- **O-3（view 面板 aside 无列的既有缺口，与 T05 无关）**：`ContentViewDrawers` 的 `detailColumnDefs` 只处理 table 面板与查询结果；view 面板走不到任一分支，得 `[]` 且 `detailRow` 为 null，aside 为空。这是 T05 之前就存在的缺口（MEMORY 已记录），T05 未引入、未修复；建议 T09 列配置面板或后续任务一并处理，不要误记为 T05 回归。
- **O-4（两份默认快照实例，并非同一对象）**：DataTable `:438` 与 DetailPanel `:61` 各调一次 `createDefaultValuePresentationSnapshot()`，各持一个实例；"共享"只在各自模块内成立。两者均不被修改、值等价，行为上无差异；若未来快照获得可变字段，应收敛为单一导出实例，避免两份静默分叉。
- **O-5（呈示文本截断行为两端不同）**：grid 的呈示 span 靠 CSS `truncate`（无 120 字 JS 截断，P9 以 200 字文本验证 DOM 全文保留），aside 的呈示 `<pre>` 无 `truncate`、靠 `whitespace-pre-wrap` 换行。时间文本短于 120 字时无差异；未来若有长文本展示器接入，两端外观会分叉（grid 截断、aside 全文换行），届时再定夺是否统一。
- **O-6（GridCell 函数覆盖率恰踩线）**：`GridCell.tsx` 四项为 82.6 / 90.21 / **80** / 82.6，Funcs 恰为 80.0。下次改动该文件若减一条函数覆盖即跌破门禁；未覆盖语句 268–269、341、346 均为既有行（中键 QuickLook、FK 下拉），与 T05 无关。
- **O-7（`value as PresentationValue` 两处断言）**：CellRenderer `:128` 与 DetailPanel FieldRow `:221` 均把 `unknown` 的 `value` 直接断言为 `PresentationValue`，无运行时收窄。实践中安全——`present()` 是全函数，非法值返回 unchanged（P6c 验证）——但属类型层面的信任而非验证；若未来有展示器假定输入已校验，应在调用点收窄或在契约中写明。
- **O-8（探针资产）**：`/tmp/probe-t05` 为一次性验证项目，已按任务要求删除（`/tmp` 在任务中途被环境清理，删除结果等效；仓库 `git status` 复核干净）。本报告 §2.2 各项方法已描述到可复现程度。建议将 P7a-table 的真实抽屉挂载断言、P7c 的两者皆缺键对照、P8 的注销后 rerender 断言吸收为仓库回归测试（P7a-table 的期望方向取决于 BUG-005 的修复/裁决结果）。

## 6. 结论

**不通过，不可做收尾提交。**

- 既有门禁全绿：相关包 748/748（`--retry=0` 复跑确认，含新增 CellRenderer.presentation 7、DataTable.presentation 7、DetailPanel.presentation 9）、windows/connection 全量 1195/1195、`pnpm typecheck` 与 `pnpm typecheck:tsc` 双引擎通过、命名门禁通过；六个 DataTable 文件覆盖率四项指标全部 ≥80%（CellRenderer 100 / 97.61 / 100 / 100；DataTable 93.68 / 95.45 / 85.71 / 93.67；DetailPanel 88.63 / 88.57 / 85.71 / 90.12；DraftRow 100 / 85.18 / 87.5 / 100；GridCell 82.6 / 90.21 / 80 / 82.6；VirtualBody 90.83 / 88.52 / 100 / 93.39；G5 进程退出码 1 唯一原因是全局阈值，见 §2.1 注）。
- 对抗性探针 17 项中 15 项通过：PRD 双锚点 grid/aside 逐字相同、BUG-003 不回归（`.123` 保留）、默认关闭（grid 旧 formatTimestamp、aside 旧 formatCell、epoch 恢复整数）、编辑原值与取消往返（grid 编辑态 input 为原值、aside Escape 恢复原值且不提交、NULL 两处仍为 NULL）、草稿行编辑态原值、计划缺失/禁用/非法值回退、注册注销后重渲染恢复默认、样式与 tooltip（与 datetime 分支逐类相同、title 为原值）、其他调用点抽查 17/17。
- 第 5 条 database 偏差评审通过：coder 的理由成立，传递链核实无误，跨库场景下 grid/aside 的 source 三元组一致。
- 阻塞项 **BUG-005（严重级）**：表/视图 aside 在生产主流程中永远处于编辑态，T05 接在 `<pre>` 分支上的呈示完全不可达——grid 显示呈示时间、aside 显示原值输入，设计 §10-A 的"cell/aside 默认一致"在主流程未达成。组件级测试全部通过恰恰掩盖了这一点（自带测试只单独渲染非编辑态 DetailPanel）。须修复（补齐 aside 展示态）或由协调方裁决修订设计并同步改写 §10-A，方可收尾；同时须补一条经真实 ContentViewDrawers 挂载表面板的回归测试。
- 非阻塞项 **BUG-006（轻微级）**：两者皆缺时 grid 的 database 为 `''`、aside 为 `undefined`，同一覆盖在 aside 命中、在 grid 不命中；推荐一行修复（TableView 侧 `database || undefined`）并补对照断言，可与 BUG-005 同批处理。
- 观察项 8 条均为非缺陷（O-1 custom bypass enabled、O-2 生产快照未接通、O-3 view aside 既有缺口、O-4 两份默认快照、O-5 截断行为两端不同、O-6 GridCell 函数覆盖踩线、O-7 两处 `as` 断言、O-8 探针资产），其中 O-2 建议在 T06 落地时补整页级回归。
- tasks.md 状态更新与 docs close commit 不在本次验证范围内，由协调方执行。

## 7. 回归验证记录（BUG-005 / BUG-006 修复）

- 验证人：tester（独立回归验证，与 T05 验证为不同 tester）
- 日期：2026-11-12
- 验证对象：修复提交 `78f71928c`（`DetailPanel.tsx` 展示态 + `TableView.tsx` source 组装）与回归测试提交 `dacfccf48`（新增 `DetailPanel.displayState.test.tsx` 10 项 + `ContentViewDrawers.detailDisplay.test.tsx` 2 项 + 8 个既有 `it` 调整）；验证开始时 HEAD 即 `dacfccf48`，工作区除本报告外干净。

### 7.1 独立探针（/tmp/probe-bug56，25 项，全部通过，退出码 0）

探针为一次性验证项目（`vitest.config.ts` 复用仓库 setup 与 tauri mock，被测源码以绝对路径导入，`node_modules` 符号链接复用仓库依赖，不污染仓库工作区；用后删除）。探针首轮 1 项失败为探针自身假设错误（误以为 DetailPanel `<pre>` 对长文本做 120 字截断，实为 `formatCell` 不截断；另 QuickLook 对话框为 portal，需在 `document` 而非 `container` 内查询），修正探针后 25/25 通过，被测代码未动。

| # | 内容 | 方法 | 结果 |
|---|------|------|------|
| R1 | BUG-005 原始复现 | DetailPanel 以 `editable=true + onCommit +` 启用快照挂载；同一 snapshot 下另挂 DataTable，比对 grid/aside 文本 | ✅ 通过。aside 默认呈示 `2026-01-15T22:30:00+08:00` / `2024-01-15T19:00:00.123+08:00` / `42`，`<pre>` > 0，无字段 input；三列 grid 文本与 aside 文本逐字相同 |
| R2 | 进入编辑态 | 点击 / Enter / Space 展示容器 | ✅ 通过。点击出现原值编辑器（初值 `2026-01-15T14:30:00Z`），`document.activeElement` 即该 input，单次点击可输；Enter 与 Space 同样打开 |
| R3 | 提交 / 取消往返 | Escape；改值后 blur；改值后 Enter | ✅ 通过。Escape 回到呈示文本且 `onFieldEdit` 未调用（含未改值情形）；blur 与 Enter 各提交一次并回到展示态 |
| R4 | NULL / blob / 非编辑态 | NULL 行；200 字 bytea 列；不传 `editable` 的 query aside；列搜索 | ✅ 通过。NULL 显示 NULL；blob 显示摘要、无 `<pre>`、无长串；query aside 呈示正常、无展示容器；搜索过滤仍可用 |
| R5 | 生产组合（真实抽屉） | 真实 `ContentViewDrawers` 挂载表面板（真实 DetailPanel、真实 `useTableDataStore` 种子、全局 registry；仅 mock `useResizable` / `useI18n`） | ✅ 通过。aside 呈示与 grid 一致、默认仅搜索框一个 input、点击出原值编辑器且自动聚焦、Escape 回到呈示。另复核 fixer 的 `ContentViewDrawers.detailDisplay.test.tsx` 仅 mock `useResizable` 与 `useI18n`，未 mock `DetailPanel` / `ContentViewDrawers`，真实挂载成立，无过度 mock 导致的回归失效 |
| R6 | BUG-006 原始复现 | (a) 真实 `buildColumnPresentationPlans` 路径对照；(b) 真实 `TableView` 挂载（`database=''`，覆盖以 database 缺省为键） | ✅ 通过。(a) 修复后 grid/aside 键相等、均命中同一缺省覆盖；修复前 `''` 不命中；`'shop'` 真实值不受影响，`''` 不命中 `'shop'` 覆盖。(b) grid `epoch_ms` 呈示 `2024-01-15T19:00:00.123+08:00`，`TableView.tsx:1280` 行真实执行——见 §7.3 缺口记录 |
| R7 | 无回归 | QuickLook 中键与按钮；草稿行/网格编辑态；PRD 锚点与 BUG-003 | ✅ 通过。中键仍打开查看器；草稿行与网格编辑态仍从原值初始化；PRD 锚点与 `.123` 保留 |

### 7.2 门禁复跑（真实结果与退出码）

| # | 内容 | 结果 |
|---|------|------|
| G1 | 相关包单测 `--retry=0` | ✅ 60 文件 **758/758 通过**，退出码 0（耗时 110s；748 T05 + 10 新增 `DetailPanel.displayState`） |
| G2 | connection 窗口全量 `--retry=0` | ✅ 114 文件 **1197/1197 通过**，退出码 0（耗时 228s；1195 T05 + 2 新增抽屉测试） |
| G3 | `pnpm typecheck` 与 `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用） | ✅ 双引擎均 OK（tsrs 0.18.0-ts7.1.0-dev.20260929 / tsc 7.0.2，root / scripts / pack-ep 三程序），退出码均为 0 |
| G4 | `node scripts/check-test-module-naming.mjs` | ✅ ok（1837 Rust 文件、256 cfg(test) 模块；2566 TS 文件、8 wdio spec 树），退出码 0 |
| G5 | 代码审读 | ✅ `git show 78f71928c` 的 diff 最小，仅 `DetailPanel.tsx` 与 `TableView.tsx` 两文件；`InlineFieldEditor` 原有 Escape/commit/blur/类型分支逻辑一字未动（除新增 `onCancel?.()` 调用、`autoFocus` 属性与依赖数组补 `onCancel`）；展示态 `<pre>` 内容逻辑与只读分支逐字相同。8 个既有 `it`（`DetailPanel.presentation` 3 个 + `DetailPanel.test` 4 个 + `contentViewDrawersMerged` 1 个；bug 文件中 blur/Enter 合为一条，实为两个 `it`）逐条复核均为有意行为变更：仅加「点击开编辑器」前置操作，或把 Escape 后断言由「残留 input 值复位」改为更强的「编辑器卸载 + `<pre>` 恢复显示」，或把可编辑断言由「挂载时 input 数」改为「展示容器存在、点击后 input 数」；无删除断言、无放宽门禁；另有一处纯格式整理（`mysqlResult` 三元折叠为一行），语义相同 |

### 7.3 遗留问题

- **TableView:1280 覆盖缺口（已如实记录，判断为可接受）**：fixer 的 BUG-006 回归测试确如其所承认，只用复现组装逻辑的 `buildGridSource` 构建 source，`TableView.tsx:1280` 的组装行本身未被仓库自带测试直接执行——仓库回归套件层面该行仍无直接覆盖。独立探针 R6(b) 以真实 `TableView` 挂载执行了该行并验证行为正确，缺口在行为层面已闭合。建议后续（T06 或整理时）把 R6(b) 的真实 `TableView` 挂载吸收为仓库回归测试，使该行获得直接覆盖；不阻塞本次关闭与 T05 收尾。

### 7.4 结论

**两 bug 验证通过并关闭，T05 可做收尾提交。** BUG-005：设计 §7「非编辑态使用同一展示结果」与 §10-A「cell/aside 默认一致」在生产主流程（表/视图 aside）已达成，§6 的阻塞项解除。BUG-006：同一缺省在 grid 与 aside 两处组装出同一 source、命中同一覆盖。门禁全绿（758/758、1197/1197、双引擎类型、命名门禁），探针 25/25 通过；本报告 §4 两行状态已同步改为已关闭。探针临时项目 `/tmp/probe-bug56` 已删除；本次验证未修改任何源码或测试，仅更新两份 bug 文件与本报告。
