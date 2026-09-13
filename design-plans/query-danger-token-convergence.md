# 查询路径危险/错误色收敛到 danger token

Written against: 54110ab85c2cf42d7110119516ffc71ed55e852f

## Evidence chain

- Surface: 主窗口连接工作区查询→结果路径：`QueryPanel` → `QueryEditorSection`（执行/停止）→ `QueryResultsPane`（`QueryTransactionModals.tsx`）→ `QueryErrorPanel` / `ResultWorkspace` → `DataTable`（选中条删除）→ `ObjectBrowser` 错误行；另含主窗口 chrome 两个错误分支（`MainPage` 加载失败 vs `ConnectionPage` 连接失败）
- Problem: 同一“错误/危险”语义在同一任务内用两套颜色语言直接矛盾：`ConnectionPage.tsx:687` 用语义 token `text-danger` 渲染连接错误，而 `MainPage.tsx:131` 用硬编码 `text-red-400` 渲染加载错误；查询失败面板（`QueryErrorPanel.tsx:44,46,71`）用 `border-red-500/20 bg-red-500/10 text-red-400`，而设计系统已为危险态定义 `danger` tone（见下）。`Button danger` 变体（`packages/ui/src/Button.tsx:17`）是全库唯一使用 `bg-red-500/90 text-white` 的变体，与 `primary`（`bg-accent text-on-accent`）、`run`（`bg-query-run text-on-accent`）的 token 写法矛盾。`QueryExecutionStatus.tsx:45` 停止按钮用 `text-red-300`，`DataTable.tsx:464` 删除行用 `text-red-400 hover:text-red-300`，`ResultWorkspace.tsx:80`、`ObjectBrowser.tsx:216`、`QueryTransactionModals.tsx:188,392` 的错误文本/容器全部硬编码 `red-*`，在暗色主题下与 `--c-danger`（light `#e05252` / dark `#ff7b72`）脱钩
- Design evidence: 宿主绑定契约——`src/styles/themes.css:35,115` 定义 `--c-danger`（light `#e05252` / dark `#ff7b72`），`tailwind.config.ts:57-59` 映射 `danger: 'var(--c-danger)'`；`packages/ui/src/Badge.tsx:10` 已拥有危险态范例 `danger: 'bg-danger/10 text-danger border border-danger/20'`；`ConnectionPage.tsx:687` 的 `text-danger` 是同一表面已存在的合法范例；`ExecutionSummaryCard.tsx:35-37` 的 `bg-success/15 text-success` 是成功态 token 范例（用于附带纠正复制成功图标的 `text-green-400`）
- Owner: `src/styles/themes.css`（`--c-danger`）、`tailwind.config.ts`（`danger` 映射）、`packages/ui/src/Badge.tsx`（danger tone 范例）、`packages/ui/src/Button.tsx`（danger 变体拥有者）
- Scope and affected surfaces: 11 处调用点（详见 Changes）+ 2 处锁死旧值的单测。`amber/yellow/green` 状态色、`Label` 必填星号、`DataTable --dt-*` 类型色不在本计划内
- Uncertainty: 无。映射关系唯一（`red-500/20→danger/20`、`red-500/10→danger/10`、`red-400/red-300→danger`、`red-500/90→danger`、`white on danger→on-accent`、`green-400 success icon→success`），不存在多解

## Design decision

把查询路径与主窗口错误分支的全部“错误/危险”硬编码红色收敛到语义 `danger` token，把 `Button danger` 变体改成与 `primary`/`run` 同构的 `bg-danger text-on-accent` 写法。此后浅/暗主题的错误色由 `--c-danger` 单点控制，`Badge danger`、`ConnectionPage text-danger` 与查询失败态共享同一语言；视觉身份（红色系警示、胶囊/边框/底色结构）保持不变，只是色值来源从 Tailwind 默认调色板换成主题 token。

## Reuse

- `--c-danger`（`src/styles/themes.css:35,115`）+ `danger` Tailwind 映射（`tailwind.config.ts:57-59`）
- `--c-success`（`src/styles/themes.css:33,113`）+ `success` 映射（`tailwind.config.ts:48-50`），仅用于三处复制成功 `Check` 图标（`QueryErrorPanel:58`、`CopyableError:65`、`ResultMessageDialog:43` 的 `text-green-400 → text-success`）
- `text-on-accent`（`tailwind.config.ts:45-47`），用于 `Button danger` 前景色，与 `primary`/`run` 一致
- Exemplar: `packages/ui/src/Badge.tsx:10`（`bg-danger/10 text-danger border border-danger/20`）、`src/windows/connection/ConnectionPage.tsx:687`（`copyable text-sm text-danger`）、`src/windows/connection/result-workspace/ExecutionSummaryCard.tsx:35-37`（`bg-success/15 text-success`）

