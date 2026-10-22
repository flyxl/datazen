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

**已裁定（用户，2026-10-22）：表级覆盖放独立 store。** 落点 `{appData}/timestamp_overrides.json` + 宿主 `src-tauri/src/store/timestamp_overrides.rs`（与 `named_views.json` 同族：懒加载 + 逐条容错 + 原子写）+ 三条薄命令 + web 端口。理由：连接 profile 保持「连接怎么连」的纯粹性；导出/迁移连接时不带走一堆显示规则；且与「规则=偏好、覆盖=数据」的归属区分一致。**不是**放进连接 profile（`src-tauri/src/store/connections.rs`）。

### 保留现状的一条语义（加时区支持时**不许**改）

`src/lib/formatters.ts` 的 `formatTimestamp` 对**没有时区标识的库文本**（`2026-03-01 00:15:30`）是**原样返回**的 —— 因为 `new Date()` 会把墙钟文本按**本地时区**解释，于是每个单元格被偏移一个本地 UTC 偏移量，还会拼上一个假的 `Z`。⚠️ 这是这个仓库已经踩过的坑，Wave 11 的格式化本体必须沿用它。

### 验收口径（issue 的 8 条 → 本仓形态）

Settings UI（`iso`/`iso-tz`/`locale`）· 规则引擎（表名/列名/列类型三种匹配）· 表级覆盖的持久化 · 时区偏移格式 · Unix 秒 vs 毫秒 · **三级优先级** · **opt-in（默认关闭，既有行为逐字节不变）** · 文档落到 `docs/features/`（允许的文档类型）· E2E 留给 R 阶段。

## 轨 B `result-grid` 的复测裁定（2 条中危）

复测 **TEST_PASSED**，但查出 2 条中危 ⇒ **修复轮中**。

### 中危一 · 放行通道的启用条件缺失/不唯一

并集的放行臂（`src/lib/resultEditability.ts`）只以 `server.writable` 为条件，而**「新管线已接通」在代码里零表示**（`previewResultCellChange` 生产代码零调用）。今天这个洞靠**两个散落条件**关着：① 驱动契约默认实现（契约级、刻意的）② `ContentViewDrawers.tsx` 的 `!!runtimeBinding`（**偶然的**，与判定无关）。

Tester 在**组件层实测复现了后果**：来源判定可写 + 窄判定未验证 + 有 runtimeBinding ⇒ 网格**声称可编辑** → 用户改值回车 → 窄路径拒绝 → **零写语句离开进程**、**`exec.error` 为 null（失败被完全静默）**、单元格**仍显示新值** ⇒ 正是 DB-14 要消灭的「改了、然后悄悄丢掉」；另一半是「无 runtimeBinding 时不可编辑**且 `data-dt-grid-readonly-reason` 缺失**」⇒ 不响应的灰格子（PRD §4.2 明令禁止）。

⇒ **裁定**：加**一个显式的、单一**的启用开关，条件写成「来源判定可写 **且** 新管线已接通」，并补一条把它钉死的测试（今天必须为 false ⇒ 放行臂不可达）。**在第一个驱动覆写落地之前它必须保持关闭。**

### 中危二 · 目标面打偏了（**协调者裁定**）

Tester 查出：`src/windows/connection/result-workspace/ResultTableView.tsx` 的 `canEdit` **一行未改**（仍是 `!safeMode && !!onCellEdit && writableMapping === 'verified' && !!runtimeBinding`），新判定只 governs **行详情抽屉**。

**裁定为真缺陷**：缺陷本体就在网格本体 —— PRD 总纲写的是「查询结果网格 | …双击进编辑但**提交即丢弃** | `ResultTableView`」「问题：`ResultTableView` 的编辑「提交即丢弃」」「查询结果网格编辑即丢弃 | `ResultTableView.tsx` | `handleCellDoubleClick`、只读结果 `DataTable` 装配」。⇒ **10 册 §6 的文件清单漏了 `ResultTableView.tsx`**（契约缺口，不怪按清单做的实现），但不打靶心就没有消灭第三种状态。⇒ 网格本体的 `canEdit` 必须由判定驱动，且只读时**根本不得进入编辑态**（`handleCellDoubleClick` 那条「编辑只关闭编辑器」的旧机制正是「悄悄丢掉」的来源）。

