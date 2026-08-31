# v01x-filter-pagination Bug 清单

| Bug ID | 描述 | 状态 | 重现步骤 | 验证记录 |
|---|---|---|---|---|
| `v01x-filter-pagination-BUG-001` | **S3**：`FilterEditor` busy 时，已完成条件 chip 的删除入口仍可触发 `onRemove`，loading/busy 禁止过滤变更契约不完整 | 已验证 | 见下方详细步骤 | 2026-08-31 独立复验：loading 下 chip 编辑/删除均不可用，idle 删除可用 |
| `v01x-filter-pagination-BUG-002` | **S3**：`DataTable` 收到 `loading=true` 后未向 `FilterEditor`、`FilterBar`、`Pagination` 传递 loading，实际表格路径仍可在请求中修改过滤条件或分页 | 待修复 | 见下方详细步骤 | 2026-08-31 独立接线探针失败：三个子组件均收到 `loading=false` |
| `v01x-filter-pagination-BUG-003` | **S3**：同一表的旧分页请求完成后无取消/代数校验，会覆盖新 Apply 的过滤状态；新过滤请求被 `existing.loading` 直接跳过 | 待修复 | 见下方详细步骤 | 2026-08-31 独立 Store 竞态探针失败：旧请求完成后调用次数仍为 2，未产生 page 0 + 新 filters 请求 |

## 环境记录

- worktree 缺少 gitignore 的 `src/locales/builtinLocales.ts`；该目录由沙箱拒绝写入。组件测试通过临时 resolver 使用外部只读 stub 验证，未修改 locale/codegen，也未将环境问题登记为业务 bug。
- 本轨无 Rust 改动；Host 桌面 E2E 按约定留待 R。
- 独立复验日期：2026-08-31（Asia/Shanghai）。默认配置定向 Vitest 受上述缺失生成文件阻塞；使用 `/private/tmp` 临时 resolver 后，parser、FilterBar、FilterEditor、Pagination、tableDataStore 共 5 文件 90/90 通过；菜单层级相关 5 文件 34/34 通过。
- 独立 `/private/tmp` 探针确认 BUG-002、BUG-003；探针文件与 resolver 均未写入本 worktree。
- `pnpm typecheck` exit 2：仅有缺失 `src/locales/builtinLocales.ts`，以及既有 `src/windows/settings/SettingsContent.tsx(66,31)` 的隐式 `any` 基线错误；未报告本轨修改文件错误。
- `git diff --check` 与 `d8e9c59b..31929367` 提交范围的 diff check 均通过。

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
- 状态：待修复
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

## v01x-filter-pagination-BUG-003

- 严重等级：S3
- 状态：待修复
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
