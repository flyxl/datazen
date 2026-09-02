# Data Sync UI/UX 代码审查

> 审查范围：`src/windows/data-sync/` 全部组件；相关 hooks（`useMigrationWindowMenuActions`）；`src/locales/en.ts` 中 `sync.*` 键；与 `DataTransferWindow` 的模式对比；E2E 旅程 `e2e/specs/journeys/data-sync-journey.ts` 及关联 spec。  
> 审查日期：2026-08-28
> 实施更新：2026-09-02，Data Sync 已切换为与 Data Transfer 对齐的 6 步向导；下文涉及“单页工作台”的历史判断以本更新为准。

---

## 概述

Data Sync 当前实现为 **6 步向导**：用户依次在 Endpoints / Setup / Objects / Compare / Preview / Result 中选择源目标、配置选项、确认表映射、审阅行差异、预览 SQL 并执行变更集。Compare 步仍由 `inspect_data_sync` → `compare_data_sync` 驱动，审阅组件为 `MappingPanel` / `TableListPanel` / `DiffDetail`，Preview 步使用 `SqlPreview`。

**Dedicated Session 改造**（`DataSyncWindow.tsx` + `lib/dedicatedDbSession.ts`）已落地：端点使用 `ensureDedicatedSession` / `listDatabasesDedicated` / `releaseDedicatedSession`，与 Data Transfer 一致；并额外监听 `datazen:connection-closed` 与 `datazen:connections-changed`，优于 Transfer 窗口。

**架构特点**：无独立 Zustand store，全部状态集中在 `DataSyncWindow.tsx`（约 1033 行）；`SavedTasksBanner`、`SyncProgressPanel`、`ResumeSyncDialog` / `ConflictSyncDialog` 等组件 **已实现但未接入** 主窗口，i18n 中仍保留大量旧版 bulk-sync 文案，表明从旧流程迁移尚未完全收尾。

整体 UX 流程（endpoints → setup → objects → compare → preview → result）主路径完整，E2E 覆盖 PG/MySQL 双驱动的主旅程与较多边界分支；但在 **取消/成功反馈、无障碍、只读目标提示、死代码清理** 等方面仍有改进空间。

---

## 优点

1. **流程清晰、信息密度合理**  
   向导将端点、配置、对象映射、行级比较、SQL 预览和执行结果分步呈现；Compare 步分为统计条（`CompareSummary`）与表列表 + 行级差异（`TableListPanel` + `DiffDetail`），符合「先总览、再下钻」的审阅习惯。

2. **Dedicated Session 与会话生命周期**  
   `DataSyncWindow.tsx:172–184, 281–311` 与 `dedicatedDbSession.ts` 正确管理独立会话；`connection-closed` 跨窗口监听（`:99–110`）可在主连接关闭时清理端点状态，避免脏 session。

3. **安全与 destructive 操作分层确认**  
   - 启用 Delete 选项：`OptionsBar.tsx:44–46` → `deleteConfirmOpen` 对话框（`:965–981`）  
   - 执行含 DELETE 的变更集：`executeConfirmOpen`（`:1008–1030`）  
   - 目标只读时禁用 Execute（`ExecuteBar.tsx:26, 36–39`）

4. **与异构/不兼容场景的引导**  
   目标连接列表禁用 unsupported pair（`DataSyncWindow.tsx:148–161`）；映射行内提供 Schema Diff / Data Transfer 入口（`MappingPanel.tsx:86–107`）；路径标签 `data-sync-path` 区分 Direct / IR。

5. **可测试性基础良好**  
   关键控件具备 `data-testid`（compare、mapping-row、summary、start、preview 等）；单元测试 `DataSyncWindow.test.tsx` 覆盖 idle、校验、compare→execute、schema picker、heterogeneous target；E2E 有完整 PG/MySQL journey 与 `data-sync-edge-cases.ts`。

6. **大表行差异分页**  
   `DiffDetail.tsx:14, 44–46, 209–230` 对 500 行分页，避免一次性渲染过多 DOM。

---

## 问题清单

### 高

