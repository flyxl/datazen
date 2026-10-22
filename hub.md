# Wave 8 跨轨台账

> 开发期产物，**交付时必须删除**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。
> 本文件只记**跨轨**事实；单轨进度留在各轨自己的 `progress.md`。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `editors` | `feature/editors` | `.worktrees/datazen-editors` | **实现中**（Coder `2e4b6dcf`，05 册：类型化单元格编辑器） |
| B `result-grid` | `feature/result-grid` | `.worktrees/datazen-result-grid` | **实现中**（Coder `86b3084f`，10 册：结果网格可写性判定） |
| C `i18n`（**非 PRD 轨，但属发布门禁**） | `chore/i18n-zh-cn-and-like` | `.worktrees/dz-i18n-2` | **进行中**（Agent `4cf71853`）：补 zh-CN 的 338 个缺失 key + 修 `filter.like` 的 9 语种误译 + 把 zh-CN 加进门禁 |

集成分支 `feature/data-browsing-prd` HEAD = `66fa0b72e`（Wave 7 已收尾：两轨落地、台账已删、已推远端）。

## 门禁基线（Wave 7 合流后，协调者实测）

```
npx vitest run                        → EXIT=0   706 files / 7344 tests
node scripts/typecheck.mjs            → EXIT=0   tsrs 0.18.0-ts7.1.0-dev，三个 program，0 error TS
node scripts/typecheck.mjs --engine=tsc → EXIT=0   tsc 7.0.2（权威），三个 program，0 error TS
node scripts/i18n-sync-check.mjs      → EXIT=1   408 missing = 51×8（count-frontend 29 + named-views 22）
```
Rust（Wave 7 收尾时实测）：宿主 1429 / feature-core 527 / driver-api 396(+6 集成) / sqlserver 230。

**仓库级 CI 守卫（本轮首次全量跑，全部绿）**：

| 守卫 | 结论 |
| --- | --- |
| `pnpm test:ids` | EXIT=0（ID 术语规范） |
| `pnpm test:layers` | EXIT=0（模块分层） |
| `pnpm test:boundaries` | EXIT=0（驱动 import 边界） |
| `pnpm test:driver-types` | EXIT=0（驱动类型隔离） |
| `pnpm test:platform-arch` | EXIT=0 —— PASS，30 个 workspace member 分类、11 条规则、0 violation、3 advisory |
| `pnpm test:shared-crate-panic` | EXIT=0 —— PASS，1 crate / 3 文件 / 0 test-exempt unwrap / 0 violation |
| `pnpm test:platform-arch:mutations` | EXIT=0 —— **8/8 变异都把守卫变红**，且 `git status --porcelain` 在变异后为空（revert 已核实） |

⇒ 意义：本批 PRD 新增的共享层模块（`feature-core/src/services/named_views/`）与共享 crate 的改动（`driver-api/src/filters.rs` 的 serde 属性）**没有越过任何架构边界**。

## Wave 8 的协调者裁定（两册的契约都有一批「行数与路径已过期」的落点）

### 02 键盘 · 契约的现状描述已过期

