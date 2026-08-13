# Data Synchronization（对标 Navicat Diff Sync）

> 分支：`feat/data-sync-navicat`  
> Worktree：`/Users/wuxiaolong/code/rust-projects/datazen/.worktrees/data-sync-navicat`  
> 规格：`docs/data-synchronization-prd.zh-CN.md` V1.2  
> 方案：`docs/data-synchronization-implementation-plan.zh-CN.md`

本次只做 **Data Synchronization**（结构完全一致 + 相同 PK）。不实现 Transfer。

## 功能清单

| ID | 功能 | 状态 | 单测 | 独立测试 agent | 覆盖率 | Commit |
|----|------|------|------|----------------|--------|--------|
| F1 | `data_sync` 领域模型 + 状态机 + ChangeSet 选择规则 | done | 30 PASS | [F1 QA](45f474d3-e7da-4c9a-b593-ce3d61ce5f80) PASS | 99.31% | `42ea3d1` |
| F2 | 同族 pairing + 结构完全一致 + 相同 PK 硬门闸 | done | 53 PASS | [F2 QA](3db2440f-4484-442c-bbe6-30229916c515) PASS | 99.49% | `8518c32` |
| F3 | 停用旧 DROP+INSERT `sync_tables` 产品路径 | done | Rust+Vitest PASS | [F3 QA](1664c61a-a915-4a67-b603-e61a0dc06156) PASS | Rust 100% / TS 86.11% | 待提交 |
| F4 | Host 流式 PK 比较（mock driver / keyset 编排） | pending | — | — | — | — |
| F5 | ChangeSet → 参数化 INSERT/UPDATE/DELETE SQL | pending | — | — | — | — |
| F6 | 专用 Execute IPC（read_only / 事务 / Cancel） | pending | — | — | — | — |
| F7 | Compare → Apply → Recompare=0 编排闭环 | pending | — | — | — | — |
| F8 | DataSyncWindow Diff Workspace（最小可用） | pending | — | — | — | — |
| F9 | i18n + Host E2E 契约 journey | pending | — | — | — | — |
| F10 | 拆除旧引擎残留 + 架构/AGENTS 文档 + merge main | pending | — | — | — | — |

## 测试约定

- 开发 agent 写单元测试，不充当验收 agent。
- 每完成一个功能，**新开**测试 agent：只输出 E2E 用例、测试结果、bug 重现步骤；不修复。
- 变更脚本/模块行覆盖率 ≥ 80%。
- 测试不通过 → 另开编码 agent 修复 → 再开新测试 agent 回归。
- 测试通过后才 commit。

## F1 备注

- 模块：`src-tauri/src/data_sync/`
- 禁止 `COMPARING → EXECUTE*`
- ChangeSet 只含已勾选且被 options 允许的行；DELETE 默认不选
- 同一库（同 connection + database + schema）自同步禁止