| # | 位置 | 现象 | 建议 |
|---|------|------|------|
| H1 | `DataSyncWindow.tsx:546–550` | `handleCancel` 仅调用 `cancelDataSync(jobId)`，**不重置 `syncState`**、不清除 loading UI。若后端 cancel 不导致 compare promise reject，界面会永久停在 inspecting/comparing（`:827–839`），Cancel 按钮不消失。E2E（`DS-EDGE-012`、`runCompareCancelBranch`）依赖 cancel 后 UI 恢复，存在 flaky 风险。 | Cancel 后显式 `setSyncState(mappingResults.length ? 'compared' : 'idle')`；可选展示 `sync.compareCancelled` 提示；对 in-flight promise 使用 abort/cancel token。 |
| H2 | `DataSyncWindow.tsx:804–805` | 切换 `sourceId` / `targetId` **未调用 `resetCompareState`**（对比：`handleSourceDatabaseChange` 等在 `:418–447` 会 reset）。用户改连接后仍看到旧 mapping/compare 结果，可能误执行。 | 为 `onSourceChange` / `onTargetChange` 包装 handler，变更时 reset mapping、syncState、selectedTable。 |
| H3 | `SavedTasksBanner.tsx`、`SyncProgressPanel.tsx`、`ResumeSyncDialog.tsx` | 三个组件 + `getSyncTasks` IPC（`commands/sync.ts:84`）**完全未接入** `DataSyncWindow`，但 i18n 仍保留 `sync.savedTasks`、`sync.progressTitle`、`sync.resume*` 等大量键。用户无法恢复中断任务；代码与文案形成「幽灵功能」。 | 要么接入 resume/progress 流程，要么删除未使用组件并清理 i18n；避免维护两套 mental model。 |
| H4 | `DataSyncWindow.tsx:700, 821–842` | 执行成功后 `syncState === 'done'`，**无成功 toast/banner/状态条**；用户仅能从 summary 数字变化推断。与 Transfer 的 `result` 步骤形成反差。 | 增加 `sync.executeDone` 成功提示（StatusBar 或 CompareSummary 内联）；可选「Re-compare」CTA。 |
| H5 | `e2e`（全库 grep `targetReadOnly`） | UI 层 `sync.targetReadOnly`（`ExecuteBar.tsx:36–39`）**无 Host E2E**；`data-sync-real.ts` 仅测 legacy IPC 移除。只读目标用户可能不知道为何无法 Execute。 | 增加 journey：只读目标连接 → compare 成功 → `data-sync-start-disabled` + 只读文案可见。 |

### 中

| # | 位置 | 现象 | 建议 |
|---|------|------|------|
| M1 | `DataSyncWindow.tsx:215–279, 335–338` | 库列表加载失败时 `catch` 静默置空数组，**无错误提示**；用户可能在未选库时点 Compare 得到 `sync.selectDbRequired`（`:461–464`），根因不明。 | 失败时 `setErrorMsg` + toast；EndpointsBar 库 Select 旁显示 retry。 |
| M2 | `DataSyncWindow.tsx:281–311` | `ensureDedicatedSession` 在 `useEffect` 中调用，**失败无 UI 反馈**（仅 compare 时 `refreshEndpointSessions` 才报错）。 | session 建立失败时 inline 错误态或禁用 Compare。 |
| M3 | `DiffDetail.tsx:37, 95–907` | 切换选中表时 **`page` 状态不重置**（无 `useEffect` 依赖 `table`）。从大表第 N 页切到小表可能空白。 | `useEffect(() => setPage(0), [table.sourceTable])`。 |
| M4 | `DiffDetail.tsx:127–130` | 列头使用 `sync.colN`（Col 1, Col 2…），**非真实列名**，审阅宽表时认知成本高。 | 若后端/compare 结果含列名 metadata，替换为真实 header；否则在 tooltip 中展示。 |
| M5 | `MappingPanel.tsx:60–117` | `incompatibleReason` 作为 flex 行内第四块（`:110–116`），**破坏行布局**（与固定宽度 status/actions 列混排），长 reason 时 UI 错位。 | 将 reason 移到表名下方第二行，或单独 tooltip 列。 |
| M6 | `DataSyncWindow.tsx:879–892` vs `EndpointsBar.tsx:64–66` | 右侧 Tab（Row diff / SQL preview）为裸 `<button>`，**无 `role="tablist"` / `aria-selected`**；筛选按钮（`TableListPanel.tsx:80–89`）无 `aria-pressed`。 | 采用与 app 内其他 Tab 一致的模式；filter 按钮加 `aria-pressed={filter === f}`。 |
| M7 | `EndpointsBar.tsx:73–79, 118–124` | 连接 Select 有 `<label>`，**库 Select 无可见 label**（仅 placeholder），屏幕阅读器依赖 placeholder，不符合 a11y 最佳实践。 | 为库选择添加与 schema 相同的 label 结构（`:83–84`）。 |
| M8 | `DataSyncWindow.tsx:736–738` | 「Copy report」**无复制成功反馈**（对比 `SqlPreview.tsx:84–92` 有 copied 状态）。 | 复用 toast 或按钮短暂 `common.copied`。 |
| H*→M | `DataSyncWindow.tsx:649–651, 674–676` | 回滚错误文案为 `sync.failedMsg + common.cancel`（「Failed: Cancel」），**语义混淆**（用户未点 Cancel）。 | 使用专用键如 `sync.rolledBack` / `sync.transactionAborted`。 |
| M9 | `DataSyncWindow.tsx:908–921` | SQL Preview Tab 在 session 未就绪时**无 empty/fallback**（条件渲染为空）。 | 显示 `sync.sessionRequired` 或 loading。 |
| M10 | `DataSyncWindow.tsx`（根节点） | 缺少 `data-testid="data-sync-window"`（Transfer 有 `data-transfer-window`，`:558`）。 | 根 div 增加 testid，便于 E2E 窗口级断言。 |

