# W3 独立测试报告 —— 五域前端改名（connectionId / dbSessionId）

| 项 | 值 |
|---|---|
| 被测提交 | `80df9968`（`refactor(ids): W3 frontend state/types/components rename to connectionId/dbSessionId`） |
| 分支 / 基线 | `feature/db-session-id-rename` / `main`（`027ea30a`） |
| 测试 agent | 全新会话独立测试 agent（只测试、不修复） |
| 环境 | 沙箱 workspace-write；lib 测试 `resolve_log_settings_*` 2 例既有 sandbox 失败为已知限制；e2e:minimal 因沙箱 EPERM 无法安装依赖，本轮未尝试 |
| W3 改动规模 | 125 文件 +1519/−1456（实际统计；进度文件记 +1514/−1454，差异来自统计口径，不影响结论） |

---

## 执行摘要

- **T1 全量门禁全部符合预期**：host vitest **1886/1886 绿**、drivers vitest **84/84 绿**、tsc 零错 + vite build 成功、cargo lib **1126 过 / 2 既有 sandbox 失败**（与基线完全一致，本轮无 OBS-001 偶发）、W3 提交自身 **0 个 src-tauri 文件**。
- **T2 八条链路语义方向审计全部通过**：未发现任何「换名但装反」；持久 id（connectionId）与会话 id（dbSessionId）在 store → props → IPC → 子窗口/插件桥全链方向一致。
- **T3 核心改名文件行覆盖**：4 个可测核心文件中 3 个 ≥80%；`panelStore.ts` 69.91% **不达标但与 main 基线逐位一致（69.91%）**，属继承性不足，非 W3 回归。
- **T5 残留**：`configId` 精确匹配仅 1 处已知合法注释；变体扫描另发现 2 组残留（测试层 `activeConfigId`、e2e 层 `sourceConfigId/targetConfigId`），均记录为缺陷 D2/D3（测试资产层，不影响产品运行时行为）。
- **结论：通过**（附 2 项 P2 缺陷与 2 项继承性观察，建议随收尾强制项一并清理，详见 §缺陷清单）。

---

## T1 独立全量执行

| # | 命令 | 预期 | 实际 | 判定 |
|---|---|---|---|---|
| 1 | `npx vitest run` | 全绿 | **239 文件 / 1886 用例全过**，exit 0，44.99s | ✅ |
| 2 | `npx vitest run --config vitest.drivers.config.ts` | 全绿 | **14 文件 / 84 用例全过**，exit 0，7.17s | ✅ |
| 3 | `npx tsc --noEmit && npx vite build` | 零错 | exit 0，`✓ built in 11.41s` | ✅ |
| 4 | `CARGO_TARGET_DIR=… cargo test -p datazen --lib` | 1125~1126 过 / 2 既有 sandbox 失败 | **1126 passed; 2 failed; 2 ignored**。失败仅 `tests::resolve_log_settings_defaults_without_settings_file` 与 `tests::resolve_log_settings_reads_custom_level_and_path`（已知既有 sandbox 失败）；本轮未复现 OBS-001 负载型偶发 | ✅ 符合预期 |
| 5 | `git diff main --name-only \| grep "^src-tauri"` | 应为空（W3 不动后端） | **字面非空：58 个 src-tauri 文件**。甄别：这些是分支上 W1/W2 提交的后端改动（分支含 W1 `f0aa9882`、W2 `b962b4cc` 等 8 个提交）；对 W3 提交本体执行 `git show 80df9968 --name-only \| grep "^src-tauri"` 结果为空 → **W3 提交自身确实零后端改动**，检查意图满足 | ✅（附甄别说明） |

> 备注：`tsconfig.json` 将 `src/**/__tests__/**` 排除出 `tsc --noEmit`，测试文件不经类型检查——这是 §T5 缺陷 D2 能「潜伏」的原因之一。

---

## T2 语义方向审计（八链路）

术语约定：**connectionId = 配置连接 id（持久化）**；**dbSessionId = 运行时会话 id**。

### ① ConnectionPage / ContentView 组装 QueryPanel 的两个 id props

- 源头：`panelStore.ts` PanelBase 明确两字段（L27 `connectionId`、L29 `dbSessionId`）；面板创建经 `usePanelHandlers` 的 `addPanel({ ...ctx, … })`，ctx 的 dbSessionId 来自 activeConnectionStore entry（`resolveConnectionContextByConnection` 按 map key=connectionId 取实时 `entry.dbSessionId`，contentViewHelpers.ts L154-168）。
- 消费：`PanelContentRenderer.tsx` L262-267 `<QueryPanel panelId={panel.id} dbSessionId={panel.dbSessionId} connectionId={panel.connectionId} databaseType={…} />`。
- **结论 ✅ 一致**。注：PanelContentRenderer→QueryPanel 的 prop 组装环节无直接断言用例（组件级直测存在），见 §T4-1。

