# Schema Diff 架构

> P5 桌面 JobRuntime 接入已完成；迁移三件套独立测试与冒烟测试通过。Source of truth: `modules/src/migration/schema_diff/`, `src-tauri/src/commands/schema_diff/`, `src-tauri/src/commands/schema_diff.rs`, `packages/driver-api/src/schema_migration.rs`。

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
| `modules/src/migration/schema_diff/compare.rs` | 列、PK、索引差异；支持 `TypeNormalizer` |
| `modules/src/migration/schema_diff/ir.rs` | Snapshot → `MigrationOperation` |
| `modules/src/migration/schema_diff/operations.rs` | 方言无关操作及风险 |
| `modules/src/migration/schema_diff/dependencies.rs` | 操作依赖与执行顺序 |
| `modules/src/migration/schema_diff/plan.rs` | 能力检查、DDL 渲染、计划生成 |
| `modules/src/migration/schema_diff/deploy.rs` | 目标库部署及事务/部分失败结果 |
| `modules/src/migration/schema_diff/types.rs` | Snapshot、Diff、Plan DTO |
| `modules/src/migration/schema_diff/profile_repository.rs` | 持久化 Schema Diff profile 的领域存储端口 |
| `modules/src/migration/schema_diff/reviewed.rs` | 评审计划的**规则**（身份指纹、目标快照复核、依赖目录比对）与存储端口 `ReviewedPlanRepository` |
| `src-tauri/src/commands/schema_diff/` | Tauri IPC 与桌面 Job adapter |

Host 侧 `src-tauri/src/commands/schema_diff.rs` 只是模块门面（模块声明 + re-export），
命令按职责拆到同目录下的子模块：

| 文件 | 职责 |
|---|---|
| `src-tauri/src/commands/schema_diff.rs` | 门面：模块声明、命令 re-export、共享 helper 导入 |
| `src-tauri/src/commands/schema_diff/object_scope.rs` | schema scope 门闸与 schema 对象选择 |
| `src-tauri/src/commands/schema_diff/profile_commands.rs` | Schema Diff profile 的增删查；经 `ApplicationServices` 访问领域 profile 端口与连接目录 |
| `src-tauri/src/commands/schema_diff/dependency_catalog.rs` | 目标库外键依赖目录读取与复核 |
| `src-tauri/src/commands/schema_diff/plan_commands.rs` | 表级迁移计划生成 |
| `src-tauri/src/commands/schema_diff/object_plan_commands.rs` | 视图 / 存储过程 / 序列 / 类型计划生成 |
| `src-tauri/src/commands/schema_diff/deploy_commands.rs` | 迁移计划执行与取消 |
| `src-tauri/src/commands/schema_diff/compare_commands.rs` | 单表列级差异比较 |
| `src-tauri/src/commands/schema_diff/reviewed_repository.rs` | 评审计划存储端口的桌面实现（单租户、进程内存）与组织作用域取用 |

`src-tauri/src/commands/schema_diff/job.rs` 与 `unified_plan.rs` 保持各自的子树
（桌面 Job adapter、统一计划与依赖目录复核）。

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

Schema Diff 的 prepare/apply 请求以 `sourceDbSessionId` / `targetDbSessionId` 指定用户当前授权的端点；Job 列表、详情、取消与恢复核验则按 `jobId` 操作共享桌面 Job host。桌面 Job adapter 接受并持久化 Job 后，在后台 dispatch 对应 handler。

- **专用会话由前端建、由前端放**。源端与目标端各自持有一条专用会话：端点组件用 `ensureDedicatedSession` 调 `connectDedicated` 建立，并在端点或目标库变化、以及组件卸载时用 `releaseDedicatedSession` 释放上一条（走的是与普通释放同一套引用计数，计数归零才真正断开）。释放发生在建立新会话**之前**，不会新旧两条同时挂着。端点没变时先 `ping` 探活，探活失败才重连。后端只做 `get_session` / `get_session_config` 查找。
- **profile 落盘的是 `connectionId`**。加密的 profile 文件保存 `sourceConnectionId` / `targetConnectionId`，不保存任何运行时会话 ID；Deploy 的历史记录写入的同样是解析出来的 `connectionId`。
- **反查依赖 owner 映射**。部署时把目标会话反查成 `connectionId` 以便落库；反查失败即中止部署并报「目标连接归属不可用」，不会退化成写一个空归属。Idle 回收故意保留 owner 映射正是为了让这条反查在会话物理连接消失后仍然可用。
- **目标库在建连时就定死，不切库**。专用会话建立时把用户选中的库作为连接覆盖传下去（SQLite 例外：`main` 是目录别名，会传文件路径而不是 `main`），因此会话始终停在选中的库上，宿主不需要切库路径。
- **prepare/apply 均由共享 Job 生命周期管理**。`cancel_schema_diff_deploy` 按 `jobId` 向 `JobHost` 记录取消意图；关闭面板不会取消已接受的 Job。Job 状态、提交边界与结果可通过详情查询，恢复核验只读，不会重放旧 handler。

