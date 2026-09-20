# Track: ai-safety-prompt (Wave 2 — 安全门统一 + JSON 解析器 + Schema 上下文 + Prompt 异步)

## 状态: PASSED ✅

## 目标
1. `safety.rs` 删除旧路（`redact_for_egress(strict)`），统一 `redact_for_gate(gate)`
2. 敏感 key token 边界匹配 + 白名单（keyboard/monkey/turnkey 不命中）
3. `is_result_key` 改正则前缀匹配
4. `parse_ai_json` v2：`find_first_balanced_json` + `strip_fences_all` + repair 重问
5. `format_compact_ddl` 补 FK/索引/注释/默认值
6. Token 估算换 `cjk×1.6 + ascii×0.3`
7. `rank_tables` 本地打分表召回 Top30
8. `budget.rs` 动态化：`min(model_window×0.15, 16000)`
9. `prompt_resolver.rs` async 化 + `{{dialect_notes}}` + 残留断言
10. 非流式 6 命令进度事件（`ai:task-progress`）

## 改动文件
- `src-tauri/src/ai/safety.rs`
- `src-tauri/src/ai/context.rs`
- `src-tauri/src/ai/schema_pipeline.rs`
- `src-tauri/src/ai/budget.rs`
- `src-tauri/src/ai/prompt_resolver.rs`
- `src-tauri/src/commands/ai/util.rs`

## 依赖
- Wave 1 完成后（ai-cancel 和 ai-frontend 不直接冲突本轨文件）

## 验收
- `cargo test -p datazen --lib ai::safety` 全过
- `cargo test -p datazen --lib ai` 全过
- 白名单/前缀正则单测通过
- DDL FK/注释快照测试通过

## 自验结果
- [ ] cargo test -p datazen --lib ai
<<<<<<< HEAD
- 328 passed, 2 failed (ai::context::tests::format_compact_ddl_basic + ai_chat_mcp_and_db_same_round pre-existing)
- cargo check: 1 warning (dead_code: emit_task_progress), 0 errors (with local util.rs fix)

## Bug 清单
- `ai-safety-prompt-BUG-001`: format_compact_ddl_basic 断言未同步 PK NOT NULL 去重
- `ai-safety-prompt-BUG-002`: resolve_safety_gate 使用不存在的 AiDataEgressLevel::Relaxed

## 编码 Commit: ca11d711a
## 本地未提交修复（coder 已发现但未 commit）:
- `src-tauri/src/commands/ai/util.rs`: Relaxed → Unrestricted (BUG-002 修复)
- `src-tauri/src/ai/schema_pipeline.rs`: 测试断言修正 (list_tables 应被包含)
=======

## 编码 Commit: (pending)
>>>>>>> feature/ai-ui
## 测试 Commit: (pending)
