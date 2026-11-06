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
| BUG-020 | T22 | 插件模板参数未转义：审批 SQL 泄漏凭据，SQL 字面量可被参数改变 | 已验证 |
| BUG-021 | T22 | Workflow 工具参数未转义：模型可改写只读 Workflow 的 SQL | 已验证 |
| BUG-022 | T23 | 本地 CLI 子进程退出后子孙持有 stdout 时 timeout/cancel 挂起并留下孤儿 | 已验证 |
| BUG-023 | T24 | apply_write 允许只读 SQL 执行；applied 回执/审计缺少 rowCount | 待修复 |


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

**结论：插件 BUG-020 已在 `620133a10` 验证；T22 仍因 BUG-021 不通过。** opt-in、运行时 disable、命名、schema 校验、只读直跑、
非只读审批、Stop、结果 gate、审计和前端开关主路径通过；插件 `input` 模板转义
与审批 SQL 脱敏已修复，但 Workflow 工具参数未转义。详见
[BUG-020](ai-assistant-parity-bugs/BUG-020.md) 与
[BUG-021](ai-assistant-parity-bugs/BUG-021.md)。

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


## T22 BUG-020 修复（已独立复测）

本地 `fix: escape plugin SQL templates and redact previews` 已修复
[BUG-020](ai-assistant-parity-bugs/BUG-020.md) 的两个根因：

- SQL 模板改为上下文感知渲染；单引号内转义 `'`/方言反斜杠，字符串外只允许
  number/bool/NULL 且要求 token 边界；dollar quote、双引号/反引号/方括号标识符、
  SQL 注释中的 placeholder 以及未闭合结构一律 fail-closed。
- 非只读审批前先校验模板；审批 SQL 先经 `redacted_approval_arguments` 把敏感
  key 替换为 `[REDACTED]`，再从脱敏参数渲染，避免 `SELECT '{{password}}'`
  把 `hunter2` 带进 `McpApprovalRequest.sql`。

本地复测结果：

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --lib --features ai,workflow,wapps` | 1715 passed / 0 failed |
| `cargo test -p datazen_desktop --lib` | 1391 passed / 0 failed / 10 ignored |
| T22 core renderer bridge tests | 10 passed |
| T22 host bridge tests | 8 passed |
| 定向前端（SettingsContent + AiSettingsSection + settingsClient + locales） | 69 passed |
| 全量前端 `vitest` | 8233 passed / 8 failed（既有 Pro/tauri/driver 环境用例） |
| `cargo fmt --all -- --check` / `git diff --check` | 通过 / 通过 |
| `pnpm typecheck:tsc` | 仍为既有 `LegacyCommandName` / `mcpAllowed*` 错误，无 T22 新增错误 |

覆盖用例：`O'Brien`、`x' OR '1'='1`、字符串内外 placeholder、数字/布尔/NULL、
非法字符串/数组/对象、token 拼接、dollar quote、双引号/反引号/方括号、行/块注释、
未闭合字符串/注释/dollar quote、MySQL 反斜杠方言、反斜杠转义吞掉 placeholder 起始
花括号、NO_BACKSLASH_ESCAPES 下的反斜杠引号歧义、只读直跑输入转义、非只读审批
SQL 无 `hunter2` 且非敏感参数正确转义。

## T22 `620133a10` 独立复测（BUG-020 已验证，T22 不通过 BUG-021）

`620133a10` 的插件部分通过独立复测：

- `'{{name}}'` + `O'Brien` -> `O''Brien`；`x' OR '1'='1` 不能改写条件。
- `\{{name}}` 半渲染、MySQL/ClickHouse `\'` 歧义 fail closed；PG `E'...'` 正常。
- 字符串外仅 number/bool/NULL；标识符/表达式拒绝；dollar quote、双引号、反引号、
  方括号、注释内 placeholder 与未闭合结构均 fail closed。
- 审批 `SELECT '{{password}}'` 预览为 `SELECT '[REDACTED]'`，无 `hunter2`；
  `arguments` 脱敏；非敏感参数显示并转义；预校验在任何 Allow/审批/执行前，
  不安全模板 audit deny 且无执行。

