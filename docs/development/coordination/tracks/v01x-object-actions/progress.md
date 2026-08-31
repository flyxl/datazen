# v01x-object-actions 进度

## 1. 功能摘要

- 编号：v01x-object-actions
- 范围：已加载 Schema 对象的纯搜索/分组/类型过滤，以及 Table/SQL action 与 QueryOpenContext 契约
- 状态：编码完成；共享页面最终接线留给 I 轨，桌面 E2E 留待 R 回归
- 编码 commit：待提交
- 测试 commit：本编码轮未拆分独立测试代理；测试结果记录如下

## 2. E2E 用例登记

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| OA-E2E-001 | 多连接、多 database/schema 的全局对象搜索按连接、database、schema、对象类型分组 | 留待 R 回归 | 本轨不接共享页面，当前未执行桌面 E2E |
| OA-E2E-002 | 搜索 column 命中后保留所属 table、connection、database、schema，并由 I 轨打开/高亮父表 | 留待 R 回归 | 领域纯函数已覆盖，桌面路径留待 R |
| OA-E2E-003 | Table action 菜单显示 Open Data、SELECT、INSERT、UPDATE、DDL，QueryOpenContext 不丢上下文 | 留待 R 回归 | 当前无页面接线，留待 R |
| OA-E2E-004 | 不同驱动通过既有 metadata/driver builder 生成正确 identifier quoting，action 不携带凭据且不执行 SQL | 留待 R 回归 | 驱动方言专属验证留待对应 driver crate / R |

## 3. 测试结果

- `pnpm exec vitest run src/lib/__tests__/schemaObjectSearch.test.ts src/lib/__tests__/tableSqlActions.test.ts`：2 个文件、13 个测试通过。
- `git diff --check`：通过。
- `pnpm exec tsc --noEmit`：仍受 worktree 基线阻塞；本轨类型错误已清零，剩余为缺失 ignored `src/locales/builtinLocales.ts` 与既有 `src/windows/settings/SettingsContent.tsx:66` 隐式 `any`。
- 纯逻辑覆盖范围：大小写/空搜索、多连接多 schema 分组、column→table、function/procedure→routine、对象类型过滤、五项 action、完整 QueryOpenContext、identifier quoting、凭据隔离和无执行路径。

## 4. 设计决策 / 遗留注意

- `SchemaObjectIndexEntry` 复用现有 `TableInfo`、`DatabaseObject` 和 `schemaStore.columnMap` 形状；只搜索调用方提供的已加载 slice，不触发 IPC、不伪造未加载对象。
- `ObjectSearchResult` 保留 `connectionId`、`dbSessionId`、database、schema、object name；column 结果额外保留 `tableName`。procedure 在搜索契约中归一为 `routine`，并保留 `sourceKind`。
- `buildTableContext` 接受现有 TablePanel/ViewPanel/search-result 的结构字段；`database` 作为 QueryPanel 选择上下文保留，不猜测并拼进 SQL 资格名。
- `quoteTableIdentifier` 逐段调用现有 `escapeIdent` metadata API；SELECT 只提供安全草稿，INSERT/UPDATE/DDL 只提供带 driver-generated 占位注释的 draft-only 模板，后续由 driver/已有 command 生成方言，不执行 SQL。
- 未修改 `ConnectionPage.tsx`、`ContentView.tsx`、`PanelContentRenderer.tsx`、`panelStore.ts`、`QueryPanel.tsx`、locales、hub、未跟踪规格文档或 codegen 文件。
