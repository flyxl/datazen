# Skill 跨数据库工作流 — 进度跟踪

## 功能概述
扩展现有 Skill 系统，支持跨数据库 SQL 模板工作流。

## Phase 进度

| Phase | 描述 | 状态 |
|-------|------|------|
| 1 | 数据模型扩展 (SkillStep/Condition/ForEach/ErrorStrategy/SkillExecutionResult) | ✅ 完成 |
| 2 | 模板引擎增强 (结构化结果存储 + 深层路径解析 rows.0.field / rows.* / rows_count) | ✅ 完成 |
| 3 | 连接解析器 (get_or_connect + resolve_step_connection + 连接复用) | ✅ 完成 |
| 4 | 条件执行与循环 (Condition 表达式求值 + ForEach 循环) | ✅ 完成 |
| 5 | 错误处理 (Abort/Skip/Fallback 策略) | ✅ 完成 |
| 6 | 超时控制 (per-step + 全局 timeout) | ✅ 完成 |
| 7 | 执行历史 (持久化 + 自动清理 + IPC 命令) | ✅ 完成 |
| 8 | 前端 UI (编辑器增强 + 分步结果面板 + 执行历史) | ✅ 完成 |
| 9 | IPC 命令更新 + MCP 适配 | ✅ 完成 |
| 10 | 兼容性验证 (旧 YAML 格式兼容) | ✅ 完成 |
| 11 | E2E 测试 | 待开始 |

## 变更文件
- `src-tauri/src/mcp/skills.rs` — 核心: 数据模型 + 执行器 + 模板引擎 + 条件/循环
- `src-tauri/src/mcp/skill_history.rs` — 新增: 执行历史管理
- `src-tauri/src/mcp/mod.rs` — 导出新类型
- `src-tauri/src/services/connection_manager.rs` — 新增 get_or_connect
- `src-tauri/src/commands/ai.rs` — 更新 skill_execute 返回类型 + 新增 history IPC
- `src-tauri/src/commands/mod.rs` — AppState 增加 skill_history
- `src-tauri/src/commands/mcp.rs` — AppState 初始化同步
- `src-tauri/src/lib.rs` — 初始化 SkillHistoryManager + 注册新命令
- `src/types/index.ts` — 新增前端类型
- `src/commands/ai.ts` — 新增前端 IPC 封装
- `src/stores/aiStore.ts` — 更新 SkillExecutionResult 类型
- `src/components/ai/SkillsPanel.tsx` — 重写: 支持新步骤类型 + 分步结果 + 历史
- `src/locales/zh-CN.ts` + `en.ts` — 新增 i18n 键

## 单元测试
- skills: 23 tests (数据模型/模板解析/条件求值/深层路径/通配符/兼容性)
- skill_history: 4 tests (记录/过滤/清理/持久化)
- 全部通过 (159 total lib tests)
