# v0.1.x page-integration 遗留与例外

## BUG-PI-001 — Host desktop E2E 未在本轨执行

- 状态：待 R 轨/具备桌面环境后补测。
- 影响：本轨无法实证真实 Tauri IPC、Webdriver 键盘路径和真实数据库交互；Host Vitest 与 typecheck 已通过，但不能替代桌面 E2E。
- 原因：当前执行环境没有可用的 `pnpm tauri build --debug --features webdriver` + Webdriver + 数据库 fixture 组合。
- 建议 journey：connection discovery（含 SSH 表单编辑保存）、object/column search → table action → TablePanel/QueryPanel、pending preview/confirm/rollback、filter/pagination race、query cancel/result chart/table/AI actions。

## BUG-PI-002 — 全局对象搜索只检索已加载 schema

- 状态：已知边界，非本轨缺陷。
- 影响：未加载的数据库/schema/object 不会出现在搜索结果中。
- 原因：本轨只消费既有 `schemaObjectSearch` 和 schema cache API；主动拉取/缓存策略属于 schema/object domain 轨，不在共享页面最终接线范围内。

## BUG-PI-003 — INSERT/UPDATE/DDL action 仍是安全草稿

- 状态：已知设计边界，非本轨缺陷。
- 影响：页面动作打开带 context 的 QueryPanel 草稿，不直接执行方言 SQL。
- 原因：遵循 `tableSqlActions` 契约和用户确认边界；真正执行仍需用户在 QueryPanel 中确认，页面没有复制领域 SQL 实现。

除上述 E2E 环境例外和既有 API 边界外，本轨未发现需要修改已闭环驱动/领域轨的缺陷。
