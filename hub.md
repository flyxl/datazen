# Wave 2 跨轨台账

> 本文件是 **Wave 2 开发期**的协调者台账（`hub.md`）。随分支流转，**交付时必须删除**——结论要落成代码、测试与 `docs/` 正式文档。
> Wave 1 的台账已在 Wave 1 交付时销毁（`53d2b9af7`）。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `grid-cell` | `feature/grid-cell` | `.worktrees/datazen-grid-cell` | 第 1 轮复测 **TEST_FAILED**（GC-D1 阻断 + GC-D4 预算），第 2 轮修复中 |
| B `web-transport` | `feature/web-transport` | `.worktrees/datazen-web-transport` | 第 2 轮修复已交，复测中（含竞态修复验收） |

## Wave 2 的协调者裁定（已下达）

### 轨 A（grid-cell）

| ID | 裁定 |
| --- | --- |
| GC-D1 | `__tableDataStore` 注册到 `src/main.tsx`，**门控 `import.meta.env.VITE_E2E`**——不照抄 `__schemaStore` 的无条件写法，避免给生产构建新增无条件全局句柄；E2E 本来就依赖 `VITE_E2E`（`tid()`），故零风险 |
| GC-D2 | `role="grid"` 的子节点只剩 row/rowgroup；`role="status"` 的 live 播报语义**必须保留** |
| GC-D3 | fallback（消费方无 `onRowSelect` 时自行发 row 模式选择）**收窄到行号槽路径**——否则将来会清空「传了受控 `selectedRows` 但没传 `onRowSelect`」消费方的行集 |
| GC-D4 | **本轮做掉**菜单抽取（01 §Step 9）——不是洁癖：02/03/05/06 四册都要往 `DataTable.tsx` 加东西，734 + 4×60 ≈ 974 **必破 800**；且 Tester 刚建好的 763 行测试此时是唯一的安全网 |
| GC-D5 | `capturePointer` 的裸 `catch { }` 收窄到 `NotFoundError`；其余 `console.warn` + 继续（**不 rethrow**，rethrow 会把降级变成全失效） |

### 轨 B（web-transport）

| ID | 裁定 |
| --- | --- |
| 9-1 | MethodMap **保留全部 9 条**（契约完整性）——删掉会制造「07/08/09 落地时忘了加回来」的坑，正是本项目反复踩的病。但桌面 adapter 必须把 Rust 未注册的 5 条声明为 `unsupportedMethods`，让调用得到明确错误而非裸的 `command not found` |
| 9-1 补充 | 缺口分两类：`capability`（形态永远做不到，如浏览器形态的命名视图无宿主持久化）与 `notImplemented`（后端能做还没做，如桌面 07/08/09）。措辞按类分，种类机器可读（`ApiError.unsupportedKind`，**不进 `toPayload()`**，§6.3 线上四字段形状不变） |
| 9-2 | `clonePendingRowChange` 必须在标志**存在时**复制（`...(x === undefined ? {} : { x })`）——写 `insertMarked: change.insertMarked` 会把 `undefined` 塞进 JSON，改变线上形状，并破坏「insert-free 指纹逐字节一致」 |
| 9-3 | **不改** `configRevision` 的 `Counter` 类型——`types/jobs.ts:47` 同一个字段名已经这么声明，`types/identity.ts` 写明「线上十进制字符串 + `toCounter` 收窄」是本包的既定分工。改为登记**包级待核项** |
| 9-4 | `totalRows` 两侧都改 `number \| null`——Rust `Option<u64>` 意味着**键一定在、值可能为 null**，`?: number` 说的是"键可能缺"，是对线上的另一种（错的）断言 |

## 已完成的协调者动作（不在任何轨内）

- `e770f9135` 订正契约与 01 分册的 6 处冲突（`data-dt-selection-*` 语义、`data-dt-surface` 落点、`data-dt-row` 落点、`data-dt-editing` 归属、Delete 键语义、行号槽点击口径、TableHeader 例外）
- `6551dc018` 修正平台架构文档的落地形态与「编译期双向对齐断言」这一**不实表述**（`BackendTransport.call<K extends keyof MethodMap>` 是单向约束）
- `00cebfa94` 给 `TEST-INFRA-001` 补机制定性与 9 条修复验收标准
- `241e93515` 补该缺陷的新观测（受害面更广、`scripts/` 内部即可复现、还会污染 typecheck）

## 协调方法的两条实测教训

