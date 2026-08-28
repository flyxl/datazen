# Data Transfer 产品定义（PRD）

> **状态**：V1 已 GA；`productFeatures.dataTransfer` 为 `true`；6 步向导与 E2E 已合入。  
> **用户手册**：[data-transfer-guide.zh-CN.md](./data-transfer-guide.zh-CN.md)  
> **UI 设计存档**：[data-transfer-ui-redesign.zh-CN.md](./data-transfer-ui-redesign.zh-CN.md)（含 SVG 线框）  
> **架构**：[data-sync.md](../architecture/backend/data-sync.md)（Transfer 与 Sync 引擎分离说明）

---

## 1. 产品定位

**Data Transfer** 是 DataZen 的**异构 / 单向数据迁移**工具：在源与目标结构不必一致、主键不必相同、甚至跨 SQL 方言时，将选定表的结构和/或数据复制到目标库。

与另两款迁移产品的边界：

| 产品 | 核心问题 | 关键约束 |
|------|----------|----------|
| **Data Sync** | 两库已对齐，如何增量同步行差异？ | 同方言族、结构一致、PK 一致 |
| **Schema Diff** | 两库结构有何不同，如何部署 DDL？ | 对比 + 受控 Deploy，不做 bulk 数据 |
| **Data Transfer** | 如何把数据/表搬到另一个（可能异构的）库？ | 单向拷贝；可建表、可列映射、可走 IR |

```text
迁库 / 灌空库 / 跨方言导入 / 结构不一致的全量搬运  →  Data Transfer
结构先对齐                                      →  Schema Diff（可选前置）
对齐后持续增量                                  →  Data Sync
```

---

## 2. 目标用户与场景

### 2.1 目标用户

- 需要在 **MySQL ↔ PostgreSQL** 等异构环境间迁移的 DBA / 后端工程师
- 从生产导出子集到分析库、测试库的全量复制场景
- 接受「单向、批处理」而非实时同步的运维人员

### 2.2 典型场景

| 场景 | Mode | Write mode | 说明 |
|------|------|------------|------|
| 空库灌数 | data / structure+data | insert | 目标表已存在或勾选 create new |
| 同构换库 | structure+data | insert / truncate+insert | 同方言 direct 路径 |
| 跨方言迁表 | structure+data | insert | IR 路径 + 列/类型映射 |
| 仅同步表结构 | structure | — | 生成/执行 CREATE |
| 覆盖目标表 | data / structure+data | truncate+insert / drop+create | 破坏性，需显式确认 |

### 2.3 非目标（Out of Scope V1）

- 双向同步、冲突解决（→ Data Sync）
- 仅对比结构 diff 而不执行（→ Schema Diff）
- 视图/函数/触发器/存储过程迁移
- Redis / 非 SQL 类别传输
- 增量 CDC、定时调度任务（未来独立 Workflow / 任务中心）
- 传输过程中的断点续传表级 checkpoint（V1 仅 job 级 cancel）

---

## 3. 功能需求

### 3.1 配对与门闸（Pairing）

**必须：**

- 根据源/目标 `databaseType` 分类配对：
  - **direct** — 同 sync family（如 PG↔PG）
  - **ir** — 跨 SQL 方言（如 MySQL→PG），经 `transfer/` IR 适配器
  - **unsupported** — 跨 category（SQL↔Redis）或 unsupported type
- UI 禁用 unsupported 目标；显示 `pairing.reason`
- 阻断：目标连接 `read_only`、同连接+同库+同表自覆盖

**实现参考：** `src/lib/transferPairing.ts` ↔ `src-tauri/src/transfer/pairing.rs`

### 3.2 端点（Endpoints）

**必须：**

- 源/目标：持久化 `connectionId` + 运行时 `dbSessionId`（专用 side session）
- Catalog 选择：`database` 下拉（`get_databases` / dedicated session）
- 显示传输路径 badge（direct / ir）

