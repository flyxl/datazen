# v01x-filter-pagination 进度

## 1. 功能摘要

- 编号：`v01x-filter-pagination`（D 轨道）
- 范围：受控 FilterExpression parser、结构化条件适配、FilterEditor/FilterBar loading/error 契约、Pagination filter-reset reducer/callback
- 状态：修复完成，待独立复验（`v01x-filter-pagination-BUG-001`）
- 编码 commit：`d8e9c59b983ec3d7dc56163e0fa304d2f6b66b20`
- 测试 commit：`db18a4b4`
- 修复 commit：本次提交（最终 hash 见提交记录）
- 边界：未修改 `DataTable.tsx`、`ContentView.tsx`、`PanelContentRenderer.tsx`、`QueryPanel.tsx`、`panelStore.ts`、共享 locale、codegen 文件或 hub

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| D-E2E-001 | 在带 schema 列 allow-list 的表中输入 `status = 'paid' AND amount > 100`，只生成结构化条件并按绑定值查询 | 留待 R 回归 | parser 单测通过；页面/Store 接线由 I，桌面 E2E 留待 R |
| D-E2E-002 | 输入函数、子查询、注释、分号、未闭合字符串或未知列，显示解析错误且不发起查询 | 留待 R 回归 | parser 单测通过；页面接线由 I，桌面 E2E 留待 R |
| D-E2E-003 | 从非首屏应用 filter，页码回到 0；旧请求不覆盖新结果 | 留待 R 回归 | Pagination reset/filterRevision 契约单测通过；request generation 由 I 接线 |
| D-E2E-004 | filter 请求 loading 时，FilterEditor/FilterBar/Pagination 展示 busy 并禁用变更控件 | 留待 R 回归 | FilterBar/Pagination 与修复轮组件自测通过；独立测试代理复验及桌面 E2E 留待 R |

## 3. 测试结果

- 独立定向 Vitest（默认配置首轮）：parser/Pagination 2 files、37 tests passed；FilterBar/FilterEditor 与 tableDataStore 受缺失 bootstrap 生成的 `src/locales/builtinLocales.ts` 阻塞，相关 32 个 store tests 未能执行成功。
- 独立定向 Vitest（`/private/tmp` 临时内存 locale resolver，不写 worktree）：5 files、88 tests passed、0 failed（parser 28、FilterBar 1、FilterEditor 18、Pagination 9、tableDataStore 32）。
- loading 边界临时探针：1 case 复现 `v01x-filter-pagination-BUG-001`，busy 完成条件 chip 的 `onRemove(0)` 被调用；该探针失败是为确认缺陷，未修改源代码。
- 修复轮定向 Vitest（`/private/tmp` 临时 resolver，不写 worktree）：5 files、90 tests passed、0 failed（parser 28、FilterBar 2、FilterEditor 19、Pagination 9、tableDataStore 32）；原始 busy chip 探针 1/1 通过。
- 修复轮定向 coverage（报告输出到 `/private/tmp`）：5 files、90 tests passed、0 failed；行覆盖率：`filterExpression.ts` 95.00%、`FilterBar.tsx` 100.00%、`FilterEditor.tsx` 88.98%、`Pagination.tsx` 96.66%、`tableDataStore.ts` 85.95%。
- `pnpm typecheck`：基线环境阻塞，exit 2；2 个诊断为缺失 `src/locales/builtinLocales.ts`，以及由此引出的既有 `src/windows/settings/SettingsContent.tsx(66,31)` 参数 `code` 隐式 any；未报告本轨修改文件诊断。
- `git diff --check`：通过。
- 覆盖重点：parser 覆盖合法 literal/operator/precedence/escaping、allow-list、函数/子查询/注释/分号/未知列/非法语法拒绝；Pagination 覆盖纯 reducer、revision reset、loading；FilterEditor/Bar 覆盖 error/busy 契约。
- Rust：本轨无 Rust 改动，不需要设置 `CARGO_TARGET_DIR=/private/tmp/datazen-v01x-filter-pagination` 或运行 Cargo。

## 4. 设计决策 / 遗留

- parser 使用 lexer + 递归下降 grammar：`primary → AND → OR`，只接受有限列名、`= != > >= < <=`、`IS NULL`/`IS NOT NULL`、字符串/数字/布尔/NULL 和括号。
- AST 不携带可执行 SQL 文本；`filterExpressionToConditions` 仅适配现有 `FilterCondition[]`，混合 AND/OR 的完整语义由调用方保留 AST，不可将 raw input 拼接为 SQL。
- `FilterEditor` 保留既有高级 Column/Operator/Value 入口；新增 loading/error 是 caller-owned 契约，组件不直接调用 Store/IPC。
- `Pagination` 通过 `paginationReducer`、`resetPageOnFilterChange` 和 `filterRevision/onPageReset` 提供 page=0 契约；请求取消/过期响应丢弃由 I 轨接线，非本轨 Store 责任。
- Context Menu 分层和页面最终接线未出现在本编码 commit，按轨道边界留给 I；本轨仅记录 E2E 旅程留待 R。
- Bug 修复轮：`FilterConditionChip` 在 `loading` 时禁用编辑/删除入口，`FilterBar` 与 `FilterEditor` 容器和条件变更入口统一暴露 `aria-busy`/`aria-disabled`；状态推进为待独立测试代理复验。
