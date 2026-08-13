# 数据同步（Data Synchronization）

> [返回架构总览](../README.md)

DataZen 把三类能力拆开，**本模块只实现 Data Synchronization**（对标 Navicat Diff Sync）：

| 产品 | 何时用 | V1 |
|------|--------|----|
| **Data Synchronization** | 同族、结构完全一致、相同 PK；Compare → ChangeSet → 参数化 DML | 已实现引擎 + 映射门闸壳；Apply 尚未接到窗口 |
| **Data Transfer** | 异构 / 需 IR 转换 | **不实现**（pairing 标 `path: ir, supported: false`） |
| **Structure Sync** | 只改 DDL | Schema Diff Deploy，见 [schema-diff.md](schema-diff.md) |

旧产品路径 **DROP + INSERT 覆盖拷贝**已拆除。兼容 IPC `sync_table` / `sync_tables` 立即拒绝，文案见 `data_sync::OVERWRITE_COPY_RETIRED`。

## 1. 硬门闸

1. `normalize_sync_family` 必须相同。V1 仅 `mysql`（含 MariaDB）与 `postgresql`。
2. 列名 / 类型 / 可空性必须一致（`types_eq`：`INT`=`INTEGER`，剥 MySQL display width，保留 VARCHAR 长度与 UNSIGNED）。
3. 主键集合与顺序必须一致。
4. 禁止同一连接 + 同一 database + 同一 schema 自同步。
5. 异构 SQL（如 PG→MySQL）走 IR，**不是** Synchronization。

## 2. 模块

```text
src-tauri/src/data_sync/
├── pairing.rs      # 同族 / IR / unsupported
├── types_eq.rs     # 类型规范化比较
├── gate.rs         # PK + 结构门闸
├── mapping.rs      # 表级 MATCHED / UNMAPPED_* / INCOMPATIBLE
├── model.rs        # SyncTask / RowChange / ComparisonResult
├── state.rs        # DRAFT → … → COMPLETED；禁止 COMPARING → EXECUTE*
├── session.rs      # 会话
├── changeset.rs    # 只含已勾选且 options 允许的行；DELETE 默认不选
├── compare.rs      # Host keyset 流式页合并（无方言 SQL）
├── sql.rs          # 参数化 INSERT/UPDATE/DELETE + Preview 字面量
├── execute.rs      # 专用执行通道；read_only / 事务 / 可取消
├── apply_loop.rs   # Compare → Apply → Recompare=0（内存闭环）
└── legacy.rs       # 覆盖拷贝退役文案

src-tauri/src/commands/sync/
├── inspect.rs      # inspect_data_sync：get_tables + schema → classify_tables
├── exec.rs         # execute_data_sync：不经 execute_query / sql_guard
└── table_sync.rs   # sync_table / sync_tables 仅 refuse_overwrite_copy
```

`src-tauri/src/sync/` 仍保留 IR 适配器与旧 `compare_databases` 抽样对比；**不得**再作为 Synchronization 执行引擎。

## 3. 执行链

```text
DataSyncWindow
    │ Compare
    ▼
inspect_data_sync  →  require_data_sync_family + classify_tables
    │
    ▼  （行 Diff / Apply 尚未接线）
compare_table_pages  →  ChangeSet  →  generate_table_sql  →  execute_data_sync
    │
    ▼
execute_statements（begin → query_with_params → commit；失败 rollback）
```

- ChangeSet 只含选中且被 options 允许的变更；DELETE 默认不选。
- SQL 执行用参数绑定；Preview 用字面量。占位符 `?` / `$n` 由 Driver quote 回调提供。
- 禁止把 ChangeSet 交给 `execute_query`（Safe Mode「UPDATE 必须有 WHERE」会误伤）。

## 4. 前端

- 窗口：`src/windows/data-sync/DataSyncWindow.tsx`（单例 `data-sync`）
- Pairing：`src/lib/syncPairing.ts`（与 Rust `pairing.rs` 对齐）
- IPC：`src/commands/sync.ts` → `inspectDataSync` / `executeDataSync`
- Apply 按钮 `data-testid="data-sync-start-disabled"` 保持禁用，直到 Diff Execute 接到窗口
- 覆盖拷贝横幅：`data-testid="data-sync-overwrite-retired"`

## 5. 测试落点

| 类型 | 位置 |
|------|------|
| 领域 / IPC 单测 | `src-tauri/src/data_sync/**`、`commands/sync/tests.rs`（Host，不写驱动方言） |
| 窗口 / pairing / i18n | `src/windows/data-sync/__tests__/`、`src/lib/__tests__/syncPairing.test.ts`、`src/locales/locales.test.ts` |
| Host E2E | `e2e/specs/data-sync-window.ts`、`e2e/specs/data-sync-real.ts` |
| 驱动方言深度 | **不要**放进 Host；写在 `packages/drivers/<id>/` |

进度与切片报告：`docs/progress/data-sync-navicat.md`。
