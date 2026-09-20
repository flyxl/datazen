# Track: ai-cancel (Wave 1 — 取消注册表 + 流式取消 + Tool Loop 护栏)

## 状态: READY_FOR_TEST

## 目标
1. ✅ 新建 `src-tauri/src/ai/cancel.rs` — `CancellationRegistry`（requestId → CancellationToken）
2. ✅ `AppState` 新增 `cancel_registry` 字段
3. ✅ 新增 IPC `ai_cancel` 命令
4. ✅ `ai_generate_sql` / `ai_chat` 入口接入 cancel 注册/检查/注销
5. ✅ `run_streaming_tool_loop` 加 `ToolLoopGuard`（rounds/token/time 预算）
6. ✅ Tool 结果截断摘要化（per-tool 2KB, per-round 12KB）
7. ✅ Unknown tool 直接终止回错
8. ⏳ MCP 写工具确认流程（`ai_confirm_tool` IPC + 前端 Dialog）— 函数已定义，IPC 待后续阶段接入
9. ✅ 三个 Protocol 的 SSE 循环加 cancel 检查
10. ⏳ `StreamChunk` 透传 `cancelled: bool` — 取消时通过 done chunk + `AiError::Cancelled` 传递

## 改动文件
- `src-tauri/src/ai/cancel.rs`（新）
- `src-tauri/src/ai/mod.rs`（export）
- `src-tauri/src/ai/protocol/openai_chat.rs`
- `src-tauri/src/ai/protocol/openai_responses.rs`
- `src-tauri/src/ai/protocol/anthropic.rs`
- `src-tauri/src/commands/ai/chat.rs`
- `src-tauri/src/commands/ai/generate.rs`
- `src-tauri/src/commands/ai/mod.rs`（注册新命令）

## 依赖
- Wave 0（ai-api-base）：`AiError::Cancelled`、`StreamChunk.cancelled`、`CompletionRequest.cancel_token`

## 验收
- `cargo test -p datazen --lib ai` 全过
- `cargo check -p datazen --lib` 编译通过
- cancel 注册/取消/注销单测通过
- ToolLoopGuard 6 轮收敛单测通过

## E2E 用例
- 【本机可执行】NL2SQL 生成中点击停止，≤500ms 停止，保留草稿
- 【留待 R 回归】Chat 多轮中写 MCP 工具弹窗确认

## 自验结果
- [x] cargo test -p datazen --lib ai — 307 passed, 0 failed
- [x] cargo check -p datazen --lib — 编译通过，0 warnings

## 编码 Commit: d969305c2
## 测试 Commit: b74a143dd

## 测试子代理复验结果

### 独立复验
- [x] cargo test -p datazen --lib ai — **326 passed**, 0 failed（含19条新增测试）
- [x] cargo test -p datazen --lib cancel — **16 passed**, 0 failed
- [x] cargo check -p datazen --lib — 编译通过，0 warnings
- [x] cargo test -p datazen-ai-api --lib — 0 passed（该 crate 无 lib 测试）

### 新增测试（19条）
| 测试 | 覆盖路径 |
|------|----------|
| test_tester_guard_new_defaults | ToolLoopGuard 默认值 |
| test_tester_guard_check_round_succeeds_within_limits | 6轮内 check_round 通过 |
| test_tester_guard_check_round_fails_on_max_rounds | 超6轮拒绝 |
| test_tester_guard_check_round_fails_on_token_budget | token 超预算拒绝 |
| test_tester_guard_check_round_fails_on_time_limit | 超时拒绝 |
| test_tester_guard_accumulate_usage | token 累积 |
| test_tester_guard_truncate_under_cap | 截断-未超限 |
| test_tester_guard_truncate_over_cap | 截断-超限含省略号 |
| test_tester_guard_truncate_exact_cap | 截断-恰好边界 |
| test_tester_guard_truncate_and_wrap | wrap 为 Tool message |
| test_tester_guard_truncate_and_wrap_truncates_large | wrap + 截断大内容 |
| test_tester_classify_tool_ask_questions | classify ask_questions |
| test_tester_classify_tool_db (5 tools) | classify DB 工具 |
| test_tester_classify_tool_mcp | classify MCP 工具 |
| test_tester_classify_tool_unknown | classify 未知/MCP 格式错误 |
| test_tester_is_readonly_db_tool | readonly DB 工具判定 |
| test_tester_mcp_needs_confirm_schema_based | schema x-write 判定 |
| test_tester_mcp_needs_confirm_name_based | 名称关键字判定 |
| test_tester_mcp_needs_confirm_readonly | 只读工具不需确认 |

### 覆盖率评估
| 模块 | 新增前估计 | 新增后估计 | 说明 |
|------|-----------|-----------|------|
| cancel.rs | ~90% | ~90% | 4单测覆盖 register/cancel/unregister/cancelled-token |
| ToolLoopGuard | ~30% | **≥85%** | 12条新测试覆盖 check_round 三路失败、truncate、wrap、边界 |
| Protocol SSE cancel | ~80% | ~80% | 三协议均用 select!+break，逻辑一致 |
| classify_tool / is_db_tool | ~70% | **≥90%** | 覆盖所有 ToolKind 变体 + 边界 |
| ai_chat_impl register/unregister | ~80% | ~80% | mock_provider_tests 覆盖正常路径 |
| ai_generate_sql_impl register/unregister | ~80% | ~80% | wiremock 测试覆盖正常路径 |