### 只登记、不修

| # | 事项 | 处置 |
| --- | --- | --- |
| 2 | 往命令壳塞一条规则 ⇒ 1439 + 539 条用例**全绿**（Coder 以为写了自检，实测抓不到；`ipc_surface_tests.rs` 里新增的是**注册**自检） | 要么改成真能抓（源码扫描 `src-tauri/src/commands/*.rs` 出现 `grid.` 前缀字面量 ⇒ 红），要么登记为「靠阅读保证的不变量」。**前者优先**（这是用户定则的核心） |
| 3 | `assertResultWritable` 的 throw 被去掉 ⇒ 1508 条前端用例**全绿**；而它的注释自称「绕过它的调用者也不得写穿只读结果」—— 该主张**无证据** | 补一条 store 级用例（便宜、值得） |
| 6 | A-7 的 E2E 未交付且**未在仓库留档**（分支内无 progress.md / hub.md）⇒「留 R 阶段」只在对话里成立 | **协调者做**：R 阶段的 E2E 清单要落进**永久文档**（台账会销毁 ⇒ 只活在对白里不算数） |
| 9 | 窄写路径不查连接的只读标志 | 既有行为，非本轨引入 ⇒ 登记 |
| 10 | `resolveResultEditability` 的 legacy 臂返回 `table: null`（来源判定只读时 `server.table` 实际是 null） | 不健全断言 ⇒ 登记 |
| — | **14 个只读原因码今天只有 2 个可达**（`unknownOrigin` / `sessionReadOnly`；其余需驱动覆写或需可写的后端判定） | 契约的条件交付设计如此，**不是缺陷**。记账：要拿到「为什么」级别的解释，需要 ① 至少一个驱动覆写 `describe_result_origin`（~12 行委派 `describe_result_origin_with_catalog`，helper 已就绪、用真 sqlparser AST）**且** ② 前端接新管线 |

### 值得记下的方法论（Tester 的两个发现）

1. **「并集」不是免费的**：它保住了在用功能，但**造出了一个今天关着、将来一接驱动就打开的洞**。⇒ 「A 或 B」的合成必须配一个**显式的单一启用开关**，否则两个可选路径的并集天然会打开错误的那条。
2. **契约的文件清单会漏掉缺陷本体所在的文件**（10 册漏了 `ResultTableView.tsx`）⇒ 分册的 §6 清单**不能**当作「缺陷在哪」的依据；判据要看 PRD 总纲里「缺陷本体」那一段。

## 轨 A `editors`（05 册）的 8 项裁定

交付 6 提交（HEAD `3243330c5`）。**8 项偏离全部接受**，其中一项是**协调者纠正了自己先前的指示**：

| # | 偏离 | 裁定 |
| --- | --- | --- |
| 1 | `grid.cell.invalidValue` 进了 `PRODUCER_PENDING` | **接受，且纠正我先前的指示**（我先前说「不许进，因为它有真实 Rust 生产者」——**错了**：这个码由**前端**产生，非法值走不到 Rust，后端没有这个拒绝语义）。⇒ **`PRODUCER_PENDING` 的含义是「这个码不由 Rust 生产」，不是「还没人生产」**（守卫的注释本来就这么写）。已同步契约 00 §8.2 第 2 行与表下的通则 |
| 2 | `DataTable` 的 8 个新 prop 合并成一个 `cellEditing: CellEditingContext` | 接受：散 prop + 文档注释在给定预算下必然超（实测 +58/+61 vs 预算 +40/+40） |
| 3 | JSON / long-text 编辑器是格内多行 textarea，不是弹层 | 接受：弹层会把 Apply/Cancel 搬进控件，与「提交时机只有一处（`CellEditorHost`）」直接冲突 |
| 4 | `readOnlyReasonMessageKey` 放在 `cellEditorKind.ts`（不在 `gridErrors.ts`） | 接受：`gridErrors.ts` 是 01 的骨架，本轨只被授权碰 `grid.cell.*` 块；映射仍唯一 |
| 5 | 带时区的 `time`（`timetz`）走文本编辑器 | 接受：选择器装不下偏移量，用它会**悄悄挪动瞬时**（契约 §7 #8） |
| 6 | `money` 归 `text` 而非 `number` | 接受：契约 §3.5 的规则表把 money 列在数值族，但 **§7 #21 明写「money 归 text、不做金额解析」** ⇒ §7 更具体 |
| 7 | `FILTER_OPERATOR_META.columnFamilies` → `cellTypes` | 接受（08 §3.5 的设计形状；全仓无外部消费者） |
| 8 | `CHANGELOG.md` 记三条用户可见行为变化 | 接受（算子集对 `enum`/`uuid` 收窄、JSON 拒绝提交、`point` 改判文本色）；并行轨未改 CHANGELOG ⇒ 无冲突 |

