# Workflow 跨数据库工作流 — 进度跟踪

## 功能概述
扩展现有 Workflow 系统，支持跨数据库 SQL 模板工作流。

## Phase 进度

| Phase | 描述 | 状态 |
|-------|------|------|
| 1 | 数据模型扩展 (WorkflowStep/Condition/ForEach/ErrorStrategy/WorkflowExecutionResult) | ✅ 完成 |
| 2 | 模板引擎增强 (结构化结果存储 + 深层路径解析 rows.0.field / rows.* / rows_count) | ✅ 完成 |
| 3 | 连接解析器 (get_or_connect + resolve_step_connection + 连接复用) | ✅ 完成 |
| 4 | 条件执行与循环 (Condition 表达式求值 + ForEach 循环) | ✅ 完成 |
| 5 | 错误处理 (Abort/Skip/Fallback 策略) | ✅ 完成 |
| 6 | 超时控制 (per-step + 全局 timeout) | ✅ 完成 |
| 7 | 执行历史 (持久化 + 自动清理 + IPC 命令) | ✅ 完成 |
| 8 | 前端 UI (编辑器增强 + 分步结果面板 + 执行历史) | ✅ 完成 |
| 9 | IPC 命令更新 + MCP 适配 | ✅ 完成 |
| 10 | 兼容性验证 (旧 YAML 格式兼容) | ✅ 完成 |
| 11 | E2E 测试 (环境 + 数据 + 验证) | ✅ 完成 |

## 变更文件

**后端核心**:
- `src-tauri/src/mcp/workflows.rs` — 核心: 数据模型 + 执行器 + 模板引擎 + 条件/循环
- `src-tauri/src/mcp/workflow_history.rs` — 新增: 执行历史管理
- `src-tauri/src/mcp/mod.rs` — 导出新类型
- `src-tauri/src/services/connection_manager.rs` — 新增 get_or_connect
- `src-tauri/src/commands/ai.rs` — 更新 workflow_execute 返回类型 + 新增 history IPC
- `src-tauri/src/commands/mod.rs` — AppState 增加 workflow_history
- `src-tauri/src/commands/mcp.rs` — AppState 初始化同步
- `src-tauri/src/lib.rs` — 初始化 WorkflowHistoryManager + 注册新命令

**前端**:
- `src/types/index.ts` — 新增前端类型
- `src/commands/ai.ts` — 新增前端 IPC 封装
- `src/stores/aiStore.ts` — 更新 WorkflowExecutionResult 类型
- `src/components/ai/WorkflowPanel.tsx` — 重写: 支持新步骤类型 + 分步结果 + 历史
- `src/locales/zh-CN.ts` + `en.ts` — 新增 i18n 键

**测试**:
- `src-tauri/tests/workflow_tests.rs` — 13 个后端集成测试
- `e2e/specs/workflow.ts` — 6 个前端 E2E 测试
- `scripts/test-cross-db-workflow.yaml` — 测试用跨库 Workflow 定义
- `scripts/setup-workflow-testdata.sh` — 测试数据初始化脚本
- `.env` — 测试数据库连接配置

## 测试覆盖

### 单元测试 (27 tests, 全部通过)
- workflows: 23 tests (数据模型/模板解析/条件求值/深层路径/通配符/兼容性)
- workflow_history: 4 tests (记录/过滤/清理/持久化)
- 全部通过 (159 total lib tests)

### 后端集成测试 (13 tests, 全部通过)

| TC | 描述 | 类型 |
|----|------|------|
| TC-01 | 跨库 Workflow YAML 解析 | 纯解析 |
| TC-02 | PG→MySQL 跨库查询 + 模板解析 | 真实数据库 |
| TC-03 | 结构化结果序列化/反序列化 (camelCase) | 纯逻辑 |
| TC-04 | 条件判断 (真实数据: U001 有订单, U999 无订单) | 真实数据库 |
| TC-05 | ForEach 批量查询 (5 订单 → 5 物流) | 真实数据库 |
| TC-06 | 错误处理 (无效 SQL 返回错误) | 真实数据库 |
| TC-07 | 超时行为 (正常完成 + 超时触发) | 真实数据库 |
| TC-08 | 连接复用 (同一 connection 变量多步骤共享) | 纯解析 |
| TC-09 | 旧格式向后兼容 (无 timeout/error_handling 字段) | 纯解析 |
| TC-10 | 执行历史持久化 (记录/列表/详情/清理/重载) | tempdir |
| TC-11 | 通配符模板 IN 子句 (rows.*.field) | 纯解析 |
| TC-12 | 嵌套条件 + ForEach 结构解析 | 纯解析 |
| TC-13 | 完整跨库数据流端到端验证 (PG 3 订单 → MySQL 3 物流) | 真实数据库 |

- 测试文件: `src-tauri/tests/workflow_tests.rs`
- 测试 YAML: `scripts/test-cross-db-workflow.yaml`
- 运行命令: `cargo test -p datazen --test workflow_tests -- --nocapture`

### 前端 E2E 测试 (6 tests, 全部通过)

| TC | 描述 |
|----|------|
| SW-01 | Workflow CRUD (创建/读取/列表/删除) via IPC |
| SW-02 | 执行 Workflow 返回结构化结果 (steps/status/timing) |
| SW-03 | 执行历史自动记录 + 详情查询 + 清理 |
| SW-04 | 变量替换到 SQL 模板 |
| SW-05 | 条件步骤分支执行 (then_steps) |
| SW-06 | 无效 SQL 错误处理 (不崩溃) |

- 测试文件: `e2e/specs/workflow.ts`
- 运行命令: `pnpm e2e -- --spec e2e/specs/workflow.ts`