**① 给代理的指令若含互相矛盾的子判据，代理会用可测量的方式绕开——应当让它带证据来谈偏离。**
GC-D1 我给了两个子判据：「产物里 grep ≥ 1（证明接线存在）」+「生产构建不得暴露可用句柄」。代理先按我的原文实现，**实测发现单分支会被 tree-shaking 整段折掉、名字都不在产物里**，于是改成双分支并**带着实测数字**来谈偏离。Tester 复核后接受，但补了一条纠正：单分支**也能**满足第二个约束，双分支的唯一增益是**可观测性**（生产产物保留句柄名 + 显式 `undefined` 可断言），不是功能增益。
⇒ 教训：**矛盾在指令里，不在代理**。以后遇到"判据冲突"要么自己先想清楚，要么在指令里写明"若两条冲突，以可观测性为准并带数字来谈"。

**② "没有新缺陷"要区分"没有缺陷"和"没有测到"。**
轨 A 第 1 轮 Tester 报"0 条本轨缺陷"，第 2 轮却发现 2 条——其中 GC2-D1（本轨引入的 dev 期回归）**恰恰是它上轮那条新测试指不到的地方**：那条测试是**源码正则**接线测试，只能盯 `main.tsx` 的形状，盯不住运行期后果（入口模块的 else 分支把 store 文件 DEV 块刚装好的句柄覆盖成 undefined）。
⇒ 教训：**接线测试 ≠ 行为测试**。凡是"入口模块/注册点"这类接线，回归锁必须是**运行期**的（导入后读句柄），不能是读源码。

## 跨轨债务台账（Wave 3 前必须处理）