Workflow 仍失败：独立临时测试让只读 Workflow 的 Query 模板
`SELECT * FROM users WHERE name = '{{name}}'` 接收 `{"name":"O'Brien"}`，
`authorize` 直接 Allow（无审批），`execute` 成功，`mock.query_log()` 为
`SELECT * FROM users WHERE name = 'O'Brien'`，未转义为 `O''Brien`。
根因是 `execute_workflow_tool` 把变量直接交给 `WorkflowExecutor`，后者
`resolve_template` 正则原样替换；见 [BUG-021](ai-assistant-parity-bugs/BUG-021.md)。

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --lib --features ai,workflow,wapps` | 1715 passed |
| `cargo test -p datazen_desktop --lib` | 1391 passed / 10 ignored |
| T22 core renderer | 10 passed |
| T22 host bridge | 8 passed |
| T22 定向前端（SettingsContent + AiSettingsSection + settingsClient） | 46 passed |
| `tailwind-colour-classes` | 7 passed |
| 全量前端 | 8233 passed / 8 failed（既有 Pro/tauri/driver 环境用例） |
| `cargo test -p datazen-server --features mcp --locked` | 361 passed |
| 映射 worktree 包的 `pnpm typecheck:tsc` | 通过 |
| `cargo fmt --all -- --check` / `git diff --check` | 0 / 0 |

`620133a10` 的 7 个文件均为本修复相关；无 generated/无关文件。


## T22 BUG-021 修复（独立复测未通过）

本地 `fix: apply SQL-safe rendering to workflow templates` 已修复
[BUG-021](ai-assistant-parity-bugs/BUG-021.md)：

- 共用 `datazen::sql_template` 渲染器，Workflow Query/Command SQL 在 executor
  入口按目标连接方言做 SQL 上下文渲染；非 SQL 字段保持原行为。
- 输入变量保留 JSON 类型；`steps.*`（含 `rows[0]`）与 loop 变量按 typed value
  渲染。字符串只在单引号内转义，字符串外只接受 number/bool/NULL；注释、dollar
  quote、标识符和未闭合结构 fail closed。
- Workflow 只读分类后、Allow/审批/执行前遍历全部可达 SQL 模板预校验；失败 audit
  deny，不弹审批、不执行。
- 非只读审批 SQL 用脱敏变量/默认值渲染；未知 steps/loop 以 SQL NULL 代入，
  不泄漏凭据。

本地复测结果（本次修复）：

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --lib --features ai,workflow,wapps` | 1721 passed / 0 failed |
| `cargo test -p datazen_desktop --lib` | 1397 passed / 0 failed / 10 ignored |
| 共享 SQL renderer tests | 10 passed |
| 新增 executor workflow SQL 黑盒 | 5 passed |
| T22 host bridge tests | 14 passed |
| 全量前端 `vitest` | 8233 passed / 8 failed（既有 Pro/tauri/driver 环境用例） |
| `cargo fmt --all -- --check` / `git diff --check` | 通过 / 通过 |

覆盖用例：只读 Workflow `O'Brien` 实际 query log 为 `O''Brien`；字符串外注入串
在 dispatch 前拒绝；number/LIMIT；`steps.q1.rows[0].name` 与 foreach `item.id`
类型；MySQL 反斜杠；注释内 placeholder；非 SQL AI prompt 保持 `O'Brien` 原样；
非只读审批 SQL 含 `[REDACTED]`、`O''Brien`、`LIMIT 10` 且不含明文凭据；
未定义 Workflow 模板变量在审批前 deny；foreach 变量在预校验阶段已知、执行时按
typed value 转义。

## T22 `c1946962a` 独立复测：BUG-021 未通过

正常路径已通过：只读 Workflow `O'Brien` 实际 query log 为 `O''Brien`；`steps.*`/
loop/方言/数字 LIMIT/注释/dollar/标识符/未闭合、condition/fallback 分支预校验、
非 SQL prompt 原样、非只读审批 SQL 无凭据、插件 BUG-020 回归均通过。

两个独立临时测试仍失败：

1. **execute 入口未预校验，失败审计为 allow**：直接调用
   `execute_workflow_tool` 执行 `SELECT * FROM users WHERE name = {{name}}`，
   executor 在派发前拒绝（无查询副作用），但 `mcp-audit` 记录 `allow`；要求
   authorize/execute 前预校验且失败 audit deny。
