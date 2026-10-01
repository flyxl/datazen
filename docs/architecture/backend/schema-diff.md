# Schema Diff 架构

> Source of truth: `src-tauri/src/schema_diff/`, `src-tauri/src/commands/schema_diff.rs`, `packages/driver-api/src/schema_migration.rs`。

Schema Diff 的职责是把源库结构视为 **desired state**，比较目标结构，生成可审阅的迁移计划，并在目标库执行。

## 1. 执行链

```text
Source / Target schema snapshots
          ↓
       compare.rs
          ↓
   MigrationOperation
   (dialect-neutral)
          ↓
    dependencies.rs
          ↓
        plan.rs
          ↓
Driver MigrationRenderer
          ↓
MigrationStatement / Plan
          ↓
Review
          ↓
      deploy.rs
```

Host 不直接拼接 PostgreSQL/MySQL/SQLite 的 DDL。方言实现位于 Driver API 和具体 Driver。

## 2. Backend

| 文件 | 职责 |
|---|---|
| `schema_diff/compare.rs` | 列、PK、索引差异；支持 `TypeNormalizer` |
| `schema_diff/ir.rs` | Snapshot → `MigrationOperation` |
| `schema_diff/operations.rs` | 方言无关操作及风险 |
| `schema_diff/dependencies.rs` | 操作依赖与执行顺序 |
| `schema_diff/plan.rs` | 能力检查、DDL 渲染、计划生成 |
| `schema_diff/deploy.rs` | 目标库部署及事务/部分失败结果 |
| `schema_diff/types.rs` | Snapshot、Diff、Plan DTO |
| `commands/schema_diff.rs` | Tauri IPC 边界 |

Driver API 中的 `MigrationRenderer` 负责把操作转换为 `MigrationStatement`；`MigrationCapabilities` 描述是否支持某操作、是否需要 rebuild、DDL 是否事务化；`TypeNormalizer` 用于跨 Driver 类型别名比较。

## 3. Diff 方向

**Source = desired，Target = apply site。**

例如：

```text
source: users.name VARCHAR(255)
target: users.name VARCHAR(100)

→ AlterColumnType
  from = VARCHAR(100)
  to   = VARCHAR(255)

→ Driver renderer
→ ALTER ... 使 target 达到 source
```

目标多出的列/索引会产生 Drop 操作；这些操作属于 destructive risk。

当前 column diff 检查：

- data type
- nullable
- primary-key membership
- default
- comment
- auto increment

Index diff 检查：

- index name
- columns
- uniqueness

Primary key 使用 effective primary keys 生成独立的 Drop/Add 操作。

## 4. 风险

`MigrationOperation::risk()` 将操作归为：

- `Additive`
- `Rewrite`
- `Destructive`

Plan 阶段结合 Driver capability 判断 unsupported/rebuild 等情况。Deploy 前端负责显式确认破坏性操作。

## 5. Frontend

Schema Diff UI 位于 `src/windows/schema-diff/`，当前为：

```text
Endpoints
  → Objects
  → Compare
  → Plan
  → Deploy
```

主要组件：

- `SchemaDiffWindow`
- `SchemaDiffEndpointsBar`
- `SchemaDiffObjectsStep`
- `SchemaDiffTableListPanel`
- `SchemaDiffPlanPanel`
- `SchemaDiffDeployPanel`

## 6. 连接与目标

Schema Diff 的 IPC **全部**收成对的 `sourceDbSessionId` / `targetDbSessionId`：`commands/schema_diff.rs` 里没有任何一条路径会建立会话，比较、应用、预览全部消费前端已经建好的会话。

- **专用会话由前端建、由前端放**。源端与目标端各自持有一条专用会话：端点组件用 `ensureDedicatedSession` 调 `connectDedicated` 建立，并在端点或目标库变化、以及组件卸载时用 `releaseDedicatedSession` 释放上一条（走的是与普通释放同一套引用计数，计数归零才真正断开）。释放发生在建立新会话**之前**，不会新旧两条同时挂着。端点没变时先 `ping` 探活，探活失败才重连。后端只做 `get_session` / `get_session_config` 查找。
- **profile 落盘的是 `connectionId`**。加密的 profile 文件保存 `sourceConnectionId` / `targetConnectionId`，不保存任何运行时会话 ID；Deploy 的历史记录写入的同样是解析出来的 `connectionId`。
- **反查依赖 owner 映射**。部署时把目标会话反查成 `connectionId` 以便落库；反查失败即中止部署并报「目标连接归属不可用」，不会退化成写一个空归属。Idle 回收故意保留 owner 映射正是为了让这条反查在会话物理连接消失后仍然可用。
- **目标库在建连时就定死，不切库**。专用会话建立时把用户选中的库作为连接覆盖传下去（SQLite 例外：`main` 是目录别名，会传文件路径而不是 `main`），因此会话始终停在选中的库上，宿主不需要切库路径。
- **只有部署阶段可取消，比较阶段不可**。`cancel_schema_diff_deploy` 收一个 `jobId`，写的是与 Data Sync / Data Transfer 同一个进程级 job 标志（`services/job_registry.rs`），后端不做任何归属校验，因此谁拿到该 jobId 都能取消；部署执行时取出这个标志并观察它（`commands/schema_diff.rs:2177-2180`）。比较（compare）本身没有对应的取消命令，只能靠关闭面板让前端释放专用会话。

与另两个成员的边界：Schema Diff 负责**结构差异对比与 DDL 迁移生成**；同族数据复制属于 Data Synchronization，异构数据搬迁属于 Data Transfer。三者共用「专用会话 + 持久化 connectionId」的约束，但各自的会话由谁建立、profile 存什么并不相同。三者的取消也都落到同一张 job 标志表上，但只有 Schema Diff 的取消点局限在部署阶段。

## 7. Tests

- Rust unit tests：`src-tauri/src/schema_diff/**`
- Frontend：`src/windows/schema-diff/__tests__/`
- E2E：`e2e/specs/schema-diff-*.ts` 与 journey tests

Driver-specific 方言测试放在 `packages/drivers/<id>/`，不要在 Host 复制方言实现。
