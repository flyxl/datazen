---
name: subagent-coordinator
description: >-
  Orchestrate multi-track parallel feature development with subagents, worktrees,
  and zero-conflict hub aggregation. Use when executing complex multi-feature tasks,
  large refactorings, or following the subagent-dev playbook in DataZen.
---

# Subagent Parallel Development Coordinator

本 Skill 指导主会话 Agent 作为**协调者 (Coordinator)**，负责统筹、拆轨、派发并合流多子代理并行开发与测试任务。

## 核心硬性纪律

1. **协调者不亲自写大段业务代码**：专注架构分解、简报编写、状态机推进与合流裁决。
2. **严防主线污染**：所有开发均在 `.worktrees/datazen-<track-id>` 中进行，主检出仅用于合流与清理。
3. **方案 B 零冲突规程**：子代理严禁触碰 `hub.md`；协调者统一使用 `node scripts/aggregate-hub.mjs` 聚合总览。

---

## 标准协调工作流

### 1. 波次与冲突面拆解
- 依据**文件冲突面**划分子任务轨道，而非功能相邻度。
- 触碰互斥文件或同一注册块不同行可并行（同一 Wave）；存在核心依赖或模式复用软依赖必须串行（分 Wave）。
- 确定基准集成分支（如 `feat/<initiative-name>`）。

### 2. 准备并行轨道 (Bootstrap)
对于当前波次的每个 `<track-id>`，在主检出执行：
```bash
scripts/new-feature-worktree.sh <track-id> <base-branch>
```
脚本会自动在 `.worktrees/datazen-<track-id>` 初始化分支、软链依赖及代码生成。

### 3. 并行派发编码子代理 (Coder)
- 模板参考：`docs/development/subagent/templates/coder-brief.md`。
- **硬性要求：同一 Wave 的全部 Coder 必须在同一条消息中通过多个 `Task` 工具调用并行派发，严禁串行逐个启动。**
- 每个 Coder 启动 **全新实例**（subagent_type="generalPurpose"）。
- 必读清单必须包含：
  1. `AGENTS.md`
  2. `docs/development/subagent/coder.md`（编码代理专享规程）
  3. `docs/development/coordination/tracks/<track-id>/progress.md`
- 明确工作目录 `pwd` 为 `.worktrees/datazen-<track-id>`。

### 4. 即时测试派发 (Tester)
- **硬性要求：某个 Coder 返回 `READY_FOR_TEST` 后，协调者必须立即为该轨启动 Tester，无需等待同 Wave 其他 Coder 完成。**
- 若多个 Coder 同时返回，Tester 也必须在同一条消息中并行派发。
- 启动 **全新测试代理**（禁止复用编码代理）。
- 模板参考：`docs/development/subagent/templates/tester-brief.md`，必读 `docs/development/subagent/tester.md`。
- Tester 职责不仅是重跑编码代理的自测，还必须：
  1. **Review 编码代理的实现逻辑**——审查代码变更是否正确、是否有边界遗漏。
  2. **覆盖率驱动的测试补齐**——确保改动代码测试覆盖率 ≥ 80%，精准补齐未覆盖的分支和路径，不追求数量。
  3. **设计 E2E 测试用例**——在 progress.md 登记或直接编写可执行的 E2E 测试。
- 测试代理只测不修。若出现缺陷，只在 `tracks/<track-id>/bugs.md` 登记；全部通过后才置 `TEST_DONE`。

### 4.1 Bug 修复循环（Tester 完整上报后的闭环流程）
Tester 必须先完成一轮**完整测试**（4 阶段 A/B/C/D 全部跑完），最终将所有发现的 Bug 一并在 `bugs.md` 登记，并以 `TEST_FAILED` 状态统一上报。协调者收到上报后启动修复循环：

```text
[Bug 修复循环]
Tester 完成完整测试 → 一并上报 Bug 清单 + TEST_FAILED
→ 协调者指派原 Coder agent（resume）修复全部 Bug
→ Coder 修复并提交 → 协调者派发全新 Tester 完整复测
→ 通过 → 闭环 / 不通过 → 回到循环起点
```

**硬性规则**：
1. **完整上报**：Tester 不逐个上报 Bug，而是在完成全部测试阶段后统一上报 Bug 清单和 `TEST_FAILED` 状态。
2. **复用原 Coder**：修复阶段优先使用 `Task` 工具的 `resume` 参数恢复**原编码 Coder agent**，利用其已有上下文高效修复。仅当原 Coder 不可恢复时才启动全新 Rescuer。
3. **全新 Tester**：每轮复测必须使用**全新 Tester 实例**（不复用前轮 Tester）。
4. **最大循环次数**：同一轨道最多允许 **5 轮完整修复-复测循环**。超过 5 轮仍有未修复 Bug，协调者须将其标记为 `ESCALATED` 并向用户汇报，由用户决定下一步。
5. **Bug 状态流转**：`待修复` → `修复中`（Coder 接手）→ `待复测`（Coder 提交）→ `已修复`（Tester 通过）或回到 `待修复`（Tester 不通过）。
6. **修复 Coder 简报**：必须包含完整 Bug 清单（ID + 描述 + 重现步骤 + 日志），以及"仅修复这些 Bug，不做范围外改动"的纪律约束。

### 5. 活性监控与断点恢复
- 编码代理给予 20 分钟纯探索宽限期。
- 依据 `git -C <worktree> status --short`、系统进程和构建产物 mtime 判定活性。
- **死亡恢复**：
  - 死亡 ≤3 次：发消息 `"继续"` 原实例续跑。
  - 死亡 >3 次：启动全新**接管代理 (Rescuer)**，参考 `docs/development/subagent/rescuer.md`，先盘点现场未提交改动再补齐。

### 6. 合流、聚合与 Worktree 清理
当某轨测试通过闭环后，协调者在集成分支合流：
```bash
# 1. 合并功能分支
git merge --no-ff feature/<track-id> -m "feat(coordination): merge track <track-id>"

# 2. 健全性校验
npx tsc --noEmit
cargo test -p datazen --lib

# 3. 运行方案 B 聚合脚本更新总览
node scripts/aggregate-hub.mjs

# 4. 释放写锁并清理 worktree 及分支
git worktree remove .worktrees/datazen-<track-id>
git branch -d feature/<track-id>
```

### 7. 全量回归 (R 阶段)
**所有 Wave 的全部轨道合并完毕后，统一执行一次 R 阶段全量回归**（中间各 Wave 合入时仅做合并健全性校验，不做完整回归）：
1. 跑通全套构建与测试：`pnpm build`（或 `npx tsc --noEmit`）、`cargo test -p datazen --lib`、前端单测。
2. 逐项回归**所有 Wave 所有轨道**在 `progress.md` 登记的【留待 R 回归】E2E 用例。
3. 确保所有 Bug 均已关闭。
