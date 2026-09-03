# 子代理并行开发 Playbook

> 来源：2026-08 IPC 重构实战沉淀，本手册是多功能、长周期需求下子代理并行开发与测试的工程化规程。  
> **模块化重构通知**：为减少子代理上下文 Token 消耗并消灭合并冲突，本 Playbook 已按角色拆分至 `docs/development/subagent/`。子代理派发时**请勿通读本文档**，只需根据角色按需阅读对应规程。

---

## 快速导航与角色规程

- 体系架构总览：[docs/development/subagent/README.md](subagent/README.md)
- **协调者 (Coordinator) 规程**：[docs/development/subagent/coordinator.md](subagent/coordinator.md)
  - 核心职责：波次编排、Worktree 创建、派发、活性监控、合流仲裁与清理。
  - 进度聚合：运行 `node scripts/aggregate-hub.mjs` 自动生成总览。
- **编码子代理 (Coder) 规程**：[docs/development/subagent/coder.md](subagent/coder.md)
  - 核心职责：全新实例、独立 Worktree、单功能实现 + 单测自验。严禁修改主检出与 hub.md。
- **测试子代理 (Tester) 规程**：[docs/development/subagent/tester.md](subagent/tester.md)
  - 核心职责：独立全新实例、只测不修、零信任复验、E2E 登记与 Bug 记录。
- **接管代理 (Rescuer) 规程**：[docs/development/subagent/rescuer.md](subagent/rescuer.md)
  - 核心职责：现场盘点未提交改动、分段审计 diff、最小补丁修复。
- **派发简报模板**：
  - [编码简报模板](subagent/templates/coder-brief.md)
  - [测试简报模板](subagent/templates/tester-brief.md)
  - [接管简报模板](subagent/templates/rescuer-brief.md)

---

## 核心状态机与工作流

```text
[功能生命周期]
编码(+单测) → commit → 测试 ─┬→ 通过 → 功能「已完成」→ commit (TEST_DONE)
                            └→ 不通过 → bug 登记「待验证」→ 协调者置「验证不通过」+commit
                                      → 修复代理修复 → 「待验证」+commit
                                      → 全新测试代理复验 → 通过：「已修复」+功能「已完成」+commit

[阶段流转]
DISPATCHED → BOOTSTRAP → CODING → READY_FOR_TEST ─┬→ TESTING → PASSED → READY_TO_MERGE → MERGED → CLEANUP → CLOSED
                                                  └→ FAILED → BUG_RECORDED → REPAIRING → TESTING
```

---

## 方案 B：零冲突进度架构

各功能分支只维护自身独立目录 `docs/development/coordination/tracks/<track-id>/`（包含 `progress.md` 与 `bugs.md`），严禁修改或提交公共总览 `hub.md`。

各分支合并时路径完全正交（0 冲突）。协调者在主检出运行以下命令实时聚合总览：

```bash
node scripts/aggregate-hub.mjs
```
