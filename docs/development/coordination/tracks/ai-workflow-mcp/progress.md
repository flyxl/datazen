# Track: ai-workflow-mcp (Phase 3.5 Workflow FR-18 + Phase 3.6 复用+MCP FR-19/20)

## 状态: PENDING

## 目标
1. Workflow 复用 seed — WorkflowChatPanel 复用 AI 聊天 seed
2. 统一连接解析 — resolveSeed 抽取为共享函数
3. dry-run 预览 — workflow 执行前预览 SQL
4. 共享组件抽取 — `components/ai/shared/` 提取通用逻辑
5. MCP 读写分类 — MCP tools 按读/写分类 + 写工具需确认
6. 设置页 MCP 区 — MCP server 管理界面

## 文件清单
- 后端：`src-tauri/src/workflow/executor.rs`（seed 复用）、`src-tauri/src/mcp/`（读写分类）
- 前端：`src/windows/workflow/WorkflowChatPanel.tsx`（复用）、`src/components/ai/shared/`（新目录）、MCP 设置区
- 测试：workflow seed 复用 + MCP 分类断言
