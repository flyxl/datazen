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
