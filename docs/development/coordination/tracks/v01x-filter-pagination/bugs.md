# v01x-filter-pagination Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| `v01x-filter-pagination-BUG-001` | **S3**：`FilterEditor` busy 时，已完成条件 chip 的删除入口仍可触发 `onRemove`，loading/busy 禁止过滤变更契约不完整 | 已验证 | 见下方详细步骤 | 2026-08-31 独立复验：loading 下 chip 编辑/删除均不可用，idle 删除可用 |
| `v01x-filter-pagination-BUG-002` | **S3**：`DataTable` 收到 `loading=true` 后未向 `FilterEditor`、`FilterBar`、`Pagination` 传递 loading，实际表格路径仍可在请求中修改过滤条件或分页 | 已验证 | 见下方详细步骤 | 2026-08-31 本修复轮 DataTable 回归：FilterEditor/FilterBar/Pagination 均收到 `loading=true`，loading 控件不可变更 |
| `v01x-filter-pagination-BUG-003` | **S3**：同一表的旧分页请求完成后无取消/代数校验，会覆盖新 Apply 的过滤状态；新过滤请求被 `existing.loading` 直接跳过 | 已验证 | 见下方详细步骤 | 2026-08-31 本修复轮 Store 回归：新 page 0 + filters 请求正常发起，旧 page 2 响应晚到被忽略 |
| `v01x-filter-pagination-BUG-004` | **S3**：DataTable 右键菜单未按 v0.1x 验收要求分层，低频复制格式、NULL 和批量操作仍全部平铺在一级菜单，没有二级 submenu | 已验证 | 见下方详细步骤 | 2026-08-31 修复：高频动作留在一级，格式化/批量复制归入 `more-actions` submenu；定向与相关 Vitest 通过 |

## 环境记录

- worktree 缺少 gitignore 的 `src/locales/builtinLocales.ts`；该目录由沙箱拒绝写入。组件测试通过临时 resolver 使用外部只读 stub 验证，未修改 locale/codegen，也未将环境问题登记为业务 bug。
- 本轨无 Rust 改动；Host 桌面 E2E 按约定留待 R。
- 独立复验日期：2026-08-31（Asia/Shanghai）。默认配置定向 Vitest 受上述缺失生成文件阻塞；使用 `/private/tmp` 临时 resolver 后，parser、FilterBar、FilterEditor、Pagination、tableDataStore 共 5 文件 90/90 通过；菜单层级相关 5 文件 34/34 通过。
- 独立 `/private/tmp` 探针确认 BUG-002、BUG-003；探针文件与 resolver 均未写入本 worktree。
- `pnpm typecheck` exit 2：仅有缺失 `src/locales/builtinLocales.ts`，以及既有 `src/windows/settings/SettingsContent.tsx(66,31)` 的隐式 `any` 基线错误；未报告本轨修改文件错误。
- 本修复轮先执行 `node scripts/generate-builtin-locales.mjs`，再运行默认配置定向 Vitest 6 files / 103 tests 和 `pnpm typecheck`，均通过；生成物保持 ignored，不纳入提交。
- `git diff --check` 与 `d8e9c59b..31929367` 提交范围的 diff check 均通过。
- 2026-08-31 独立复测：生成 builtin locales 后，定向 Vitest 7 files / 116 tests、DataTable + tableDataStore 13 files / 122 tests、TableView/NlFilterInput 2 files / 8 tests 均通过；`pnpm typecheck` 与 `git diff --check d8e9c59b^..fe6fb7fe` 均通过。发现 BUG-004；未修改功能代码，生成物 `src/locales/builtinLocales.ts` 保持 ignored。
- 2026-08-31 BUG-004 修复复验：生成 builtin locales 后，定向菜单/DataTable/WebContextMenu/native menu 5 files / 43 tests、相关前端回归 115 files / 900 tests、`pnpm typecheck` 均通过；`git diff --check` 通过。Prettier 未安装，无法运行 `pnpm exec prettier --check`。

## v01x-filter-pagination-BUG-001

- 严重等级：S3
- 状态：已验证
- 发现时间：2026-08-31 08:17（Asia/Shanghai）
- 关联文件：`src/components/FilterEditor.tsx` 的 `FilterConditionChip`（约 164-196 行）

### 重现步骤

