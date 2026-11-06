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
| BUG-018 | T20 | Redis key_pattern 可被多 key EXISTS 绕过；拒绝的 Redis 命令审计记为 allow；batch 无逐语句审计且审计 rowCount 为空 | 已验证 |
| BUG-019 | T21 | HTTP token 输入框使用未定义的 Tailwind 颜色类 `bg-bg`，`tailwind-colour-classes` 门禁失败 | 已验证 |
| BUG-020 | T22 | 插件模板参数未转义：审批 SQL 泄漏凭据，SQL 字面量可被参数改变 | 待修复 |


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

## T20 MCP batch/Redis 独立复测（2026-11-06，`4d3de19d2`）

**结论：已验证。** `4d3de19d2`（父 `0c601ddfa`）修复了 batch 审计的 SQL/凭据泄漏、
逐语句实际 target 和结构化 index，并保持 Redis/T19 行为。详见
[BUG-018](ai-assistant-parity-bugs/BUG-018.md)。

- `SELECT no_such_column_t20, 'secret-pw-t20'` 的 tool 输出和审计 raw JSON 均不含
  SQL/列名/合成凭据，只暴露 `errorCode` 和安全描述；连接、commit、rollback 失败同样安全。
- MySQL `SELECT 1; SELECT COUNT(*) FROM datazen_t18.datazen_t18_probe` 的审计 index
  0/1 分别记录 `datazen_demo` 和实际 `datazen_t18`；越界/跳过语句记录尝试 target，
  汇总行保留且无 index。
- 审计 `index` 为零基结构化字段，前端类型同步为 `index?: number | null`，typecheck 通过。
- Redis EXISTS/MGET 多 key、TOUCH/写拒绝、deny/allow 审计不回归；T19 query/list_tables/
  scope/审批/审计不回归。