**V1 缺口（Backlog P1）：**

- Schema 选择（类型 `TransferEndpoint.schema` 已存在，UI 未暴露）
- 右键/菜单 **预填 Source**（connectionId + database [+ table]）
- Source ↔ Target 一键交换

### 3.3 传输模式（Mode）

| 值 | 用户文案 | 行为 |
|----|----------|------|
| `structure` | Structure only | 仅 CREATE（或 ddlOverride） |
| `data` | Data only | 仅 INSERT 类写入（目标表须可写） |
| `structureAndData` | Structure + data | 先建表再灌数 |

### 3.4 对象选择（Inspect / Objects）

**必须：**

- `inspect_transfer`：枚举源表，推断目标表名（同名）、status、列清单
- 用户勾选 enabled 表；status 包括 `MATCHED` | `CREATE_NEW` | `UNMAPPED_*` | `INCOMPATIBLE` | `DISABLED`
- 不兼容表携带 `incompatibleReason`

**V1 缺口（Backlog P1）：**

- Objects 步展示 `sourceRowCount`、人类可读 status
- 表搜索、批量选、按 status 筛选
- Objects 步 inline 编辑目标表名

### 3.5 映射（Mapping）

**必须（V1 已实现 UI）：**

- 表级：目标表名、`createNew` 开关
- 列级：源列 → 目标列、`skip`、跨方言 `targetNativeType`
- 工具：**Auto-match by name**、**Clear unmapped**
- 目标表名 commit 后局部 re-inspect

**应持续增强：**

- 映射完成度指示（N/M 列）
- 按名称+类型智能匹配
- 类型建议（adapter hints）

### 3.6 写入选项（Options）

| Write mode | 破坏性 | 行为摘要 |
|------------|--------|----------|
| `insert` | 否 | 批量 INSERT |
| `truncateInsert` | 是 | TRUNCATE + INSERT |
| `dropCreateInsert` | 是 | DROP + CREATE + INSERT |

**必须：**

- 破坏性模式需 `confirmedDestructive`（UI 勾选 + job options）
- `batchSize`（默认 500）、`stopOnError`（默认 true）

**Backlog P1：** Execute 前 **最终确认 Dialog**（列出将 TRUNCATE/DROP 的表），对齐 Data Sync。

### 3.7 预览（Preview）

**必须：**

- `preview_transfer`：DDL 列表、write plans（estimatedRows）、warnings
- `canExecute` / `blockReason` 门闸
- DDL **可编辑**（`ddlOverride` per table），Preview 步 SqlEditor

### 3.8 执行（Execute）

**必须：**

- `execute_transfer(job, jobId)`；`cancel_transfer(jobId)`
- 结果：`TransferExecutionResult`（per-table success/error/rowsInserted、partial、cancelled）
- 目标 read_only 时 UI + 后端阻断

**V1 缺口（Backlog P0）：**

- **执行进度 UI**（表级/行级，参考 `SyncProgressPanel`）
- Preview 与 Execute 合并为一步（减少无效点击）
- 失败表「仅重试」

### 3.9 入口与发现性

| 入口 | V1 状态 | 目标 |
|------|---------|------|
| Tools 菜单 | ✅（门闸后可见） | — |
| 连接树右键 | ✅ 打开窗口 | 预填 source |
| Data Sync 窗口跳转 | ❌ | 异构对提示 + 一键打开 Transfer |
| Schema Diff 跳转 | ❌ | 结构不同提示 |

---

## 4. 非功能需求

### 4.1 安全

- 破坏性写入双重确认（Options 勾选 + 建议 Execute Dialog）
- 目标 read_only 硬阻断
- 日志不输出连接密码；SQL 预览可含 schema 信息但不含凭据

### 4.2 性能

- 批量 INSERT，`batchSize` 可配置
- 大表 inspect/preview 应显示 loading；Execute 需进度反馈（待实现）

