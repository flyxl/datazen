# v01x-filter-pagination 进度

## 1. 功能摘要

- 编号：`v01x-filter-pagination`（D 轨道）
- 范围：受控 FilterExpression parser、结构化条件适配、FilterEditor/FilterBar loading/error 契约、Pagination filter-reset reducer/callback
- 状态：本轨通过（BUG-001/002/003/004/005 均已验证，桌面 E2E 留待 R）
- 编码 commit：`d8e9c59b983ec3d7dc56163e0fa304d2f6b66b20`
- 后续修复 commit：`31929367`、`fe6fb7fe`、`76910e10`、`a9d887e8`
- 既有测试 commit：`db18a4b4`、`3f7e01f7`、`8dfc8b0b`、`04f78060`
- 最终复测 commit：本次提交（`test(ipc): f5 filter pagination verification`）
- 边界：本轨补充 Filter/DataTable loading 接线与过滤入口保护、`tableDataStore.ts` 请求 revision，以及 DataTable context menu 分层与测试；仅新增 en/zh-CN 菜单文案，未修改 `ContentView.tsx`、`PanelContentRenderer.tsx`、`QueryPanel.tsx`、`panelStore.ts`、codegen 文件或 hub

## 2. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| D-E2E-001 | 在带 schema 列 allow-list 的表中输入 `status = 'paid' AND amount > 100`，只生成结构化条件并按绑定值查询 | 留待 R 回归 | parser 单测通过；页面/Store 接线由 I，桌面 E2E 留待 R |
| D-E2E-002 | 输入函数、子查询、注释、分号、未闭合字符串或未知列，显示解析错误且不发起查询 | 留待 R 回归 | parser 单测通过；页面接线由 I，桌面 E2E 留待 R |
| D-E2E-003 | 从非首屏应用 filter，页码回到 0；旧请求不覆盖新结果 | 留待 R 回归 | Pagination reset/filterRevision 契约单测通过；tableDataStore request revision 与旧响应丢弃回归通过 |
| D-E2E-004 | filter 请求 loading 时，FilterEditor/FilterBar/Pagination 展示 busy 并禁用变更控件 | 留待 R 回归 | DataTable loading 接线回归通过；FilterBar/Pagination 与 FilterEditor 控件单测通过；桌面 E2E 留待 R |

## 3. 测试结果

