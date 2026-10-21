# Wave 6 跨轨台账

> 开发期产物，**交付时必须删除**。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `filters-frontend` | `feature/filters-frontend` | `.worktrees/datazen-filters-fe` | 第 3 轮修复中（FF-D1 出厂路径绕开 helper / FF-D3 摘要 / FF-D4 兜底） |
| B `count-backend` | `feature/count-backend` | `.worktrees/datazen-count` | 第 1 轮复测中（`3ed99203`） |

## 合流预演：两处冲突，**都必须按语义解**

`git merge-tree` 预演的结果：

### 冲突 1 · `src/stores/tableDataStore.ts`（**关键**）
- **HEAD（轨 A）**：`filters`（**不剥离**不完整条件）＋一段注释说明「剥离会让查询回答用户没问的问题——结果集比屏幕上的过滤列表**更宽**；宿主以 `grid.filter.incomplete` 拒绝是可见、可操作的错误，而不是静默变宽」。
- **轨 B**：`filters: filters.filter(isCompleteFilter)`。

⇒ **取 HEAD（轨 A）那一版。** 轨 B 那行是它**分叉于 08 的 Step 10 落地之前**的**旧状态**，不是它的意图——它做的是三态计数，与筛选无关。**机械选一边会退回 08 的政策变更。**

### 冲突 2 · `src-tauri/src/services/mod.rs`
- **HEAD（协调者）**：删掉两个未用的再导出（`FilterConnector` / `FilterRejection`）。
- **轨 B**：新增 `pub use datazen_feature_core::services::table_browsing;`，并把那两个名字又写回清单。

⇒ **保留轨 B 的 `table_browsing` 再导出 + 保持那两个未用名��被删**（轨 B 的 `commands/error.rs` 走 `datazen_feature_core::services::FilterRejection` 的**直接路径**，不经过这条门面 ⇒ 确实没人用）。

## 本波的关键产出

### ① 08 的前端半落地：筛选从「10 个算子 + 全局逻辑」到「22 个算子 + 逐条连接符」
- 算子元数据表 `src/lib/filterOperators.ts`（22 个算子 × 值控件/列族/转义语义，全是**数据**）；
- `src/components/filter/` 拆出 8 个文件，`FilterEditor.tsx` 558 → 403；
- **逐条连接符**按 **SQL 优先级**渲染（`a OR b AND c` ⇒ `a OR (b AND c)`）——左到右会**静默改变结果集**；
- 共享层与前端**同口径**：33 条规范用例写成**同一张 JSON 表**供两侧共读，判决 TSV **逐字节相同**。

### ② 09 的后端落地：行数三态（精确 / 估算 / 不支持）+ 真实主键排序
- 四个 trait 方法的默认实现让**不改一行的驱动**行为与今天完全一致（`skip_count_query()==true` ⇒ `Unsupported`，否则 `Exact`）；
- 主键来源从「列上的 `is_primary_key` 标志」改为「`CachedColumns.primary_keys`」（分册 §1.1 的唯一行为变更，树内 15 个驱动实测影响为零，另加 `tracing::debug!` 让不一致的表可被发现）；
- PG 用 `pg_class.reltuples` 估算，**语句里不出现** `current_database()`（PG 的「库」由**连接池**承载，不是语句里的名字）。

### ③ 三条方法论（本波新得）
- **被测的规则 ≠ 出厂的规则**（FF-D1）：`supportedOperatorsForMeta` 编码了「空声明 ⇒ 共享 19」，但生产路径**直接读原始数组**绕开它 ⇒ 同一个输入 helper 给 19、编辑器只给 1 个选项。**这类缺口只有"让出厂路径经过被测路径"才治得住。**
- **同一件事两种判定**（共享层空界）：逗号串形式拒绝空界、数组形式接受 ⇒ 静默错结果集。
- **门禁被"登记数字"磨掉牙齿**：`error-banner-call-site-parity` 改了数字后，必须再验证「加一个未登记调用点 ⇒ 仍会红」。

## 流程事故（轨 A 自报，已销案）

复测代理把 5 个回归锁文件留在**编码代理的工作树**里；编码代理收尾时用了 `git add -A src` ⇒ 那 3 个文件被**连带收进前一个提交**，而它们依赖的 fixture 还在**未跟踪**状态 ⇒ **那个提交在一份干净检出上是红的**。

**已修**：5 个文件拆成独立提交 `2418038a4`（内容一字未改、出处写在提交信息里）；协调者已独立核实**自包含**且在干净检出上 54 例全绿。

⇒ **纪律收紧**：**代理在自己的工作树里收尾时禁止 `git add -A`**（一律 `git add <具体文件>`）。理由：复测代理的验证文件与生产文件**混在同一棵树里**，`-A` 会把它们一起卷进来，而它们可能依赖尚未入库的 fixture ⇒ 产出**在干净检出上红**的提交——这种红**只在别人 checkout 时才暴露**，本地全绿。

## 轨 B 复测查出的三条高危（已下发修复）