1. 渲染 `FilterEditor`，传入一个完整条件，例如 `name = 'alice'`，并设置 `loading={true}`。
2. 保持编辑器展开，点击完成条件 chip 的删除按钮（`filter.remove`）。
3. 观察 caller 的 `onRemove` 回调。

### 预期结果

loading/busy 期间所有过滤条件变更入口均不可操作，`onRemove` 不应被调用。

### 实际结果

完成条件以 `FilterConditionChip` 渲染，其删除按钮没有 `disabled={loading}`；独立探针实测 `onRemove(0)` 被调用一次。展开后的 Select、Value、Apply 等控件虽被禁用，但 chip 删除绕过了 busy 保护。

### 验证记录

| 日期 | 验证人 | 方法 | 结果 |
|---|---|---|---|
| 2026-08-31 | 独立测试子代理 | 临时 Vitest + React Testing Library，`loading=true` 点击完成条件 chip 删除 | 复现；待修复代理处理后重新验证 |
| 2026-08-31 | 修复代理 | 定向 Vitest + React Testing Library，覆盖 `loading=true` 的完成条件 chip 编辑/删除和 idle 删除 | 通过；FilterEditor/FilterBar 21/21，原始 busy chip 探针 1/1 |
| 2026-08-31 | 独立测试代理 | 定向 Vitest（FilterEditor）验证 `loading=true` chip 编辑/删除，以及 idle 删除 | 通过；完成条件 chip 编辑、删除均保持 disabled，idle 删除调用 `onRemove(0)` |

### 修复记录

- `FilterConditionChip` 接入 `loading`，编辑和删除按钮均使用原生 `disabled` 与显式 `aria-disabled`；busy 时不会触发 `onRemove` 或进入编辑态。
- `FilterBar` 的容器、条件删除和清除入口统一暴露 busy/disabled 语义；非 loading 删除路径保持可用。

## v01x-filter-pagination-BUG-002

- 严重等级：S3
- 状态：已验证
- 发现时间：2026-08-31（Asia/Shanghai）
- 关联文件：`src/components/DataTable/DataTable.tsx`（`FilterEditor` 约 375-389 行、`FilterBar` 约 390-392 行、`Pagination` 约 478-484 行）；`src/windows/connection/TableView.tsx` 已在约 267 行把 loading 传给 `DataTable`

### 重现步骤

1. 打开已有数据的表，使表格存在 applied filter 和分页控件。
2. 触发一次会使 `TableView` 的 `loading` 变为 `true` 的表数据请求。
3. 在请求完成前，观察 FilterEditor（展开路径）、FilterBar（折叠路径）和 Pagination 的子控件。
4. 尝试点击完成条件 chip 的编辑/删除、清除过滤条件、上一页/下一页或页大小。

### 预期结果

请求进行中三个子组件都应收到 `loading=true`；过滤条件编辑/删除/清除和分页变更入口应 disabled，避免用户状态变更与当前请求错位。

### 实际结果

`TableView` 虽将 `loading` 传给 `DataTable`，但 `DataTable` 渲染三个子组件时均未传 `loading`。独立 React 探针将 `DataTable loading=true` 渲染为：`FilterEditor=false`、`Pagination=false`、折叠 `FilterBar=false`。因此组件内部默认 `loading=false`，请求期间这些入口仍可操作。

### 修复与验证

- `DataTable` 将实际 `loading` 透传给 `FilterEditor`、`FilterBar`、`Pagination`；`TableView` 顶部手动过滤入口也在 loading 时禁用。
- loading 时按值过滤菜单动作不再可用，避免上下文菜单绕过 busy 保护；idle 菜单分层与动作顺序不变。
- 定向 RTL 回归通过：`DataTable.test.tsx` 覆盖三类子组件收到 busy 状态；本修复轮 6 files / 103 tests 全部通过。

## v01x-filter-pagination-BUG-003

- 严重等级：S3
- 状态：已验证
- 发现时间：2026-08-31（Asia/Shanghai）
- 关联文件：`src/stores/tableDataStore.ts`（约 622 行 `existing.loading` 短路、约 671-718 行无 request generation 校验）；`applyFilters` 约 799-814 行

### 重现步骤

1. 首次加载 `users` 表到 page 0。
2. 调用分页到 page 2，并让该 `getTableData` 请求保持 pending。
3. 在旧 page 2 请求 pending 时，添加 `id = 99` 并执行 Apply。
4. 等待旧 page 2 请求返回旧数据。

### 预期结果