不新增图元。现有系统已能表达全部决策；`red-*` 是应被收敛的并行路径。

## Changes

1. `packages/ui/src/Button.tsx:17`
   - Change: `danger: 'bg-red-500/90 text-white hover:bg-red-500 disabled:opacity-50'` → `danger: 'bg-danger text-on-accent hover:bg-danger/90 disabled:opacity-50'`（与 `primary: 'bg-accent text-on-accent …'`、`run: 'bg-query-run text-on-accent …'` 同构）
   - Preserve: 变体名 `danger`、尺寸、focus ring、disabled 行为不变；调用点（`QueryTransactionModals.tsx:181` 中止事务回滚按钮等）不用改
   - Verify: 中止事务回滚按钮、确认框危险按钮在浅/暗主题下均为 `--c-danger` 底 + `--c-on-accent` 字，hover 略透明

2. `src/components/query/QueryErrorPanel.tsx:44,46,52,58,71`
   - Change: `:44` → `rounded-md border border-danger/20 bg-danger/10 px-4 py-3`；`:46` → `text-danger`；`:52` 复制按钮 hover `hover:bg-red-500/10` → `hover:bg-danger/10`；`:58` 已复制 `Check` 的 `text-green-400` → `text-success`；`:71` 错误正文 `text-red-400` → `text-danger`
   - Preserve: 版式（标题 uppercase + 复制按钮 + mono 正文 + accent 操作组）、`data-testid="query-error-message"/"query-copy-error"`、`t('common.executionFailed')` 文案不变
   - Verify: 查询失败面板边框/底色/标题/正文与 `Badge danger` 同色系；复制成功态为 `success` 绿

3. `src/components/query/QueryExecutionStatus.tsx:45`
   - Change: 停止按钮 `text-red-300 hover:bg-red-500/10` → `text-danger hover:bg-danger/10`
   - Preserve: `Square` 图标、`disabled={!canCancel}`、`tid('editor-stop-button')`、title 的 `cancelUnavailable/cancelUnknown` 逻辑不变
   - Verify: 执行中停止按钮为 danger 色，disabled 态仍 `opacity-50`

4. `src/components/DataTable/DataTable.tsx:464`
   - Change: 删除行按钮 `text-red-400 hover:bg-surface-raised hover:text-red-300` → `text-danger hover:bg-surface-raised hover:text-danger`
   - Preserve: `Trash2` 图标、`data-testid="data-table-delete-rows"`、`title={t('dataTable.deleteRow')}`、选中条其余按钮不变
   - Verify: 选中条删除按钮与查询失败面板同红，hover 只变字色不换底色系

5. `src/windows/connection/query/QueryTransactionModals.tsx:188,392`
   - Change: `:188` 中止事务详情 `pre … text-red-400` → `text-danger`（其余 `border-edge bg-surface` 不动）；`:392` explain 错误 `CopyableError … border-red-500/20 bg-red-500/10 … text-red-400` → `border-danger/20 bg-danger/10 … text-danger`
   - Preserve: 两个 `Dialog` 的 title/description/footer（ghost + danger 按钮）不动；`txAbortedDetail` 换行/滚动不动
   - Verify: 中止事务弹窗与 explain 错误块同为 danger 语言

6. `src/windows/connection/result-workspace/ResultWorkspace.tsx:80`
   - Change: `CopyableError … className="max-w-2xl text-sm text-red-400"` → `className="max-w-2xl text-sm text-danger"`
   - Preserve: 居中容器、`tid('result-workspace-error')`、空态 `t('sqlFile.noResults')` 分支不动
   - Verify: 结果区错误文与 `QueryErrorPanel` 同色

7. `src/windows/connection/ObjectBrowser.tsx:216`
   - Change: `CopyableError … className="px-3 py-2 text-xs text-red-400"` → `className="px-3 py-2 text-xs text-danger"`
   - Preserve: loading/empty 行、对象列表选中态不动
   - Verify: 对象浏览器错误行同为 danger 色

8. `src/windows/main/MainPage.tsx:131`
   - Change: 加载失败 `<p … text-red-400>` → `text-danger`
   - Preserve: `select-text text-center text-sm`、`data-testid="welcome-load-error"`、重试按钮不动
   - Verify: 首屏加载失败与 `ConnectionPage:687` 连接失败同为 `text-danger`