| # | 缺陷 | 性质 |
| --- | --- | --- |
| **CNT-D1** | `reuse_driver_forwards_browsing_strategy_methods` 是**空转**的回归锁：用「wrapper == inner」比较，而 inner 是 `MockDriver`、它那三个方法走的**正是契约默认** ⇒ 不转发也相等。4 条转发只钉住 1 条 | **"自反比较"是恒真的**——当 inner 与默认值等价时，wrapper == inner 恒成立。回归锁必须断言**硬编码期望值**或让 inner **逐条覆盖** |
| **CNT-D2** | `table_target::read` 把**负数估算**（PG 对从未 ANALYZE 的表给 `reltuples = -1`）当行数发出去：`Some(-1)`；共享执行器同输入返回 `None` ⇒ 同一 DTO 两种形状 ⇒ 前端渲染 `1--1 / -1`、**Next 永久禁用** | **两条入口必须共用同一个闸门**，不能各写一份解析 |
| **CNT-D3** | 发布耦合的量化：只上后端轨 ⇒ PG 行数从精确变估算，而**无组件读 `totalRowsKind`** ⇒ ①估算值与精确值同形渲染 ②估算不可用时显示「0 行」且只能看第 1 页 | 契约 §1.1 的「唯一行为变更」**不成立**，已补记变更 2/3 |

⇒ **第四条方法论**（本波新得）：**回归锁不能自反比较**。「wrapper == inner」在 inner 恰好用契约默认时**恒真**——它看起来在测转发，其实在测"默认值 == 默认值"。**要么断言硬编码期望值，要么让 inner 逐条覆盖被转发的方法。**

## 跨轨债务

| # | 事项 | 负责 | 触发条件 |
| --- | --- | --- | --- |
| G1 | **09 的后端三态与前端三态渲染必须同批上线**（PG 声明 `Estimated`，而 `≈` 渲染属 Step 6/8/9） | 09 前端半（Wave 7 第一件事） | 两侧在同一集成分支 ⇒ 合流后同时生效；**单侧上线即回归** |
| G2 | **08 的 Step 10 同理**：前端不再剥离不完整条件、宿主必须同时拒绝 | 已同分支 | 同上 |
| G3 | `src/types/index.ts` **844**（基线 825，Wave 6 +19）、`driver-api/traits.rs` **1275**、`reuse.rs` **1484**、`types.rs` **991**、`mock_driver.rs` **1065** —— **都本轮之前就越线** | 下一碰它的轨 | 拆分 `traits.rs` 本身是 09 §Q8 的待裁定项 |
| G4 | **`sqlserver` 的 `file_line_cap` 守卫在 `main` 上就已经是红的**（`traits.rs` 1160 > 上限 1020，main 实测 EXIT=101） | 协调者合流时处理 | 守卫的 doc 说「钉在实测大小、可缩不可长」⇒ 钉子过期了；本批把 traits.rs 推到 1275 让它**更红**。处理：把钉子更新到实测值并说明「拆分 `traits.rs` 本身是 Q8 待裁定项」 |
| G5 | 05 的 `cellTypes.ts` 未落地 ⇒ 算子按 `DataTypeFamily`（7 族）兜底，`enum`/`uuid` 暂与 `text` 同族 | 05 落地 | 05 落地后**只改 `src/lib/filterOperators.ts` 一处**（分派键换成 `resolveCellType` 的 11 值） |
| G6 | 命名视图（08 Steps 12–14）无人认领 | Wave 7 | Rust + UI 同一轨做完（顺序自洽） |
| G7 | `grid.view.*` 两个码**未登记**（登记了会让「每个注册码都要有生产者」门禁红） | Wave 7 命名视图落地后 | 同上 |
| G8 | `grid.selection.*` 三个码**零生产者**（Wave 1 登记）；显式列在 `PRODUCER_PENDING`，一旦有生产者门禁就红 | 需要一次显式决定 | ①让选择层改抛错（行为变更，需独立轨）或 ②撤登记并从契约 §8.2 删行 |
| G9 | i18n：每个非英语 locale 缺 170+ key（16+22 是本批新增，其余既有） | 发布前 | `scripts/i18n-sync-check.mjs` 是发布硬门禁 |
| G10 | 真实数据库 E2E 回归（PG 的外键跳转、行数估算） | R 回归 | 本环境无 DB；FK2-D1 正是 mock 看不见的那类缺陷 |
| G11 | `table_target::read` 与 `QueryExecutor::get_table_data` 对 count 单元格的解析口径不同（前者还收 `Value::String` 并 `parse()`） | 登记，不改 | **改动前就有**、本次逐字保留（§1.1 说只有一次行为变更） |
| G12 | 估算失败后的补发**无法分辨是哪条语句失败** | 接受 | 失败路径多一次往返后可接受 |
| G13 | `src-tauri/src/commands/data/tests.rs` 曾到 782/800 | 轨 B 拆文件 | 后续再加测试前必须拆 |

## Wave 7 计划（Wave 6 合流后）

| 轨 | 内容 | 边界 |
| --- | --- | --- |
| FE | **09 的前端半**（Step 6/8/9：`databaseMeta.countStrategy`、设置项 `rowCountMode` 等三项、`Pagination` 三态渲染与稳定性提示、"精确统计"入口、i18n、`grid.count.estimateUnavailable` 登记）**＋ 02 键盘** | 与 08 的前端半**同族**（`DataTable`/`VirtualBody`/`useGridSelection`）⇒ **必须串行**，不能同波 |
| BE | **08 命名视图**（Steps 12–14：store + 命令 + UI，Rust 与 UI 同一轨） | `src-tauri/src/store/` + `commands/named_views.rs` + `src/components/filter/NamedViewMenu.tsx` |

⚠️ 09 的前端半**必须先落地**（G1）：PG 的估算值在没有 `≈` 渲染时会被当成精确值显示。