- 独立定向 Vitest（默认配置首轮）：parser/Pagination 2 files、37 tests passed；FilterBar/FilterEditor 与 tableDataStore 受缺失 bootstrap 生成的 `src/locales/builtinLocales.ts` 阻塞，相关 32 个 store tests 未能执行成功。
- 独立定向 Vitest（`/private/tmp` 临时内存 locale resolver，不写 worktree）：5 files、88 tests passed、0 failed（parser 28、FilterBar 1、FilterEditor 18、Pagination 9、tableDataStore 32）。
- loading 边界临时探针：1 case 复现 `v01x-filter-pagination-BUG-001`，busy 完成条件 chip 的 `onRemove(0)` 被调用；该探针失败是为确认缺陷，未修改源代码。
- 修复轮定向 Vitest（`/private/tmp` 临时 resolver，不写 worktree）：5 files、90 tests passed、0 failed（parser 28、FilterBar 2、FilterEditor 19、Pagination 9、tableDataStore 32）；原始 busy chip 探针 1/1 通过。
- 修复轮定向 coverage（报告输出到 `/private/tmp`）：5 files、90 tests passed、0 failed；行覆盖率：`filterExpression.ts` 95.00%、`FilterBar.tsx` 100.00%、`FilterEditor.tsx` 88.98%、`Pagination.tsx` 96.66%、`tableDataStore.ts` 85.95%。
- BUG-004 修复轮定向 Vitest：菜单/DataTable/WebContextMenu/native menu 5 files、43 tests passed、0 failed；相关前端回归 115 files、900 tests passed、0 failed。
- 本修复轮先执行 `node scripts/generate-builtin-locales.mjs` 再运行默认配置定向 Vitest：6 files、103 tests passed、0 failed（新增 DataTable 接线 2、tableDataStore 竞态 1）；生成物随后删除。
- `node scripts/generate-builtin-locales.mjs` 后 `pnpm typecheck` exit 0；生成的 gitignored `src/locales/builtinLocales.ts` 已删除，未纳入提交。
- `git diff --check`：通过；未改动 parser allow-list、Apply payload 或 page reset=0；菜单层级按既有 Web Context Menu submenu API 实现。
- 2026-08-31 最终独立复测：先生成 ignored `src/locales/builtinLocales.ts`；相关定向 Vitest 10 files / 135 tests passed，完整 Host Vitest 265 files / 2178 tests passed；`pnpm typecheck` exit 0；当前 worktree 与 `d8e9c59b^..76910e10` 的 `git diff --check` 均通过。
- 最终复测结论：发现 `v01x-filter-pagination-BUG-005`（`Set NULL` 仍在 DataTable context menu 一级，未完全满足 PRD 的二级分层要求）；仅记录，未修改功能代码。
- BUG-005 修复轮：`Set NULL` 已移入 `more-actions` submenu；builder 与 DataTable 断言 root 不含 `set-null`、submenu 含 `set-null`，并验证原 handler 仍可调用。
- BUG-005 修复轮验证：生成并随后清理 ignored builtin locale 后，定向 Vitest 2 files / 27 tests、相关菜单回归 6 files / 51 tests、`pnpm typecheck`、`git diff --check` 均通过；未改动 `hub.md`。
- 2026-08-31 本次最终独立复测覆盖完整提交链 `d8e9c59b`、`31929367`、`fe6fb7fe`、`76910e10`、`a9d887e8` 与 `04f78060`：先成功生成并在测试后清理 ignored `src/locales/builtinLocales.ts`；定向/相关 Vitest 12 files、146 tests 全部通过，完整 Host Vitest 265 files、2178 tests 全部通过，`pnpm typecheck` exit 0，当前与提交范围 `git diff --check` 均通过。
- 本次审查确认 loading 透传、request revision 防旧响应、parser/Apply 结构化 payload、page reset、chip busy，以及 DataTable 菜单 root/submenu、键盘 focus/Escape 和 handler 均符合验收；未发现新的业务 bug，BUG-001~005 均已验证，本轨通过。除既有 `hub.md` 外未产生其他工作区修改。
- 覆盖重点：parser 覆盖合法 literal/operator/precedence/escaping、allow-list、函数/子查询/注释/分号/未知列/非法语法拒绝；Pagination 覆盖纯 reducer、revision reset、loading；FilterEditor/Bar 覆盖 error/busy 契约。
- Rust：本轨无 Rust 改动，不需要设置 `CARGO_TARGET_DIR=/private/tmp/datazen-v01x-filter-pagination` 或运行 Cargo。

## 4. 设计决策 / 遗留

- parser 使用 lexer + 递归下降 grammar：`primary → AND → OR`，只接受有限列名、`= != > >= < <=`、`IS NULL`/`IS NOT NULL`、字符串/数字/布尔/NULL 和括号。
- AST 不携带可执行 SQL 文本；`filterExpressionToConditions` 仅适配现有 `FilterCondition[]`，混合 AND/OR 的完整语义由调用方保留 AST，不可将 raw input 拼接为 SQL。
- `FilterEditor` 保留既有高级 Column/Operator/Value 入口；新增 loading/error 是 caller-owned 契约，组件不直接调用 Store/IPC。
- `Pagination` 通过 `paginationReducer`、`resetPageOnFilterChange` 和 `filterRevision/onPageReset` 提供 page=0 契约；`tableDataStore` 以 per-table request/loading revision 丢弃过期响应和错误。
- Context Menu 根菜单保留 Copy、Copy Row、Filter、Delete、Export 等高频入口；Set NULL、JSON/INSERT/UPDATE/CSV、列名和选中行复制归入 `more-actions` submenu。仅在 loading 时隐藏按值过滤动作，避免上下文菜单绕过 busy 保护；页面既有 `TableView → DataTable` loading 接线保留，并由本轨补齐三类子组件接线。
- Bug 修复轮：`FilterConditionChip` 在 `loading` 时禁用编辑/删除入口，`FilterBar` 与 `FilterEditor` 容器和条件变更入口统一暴露 `aria-busy`/`aria-disabled`；`DataTable` 透传真实 loading，回归测试通过。
