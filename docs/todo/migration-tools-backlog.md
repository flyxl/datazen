# 迁移工具 Backlog（Schema Diff / Data Sync / Data Transfer）

> **更新日期：** 2026-08-28  
> **就绪度（已实现能力）：** [migration-tools-review-v0.1.0.md](../development/migration-tools-review-v0.1.0.md)  
> **产品定义（Transfer）：** [data-transfer-prd.zh-CN.md](../features/data-transfer-prd.zh-CN.md)

本文汇总三项迁移工具的**非阻塞 GA**后续项。实现时请遵循 AGENTS.md 测试落点规则；Host UI 变更需同步 E2E。

---

## 优先级

| 级别 | 含义 |
|------|------|
| **P1** | 下一小版本（V1.1）优先 |
| **P2** | 体验对齐 / 跨工具一致性 |
| **P3** | 长期可选 |

---

## Schema Diff

| ID | 项 | 优先级 | 现状 / 缺口 | 建议实现 |
|----|-----|--------|-------------|----------|
| SD-1 | **配置 JSON 含 database/schema** | P1 | 导出为 `version: 2`，仅含 `sourceConnectionId` / `targetConnectionId` 与表列表；导入后不恢复库/schema 选择 | 扩展 `SchemaDiffConfigJson`（v2 字段或 v3），`handleImportConfig` 写入 `useSchemaDiffEndpoints` |
| SD-2 | **Endpoints 步 Swap** | P2 | `MigrationEndpointsBar` 在 Schema Diff 中 `showSwap={false}` | 启用 Swap 并重置 wizard 状态（与 Data Sync 行为一致） |
| SD-3 | **帮助链接独立锚点** | ✅ | TitleBar 帮助 → `#schema-diff`（`docsUrls.ts` + 在线手册） | — |
| SD-4 | **连接树预填 Source** | P2 | 右键「Compare Schema」仅 `openSchemaDiffWindow()`，无 URL params | 仿 `openBackupWindow`：`connectionId` + `database` (+ `schema`) query params + window 内读取 |
| SD-5 | **Objects 步增强** | P3 | 自源库拉表 + 全选/取消；无搜索、无行数 | 搜索过滤、按 schema 分组、显示 approximate row count（若驱动支持） |
| SD-6 | **视图 / 函数 / 触发器** | P3 | 产品明确 out of scope | 需 IR + plan 扩展；单独 RFC |

**源码锚点：** `src/windows/schema-diff/SchemaDiffWindow.tsx` · `src/commands/schemaDiff.ts` · `src/lib/docsUrls.ts`

---

## Data Transfer

| ID | 项 | 优先级 | 现状 / 缺口 | 建议实现 |
|----|-----|--------|-------------|----------|
| DT-1 | **执行进度面板** | P1 | Execute 以 spinner + Result 汇总为主；无表级/行级进度 | 参考 Data Sync 设计稿中的 `SyncProgressPanel` 模式；IPC 若缺进度事件需后端补充 |
| DT-2 | **右键预填 Source** | P1 | 连接树 `onDataTransfer` 无 prefill；`openDataTransferWindow()` 无 params | URL/localStorage：`connectionId`、`database`；Endpoints 步自动选中 |
| DT-3 | **Objects 步信息密度** | P1 | 表列表基础 checkbox；无搜索、行数、跳过原因图标 | 搜索框、行数列、`inspect` 状态 badge（read_only / unsupported 等） |
| DT-4 | **Endpoints schema + Swap** | P2 | 仅 database 下拉；`showSwap={false}`；PG schema 未在 Endpoints 暴露 | 对齐 `MigrationEndpointsBar` 与 Data Sync；跨 schema 迁移需 inspect 路径验证 |
| DT-5 | **与 Sync / Diff 互跳** | P2 | Data Sync 窗口有 CTA 打开 Transfer/Diff，但**不带端点上下文** | 共享 query param 契约（connectionId、database、schema） |
| DT-6 | **保存 / 加载任务模板** | P3 | 无命名 job 持久化 | 本地 JSON 或 AppSettings 子键 |
| DT-7 | **单页工作台** | P3 | 当前 6 步向导 | PRD V2.0；非近期目标 |

**源码锚点：** `src/windows/data-transfer/DataTransferWindow.tsx` · `src/lib/windowManager.ts` · `ConnectionNavigatorTree.tsx`

---

## Data Sync

| ID | 项 | 优先级 | 现状 / 缺口 | 建议实现 |
|----|-----|--------|-------------|----------|
| DS-1 | **表级进度面板** | P1 | `SyncProgressPanel` 在设计评审中存在，**未接入** `DataSyncWindow` | Execute 长跑时 modal / 侧栏表级状态 |
| DS-2 | **保存任务 / 恢复** | P2 | `SavedTasksBanner`、`ResumeSyncDialog` 未接入 | 持久化 job 快照 + 冲突检测 |
| DS-3 | **Execute 细粒度 cancel** | P2 | 已有 Cancel IPC；UI 反馈可加强 | 与 DS-1 一并设计 |

**参考：** [data-sync-ui-review.md](../development/data-sync-ui-review.md) 附录（未接入组件列表）

---

## 文档与在线手册

| ID | 项 | 优先级 | 说明 |
|----|-----|--------|------|
| DOC-1 | **manual.html 三分法锚点** | ✅ | `#data-sync` / `#data-transfer` / `#schema-diff` 子章节 + TOC 子链接 |
| DOC-2 | **Schema Diff 手册与 UI 同步** | P1 | 产品已改为 **5 步向导**（Endpoints → Objects → Compare → Plan → Deploy）；部分架构/设计存档仍写「双栏单页」 |
| DOC-3 | **配置 JSON 示例** | P1 | 手册应写 `version: 2` + `sourceConnectionId` / `targetConnectionId`（非 v1 `configId`） |

---

## 建议版本切分

| 版本 | Schema Diff | Data Transfer | Data Sync | 文档 |
|------|-------------|---------------|-----------|------|
| **V1.1** | SD-1 | DT-1 ~ DT-3 | DS-1 | DOC-2、DOC-3 |
| **V1.2** | SD-2 ~ SD-4 | DT-4 ~ DT-5 | DS-2 | DOC-1 |
| **V2.0+** | SD-6 | DT-6 ~ DT-7 | DS-3 深化 | — |

---

## 验收提示（实现单项时）

- Host 单测：`npx vitest run src/windows/{schema-diff,data-transfer,data-sync}`
- E2E：`pnpm e2e:schema-diff` · `pnpm e2e:data-transfer` · data-sync journey specs
- 新增 URL prefill 需覆盖 `windowManager` 单测 + 对应窗口 mount 测试
- i18n：开发期仅 `en.ts` + 可选 `zh-CN.ts`