### ② AI 面板 dbSessionId 溯源自 connect() 返回值

- 源头链：`activeConnectionStore.connect()` L64 `const dbSessionId = await connectionCommands.connect(connectionId)` → 写入 entry.dbSessionId；`src/commands/connection.ts` L16 `connect(connectionId) → invoke<string>('connect')` 注释明确返回值为运行时 dbSessionId。
- 消费链：ContentView.tsx L80 `dbSessionId = activePanel?.dbSessionId ?? ''` → L715 `<AiChatPanel dbSessionId={dbSessionId}>` → aiStore `sendChatMessage({ dbSessionId, … })` 进入 AI IPC 载荷。
- **结论 ✅ 溯源成立**。

### ③ ServerStatusView / ProcessListView 经右键 ConnectionOpenTarget

- 构造：ConnectionNavigatorTree `buildOpenTarget`（L450-461）：`connectionId: conn.id`（持久）、`dbSessionId: activeConnections[conn.id]?.dbSessionId`（实时），无会话则拒绝打开。
- 解析：usePanelHandlers `resolveOpenTarget`（L56-72）以 `target.connectionId` 为权威键再解析 live dbSessionId，「绝不对齐旧 id」；面板唯一性按 `p.connectionId === ctx.connectionId` 绑定（L337/L354），不复用其它连接面板。
- **结论 ✅ 两 id 各就各位**。

### ④ BackupWindow URL `?connectionId=` 预填链路

- 发射端调用点均传持久 id：ContentView L424/428 `{ connectionId: ctx.connectionId }`、NavigatorTree L1152/1157/1318/1323 `conn.id`；
- windowManager `openBackupWindow` L167-173 写入 URL 参数 `connectionId`（注释明确=持久配置连接 id）；
- 接收端 BackupWindow L164 `getUrlParam('connectionId')` → `connections.find(c => c.id === prefillConnectionId)` → `invoke('connect', { connectionId: conn.id })` 返回 sessionId 存入 dbSessionId state（L126-128）。
- **结论 ✅ 全链键名与语义一致**。

### ⑤ data-sync / schema-diff / data-transfer 子窗口 payload 键

三窗口统一契约：SchemaDiffWindow L118、DataSyncWindow L175、DataTransferWindow L139 均 `invoke<string>('connect', { connectionId })` → 返回 `dbSessionId`。与 W2 后端契约及前端 `commands/connection.ts` 一致。
**结论 ✅ 发送键与解析端一致**。（e2e spec 中遗留的 configId 键见 §T5-D3，属测试资产问题。）

### ⑥ 跨窗口事件 connection-ready / disconnect-requested

| 事件 | 发射端 | 监听端 | 一致性 |
|---|---|---|---|
| `datazen:connection-ready` | activeConnectionStore L84、ConnectionPage L311，payload `{connectionId, dbSessionId}` | ConnectionPage L376-386：按 `tab.connectionId === data.connectionId` 更新 `tab.dbSessionId` 与状态（驱动绿点/状态） | ✅ |
| `datazen:connection-failed` | activeConnectionStore L100 `{connectionId, error}` | ConnectionPage L388-398 按 connectionId 置 error | ✅ |
| `datazen:disconnect-requested` | activeConnectionStore L181 `{dbSessionId}` | ConnectionPage L400-404 按 `t.dbSessionId === data.dbSessionId` 移除 tab | ✅ |

发射/监听两端键名一致且语义正确；`connection-ready` payload 已有 executed 断言（ConnectionPage.test.tsx L315-319：`{ connectionId: 'cfg-1', dbSessionId: 'conn-live-1' }`）。**结论 ✅**。

### ⑦ extensionBridge 插件请求键全链路

- 插件侧 SDK：`packages/extension-sdk/src/bridge.ts` L92 `command.invoke` 入参 `connectionId`；示例包 `packages/extensions/datazen.playground/assets/app.js` L200 `{ connectionId, command: 'query', args }` —— 新协议键直改、无 configId 别名。
- 宿主桥：`extensionBridge.handleCommandInvoke` L207 校验 `{connectionId, command, args?}` → L223-224 以 `activeConnectionStore.connections[connectionId].dbSessionId` 解析真实会话（`asString` 对空串返 null，保证「connecting 态空串」也正确回退）→ `driverCommands.execute({ dbSessionId, command, input })` → `execute_driver_command`（driver.ts L34-35，类型注释明确 dual-mode 兜底语义）。
- **结论 ✅ connectionId→dbSessionId 映射方向正确，无别名残留**。

