# v01x-filter-pagination 进度

## 1. 功能摘要

- 编号：`v01x-filter-pagination`（D 轨道）
- 范围：受控 FilterExpression parser、结构化条件适配、FilterEditor/FilterBar loading/error 契约、Pagination filter-reset reducer/callback
- 状态：编码完成
- 编码 commit：待本次提交
- 边界：未修改 `DataTable.tsx`、`ContentView.tsx`、`PanelContentRenderer.tsx`、`QueryPanel.tsx`、`panelStore.ts`、共享 locale、codegen 文件或 hub

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| D-E2E-001 | 在带 schema 列 allow-list 的表中输入 `status = 'paid' AND amount > 100`，只生成结构化条件并按绑定值查询 | 留待 R 回归 | 本轨未构建桌面 E2E |
| D-E2E-002 | 输入函数、子查询、注释、分号、未闭合字符串或未知列，显示解析错误且不发起查询 | 留待 R 回归 | parser 单测已覆盖；页面接线由 I 完成 |
| D-E2E-003 | 从非首屏应用 filter，页码回到 0；旧请求不覆盖新结果 | 留待 R 回归 | Pagination reset 契约已提供；request generation 由 I 接线 |
| D-E2E-004 | filter 请求 loading 时，FilterEditor/FilterBar/Pagination 展示 busy 并禁用变更控件 | 留待 R 回归 | 组件单测已覆盖；桌面旅程留待 R |

## 3. 测试结果

- `npx vitest run src/lib/__tests__/filterExpression.test.ts src/components/DataTable/__tests__/Pagination.test.tsx`：2 files、37 tests passed、0 failed。
- 组件定向 Vitest：在不写入 worktree 的临时 Vite resolver 中注入缺失 generated locale stub 后通过；2 files、19 tests passed、0 failed。worktree 缺失的 `src/locales/builtinLocales.ts` 是 bootstrap 生成文件，目标目录禁止写入，本轨未生成或提交。
- `npx tsc --noEmit`：受基线环境阻塞，2 个诊断：缺失 bootstrap 生成的 `src/locales/builtinLocales.ts`，以及由该缺失导致的既有 `SettingsContent.tsx` 参数隐式 any；未报告本轨修改文件诊断。
- `git diff --check`：通过。
- 覆盖重点：parser 覆盖合法 literal/operator/precedence/escaping、allow-list、注入和非法语法；Pagination 覆盖纯 reducer、revision reset、loading。
- Rust：本轨无 Rust 改动，不需要设置 `CARGO_TARGET_DIR=/private/tmp/datazen-v01x-filter-pagination` 或运行 Cargo。

## 4. 设计决策 / 遗留

- parser 使用 lexer + 递归下降 grammar：`primary → AND → OR`，只接受有限列名、`= != > >= < <=`、`IS NULL`/`IS NOT NULL`、字符串/数字/布尔/NULL 和括号。
- AST 不携带可执行 SQL 文本；`filterExpressionToConditions` 仅适配现有 `FilterCondition[]`，混合 AND/OR 的完整语义由调用方保留 AST，不可将 raw input 拼接为 SQL。
- `FilterEditor` 保留既有高级 Column/Operator/Value 入口；新增 loading/error 是 caller-owned 契约，组件不直接调用 Store/IPC。
- `Pagination` 通过 `paginationReducer`、`resetPageOnFilterChange` 和 `filterRevision/onPageReset` 提供 page=0 契约；请求取消/过期响应丢弃由 I 轨接线，非本轨 Store 责任。
- Context Menu 分层和页面最终接线按任务要求留给 I；本轨仅记录 E2E 旅程留待 R。
