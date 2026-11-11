# BUG-005：可编辑详情抽屉永远渲染编辑器，呈示文本在生产主流程不可达

- 状态：已关闭（2026-11-12 独立回归验证通过）
- 严重程度：严重（阻塞：设计 §7「非编辑态使用同一展示结果」与 §10-A「cell/aside 默认一致」在主流程未达成）
- 发现阶段：issue-32 T05 独立测试验证（见 `docs/development/issue-32/testing/T05-grid-detail-wiring.md`，由协调方复核代码确认）
- 归属判定：**T05 贡献模块引入**（T05 接在 `DetailPanel` 的 `<pre>` 展示分支上，但可编辑面板永远走不到该分支）

## 现象

表/视图 aside 呈示分支在生产主流程不可达：任一表面板 + 启用快照 + 打开详情抽屉，aside `<pre>` 数量为 0，字段值显示原值 `2026-01-15T14:30:00Z` / `1705316400123` / `42`，而 grid 显示呈示文本。且「取消恢复展示」不存在：Escape 后仍是原值 input，没有回到展示的出口。

## 复现步骤

1. 打开任一表面板，启用 value-presentation 快照（T05：`iso-tz` + 上海时区，全局 `native` 解释，`epoch_ms` 列有 Unix 毫秒覆盖）。
2. 选中一行，打开详情抽屉。
3. 修复前：抽屉内 `aside pre` 数量为 0；`created_at` 字段是值为 `2026-01-15T14:30:00Z` 的 input（应为呈示文本 `2026-01-15T22:30:00+08:00`），`epoch_ms` 是值为 `1705316400123` 的 input（应为 `2024-01-15T19:00:00.123+08:00`）。
4. 在任一字段 input 中改值后按 Escape：修复前仍停留在原值 input；修复后回到呈示文本，且 `onFieldEdit` 未被调用。

## 期望行为

- 可编辑面板默认显示呈示文本（与 grid 一致），不挂载编辑器。
- 点击展示容器（或在其上按 Enter/Space）进入编辑态，编辑器以原值初始化，单次点击即可输入。
- 改值后 blur/Enter 提交并回到展示态；Escape 取消、不提交、回到展示态。

## 根因分析

`packages/frontend/src/components/DataTable/DetailPanel.tsx:256-257`（修复前行号）：`FieldRow` 的第一分支是 `editable && onCommit ? <InlineFieldEditor …/>`，而 `ContentViewDrawers.tsx:435-442` 对表/视图面板恒传 `editable=true + onFieldEdit`，故 `DetailPanel.tsx:256-257` 的 `FieldRow` 永远走 `InlineFieldEditor`（原值 input），T05 接在 `<pre>` 分支（`:268-274`）的呈示永不渲染。`InlineFieldEditor` 内部只有 focus/blur 的编辑态，没有「回到展示」的出口，所以 Escape 也只能复位 input，不能恢复展示。

Coder 自带测试只单独渲染非编辑态 `DetailPanel`（`DetailPanel.presentation.test.tsx` 中呈示断言均不带 `editable`），故未抓住。

## 修复记录

只改 `DetailPanel.tsx` 内 `FieldRow` 与 `InlineFieldEditor`（提交 `78f71928c`）：

- `FieldRow` 新增本地状态 `editing`（boolean，默认 false，`DetailPanel.tsx:209`）。分支顺序保持现有优先级，只把第一分支拆成两态：
  - `editable && onCommit && !editing` → 展示态：可聚焦的展示容器（`div`，`role="button"`、`tabIndex={0}`，onClick 与 Enter/Space 的 onKeyDown 均进入编辑态，`DetailPanel.tsx:262-299`），内含原 `<pre>`（同样式、同样内容逻辑：NULL 显示 NULL；`presented?.kind === 'text'` 显示 `presented.text`，否则 `pretty ?? formatCell(value)`；blob 列显示与现有 blob 分支完全相同的摘要 div，不展示 `formatCell` 长串）。
  - `editable && onCommit && editing` → `InlineFieldEditor`（原值初始化，一字不动其初始化逻辑），接线 `onCommit={(v) => { onCommit(v); setEditing(false); }} onCancel={() => setEditing(false)} autoFocus`（`DetailPanel.tsx:300-312`）。
  - 其余分支（isBlob 摘要、`<pre>` 默认展示）一字不动。
