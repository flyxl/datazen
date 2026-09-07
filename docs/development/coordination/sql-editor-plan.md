# SQL Editor 升级 — 协调实施计划

> **PRD**：`docs/todo/sql-editor/prd.md`  
> **完整实施方案**：`docs/todo/sql-editor/implementation-plan.md`  
> **Playbook**：`docs/development/subagent-dev-playbook.md` + `docs/development/subagent/`（Coordinator/Coder/Tester/Rescuer）  
> **集成分支**：`feat/sql-editor`  
> **协调总览**：`docs/development/coordination/hub.md`（只读生成物，`node scripts/aggregate-hub.mjs`）  
> **轨目录**：`docs/development/coordination/tracks/<track-id>/`（各轨仅维护本轨 `progress.md` + `bugs.md`）

---

## 0. 角色与基线约定

| 角色 | 约束 |
|------|------|
| 协调者 | 不写业务代码；维护 hub；merge / worktree 清理；写锁台账；关键节点向用户同步 |
| 编码代理 | 仅在 `.worktrees/datazen-<track>`；`scripts/new-feature-worktree.sh <track> feat/sql-editor`；全新实例 |
| 测试代理 | 全新实例（禁复用 Coder）、独立 worktree；只测不修；零信任；覆盖率 ≥80% |
| 基线 | 所有同波次轨道以主集成分支（`feat/sql-editor`）最新 HEAD 为基准拉出 |

---

## 1. 波次编排

### Stage 1：等价拆分热点容器

#### Wave 1A（3 轨并行：SqlEditor + QueryPanel 壳层拆分 + 测试夹具）

| Track | 任务摘要 | 主要写路径 | 状态 |
|-------|----------|------------|------|
| **sql-s1-a** | SqlEditor 壳层拆分：提取 contracts 与 editorExtensions，拆入小 factory，保持快捷键/drop 完全等价，生产文件 <500 行 | `src/components/SqlEditor.tsx`、`src/components/sql-editor/` | **未开始** |
| **sql-s1-b** | QueryPanel 壳层拆分：剥离侧栏、事务弹窗与编辑器装配，主文件降至 ~300 行，复用既有 aiQueryActions 与 result-workspace | `src/windows/connection/QueryPanel.tsx`、`src/windows/connection/query/` | **未开始** |
| **sql-s1-c** | E2E 与性能夹具：封装稳定 helper/selector，创建 20,000 行确定性 SQL fixture，建立性能测量 harness 与 coverage 配置 | 新 E2E helper、fixtures 及 Vitest coverage 配置 | **未开始** |

#### Wave 1B（1 轨串行：i18n key 契约，待 Wave 1A 合流后启动）

| Track | 任务摘要 | 主要写路径 | 状态 |
|-------|----------|------------|------|
| **sql-s1-d** | 一次性定义本项目所需的 AC-01～AC-21 英文/中文翻译 key 契约，保证 TranslationKey 可编译，作为下游唯一翻译来源 | `src/locales/en/query.ts`、`src/locales/zh-CN/query.ts` 等 | **未开始** |
