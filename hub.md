# Wave 7 跨轨台账

> 开发期产物，**交付时必须删除**（AGENTS.md「进度台账：开发期间允许，交付即销毁」）。
> 本文件只记**跨轨**事实；单轨进度留在各轨自己的 `progress.md`。

## 轨道状态

| 轨 | 分支 | worktree | 状态 |
| --- | --- | --- | --- |
| A `count-frontend` | `feature/count-frontend` | `.worktrees/datazen-count-fe` | ✅ **已落地**（合并提交 `b7b530e86`）：复测 TEST_PASSED、零缺陷；Coder 收尾轮 `bcc8b9bee`（陈旧注释 / mock 码 / 收编复测代理的第二轮锁） |
| B `named-views` | `feature/named-views` | `.worktrees/datazen-named-views` | 复测 **TEST_PASSED**（Tester `eb6a116d` 独立复核 11 个验收点 + 7 处变异自证），但查出 **D1**（用户自由文本进宿主消息）⇒ **修复轮中**（Coder `29f87132`），修完由原 Tester 复验 |
| C `i18n-sync-existing`（**非 PRD 轨，但属发布门禁**） | `chore/i18n-sync-existing` | `.worktrees/dz-i18n-sync` | **翻译中**（Agent `76f074fd`）：补齐 **192 key × 8 语种 = 1536** 的既有缺口，让 `i18n-sync-check.mjs` 变绿 |

集成分支 `feature/data-browsing-prd` HEAD = `00e373c4b`。

## 本波中途发生的一件事：并入 origin/main（168 提交）

`origin/main` = `77e6b8652`（PR #82 `ts7-upgrade` + 「存储层端口化」大重构）。合并提交 **`46b85adeb`**，8 个冲突按语义解完（不是机械选边）：

| 冲突 | 解法 | 关键理由 |
| --- | --- | --- |
| `src-tauri/src/log_redact.rs`（删改） | 接受删除，2 处调用点改指 `datazen_platform_infra::redaction::*` | main 删了薄壳；我们的 `redact_paths_for_ipc` 在 platform-infra 里，main 没动 |
| `src-tauri/src/services/mod.rs` | 并集 + 按 main 的新家重指 | `schema_scope` 搬到 feature-core；宿主 query-executor 门面被撤 ⇒ 再导出改指 feature-core |
| `file_line_cap.rs` 的钉子 | **重新实测 = 1265**（HEAD 1275 / main 1150 **两个都过期**） | 自证：`traits.rs` 加 1 行 ⇒ 守卫红（`1266 lines, over the 1265-line limit`），复原 ⇒ 绿 |
| `commands/error.rs` | 并集 | `DriverError`(main) + `FilterRejection`(我们)；脱敏 = 先凭据后路径 |
| `data_browsing/write_exec.rs` | **取共享层版本**（丢弃 main 的宿主本地 `data_session.rs`） | 实为 git 的 rename/rename(1to2) 拆分；按已裁定契约 §9.2「执行在共享层、宿主只留适配层」。**resolver 核对 diff：main 版是我们版的子集**（我们版多了 INSERT、config_revision、stale-plan 守卫、runtime 注入） |
| `commands/data.rs` | 适配层版本 + 叠加 main 的 `crate::db::*`→`datazen_driver_api::*` 迁移 | main 对该文件的唯一改动是机械迁移 |
| `commands/schema.rs` / `commands/table_target.rs` | 并集 | 同上的 driver_api 迁移 + 我们的主键来源/默认排序块叠在 main 的 dialect 构造之上 |

**合并后的门禁**（resolver 实测 + 协调者复核）：host 1413 / feature-core 507 / driver-api 396 / sqlserver 230，vitest 694 files / 7208 tests，typecheck 0 error TS，`cargo fmt --check` 0。

**测试计数下降不是丢测试**（逐个核实过）：宿主 1437→1413（117 个随 main 的包迁移进了 `packages/`，1 个改名进 feature-core）；driver-api 403→396（8 个方言 SQL 测试随方言 SQL 搬到各驱动 crate，另加 1 个）。**零测试丢失，只是换了包。**

### 协调者对本波新增事实的裁定