Apply 应使当前页回到 0；旧请求应被取消，或其返回结果因 request generation 过期而被忽略；随后应发起带新过滤条件的 page 0 请求，旧结果不得覆盖新过滤状态。

### 实际结果

`applyFilters` 虽先把 page 设为 0 并更新 filters，但 `reloadActive` 调用 `loadTableData` 时命中 `existing.loading` 直接返回，未发起新请求。旧请求返回后仍按其捕获的 page 2/旧 filters 结果写回状态。独立 Store 探针实测 `getTableData` 调用次数为 2（预期 3），未出现 page 0 + `[{ column: 'id', operator: 'eq', value: 99 }]` 的新请求，最终页码被旧响应写回 2。

### 修复与验证

- 每个 `TableState` 维护单调递增的 `requestRevision` 和当前请求的 `loadingRevision`；页面、页大小、过滤条件和排序变更都会推进 revision。
- 相同 revision 的并发加载仍去重；revision 变化时允许新请求替换旧请求，响应和错误只有在 revision 仍匹配时才写回。
- 定向 Store 回归通过：旧 page 2 请求晚于新过滤 page 0 请求返回时，旧 rows/page 不会覆盖新状态；新过滤结果最终正确落盘。

## v01x-filter-pagination-BUG-004

- 严重等级：S3
- 状态：已验证
- 发现时间：2026-08-31（Asia/Shanghai）
- 关联文件：`src/lib/dataTableContextMenu.ts`（`buildDataTableContextMenuItems` 约 222-268 行）；调用路径为 `src/components/DataTable/DataTable.tsx` 的 `handleContextMenu`

### 重现步骤

1. 打开带数据的 DataTable，在任意单元格上打开右键菜单。
2. 观察菜单项及其层级。
3. 用 `buildDataTableContextMenuItems` 的 cell-context 入参构建菜单，或直接在 DataTable 测试中检查 `showNativeContextMenu` 的第一参数。

### 预期结果

按照 v0.1x PRD/实施验收，一级菜单应收敛到 Copy、Edit、Filter、Export、Delete Mark 等高频入口；Copy as JSON、SQL INSERT、SQL UPDATE、CSV、Set NULL、Copy Selected Rows 等低频/批量操作应放入二级 submenu。

### 实际结果

`buildDataTableContextMenuItems` 仅返回 `kind: 'item'` 和 `kind: 'separator'`，没有任何 `kind: 'submenu'`。JSON、INSERT、UPDATE、CSV、NULL、Copy Selected Rows 等操作均直接平铺在一级菜单；现有测试也把该平铺结构作为预期（`src/lib/__tests__/dataTableContextMenu.test.ts` 约 130-142 行、`src/components/DataTable/__tests__/DataTable.test.tsx` 约 221-233 行）。分隔线只能分组，不能满足“二级菜单”验收。

### 验证记录

| 日期 | 验证人 | 方法 | 结果 |
|---|---|---|---|
| 2026-08-31 | 独立复测代理 | 源码审查 + `dataTableContextMenu`/`DataTable` 定向 Vitest | 发现：菜单无 submenu；相关测试均通过但验证的是错误的平铺契约 |
| 2026-08-31 | 修复代理 | `dataTableContextMenu`、`DataTable`、`WebContextMenu` 相关 Vitest + 全部相关前端回归 | 通过：根菜单高频动作与 submenu 结构、focus/Escape、禁用 item、loading 过滤保护均符合预期 |

### 修复与验证

- DataTable builder 复用既有 `NativeMenuItemDef.kind = 'submenu'` API，根菜单保留 Copy、Copy Row、Filter、Set NULL、Delete Row、Export；Copy as JSON/SQL INSERT/UPDATE/CSV、Copy Column Name、Copy Selected Rows 归入 `more-actions` 二级菜单。
- 无 cell 命中时仍保留选中行 Delete/Export 的根入口，选中行复制与 CSV 进入同一 submenu；没有可用低频 action 时不生成空 submenu。
- 保留既有 action/handler 可用性与 loading 语义：`loading` 时 DataTable 不生成 Filter by This Value；WebContextMenu 禁用 item 仍保持 inert，submenu 可通过鼠标 hover 或键盘 focus 打开，Escape 关闭。
- 新增 builder 结构/动作测试、DataTable 真实右键结构/loading 测试，以及 WebContextMenu 禁用态/键盘 focus 测试；未增加任何驱动分支。