2. **未定义 `steps.*` 未 fail closed**：只读 Workflow
   `SELECT '{{steps.nope.rows[0].name}}'` 的 authorize 返回 `Allow`；
   `workflow_template_value` 对任何 `steps.` 前缀直接给 Null，没有校验 step id。

复跑：datazen 1721、desktop 1397/10 ignored、server 361、host 14、全量前端
8233/8 既有环境失败、映射 typecheck OK、fmt/diff 0。详见
[BUG-021](ai-assistant-parity-bugs/BUG-021.md)。

## T22 BUG-021 第二次修复：execute 预校验 / steps fail closed

`fix: revalidate workflow templates at execute` 补齐独立复测发现的两项：

- `execute_workflow_tool` 重新读取当前定义并在 executor 前再次预校验 SQL 模板；
  失败 audit `deny`（reason `workflowTemplateInvalid`），不执行、不产生查询。
- `steps.*` 不再无条件 Null：新增共享 step 图遍历（含 condition/foreach/
  fallback），校验 step id 存在及路径语法；未定义 step、非法索引、未闭合路径
  在 authorize/execute 前拒绝，合法 steps/loop 路径保持通过。
- 新增 TOCTOU、execute 直接调用、未定义/非法路径、合法 steps 和
  condition/foreach 可达 step 测试。

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --lib --features ai,workflow,wapps` | 1721 passed / 0 failed |
| `cargo test -p datazen --lib --features mcp` | 1725 passed / 0 failed |
| `cargo test -p datazen_desktop --lib` | 1402 passed / 0 failed / 10 ignored |
| T22 host bridge | 19 passed |
| 全量前端（未改前端） | 8233 passed / 8 既有环境失败 |
| fmt / diff | 0 / 0 |

## T22 `07d65b919` 复测：loop 双段路径仍未 fail closed

上一轮两项已确认修复：直接 execute 非法模板 audit deny（reason
`workflowTemplateInvalid`）、零查询；authorize 后修改定义（TOCTOU）同样 deny、
零查询。未定义 step、非法索引/未闭合路径被拒绝；合法
`steps.q1.rows[0].name`、condition/foreach 可达 step、合法 loop `item.id`
通过并按 typed value 转义。

独立临时测试仍失败：

```text
ForEach as_var=item, body: SELECT '{{item..id}}' AS value
authorize => Allow（期望 deny）
```

`workflow_template_value` 的 loop 分支使用
`trim_start_matches('.')`，把 `item..id` 的前导双点全部吞掉，绕过了“空/重复段”
校验。合法 `item.id` 已验证执行时输出 `'O''Brien'`。

复跑：datazen 1721、desktop 1402/10 ignored、server 361、T22 host 19、全量前端
8233/8 既有环境失败、映射 typecheck OK、fmt/diff 0。详见
[BUG-021](ai-assistant-parity-bugs/BUG-021.md)。

## T22 BUG-021 第四次修复：loop 双段路径

`fix: reject malformed loop template paths`：

- loop 路径只去除恰好一个分隔点；空路径、重复点、首尾点、`..` 残留均 deny。
- 共享 typed resolver 同步对 step/loop 路径做同样的 fail-closed 校验，避免
  执行层回退到空段跳过语义。
- 新增 `item..id`、`item.`、`.id`、`item...id` deny 测试，以及合法
  `item.id`/`item.rows[0].name` 审批与执行转义回归。
- 新增 step 双段/尾点 deny 测试。

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --lib --features ai,workflow,wapps` | 1721 passed / 0 failed |
| `cargo test -p datazen --lib --features mcp` | 1725 passed / 0 failed |
| `cargo test -p datazen_desktop --lib` | 1404 passed / 0 failed / 10 ignored |
| T22 host bridge | 21 passed |
| 全量前端（未改前端） | 8233 passed / 8 既有环境失败 |
| fmt / diff | 0 / 0 |

## T22 `4242cbee0` 最终复测：BUG-021 已验证

`fix: reject malformed loop template paths` 通过独立复测。

