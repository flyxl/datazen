# ID 术语统一重构 — 进度管理文件

> 分支：`feature/db-session-id-rename`（worktree：`.worktrees/db-session-id-rename`）
> 约定：**`connectionId` = 配置连接 id（持久化，原 `configId`）**；**`dbSessionId` = 运行时数据库会话 id（内存态，原 `connectionId`）**
> 流程：每个工作项 = 编码 agent 开发（含单测）→ commit → 全新测试 agent 测试（输出 E2E 用例与结果、覆盖率 ≥80%，只测不修）→ commit → bug 按 `验证不通过 → 待验证 → 已修复` 闭环。

## 一、功能工作项

| # | 工作项 | 范围 | 状态 | 完成时间 | 备注 |
|---|--------|------|------|----------|------|
| W1 | 后端核心与服务层术语落地 | `services/connection_manager.rs` 内部命名（`config_id_map`→`session_owner_map` 等）、错误信息区分两种 id；IPC 契约暂不变 | ✅ **已完成（测试通过，0 缺陷）** | 2026-08-24 | 26 文件 +482/-278；核心模块行覆盖 86.94%~96.74%（≥80% 达标）；185 个 IPC 命令签名零变化；报告见 `test-reports/W1-test-report.md` |
| W2 | IPC 契约切换（前后端原子批） | Tauri 命令参数改名 + `src/commands/*` 封装同步 + 全部前后端调用点 + **MCP 参数直接改名 `connection_id`（不留 `config_id` 别名）** + **SQLite 列与持久化字段直接改名**（历史库按既有迁移模式做一次性列重命名） | ❌ 测试不通过（BUG-001~003 闭环中） | - | 开发 b962b4cc；独立测试发现 13 命令语义装反、connection.rs 覆盖率 65.37%<80%、缺 v3 迁移回归测试；详见决策 D3、Bug 表 |
| W3 | 前端状态/类型/组件改名 | `types/index.ts`、stores、`connectionViews/types.ts`、组件 props、跨窗口事件 payload、windowManager、extensionBridge 显式目标 | 未开始 | - | |
| W4 | 外部契约与文档对齐 | MCP 资源输出字段/tool_help 文档、CHANGELOG 破坏性变更记录（D1）、ops-dashboard 指南等 configId 表述替换 | 未开始 | - | SQLite 列已随 W2 直接改名（D1），原"列名保留"子项作废 |
| W5 | 文档与守护 | `docs/architecture/naming.md`、AGENTS.md 精简更新、lint/grep 守护规则 | 未开始 | - | |

收尾：回归测试 → 文档更新 → 合并 main。

## 二、Bug 跟踪

状态流转：`验证不通过`（测试发现）→ `待验证`（编码 agent 已修复）→ `已修复`（测试 agent 验证通过）

| Bug ID | 发现于 | 描述 | 重现步骤 | 状态 | 关联工作项 |
|--------|--------|------|----------|------|------------|
| BUG-001 | W2 独立测试 T2 | **13 个命令语义装反**：ai 域 7 个 + backup 域 6 个命令，参数名 `connection_id`（配置语义）实际按运行时会话语义使用（直调 `get_session`、无 resolve_session 双模）；当前靠前端把 dbSessionId 装在 connectionId 键里碰巧可用，W3 改造时必断 | 详见 `test-reports/W2-test-report.md` §T2 缺陷 D1（含 13 个命令清单） | 验证不通过 | W2 |
| BUG-002 | W2 独立测试 T3 | `commands/connection.rs` 行覆盖率 **65.37% < 80%** 验收线（其余核心模块达标：history_db 91.14%、mcp/server 88.11%、driver_command 81.95%、query 80.62%） | `cargo llvm-cov -p datazen --lib --summary-only` 复测 | 验证不通过 | W2 |
| BUG-003 | W2 独立测试 T2/G1 | history_db 迁移链缺**存量 v3 库（config_id 列）起点**的固化回归测试（走读与三起点 SQL 等价执行均 PASS，但无自动化测试保护） | 报告 §T2 缺口 G1 | 验证不通过 | W2 |
| OBS-001 | W2 独立测试 T1 | store 并发用例负载型偶发失败（隔离重跑 5/5 过；代码 vs main 零改动；main 基线同现象）——既有环境波动，不计缺陷 | 隔离单跑该用例即可复现通过 | 观察 | - |

## 三、测试记录

### E2E / 单元测试用例与结果