- `pnpm typecheck:tsc` 在临时把 `@datazen/*` 指向当前 worktree 包后通过；不映射时的
  `LegacyCommandName`/`mcpAllowedDatabases` 报错来自主检出旧包，不是 T20/T19 类型回归。

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --features mcp --lib` | 1707 passed |
| `cargo test -p datazen_desktop --lib` | 1368 passed / 10 ignored |
| `cargo test -p datazen-server --features mcp --locked` | 361 passed |
| T19 定向前端 | 39 passed |
| 全量前端 `vitest` | 8231 passed / 8 failed（环境用例，非 T20） |
| `pnpm typecheck:tsc` / `cargo fmt --all -- --check` / `git diff --check` | 通过 / 0 / 0 |
| T20 修复范围 | 5 个 T20 文件；无 generated/无关文件 |

## T21 MCP Streamable HTTP 独立复测（2026-11-06，`3b83395f3` + `efa09bdca`）

**结论：已验证。** `efa09bdca`（`fix: use defined surface color for MCP token field`）
把 token 输入框的 `bg-bg` 改为已定义 `bg-surface-inset`，颜色门禁与 T21 全量验收通过。

- 默认不监听；启用后真实 `TcpListener` 只绑定 `127.0.0.1`，端口 0 返回实际端口与 URL；
  disable/Stop 关闭 listener；端口冲突返回可读 `could not bind`，且不留下 enabled 开关或
  一次性 token。
- Bearer token：缺失/错误/轮换前旧 token 均 401；正确 token 可 initialize、tools/list、
  tools/call；轮换后旧 token 立即 401。token 文件仅 `version/hash/last4`，Unix 权限 0600；
  只有 enable/regenerate 生成响应返回一次明文，status/日志/IPC 其余路径不回显。
- HTTP 复用同一 `DataZenMcpServer` 工具/scope/权限/审批/审计：临时 HTTP 集成验证了
  T19 scope deny + audit、T20 `execute_batch` 逐语句 index、Stop 后端口拒绝连接。
- 并发 session 隔离、DELETE 关闭、Stop 释放有状态会话，以及 stdio 下 T19/T20 回归测试均通过。
- 前端开关/端口/实际 URL/状态/token 一次性显示；临时测试验证运行中改端口与 disable 清空 token。
- `pnpm typecheck:tsc` 在临时映射 `@datazen/*` 后通过。未映射时复现 43 errors（其中 5 个
  `mcp_http_*` + 既有 stale-main 类型），但 worktree 的 `LegacyCommandName` 已包含全部 5 个
  新命令；映射后无错误，故不是 T21 类型回归。
- **BUG-019 已验证**：`efa09bdca` 将 token 输入框改为 `bg-surface-inset`；
  `vitest run scripts/__tests__/tailwind-colour-classes.test.ts` 7 passed / 0 failed，
  不再报告未定义颜色类。详见 [BUG-019](ai-assistant-parity-bugs/BUG-019.md)。

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --features mcp --lib` | 1707 passed |
| `cargo test -p datazen_desktop --lib` | 1383 passed / 10 ignored |
| `cargo test -p datazen_desktop --lib mcp::http::tests` | 12 passed |
| `cargo test -p datazen_desktop --lib commands::mcp::tests` | 10 passed |
| `cargo test -p datazen-server --features mcp --locked` | 361 passed |
| T21 定向前端（SettingsContent + McpApprovalHost） | 38 passed |
| 全量前端 `vitest` | 8232 passed / 8 failed（均为既有 Pro/tauri/driver 环境用例） |
| `pnpm typecheck:tsc` / `cargo fmt --all -- --check` / `git diff --check` | 通过 / 0 / 0 |
| T21 范围 | 29 个 T21 文件；无 generated/无关文件 |

## T22 插件/Workflow 工具桥独立复测（2026-11-06，`fc97c8647`）

**结论：不通过（BUG-020）。** opt-in、运行时 disable、命名、schema 校验、只读直跑、
非只读审批、Stop、结果 gate、审计和前端开关主路径通过；但插件 `input` 模板参数未转义，
审批 SQL 可泄漏凭据，字符串参数可改变 SQL。两处均见
[BUG-020](ai-assistant-parity-bugs/BUG-020.md)。

- 未 opt-in 时无工具定义；运行时禁用插件后定义消失，旧工具调用 fail closed、不弹审批、
  不执行；独立临时测试通过。
- 命名 `plugin_<id>__<tool>` / `workflow_<id>` 唯一；非法/未知参数在审批前拒绝。
- 只读 workflow 直跑；非只读 plugin/workflow 走 T19 审批 broker；Deny/Timeout/Cancel
  不执行；Stop 取消；结果 64 KiB 截断；工具结果经 gate 脱敏。
- 独立临时测试发现：模板 `SELECT '{{password}}'` 的审批 `sql` 原样含 `hunter2`
  （`arguments.password` 已 `[REDACTED]`）；模板 `name = '{{name}}'` 对 `O'Brien`
  原样渲染，未做 SQL 字面量转义。临时测试已删除，未修改生产代码。
- T19/T20/T21 回归套件随完整 Rust/前端测试通过。

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --features mcp --lib` | 1714 passed |
| `cargo test -p datazen_desktop --lib` | 1389 passed / 10 ignored |
| T22 core bridge | 5 passed |
| T22 host bridge | 6 passed |
| `cargo test -p datazen-server --features mcp --locked` | 361 passed |
| T22 前端（SettingsContent 34 + settingsClient 6 + McpApprovalHost 5） | 45 passed |
| `tailwind-colour-classes` | 7 passed |
| 全量前端 | 8233 passed / 8 failed（均为既有 Pro/tauri/driver 环境用例） |
| `pnpm typecheck:tsc`（临时映射 `@datazen/*`） | 通过 |
| `cargo fmt --all -- --check` / `git diff --check` | 0 / 0 |
| T22 范围 | 41 个 T22 文件；无 generated/无关文件 |

> 未映射的 `@datazen/*` 仍解析到主检出旧包，`mcpAllowedDatabases/Namespaces` 与
> `LegacyCommandName` 报错属于并行 worktree 环境；映射到当前 worktree 后 tsc 通过，
> 未发现 T22 新增类型错误。
