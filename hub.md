# Wave 9 跨轨台账

> 开发期产物，**交付时必须删除**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。
> Wave 8 的台账已随两轨落地销毁；**下面带走的是仍然有效的部分**（债务、E2E 清单、方法论）。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `02-keyboard` | `feature/02-keyboard` | `.worktrees/datazen-keyboard` | ⏳ 待开 |
| B `11-keyset-paging` | `feature/11-keyset` | `.worktrees/datazen-keyset` | ⏳ 待开 |
| C `i18n`（非 PRD 轨，属发布门禁） | `chore/i18n-zh-cn-and-like` | `.worktrees/dz-i18n-2` | 第三轮已交（**门禁首次全绿**：9 语种 × 2709 key）；脚本工具链入库中 |

集成分支 `feature/data-browsing-prd` HEAD = `ee5eb13ad`（Wave 8 已落地并验收：05 编辑器 + 10 结果网格）。

## Wave 9 两册的协调者裁定（契约的现状描述已过期）

### 02 键盘

| 契约写的 | 实测 | 裁定 |
| --- | --- | --- |
| Step 2「容器成为焦点宿主」：给滚动容器加 `data-dt-surface` / `tabIndex` / `aria-*` | **Wave 1（01 册）已经做完了**：`DataTable.tsx` 的 `surfaceDomProps` 里已有 `data-dt-surface`、`role="grid"`、`tabIndex: 0`、`aria-rowcount` / `aria-colcount` / `aria-activedescendant`，且已把 `surfaceProps`（含 `onKeyDown`）展开到滚动容器上 | **本册不重建 surface**。本册的工作是**键盘语义**（纯逻辑模块 + 接线 + 行容器退出 Tab 序列）。契约 K-7「不新增 role」也已过期（role 存在） |
| Step 3：行容器加 `data-dt-row-view` | **已存在**（`VirtualBody.tsx`） | 剩下的是：行容器 `tabIndex={0}` → `-1`、行级 `data-dt-selected` |
| Step 4：删除行容器上被取代的键盘分支（Enter/Space → `onRowSelect`） | **仍在** | 照做。契约给的两条删除理由都成立（Enter 的归属已裁定给「进入编辑」；行 `tabIndex={-1}` 后焦点在行号槽按钮上按 Enter 会**双触发**） |
| `EditableCell.tsx`「不触及」 | **它现在是 05 册的**（60 行兼容壳，IME 守卫在 `editors/CellEditorHost.tsx`） | **仍然不触及**（02 册当初让开它是为了不和 05 撞车，05 已接管） |

**行数预算（Wave 8 落地后的实测）**：`DataTable.tsx` **746**（≤40 净增）、`VirtualBody.tsx` **771**（≤30）、`TableView.tsx` **774**（**离 800 只剩 26 ⇒ 净增 ≤ 15**）、`useGridSelection.ts` 321、`gridSelection.ts` 403（都宽裕）。新建 `src/lib/gridKeyboard.ts`、`src/hooks/useGridKeyboardNav.ts` 各 ≤ 250。

**其它纪律**：① `src/lib/gridKeyboard.ts` **禁止** import React / store / 读 DOM（与 01 册 `gridSelection.ts` 同一纪律）；② 所有移动/扩区一律复用 01 的纯函数（`moveCellFocus` / `selectionFromAnchorFocus` / `clampCellCoord` / `coordAt`），**不新增任何区域运算**；③ `useGridSelection.ts` 的 `surfaceProps.onKeyDown` 必须在**最前面**调用 02 的 handler（§3.3 唯一入口纪律）；④ **D7 零文案断言**、**D5 反向注入**；⑤ **本批新增的硬规矩**：**至少一条用例驱动真实的 store 动作（不走 spy）**；不触及 store 动作的册，则至少一条用例驱动**真实的 hook**（不走 mock）。

### 11 键集分页

