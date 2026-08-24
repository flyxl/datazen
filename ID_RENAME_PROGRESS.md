# ID 术语统一重构 — 进度管理文件

> 分支：`feature/db-session-id-rename`（worktree：`.worktrees/db-session-id-rename`）
> 约定：**`connectionId` = 配置连接 id（持久化，原 `configId`）**；**`dbSessionId` = 运行时数据库会话 id（内存态，原 `connectionId`）**
> 流程：每个工作项 = 编码 agent 开发（含单测）→ commit → 全新测试 agent 测试（输出 E2E 用例与结果、覆盖率 ≥80%，只测不修）→ commit → bug 按 `验证不通过 → 待验证 → 已修复` 闭环。

## 一、功能工作项

| # | 工作项 | 范围 | 状态 | 完成时间 | 备注 |
|---|--------|------|------|----------|------|
| W1 | 后端核心与服务层术语落地 | `services/connection_manager.rs` 内部命名（`config_id_map`→`session_owner_map` 等）、错误信息区分两种 id；IPC 契约暂不变 | ✅ 开发完成（待测试） | 2026-08-24 | 26 文件 +482/-278；新增 3 个不变式测试；lib 测试 1115 过 / 2 失败（既有环境问题）；IPC 签名零变化 |
| W2 | IPC 契约切换（前后端原子批） | Tauri 命令参数改名 + `src/commands/*` 封装同步 + 全部前后端调用点 + MCP 双参数兼容（新增 `connection_id`，保留 `config_id` 别名） | 未开始 | - | |
| W3 | 前端状态/类型/组件改名 | `types/index.ts`、stores、`connectionViews/types.ts`、组件 props、跨窗口事件 payload、windowManager、extensionBridge 显式目标 | 未开始 | - | |
| W4 | 持久化与外部契约对齐 | SQLite 列名保留 + 注释标注新语义；MCP tool_help/资源文案；allowlist 命名；history_db 迁移注释 | 未开始 | - | |
| W5 | 文档与守护 | `docs/architecture/naming.md`、AGENTS.md 精简更新、lint/grep 守护规则 | 未开始 | - | |

收尾：回归测试 → 文档更新 → 合并 main。

## 二、Bug 跟踪

状态流转：`验证不通过`（测试发现）→ `待验证`（编码 agent 已修复）→ `已修复`（测试 agent 验证通过）

| Bug ID | 发现于 | 描述 | 重现步骤 | 状态 | 关联工作项 |
|--------|--------|------|----------|------|------------|
| （暂无） | | | | | |

## 三、测试记录

### E2E / 单元测试用例与结果

| 轮次 | 工作项 | 用例摘要 | 结果 | 覆盖率 | 测试 agent |
|------|--------|----------|------|--------|------------|
| D1（开发自测） | W1 | 不变式：驱逐自动重连保持 db_session_id；resolve_session 双模解析；错误文案区分两种 id | `cargo test -p datazen --lib`：1115 通过 / 2 失败（既有 sandbox 环境问题，干净 HEAD 复现，非本次引入） | 待独立测试 agent 评估 | 编码 agent ad05a2ae |
| T1（独立测试） | W1 | 见 `test-reports/W1-test-report.md` | 待测试 | 待评估 | 全新测试 agent |

## 四、提交记录

| Commit | 说明 |
|--------|------|
| c76118f9 | 进度文件初始化 |
| （本次） | W1 开发里程碑：后端核心/服务层改名 + 单测 + 进度更新 |