| # | 事项 | 谁负责 | 触发条件 |
| --- | --- | --- | --- |
| D1 | `src/types/index.ts` **838 行（既有超标）**，轨 B 净增 +1 → 839。**裁定：接受记账，不在功能轨里拆** | **Wave 3 `insert-frontend` 第一件事**（与 D11 的 `panelStore` 拆分一起做，同属 file-first 迁移） | 理由：838 是既有超标、非本轨引入；拆分有"index 与领域文件各声明一份、应用导入 index 副本"的**静默漂移**风险（搬走声明就换了应用实际导入的那一份）；在功能轨里做风险高于收益。**硬约束：本轨不得再增长（保持 ≤839）** |
| D2 | 07 的 `embedded_schema_of` 提升（宿主 `commands/schema.rs:13` 私有 fn） | 07 轨开工第一步 | 07 开工时。提升后**严禁两边各留一份实现** |
| D3 | 桌面 `unsupportedMethods` 里那 5 条 | 07 / 08 / 09 各自落地 Rust 命令时 | 那条"读 run.rs 比对是否注册/是否声明"的交叉核对测试**会红**，要求同步删声明——这是设计好的提醒 |
| D4 | `TEST-INFRA-001` 的**残留 → 轨 B 第 3 轮修**：`tsc` 读侧完全没被锁覆盖，**两个**失败模式——TS6053（并发 typecheck **15%**）+ 「故意违规的夹具被类型检查」（约 **2–3%**，合计 **17–18%**） | 轨 B 第 3 轮（已下发） | 根 `tsconfig.json` + `packages/ui/tsconfig.json` + `packages/backend-client/tsconfig.json` **各加** `exclude: ["**/__boundaryProbe__*", "**/__uiBoundaryProbe__*"]`。**`tsc` 是独立进程，拿不到 vitest 进程内的 `mkdir` 锁**；让它持锁会把 typecheck 与 88s 的 vitest 串行化，且防不住「探针被类型检查」 |
| D9 | `packages/backend-client/src/types/tableData.ts` 的 `configRevision` 注释断言「adapter 会用 `toCounter` 收窄、如同 `ProfileView.configRevision`」——**两半都不成立**（没有任何 adapter 收窄 `ProfileView.configRevision`；桌面 adapter 是裸 cast） | 轨 B 第 3 轮（已下发） | 改成陈述事实：线上是十进制字符串、本包不收窄、收窄责任在 adapter（当前桌面 adapter 未收窄，是已知缺口） |
| D10 | 桌面 `unsupportedMethods` 的交叉核对测试用**裸字符串包含**判定注册（`commands::X,`），将来换拼写就看不见——而它的全部职责就是发现这种注册 | 轨 B 第 3 轮（已下发） | 改用**解析 `invoke_handler` 列表**里的 `crate::commands::X` 条目 |
| D11 | `src/stores/panelStore.ts` 已 **797/800**（余量 3 行），轨 B 净增 +4 | **下一个真正碰到 `panelStore.ts` 的轨**（**修正**：原写"Wave 3 insert-frontend"，但 04 分册 Step 7/8 **不碰** panelStore；真正会碰它的是 10（结果网格写回）等册） | 拆分该 store。**碰到它的轨不得再增长（保持 ≤797）** |
| D12 | `TEST-INFRA-001` 修好后**必须从 `docs/bugs/` 删除**（含 `docs/README.md` 索引行） | 协调者（合流收尾时） | `docs/bugs/` 只收"已核实复现、**本轮明确不修**"的缺陷——修好了就必须删，否则是过期文档 |
| D14 | `aria-rowcount` 的**取值语义契约里没写**（轨 A 改成了「数据行 + 表头行 + 状态行」，状态行是瞬态的） | 01 / 05 定 ARIA 契约时 | 不是缺陷（§4.4 只要求"挂这个属性"）；但取值口径必须写进契约，否则下一册会各自发明算法 |
| D15 | `parseRegisteredCommands` 的**去注释顺序**有边界：块注释剥离先于行注释，若 `//` 注释里含 `/*` 且其后任意位置有 `*/`，会吃掉中间的真实条目（Tester 已构造复现）。**当前不是 live**（run.rs 块内无 `/*`），但这是"靠文件当前内容侥幸"而非机制保证 | 下一个碰 `src/platform/__tests__/tauriBackendTransport.tableData.test.ts` 的轨 | 修法：行注释剥离放到块注释之前，或用一次词法扫描同时处理（成本 1 行） |
| D16 | 解析器把 `#[cfg(feature = "…")]` 门控的条目**视为已注册**（run.rs 现有 6 个这样的条目，全是 webdriver/测试专用；9 条表数据命令当前无一在 cfg 之下）。这是**未言明的语义选择** | 同上 | 端口应当覆盖产品全集 ⇒ 视为已注册**大概是对的**，但必须在注释里点明，否则 07/08/09 把命令注册在 feature 之下时会让人困惑 |
| D17 | 新门禁 `scripts/__tests__/probe-exclusion.test.ts` **硬编码 3 个 tsconfig 路径**，而不是从"所有 tsconfig"推导 | 下一个新增 tsconfig 的轨 | 将来新增一个能看见探针的 tsconfig，门禁**不会发现**。当前其余 program（`tsconfig.scripts.json` / `pack-ep` / `e2e`）都不覆盖探针根，所以当前是闭合的 |
| D13 | 探针路径的 `exclude` 目前只在 3 个 tsconfig 里；若将来新增带 `include` 的 tsconfig，**必须同步加 exclude** | 新增 tsconfig 的那个轨 | 门禁 `scripts/__tests__/probe-exclusion.test.ts` 断言的是"当前这三个都含 exclude"，**不覆盖将来新增的配置**——新增 tsconfig 时要主动加 |
| D5 | 10 分册的 `src-tauri/src/commands/table_target.rs:55`「Manual table session target changed」与 `stalePlan` 同族 | 10 轨 | 10 落地时统一到 `grid.commit.stalePlan` |
| D6 | `grid.insert.noWritableColumn`（04/06 域唯一零生产者码） | 06 或 04 后续轮 | 需 `ColumnSchema` 有 generated/identity 标记才能做准；保守版（全列 Unset 且无 `default_value`/`is_auto_increment` 可依赖 → 拒绝）可先做 |
| D7 | `backend-client` 的**解码边界**是否真的执行 `toCounter` 归一化 | 包级待核项 | 若某 transport 把原始字符串直接交给 app 层，`Counter` 这个 branded number 就是在说谎。全包性质，要改必须连 `types/jobs.ts` 一起改 |
| D8 | `DataTable.tsx` 预算 | 轨 A 本轮抽取后应 ≤600 | 02/03/05/06 开工前确认余量 |

## Wave 3 计划（待 Wave 2 合流后确认）

| 轨 | 内容 | 热文件约束 |
| --- | --- | --- |
| `insert-frontend` | 草稿行 UI + TS 镜像接线 | 碰 `TableView.tsx` → 与 `grid-cell` 冲突，**排 Wave 3** |

**交接给 Wave 3 的硬信息**：

1. `insertMarked` 的克隆已修（轨 B），但**前端仍无 `insertMarked` 的生产者**——Wave 3 建草稿行时**必须**产出它，否则后端的插入路径走不到；
2. `deleteRows` **不写行集**；删除后要保留选中视觉必须走 `setRowModeSelection`，不能自己拼 patch；
3. 本批 RETURNING **不做回填**：定位不到新行 = 刷新 + `grid.insert.returningUnavailable` warning，界面**不得**声称能精确定位新行；
4. 45 码里只有 **8 个**有生产者（27 处 throw 点），前端只应按这 8 个分支；`grid.selection.*` 三个码**零生产者**（越界一律 `clampCellCoord → null`）；
5. 前端错误码注册表 `src/lib/gridErrors.ts` 现登记 11 个码——**新增码必须同时登记**，否则 `classifyGridError` 返回 `unknown`。