### 两处**环境**问题（不是代码问题，但会让门禁不可比）

1. **驱动集**：两个 Wave 8 工作树是脚本默认的 **basic** 集（10 驱动、无 sqlserver），而集成分支是 **all** 集（21 驱动）⇒ 同一份宿主单测的绿/红**不可比**（12-testing D6）。已把两棵树重新铺成 all，并要求复测在 all 集下重跑。
2. **i18n 追加位冲突**：两轨都往 `src/locales/en/query.ts` **末尾**追加（editors 36 条、result-grid 19 条）⇒ 合流必然文本冲突 ⇒ 手工取并集（纯追加、互不重叠 ⇒ 零语义风险）。

### i18n 第三轮的需求已确定（等 Wave 8 两轨落地后开）

合流后新增的 en key：named-views **22** + editors **36** + result-grid **19** = **77 条 × 9 语种 = 693**。⚠️ 门禁的 stale 规则**没有符号豁免**（实测把 `'≈ {count}'` / `'—'` 原样写进语种会立刻报 stale）⇒ 纯符号型新 key 会被逼着改用本族措辞 —— 这条盲区值得单独立项（与 `i18n-key-collision-check` 合并考虑）。

## 轨 B `result-grid` 修复轮的结果（3 提交 `008f03e9f`）与两个后续

**两条中危都修完，聚焦复验中**（Tester `aa4164d2`，同一实例）。

### 修完的东西

| 项 | 修法 |
| --- | --- |
| 放行闸门 | 单一具名常量 `RESULT_ORIGIN_WRITE_BACK_ENABLED`（今天 `false`）+ 同源入口 `originChannelWritable()`；通道关闭时来源判定**只能拒绝不能放行**（保留后端原因、页级事实不翻案）。注释写死前提：**① 至少一个驱动覆写 `describe_result_origin` ② 前端写走新管线 —— 两者缺一不可**（只落其一就会把「声称可编辑、写入被静默拒绝」放回来） |
| 目标面（网格本体） | `result-workspace/ResultTableView.tsx` 的 `canEdit` 由判定驱动；只读时 `handleCellDoubleClick` **根本不打开编辑器**；只读时 `data-dt-grid-readonly-reason` **存在且非空** |
| 判定键 | 统一为「**会话 + 结果序号**」⇒ 网格与行详情抽屉**共享同一份判定**（此前是两套）；请求里的 `connectionId` 去掉，后端从会话反查连接 |
| 缺陷 2（命令壳纪律） | **变成真能抓的守卫**：`ipc_surface_tests.rs` 新增 `the_command_shell_carries_no_rule_codes` —— 递归扫描 `src-tauri/src/commands/**` 的**生产**源文件（跳过 `*_tests.rs` 与 `tests/` 目录），出现 `"grid.` 字面量即红（报文件名与行号）。**这比「靠阅读保证」值钱得多** —— 它是用户定则「`src-tauri` 只做 command 适配」的第一条可执行守卫 |
| 缺陷 3 | `assertResultWritable` 补 store 级用例（只读⇒抛；可写⇒放行；没人判定过⇒放行；只判被写的那个结果） |
| 额外自查发现并修掉 | **判定可能活得比它的行更久**：结果被替换 + 旧请求在途 ⇒ 旧答案会落进新结果。现在请求带**世代号**，结果替换则世代号前进，在途旧答案被丢弃 |
| 一条**如实报告**的未转红 | 移除网格的**逐行守卫**没有用例转红 —— `canEdit` 与逐行守卫是两道防线、后者被前者遮住。Coder 没有把它说成「抓到了」；已交 Tester 判断（可接受 / 覆盖缺口） |