| 验收项 | 结果 |
|---|---|
| loop `item..id`、`item.`、`.id`、`item...id`、`item.id..`、`item.rows..name` | authorize + execute deny，`workflowTemplateInvalid`，零查询 |
| step `steps.q1..name`、`steps.q1.id..` | authorize + execute deny，零查询 |
| 合法 `item.id`、`item.rows[0].name`、`steps.q1.rows[0].name` | authorize/execute 通过，`O'Brien` -> `O''Brien` |
| 前两轮 execute 重校验、TOCTOU、未定义 steps | 不回归 |
| 插件 BUG-020、T19–T21 | 不回归 |

| 范围 | 结果 |
|---|---|
| `datazen --features ai,workflow,wapps` | 1721 passed |
| `datazen --features mcp` | 1725 passed |
| `datazen_desktop` | 1404 passed / 10 ignored |
| `datazen-server --features mcp` | 361 passed |
| T22 host bridge | 21 passed |
| 全量前端 | 8233 passed / 8 既有环境失败 |
| 映射 worktree typecheck | OK |
| fmt / diff | 0 / 0 |

独立临时测试已删除；`4242cbee0` 5 个文件均为本修复相关。**BUG-021：已验证。**

## T23 本地 CLI Agent Provider 独立复测：未通过（BUG-022）

正常路径通过：8 个预设（Claude Code、Codex、Cursor、OpenCode、Grok、Qoder、
CodeBuddy、Pi）命令/参数/检测；缺失可执行文件错误可读；stdout 流式顺序；
stderr 凭据/绝对路径脱敏；敏感 env 不继承；默认隔离临时 cwd；非 0 退出；
直接子进程运行中的 cancel 杀进程组；设置页选择/自定义 path/workspace/校验。

**阻塞失败**：直接子进程已退出、后台子孙仍持有 stdout/stderr 时，适配器在
`WaitResult::Exited` 分支无 timeout、无 cancel 地等待 reader EOF。

```sh
#!/bin/sh
sleep 3 & printf 'started\n'; exit 0
```

`timeoutSecs=1` 的 `complete()` 外层 2 秒仍未返回；`started\n` 后直接 shell
退出，再调用 `cancel()` 的 `stream_complete()` 也外层 2 秒未返回。后台子孙成为
孤儿。根因是 timeout/cancel 只与 `child.wait()` 竞争，进入 `Exited` 后
`stdout_task.await` / `stderr_task.await` 不再受约束，且未调用
`kill_process_tree`。详见 [BUG-022](ai-assistant-parity-bugs/BUG-022.md)。

| 范围 | 结果 |
|---|---|
| `datazen --features ai` | 1672 passed |
| `datazen --features ai,workflow,wapps` | 1738 passed |
| `datazen --features mcp` | 1742 passed |
| `datazen_desktop` | 1405 passed / 10 ignored |
| `datazen-server --features mcp` | 361 passed |
| T23 local_cli 定向 | 17 passed |
| T23 host 定向 | 1 passed |
| T23 定向前端 | 20 passed |
| 全量前端 | 8234 passed / 8 既有环境失败 |
| 映射 worktree typecheck | OK |
| fmt / diff | 0 / 0 |

独立临时测试已删除；`68470a6f5` 18 个文件均为 T23 相关。**T23：未通过
（BUG-022）。**

## T23 `b124878a1` 复测：BUG-022 仍未通过（timeout 语义）

`fix: bound CLI pipe drain and kill orphan descendants` 已修复挂起/孤儿主路径：

| 验收项 | 结果 |
|---|---|
| cancel during exited-child pipe drain | 快速 Cancelled，后台 PID 消失 |
| 主进程退出 + background sleep 30 | ~500ms grace 内返回，后台 PID 消失 |
| normal alpha/beta/gamma | 完整输出，不截断 |
| 非 0/缺失/早取消/stderr/env/cwd | 不回归 |

但 item 1 的 timeout 结果不满足：

```sh
sleep 3 & printf 'started\n'; exit 0
# timeoutSecs=1
```

实际约 854ms 返回 `Ok(..., content: "started\n")`，后台 sleep 已清理，但不是
`Err(AiError::Timeout(1))`。原因是 500ms pipe drain grace 先于 timeout，杀进程组
后按成功返回。详见 [BUG-022](ai-assistant-parity-bugs/BUG-022.md)。

