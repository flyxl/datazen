# AI Assistant Parity 缺陷列表

| Bug | 关联任务 | 标题 | 状态 |
|---|---|---|---|
| BUG-001 | T01 | ai.md 仍把旧状态字段名与已删除的 nlConfig.ts 当作当前事实 | 已验证 |
| BUG-002 | T02 | 新增 chat_logs_persisted_connection_id 在默认并行测试下间歇失败（约 30%） | 已验证 |
| BUG-003 | T04 | AiToolContext.allowed_connection_id=None 时 query_readonly/sample_rows 默认放行任意连接（workflow Agent 可跨连接读取） | 已验证 |
| BUG-004 | T06 | 前端未采集编辑器选区，autoContext.selection 没有真实消费者，选区自动上下文缺失 | 已验证 |
| BUG-005 | T07 | 方言/无语言标记的 SQL 代码块显示 Run 但不会执行 | 已验证 |
| BUG-006 | T08 | RequireConfirm 无桌面确认 UI/超时，宿主固定拒绝，策略无法接受调用 | 已验证 |
| BUG-007 | T09 | 删除连接后 AI 清理失败只 warn，connections.json 已删而 notes/memories/audit 成为孤儿且无重试 | 已验证 |
| BUG-008 | T10 | 连接笔记排在 DB comment 之后；无匹配 databaseType 的模板仍回退注入第一个 default | 已验证 |
| BUG-009 | T11 | 记忆注入未清理 ALM/206A-F/2028 等 bidi/格式字符；非 CJK Unicode token 估算低估导致 1500 预算可被突破 | 已验证 |
| BUG-010 | T12 | 真实 Chat/NL2SQL 前端调用未传 memoryMode/scope；NL2SQL 还缺 connectionId，且流处理丢弃 memoryUsed | 已验证 |
| BUG-011 | T13 | MemorySection 使用原生 select 违反 noNativeSelect 门禁；关闭 memory 时 ai_memory_create 仍可写入 candidate | 已验证 |
| BUG-012 | T14 | pattern 无真实自动采集入口；确认冲突候选后同一槽位两条 active；Chat 代码块不建立 correction 基线 | 已验证 |
| BUG-013 | T15 | 旧 localStorage `dbSessionId::db` 会话迁移到 AppDb 后仍按旧 key 存储，当前 connectionId 会话列表/恢复看不到，迁移成功提示失真 | 已验证 |
| BUG-014 | T16 | 新兼容预设 validate_config 只依赖 `/models`，未回退 probe，仅实现 chat/messages 的兼容端点验证误报 | 已验证 |
| BUG-015 | T17 | autoUse 为死设置且 listing 无显式选择门控；同名技能未并列；缺 2-skill 正文预算 | 已验证 |
| BUG-016 | T18 | `#` 方言、CLI ack、onboarding、macOS bundle argv、SQL Server 非法 LIMIT 已修复 | 已验证 |
| BUG-017 | T19 | MCP database/namespace scope 只校验工具参数，SQL 可跨库/跨 schema 执行；schema resource 与默认解析误拒；审计目标/行数/requestId 不完整 | 已验证 |


## T19 MCP scope / 逐次审批 / 审计验收记录（已验证）

最终复测提交：`8fa40665a`（`fix: inject resolved MCP default target into queries`，
父 `4d851fae5`），2026-11-06 在真实 MySQL/PostgreSQL 上通过。

### 验收结果

- SQL scope：允许 `mysql` 时跨库、`information_schema`、CTE、子查询、UNION、JOIN、
  DML、`USE`/`SET`、解析失败和多语句均在任何执行前拒绝；PG 允许 `postgres` +
  `public` 时 `information_schema`/`pg_catalog` 同样拒绝。允许范围内 SELECT/CTE
  正常。
- SHOW：`SHOW TABLES FROM datazen_demo`、`SHOW DATABASES`、`SHOW CREATE TABLE`、
  `SHOW COLUMNS/INDEX/TABLE STATUS` 的 scope 外目标均拒绝；`SHOW TABLES FROM mysql`、
  `SHOW COLUMNS FROM mysql.user` 等 scope 内调用正常。
- 默认目标：真实 `conn_e2e_pg` 的 `database=null`、`schema=null`、scope `postgres`
  时，`SELECT 1 AS ok` 与 `list_tables` 均成功；审计为 `postgres/public`、
  `decision=allow`、`rowCount=1`（query）、requestId 非空，不再出现
  `MissingDatabase`。
- 显式目标：`database=template1`、`schema=information_schema` 的显式调用未被默认值
  覆盖，审计记录实际目标；scope 外显式目标仍拒绝。
- 审计：scope 拒绝记录 SQL 中实际 `datazen_demo`/`information_schema`/
  `pg_catalog`；allow/deny/read 均有 requestId；前端审计表展示 requestId 与
  namespace。
- schema resource：`datazen://schema/conn_e2e_pg/postgres` 在 `postgres` + `public`
  scope 下返回非空 DDL（1547 chars），不误拒、不为空。
- 审批：Allow once 执行并审计，Deny/timeout/cancel/Stop 不执行；headless 无
  presenter 时一律 Deny，不回归。

### 回归统计

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --features mcp --lib` | 1685 passed |
| `cargo test -p datazen_desktop --lib` | 1358 passed / 10 ignored |
| `cargo test -p datazen_desktop --lib mcp::server::tests` | 62 passed / 1 ignored（live PG） |
| `cargo test -p datazen --features mcp --lib mcp::domain::sql_scope` | 11 passed |
| 真实多库驱动测试（`PostgresDriver`） | 2 passed |
| live PostgreSQL（`DATAZEN_LIVE_PG=1 ... --ignored`） | 1 passed |
| T19 定向前端（`McpApprovalHost` + `SettingsContent` + settings） | 39 passed |
| 全量前端 `vitest` | 8231 passed / 8 failed（worktree Pro/驱动构建环境用例，非 T19） |
| `pnpm typecheck:tsc` | 临时映射 `@datazen/*` 到当前 worktree 后通过 |
| `cargo fmt --all -- --check` / `git diff --check` | 0 |

黑盒 MCP 使用隔离 `DATAZEN_DATA_DIR` 的 `datazen_desktop --mcp`；审计为追加式
JSONL，审计 append 失败不影响主调用。`8fa40665a` 的 7 个代码文件全部属于 T19，
无 generated/无关文件。

> Worktree 的 `node_modules` 软链到主检出，默认会让 `@datazen/*` 解析到主检出的旧
> shared settings 类型。本次 `pnpm typecheck:tsc` 在把 `@datazen/*` 临时指向当前
> worktree 包的映射下执行，检查完成后已恢复软链；这是并行 worktree 的环境动作，
> 不进入提交。