### 两个后续（已记账）

| # | 事项 | 处置 |
| --- | --- | --- |
| **F1** | **放行通道的两个前提都还没到**：① 至少一个驱动覆写 `describe_result_origin`（helper `describe_result_origin_with_catalog` 已就绪，~12 行委派）② 前端写走新管线（`previewResultCellChange` 的 Rust 侧已就绪，前端未接） | **必须同批落地**。这是一个独立的小轨（可作 Wave 10 的第五轨或单独一波）：驱动侧 ~12 行/驱动 + 前端接线 + 把 `RESULT_ORIGIN_WRITE_BACK_ENABLED` 翻成 `true`（届时开关两侧的用例必须改） |
| **F2** | 行详情抽屉读的是**默认窗格**的 exec（`s.queryExec.get(activePanel.id)`）⇒ 拆分窗格时它与网格显示的结果不是同一个 ⇒ 两边判定键会不同 ⇒ **拆分窗格下的抽屉只读原因会不准**（缺判 ⇒ 退化为 `unknownOrigin`，窄判定仍生效，无回归） | 既有实现的旧账，不在本轮范围。**需要单独一条**（要么让抽屉也按自己窗格读 exec，要么让判定键跨窗格一致） |

## Wave 8 两轨的复测裁定

### 轨 A `editors`：TEST_FAILED（1 条必须合流前修）+ 修复轮

**必必修的 7 条**：① **导航守卫漏 `countExact` 与 `load`** ⇒ 编辑器卸载 ⇒ **用户已键入的草稿静默丢弃**（契约 05 §4.4 明令禁止）；② **ClickHouse `UInt*` 退化成 text**（颜色/编辑器/算子集三项全变）；③ CHANGELOG 只记 3 条、实测 **12 类**类型名变了族；④ **G5 的算子矩阵零回归守卫**（把 `enum` 改回 text 全集 ⇒ 全仓 7440 条用例全绿）；⑤ **驱动 hints 到不了筛选算子分派**（08 §3.5 的「一处声明、两处生效」未兑现）；⑥ `classifyDataType` 不接 hints ⇒ **颜色与编辑器分家**；⑦ 空 `columnEditorInfo` 会让 `data-dt-cell-type` **整个消失**。

**协调者已改契约（三处）**：
- 05 §3.5 **序 19 补 `uint`**（`int` 前缀匹配不到 `uint`；ClickHouse `UInt8…UInt256` / `Nullable(UInt32)`）。
- 05 §3.4 第 2 条：`classifyDataType` 必须接受**同一份**驱动 hints（**可选、向后兼容**）⇒ 兑现 §3.1「一次判断一次投影」。
- `src/lib/gridErrors.ts` 的骨架注释同步新语义（`PRODUCER_PENDING` = 「这个码不由 Rust 生产」）。

### 轨 B `result-grid`：TEST_PASSED，但 ★ 被升级为中危 ⇒ 修复轮

**★ 的裁定：采纳 (A)「让它成真」，并补一条关键约束**。Tester 论证逐行守卫是**可证明永远不会拒绝的死代码**（它的谓词与 `canEdit` 是同一个函数；拒绝分支强制 `writable:false` ⇒ `resolveGridEditability` 的第一个分支立刻返回）⇒ **PRD §1.3 第 6/7 条（被编辑行主键值非 NULL / 行身份在已取到的行内唯一）在生产路径上无人执行**，且本轮把可达性从「来源判定可写时会算」**收窄**为「永不可算」。

**约束**：定位键列**不能只来自来源判定**（那正是它算不出来的原因）⇒ **必须来自宿主已有的目录事实**（`CachedColumns.primary_keys`；宿主壳本来就取了目录缓存的归一化主键列表）。四步：① 宿主把目录主键列传给判定入口 ⇒ **窄通道上也有定位键列**；② 共享层**拒绝时也保留** `identity_columns`；③ 前端把页级事实抽成**与通道无关**的独立函数，先算再 AND；④ 给它一个**今天可构造**的单独触发场景（「窄判定 verified 但被点那行主键为 NULL ⇒ 编辑器不得打开」）⇒ 去掉逐行闸的 mutation 才会转红。
⇒ **不再维持现状**（Tester 原话：「现状是最差的一种：看上去守住了，实际没有」）。