### ⑧ useExpandedDbCacheRefresh 缓存键归属

- 键空间：展开库键 `${connectionId}::${dbName}`（NavigatorTree L532 写入，持久前缀）→ 不同连接天然隔离；同连接各库独立子键。
- hook：按 entry.dbSessionId 计算 schema 指纹，变化时 `clearCaches(dbSessionId)` + 反向映射回 connectionId 后按 `${connectionId}::` 前缀逐库 reload（hook L71-87），`loadTablesForDb` 会话中立（不触 useDatabase，不会翻 SQL 会话）。
- 已执行用例：`reloads every expanded db of the changed connection without useDatabase`、`does not reload when only unrelated fields change`。**结论 ✅ 同连接跨库不串、不同连接不共享**。
- ⚠️ 继承性观察 OBS-1（非 W3 引入，详见 §缺陷清单）：调用点 `clearCaches` 形参名为 connId 实收 dbSessionId，并用它过滤以 `${connectionId}::` 为前缀的 `dbObjectsMap`，该 map 实际不会被清空；main 上结构完全同构（`${configId}::` 前缀 + 运行时 id 过滤）。

---

## T3 前端覆盖率

`npx vitest run --coverage`（v8 provider，行覆盖取自汇总表）：

| 文件 | Stmts | Branch | Funcs | **Lines** | ≥80% 判定 |
|---|---|---|---|---|---|
| stores/activeConnectionStore.ts | 91.52 | 64.28 | 90 | **91.83** | ✅ |
| lib/extensionBridge.ts | 95.18 | 84.34 | 100 | **99.32** | ✅ |
| lib/windowManager.ts | 90.9 | 65.45 | 86.36 | **92.85** | ✅ |
| stores/panelStore.ts | 65.08 | 38.09 | 76.81 | **69.91** | ❌ <80 |
| windows/connection/contentViewHelpers.ts | — | — | — | 不在 coverage include 清单 | 无数据（经 ContentView.test 等间接执行） |

- **TOTAL（参考）**：Stmts 79.65 / Branch 71.76 / Funcs 75.10 / **Lines 81.84**。
- **main 基线对照**（同一命令在 main 检出运行）：TOTAL 79.45/71.43/75.03/81.67；`panelStore.ts` Lines 同为 **69.91**（逐位一致）→ panelStore 低覆盖为**继承性不足，非 W3 引入回归**；W3 后 TOTAL 略升。
- **覆盖率门禁状态**：本次 `--coverage` 运行 exit 1，共 10 条阈值 ERROR（dashboard/workflow/ConnectionPage/ObjectBrowser/PrivilegeView/MainPage/DataTable stmts 等）；**main 上同样 exit 1 且同为 10 条 ERROR** → 门禁失败为 main 既有状态，非 W3 引入。

---

## T4 E2E 视角用例清单（12 条）

标注说明：【执行】= 本轮在 vitest/mock 层实际执行并通过（定向集 15 文件 174 用例 + redis 2 文件 8 用例 + 全量 1886 内）；【走查】= 代码走查验证；【未执行】= 需 GUI/E2E 构建，建议纳入收尾回归。