- **`grid.count.*` 错误码域已空**：契约 00 §8.2 第 16 行已标「已删除」，§8.3 域计数表已改 0。代价（诚实记录）：后端真的因「驱动不支持估算」而拒绝时，前端不再有专属分类，落回原始消息展示。
- **i18n key 也不得占用 `grid.*` 命名空间**：`grid.<域>.<原因>` 是错误码专用 ⇒ 「这个库估不出来」的 i18n key 从 `grid.count.estimateUnavailable` 改名为 `pagination.estimateUnavailable`（渲染在分页器上，`pagination.*` 是那一片的既有命名空间）。契约 00 §8.2 末尾已补这条通则。**命名即事实**。
- **类型门禁改成只跑 `tsrs`**（用户要求）：提交 `00e373c4b`。本地门禁不再等于发布门禁——发布流水线的 `union-typecheck` 仍跑权威的 `tsc`（TS 7 GA），所以「PR 绿但发版红」是可能的；`pnpm typecheck:tsc` 是逃生口。Windows 上 tsrs 无二进制 ⇒ **回退**到 tsc 并打印回退原因（绝不静默）。

## 本波的两条决策（协调者裁，优先级高于分册细节）

### ① `grid.count.estimateUnavailable` **不是错误码**

它描述的是**前端能力位**（用户要了估算 + 面板已加载 + 答案不是估算），不是后端拒绝。⇒ 走错误码通道是错的，且 `PRODUCER_PENDING` 会变成垃圾桶（它永远不会有生产者）。改为：`Pagination` 新增 `estimateUnavailable` prop **直出**文案；`src/lib/gridErrors.ts` 的联合类型与注册表各删一行；`PRODUCER_PENDING` 去掉该条。

代价（诚实记录）：后端真的因为「驱动不支持估算」而拒绝时，前端不再有专属分类 ⇒ 落回原始消息展示。这可接受，因为后端在有生效筛选等场景已经**降级为精确**而非拒绝。

### ② 命名视图的落地形态：**共享规则进 feature-core，持久化留 host**

用户定则要求「两端共同逻辑放共享 crate，`src-tauri` 只做 command 适配」。08 §3.7 的 T0-12 说「持久化留 host，web 侧 desktop-only，迁移存储端口是独立契约改动」。

⇒ 本波把「独立契约改动」**做掉**：
- `packages/feature-core/src/services/named_views/`：DTO + 与形态无关的规则（名称归一 / scope key / 导入重名解析 / validate）+ **存储端口 trait** + domain service；
- `src-tauri/src/store/named_views.rs`：桌面 `Store` 上的端口实现（只做持久化）；
- `src-tauri/src/commands/named_views.rs`：薄壳（校验 → 转发 → 错误映射）；
- web 侧当前无可写 `{appData}` ⇒ 仍须**明确的能力缺失**而不是空列表；`MethodMap` / `BackendClient` / 前端封装 Wave 2 已铺好，本波只把桌面侧注册好并从 `DESKTOP_UNSUPPORTED_METHODS` 删三条。

## 本波已知的形状隐患（Coder 必须处理）

1. **同一个 DTO 两种形状**：`packages/backend-client/src/types/tableData.ts:227-231` 的 `FilterCondition` **没有 `connector`**，而 `src/types/filters.ts:88-101` 与 Rust `packages/driver-api/src/filters.rs:25-35` **都有** ⇒ 保存视图会**丢逐条连接符**。⇒ 只留一份权威 TS 定义 + 逐字相等断言。
2. **`updatedAt` 形状冲突**：TS 是 `string`（必填），Rust 是 `Option<DateTime<Utc>>` ⇒ serde 发 `null`。两侧必须一致。
3. **桌面错误码可达性**：`CommandError::Serialize` 发纯字符串 ⇒ 前缀会丢；前端必须用 `src/lib/fkNavigationActions.ts:90-112` 的 `hostErrorMessage` 沿 `cause` 链取原文，**不能**直接用 `error.message`。

## 轨 B `named-views` 的 12 项裁定（协调者，逐条给理由）

Coder 交付 5 提交（`66fc97aae`/`fa0dcfd59`/`c4a8c98b4`/`c6b59dda0`/`89ec12d01`），并把新基线并进来后 HEAD = `c9d881ec4`。12 项待裁定的**逐条结论**：

