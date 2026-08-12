# Host Contract × Driver 覆盖 — 进度管理

> 分支：`feat/e2e-ui-path-coverage`  
> 工作区：`/Users/wuxiaolong/code/rust-projects/datazen-e2e-coverage`  
> 流程：开发(+单测) → **新**测试 Agent →（失败则编码 Agent 修复）→ 提交 → 下一功能

## 状态图例

| 状态 | 含义 |
|------|------|
| `todo` | 未开始 |
| `doing` | 开发中 |
| `test` | 等待/进行独立测试 Agent |
| `fix` | 测试失败，修复中 |
| `done` | 测试通过且已提交 |
| `blocked` | 阻塞 |

## 功能清单

| ID | 功能 | 单测要求 | 状态 | Commit | 测试 Agent 报告 |
|----|------|----------|------|--------|-----------------|
| F0 | Host UI 覆盖规则文档 + 既有缺口 Host E2E（筛选/索引/备份等） | FilterEditor 既有单测保持绿 | `done` | `d062bf2` | [复测](a07d8a2b-b28d-43b1-8f35-072d7740abde)：PASS_WITH_SKIPPED_E2E，覆盖率 88.98% |
| F1 | `DriverFixture` 类型 + PG/MySQL/SQLite 夹具 + dialect seed helpers | 覆盖率 ≥80% | `done` | （本提交） | [F1](eb97a24a-26b1-4ee5-975b-39b44c328573)：PASS 覆盖率 92.15% |
| F2 | Host Contract journeys：HC-DATA / HC-FILTER / HC-QUERY + 矩阵入口 | journey 纯逻辑单测 ≥80% | `doing` | — | — |
| F3 | 其余契约：HC-CONN / EDIT / STRUCT / INDEX / EXPORT / OBJ / EXPLAIN + 能力门控 | 门控与 skip 逻辑单测 ≥80% | `todo` | — | — |
| F4 | `pnpm e2e:contract:*` 脚本、CI 分流说明、旧 `mysql.ts` 与契约去重 | 脚本/清单单测或静态校验 | `todo` | — | — |
| F5 | 更新 AGENTS.md / e2e-coverage / architecture；merge → main；push | — | `todo` | — | — |

## 当前焦点

- **正在进行**：F2 journeys + matrix

## Bug 记录

| ID | 发现于 | 复现步骤 | 状态 |
|----|--------|----------|------|
| BUG-COV-001 | F0 | FilterEditor 覆盖率不足 | `fixed` |
| BUG-E2E-001 | F0 | IDX 顺序问题 | `fixed` |
| BUG-E2E-002 | F0 | TF-004 选列脆弱 | `fixed` |
| BUG-E2E-003 | F0 | PRV-001 断言过弱 | `fixed` |

## 变更日志

| 日期 | 事件 |
|------|------|
| 2026-08-13 | 创建本进度文件；确认 F0–F5 范围 |
| 2026-08-13 | F0 done (d062bf2)；F1 PASS |