| 契约写的 | 实测 | 裁定 |
| --- | --- | --- |
| Step 5 改 `src-tauri/src/services/query_executor/mod.rs` | **该文件已被 main 撤掉**（宿主门面） | 共享层是 `packages/feature-core/src/services/query_executor/mod.rs`；IPC 侧是 `src-tauri/src/commands/schema.rs`（**466**） |
| Step 4 把 `build_select_sql` / `build_count_sql` / `format_condition` / `filter_is_complete` / `filter_join` 一起搬进 `table_read_sql.rs` | `format_condition` / `filter_is_complete` / `filter_join` / `join_filters` / `validate_filters` 都在 **`filter_sql/mod.rs`（580 行）** —— 那是 Wave 5 刚目录化出来的 | **只搬 `build_select_sql`（`query_executor/mod.rs:349`）与 `build_count_sql`（`:316`）及其私有助手**；**`filter_sql/` 一行不动**（搬回去等于撤销上一波，且与 08 的成果冲突） |
| `traits.rs` 1062 / `reuse.rs` 1159 / `types.rs` 958 | **1257 / 1500 / 991** | `traits.rs` 被 sqlserver 的 **ratchet 钉在 ≤1257（只许缩不许涨）** ⇒ 加 trait 方法的同一次提交**必须把等量的行搬出去**（纯位移），并**重新实测**钉子 + **自证**（加 1 行 ⇒ 红）；`reuse.rs` 已超限 ⇒ 采纳契约 §6.1 的**选项 (a)**：把 `impl DatabaseDriver for ReuseDriver` 整体搬到 `reuse/driver_impl.rs`；`types.rs` 只加 4 行 |
| `tableDataStore.ts` 729（距 800 只剩 1 行） | **690** | 净增 **> 40** 就必须在**同一次提交**里把 `pendingChanges` 相关动作拆到 `src/stores/tableData/actions.ts`（不许留给下一册） |

**其它纪律**：① `CursorValue` **必须带标签**（`tag="kind", content="value"`）—— `Value` 是 `#[serde(untagged)]`，`[1,2,3]` 在「Rust → 前端 → Rust」两跳里会被读成 `Bytes`，**键集边界悄悄算错**；② 顺序是硬约束（Step 1~2 只加**默认全 false / 全 None**的契约与共享实现，此时任何驱动行为逐字节不变；Step 3~5 建能力但默认不开启；Step 6~7 才让驱动与前端开启）；③ `grid.paging.*` **3 条**必须有 Rust 生产者、**追加在 `gridErrors.ts` 末尾**（插入位）、**不许**进 `PRODUCER_PENDING`；④ i18n 新 key **追加在 `en/query.ts` 末尾**（插入位）；⑤ sqlserver 的谓词用例必须断言 **SQL 里不出现行值比较**（`((a, b) > …)` 那种形状）；⑥ **不要碰 `panelStore.ts`**（本轮已从 799 拆到 **765**）；⑦ E2E 留给 R 阶段；⑧ D5 / D7 / 「至少一条真实 store 动作」。

## ★ 复验挖出的**高危缺陷**：窄写路径在生产上被 store 守卫关死，而界面仍说「可编辑」

**现象**（Tester 端到端实测：真实网格 + 真实 store 动作 + 真实 transport）：
- 无判定时，真实 `updateResultCell` **会**发出 `commitQueryResultCell`（窄路径本身没坏）；
- **有判定时（生产情形）**：结果一显示网格就去取判定，而后端今天的答案**恒为** `unknownOrigin`（没有任何驱动实现来源契约）⇒ store 抛错 ⇒ **零写语句**；
- 真实网格上：编辑器**打开**（界面说可编辑，因为并集保留了窄判定）→ 提交 → 报错 → **没有任何写语句离开进程**，用户的改动**丢掉了**。

**与偏离 2 直接冲突**：协调者上轮裁定接受「并集而非一票否决」，理由是「`SELECT * FROM <有主键单表>` 今天的网格编辑**是真能写库的**，一票否决会删掉在用功能 ⇒ 违反三维自查的『是否误伤合法同类』」。

⇒ **协调者错在只看了前端的并集判定，没看到 store 守卫会独立地把窄路径关掉**。结果：**在用功能被悄悄删掉**，而删它的守卫**恰恰是本轨自己加的**，且它「接口不是安全边界」的存在理由在这个场景下**不成立**（窄路径本来就有自己的、依赖**同一个活动目录**的复验）。

**采纳 (a)**：`updateResultCell` 是**窄通道**的写路径 ⇒ **只受窄判定约束**（`writableMapping === 'verified'` + `runtimeBinding`，它本来就有），**不受来源判定约束**；来源判定管 **origin 通道**的写路径（`preview_result_cell_change`，今天未接线）⇒ **通道关闭期间窄路径必须原样可用**。`assertResultWritable` 的语义随之收窄为「**origin 通道**拒绝了 ⇒ origin 通道的写路径关闭」。同时修 Tester 的低危缺陷 2：窄校验的 `throw` 在 try/catch 之外 ⇒ 抽屉路径 `.catch(() => undefined)` 把失败**吞掉** ⇒ 挪进 store 的错误处理，让两条路径都响亮。