**必修二（低危）**：命令壳守卫只守「拼写形式」——① 块注释里的字面量**误报**；② **拼出**的码绕过它（**而出厂的壳今天就有两处**：`result_origin.rs:151/194` 直接命名 `ResultErrorCode::OriginUnavailable`）；③ 同行含 `//`（URL）会**藏住**字面量。⇒ 加两条规则：禁壳里出现 `ResultErrorCode::`；剥块注释且不被 `//` 截断。补完后它才真正对应「壳里没有规则」这一**不变量**。

**方法论（值得留）**：Tester 把「被另一道闸遮住」从「更细的提示」升级为「**可证明永远不会拒绝**」，靠的是**代码级论证**（同一个谓词 + 拒绝分支强制短路），不是靠感觉。⇒ **一个守卫如果永远不会被单独触发，它就不是守卫。**

## 轨 A `editors` 修复轮（3 提交 `f988fecb3`）+ 追加一条

七条必修全部完成：导航守卫（`countExact`/`load` 加 `mayNavigate()`、**额外护住 `invalidateCachedData`**）、`uint` 规则、CHANGELOG 改**分类迁移清单**（22 个类型名分 7 组）、G5 矩阵守卫（`filterOperators.cellTypeMatrix.test.ts` + 金标准 `filterOperators.cellTypeMigration.test.ts`，后者**断言清单之外没有任何类型名变化**）、驱动 hints 接到筛选器、`classifyDataType` 接可选 `hints`、空 map 降级。

### 追加一条（协调者裁定）：表头标签也必须用同一份 hints

`TableHeader.tsx:76` 渲染列的**原始类型串**并按类型族着色 ⇒ MySQL `tinyint(1)` 的**表头数字色、单元格布尔色**。**要么两处都接、要么都不接**，不许停在「一面接了、另一面没接」。落点比预估省事：`DataTable` 已有 `cellTypeHints` prop 且已在往下传（`:520`）、`TableView.tsx:704` 已在传 ⇒ 只需把同一份 hints 送到 `TableHeader`。**不许为表头新建第二份取值路径**。预算：`DataTable` +27 → **最多 +38**；`TableView`（+11/≤15）、`VirtualBody`（+29/≤40）**不许再挤**（离 800 只剩 29 / 6 行）。

### 一处**用户可见的行为变化**（已认可，记在案）

`invalidateCachedData` 现在**不清**开着编辑器的面板（行晚一次刷新）。取舍理由：**草稿不丢 > 早一次刷新**（契约 05 §4.4 明令禁止静默丢弃用户的键入）。⇒ 若后续有轨依赖「清缓存后立刻重新取数」，由协调者统一处置。

### 两条裁定的落定

- **`grid.cell.invalidValue` 的 `PRODUCER_PENDING` ⇒ (a) 维持现状**（我先前说「不许进例外表」是**错的**：该码由**前端**产生，非法值走不到 Rust，后端没有任何一处会抛它 ⇒ 它**天然**无 Rust 生产者，而守卫要求「要么有 Rust 生产者、要么列进例外表」⇒ 不列则守卫直接红。(b) = 后端编一个它没有的拒绝语义；(c) = 该码形同虚设）。契约 00 §8.2 与 `gridErrors.ts` 骨架注释**协调者已改**。
- **`FILTER_OPERATOR_META.columnFamilies → cellTypes` 无冲突**：08 册早已落地完成（Wave 5/6），不存在「08 册按契约 §3.5 落地时撞这一处」的情形。

## 轨 B `result-grid` 修复轮（2 提交 `73dce2ba3`）：★ 采纳 (A) 的落地

**★ 的四步落地**：① **定位键列改从目录事实来**（窄写路径本来就在活动目录 `CachedColumns.primary_keys` 上复验身份）：共享层 `evaluate_result_editability(origin, primary_keys, result_columns, session_writable)` 多收参数，新增 `locating_key_columns()`（目录主键列表 × 结果列名按同名匹配；别名情形由来源判定补）；宿主壳请求多带 `table`（窄判定认出的真实关系名 —— 驱动说不出来源时它是拿到源表的**唯一**把手）。② 共享层 `read_only(reason, identity_columns)` **拒绝时不再清空** `identity_columns`（**键列是关于表的事实，不是关于判定的**）。③ 前端抽出**与通道无关**的 `resolveRowAddressability(...)`，**先算页级事实、再与结果级判定 AND**；页级事实**即使结果被拒也会被计算** ⇒ 不再结构性不可达。④ 可构造用例落在 `result-workspace/__tests__/resultGridRealTable.test.tsx`：「窄判定 verified + 被点那行主键为 NULL ⇒ 编辑器不打开、原因不吞、`onCellEdit` 0 次」。