| 范围 | 结果 |
|---|---|
| `datazen --features ai` | 1676 passed |
| `datazen --features ai,workflow,wapps` | 1742 passed |
| `datazen --features mcp` | 1746 passed |
| `datazen_desktop` | 1405 passed / 10 ignored |
| `datazen-server --features mcp` | 361 passed |
| local_cli 定向 | 21 passed |
| T23 定向前端 | 20 passed |
| 全量前端 | 8234 passed / 8 既有环境失败 |
| 映射 worktree typecheck | OK |
| fmt / diff | 0 / 0 |

独立临时测试已删除；`b124878a1` 仅改 `local_cli.rs`。**T23：未通过（BUG-022）。**

## T23 `a3990293e` 最终复测：BUG-022 已验证

`fix: report drained CLI hangs as timeouts` 修复了 timeout 语义：被迫 drain 杀进程组
不再返回 Ok。

| 验收项 | 结果 |
|---|---|
| `sleep 3 & ...` + timeoutSecs=1 | `Err(Timeout(1))`，~0.7s，孤儿清理 |
| 主进程退 + background sleep 30 | `Err(Timeout(30))`，<5s，孤儿清理 |
| stream 变体 | 已收到 `started` 后仍 Timeout；无 done；返回 Err |
| cancel during drain | Cancelled，孤儿清理 |
| 自然 EOF alpha/beta/gamma | 完整 `Ok` |
| 非 0/缺失/早取消/stderr/env/cwd | 不回归 |

| 范围 | 结果 |
|---|---|
| `datazen --features ai` | 1677 passed |
| `datazen --features ai,workflow,wapps` | 1743 passed |
| `datazen --features mcp` | 1747 passed |
| `datazen_desktop` | 1405 passed / 10 ignored |
| `datazen-server --features mcp` | 361 passed |
| local_cli 定向 | 22 passed |
| T23 定向前端 | 20 passed |
| 全量前端 | 8234 passed / 8 既有环境失败 |
| 映射 worktree typecheck | OK |
| fmt / diff | 0 / 0 |

a3990293e 仅改 `local_cli.rs`；独立临时测试已删除。**T23：通过（BUG-022 已验证）。**

## T24 写入提案与精确确认独立复测：未通过（BUG-023）

通过：proposal 绑定 connection/database/exact SQL/SHA-256/expiry；UUID token；
一次性/过期/篡改 SQL/连接/库拒绝；生产库/只读连接不执行；apply 前 guard；
propose 不执行；Agent 工具只返回 proposal；UI exact SQL/diff/影响、Allow once/
Deny、过期自动拒绝；Deny 不执行；审计 proposal/applied/denied 无 SQL/凭据；
失败可读。

**阻塞失败 1**：只读 SQL 可执行。临时 store 测试中
`propose("SELECT * FROM users")` 成功，exact `apply` 返回 Ok；宿主临时测试中
`apply_write_for_state` 返回 receipt 且 Mock `execute_calls()==1`，SELECT 被
`execute_target("execute")` 实际执行。根因是 `sql_guard::check_sql` 在
`!read_only && !safe_mode` 时直接放行，proposal 层没有“必须是写语句”校验。

**阻塞失败 2**：applied rowCount 丢失。对 UPDATE 的 apply 回执
`row_count=None`，实际驱动结果是
`{"results":[{"rowsAffected":0,...}],"totalTimeMs":0}`；`affected_rows` 只看顶层
字段/直接数组，未进入 `results`，audit applied.row_count 也为 null。违反审计
rowCount 验收；UI 回执把 null 显示为 0。

详见 [BUG-023](ai-assistant-parity-bugs/BUG-023.md)。

| 范围 | 结果 |
|---|---|
| `datazen --features ai` | 1691 passed |
| `datazen --features ai,workflow,wapps` | 1757 passed |
| `datazen --features mcp` | 1761 passed |
| `datazen_desktop` | 1409 passed / 10 ignored |
| `datazen-server --features mcp` | 361 passed |
| T24 定向前端 | 12 passed |
| 全量前端 | 8239 passed / 8 既有环境失败 |
| 映射 worktree typecheck | OK |
| fmt / diff | 0 / 0 |

独立临时测试已删除；`d79466e2` 36 个文件均为 T24 相关，无 generated/凭据/无关。
**T24：未通过（BUG-023）。**
