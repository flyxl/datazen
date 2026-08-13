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
| F3 | 停用旧 DROP+INSERT `sync_tables` 产品路径 | done | Rust+Vitest PASS | [F3 QA](1664c61a-a915-4a67-b603-e61a0dc06156) PASS | Rust 100% / TS 86.11% | `d35e6a0` |
| F4 | Host 流式 PK 比较（mock driver / keyset 编排） | done | 8 PASS | [F4 QA](e5f4b08e-a702-4ae4-a9a2-9ac3286b0332) PASS | 92.97% | `fe97153` |
| F5 | ChangeSet → 参数化 INSERT/UPDATE/DELETE SQL | done | 5 PASS | [F5 QA](745899af-3f19-4929-b4a9-3f47130e79d1) PASS | 92.84% | `d294ed6` |
| F6 | 专用 Execute IPC（read_only / 事务 / Cancel） | done | 7 PASS | [F6 QA](a2e49ad5-2211-4ad5-8435-9e7df8c1e0c6) PASS（Cancel IPC 未接线记 S3） | execute.rs 98.79% | `be8e36e` |
| F7 | Compare → Apply → Recompare=0 编排闭环 | done | 2 PASS | [F7 QA](59efff33-1814-46e9-8c90-e5edbdb28956) PASS | 95.08% | `aad15e6` |
| F8 | DataSyncWindow Diff Workspace（最小可用） | done | inspect+mappingView PASS | [F8 QA](a56d3e68-490a-4f4e-afd8-0067564090c7) PASS；已修 Select All / footer total | inspect 92.41% / mappingView 92.85% | `dfa2870` |
| F9 | i18n + Host E2E journey + Diff Workspace 壳 | done | DataSyncWindow+locales+mappingView PASS | [F9 QA](a9c637b7-1248-4398-b8a1-1cb9646778ff) PASS（E2E 无应用 BLOCKED） | window 92.15% / mappingView 92.85% | `5c0a05f` |
| F10 | 拆除旧引擎残留 + 架构/AGENTS 文档 + merge main | done | 25 PASS | [F10 QA](c0a1dfc3-7bbb-4dac-ba79-fc3925b64bfb) PASS（retest；E2E 无应用 BLOCKED） | table_sync.rs 100% | 待提交 |

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