- `InlineFieldEditor` 新增两个可选 props：`autoFocus?: boolean`（传给 input/textarea 的 `autoFocus` 属性，进入编辑态时单次点击即聚焦可输，不造成「点两次才能输入」的回归）；`onCancel?: () => void`（Escape 时调用，原有 Escape 行为——local 复位、内部 `editing=false`、不 commit——一字不动，另调 `onCancel`；`DetailPanel.tsx:355-361`、`425`）。
- 不动：QuickLook 逻辑与 middle-click、`canQuickLook` 按钮、搜索过滤、`selectedRows` 多行模式、`InlineFieldEditor` 的类型分支 commit 逻辑、`FieldRow` 容器 onMouseDown。

既有测试调整（有意行为变更，见回归测试提交；未删断言、未放宽门禁）：

- `DetailPanel.presentation.test.tsx`「initialises the editor from the raw value, never the presented text」：先断言默认展示呈示文本且无字段 input，再点击展示容器断言编辑器初值为原值。
- `DetailPanel.presentation.test.tsx`「commits a plain text edit on blur / on Enter (pre-existing path)」：操作前先点击展示容器打开编辑器。
- `DetailPanel.test.tsx`「commits edited integer on blur」：操作前先点击第 0 个展示容器。
- `DetailPanel.test.tsx`「commits boolean and json edits」：先点击第 2 个展示容器（meta）编辑 textarea，再点击第 3 个展示容器（active）编辑；`fieldInputs` 下标由 `[2]` 改为 `[0]`——一次只打开一个编辑器，active 是当时唯一的 input。
- `DetailPanel.test.tsx`「cancels edit on Escape without committing unchanged value」：操作前先点击展示容器；Escape 后的断言由「残留 input 的值复位为 Bob」改为「编辑器卸载、无字段 input、`<pre>` 恢复显示 Bob」——旧断言描述的是已不存在的中间态。
- `DetailPanel.test.tsx`「commits null when text field cleared」：操作前先点击展示容器。
- `contentViewDrawersMerged.test.tsx`「keeps the drawer's editable state verdict-driven」：可编辑侧的断言由「挂载时 input 数 > 1」改为「存在展示容器，点击后 input 数 > 1」；只读侧补断言「无展示容器」——旧断言假设编辑器挂载即存在，与新行为直接冲突；该用例的裁决语义（verdict 驱动可编辑态）未变。

回归测试（与 code 提交分开提交）：

- `packages/frontend/src/components/DataTable/__tests__/DetailPanel.displayState.test.tsx` 新增 `DetailPanel display state (issue-32 BUG-005)` 小节（8 项）：editable + 快照挂载默认显示呈示文本且无字段 input；点击展示容器出现原值编辑器且自动聚焦；Enter 打开编辑器；Escape 回到呈示文本且 `onFieldEdit` 未调用；改值后 blur / Enter 各提交一次并回到展示态；NULL 显示 NULL；blob 列仍显示摘要。
- `packages/frontend/src/windows/connection/__tests__/ContentViewDrawers.detailDisplay.test.tsx`（2 项）：经真实 `ContentViewDrawers` 挂载表面板（`useTableDataStore` 直接种 slice；表面板不走 IPC），断言 aside 呈示文本、默认无编辑器、点击出原值编辑器、Escape 回到呈示。
- 未改动任何与新行为无关的既有断言。

## 回归验证

- 验证人：tester（独立回归验证）
- 日期：2026-11-12
- 验证基线：分支 `feature/issue-32-value-presentation`，HEAD `dacfccf48`；修复提交 `78f71928c`（fix）+ 回归测试提交 `dacfccf48`（test）；T05 本体已由另一位 tester 验证（报告 `docs/development/issue-32/testing/T05-grid-detail-wiring.md`）。
- 方法：独立探针（一次性项目 `/tmp/probe-bug56`，复用仓库测试脚手架、被测源码以绝对路径导入，不污染仓库工作区；用后删除）+ 仓库门禁复跑 + 代码审读。
- 探针结果（25 项全部通过，退出码 0）：
  - R1 原始复现：DetailPanel 以 `editable=true + onCommit +` 启用快照挂载，aside 默认显示呈示文本 `2026-01-15T22:30:00+08:00` 与 `2024-01-15T19:00:00.123+08:00`，`<pre>` 数量 > 0，无字段 input；同一 snapshot 下 grid 文本与 aside 文本逐字相同（`created_at`、`epoch_ms`、`qty` 三列）。
  - R2 点击展示容器出现原值编辑器（初值 `2026-01-15T14:30:00Z`），`document.activeElement` 即该 input，单次点击可输；Enter 与 Space 同样打开编辑器。
  - R3 Escape 回到呈示文本且 `onFieldEdit` 未调用（含未改值情形）；改值后 blur 提交一次、Enter 提交一次，提交后均回到展示态。
  - R4 NULL 显示 NULL；blob 列显示摘要、无 `<pre>`、无长串；非编辑态（query）aside 行为不变（呈示文本正常、无 `role="button"` 展示容器）；列搜索过滤仍可用。
  - R5 经真实 `ContentViewDrawers` 挂载表面板：aside 呈示文本与 grid 一致、默认仅搜索框一个 input、点击出原值编辑器且自动聚焦、Escape 回到呈示。另复核 fixer 的 `ContentViewDrawers.detailDisplay.test.tsx`：仅 mock `useResizable` 与 `useI18n`，未 mock `DetailPanel` / `ContentViewDrawers`，真实挂载成立，无过度 mock 导致的回归失效。
  - R7 无回归：QuickLook 中键仍可用（中键与按钮均打开查看器）、搜索过滤仍可用、草稿行/网格编辑态仍从原值初始化、PRD 锚点与 BUG-003 不回归（`1705316400123` 呈示含 `.123`）。
  - 探针过程说明：首轮 1 项失败为探针自身假设错误（误以为 DetailPanel `<pre>` 对长文本做 120 字截断，实为 `formatCell` 不截断；另 QuickLook 对话框为 portal，需在 `document` 而非 `container` 内查询），修正探针后 25/25 通过，被测代码未动。