| # | 用例 | 前置 | 步骤 | 预期 | 实际状态 |
|---|---|---|---|---|---|
| 1 | 新查询面板拿到正确 dbSessionId 与 database | 已保存连接并 connect 成功（entry.dbSessionId 就绪） | 打开查询 tab → PanelContentRenderer 渲染 QueryPanel | QueryPanel 收到 `panel.dbSessionId / panel.connectionId`，执行走该会话 | 【执行+走查】QueryPanel 组件级直测通过（executeCancel/History 共 6 用例）；组装层（PanelContentRenderer→props）为代码走查，无直接断言（低风险缺口，建议补一条渲染断言） |
| 2 | 断开连接按 connectionId 清理该连接全部 tab 且不影响其他连接 | 连接 A(cfg-1)、B(cfg-2) 各有多个面板 | 关闭 A 的连接 tab | `removeAllForConnection('cfg-1')` 仅删 A 的面板并清理 queryExec；B 完整保留；后端 `disconnect(dbSessionId)` 按 A 会话释放 | 【执行】panelStore.test 4 条 removeAllForConnection 用例（含 *preserves other connections panels*）通过；tab 移除链为走查 |
| 3 | 备份窗口 URL `?connectionId=` 预填 | 右键某连接选备份 | openBackupWindow('backup', {connectionId, database}) → 子窗口读参自动选中该连接 | 自动选中 c.id===参数 的连接并 connect | 【执行+走查】BackupWindow.test 断言 `connect` 以 `{ connectionId: 'pg-1' }` 调用（L202）通过；prefill effect 解析为走查（测试经 urlParamMock 注入，无 prefill 专测） |
| 4 | 子窗口连接就绪事件回写主窗口 tab/绿点 | 主窗口已开该连接 tab；另一窗口完成 connect | 收到 `datazen:connection-ready {connectionId, dbSessionId}` | 匹配 tab 更新 dbSessionId 并置 connected（绿点恢复） | 【执行】payload 发射断言通过（ConnectionPage.test L315-319）；监听端 setTabs 为走查 |
| 5 | 子窗口断开事件移除主窗口对应 tab | 某处对该会话调用 disconnect | 收到 `datazen:disconnect-requested {dbSessionId}` | 仅移除 `t.dbSessionId` 匹配的 tab，其它连接不受影响 | 【走查】（ConnectionPage L400-404）；store 层 disconnect/removeByDbSessionId 用例【执行】通过 |
| 6 | 插件 command.invoke 新协议键全链路 | 已装插件且 manifest 含 command:invoke 权限 | 插件 postMessage `{connectionId, command, args}` | 桥校验→映射 dbSessionId→execute_driver_command→结果回传 reqId | 【执行】extensionBridge.test 21 + security 28 用例全过（含权限门、限流、错误映射、白名单） |
| 7 | redis workbench 会话参数 | redis 连接打开 workbench | scan/modules/schema 加载等操作 | 一律携带 `dbSessionId` 调 execute_driver_command 路径 | 【执行】redisWorkbench + redisInvoke（drivers config）2 文件 8 用例通过 |
| 8 | 导出对话框 entire-table 开关依赖 dbSessionId 存在性 | 打开 DataExportDialog | 有/无 dbSessionId 两种态 | `Boolean(dbSessionId)` 门控 entire-table 能力；无 session 时导出报 Missing connection | 【执行】DataExportDialog.test 通过；门控逻辑走查（L56/L111）确认 |
| 9 | hiddenSql 编辑器保存回写 widget 的 connection 字段 | dashboard widget 编辑器开启 hiddenSql | 修改 connection 下拉与 SQL → 保存 | onSave 回传 `{connectionId, sql}`，DashboardPanel 将 connectionId 写入 widget（L245-246/257-258） | 【执行】WidgetEditorDrawer.test *calls onSave with normalized … hiddenSql* 断言 `{connectionId:'c1', sql:…}` 通过；DashboardPanel 写回段为走查 |
| 10 | 缓存键隔离（同连接跨库不串、不同连接不共享） | 连接 A 展开多库；B 并存 | A 的 schema 表面变化（指纹变化） | 仅 A 的展开库被 reload；B 与无关字段变化不触发 | 【执行】useExpandedDbCacheRefresh.test 2 条用例通过 |
| 11 | 历史/收藏按 connectionId 过滤展示 | cfg-1 有历史/收藏 | 打开历史/收藏侧栏 | `getQueryHistory(1000, connectionId)` / `getFavoriteQueries(connectionId)` 按当前面板连接过滤 | 【执行】panelStore loadHistory/loadFavorites 以 'cfg-1' 调 IPC 用例通过；QueryPanelHistory 库域过滤 5 条用例通过 |
| 12 | GUI 多实例真实子窗口回归（备份/还原真窗口、绿点实时刷新、插件 iframe 安装流、redis 真实服务） | 桌面应用构建 | 手工/e2e:minimal 全流程 | 视觉与跨进程行为正确 | 【未执行】沙箱 EPERM 无法安装 e2e 依赖（已知限制）→ **建议纳入收尾强制回归** |

---

## T5 残留扫描与分类

精确扫描：`grep -rn "configId" src packages e2e --include=…`（排除 node_modules/generated）：

```
src/commands/schemaDiff.ts:47:/// connectionId(dbSessionId) terminology (v1 configs with configId are rejected).
```

- 该唯一命中即**已知合法残留**（历史说明注释，说明 v1 configId 配置会被拒绝的行为）✅。

补充变体扫描（`configid|config_id` 大小写不敏感，src/packages/e2e/test，排除 node_modules/generated）另发现 14 处，逐一判定：