## 方法论（两条，本批最贵的学费）

1. **集成层是盲区**：所有组件测试都**用 spy 替换** `updateResultCell`，`panelResultEdit.test.ts` 只在**孤立**层面断言守卫会抛 ⇒ **全仓没有任何测试驱动真实的 store 动作**。⇒ 三轮都没抓到那条高危缺陷。**新规（Wave 9 起每轨必须满足）**：**至少一条测试驱动真实的 store 动作**（不走 spy），且必须有一条例外不许被 spy 掉的路径。
2. **裁决要看全链路，不能只看自己盯着的那一段**：协调者核对了「前端的并集判定」，却没核对「store 守卫会独立否决」—— 两者合起来的结论与只看其中一个**相反**。⇒ 裁「某个设计是否误伤既有功能」时，必须把**从 UI 到落库的每一道否决点**列一遍（谁在什么条件下会抛），而不是只看其中一道。

### 新增的两条债

| # | 债务 | 影响 | 归属 |
| --- | --- | --- | --- |
| **G21** | **i18n 门禁的 stale 规则没有「符号豁免」**：判定是「译文与英文逐字节相同」，而**纯符号型 key**（`≈ {count}`、`—`）一旦是新增的，就会被迫翻成本族措辞 | 将来任何引入「纯符号 + 占位符」型 key 的册都会踩到；上一轮已实测确认 | **R 阶段**（要改 `scripts/i18n-sync-check.mjs` 的判定逻辑 ⇒ 门禁变更，单独做、单独验） |
| **G22** | **i18n 同步的工具链此前每轮重写**（且上一轮的工具把换行的值劈成两半、弄坏过文件） | 三轮各写一遍 = 三次踩同一个坑 | **已解**：脚本工具链入库（`scripts/i18n-sync-*.mjs` + `scripts/lib/i18nSync.mjs` + 自测），Wave 9–11 直接照用 |

## 一条被用户裁掉的固定要求（记下来，别再犯）

**协调者原先在每份简报里都写「纯位移也要有钉子」**（为搬出 `panelStore.ts` 的 `syncPaneFocus` 而新增 5 例专门单测）。**用户裁定：这条 ROE 太低、且白耗 CI 资源** ⇒ 已把那 5 例删掉（`38c532aa8`）。

**核实过没丢覆盖**：既有 `paneFocusEntryLifecycle.tester.test.ts`（**11 例**，覆盖每一个会动焦点的动作）与 `paneP2ContractProbes.tester.test.ts`（**7 例**，覆盖切分/关窗格/自我修复）**从调用侧覆盖了同一条规则，而且覆盖得更全** ⇒ 新文件是**严格重复**，删掉信号不减、CI 少跑 5 例。

**更正后的固定要求**：
- **纯位移（搬代码、重命名、拆文件）⇒ 不要求新增专门单测**；既有测试从**调用侧**覆盖住即可，**不为重复覆盖花 CI 资源**。
- 仍然要钉的：**新增行为**（新规则、新守卫、新契约兑现）、**修正行为**（缺陷修复）、**跨轨耦合**（本批新增的硬规矩：至少一条用例驱动真实的东西）。这三条是信号来源，不能省。
- 判据一句话：**这条用例会不会在行为变了时报警？** 会 ⇒ 值得；只是把同一件事再测一遍 ⇒ 不值得。

## 跨轨债务台账（Wave 7 之后仍未闭环）

