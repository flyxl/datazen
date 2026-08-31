# v01x-connection-discovery 进度

## 1. 功能摘要

- 编号：v01x-connection-discovery
- 范围：连接搜索/排序、Pinned/Recent/Group 导航层级、可解释匹配上下文、连接表单 Basic/Advanced/SSH 折叠
- 状态：编码完成；定向回归通过；Host E2E 留待 R 回归
- 编码 commit：`256301257e63871b66fa5c25126efa237142ec8b`
- 冻结边界：未修改 `ConnectionPage.tsx`、`ContentView.tsx`、`panelStore.ts`、共享 types、共享 locales、hub、未跟踪规格文档或 codegen 文件

## 2. 实现记录

- 新增 `src/lib/connectionLocator.ts`，统一 `name`、`host`、`database`、database type（id/label）和已有 schema/object/path 缓存命中语义；返回 `rank`、`reason`、`context`。
- 搜索结果按匹配强度、Pinned、`lastConnectedAt`、Group/名称/id 稳定排序；搜索态展平为单一结果集并按 connection id 去重。
- 无搜索态新增非持久化 Recent 区段（最多 5 个非 Pinned 的有效 `lastConnectedAt` 连接），保留 Pinned/Recent/Group 层级；继续复用现有 `pinned`、`lastConnectedAt` 和连接持久化。
- 导航连接行暴露 `data-search-match-reason`、`data-search-match-context` 和 title，便于 UI/测试解释命中来源；未改 ConnectionPage 最终接线。
- 表单默认只展开 Basic；Advanced 保留 SSL/Read Only/Object Filter/驱动高级字段，SSH 单独折叠；编辑已有 SSH 连接时自动展开，保存/密码/驱动插件字段/导入流程未改。

## 3. E2E 用例

| 编号 | 场景 | 类型 | 状态 |
|---|---|---|---|
| CD-E2E-001 | 搜索名称、host、database、database type，并按匹配强度/Pinned/最近连接排序 | 留待 R 回归 | 本环境未执行桌面 E2E |
| CD-E2E-002 | 搜索已连接缓存中的 schema/table/view/object/path，显示可解释 match reason/context | 留待 R 回归 | 本环境未执行桌面 E2E |
| CD-E2E-003 | 无搜索时检查 Pinned/Recent/Group 层级，搜索时同一连接不重复 | 留待 R 回归 | 本环境未执行桌面 E2E |
| CD-E2E-004 | 新建表单只显示 Basic；Advanced 与 SSH 独立折叠；编辑已有 SSH 连接保持可见 | 留待 R 回归 | 本环境未执行桌面 E2E |

## 4. 定向验证（2026-08-31）

- 使用临时 `/private/tmp` Vitest 配置为缺失的 ignored `src/locales/builtinLocales.ts` 提供测试别名，未写入工作树；相关 4 个套件共 97 个测试通过、0 失败。
- 普通 Vitest 命令受工作树缺少生成文件 `src/locales/builtinLocales.ts` 阻塞；未运行 `pnpm install`，也未生成/提交 codegen 文件。
- `npx tsc --noEmit` 仍有 2 个既有诊断：缺失 `src/locales/builtinLocales.ts`，以及 `src/windows/settings/SettingsContent.tsx:66` 的既有 implicit-any；未出现本轨新增诊断。
- `git diff --check` 与 `git diff --cached --check` 均通过；编码 commit 已生成。

## 5. 设计决策 / 遗留

- Recent 仅是导航展示区段，不新增用户/workspace 模型或持久化字段；Pinned 连接不重复进入 Recent。
- 为避免新增 locale，Pinned 区段复用 `main.ctx.pinConnection`，Recent 区段复用已有 `context.recent`。
- 本轨未执行黑盒桌面 E2E；由 R 按上表登记并回归。
