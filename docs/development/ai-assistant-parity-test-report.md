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
