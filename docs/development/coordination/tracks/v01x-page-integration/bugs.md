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

## BUG-PI-004 — DiagnosisPanel 将原始 SQL/error 发送给 AI

- 状态：已修复并验证。
- 影响：QueryPanel 已构造脱敏诊断 context，但 DiagnosisPanel 仍接收 `exec.sql`/`exec.error` 原文并直接调用 `diagnoseError`，可能将 SQL 凭据、token 或转义 JSON secret 发送给 AI。
- 修复：DiagnosisPanel 改为只接收 `buildQueryDiagnosisContext` 结果；有效 context 仅使用 `safeSql`/`safeErrorMessage` 组装诊断参数，无效或缺失 context 直接禁止请求。保留 context 中的原文供 QueryPanel 的 Fix draft-only 比较和编辑器回填，不将脱敏文本用于回填。
- 验证：DiagnosisPanel、QueryPanel 集成测试及 aiStore→AI command 测试均断言 secret/token 和 JSON escape 不出现在诊断 payload；全量 Vitest 269 文件、2214 测试通过。

## BUG-PI-005 — QueryPanel Retry 确认后因 schemaContext 漏传而被拦截

- 状态：已修复并验证。
- 影响：QueryPanel 错误快捷动作点击 Retry 并确认后，在已有 schema context 的正常页面中不会再次执行查询；用户没有额外错误提示，表现为 Retry 无效。
- 证据：首次 `diagnosisContext` 在 `QueryPanel.tsx:596-601` 关联的 `buildQueryDiagnosisContext` 输入包含 `schemaContext`；确认后 `QueryPanel.tsx:618-626` 重建 `latestContext` 时未传 `schemaContext`，而 `aiQueryActions.ts:550-557` 将其纳入 `contextFingerprint`，导致 `QueryPanel.tsx:627-635` 的最终 `retryAction` 校验返回 `context-changed`，`runExecute('full')` 不会调用。
- 修复：`QueryPanel` 抽取共享的当前 diagnosis context builder，统一构造 database/schema、connectionId、dbSessionId、connectionContext 及 `{ tables, views, columns }` schemaContext；Retry 确认后从 `useSchemaStore.getState()` 读取最新 schema snapshot，再继续调用原有 fingerprint/SQL/params 校验。未绕过 fingerprint，未直接执行 `runExecute`。
- 回归：新增 QueryPanel 用例覆盖 schemaContext 不变时确认后仅执行一次，以及 schema、SQL、绑定参数变化时确认后阻止执行；DiagnosisPanel 安全 payload、Fix draft-only 和原始 SQL 编辑器边界用例保持通过。
- 验证：`pnpm exec vitest run src/windows/connection/__tests__/QueryPanel.executeCancel.test.tsx` 为 1 file / 12 passed / 0 failed；相关 AI/页面定向 Vitest 为 4 files / 31 passed / 0 failed；`pnpm typecheck` 通过（0 diagnostics）；locale 生成和 `git diff --check` 通过。

## BUG-PI-006 — Retry 确认后的 latest context 仍可能使用旧 panel props

- 状态：已修复。
- 修复：新增 `readCurrentQueryPanelRetryValidationInput` 最小 helper；确认返回后从 `panelStore` 读取仍存在的 query panel 与 latest SQL，从 active connection 读取当前 session/server 信息，从 panel 对应 session 的 schema store 读取最新 `tables`/`views`/`columnMap`，再构造最新 context fingerprint。最终执行作为 `retryAction.invoke` 的 guarded callback，仅在点击前 context fingerprint、SQL、bound params 全部保持一致时发生。
- 安全边界：panel 不存在或 latest context 无效直接终止；未改变 AI 脱敏 builder、Fix draft-only 行为，未直接绕过 `retryAction.invoke`，也未把旧闭包身份用于最终校验。
- 回归：QueryPanel 新增确认挂起期间 database、schema、session、connection、databaseType 各一条参数化阻断用例及 panel 删除阻断用例；schemaContext、SQL、bound params 变化、不变时单次执行、AI 脱敏、Fix draft-only、取消终态既有用例保留。
- 验证：`node scripts/generate-builtin-locales.mjs` 通过；QueryPanel 定向回归为 1 file / 18 passed / 0 failed；`pnpm typecheck` 通过（0 diagnostics）。

除上述 E2E 环境例外、既有 API 边界和 BUG-PI-005/006 外，本轨未发现需要修改已闭环驱动/领域轨的其他缺陷。

## BUG-PI-007 — Retry 确认期间 active connection 缺失未被门禁阻断