9. `src/components/ui/CopyableError.tsx:59,65`
   - Change: `:59` 复制按钮 hover `hover:bg-red-500/10` → `hover:bg-danger/10`；`:65` 已复制 `Check … text-green-400` → `text-success`
   - Preserve: `selectable select-text`、mono/pre 分支、`data-testid="copyable-error-copy/message"`、无 className 时的裸返回分支不动
   - Verify: 所有经 `CopyableError` 渲染的错误（explain/对象树/ER/DDL 等）自动继承 danger 语言，复制成功态为 success

10. `src/components/ui/ResultMessageDialog.tsx:43,69,71`
    - Change: `:69` 错误图标 `text-red-400` → `text-danger`；`:43` 已复制 `Check … text-green-400` → `text-success`；`:71` 成功图标 `text-green-500` → `text-success`（同 token 对齐，不改变成功/错误区分）
    - Preserve: `Dialog max-w-sm` + footer（copy + primary OK）结构、`kind` 分支、`data-testid="result-message-copy/ok"` 不动
    - Verify: 结果弹窗错误图标与查询失败面板同红，成功图标与 `ExecutionSummaryCard` 同绿

11. 单测锁死值更新（与实现同提交）
    - Change: `packages/ui/src/__tests__/primitives.test.tsx:16` 的 `toContain('bg-red-500/90')` → `toContain('bg-danger')`；`src/components/ui/__tests__/Button.test.tsx:67` 同理
    - Preserve: 断言意图（danger 变体有危险底色）不变，只换 token 名
    - Verify: `npx vitest run packages/ui/src/__tests__/primitives.test.tsx src/components/ui/__tests__/Button.test.tsx` 通过

## Scope

- Inherit: 经 `CopyableError` 渲染的一切错误行（ER 图、DDL、索引、外键、权限、结构树等）自动继承，无需逐页改
- Verify: `ConfirmDialog`（amber 警告图标/badge）、`DetailPanel`、图表、导航树状态点（`bg-yellow-400/bg-green-500/bg-red-500` 连接点）、`--dt-*` 单元格类型色——确认未被本计划污染
- Exclude: `packages/ui/src/Label.tsx:12` 必填星号 `text-red-400`（必填语义与错误语义是否同 token 需另行裁决，本计划不碰）；`amber/yellow/green` 成功/警告独立语义（除上述三处复制成功图标外不碰）；`bg-black/50` 遮罩、`scrollbar #1c2331`、`text-white` 关闭按钮（`WindowControls`）等非错误红色；新 connection 对话框表单校验红（`StandardConnectionFields` 等）——建议作为后续同规则批量，不在本计划内

## Validation

- Product: 任一查询执行失败（如 `SELECT * FROM missing_table`），失败面板边框/标题/正文为主题 danger 色；浅/暗主题切换时错误红同步变化（light `#e05252` 系 / dark `#ff7b72` 系），不再是固定的 Tailwind red
- Interface: 覆盖 `MainPage` 加载失败分支、连接 workspace 错误/connecting 覆盖层、查询失败、explain 失败、中止事务弹窗、结果区错误、对象浏览器错误、`DataTable` 选中删除、执行中停止按钮；`400px` 窄栏与 `1080p` 下换行不断裂；复制按钮“已复制”态为 success 绿
- System: 确认无第二套错误红——`rg -n 'text-red-300|text-red-400|bg-red-500|border-red-500' src/components/query src/components/DataTable src/components/ui/CopyableError.tsx src/components/ui/ResultMessageDialog.tsx src/windows/connection/query/QueryTransactionModals.tsx src/windows/connection/result-workspace/ResultWorkspace.tsx src/windows/connection/ObjectBrowser.tsx src/windows/main/MainPage.tsx packages/ui/src/Button.tsx` → 无命中（测试文件除外）
- Repository: `npx vitest run packages/ui/src/__tests__/primitives.test.tsx src/components/ui/__tests__/Button.test.tsx` → 通过；`npx tsc --noEmit`（如仓库有该脚本则通过）→ 无新增类型错误

## Stop conditions

- Stop if 设计要求 `Button danger` 在暗色下保持 Tailwind red 而非 `--c-danger`（以站内文档或用户明确指示为准）——此时回退 Change 1，需用户裁决，其余 `text-danger` 收敛仍可继续
- Stop if 某处 `red-*` 经查实际表达“录制/断开/删除确认以外的语义”（如图表数据色）——停下并报告该处，不擅自换 token

## Design documentation

- After acceptance and validation: 无需更新设计文档。如仓库日后新增 `DESIGN.md` 或主题文档，可补记一句「错误/危险一律 `danger` token（`text-danger` / `bg-danger/10` / `border-danger/20`），`Button danger` 为 `bg-danger text-on-accent`；禁止 `red-*` 直写」，目的地为该新文件，否则为 none
