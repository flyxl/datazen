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