| 契约写的 | 实测 | 裁定 |
| --- | --- | --- |
| Step 2「容器成为焦点宿主」：给滚动容器加 `data-dt-surface` / `tabIndex` / `aria-*` | **Wave 1 已经做完了**：`DataTable.tsx` 的 `surfaceDomProps` 里已有 `data-dt-surface`、`role="grid"`、`tabIndex: 0`、`aria-rowcount`、`aria-colcount`、`aria-activedescendant`，且已把 `surfaceProps`（含 `onKeyDown`）展开到滚动容器上 | **本册不重建 surface**。本册的工作是**键盘语义**（纯逻辑模块 + 接线 + 行容器退出 Tab 序列）。契约 K-7「不新增 role」也已过期（role 存在） |
| Step 3：行容器加 `data-dt-row-view` | **已存在**（`VirtualBody.tsx:300`） | 剩下的是：行容器 `tabIndex={0}` → `-1`（`:298`）、行级 `data-dt-selected` |
| Step 4：删除行容器上被取代的键盘分支 | **仍在**（`VirtualBody.tsx:229` 的 `handleKeyDown`：Enter/Space → `onRowSelect`） | 照做。契约给的两条删除理由都成立：Enter 的归属已裁定给「进入编辑」；行 `tabIndex={-1}` 后，焦点在行号槽按钮上按 Enter 会**双触发** `onRowSelect` |
| `VirtualBody.tsx` 起点 207 行 | **725 行** | 800 行红线用**实测**值判断，不按契约的数字 |
| `DataTable.tsx` 起点 628 行 | **667 行** | 同上 |
| `TableView.tsx` 803 行（已越线） | **663 行** | 契约说「必须等 01 的抽取先落地」——**那件事 W4 已经做了**（663 < 800） |

另：**E2E（Step 7）留给 R 阶段**（`table-keyboard-nav.ts` 新建 + `TC-TABLE-013` 的假绿改真断言 + `detail-panel` / `ops-process-server` 的定位表达式迁移）——但 `TC-TABLE-013` 那条「**把 `Cmd+A` 的处理分支注释掉再跑一次、必须变红**」是这一册的**验收关键步骤**，必须明确留档。

### 11 键集分页 · 三处路径/范围要订正

| 契约写的 | 实测 | 裁定 |
| --- | --- | --- |
| Step 5 改 `src-tauri/src/services/query_executor/mod.rs` | **该文件已被 main 撤掉**（宿主门面） | 共享层是 `packages/feature-core/src/services/query_executor/mod.rs`（现 431 行）；IPC 侧是 `src-tauri/src/commands/schema.rs`（现 **466** 行） |
| Step 4 把 `build_select_sql` / `build_count_sql` / `format_condition` / `filter_is_complete` / `filter_join` 一起搬进新文件 `table_read_sql.rs` | `format_condition` / `filter_is_complete` / `filter_join` / `join_filters` / `validate_filters` 都在 **`filter_sql/mod.rs`（580 行）**——那是 Wave 5 刚目录化出来的 | **只搬 `build_select_sql`（`query_executor/mod.rs:349`）与 `build_count_sql`（`:316`）** 及其私有助手进 `table_read_sql.rs`；**`filter_sql/` 一行不动**（搬回去等于撤销上一波的工作，且会与 08 的成果冲突） |
| `traits.rs` 1062 / `reuse.rs` 1159 / `types.rs` 958 | **1265 / 1488 / 991** | 见下 |

### `traits.rs` 的 ratchet 是**硬门禁**（不是建议）

`packages/drivers/sqlserver/src/sqlserver/file_line_cap.rs` 把 `packages/driver-api/src/traits.rs` 钉在 **≤1265 行**，且是 **ratchet**（**只许缩不许涨**；加 1 行 ⇒ `cargo test -p datazen-driver-sqlserver` 变红，合并专员已实测过这条会红）。而 11 册 Step 2 要往 `traits.rs` 加两个 trait 方法。

⇒ 裁定：
1. 两个方法的**默认实现体**放进新文件 `packages/driver-api/src/traits/keyset.rs`（照 `traits/browsing.rs` / `traits/filter_contract.rs` 的既有范式），`traits.rs` 只留声明 + `mod keyset;`；
2. **但声明本身也要加行** ⇒ **同一次提交必须把等量的行从 `traits.rs` 搬出去**（文档或整段委派方法搬进既有子模块，**纯位移**，行为不变，既有测试作证）；
3. **自证**：提交后 `cargo test -p datazen-driver-sqlserver` 必须绿；再临时给 `traits.rs` 加 1 行 ⇒ 必须变红。