| # | 事项 | 裁定 | 理由 |
| --- | --- | --- | --- |
| 1 | 改共享 crate `packages/driver-api/src/filters.rs`：`connector` 加 `skip_serializing_if = "Option::is_none"` | **接受** | 契约 00 §5.2.1 明写「缺省不写」。线上出现 `connector: null` 就是「没有连接符」的**第二种拼法**，TS 侧就得同时处理「键缺失」与「键为 null」——正是本项目一直在消灭的「同一个 DTO 两种形状」。反序列化完全不变（null↔None↔缺省等价）。**并且它修掉了一个潜在 bug**：既有 TS 侧用 `!== undefined` 判定，`null` 会让判定为真 |
| 2 | `updatedAt` 选型 | **接受 `string | null`** | 对齐契约的 Rust `Option<DateTime<Utc>>`（serde 会发 null）。另一选项（Rust 改非可选 + `default = now`）会把「从未更新」的记录**谎报**成「刚更新」 |
| 3 | 多加 1 条 i18n key `filter.errorUnsupportedOperator`（22 条 = 契约 §8 的 21 条 + 这一条） | **接受** | 它**本来就在**契约 §8 的 08 词条清单里，只是基线没实现；命名视图恢复视图时需要「宿主不认识的算子」的文案。不加只能硬编码英文（违反 i18n 纪律） |
| 4 | `hostErrorMessage` 从 `src/lib/fkNavigationActions.ts` 搬到 `src/lib/hostErrorMessage.ts`，原路径**再导出** | **接受** | 菜单组件要拿错误原文，而那个纯函数所在模块拖着 fkNavigationStore / panelStore / tableDataStore 三个 store，会把 `src/commands/database` 拉进组件依赖图、打断既有测试的 `vi.mock`。再导出保住了既有 import 路径 |
| 5 | tsrs 在 scripts program 上多报 1 条 `TS2559`（`scripts/__tests__/with-driver-inject.test.ts:71`） | **销案（不是缺陷）** | 该文件与 origin/main 逐字节相同；**origin/main 修了 `scripts/with-driver-inject.mjs` 的 JSDoc**，所以它在新基线上**已经不存在**。协调者在集成分支实测 tsrs 门禁 EXIT=0。Coder 看到的是旧基线上的残留 |
| 6 | `tsconfig.json` 显式列 `types` | **接受** | TS 7 不再自动发现 `@types`；该清单与 origin/main 根配置**逐字一致** ⇒ 落新基线是同一份改动、不产生冲突 |
| 7 | `store/mod.rs` 只再导出 `HostViewIdMinter`、**不再导出 DTO**（契约 §6 的文件清单写的是「mod named_views; + re-export DTO」） | **接受偏离**，并已在契约 08 §6 记一笔 | DTO 的权威位置是 feature-core（按 T0-1/T0-2 裁定）；宿主从那里 import。再导一份等于给同一个类型**第二个名字**——这正是「同一个 DTO 两种形状」的源头 |
| 8 | 端口 DTO 的 `operator` 刻意保持 `string`，「两份 FilterCondition 逐字相等」做成逐字段断言 | **接受** | 端口**不该**知道某个方言声明了哪些算子；`operator` 逐字相等**不可能**，是设计本身。逐字段 + 编译期断言是能做到的最强形态 |
| 9 | web 边界：仓内没有 web transport ⇒ 能力缺口落在端口契约（`unsupportedMethods` + kind `capability`） | **接受** | 「桌面已实现」与「web 能服务」是**两种不同的缺口**（`notImplemented` vs `capability`），不得混为一谈；前端据此隐藏入口 |
| 10 | Step 16 E2E（`e2e/specs/table-saved-views.ts`）未做 | **接受，留 R 阶段** | E2E 需要完整 app 构建与真实库 |
| 11 | `TableToolbar` 黄金文件未变且仍绿；`error-banner-call-site-parity` 的快筛调用点改指 `TableFilterToolbar.tsx` | **接受，但须独立复验守卫仍有牙齿** | 协调者已复验过同类改动（`probe-exclusion`）：断言从「文件里的原始数组」换成「编译器解析后的有效值」时，**必须**证明去掉一个模式仍会红 |
| 12 | 三条命令已进 `generate_handler!` 且有 `include_str!` 注册自检；`grid.view.*` 有真实生产者、**未**进 `PRODUCER_PENDING` | **接受** | 协调者已抽查：Rust 侧两个码的真实 throw 点在 `packages/feature-core/src/services/named_views/error.rs`（前缀在第 0 位）、三条命令都在 `bootstrap/run.rs`、桌面 `DESKTOP_UNSUPPORTED_METHODS` 已清空 |