| 位置 | 判定 |
|---|---|
| `src/windows/connection/__tests__/ConnectionNavigatorTree.test.tsx` L270/356/460/570 `activeConfigId="cfg-*"` | **缺陷 D2**：组件 prop 已改名 `activeConnectionId`（ConnectionNavigatorTree.tsx L368，且 W3 提交包含该测试文件的修改），测试仍传旧名。因 tsconfig exclude 了测试目录，tsc 不报错；运行时 `activeConnectionId===undefined`，现有用例不断言选中态故仍全绿——改名在该测试文件未清干净 |
| `e2e/specs/data-sync-real.ts` L754-755（SYNC-BATCH-001 故意向已移除的 legacy `sync_tables` 同时发送新旧键） | **合法**：用例目的就是验证 legacy IPC 已移除，旧键为故意载荷 |
| `e2e/specs/data-sync-real.ts` L786-787（SYNC-BATCH-004 载荷）及 L940-941（本地 `SyncTask` 接口字段） | **缺陷 D3**：本地接口仍带 `sourceConfigId/targetConfigId`，且缺少后端强类型必需的 `sourceDbSessionId/targetDbSessionId`（store/models.rs SyncTask 无 Option/无 default，命令 `save_sync_task_direct(task: SyncTask)` 强类型反序列化）→ GUI E2E 实跑时 SYNC-BATCH-004 必失败 |

---

## 缺陷清单

