# 数据同步（Data Synchronization）

> [返回架构总览](../README.md) · 用户手册：[data-sync-guide.md](../../features/data-sync-guide.zh-CN.md)

DataZen 把三类能力拆开，**本模块只实现 Data Synchronization**（对标 Navicat Diff Sync）：

| 产品 | 何时用 | V1 |
|------|--------|----|
| **Data Synchronization** | 同族、结构完全一致、相同 PK；Compare → Review → Preview → Execute | Diff Workspace + 引擎 / IPC 已接通 |
| **Data Transfer** | 异构 / 需 IR 转换 / 单向拷贝 | **V1 基础**（独立 `data-transfer` 窗口；见 [data-transfer-guide.md](../../features/data-transfer-guide.zh-CN.md)） |
| **Structure Sync** | 只改 DDL | Schema Diff Deploy，见 [schema-diff.md](schema-diff.md) |

旧产品路径 **DROP + INSERT 覆盖拷贝**与 legacy IPC（`compare_databases` / `sync_table` / `sync_tables`）已完全移除。

## 1. 硬门闸

1. `normalize_sync_family` 必须相同。V1 仅 `mysql`（含 MariaDB）与 `postgresql`。
2. 列名 / 类型 / 可空性必须一致（`types_eq`：`INT`=`INTEGER`，剥 MySQL display width，保留 VARCHAR 长度与 UNSIGNED）。
3. 主键集合与顺序必须一致。
4. 禁止同一连接 + 同一 database + 同一 schema 自同步。
5. 异构 SQL（如 PG→MySQL）走 IR，**不是** Synchronization。
6. **按 database 同步，不按 connection**：选择连接后，还需各自选定源/目标数据库。IPC 接收
   `source_database` / `target_database`（`Option<String>`），`inspect` / `compare` / `apply` /
   `execute` / `generate_data_sync_sql` / `revalidate_data_sync` 在查询前对两端连接做驱动层内部
   切库 `maybe_use_database(db)`（宿主 `use_database` IPC 已随重构移除）；未传则回落到连接的默认 `database`。
   前端用 `get_databases` 枚举并默认选中连接的 `database` 下拉（Compare 前两端都必须有值）。

## 2. 模块

```text
src-tauri/src/data_sync/
├── pairing.rs      # 同族 / IR / unsupported
├── types_eq.rs     # 类型规范化比较
├── gate.rs         # PK + 结构门闸
├── mapping.rs      # 表级 MATCHED / UNMAPPED_* / INCOMPATIBLE
├── model.rs        # SyncTask / RowChange / ComparisonResult / SyncOptions
├── state.rs        # DRAFT → … → COMPLETED；禁止 COMPARING → EXECUTE*
├── session.rs      # 会话
├── changeset.rs    # 只含已勾选且 options 允许的行；DELETE 默认不选
├── compare.rs      # PK 有序行合并（内存 + 可分页 RowPageSource；见 §3）
├── sql.rs          # 参数化 INSERT/UPDATE/DELETE + Preview 字面量
├── execute.rs      # 专用执行通道；read_only / 事务 / 可取消
├── apply_loop.rs   # Compare → Apply → Recompare=0（内存闭环，单测）
└── legacy.rs       # 覆盖拷贝退役文案

src-tauri/src/commands/sync/
├── inspect.rs      # inspect_data_sync：get_tables + schema → classify_tables
├── apply.rs        # compare / apply / generate_data_sync_sql / revalidate
├── jobs.rs         # cancel_data_sync 与 compare/execute 共用的 job 取消标志
└── exec.rs         # execute_data_sync：不经 execute_query / sql_guard；可带 jobId
```

`src-tauri/src/transfer/` 保留异构 IR 适配器与 DDL 生成（Schema Diff Deploy、**Data Transfer** 等）；**不得**再作为 Data Synchronization 执行引擎。Transfer 产品与 Sync 完全分离，见 [data-transfer-guide.md](../../features/data-transfer-guide.zh-CN.md)。

## 3. Compare 实现（V1 生产路径）

**当前 IPC `compare_data_sync` / `apply_data_sync` 前置 compare**（`commands/sync/apply.rs`）：