## 流程事故：磁盘被 cargo target 撑爆（已修，纪律已加）

**症状**：`/System/Volumes/Data` 只剩 **166 MiB**（460 GiB 的卷 100% 满）。表现为：某个代理的 `npx vitest run` 整棵树崩掉（每个文件 `0 test`、尾部 Node 崩溃栈、无异常信息），`git worktree add` 报 `No space left on device`，`resolve-drivers` 中途失败。

**根因**：历次并行轨各自的 `CARGO_TARGET_DIR`（`/tmp/datazen-target-*`）**共 18 个目录、约 193 GB**，加上仓库自己的 `target/` 45 GB。cargo target **不会**因为轨结束而消失——它们只是躺在 `/tmp` 里。

**已修**：删掉 14 个**陈旧**的 target 目录（保留当时在用的两个），可用空间从 166 MiB 回到 **201 GiB**；主检出的生成物重新生成。

**纪律（新）**：
1. **每条轨的 `CARGO_TARGET_DIR` 在轨验收后必须删掉**（`rm -rf /tmp/datazen-target-<track>`），由协调者在合流收尾时统一做。
2. 代理跑完 Rust 套件后**自己删自己那个** target 目录，不要留给下一个轨。
3. 「整棵树 `(0 test)` + 无异常信息的崩溃」**先查磁盘**，不要当成代码问题去改代码——本次就有一名代理差点把环境事故当成自己的缺陷。

**顺带修掉的两处环境漂移**：
- `node_modules/typescript` 的根软链指向陈旧的 **5.9.3**（package.json 声明 7.0.2）⇒ `pnpm typecheck:tsc` 跑的不是权威编译器。已改指 7.0.2，`tsc 7.0.2` 与 `tsrs` 两个引擎在集成分支上实测全 EXIT=0。**「本地门禁=发布门禁」这句话此前在这台机器上是不成立的。**
- 合并 `origin/main` 造成三个 tsconfig 各有**两段顶层 `$comment`**（ts7 PR 的 + 我们分支的探针 exclude 说明）⇒ JSON 只允许一个 ⇒ 后者覆盖前者，且每次 TS 调用打一条 `Duplicate key` 警告。**修法不是「换成 ts7 PR 的版本」**——origin/main 删掉了探针机制，但我们分支的 `scripts/__tests__/probe-exclusion.test.ts` 还在（它在 origin/main 里不存在），连同三处 `exclude` 必须留着；换掉会把探针放回 program。改成把两段合成一段（24 / 23 / 19 条），exclude 与 types 逐字保留。实测：两个引擎三个 program EXIT=0、Duplicate key 警告 0 条、探针守卫 4 条仍绿。

## D1 的裁定：宿主规则消息只写宿主自己的话

**缺陷**（Tester 查出，低危不阻断）：命名视图的**名字是用户手打的自由文本**，被原样嵌进 IPC 错误消息（`grid.view.nameExists: a saved view named '{name}' already exists for this table`）。宿主脱敏器 `redact_secrets_for_log` 只覆盖它文档化的几种形态（JSON 式 `"password": "…"`、查询串 `?password=…` / `&password=…`、bearer 令牌、URL userinfo），**不覆盖自由文本里的裸 `key=value`** ⇒ 名为 `password=hunter2` 的视图，其字面量未经脱敏进入本机 IPC 载荷与本机日志。

**为什么这是真缺陷而不只是洁癖**：宿主消息里第一次出现了**完全用户手打的自由文本**。此前的宿主消息嵌的是**受限词汇表**里的东西（表名/列名来自库 schema，算子名来自固定枚举，如 `grid.filter.incomplete: column "id" needs a value`）——那些不是自由文本。视图名是。

**裁定（不是三选一，是一条规则）**：**宿主规则消息只写宿主自己的话——用户输入可以被描述，绝不被引用。**

- **不**给脱敏器加「自由文本 key=value」规则：误报率不可控，且规则会越滚越多（下一个自由文本字段又要加一条）。
- **不**把「用户自撰文本不保证脱敏」写成契约边界：那是把漏洞登记成特性。
- 而是**让消息里根本没有用户文本**。这条规则一次成型，且可审计：*宿主消息只含宿主撰写的文本*。