`reuse.rs`（1488 行，已超限近一倍）：采纳契约 §6.1 第 2 条的**选项 (a)**——把 `impl DatabaseDriver for ReuseDriver` 整体搬到 `packages/driver-api/src/reuse/driver_impl.rs`（`reuse.rs` 变 `reuse/mod.rs`），转发用例跟着走。理由：本册是「新增 trait 方法」的批次，转发块正是相关代码，顺手拆成本最低。

### 两册共用的 `src/locales/en/query.ts`（合流时手工解冲突的位置先定好）

- **02**：8 个快捷键提示 key ⇒ 加在**既有的 `pagination.*` 区块内**（`query.ts` 的 `pagination.*` 现在在 369–403 行）。
- **11**：12 个 `pagination.mode.*` / `pagination.hint.*` key ⇒ 加在**文件末尾**（`} as const;` 之前）。
- ⇒ 两处**编辑区域不同** ⇒ git 可自动合并。

### 其余必须带进 checklist 的

- **三个 `grid.paging.*` 码**（契约 00 §8.2 第 38–40 行）必须有**Rust 生产者**，并**追加**到 `src/lib/gridErrors.ts`（联合类型 + 注册表两处）；**不许**进 `PRODUCER_PENDING`；**不许整文件覆盖**（该文件由 01 建立，各册只追加）。
- **`CursorValue` 必须带标签**（`tag="kind", content="value"`）：`Value` 是 `#[serde(untagged)]`，`[1,2,3]` 在「Rust → 前端 → Rust」两跳里会被读成 `Bytes`，**键集边界悄悄算错**。
- **顺序是硬约束**（契约 §5）：Step 1~2 只加**默认全 false / 全 None**的契约与共享实现，此时**任何驱动行为逐字节不变**；Step 3~5 在宿主内建能力但**默认不开启**；Step 6~7 才让具体驱动与前端开启。
- **`tableDataStore.ts` 现 660 行**（契约写 729、说「距 800 只剩 1 行」）：本册净增若超 **40 行**，就必须在**同一次提交**里拆出 `src/stores/tableData/actions.ts`，不许留给下一册。
- **`ReuseDriver` 转发的回归锁**必须是**硬编码期望值**或让 inner **逐条覆盖**——**不能**用「wrapper == inner」的自反比较（Wave 6 踩过：恒真，只钉住 1 条）。
- **`sqlserver` 的谓词用例必须断言 SQL 里不出现行值比较**（`((a, b) > …)` 这种形状）——契约 Step 7 明写。
- **12-testing 的 D1–D8 是完成定义**，两条最容易被忽略的：**D5 至少一次反向注入**（亲手改坏 ⇒ 亲眼看到转红 ⇒ 还原）、**D7 零文案断言**（不许把 `t()` 渲染的文案钉进断言或定位器）。

## 剩余四册的冲突矩阵（实测，不是估计）

把 02 / 03 / 05 / 10 / 11 五册 §6 的文件清单求交：

| 组合 | 共享文件数 | 共享的是 |
| --- | --- | --- |
| **02 ∥ 10** | **1** | 只有 `en/query.ts` |
| **05 ∥ 10** | **2** | `gridErrors.ts` + `en/query.ts`（**都是纯追加**） |
| 02 ∥ 11 | 3 | `DataTable.tsx`、`TableView.tsx`、`en/query.ts` |
| 03 ∥ 10 | 3 | `gridErrors.ts`(+测试)、`en/query.ts` |
| 02 ∥ 03 | 4 | + `VirtualBody.tsx` |
| 10 ∥ 11 | 5 | **`driver-api/traits.rs`**、`driver-api/lib.rs`、`feature-core/services/mod.rs` + 2 追加文件 |
| 03 ∥ 11 | 6 | + `tableDataStore.ts`、`tableData/types.ts` |
| 05 ∥ 11 | 6 | 同上 |
| 02 ∥ 05 | 5 | + `EditableCell.tsx` |
| **03 ∥ 05** | **9** | `VirtualBody` / `DataTable` / `TableView` / `gridErrors` / `tableChanges` / `dataTableContextMenu` / `tableDataStore` / `tableData/types` / `en/query.ts` |

