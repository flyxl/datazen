---
name: subagent-coordinator
description: >-
  Orchestrate multi-track parallel feature development with subagents and git
  worktrees. Use when executing complex multi-feature tasks, large refactorings,
  or other work that benefits from parallel coding/testing agents in DataZen.
---

# Subagent Parallel Development Coordinator

本 Skill 指导主会话 Agent 作为**协调者 (Coordinator)**，负责统筹、拆轨、派发并合流多子代理并行开发与测试任务。

> **进度落文件，但随开发周期清理。** 各轨把自己的进度记录到该轨分支的 `progress.md`，协调者把跨轨汇总记录到集成分支的 `hub.md`；两者**提交进 git**，以便会话中断、重启或上下文压缩后仍可恢复。feature 开发完成并通过验收后，协调者删除这些进度文件并提交一次清理。设计结论与缺陷修复则必须进代码、测试与 `docs/` 的正式文档（`docs/features/`、`docs/architecture/`、`docs/development/`），不得只停留在进度文件里。
> 完整项目约定见 `AGENTS.md`。

## 核心硬性纪律

1. **协调者不亲自写大段业务代码**：专注架构分解、简报编写、状态推进与合流裁决。
2. **严防主线污染**：所有开发均在 `.worktrees/datazen-<track-id>` 中进行，主检出仅用于合流与清理。
3. **不派发不存在的简报模板**：简报由协调者现场编写（见 §简报七件），仓库内没有现成模板文件。

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
脚本会自动在 `.worktrees/datazen-<track-id>` 初始化分支、软链依赖、代码生成，并拷贝本地未跟踪文档（含 gitignored 的 `posts/`）。

### 3. 并行派发编码子代理 (Coder)
- **硬性要求：同一 Wave 的全部 Coder 必须在同一条消息中通过多个 subagent 工具调用并行派发，严禁串行逐个启动。**
- 每个 Coder 启动 **全新实例**。
- 必读清单必须包含：`AGENTS.md` + 该轨相关的正式文档 + 已侦察落点。
- 明确工作目录 `pwd` 为 `.worktrees/datazen-<track-id>`。
- 返回格式约定：commit hash / 改动清单 / 各套件实测数字 / 遗留项，状态词用 `READY_FOR_TEST`。

### 4. 即时测试派发 (Tester)
- **硬性要求：某个 Coder 返回 `READY_FOR_TEST` 后，协调者必须立即为该轨启动 Tester，无需等待同 Wave 其他 Coder 完成。**
- 若多个 Coder 同时返回，Tester 也必须在同一条消息中并行派发。
- 启动 **全新测试代理**（禁止复用编码代理——它有立场）。
- Tester 职责不仅是重跑编码代理的自测，还必须：
  1. **Review 实现逻辑**——审查代码变更是否正确、是否有边界遗漏。
  2. **覆盖率驱动的测试补齐**——确保改动代码覆盖率达标，精准补齐未覆盖的分支和路径，不追求数量。
  3. **设计 E2E 测试用例**——直接编写可执行的 E2E 测试，不登记待办。
- 测试代理只测不修。缺陷逐条写入**最终报告**（ID + 描述 + 复现步骤 + 日志），不落盘单文件。

### 4.1 Bug 修复循环
Tester 完成一轮完整测试后统一上报缺陷清单 + `TEST_FAILED`，协调者启动修复循环：

```text
Tester 完成完整测试 → 上报缺陷清单 + TEST_FAILED
→ 协调者指派原 Coder agent（resume）修复
→ Coder 修复并提交 → 协调者派发全新 Tester 完整复测
→ 通过 → 闭环 / 不通过 → 回到循环起点
```

**硬性规则**：
1. **完整上报**：Tester 在完成全部测试阶段后统一上报，不逐个打断。
2. **复用原 Coder**：修复阶段优先 resume **原编码 agent**，利用其已有上下文。仅当原 Coder 不可恢复时才启动全新 Rescuer。
3. **修复后必须复测**：Coder 的自验不能替代 Tester 复测，禁止跳过复测直接合入。
4. **全新 Tester**：每轮复测必须使用**全新 Tester 实例**。
5. **最大循环次数**：同一轨道最多 **5 轮**完整修复-复测循环。超过仍有未修复缺陷，向用户汇报并由用户决定下一步。
6. **修复 Coder 简报**：必须包含完整缺陷清单，以及「仅修复这些，不做范围外改动」的纪律约束。
7. **范围归属裁决**：既有缺陷是否并入本功能范围由协调者裁决——默认**不并入**已关闭功能，另立循环。

### 5. 活性监控与断点恢复
- 编码代理给予 20 分钟纯探索宽限期。
- 依据 `git -C <worktree> status --short`、系统进程和构建产物 mtime 判定活性。
- **死亡恢复**：
  - 死亡 ≤3 次：发消息 `"继续"` 原实例续跑（保留完整上下文与已有编辑，远优于新实例）。
  - 死亡 >3 次：启动全新**接管代理 (Rescuer)**：先用 `git status` 盘点现场未提交改动 → 审计已有 diff → 补缺口 → 验证 → 提交。

### 6. 合流与 Worktree 清理
当某轨测试通过闭环后，协调者在集成分支合流：
```bash
# 1. 合并功能分支
git merge --no-ff feature/<track-id> -m "feat(<track-id>): merge"

# 2. 健全性校验
pnpm typecheck
npx vitest run

# 3. 释放 worktree 及分支
git worktree remove .worktrees/datazen-<track-id>
git branch -d feature/<track-id>
```

### 7. 全量回归 (R 阶段)
**所有 Wave 的全部轨道合并完毕后，统一执行一次 R 阶段全量回归**（中间各 Wave 合入时仅做合并健全性校验）：
1. 跑通全套构建与测试：`pnpm typecheck`、`npx vitest run`、`cargo test -p datazen --lib`、必要的 E2E。
2. 逐项回归各轨在简报中标记为【留待 R 回归】的 E2E 用例。
3. 确认全部缺陷已闭环。

## 简报七件（质量的最大变量）

1. **工作目录与禁区**：worktree 绝对路径；明确禁止触碰主检出与其他轨道。
2. **必读清单**：项目约定（`AGENTS.md`）+ 相关正式文档，按序。
3. **任务与验收标准**：可逐条判定的完成标准，不接受模糊表述。
4. **已侦察落点**：协调者预先 grep 好的 `文件:行`——这是代理效率差异的最大变量；并声明「落点需自行核实」防盲从。
5. **执行纪律**：一律用 Grep 工具搜索（禁 bash 全仓 grep）；编辑前先 read（edit 工具强制要求）；文件被 git 操作或他人改动后必须重读。
6. **环境注意**：`node_modules` 是软链，禁 `pnpm install`，二进制用 `npx`；并行 cargo 轨各自设 `CARGO_TARGET_DIR`。
7. **返回格式**：commit hash / 改动清单 / 各套件数字 / 遗留。