- 对每张 MATCHED 表使用 **keyset 分页**（`data_sync/keyset.rs` 生成 `(pk…) > (?) ORDER BY pk LIMIT batch_size` SQL；`commands/sync/keyset_source.rs` 的 `DriverKeysetSource` 经 `query_with_params` 拉页）。
- Host 用 `compare_table_pages`（`data_sync/compare.rs`）按 PK 做流式有序合并，产出 INSERT / UPDATE / DELETE / UNCHANGED；`options.batch_size` 控制每页行数，`jobId` 取消标志传入 compare 循环。
- `compare_data_sync` / `apply_data_sync` 接受 `options: SyncOptionsInput`（insert / update / delete 等），影响 diff 计数与 ChangeSet 过滤。

小表/单测仍可直接用 `compare_sorted_rows` + 内存 `SliceRowSource`；生产 IPC 路径已接 keyset 流式 compare。

## 4. 执行链

```text
DataSyncWindow（Diff Workspace）
    │ Compare
    ▼
inspect_data_sync  →  compare_data_sync(options)
    │
    ▼  Review：表/行勾选 + Options（Insert/Update/Delete）
generate_data_sync_sql  →  SQL Preview（只读）
    │
    ▼  Execute 前
revalidate_data_sync  →  结构/PK 漂移则 staleTables
    │
    ▼  Execute
ChangeSet  →  generate_table_sql  →  execute_data_sync(jobId)
    │
    ▼
execute_statements（begin → query_with_params → commit；失败/Cancel 则 rollback）
    │
    ▼
再 compare_data_sync → 期望行差异为 0
```

- ChangeSet 只含选中且被 options 允许的变更；DELETE 默认不选，UI 勾选需确认。
- SQL 执行用参数绑定；Preview 用字面量。占位符 `?` / `$n` 由 Driver quote 回调提供。
- 禁止把 ChangeSet 交给 `execute_query`（Safe Mode「UPDATE 必须有 WHERE」会误伤）。

## 5. IPC 一览

| 命令 | 用途 |
|------|------|
| `inspect_data_sync` | 表映射 + 结构门闸 |
| `compare_data_sync` | 行比较（含 `options`） |
| `generate_data_sync_sql` | 由比较结果 + 勾选生成 Preview SQL |
| `revalidate_data_sync` | Execute 前复检 MATCHED / INCOMPATIBLE |
| `apply_data_sync` | compare → generate → execute 一步闭环 |
| `execute_data_sync` | 执行已生成的 `SqlStatement[]` |
| `cancel_data_sync` | 取消进行中的 compare/execute job |

Legacy：`sync_table` / `sync_tables` / `compare_databases` / `classify_sync_pair` 已移除。Pairing 由 `classify_data_sync_pair` IPC（`data_sync/pairing.rs`）单一来源；前端 `src/lib/syncPairing.ts` 薄封装调用 IPC。

## 6. 前端 Data Sync 向导

- 窗口：`src/windows/data-sync/DataSyncWindow.tsx`（单例 `data-sync`，6 步向导：Endpoints / Setup / Objects / Compare / Preview / Result）
- 子组件：`EndpointsBar` / `OptionsBar` / `MappingPanel` / `CompareSummary` / `TableListPanel` / `DiffDetail` / `SqlPreview` / `ExecuteBar`
- Endpoints 步连接 + **database** 下拉：`data-sync-source-database` / `data-sync-target-database`；向导导航：`data-sync-back` / `data-sync-next`
- Options：`data-sync-option-insert|update|delete`
- Compare 后：`data-sync-summary`、行 Diff `data-sync-row-diff`；Preview 步：`data-sync-preview`
- Execute 底栏：`data-sync-execute`（容器）/ `data-sync-start`（按钮）/ `data-sync-start-disabled`
- 比较/执行中 Cancel：`data-sync-cancel` → `cancel_data_sync(jobId)`
- Pairing：`classify_data_sync_pair` IPC + `src/lib/syncPairing.ts`（IPC 薄封装）
- IPC 封装：`src/commands/sync.ts`

## 7. 测试落点

| 类型 | 位置 |
|------|------|
| 领域 / IPC 单测 | `src-tauri/src/data_sync/**`、`commands/sync/tests.rs`（Host，不写驱动方言） |
| 窗口 / pairing / mapping | `src/windows/data-sync/__tests__/`、`src/lib/__tests__/syncPairing.test.ts` |
| Host E2E UI | `e2e/specs/data-sync-window.ts`（Diff Workspace chrome） |
| Host E2E IPC | `e2e/specs/data-sync-real.ts`（inspect/compare/apply/generate/revalidate；需 PG/MySQL 夹具） |
| 驱动方言深度 | **不要**放进 Host；写在 `packages/drivers/<id>/` |