**两条不可协商 / 可协商的界线**：
1. **10 ∥ 11 不能同波**：两者都要往 `packages/driver-api/src/traits.rs` 加 trait 方法，而它被 sqlserver 的 ratchet 钉在 ≤1265（只许缩不许涨）；两者还要改同一批驱动 crate ⇒ 同波 = 钉子值二义 + 三方合并。
2. **`en/query.ts`（5 册都碰）与 `gridErrors.ts`（4 册都碰）可以变成零冲突**：两者本质是**纯追加**（各册追加自己域的码/词条）。**给每册预留一个插入位**（不是都往末尾追加），git 就能自动合并 ⇒ 这两个文件从「必然冲突」变成「零冲突」。剩余的组件级冲突（`DataTable.tsx` / `TableView.tsx` / `VirtualBody.tsx` / `tableDataStore.ts`）必须手工解 —— 而这些正是历史上出过事故的地方（Wave 6 的 `tableDataStore.ts` 差点丢掉轨 B 的 `sorts`；`probe-exclusion` 的断言被改弱）。⇒ 并行度控制在「同一文件最多 3 册碰」以内。

**重排后的总冲突**：原方案（02∥11 / 03∥10 / 05）= 6；重排后（05∥10 / 02∥11 / 03）= **2**（且全程无 ratchet 争用）。

## D1 的守护点在**三处**（不是收编的那四个文件）

复测代理实测发现：收编的 4 个文件**抓不到** Rust 侧的消息回显（`namedViews.contract.test.ts` 的 host 消息是它**自造的 fixture**，改 Rust 不会让它红）。⇒「宿主消息不回显用户文本」这条规则的守护点：

| # | 位置 | 条数 |
| --- | --- | --- |
| 1 | `packages/feature-core/src/services/named_views/tests.rs` | 2（`a_duplicate_name_message_never_quotes_the_name`、`an_invalid_filter_logic_message_never_quotes_the_value`） |
| 2 | `src-tauri/src/store/tests/named_views_tests.rs` | 1（`the_refusal_message_never_quotes_the_view_name`） |
| 3 | `src-tauri/src/commands/named_views_error_tests.rs`（复测代理产出，已收录 `05dcc13d1`） | 4 条 D1 断言（IPC 层） |

⇒ 记在这里是为了**别让人误以为收编的 4 个文件覆盖了它**。

## Wave 11 · 时间戳格式化（GitHub issue #32）· **4 轨真并行**

**前置已落地**：契约提交 `82f5e4185`（`src/types/timestamp_format.ts` + `src/lib/timestampFormatEngine.ts` 默认实现 + `src/components/DataTable/TimestampFormatContext.tsx` 壳）。**双引擎 typecheck EXIT=0。**

### 四轨与各自拥有的文件

| 轨 | 分支 | 拥有（互不重叠） | 内容 |
| --- | --- | --- | --- |
| **A `timestamp-format`** | `feature/timestamp-format` | `src/lib/timestampFormat.ts`(新)、`src/lib/timestampFormatEngine.ts`(改内部)、`src/components/DataTable/CellRenderer.tsx`(改)、`src/lib/__tests__/` | 规则引擎 + **三级优先级**（表级覆盖 > 规则 > 全局）+ 格式化本体（`iso` / `iso-tz` / `locale`）+ 引擎接进渲染路径 + **ReDoS 防护** |
| **B `timestamp-settings`** | `feature/timestamp-settings` | **`packages/platform-api/src/settings.rs`(改，Rust 权威)**、**`packages/backend-client/src/settings.ts`(改，TS 镜像)**、`src/windows/settings/TimestampFormattingSection.tsx`(新)、`SettingsContent.tsx`(改)、`src/stores/settingsStore.ts`(改)、i18n | 设置 UI：全局开关 + 格式选择 + **规则编辑器** + 预览窗格。⚠️ **两侧必须同批**（见下） |
| **C `timestamp-overrides`** | `feature/timestamp-overrides` | `src-tauri/src/store/timestamp_overrides.rs`(新)、`store/{mod,models}.rs`(改)、`commands/timestamp_overrides.rs`(新)、`commands/mod.rs`、`bootstrap/run.rs`、`packages/backend-client/src/{types/tableData,transport,tableDataClient}.ts`、`src/commands/`(新)、`src/platform/tauriBackendTransport.ts` | 表级覆盖的**存储**（宿主 store）+ 三条命令 + web 通路四元组 |
| **D `timestamp-column`** | `feature/timestamp-column` | `src/components/DataTable/useDataTableContextMenu.ts`(改)、`src/lib/dataTableContextMenu.ts`(改)、`src/windows/connection/TableView.tsx`(改)、列头菜单的格式化面板(新)、i18n | 列头右键「格式配置」+ 每列覆盖 UI + 在 `TableView` 挂 Provider |

