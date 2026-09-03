# 子代理并行开发 Playbook 体系

> 适用：多功能、长周期需求，由主会话代理担任**协调者**，派发子代理完成并行开发与独立测试。  
> 核心目标：最大化并行吞吐，杜绝主线污染与合并冲突，各角色上下文 Token 最小化。

## 1. 角色模型

| 角色 | 专享手册 | 职责 | 核心红线 |
|------|---------|------|---------|
| **协调者** (Coordinator) | [coordinator.md](coordinator.md) | 拆功能、写简报、派发、活性监控、写锁仲裁、合流与清理、聚合总览 | 不直接写业务代码；每个关键节点向用户同步 |
| **编码子代理** (Coder) | [coder.md](coder.md) | 单功能实现 + 单元测试 + 自验三件套 + 维护本轨进度 + commit | 每个功能**全新实例**；只在分配的 worktree 工作；禁碰 hub.md |
| **测试子代理** (Tester) | [tester.md](tester.md) | 独立复验（重跑套件、实测覆盖率、E2E 登记、Bug 登记） | **必须全新实例，禁止复用编码代理**；只测不修；不信任自报数字 |
| **接管代理** (Rescuer) | [rescuer.md](rescuer.md) | 断点收尾、现场盘点、补齐验收缺口 | 全新实例；审计已有 diff；单一机制治理而非叠补丁 |

## 2. 状态机与单功能循环

```text
[功能循环]
编码(+单测) → commit → 测试 ─┬→ 通过 → 功能「已完成」→ commit (TEST_DONE)
                            └→ 不通过 → bug 登记「待验证」→ 协调者置「验证不通过」+commit
                                      → 修复代理修复 → 「待验证」+commit
                                      → 全新测试代理复验 → 通过：「已修复」+功能「已完成」+commit

[阶段状态机]
DISPATCHED → BOOTSTRAP → CODING → READY_FOR_TEST ─┬→ TESTING → PASSED → READY_TO_MERGE → MERGED → CLEANUP → CLOSED
                                                  └→ FAILED → BUG_RECORDED → REPAIRING → TESTING
```

## 3. 方案 B：零冲突进度与 Bug 架构

为了彻底消除多个 feature 分支合并到集成分支时的 `hub.md` 冲突，采用**去中心化原子文件 + 单向聚合生成**：

```text
docs/development/coordination/
├── hub.md                              # 【只读生成物】由 node scripts/aggregate-hub.mjs 自动生成
└── tracks/
    ├── <track-id>/
    │   ├── progress.md                 # 各轨独立维护：状态机、commit hash、心跳、自验结果
    │   └── bugs.md                     # 各轨独立维护：<track-id>-BUG-nnn 清单
```

- **分支隔离原则**：各轨分支只提交本轨 `tracks/<track-id>/` 目录，禁止修改或提交 `hub.md`。
- **合并零冲突**：各分支路径正交，`git merge` 时 0 冲突。
- **一键聚合**：协调者在合并后或需要时在主检出运行 `node scripts/aggregate-hub.mjs`，自动刷新 `hub.md`。

## 4. 简报模板目录

- [编码简报模板 (coder-brief.md)](templates/coder-brief.md)
- [测试简报模板 (tester-brief.md)](templates/tester-brief.md)
- [接管简报模板 (rescuer-brief.md)](templates/rescuer-brief.md)