### 低

| # | 位置 | 现象 | 建议 |
|---|------|------|------|
| L1 | `DataSyncWindow.tsx:944` | StatusBar 硬编码 `DataZen v0.1.0`，与其他窗口/应用版本可能不一致。 | 从 package/app metadata 读取或移除版本号。 |
| L2 | `src/locales/en.ts:1029–1174` | 大量 **未使用 i18n 键**：`sync.overwriteRetiredBanner`、`sync.startSync`、`sync.loadingSchemaDiff`、`sync.kind.*`、`sync.group.*` 等（旧 bulk/schema 内嵌 UI）。 | 确认无引用后删除或迁入 docs；减少翻译负担。 |
| L3 | `ResumeSyncDialog.tsx:39–40` | 对话框正文混用 **中文标点「：」** 与英文 UI 字符串，i18n 不一致。 | 文案全部走 `t()`，标点随 locale。 |
| L4 | `MappingPanel.tsx:61` | React key 为 `` `${row.status}:${name}` ``，同名不同 status 时可能冲突（边缘 case）。 | 使用 `tableKey(row)` 或 `sourceTable` 稳定 id。 |
| L5 | `OptionsBar.tsx:10–52` | 比较前即展示 Insert/Update/Delete，**新用户可能不清楚选项仅影响 compare 后的行选择与 SQL**。 | 增加简短 hint 或 compare 前 disabled + tooltip。 |
| L6 | `DataSyncWindow.tsx:741–783` | AI Explain diff 功能完整，但 **E2E/单测均未覆盖** `data-sync-explain-diff`。 | 单测 mock AI；E2E 登记为需 AI 配置的 optional journey。 |
| L7 | `TableListPanel.tsx:71–77` | 搜索框无 `aria-label` / 关联 label（仅 placeholder `common.searchTables`）。 | 添加 visually-hidden label。 |
| L8 | `DataSyncWindow.tsx:657–659` | `generateDataSyncSql` 失败时 **静默 fallback** 到 `applyDataSync`，用户无感知执行路径切换。 | 开发模式 log 或 Execute 后 status 注明 applied via fallback。 |

---

## 与 Data Transfer 的差异 / 应对齐项

| 维度 | Data Transfer (`DataTransferWindow.tsx`) | Data Sync | 建议 |
|------|------------------------------------------|-----------|------|
| 导航模式 | 六步 Wizard + 步骤指示器（`:561–593`） | 六步 Wizard + 步骤指示器 | 已对齐；Sync 的 Compare 步专注行级 Diff |
| 首次打开教育 | `TransferLimitationsDialog`（`:81–85`） | 无 | 考虑 `sync.overwriteRetiredBanner` 或能力说明一次性 Dialog |
| 只读目标 | endpoints 步骤内 **banner**（`:643–647` `transfer.readOnlyHint`） | 仅 ExecuteBar 文案（`:36–39`） | 在 `EndpointsBar` 目标区增加同等显眼 banner |
| 根 testid | `data-transfer-window` | 无 | 对齐添加 `data-sync-window` |
| Dedicated session | 相同模式（`:137–241`） | 相同 + connection-closed 监听 | Sync 已领先；Transfer 应对齐 cross-window 监听 |
| 连接列表刷新 | 无 `connections-changed` | 有（`:122–128`） | Transfer 应对齐 |
| 执行结果 | 独立 `result` 步骤展示 | 无明确 done 态 UI | Sync 增加 execute success 区域 |
| 错误处理 | `refreshEndpointSessions` 不包 try/catch（`:264–278`） | 有 connectFailed 对话框（`:201–204`） | Transfer 可对齐 |
| Schema 选择 | Transfer 无 PG schema picker | Sync 有（`:313–379`） | 产品差异，保持 |
| 异构引导 | pairing.reason + path | unsupportedPair + useTransferHint | 已对齐 |