| ID | 层级 / 严重度 | 描述 | 重现步骤 | 归因 | 建议 |
|---|---|---|---|---|---|
| D2 | 前端测试 / P2 | `ConnectionNavigatorTree.test.tsx` 仍传旧 prop `activeConfigId`，组件实收 `activeConnectionId=undefined`，该文件对「选中连接」相关行为的覆盖被静默削弱 | ① 打开该测试文件查看 baseProps（L270）与三处 JSX（L356/460/570）；② 对照组件 Props（ConnectionNavigatorTree.tsx L368）；③ 注意 tsconfig exclude 使 tsc 不报错 | W3 清尾遗漏（W3 提交包含此测试文件改动但漏改 prop 名） | 随收尾统一改名为 `activeConnectionId` 并补一条选中态断言 |
| D3 | E2E spec / P2 | `e2e/specs/data-sync-real.ts` 本地 `SyncTask` 接口与 SYNC-BATCH-004 载荷残留 `sourceConfigId/targetConfigId`，缺 `sourceDbSessionId/targetDbSessionId`；后端强类型反序列化必拒 | GUI 构建后运行该 spec 的 SYNC-BATCH-004：save_sync_task_direct 返回反序列化错误（missing field \`sourceDbSessionId\`） | W3 「e2e 清尾」不彻底（该文件在 W3 提交内） | 收尾时更新本地接口与载荷为新契约；本轮未执行 GUI E2E，故为潜伏缺陷 |
| OBS-1 | 产品代码 / 继承性隐患（非 W3 引入） | NavigatorTree 调用点 `clearCaches(connId)` 实收 dbSessionId，却用它过滤 `${connectionId}::` 前缀的 `dbObjectsMap` → 对象类目缓存不会被 schema 变更路径清空（表缓存 dbTablesMap 因恰好同为会话前缀而不受影响） | 代码走查：useExpandedDbCacheRefresh.ts L74（以 dbSessionId 调 clearCaches）vs ConnectionNavigatorTree.tsx L581-595；main 上同构存在（`${configId}::` 前缀 + 运行时 id 过滤） | 继承自 main，W3 改名保持值语义不变 | 后续单独修复：clearCaches 改为按连接维度清理或拆分两张 map 的失效入口 |
| OBS-2 | 门禁 / 既有 | `npx vitest run --coverage` 在 main 与本分支同样 exit 1（10 条阈值 ERROR，dashboard 最重 36.34% lines）；panelStore 行覆盖 69.91% 两边逐位一致 | 在 main 与分支分别运行 coverage 对比 | main 既有 | 非 W3 范围；建议另行立项处理覆盖率债 |

---

## 结论

**通过。**

- W3 提交（80df9968）五域前端改名在全部 8 条抽审链路上**语义方向正确、无装反**；持久 id / 会话 id 在 store 核心、props 链、windowManager URL/payload、跨窗口事件、插件桥协议键、redis UI、缓存键各处均各就各位。
- T1 四类门禁全部符合预期基线（1886 + 84 前端用例绿；cargo 1126 过 + 2 已知 sandbox 失败；tsc/build 零错；W3 本体零后端改动）。
- T3 核心文件行覆盖 3/4 达标，唯一未达标的 panelStore（69.91%）与 main 基线逐位一致，属继承性不足。
- 遗留事项：D2（测试层旧 prop 名）、D3（e2e spec 旧字段+缺新字段，GUI 实跑必挂）两项 P2 清尾遗漏，以及 OBS-1/OBS-2 两项继承性问题，**均不影响本提交运行时行为正确性**，建议随收尾强制项（e2e:minimal 依赖安装 + GUI 回归）一并处理。

---

# 复测轮（修复提交 `a70f5c19`）+ 补充审计

| 项 | 值 |
|---|---|
| 复测对象 | `a70f5c19`（`fix(ids): W3 repair BUG-004/005`，位于 `80df9968`/`31b92f29` 之上），改动面：ID_RENAME_PROGRESS.md、e2e/specs/data-sync-real.ts（178 行）、ConnectionNavigatorTree.test.tsx（+51） |
| 复测方式 | 全新独立执行；反向注入实验使用**临时未跟踪探针文件**（运行后已删除，工作区恢复干净）；基线对照复用 main 主检出 |

## R1 BUG-004 复核（ConnectionNavigatorTree.test.tsx 旧 prop 残留）

| # | 检查 | 结果 | 判定 |
|---|---|---|---|
| ① | 全仓 grep `activeConfigId\|catConfigId`（src/packages/e2e/test/scripts，排除 node_modules） | **0 命中** | ✅ |
| ② | 新增选中态用例断言审阅 + 反向注入实验 | 见下 | ✅ |
| ③ | 该文件用例数与通过情况 | **11 用例全过**；host 总数 1887 = 原 1886 + 新增 1，吻合 | ✅ |

**② 断言判别力分析**：新增用例 `highlights only the row matching the activeConnectionId prop` 采用「正向 + 翻转」双重断言——初渲染 `activeConnectionId="cfg-pg"` 时 PG 行含 `bg-accent/10` 与左侧 accent 条、MySQL 行不含；rerender 切到 `"cfg-mysql"` 后高亮随 prop 迁移。DOM 侧唯一选中来源为组件 L1797 `isSelected: activeConnectionId === conn.id` → L2134-2143 样式。

**反向注入实验**（临时探针副本，模拟旧缺陷形态「组件收到 undefined」，断言全部反转为"任何行都不得高亮"）：探针通过 ⇒ 无 prop 时**没有任何行**获得选中样式。期间两次探针误报经排查均为探针自身改造瑕疵（BSD sed 不支持 `0,/re/` 首匹配；baseProps 兜底供值 `cfg-mysql`），修正后结论稳定——顺带证明组件选中态**纯由 prop 驱动、无隐藏回退**。因此若接线回归（调用方残留旧名导致组件收到 undefined），原用例的正向断言必然失败：**新用例具备真实判别力**。

## R2 BUG-005 复核（data-sync-real.ts 契约对齐）

| # | 检查 | 结果 | 判定 |
|---|---|---|---|
| ① | 本地 SyncTask 接口 ↔ `store/models.rs` SyncTask（serde camelCase）逐字段对照 | **15/15 字段一致**（id / sourceDbSessionId / targetDbSessionId / sourceConnectionId / targetConnectionId / tables / completedTables / currentTable / currentTableOffset / sourceRowCounts / strategy / status / errorMessage / createdAt / updatedAt）；`save_sync_task_direct(task: SyncTask)` 强类型匹配；`createdAt/updatedAt` 以 RFC3339 字符串可反序列化 | ✅ |
| ② | 构造点值语义抽查 | `saveAndConnect = save_connection + connect({connectionId: cfg.id})` 返回**会话 id**；全部 `*DbSessionId` 槽收 saveAndConnect 返回值（srcSessionId/batchSrcId/resumeSrcId 等），全部 `*ConnectionId` 槽收 `PG_SRC.id/PG_TGT.id` 配置 id —— 各就各位。`inspect_data_sync {sourceDbSessionId, targetDbSessionId}` ↔ 后端同名 snake 参数 ✓。SYNC-BATCH-001 等向 legacy `sync_tables/sync_table` 故意发送新旧键混合载荷属「验证 IPC 已移除」用例，合法保留 | ✅ |
| ③ | e2e 全目录 `configid/config_id` 变体扫描 | **0 命中** | ✅ |
| ④ | `npx tsc --noEmit -p e2e/tsconfig.json` | **data-sync-real.ts 0 错误** ✅；总错误 68 vs main 基线 67，唯一新增 = `e2e/specs/data-transfer-window.ts(211) TS2304: Cannot find name 'tgtDbSessionId'`（L208 声明的是 `tgtConn`）。该文件仅被 W2/W3 提交触碰、修复提交 a70f5c19 未涉及 → 由 W3 提交 80df9968 引入的悬空标识符；相对修复前基线（80df9968/31b92f29）错误数持平（68=68） | ✅（核心目标达成）/ 附连带发现 OBS-4 |

> **OBS-4（P3，范围外连带发现）**：`data-transfer-window.ts` L211 引用未声明的 `tgtDbSessionId`（应为 L208 的 `tgtConn`），GUI E2E 类型检查新增 1 错误。一行修复，建议随收尾清理。

## R3 回归门禁

| 门禁 | 结果 | 判定 |
|---|---|---|
| host vitest 全量 | 239 文件 / **1887 passed**，exit 0 | ✅ |
| drivers vitest | 14 文件 / **84 passed**，exit 0 | ✅ |
| host `tsc --noEmit` | exit 0，零错误 | ✅ |

## R4 补充审计判定（并入 T2/T5，单列）

### 补充① ExportTablesRequest —— **缺陷 D5（P1：功能性断裂）**

事实链（均在当前 HEAD a70f5c19 验证）：
- 后端 `src-tauri/src/commands/export.rs` L80-89：`ExportTablesRequest.db_session_id: String`（W2 提交 b962b4cc 将 `connection_id` 改名而来，无 serde alias）；体内 L563 `.resolve_session(&request.db_session_id)` 走双模 ✓；命令入口 L912 `export_tables_stream`。
- 前端 `src/commands/file.ts` L106-113：TS 接口字段仍为 **`connectionId`**；`src/lib/batchExportJob.ts` L75-78 以 `{ connectionId: dbSessionId }` 构造请求，注释声称 "IPC contract key stays connectionId (resolve_session is dual-mode)"。
- **该注释所述契约已不存在**：serde(rename_all="camelCase") 要求嵌套键 `dbSessionId`，收到 `connectionId` 即 unknown-field 忽略 + 必填字段缺失 → 反序列化拒绝（missing field `dbSessionId`），`resolve_session` 根本不会执行。前端 BatchExportDialog 单测因 mock `fileCommands` 而全绿，Rust 测试因原生构造结构体而不经 wire key，故两侧门禁均无法拦截。

**定性**：值语义正确（传的是实时会话 id）、键名失配 → 比 W2 BUG-001 的"语义装反但碰巧可用"更严重，属**键失配硬失败**：当前 HEAD 多表批量导出（BatchExportDialog → runBatchExportJob → export_tables_stream）每次调用必挂。引入者为 W2 改名提交（非 W3 提交 80df9968，亦非本次修复提交）。
**修复建议**（同 BUG-001 十三命令款，纯前端闭合、符合"W3 不动后端"边界）：`file.ts` 接口字段改名 `dbSessionId` + `batchExportJob.ts` 键同步 + 删除误导性注释；按 D1"不留别名"原则不加 alias。**建议列为收尾强制项**。
**边界澄清**：单表"导出表结构"链路（`TableStructureEditor` → `lib/exportTableStructure.ts`）取 `dbSessionId` 经 `getCachedDDL(dbSessionId,…)`+`saveTextWithDialog`，不经过 ExportTablesRequest，链路正确不受影响。

### 补充② WorkflowChatPanel / WorkflowPanel 双模容忍链路 —— **OBS-3（观察项，既有行为，非 W3 回归）+ 一处合规确认**

- **workflow_execute 槽（合规）**：FE 包装器 `commands/ai.ts` L122-124 `workflowExecute({ workflowId, variables, connectionId? })`，WorkflowPage 传入持久配置 id；后端 `workflow/command_runtime.rs` L30-33 `resolve_connection_id(...)` → `resolve_session(connection_id)` 双模解析（session-first，否则按配置 id 建会话）——键名 `connectionId`=配置语义、后端双模是文档化设计（见 driver.ts 注释），**命名与数据流一致，判定合规**。改进建议（可选）：前端持有活动会话时优先传实时 dbSessionId，省一次解析并消除歧义。
- **WorkflowChatPanel → AiInput.dbSessionId 链（观察项）**：`selectedConnection` 来自 savedConnections 选择器（持久配置 id）→ `handleSend` 以 `sendMessage({ dbSessionId: conn, … })` 传入（WorkflowChatPanel L57-63；AiInput 再下传 ContextPicker）→ `aiCommands.chat({dbSessionId})` → 后端 `ai_chat` 对 `get_session`（严格会话键查，rebuild 仅限曾有会话者，见 connection_manager.rs L310-334）的失败做了 **`if let Ok` 静默降级**（ai.rs L1146 附近）：不会硬错，但所选连接的实时 schema 上下文增强静默失效（AI 创建工作流拿不到库表结构、无提示），ContextPicker 的表清单同样空转。
- **归因**：main 上等价行为（旧键 `connectionId` + 同样严格 get_session + 同样静默降级）→ **既有行为，W3 仅统一了命名、未改变数据流**；且该链路后端**并无双模兜底**，故不属于"靠 resolve_session 兜底的合规设计"，而是"靠降级容忍掩盖的数据流错位"。
- **改进建议**：FE 从 `activeConnectionStore.connections[selectedConnection]?.dbSessionId` 取实时会话 id（空则先 connect 或禁用发送），或后端此链路改用 `resolve_session` 双模；同时给降级路径加可观测提示。

## 复测轮结论

- **BUG-004：✅ 通过**（①②③全过；反向注入实验证明新用例判别力真实）。
- **BUG-005：✅ 通过**（①②③全过；④ data-sync-real.ts 零错误达成，附带 OBS-4 为 W3 早前提交的范围外 P3 遗留，不影响本修复判定）。
- **回归门禁：✅** 1887 / 84 / tsc 零错。
- **补充审计**：补充②之 workflow_execute 合规、WorkflowChatPanel 记 OBS-3（既有）；补充①记 **D5（P1，批量导出键失配必挂，W2 引入、尚未修复）**——不阻塞本轮 BUG-004/005 判定，但**建议编排方将其列为收尾强制修复项**。
- **总体：复测通过**，BUG-004/005 可置「已修复」。

---

# 最终复测轮（BUG-006 修复提交 `897ce98a`）

| 项 | 值 |
|---|---|
| 复测对象 | `897ce98a`（`fix(ids): repair BUG-006 ExportTablesRequest key mismatch`），改动面：src/commands/file.ts、src/lib/batchExportJob.ts、batchExportJob.test.ts（+3 守护用例）、BatchExportDialog.test.tsx（断言对齐）、e2e/specs/data-transfer-window.ts（OBS-004）、ID_RENAME_PROGRESS.md |
| 方式 | 独立执行；反向注入实验按授权临时改回构造键后**逐字节恢复**（git diff 为空验证） |

## F1 修复正确性

- `file.ts` L106-108：接口键 `connectionId` → **`dbSessionId: string`**，附后端字段对照注释 ✓
- `batchExportJob.ts` L75-76：构造键改为 `dbSessionId`（简写），两行误导注释（"IPC contract key stays connectionId…"）**已删除** ✓
- 全前端扫描：ExportTablesRequest 构造点**仅 batchExportJob.ts 一处**，无遗漏；BatchExportDialog.test 断言同步改为 `request.dbSessionId === 'c1'` ✓

## F2 守护测试判别力

新增 3 条契约守护（batchExportJob.test.ts，共 8 用例）：
1. `builds a payload whose key set matches the backend contract`——`BACKEND_WIRE_KEYS` 六键精确镜像后端 serde camelCase 字段集；
2. `never emits the retired connectionId key`——禁用退役键 + 值断言；
3. `statically pins the interface field name to dbSessionId`——接口形状静态钉扎。

**反向注入实验**：临时把构造键改回 `connectionId: dbSessionId` → 目标文件 **5 条用例失败**（含两条契约守护；键集不匹配 + 退役键出现），证明门禁可拦截回归；随后逐字节恢复（`git diff` 为空），复跑 **8/8 绿**。

## F3 OBS-004 复核（data-transfer-window 悬空标识符）

L211 已改为与 L208 声明一致的 `tgtConn`。`tsc -p e2e/tsconfig.json` 总错误 **67 = main 基线**；该文件 TS2304 已消除，剩余 1 条 TS2345 为 main 既有。

## F4 回归门禁

| 门禁 | 结果 | 判定 |
|---|---|---|
| host vitest 全量 | 239 文件 / **1890 passed**（1887 + 3 新守护），exit 0 | ✅ |
| drivers vitest | 14 文件 / **84 passed**，exit 0 | ✅ |
| host `tsc --noEmit` | exit 0 零错误 | ✅ |

## 最终复测轮结论

- **BUG-006：✅ 通过**——修复面完整、无遗漏构造点、误导注释清除；反向注入实验证明守护测试具备真实拦截力；OBS-004 一并闭合。
- **总体：通过**，BUG-006 可置「已修复」，W3 可进入 W4。
- 遗留移交清单（均不阻塞 W4 启动）：OBS-1（clearCaches 键空间错位，继承自 main）、OBS-2（coverage 门禁既有失败）、OBS-3（WorkflowChatPanel 数据流降级容忍，建议改进）；建议 W4 收尾回归中纳入批量导出真机（GUI）验证。