| 轮次 | 工作项 | 用例摘要 | 结果 | 覆盖率 | 测试 agent |
|------|--------|----------|------|--------|------------|
| D1（开发自测） | W1 | 不变式：驱逐自动重连保持 db_session_id；resolve_session 双模解析；错误文案区分两种 id | `cargo test -p datazen --lib`：1115 通过 / 2 失败（既有 sandbox 环境问题，干净 HEAD 复现，非本次引入） | 待独立测试 agent 评估 | 编码 agent ad05a2ae |
| T1（独立测试） | W1 | 10 条 E2E 视角用例（6 条已在 Rust 单元/IPC 等价层实际执行通过）；独立复核 lib 测试 1115 过 / 2 失败（既有环境问题，基线实机比对确认） | **通过**；llvm-cov 行覆盖：connection_manager 86.94%、db_tools 96.74%、query_executor 89.40%（核心模块 ≥80% 达标，TOTAL 77.03%）；缺陷 0 | 全新测试 agent 1c7f5916 |
| D2（开发自测） | W2 | 九域 IPC 契约切换；history_db v4 迁移环；MCP 旧键拒绝守护测试 | cargo lib：1115 过 / 2 既有失败；vitest **239 文件 / 1886 用例全绿**；build 分步全过（pnpm 包装器受 sandbox node-gyp 缓存 EPERM 限制，逐条等价执行成功） | 待独立测试 agent 评估 | 编码 agent 977851e6 |
| T2（独立测试） | W2 | 12 条 E2E 视角用例全部给出状态；Rust 层定向 29 单测全过 + 迁移三起点 SQL 等价执行 PASS；语义方向审计发现 BUG-001 | **不通过**：BUG-001 语义装反 ×13、BUG-002 connection.rs 覆盖率 65.37%<80%、BUG-003 缺 v3 迁移回归测试；其余全绿（lib 1114 过/vitest 1886 绿/drivers 84 绿/tsc+vite ✓）；复测条件=①D1 修复+守护测试 ②覆盖率≥80% ③e2e:minimal | 全新测试 agent dc7ba786，报告 `test-reports/W2-test-report.md` |

## 四、提交记录

| Commit | 说明 |
|--------|------|
| c76118f9 | 进度文件初始化 |
| f0aa9882 | W1 开发里程碑：后端核心/服务层改名 + 单测 + 进度更新 |
| 09b9d5cc | W1 测试里程碑：独立测试报告（通过，0 缺陷，覆盖率达标）+ 进度更新 |
| b962b4cc | W2 开发里程碑：九域 IPC 契约切换 + history_db v4 迁移 + MCP 无别名直改 + 驱动 UI 单测补跑（84/84）+ 进度更新（含 D3 附带变更） |
| （本次） | W2 测试里程碑：独立测试**不通过**，登记 BUG-001/002/003（验证不通过）+ 测试报告 + 进度更新 |

## 五、决策记录

| # | 日期 | 决策 | 来源 |
|---|------|------|------|
| D1 | 2026-08-24 | W2 范围修订：① MCP 工具参数直接改名 `connection_id`，**不**保留 `config_id` 兼容别名（接受外部契约破坏性变更，W4 文档中记录 CHANGELOG）；② SQLite 列名与持久化/配置文件字段名直接改为新术语，**不**做双轨兼容；`query_history`/`favorite_queries` 沿用 history_db 既有的一次性列重命名迁移模式处理存量库。原「保留别名 + 保留旧列名」方案作废。 | 用户指示 |
| D2 | 2026-08-24 | 合并冲突策略：main ↔ feature 双向合并遇冲突时，**两边修改都要保留**——把 main 侧的新逻辑/修复按本分支新术语（connectionId/dbSessionId）适配后融入，同时不丢失本分支的改名成果；不做"二选一"式解决。无法字面并存处（如同名标识符）以语义融合方式落地，并在进度文件登记具体取舍。收尾顺序：回归通过 → main 合入 feature 并复验编译/测试 → 文档更新 → feature 合回 main。 | 用户指示 |
| D3 | 2026-08-24 | W2 附带变更两项：① `packages/drivers/redis/ui` 16 文件将 invoke 参数键提前改为 `dbSessionId`（经用户许可的直接改名，宿主 ConnectionViewProps 契约保持至 W3）；② 修复既有环境缺陷 `src/test/setup.ts`（jest-dom 副作用导入在本环境注册为空匹配器集，干净 HEAD 上即导致 285 用例 "Invalid Chai property" 失败），改为显式 `expect.extend`，全量转绿。 | 编码 agent 报告 |
