# 迁移工具就绪度（Schema Diff / Data Sync / Data Transfer）

> 更新日期：2026-08-28  
> 三项功能均已通过 `productFeatures` 对用户可见（`schemaDiff` / `dataSync` / `dataTransfer` 均为 `true`）。

## 总览

| 功能 | 入口 | UI 形态 | E2E | Host 单测 |
|------|------|---------|-----|-----------|
| **Data Sync** | Tools → Data Sync；连接树 | 单页 Diff Workspace | `data-sync-*` | `src/windows/data-sync` |
| **Schema Diff** | Tools → Schema Diff；连接树 | **5 步向导**（Endpoints → Objects → Compare → Plan → Deploy） | `pnpm e2e:schema-diff`（7 spec） | `src/windows/schema-diff`（12 tests） |
| **Data Transfer** | Tools → Data Transfer；连接树 | 6 步向导（Endpoints → Setup → Objects → Mapping → Preview → Result） | `pnpm e2e:data-transfer` | `src/windows/data-transfer` |

**产品边界**（不变）：

- **Sync** — 同方言族、结构一致、相同 PK → 行级 Diff
- **Schema Diff** — 结构对比 + 受控 DDL Deploy（不灌行数据）
- **Transfer** — 异构 / 结构不一致 → 单向结构+数据搬运（IR 路径）

## Schema Diff（当前能力）

**已实现：**

- `MigrationEndpointsBar` + `useSchemaDiffEndpoints`：连接、database、schema（PG 等）；Endpoints 步暂 **无 Swap**
- Objects 步：自源库拉表多选；Compare 步：左表列表 + 列级 diff；Plan / Deploy 分步
- 对比 → 生成计划 → 部署闭环；跨方言 IR 类型映射
- 限制说明弹窗（首次打开，可「不再显示」）
- PG 部署使用 driver 事务 API（避免 idle-in-transaction 锁表）

**已知限制：**

- 视图 / 函数 / 触发器 / 存储过程不在范围
- 配置 JSON **v2** 不含 database/schema 字段（导入后需手动选库/schema）
- MySQL DDL 为语句级提交，失败时状态为 `mixed` 而非整批回滚

**Backlog：** [migration-tools-backlog.md](../todo/migration-tools-backlog.md)（config 含 db/schema、Swap、帮助锚点、预填等）

**文档：** [schema-diff-guide.zh-CN.md](../features/schema-diff-guide.zh-CN.md)

## Data Sync（当前能力）

- 核心 Compare → Review → Preview → Execute 已就绪（2026-08 UI 修复合入）
- 拒绝跨方言配对（PG↔MySQL）

**文档：** [data-sync-guide.zh-CN.md](../features/data-sync-guide.zh-CN.md)

## Data Transfer（当前能力）

**已实现：**

- 6 步向导 + `bg-surface` / `border-edge` token 对齐
- direct（同方言）与 IR（跨方言）路径；列映射与 `targetNativeType`（create-new + structure 模式）
- Preview 底栏 Execute；破坏性 write mode 多重确认
- 限制说明弹窗；向导内能力限制摘要（`transfer.limitations.*`）
- E2E：PG↔PG、PG↔MySQL、MySQL↔PG journey + 2500 行宽类型 fixture

**Backlog（非阻塞 GA）：** 见 [migration-tools-backlog.md](../todo/migration-tools-backlog.md)（Transfer：进度面板、预填、Objects 增强、schema+Swap 等）

**文档：** [data-transfer-guide.zh-CN.md](../features/data-transfer-guide.zh-CN.md) · [data-transfer-prd.zh-CN.md](../features/data-transfer-prd.zh-CN.md)

## 测试命令

```bash
npx vitest run src/windows/schema-diff
npx vitest run src/windows/data-sync
npx vitest run src/windows/data-transfer
pnpm e2e:schema-diff:build
pnpm e2e:data-transfer:build
pnpm e2e:skip-build -- --spec e2e/specs/journeys/data-sync-journey.ts
```

覆盖矩阵见 [e2e-coverage.md](./e2e-coverage.md)。