| # | 债务 | 影响 | 归属 |
| --- | --- | --- | --- |
| G1 | 08 Step 10 前端不再剥离不完整条件 ⇔ 宿主必须同时拒绝 | 两边不同步 ⇒ 查询静默变宽 | 08 已成对落地（轨 B + 轨 A），**本波可销** |
| G2 | 09 后端三态 ⇔ 前端三态渲染「≈」 | 后端先上 ⇒ 估算与精确同形 | 本波（轨 A）落地，**本波可销** |
| G3 | 五处越线文件（`traits.rs` 1275 / `reuse.rs` 1484 / `mock_driver.rs` 1091 / `types.rs` 991 / `src/types/index.ts` 707） | 已回落，仍需盯 | 持续 |
| G4 | sqlserver 的 `DATABASE_DRIVER_TRAITS_CEILING` 钉子 | 已修 | 已销 |
| G5 | 05 的 `cellTypes.ts` 未落地（算子按 `DataTypeFamily` 7 族兜底） | 05 落地后只改 `src/lib/filterOperators.ts` 一处 | Wave 10 |
| G6 | 命名视图无人认领 | — | **本波轨 B** |
| G7 | `grid.view.*` 两码未登记 | 门禁 `registered ⊆ produced ∪ PENDING` | **本波轨 B** |
| G8 | `grid.selection.*` 三码零生产者（01 实装走的是 `clampCellCoord → null`，从不抛错） | 挂在 `PRODUCER_PENDING` 里三轮 ⇒ 例外清单变垃圾桶 | **待 Wave 8（02 键盘）后裁定**：键盘导航天然会产生「焦点指向的行已被重取数换掉」的场景，那时 `grid.selection.stale` 可能有真实生产者；**若 02 也不产生它**，则三码全部**撤登记**（注册表里没有生产者的码是对产品错误面的谎报），并同步改契约 00 §8.2 的闭集与两处断言 |
| G9 | i18n 缺口：**192 key × 8 语言 = 1536**（zh-CN 已全；缺的是 de/es/fr/ja/ko/pt-BR/ru/zh-TW）。实测 `node scripts/i18n-sync-check.mjs` EXIT=1，摘要「1536 missing key(s), 0 stale」 | **发布硬门禁**（`scripts/i18n-sync-check.mjs` 在 `.github/workflows/release.yml` 的 `pre-release-guards` 里）。缺口**主要不是本次 PRD 造成的**（chat.shareEditor / newConn.clearStoredCredentials / dashboard.scheduleAuthorization 等是既有欠账），但本 PRD 又新增了 `pagination.*` / `defaultSort.*` / `settings.rowCountMode.*` / `filter.view.*` 等 en key ⇒ 缺口还会变大 | **R 阶段前必须清**；且必须在**全部 en key 定稿之后**做，否则要返工两遍 |
| G10 | 真实 DB E2E 未跑 | 发布信心不足 | R 阶段 |
| G11 | `table_target::read` 与 `get_table_data` 的精确计数解析口径不同 | 改动前就有，登记不改 | 登记 |
| G12 | 估算失败补发无法分辨失败语句 | 可观测性弱 | 登记 |
| G13 | `data/tests.rs` 曾 782/800 | 已拆 | 已销 |
| G14 | Web 通路不持久化三个计数设置 | 需 platform-api + backend-client + 桌面映射同改 | 登记 |
| G15 | `SettingsContent.test.tsx` 1354 行既有超限 | 既有债，本波只加 3 行 fixture | 登记 |
| G16 | `scripts/__tests__/probe-exclusion.test.ts` 的实现换了：原来读 tsconfig 的**原始** `exclude` 数组（TS 5 的编译器 API），TS 7 删掉了 `typescript` 的编译器 API ⇒ 改成 `tsc --showConfig` 读**编译器解析后的有效** exclude | **协调者已自证仍会红**：把根 tsconfig 的 `**/__uiBoundaryProbe__*` 改掉 ⇒ 精确报 `expected [...] to include '**/__uiBoundaryProbe__*'`；且断言强度不减（多一条「必须恰好是保留的探针名」） | 已销（自证） |
| G17 | 宿主还剩 6 条 `unused import` 警告（`commands/ai/chat.rs:14`、`commands/ai/util.rs:7`、`commands/ai/tests.rs:10`、`dashboard/execute.rs:9`、`services/migration_endpoint.rs:3`、`commands/connection_import/ipc_tests.rs:3`） | 全在 **main 改过的文件**里，是 main 自己的遗留；动了容易和 main 后续修复打架 | 登记（不修） |
| G18 | `git diff --check` 报 2 行尾随空白（`docs/features/workflow-guide.en.md:4`、`.zh-CN.md:4`） | 是 main 改的行上的 Markdown 硬换行（两空格），**合并基点里本来就有**同样写法；本次解冲突自身干净 | 登记（不修） |
| G19 | `pnpm typecheck` 前置的 `verifyDepsBeforeRun` 会尝试 `pnpm install` 并失败（`workspace hoist directory is not a real directory`，因为 node_modules 是软链） | **只是噪声**，typecheck 本身照常执行并全绿 | 登记（worktree 环境特性） |
| G20 | `packages/driver-api/src/traits.rs` 的 ratchet 钉子 | 合并后 **1257**（原 1265）；钉子**只许缩不许涨** ⇒ 任何要往它加 trait 方法的册（11）**必须同提交腾出行数**并重新实测 | 持续 |
| **G21** | **i18n 门禁的 stale 规则没有「符号豁免」**：判定是「译文与英文逐字节相同」，而**纯符号型 key**（`≈ {count}`、`—`）一旦是新增的，就会被迫翻成本族措辞 | 将来任何引入「纯符号 + 占位符」型 key 的册都会踩到（已实测确认） | **R 阶段**（改 `scripts/i18n-sync-check.mjs` 的判定逻辑 ⇒ 门禁变更，单独做、单独验） |
| **G22** | **i18n 同步的工具链此前每轮重写**（且上一轮的工具把换行的值劈成两半、弄坏过文件） | 三轮各写一遍 = 三次踩同一个坑 | **已解**：工具链入库（`scripts/i18n-sync-*.mjs` + `scripts/lib/i18nSync.mjs` + 自测），Wave 9–11 直接照用 |
| **G23** | **`src/stores/panelStore.ts` 已到 799/800** | 下一波一动就破红线 | **已销**：本轮把「窗格焦点的唯一写者」整块搬到 `src/stores/paneFocus.ts`（纯函数、54 行），`panelStore.ts` 799 → **765**（余量 35），并补 5 例钉住搬出的规则 |

