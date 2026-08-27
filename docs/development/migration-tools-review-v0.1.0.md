# Migration Tools Review (v0.1.0)

> 结构对比 / 数据同步 / 数据传输 — 生产就绪度评估  
> 评估日期：2026-08-28  
> v0.1.0 状态：三项功能入口已通过 `productFeatures` 隐藏，代码保留供开发与 E2E 直达。

## Executive Summary

三个功能在代码层面均已实现 **Compare/Inspect → Review → Execute** 主链路，后端 IPC 已注册且可通过 `window.html?window=…` 直达；但在 **v0.1.0 产品层全部被门闸关闭**（`src/lib/productFeatures.ts` 与 `src-tauri/src/product_features.rs`）。

| 功能 | 架构完整度 | 生产就绪度 | 建议启用顺序 |
|------|-----------|-----------|--------------|
| **结构对比 (Schema Diff)** | 高 — 对比/计划/部署闭环完整 | 中 — 缺库/schema 选择、部署 E2E、MySQL 原子性 UX | 第二启用 |
| **数据同步 (Data Sync)** | 最高 — 引擎 + Diff Workspace 最成熟 | 中高 — 核心路径可用，若干 UI/IPC 接线未完成 | **第一启用** |
| **数据传输 (Data Transfer)** | 中高 — 向导 + IR 路径存在 | 中 — 跨方言 Execute 未 E2E、文档与 UI 不一致 | 第三启用 |

**共同现状：**

- 菜单/右键入口被门闸隐藏；Web `MenuBar` 在 v0.1.0 仅条件显示 Schema Diff（当前也为 false）。
- i18n 键已在 `en.ts` / `zh-CN.ts` 等补齐。
- 多库支持：Sync / Transfer 有 **database** 下拉；Schema Diff **无 database/schema 选择**，依赖连接默认库。
- 异构支持：Sync **拒绝** PG↔MySQL；Transfer **支持 IR 路径**；Schema Diff **跨方言走 IR 映射**（失败则 warning + skip）。

---

## Schema Diff

### 架构与完整度

**已实现：**

- `SchemaDiffWindow`：compare → plan → review/deploy 三步向导
- IPC：`compare_table_schemas` → `prepare_schema_diff_plan` → `execute_schema_diff_deploy`
- 后端 `src-tauri/src/schema_diff/`：compare / plan / deploy / dialects
- 跨方言 IR diff + 类型映射；破坏性 DDL 需 `DEPLOY` 确认词

**明确不做 / 半成品：**

| 能力 | 状态 |
|------|------|
| 视图/函数/触发器/存储过程 | ❌ 文档明确不做 |
| Database / Schema 选择器 | ❌ 完全缺失 |
| 部署前自动备份 | ❌ |
| `requireRollback` 后端强制 | ❌ 仅前端门闸 |

### 测试覆盖

- Rust 单测：plan / deploy / compare 有覆盖
- E2E：`schema-diff-window.ts` 仅窗口壳 + 表名必填
- **部署 E2E：无**

---

## Data Sync

### 架构与完整度

**已实现（V1 生产路径）：**

- `DataSyncWindow`：Endpoints → inspect → compare → review → generate SQL → execute
- 同族门闸（MySQL/MariaDB + PostgreSQL）；结构 + PK 必须一致
- 专用 `execute_data_sync` 通道；Cancel job 支持

**缺口：**

| 项 | 状态 |
|----|------|
| Execute 前 `revalidate_data_sync` | ❌ UI 未调用 |
| `SavedTasksBanner` / `ResumeSyncDialog` | ⚠️ 组件存在，未接入主窗口 |
| `largeValueMode` | ⚠️ 类型存在，compare 未使用 |

### 测试覆盖

- Rust 单测：非常充分（`data_sync/**`）
- E2E UI：`data-sync-window.ts` 含 DSW-EXEC-001 PG 闭环
- E2E IPC：`data-sync-real.ts`

---

## Data Transfer

### 架构与完整度

**已实现：**

- 8 步向导 + `classify_transfer_pair` / inspect / preview / execute
- 同族 direct insert；IR 跨方言路径；truncate+insert / drop+create（需确认）

**缺口：**

- 无 schema 选择 UI（类型存在）
- 用户指南写「V1 不支持列映射 UI」，但 `ColumnMappingEditor` 已实现 — **文档过时**
- 跨方言 Execute E2E：**无**

### 测试覆盖

- Rust 单测：中等
- E2E：`data-transfer-window.ts` PG→PG insert 闭环

---

## Cross-cutting Recommendations

### P0 — 启用前必做

1. Data Sync Execute 前接入 `revalidate_data_sync`
2. Schema Diff 增加 database（+ schema）选择
3. Schema Diff 部署 E2E（PG→PG 至少一条）
4. Data Transfer 跨方言 E2E（PG→MySQL insert）
5. 更新 data-transfer-guide / schema-diff-guide（JSON v2、列映射 UI）

### 启用顺序建议

**Sync → Schema Diff → Transfer**

---

## Re-enable Checklist (v0.1.x)

### Data Sync

- [ ] `revalidate_data_sync` 接入 Execute
- [ ] MySQL→MySQL apply E2E 至少 1 条
- [ ] UPDATE / DELETE Execute E2E 各 1 条
- [ ] SavedTasks UI 接入或移除
- [ ] MenuBar + 原生菜单 + 连接树右键三门一致

### Schema Diff

- [ ] Database/schema 选择器
- [ ] compare → plan → deploy PG E2E
- [ ] 跨方言 plan E2E 1 条
- [ ] 用户文档 JSON v2 对齐

### Data Transfer

- [ ] 更新用户指南
- [ ] PG→MySQL data insert E2E
- [ ] truncate+insert / drop+create E2E
- [ ] structure / structure+data E2E

### 共同

- [ ] `productFeatures.ts` ↔ `product_features.rs` 同步
- [ ] `docs/development/e2e-coverage.md` 矩阵更新
- [ ] 发布说明写明支持矩阵与明确不支持项

---

## Related

- Architecture: [schema-diff.md](../architecture/backend/schema-diff.md), [data-sync.md](../architecture/backend/data-sync.md)
- E2E journeys: `e2e/specs/journeys/*-journey.ts`（`pnpm e2e:skip-build -- --suite journeys`）
- Feature gates: `src/lib/productFeatures.ts`, `src-tauri/src/product_features.rs`
