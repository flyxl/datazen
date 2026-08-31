# v01x-object-actions 进度

## 1. 功能摘要

- 编号：v01x-object-actions
- 范围：已加载 Schema 对象的纯搜索/分组/类型过滤，以及 Table/SQL action 与 QueryOpenContext 契约
- 状态：已完成；共享页面最终接线留给 I 轨，桌面 E2E 留待 R 回归
- 编码 commit：c6da58b6
- 测试 commit：本提交（独立复测轮 f2）
- 修复 commit：df1c0ad9（BUG-001 修复轮）

## 2. E2E 用例登记

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| OA-E2E-001 | 多连接、多 database/schema 的全局对象搜索按连接、database、schema、对象类型分组 | 留待 R 回归 | 本轨不接共享页面，当前未执行桌面 E2E |
| OA-E2E-002 | 搜索 column 命中后保留所属 table、connection、database、schema，并由 I 轨打开/高亮父表 | 留待 R 回归 | 领域纯函数已覆盖，桌面路径留待 R |
| OA-E2E-003 | Table action 菜单显示 Open Data、SELECT、INSERT、UPDATE、DDL，QueryOpenContext 不丢上下文 | 留待 R 回归 | 当前无页面接线，留待 R |
| OA-E2E-004 | 不同驱动通过既有 metadata/driver builder 生成正确 identifier quoting，action 不携带凭据且不执行 SQL | 留待 R 回归 | 驱动方言专属验证留待对应 driver crate / R |

## 3. 测试结果

- 独立定向 Vitest（修复轮）：`pnpm exec vitest run src/lib/__tests__/schemaObjectSearch.test.ts src/lib/__tests__/tableSqlActions.test.ts`，2 个文件 / 16 个测试通过（含 3 个命中字段/原因回归测试）。
- 独立定向 Vitest（复测轮 f2）：同上，2 个文件 / 16 个测试通过。
- 聚焦覆盖率（报告目录 `/private/tmp/datazen-v01x-object-actions-coverage-f2`）：2 个文件 / 16 个测试通过；合计 Statements 93.52%（159/170）、Branches 82.84%（140/169）、Functions 97.61%（41/42）、Lines 97.24%（141/145）。`schemaObjectSearch.ts`：Lines 96.46%；`tableSqlActions.ts`：Lines 100%。
- 前一轮独立定向 Vitest：`pnpm exec vitest run src/lib/__tests__/schemaObjectSearch.test.ts src/lib/__tests__/tableSqlActions.test.ts`，2 个文件 / 13 个测试通过。
- 相关 Vitest：加入 `src/lib/__tests__/databaseTypes.test.ts` 后，2 个文件 / 13 个测试通过；`databaseTypes.test.ts` 套件 0 个测试并在模块解析阶段失败，原因是 worktree 基线缺失 ignored `src/locales/builtinLocales.ts`，不是本轨断言失败。
- 聚焦覆盖率（报告目录 `/private/tmp/datazen-v01x-object-actions-coverage`）：2 个文件 / 13 个测试通过；合计 Statements 93.16%（150/161）、Branches 81.76%（130/159）、Functions 97.50%（39/40）、Lines 97.08%（133/137）。`schemaObjectSearch.ts`：Lines 96.19%；`tableSqlActions.ts`：Lines 100%。
- `pnpm typecheck`：失败（exit 2），仅报告既有基线问题：缺失 ignored `src/locales/builtinLocales.ts`，以及 `src/windows/settings/SettingsContent.tsx:66` 的隐式 `any`；未报告本轨新增源码类型错误。
- `git diff --check`：工作树 diff 和 `d0865942..HEAD` 完整 feature diff 均通过；本轮未触碰共享 hub，保留其预先存在的修改。
- 静态安全审查：`schemaObjectSearch.ts` / `tableSqlActions.ts` 无 IPC、command 或 SQL 执行调用；`TableContext` 未携带凭据，额外 `password` 不会进入 action/context；identifier 逐段复用 `escapeIdent`。
- 纯逻辑覆盖范围：大小写/空搜索、多连接多 schema 分组、column→table、function/procedure→routine、对象类型过滤、五项 action、完整 QueryOpenContext、identifier quoting、凭据隔离和无执行路径。
- 验收结论：独立复测通过，BUG-001 状态更新为“已修复”；未发现新业务 bug。

## 4. 设计决策 / 遗留注意

- `SchemaObjectIndexEntry` 复用现有 `TableInfo`、`DatabaseObject` 和 `schemaStore.columnMap` 形状；只搜索调用方提供的已加载 slice，不触发 IPC、不伪造未加载对象。
- `ObjectSearchResult` 保留 `connectionId`、`dbSessionId`、database、schema、object name；column 结果额外保留 `tableName`。procedure 在搜索契约中归一为 `routine`，并保留 `sourceKind`。
- `buildTableContext` 接受现有 TablePanel/ViewPanel/search-result 的结构字段；`database` 作为 QueryPanel 选择上下文保留，不猜测并拼进 SQL 资格名。
- `quoteTableIdentifier` 逐段调用现有 `escapeIdent` metadata API；SELECT 只提供安全草稿，INSERT/UPDATE/DDL 只提供带 driver-generated 占位注释的 draft-only 模板，后续由 driver/已有 command 生成方言，不执行 SQL。
- `searchSchemaObjects` 为每个结果保留 `matchedFields`（name/host/database/type/schema/object/table/column）和首要 `matchReason`；column 名称命中与所属 table 命中分别标识，空搜索返回空命中字段且不改变结果集合。
- 未修改 `ConnectionPage.tsx`、`ContentView.tsx`、`PanelContentRenderer.tsx`、`panelStore.ts`、`QueryPanel.tsx`、locales、hub、未跟踪规格文档或 codegen 文件。