**冲突面：只有 `src/locales/en/query.ts`（B 与 D 各加词条）** ⇒ 预留插入位即可自动合并。

### 为什么能比 Wave 8 更并行（三条机制）

1. **共享的只有契约，而契约已提前提交** ⇒ 四条轨**编译期无依赖**（Wave 8 的两轨是各自独立，不需要这一步；这里是四轨共用一个「引擎」概念，不预置契约就必然互等）。
2. **引擎经上下文到达渲染侧** ⇒ 不必改 `DataTable.tsx` / `VirtualBody.tsx` / `GridCell.tsx` —— **那三个同时是 05 册的落点**，多占一个就多一份冲突面。
3. **配置与覆盖的数据形状由契约固定** ⇒ C 轨的端口 DTO 与 D 轨用的类型是**同一形状**。⚠️ **必须加一条逐字段的编译期断言**（照命名视图轨 `FilterConditionConnectorMatches` 的做法）—— 否则就是「同一个 DTO 两种形状」那个老坑。

### 与 issue 方案的 5 处偏离（已裁定）

| # | issue 写的 | 改成 | 理由 |
| --- | --- | --- | --- |
| 1 | 类型加进 `src/types/index.ts` | 新文件 `src/types/timestamp_format.ts` | `index.ts` 现 707 行，本批刚因为 800 行红线搬走过 AI 类型；settings 类型另有 `src/types/settings.ts` |
| 2 | 规则用 `RegExp` | **模式字符串** + 运行时编译 + 防 ReDoS | `RegExp` 不可序列化（存不进 settings JSON），且用户要能编辑它 |
| 3 | 表级覆盖「Persist in connection metadata」 | **独立 store**（宿主）+ 薄命令 + 端口 | 连接档案内会长出一个「每表每列显示规则」的杂物袋；规则本身是 per-table 的，`connectionId` 只是索引 |
| 4 | `custom` 格式 | **Wave 11 不做** | issue 的示例是 **Java 风格** `yyyy-MM-dd`，JS 生态是 `YYYY-MM-DD` —— 小写 `yyyy` 在 JS 里是「年内的第几天」。语义不定死就实现＝埋雷。issue 自己也标了 future enhancement |
| 5 | BIGINT 自动识别 | **默认关闭** + 命中时**显式显示解释**（Unix 秒/毫秒）+ 允许逐列覆盖 | 一个叫 `at` 的 BIGINT 列可能是计数。issue 的三条要求全部保留 |

### 设置项的落点：**跨形态共享的类型链，两侧必须同批**

「全局设置 + 规则」落在 `{appData}/settings.json` ✓（`src-tauri/src/store/mod.rs:293`，**全局**，不分连接）。但**不是**加一个 TS 字段就完事 —— 设置类型是**跨形态共享**的：

```
packages/platform-api/src/settings.rs        ← Rust 权威（UserPreferences，带 #[serde(deny_unknown_fields)]）
      ↓
packages/backend-client/src/settings.ts      ← TS 镜像 + toSharedSettings() 转换
      ↓
src/types/index.ts 的 AppSettings           ← extends UserPreferences, ExecutionSettings
      ↓
src/stores/settingsStore.ts → 设置 UI
```