## R 阶段的 E2E 清单（各册都把 E2E 推到了 R 阶段，这里一次列全，别丢）

**新建的 spec 文件（8 个，当前都不存在）**：

| 文件 | 来自哪册 | 要点 |
| --- | --- | --- |
| `e2e/specs/table-count-and-order.ts` | 09 | 精确态 / 估算态（≈ 与 Count 按钮）/ 不排序 / 行数未知 / 复合主键；**无主键表打开不失败是回归断言，不得 skip** |
| `e2e/specs/table-saved-views.ts` | 08 | SV-001 保存并切换 / SV-002 重启后仍在（证明真落盘）/ SV-003 改名删除设默认 / SV-004 导出再导入 / SV-005 跨连接不共享 |
| `e2e/specs/table-keyboard-nav.ts` | 02 | 键盘旅行（真实浏览器里的 `Meta+A`、焦点、虚拟滚动、行号槽按钮交互） |
| `e2e/specs/table-keyset-paging.ts` | 11 | 顺序翻页无重复、降级提示 |
| `e2e/specs/table-clipboard.ts` | 03 | 跨表粘贴、剪贴板失败分支 |
| `e2e/specs/table-editors.ts` | 05 | 各类型编辑器 |
| `e2e/specs/result-grid-readonly.spec.ts` | 10 | 结果网格只读判定 |
| `e2e/specs/journeys/grid-selection-journey.ts` | 01 | 选择 → 键盘扩区 → 复制 → Escape → 暂存改动与回滚（12-testing 的联合旅程骨架） |

**要改的既有 spec**：
- `e2e/specs/table-batch-ops.ts`：`TC-TABLE-013` **今天三条假绿因素叠加**（选择器匹配不到、无处理函数、无断言）⇒ 必须改成真断言；且 02 册的验收关键步骤是「**把 `Cmd+A` 的处理分支临时注释掉再跑一次，必须变红**」——**不做这步等于没验证**。
- `e2e/specs/table-filter.ts`：TF-008 改写为「空条件 Apply 显示显式不完整提示且不发查询」；新增 TF-012 BETWEEN / TF-013 IN token / TF-014 Contains 把 `%` 当字面量 / TF-015 SQLite 不出现 regex。
- `e2e/contract/journeys/run-extended.ts`：加分页 journey（顺序翻页）。
- `e2e/specs/detail-panel.ts` / `e2e/specs/ops-process-server.ts`：**定位表达式迁移**（焦点裁定后行容器 `tabIndex` 变了 ⇒ 旧选择器会「以选择器为 null 的形式失败」，那种失败最容易被误判成「测试过时」而被随手删掉）。

**E2E 为什么整体推到 R 阶段**：它需要**完整 app 构建**（`pnpm tauri:build:webdriver`，禁止裸 `cargo build`）+ **真实库**（`e2e/.env`）。各轨在开发期只交组件级与 store 级用例；`TC-TABLE-013` 这类「假绿」问题**在 R 阶段一次性处理**。