---

## E2E / 可测试性建议

### 已有覆盖（简要）

- **`data-sync-journey.ts`**：PG/MySQL 完整旅程——校验分支、Delete 启用确认、六步导航、compare、review（filter/search/copy/mapping toggle）、preview、execute、delete execute 确认、PG schema picker。
- **`data-sync-edge-cases.ts`**：selectBoth、cannotSameDb、unsupportedPair、delete 确认、swap、compare cancel。
- **`DataSyncWindow.test.tsx`**：核心 IPC 与 UI 状态（Vitest）。

### 覆盖缺口（UI 视角）

| 缺口 | 建议 spec |
|------|-----------|
| 只读目标 Execute 禁用 + 文案 | 新 case：`read-only` 连接作 target → compare → assert `data-sync-start-disabled` + `sync.targetReadOnly` |
| 执行成功 done 态 | execute 后 assert 成功提示（实现 H4 后补） |
| AI Explain diff | optional：`data-sync-explain-diff` + mock AI / skip if not configured |
| Incompatible 表 → Schema Diff / Transfer 按钮 | seed incompatible schema → click `data-sync-open-transfer` / schema diff（可 mock window open） |
| DiffDetail 分页 | seed >500 row diffs → `sync.pageNext`（或单元测试覆盖） |
| 端点变更清除旧结果 | compare 后换 source → mapping 应清空 |
| Cancel 后 state 恢复 | 强化 `DS-EDGE-012`：assert 非 loading 且可再次 Compare |
| 根窗口 testid | 添加后用于 window 级 wait |

### 可测试性改进

1. 为 `TableListPanel` filter 按钮增加 `data-testid={`data-sync-filter-${f}`}`。
2. 向导步骤使用 `data-sync-step`，底部导航使用 `data-sync-back` / `data-sync-next`；Compare 与 Preview 分离，不再依赖右侧 Tab。
3. 将 `DataSyncWindow` 状态机（idle/inspecting/compared/done）暴露为 `data-sync-state` 属性供 E2E 断言。

---

## 优先修复建议（Top 5）

1. **修复 Cancel 流程（H1）**  
   `handleCancel` 必须恢复 UI 状态；必要时统一 compare/execute 的 in-flight 处理。这是 E2E 稳定性与用户体验的基础。

2. **端点变更时 reset compare 结果（H2）**  
   防止 stale mapping 导致误操作；改动面小、收益高。

3. **清理或接入 dead components（H3）**  
   决定 resume/progress 是否在 Diff Workspace 保留；若不保留，删除 `SavedTasksBanner`、`SyncProgressPanel`、`ResumeSyncDialog` 并 prune i18n，降低维护成本。

4. **执行成功 / 失败反馈（H4 + M8 + rolledBack 文案）**  
   明确的 done banner、rolledBack 专用错误键、copy report 反馈。

5. **只读目标 UX 与 E2E（H5 + 对齐 Transfer banner）**  
   在 `EndpointsBar` 目标侧展示 read-only hint；补 Host E2E，闭合 AGENTS.md 要求的 UI 路径覆盖。

---

## 附录：文件与职责速查

| 文件 | 职责 |
|------|------|
| `DataSyncWindow.tsx` | 主容器、6 步状态机、IPC 编排、对话框 |
| `EndpointsBar.tsx` | Endpoints 步源/目标/库/schema |
| `OptionsBar.tsx` | Insert/Update/Delete 选项 |
| `MappingPanel.tsx` | 表映射摘要与 include 勾选 |
| `CompareSummary.tsx` | 差异统计、Copy report、Explain |
| `TableListPanel.tsx` | 表列表 filter/search |
| `DiffDetail.tsx` | 行级差异表格与分页 |
| `SqlPreview.tsx` | 服务端/客户端 SQL 预览 |
| `ExecuteBar.tsx` | 选中行计数与 Execute |
| `SavedTasksBanner.tsx` | **未接入** — 保存任务列表 |
| `SyncProgressPanel.tsx` | **未接入** — 表级进度对话框 |
| `ResumeSyncDialog.tsx` | **未接入** — 恢复/冲突对话框 |

相关 hook：`src/hooks/useMigrationWindowMenuActions.ts:32`（菜单打开 Sync 窗口）。  
无 `src/stores/*` 专用 store。
