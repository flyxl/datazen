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
| BUG-017 | T19 | MCP database/namespace scope 只校验工具参数，SQL 可跨库/跨 schema 执行；schema resource 与默认解析误拒；审计目标/行数/requestId 不完整 | 待修复 |

## T19 MCP scope / 逐次审批 / 审计验收记录

| 范围 | 命令 | 结果 |
|---|---|---|
| 策略矩阵 + 审批预览 | `cargo test -p datazen --features mcp --lib mcp::domain` | 16 passed |
| MCP 集成（scope/deny/timeout/cancel/audit） | `cargo test -p datazen_desktop --lib mcp::server::tests` | 57 passed（含既有 server 测试） |
| 前端审批事件流 | `pnpm vitest run packages/frontend/src/components/__tests__/McpApprovalHost.test.tsx` | 5 passed |
| MCP 设置 scope/审计 | `pnpm vitest run packages/frontend/src/windows/settings/__tests__/SettingsContent.test.tsx` | 32 passed |
| 全量前端 | `pnpm vitest run` | 8314 passed；3 个 Pro 构建脚本用例因 worktree 缺 `@tauri-apps/api` 解析失败（非 T19） |
| 全量桌面 lib | `cargo test -p datazen_desktop --lib` | 1353 passed / 9 ignored |
| 全量 datazen MCP lib | `cargo test -p datazen --features mcp --lib` | 1672 passed |
| 类型检查 | `pnpm typecheck:tsc` | 通过（worktree 包映射；见下） |

审计采用追加式 JSONL（`mcp-audit.jsonl`）而不是新 AppDb 迁移：满足可持久化/可查询，
且审计写失败不会改变主调用结果。未覆盖：跨 database 的 SQL 限定名无法仅凭 scope
静态证明；当前以连接默认 database + 数据库自身权限作为边界。

> Worktree 的 `node_modules` 软链到主检出，默认会让 `@datazen/*` 解析到主检出的旧
> shared settings 类型。本次 `pnpm typecheck:tsc` 在把 `@datazen/*` 临时指向当前
> worktree 包的映射下执行，检查完成后已删除映射；这是并行 worktree 的环境动作，
> 不进入提交。

## T19 独立复测结论（2026-11-06，`57bb4f4bd`）

**结论：不通过。** 审批主路径和显式参数 scope 可用，但 BUG-017 记录了三类硬缺口：
database/namespace scope 不约束 `query` 的 SQL 目标（真实 MySQL 允许库 `mysql` 时
读取 `datazen_demo.test_orders` 成功；PostgreSQL 允许 namespace `public` 时读取
`information_schema` / `pg_catalog` 成功）；namespace scope 下 schema resource 全部
误拒；默认 database/schema 为 null 时误拒；审计的 database/namespace/requestId
不完整，MySQL SELECT 的 1 行被记为 `rowCount: 0`。详见
[BUG-017](ai-assistant-parity-bugs/BUG-017.md)。

独立复测统计：

| 范围 | 结果 |
|---|---|
| `cargo test -p datazen --features mcp --lib mcp::domain` | 16 passed |
| `cargo test -p datazen_desktop --lib mcp::server::tests` | 57 passed |
| `cargo test -p datazen_desktop --lib` | 1353 passed / 9 ignored |
| `cargo test -p datazen --features mcp --lib` | 1672 passed |
| T19 定向前端（`McpApprovalHost` + `SettingsContent`） | 37 passed |
| 全量前端 `vitest` | 8231 passed / 8 failed；失败为 worktree 的 Pro/驱动构建环境用例，非 T19 文件 |
| `pnpm typecheck:tsc` | 临时映射 `@datazen/*` 到当前 worktree 后通过；未映射时因主检出旧类型失败 |
| `cargo fmt --all -- --check` / `git diff --check` | 0 |
| 变更文件 | 50 个，全部 T19，无 generated/无关文件 |

黑盒 MCP 使用隔离 `DATAZEN_DATA_DIR` 的 `datazen_desktop --mcp`；审批在 headless 无
presenter 时一律 Deny，Allow once 由桌面集成测试覆盖。审计 append 失败时查询仍成功，
未阻断调用。


## BUG-017 修复后复测（2026-11-06，`f1a94b2d1`）

**结论：仍不通过，BUG-017 保持“待修复”。** 该提交相对 `a0676c8f0` 仅增加审计表
namespace 展示，Rust SQL scope 逻辑未变。真实 MCP 黑盒仍复现：

- MySQL 允许库 `mysql` 时，`SHOW TABLES FROM datazen_demo`、`SHOW DATABASES`、
  `SHOW CREATE TABLE datazen_demo.test_orders` 均执行成功，SQL scope 未覆盖 `SHOW`；
  审计把目标记为 `mysql`。
- PostgreSQL 连接 `database=null`、`schema=null`、scope `postgres` 时，`SELECT 1`
  在解析前报默认目标探针 `has no database`，仍无法按实际默认库解析。
- 允许 `postgres` + `public` 时，schema resource 不再报错，但返回内容为空（PG 驱动
  要求显式 schema，`connection_schema_context` 固定传 `schema: null`）。
- 其余重点通过：显式默认库下 MySQL/PG 跨库跨 schema 的普通 SELECT、CTE、子查询、
  UNION、JOIN、DML、多语句、USE/SET、解析失败均在执行前拒绝；schema 非 null 时
  namespace 探针可用；SELECT rowCount/requestId、前端审计列、审批回归正常。

复跑统计：`datazen` 1679 passed；`datazen_desktop` 1358 passed / 9 ignored；T19
定向前端 39 passed；全量前端 8231 passed / 8 failed（环境用例）；临时映射
`pnpm typecheck:tsc` 通过；`cargo fmt --all -- --check`、`git diff --check` 为 0。
