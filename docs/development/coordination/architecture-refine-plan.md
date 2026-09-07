# DataZen 架构重构与多包迁移 — 协调实施计划

> **PRD / 方案**：`docs/todo/architecture-refine.md`  
> **分类体系**：Driver / Theme / EP (Host Extension Point) / Workspace App + `@datazen/ui`  
> **Playbook**：`docs/development/subagent-dev-playbook.md` + `docs/development/subagent/`（Coordinator/Coder/Tester/Rescuer）  
> **集成分支**：`feat/sql-editor`  
> **私有增强仓**：`https://github.com/flyxl/datazen-extension-sql-pro`  
> **协调总览**：`docs/development/coordination/hub.md`（只读生成物，`node scripts/aggregate-hub.mjs`）  
> **轨目录**：`docs/development/coordination/tracks/<track-id>/`（各轨仅维护本轨 `progress.md` + `bugs.md`）

---

## 0. 角色与基线约定

| 角色 | 约束 |
|------|------|
| 协调者 | 不写业务代码；维护 hub；merge / worktree 清理；写锁台账；关键节点向用户同步；Coder 完成立即派发 Tester |
| 编码代理 | 仅在 `.worktrees/datazen-<track>`；`scripts/new-feature-worktree.sh <track> feat/sql-editor`；全新实例 |
| 测试代理 | 全新实例（禁复用 Coder）、独立 worktree；只测不修；零信任；覆盖率 ≥80% |
| 调度时序 | 某个 Coder 执行完返回 READY_FOR_TEST，协调者立即派发对应 Tester 验证，无需等待同 Wave 其他 Coder |
| 基线 | 所有同波次轨道以主集成分支（`feat/sql-editor`）最新 HEAD 为基准拉出 |

---

## 1. 波次编排

### Wave 1：核心包拆分与通用 UI 基础设施（2 轨并行）

| Track | 任务摘要 | 主要写路径 | 状态 |
|-------|----------|------------|------|
| **arch-p1-sdk** | 核心 SDK 拆分：创建 packages/extension-points 与 packages/driver-sdk，重命名 packages/app-sdk，配置别名与向后兼容 | `packages/driver-sdk/`、`packages/extension-points/`、`packages/app-sdk/`、`tsconfig.json` | **已完成** |
| **arch-p1-ui** | 通用设计系统：创建 packages/ui，抽离 Button/Input/Select/Dialog/Tabs/Badge/Label/cn，配置 ESM 导出与路径别名 | `packages/ui/`、`tsconfig.json`、`vite.config.ts` | **已完成** |

### Wave 2：私有增强仓沉淀与调用方平滑迁移（2 轨并行）

| Track | 任务摘要 | 主要写路径 | 状态 |
|-------|----------|------------|------|
| **arch-p2-pro** | 增强 SQLEditor 剥离与私有仓推送：将 intentions/hover/signatureHelp/joinCompletion 移至 flyxl/datazen-extension-sql-pro 私有仓，Host 仅留纯净 Fallback 扩展点插槽 | `flyxl/datazen-extension-sql-pro`、`src/components/sql-editor/` | **已完成** |
| **arch-p2-callers** | 调用方迁移：驱动包改用 @datazen/driver-sdk 与 @datazen/ui，更新 resolve-drivers.mjs 脚本与 Host 导入 | `packages/drivers/*/ui/`、`scripts/resolve-drivers.mjs`、`src/` | **已完成** |

### Wave 3：历史净化与规范收口（1 轨串行）

| Track | 任务摘要 | 主要写路径 | 状态 |
|-------|----------|------------|------|
| **arch-p3-finalize** | 净化 datazen git 历史（彻底清除 SQLEditor 增强代码痕迹），更新 AGENTS.md 与 LICENSE linking exception，全量回归验证 | `AGENTS.md`、`LICENSE`、全量测试套件 | **已完成** |