- 门禁结果（全部真实结果）：
  - G1：`npx vitest run packages/frontend/src/components/DataTable packages/frontend/src/lib/valuePresentation packages/frontend/src/hooks/__tests__/useValuePresentation.test.tsx --retry=0` → 60 文件 **758/758 通过**，退出码 0（耗时 110s；748 T05 + 10 新增 `DetailPanel.displayState`）。
  - G2：`npx vitest run packages/frontend/src/windows/connection --retry=0` → 114 文件 **1197/1197 通过**，退出码 0（耗时 228s；1195 T05 + 2 新增抽屉测试）。
  - G3：`pnpm typecheck` 与 `pnpm typecheck:tsc`（经 `npx -y pnpm@12.18.0` 等价调用）双引擎均 OK（tsrs 0.18.0-ts7.1.0-dev.20260929 / tsc 7.0.2，root / scripts / pack-ep 三程序），退出码均为 0。
  - G4：`node scripts/check-test-module-naming.mjs` → ok（1837 Rust 文件、256 cfg(test) 模块；2566 TS 文件、8 wdio spec 树），退出码 0。
  - G5 代码审读：`git show 78f71928c` 的 diff 最小，仅 `DetailPanel.tsx`（展示态 + 接线）与 `TableView.tsx`（source 组装）两文件；`InlineFieldEditor` 原有 Escape/commit/blur/类型分支逻辑一字未动，除新增 `onCancel?.()` 调用、`autoFocus` 属性与依赖数组补 `onCancel`；展示态 `<pre>` 的内容逻辑与只读分支逐字相同，blob 摘要表达式与只读分支相同。
- 8 条既有断言改动复核结论：逐条核对 `dacfccf48` 的 diff，8 个 `it` 全部确属有意行为变更，无借机放宽、无删除断言、无放宽门禁——
  - `DetailPanel.presentation.test.tsx` 3 个：`initialises the editor…`（新增默认展示呈示文本且无字段 input 的断言后才点击开编辑器，原编辑器初值断言一字未动）、`commits a plain text edit on blur / on Enter (pre-existing path)` 2 个（仅操作前加点击开编辑器，提交断言未动；bug 文件中合为一条，实为两个 `it`）；
  - `DetailPanel.test.tsx` 4 个：`commits edited integer on blur` 与 `commits null when text field cleared`（仅加点击开编辑器）、`commits boolean and json edits`（加两次点击开编辑器；`fieldInputs[2]` 改为 `[0]`——一次只打开一个编辑器，active 是当时唯一的 input，提交值断言未变）、`cancels edit on Escape…`（Escape 后断言由「残留 input 值复位为 Bob」改为「编辑器卸载、无字段 input、`<pre>` 恢复显示 Bob」——旧断言描述的是已不存在的中间态，新断言更强）；
  - `contentViewDrawersMerged.test.tsx` 1 个：`keeps the drawer's editable state verdict-driven`（可编辑侧由「挂载时 input 数 > 1」改为「存在展示容器，点击后 input 数 > 1」；只读侧补「无展示容器」断言；裁决语义未变）。
  - 另有一处纯格式整理（`mysqlResult` 三元折叠为一行），语义相同。
- 结论：**验证通过，BUG-005 关闭。** 设计 §7「非编辑态使用同一展示结果」与 §10-A「cell/aside 默认一致」在生产主流程（表/视图 aside）已达成；T05 的该阻塞项解除。