⚠️ **`deny_unknown_fields` 是硬耦合**：Rust 侧不加字段、TS 侧就发 ⇒ **反序列化直接失败**（不是「那个字段被忽略」，是整个 `settings.json` 读不出来）。⇒ **轨 B 必须 Rust 与 TS 两侧同批改**，不能只改 TS。

### 归属判据：跟着**谁**走

| 层 | 归属 | 落点 |
| --- | --- | --- |
| 全局设置 + 规则引擎 | **用户偏好** —— 跟着用户走 | `settings.json` |
| 表级 / 列级覆盖 | **连接/表的数据** —— 跟着某个连接走 | 独立 store（`{appData}/<name>.json` + 宿主 store + 薄命令 + web 端口） |

判据不是「它算不算设置」，而是「**它跟着谁走**」。这正是 issue 三层里前两层与第三层的分界。

**待协调者/用户拍板的取舍**：表级覆盖放独立 store（本方案，与命名视图一致；连接 profile 保持「连接怎么连」的纯粹性，导出连接时不带走一堆显示规则）**vs** 放进**连接 profile**（`src-tauri/src/store/connections.rs`；覆盖跟着连接一起导出/迁移）。

### 保留现状的一条语义（加时区支持时**不许**改）

`src/lib/formatters.ts` 的 `formatTimestamp` 对**没有时区标识的库文本**（`2026-03-01 00:15:30`）是**原样返回**的 —— 因为 `new Date()` 会把墙钟文本按**本地时区**解释，于是每个单元格被偏移一个本地 UTC 偏移量，还会拼上一个假的 `Z`。⚠️ 这是这个仓库已经踩过的坑，Wave 11 的格式化本体必须沿用它。

### 验收口径（issue 的 8 条 → 本仓形态）

Settings UI（`iso`/`iso-tz`/`locale`）· 规则引擎（表名/列名/列类型三种匹配）· 表级覆盖的持久化 · 时区偏移格式 · Unix 秒 vs 毫秒 · **三级优先级** · **opt-in（默认关闭，既有行为逐字节不变）** · 文档落到 `docs/features/`（允许的文档类型）· E2E 留给 R 阶段。

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
| G20 | `packages/driver-api/src/traits.rs` 合并后 **1265** 行（HEAD 1275 / main 1150） | 钉子已重测并自证会红；`traits/**` 下所有文件 ≤ 800（最大 693），800 行上限守卫仍绿 | 已销（自证） |

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

## 波次总计划（10 波 + R）

| 波 | 内容 | 状态 | 组件级冲突 |
| --- | --- | --- | --- |
| 1 | `grid-selection` + `insert-backend` | ✅ | — |
| 2 | `grid-cell` + `web-transport` | ✅ | — |
| 3 | `insert-frontend` | ✅ | — |
| 4 | `dirty-cells` + `fk-navigation` | ✅ | — |
| 5 | `fk-navigation-ui` + `filters` | ✅ | — |
| 6 | `filters-frontend` + `count-backend` | ✅ | `tableDataStore.ts` |
| 7 | `count-frontend` + `named-views` | ✅ | `en/query.ts`、`gridErrors.ts`、`tauriBackendTransport.ts` |
| **8** | **05 编辑器 ∥ 10 结果网格** | 🔄 进行中 | 0（两个都是纯追加，预留插入位后可自动合并） |
| 9 | 02 键盘 ∥ 11 键集分页 | ⏳ | 2（`DataTable.tsx`、`TableView.tsx`） |
| 10 | 03 剪贴板 ∥ 12-testing 的 D1–D8 审计 | ⏳ | 0 |
| R | 全量回归 + E2E + i18n 收尾 + 契约改写 + 删 `docs/prd/` | ⏳ | — |