与另两个成员的边界：Schema Diff 负责**结构差异对比与 DDL 迁移生成**；同族数据复制属于 Data Synchronization，异构数据搬迁属于 Data Transfer。三者共用「专用会话 + 持久化 connectionId」的约束，但各自的会话由谁建立、profile 存什么并不相同。三者 prepare/apply Job 共用桌面持久化 host 与取消、详情和恢复核验机制。

## 7. 评审计划：规则留包，存储走端口

一次性评审计划（prepare → 人工复核 → deploy → 消费）拆成两半：

| 位置 | 职责 |
|---|---|
| `modules/src/migration/schema_diff/reviewed.rs` | **规则**：配置身份指纹、目标快照复核、依赖目录比对、计划有效期；并定义存储端口 `ReviewedPlanRepository` 与随端口提供的进程内实现 `InMemoryReviewedPlanRepository`（按组织分桶） |
| `src-tauri/src/commands/schema_diff/reviewed_repository.rs` | **存储实现**：桌面形态的实例，在包内进程内实现之上叠加**单租户**收窄 |

- **包内没有进程级全局可变状态**。计划寄居在宿主注入的实例里（`AppState.schema_diff_reviewed_plans`），不再寄居在包里的 `static` 上：多租户宿主可以按组织分桶，A 组织的计划不会被 B 组织读到、顶掉或提前消费掉。
- **端口方法一律带 `scope: &OrganizationScope`**（`datazen::platform_api::ports::scope`）。**越组织访问按「不存在」处理**——取不到、删不掉，也**不**返回「无权限」错误，那会泄漏「存在但无权」（CM-05）。
- **端口的行类型是领域词汇 `ReviewedPlan`**，不另造一份计划结构；存储实现不得把规则的判定（有效期、身份比对）搬走。
- **桌面是单租户**：只承认自己那一个组织（`DesktopIdentity::organization_id()`），别的组织一律按「不存在」处理（越组织**写入直接丢弃**）。桌面身份不可用时该存储**不承认任何组织**，不编造组织身份。
- 计划的**有效期**（30 分钟）与**容量上限**由包给出常量（`REVIEWED_PLAN_TTL` / `REVIEWED_PLAN_MAX_ENTRIES`），存储实现据此裁剪；桌面沿用**进程内存**。
- **与改造前并非「完全一致」的一处**：冻结时刻的时间基准从单调钟（`Instant::elapsed()`，免疫系统时钟调整）改成**墙上时钟**（Unix 毫秒）——这是端口化后支持落盘实现的必要取舍（`Instant` 不可持久化），规则层与存储层必须读同一份时刻。两处系统时钟异常都按**失效关闭**处理：
  - **时钟回拨**（`now < created_at_ms`）原本会让本该过期的计划**重新变新**（失效开放）；本实现已用**规则层与存储层共用的同一个** `ReviewedPlan::is_expired` 钳住——回拨时直接判**已过期**，存储层的裁剪也用它，回拨条目会被清掉而不是占位。
  - **时钟前跳**（超过 TTL）会让刚冻结的计划**立刻过期**（失效关闭方向），用户重新比对即可恢复。

## 8. Tests

- Rust unit tests：`modules/src/migration/schema_diff/**`
- Frontend：`src/windows/schema-diff/__tests__/`
- E2E：`e2e/specs/schema-diff-*.ts` 与 journey tests

Driver-specific 方言测试放在 `packages/drivers/<id>/`，不要在 Host 复制方言实现。