### 4.3 可靠性

- `stopOnError`：首错停止 vs 继续（partial result）
- Cancel：best-effort 停止当前 job
- Side session 与主连接树隔离（`dedicatedDbSession`）

### 4.4 可测试性

- Host E2E：`e2e/specs/data-transfer-window.ts`（PG→PG 闭环）
- 跨方言 Execute E2E：PG→MySQL insert（待稳定）
- 驱动相关测试在 Host / driver crate，不按方言拆到各 driver

---

## 5. 技术架构摘要

```text
DataTransferWindow (React 向导)
    ↕ IPC transferCommands (inspect / preview / execute / cancel)
src-tauri/src/commands/data_transfer/
src-tauri/src/data_transfer/     ← 执行编排
src-tauri/src/transfer/          ← IR 适配器 + DDL 生成（与 data_sync 引擎分离）
```

**关键类型：** `TransferJob`, `TransferTableMapping`, `TransferPreview`, `TransferExecutionResult`（`src/commands/transfer.ts`）

---

## 6. UI/UX 原则与 Backlog

### 6.1 设计原则（已对齐部分）

1. **与 Data Sync 视觉同源** — `bg-surface` / `border-edge`、居中 stepper、Endpoints 步样式 ✅
2. **专业但不晦涩** — status 人类可读 ✅
3. **先审查后破坏** — Preview 必达，破坏性操作多次确认 ✅
4. **可恢复反馈** — Cancel、Result 汇总 ✅；细粒度进度面板仍为 backlog

### 6.2 Backlog（非阻塞）

| 项 | 优先级 | 说明 |
|----|--------|------|
| 执行进度面板 | P1 | 当前以 spinner 为主 |
| 右键预填 Source | P1 | query params + localStorage |
| Objects 信息密度 | P1 | 行数、原因、图标 |
| Endpoints schema + Swap | P2 | 同 Sync EndpointsBar |
| 单页工作台 | P3 | 长期可选 |
| 保存任务模板 | P3 | 命名 job 复用 |

---

## 7. 验收标准（V1 GA）

### 7.1 功能验收

- [x] PG→PG：`structure` / `data` / `structure+data` × `insert` 闭环（E2E）
- [x] PG↔MySQL：IR 路径 journey + 宽类型 fixture（E2E）
- [x] 列映射 UI：auto-match、skip、create new、targetNativeType（structure + create-new）
- [x] DDL override 生效于 execute
- [x] Cancel 可中止长跑 job
- [x] read_only / unsupported pairing / 自覆盖 正确阻断
- [x] `productFeatures.dataTransfer` 为 true

### 7.2 文档与测试

- [x] 用户指南与 UI 能力一致
- [x] Host E2E：`pnpm e2e:data-transfer`（见 [e2e-coverage.md](../development/e2e-coverage.md)）
- [x] 就绪度摘要：[migration-tools-review-v0.1.0.md](../development/migration-tools-review-v0.1.0.md)

---

## 8. 版本规划（建议）

| 版本 | 范围 |
|------|------|
| **V1.0（当前）** | 6 步向导、direct + IR、映射 UI、preview/execute、cancel、E2E |
| **V1.1** | 预填入口、进度面板、Objects 增强 |
| **V1.2** | Endpoints schema + swap、与 Sync/Diff 互跳 |
| **V2.0** | 单页工作台、保存/加载任务、可选调度 |

---

## 9. 相关文档

- [用户使用手册](./data-transfer-guide.zh-CN.md)
- [UI 重构规格（SVG 线框）](./data-transfer-ui-redesign.zh-CN.md)
- [Data Sync 手册](./data-sync-guide.zh-CN.md)
- [Schema Diff 手册](./schema-diff-guide.zh-CN.md)
- [迁移工具就绪度评估](../development/migration-tools-review-v0.1.0.md)
- [窗口架构](../architecture/windows.md)