- 状态：已修复。
- 影响：Retry 确认弹窗等待期间，如果对应 active connection 条目被移除而 query panel 仍存在，最终校验可能继续使用 panel/schema 中未变化的 context fingerprint，随后进入 `runExecute('full')`；安全门禁未能证明当前连接仍是可用且归属匹配的 active connection。
- 修复：`readCurrentQueryPanelRetryValidationInput` 现在要求 active connection map 自有条目存在，entry `connectionId` 与 panel 一致，entry 状态为 `connected`，panel/entry 的非空 `dbSessionId` 且严格一致；panel 不存在、连接无效或最新 diagnosis context 构建失败均直接返回 invalid。有效连接仍构造最新 fingerprint，并继续交给 `retryAction.invoke` 同时校验 context、SQL 和 bound params，执行最多一次。
- 回归：新增异步 confirm 期间 active connection 被移除、状态变为非 connected、session 不匹配、map entry identity 不匹配四种阻断用例，并覆盖 latest context 失效；既有上下文不变单次执行及 database/schema/session/connection/databaseType/SQL/bound params 变化门禁保持通过。
- 安全边界：未引入 identity/workspace 模型，未直接调用 `runExecute` 绕过 `retryAction.invoke`；AI 脱敏 payload、Fix draft-only 和取消终态保持不变。
- 验证：locale 生成通过；QueryPanel 定向回归为 1 file / 23 passed / 0 failed；相关 AI/Query Vitest 为 4 files / 80 passed / 0 failed；`pnpm typecheck` 通过（0 diagnostics）。

### 2026-08-31 独立最终复测证据

- 定向/相关 Host Vitest 24 files / 377 tests、完整 Host Vitest 269 files / 2224 tests、contract 3 files / 22 tests、`pnpm typecheck` 均通过；这些结果不能覆盖 active connection 条目缺失的未测静态分支。
- Host E2E 按 R 登记：数据库端口访问受环境限制，且缺少 webdriver binary；未声称真实桌面 IPC 通过。

### 2026-08-31 全新独立最终复测证据（21646559 / 09c90bb2 / 3950508e / c2d41a77 / 739a9453）

- 定向/相关 Host Vitest 31 files / 466 tests、完整 Host Vitest 269 files / 2229 tests、contract 3 files / 22 tests、`pnpm typecheck`、`git diff --check` 全部通过。
- Retry confirm 竞态覆盖 active connection 条目移除、非 `connected`、session mismatch、map entry identity mismatch；同时覆盖 panel 删除、database/schema/session/connection/databaseType/SQL/bound params/schema context 变化阻断，以及有效不变上下文仅执行一次。
- DiagnosisPanel → aiStore → ai command 仅验证 `safeSql`/`safeErrorMessage` 进入诊断 payload；Fix 保持 draft-only；取消终态继续要求精确 `executionId` + `dbSessionId`，未发现新增功能 bug。
- Host E2E 按 R 登记：`pnpm e2e:skip-build -- --suite core` 因 PostgreSQL 端口 `Operation not permitted`、缺少 `target/debug/datazen` webdriver binary 和 `dist/index.html` 退出 1；未声称真实桌面 IPC 通过。`src/locales/builtinLocales.ts` 生成后已清理。

## BUG-PI-008 — Host Data Sync/Schema Diff 硬编码 PostgreSQL driver-id 分支

- 状态：待验证（修复后）。
- 证据：`src/windows/data-sync/DataSyncWindow.tsx:357,391` 与 `src/windows/schema-diff/useSchemaDiffEndpoints.ts:220,254` 直接以 `databaseType !== 'postgresql'` 决定是否加载 schema。
- 影响：Host 行为差异依赖具体 driver id；新增 PostgreSQL 兼容 driver 或可提供 schema 的其他 driver 不能仅通过 `DatabaseTypeMeta`/capability 复用该路径，违背 Host driver-agnostic 约定。
- 复现/判定：静态扫描生产 `src/**`（排除 tests/generated）发现 4 处同类分支；本轮未修改实现，也未将该架构缺陷伪装成测试通过。
- 修复：Data Sync 与 Schema Diff 均使用 `DB_REGISTRY[connection.databaseType]` 的 `supportsTables` + `supportsSQL` metadata 决定是否调用既有 `databaseCommands.getTables` schema-catalog/driver-command 入口；schema picker 仍只在 driver 返回非空 `TableInfo.schema` 时显示。未新增具体 database type 分支或 metadata 字段。
- 验证：新增非 PostgreSQL SQL driver 的 schema discovery Host 用例；Data Sync/Schema Diff 定向回归 2 files / 20 tests 通过，相关目录回归 7 files / 31 tests 通过，`pnpm typecheck` 与 `git diff --check` 通过。真实桌面 E2E 仍按 BUG-PI-001 受环境限制未执行。

## BUG-PI-009 — AI 诊断底层仍信任并暴露 raw SQL/error

- 状态：待修复；本轮仅记录。
- 证据：`src-tauri/src/commands/ai/generate.rs:249` 将 raw `error_message` 写入 debug 日志，`:289` 将 raw `sql`/`error_message` 拼入 provider user prompt；`src/stores/aiStore.ts:179-182` 也记录传入的 raw error。
- 影响：QueryPanel 的正常 UI 链路会先经 `buildQueryDiagnosisContext` 脱敏并传递 `safeSql`/`safeErrorMessage`，但直接 IPC 或 store 调用可绕过这一边界；因此 AI provider 与日志层没有端到端的 raw SQL/error 防护。
- 判定：本轮 UI safe-payload 静态检查通过，纯前端安全子集 `3 files / 22 tests` 通过；底层信任/日志静态检查不通过。本轮不修复业务代码。