**落地范围（协调者核实过不止一处）**：
1. `NamedViewError::NameExists` 的消息去掉视图名（名字仍作为**结构化数据**留在变体里，加 `name()` 访问器——调用方要用还能拿到，只是不进消息）；
2. `dto.rs` 里 `filter_logic` 校验消息**回显了用户给的值**（`got "{}"`）——**同一类漏洞，容易漏**，一并去掉；
3. 补两条永久回归锁：视图名含 `password=hunter2` ⇒ 消息里既没有名字也没有 `password=`；`filter_logic` 含凭据形态 ⇒ 消息里没有该值。

**代价（诚实记录）**：宿主日志从此看不到「是哪个视图名冲突了」。前端**本来就看不到**（`describe()` 换成本地化词条 `filter.view.errorNameExists`），所以这个信息对用户没有价值；真要排查，store 层的测试与结构化载荷仍在。

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

## 门禁基线（Wave 6 合流后实测）

- `npx vitest run` ⇒ EXIT=0，693 files / 7197 tests
- `pnpm typecheck` ⇒ EXIT=0，error TS 0 / TS2578 0
- Rust：宿主 1437 / feature-core 428 / driver-api 403 / platform-infra 26 / postgres 215，全绿；`cargo fmt --check` EXIT=0

## 波次总计划（9 波）

| Wave | 内容 | 状态 | 共享文件（合流时需手工解） |
| --- | --- | --- | --- |
| 1 | `grid-selection` + `insert-backend` | ✅ | — |
| 2 | `grid-cell` + `web-transport` | ✅ | — |
| 3 | `insert-frontend` | ✅ | — |
| 4 | `dirty-cells` + `fk-navigation` | ✅ | — |
| 5 | `fk-navigation-ui` + `filters` | ✅ | — |
| 6 | `filters-frontend` + `count-backend` | ✅ | `src/stores/tableDataStore.ts` |
| 7 | `count-frontend` + `named-views` | 🔄 进行中 | `src/locales/en/query.ts`、`src/lib/gridErrors.ts`、`src/platform/tauriBackendTransport.ts` |
| 8 | **02 键盘 ∥ 11 键集分页** | ⏳ | `DataTable.tsx`、`TableView.tsx`、`src/locales/en/query.ts` |
| 9 | **03 剪贴板 ∥ 10 结果网格** | ⏳ | `src/lib/gridErrors.ts`、`src/locales/en/query.ts` |
| 10 | **05 编辑器**（单轨） | ⏳ | `src/lib/gridErrors.ts`、`src/locales/en/query.ts`、`src/lib/tableChanges.ts`、`src/lib/dataTableContextMenu.ts` |
| R | 12-testing 的 **D1–D8 审计** + 全量回归 + E2E 补齐 + i18n 补齐 + 契约改写 + 删 `docs/prd/` | ⏳ | — |

### 12-testing 不是一条「并行代码轨」

它是**整批方案的共享测试基建册**：第 1.1 节给出的是**完成定义 D1–D8**（契约不破 / 按 §9 矩阵交付 / 门禁逐条全绿 / 既有测试同步更新 / **至少一次反向注入** / 环境归属写清 / **零文案断言** / 类型干净），第 5 节给的是 journey 骨架。⇒ 它的落地形态是**R 阶段的一次独立审计**（逐条对 D1–D8 检查已交付的 12 册），不是与 05 并行的一条轨。

### `packages/driver-api/src/traits.rs` 是**硬串行点**

sqlserver 的 `file_line_cap.rs` 把 `traits.rs` 钉在 **≤1265 行**，且是 **ratchet**（只许缩不许涨；加 1 行 ⇒ `cargo test -p datazen-driver-sqlserver` 变红）。⇒ **10 与 11 不能并行**（两册都要往 `traits.rs` 加 trait 方法，都必须腾出行数），必须串行。11 排在 10 之前（11 的方法体可落进新子模块 `traits/keyset.rs`，腾行数的成本更低）。

R 阶段：全量回归 ⇒ 逐节把 `docs/prd/data-browsing-design/` 改写为「已实现事实」并入 `docs/architecture/` 与 `docs/features/` ⇒ 删除 `docs/prd/` 整个目录 ⇒ 清 G9（i18n）⇒ 推分支。
