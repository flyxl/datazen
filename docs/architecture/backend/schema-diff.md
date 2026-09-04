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

## 6. Tests

- Rust unit tests：`src-tauri/src/schema_diff/**`
- Frontend：`src/windows/schema-diff/__tests__/`
- E2E：`e2e/specs/schema-diff-*.ts` 与 journey tests

Driver-specific 方言测试放在 `packages/drivers/<id>/`，不要在 Host 复制方言实现。