**钉死效果（自证）**：去掉逐行闸 ⇒ 转红（`expected [ <input…> ] to have a length of +0 but got 1`）；共享层又把定位键列清空 ⇒ 转红（`left: [] / right: [IdentityColumn { column: "id", … }]`）。⇒ **上一轮「被遮住」的问题消失。**

**命令壳守卫补成真守卫**：两条规则（壳里出现 `grid.` 任何拼写 ⇒ 红；壳里**点名** `ResultErrorCode` ⇒ 红）+ **出厂那两处已改掉**（改走共享层 `read_only_error_code(reason)` 映射）+ 剥注释换成**认识字符串**的扫描器（块注释提到码**不误报**；URL 里的 `//` 不再把行截短、不再把藏在后面的码漏掉）。

### 两条裁定的落定（协调者）

| # | 事项 | 裁定 |
| --- | --- | --- |
| **C1** | **§1.3 第 7 条（行身份在已取到的行内唯一）今天只有前端执行者**，服务端无对应实现 | **接受**。理由：**「已取到的行」只在客户端**；服务端要复验就得把整页行传上去 —— 那是更差的契约。共享层那侧由 `validate_result_cell_request` 兜第 6 条的 NULL 身份值（那条管线仍未接线）。⇒ 登记为「服务端无对应实现，属归属决定而非缺口」 |
| **C2** | **「定位键列未知时 fail open」**（目录读不到 ⇒ 不拒绝） | **接受**。理由：fail closed 的代价是「目录一读不到 ⇒ 整个网格变灰」；而窄写路径**自己对活动目录的复验仍在后面挡着**，所以放行的后果由那道窄校验兜底。⇒ 记在案，若将来要求 fail closed，须同时给出「目录不可读时的降级路径」 |

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

## Wave 8 合流（`23ca53098`）：两处冲突都是**手工按语义解**的

| 冲突 | 解法 |
| --- | --- |
| `src/locales/en/query.ts` | 两轨各在**末尾**追加一段（editors **36** 条 `cellEditor.*`、result-grid **19** 条 `query.gridEdit.*`/`query.gridReadOnly.*`）⇒ **key 集交集为 0**（程序化核对过）⇒ 取**并集 55** 条 |
| `src/windows/connection/ContentViewDrawers.tsx` | 两轨都改了 `<DetailPanel>` 块 ⇒ 取 **result-grid 的结构**（只读原因横幅 + **判定驱动**的 `editable`）+ **补上 editors 的 `cellTypeHints`** ⇒ **两轨的改动都活着**（已交两位 Tester 在**集成结果**上专项复核） |

**合流后门禁（协调者实测）**：vitest **734 files / 7538 tests**；两轨收编的 9 个验收文件 **34 tests**；tsrs 与 tsc 双引擎 0 error；驱动 UI 套件 88 files / 1005 tests；Rust：host 1440 / feature-core 541（+ 集成 4+9+3+1）/ driver-api 417（+ 集成 5+14+1）/ sqlserver 15 套件全 ok（棘轮在内）；`cargo fmt --check` 0；i18n 缺口 **693 = 77×9**（named-views 22 + editors 36 + result-grid 19）。

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
| **8** | **05 编辑器 ∥ 10 结果网格** | ✅ **已落地**（`4010d7ae2` + `23ca53098`） | 2 处冲突**手工按语义解**（见下） |
| 9 | 02 键盘 ∥ 11 键集分页 | ⏳ | 2（`DataTable.tsx`、`TableView.tsx`） |
| 10 | 03 剪贴板 ∥ 12-testing 的 D1–D8 审计 | ⏳ | 0 |
| R | 全量回归 + E2E + i18n 收尾 + 契约改写 + 删 `docs/prd/` | ⏳ | — |

