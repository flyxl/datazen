# 12 · 数据浏览测试基建（共享）

> **本册地位**：这是数据浏览整批方案（DB-01 / DB-02 / DB-03 / DB-04 / DB-05 / DB-06 / DB-07 / DB-08 / DB-09 / DB-14 / DB-18）的**共享测试基建册**。`01-selection` 到 `11-keyset-paging` 每一份分册的第 9 节（测试清单）都必须按本册的分类、落点、命名、夹具与门禁口径来写；分册不得各自定义「测试放哪」「结论怎么写」「什么算通过」。
>
> **目标读者**：第一次给本项目写测试的实习生。本册假设你已读 `00-contracts.md`（尤其是第 1.2 节的 `tid()` 与 `data-dt-*` 约定、第 4.4 节的 DOM 属性扩展表、第 10 节的分册模板），但没写过本仓任何一条测试。
>
> **本册不重复仓库既有规范，只补充数据浏览这一批方案特有的做法。** 下面这些规范请按原文执行，本册只说明「在数据浏览场景里怎么落地」：
>
> | 既有规范 | 本册对它的落地补充 |
> | --- | --- |
> | [AGENTS.md](../../../AGENTS.md) 的「测试命令的长输出必须落临时文件再读」「测试文件参与类型检查」「同一棵工作树不得同时被提交方和验证方使用」「驱动测试落点」「E2E 测试」 | §7 给出可直接复制的门禁脚本与结论记录格式 |
> | [docs/architecture/testing.md](../../architecture/testing.md) 测试分层与运行命令 | §3 把四类落点压成一张表，并写清各自「不适合测什么」 |
> | [docs/development/interaction-and-testing-principles.md](../../development/interaction-and-testing-principles.md) 状态机 / 软排序 / Journey Test / 数据属性解耦 / 三维自查 / 零文案断言 | §5 给出可直接抄改的 journey 骨架；§10 把它变成逐条错题 |
> | [docs/development/verification-methodology.md](../../development/verification-methodology.md) 证据分级、反向注入、假守卫 | §1 把「全绿不算证据」写进本批方案的完成定义 |
> | [docs/development/panic-policy.md](../../development/panic-policy.md) 生产路径禁止裸 `unwrap()` / `expect()` | §4「mock 与真实依赖的边界」说明测试豁免的确切边界 |
> | [docs/development/worktree-isolation.md](../../development/worktree-isolation.md) worktree 隔离与结果不可比 | §7 的 HEAD / 工作区 sha 首尾记录，§6 的驱动集语义 |
> | [docs/development/e2e-testing.md](../../development/e2e-testing.md) E2E 操作手册 | §3 只写数据浏览相关的 suite 与落点；构建、排错、环境准备照该手册，不在此重复 |

---

## 1. 目标与验收口径

### 1.1 本批方案的完成定义（Definition of Done）

一个数据浏览改动**同时**满足下表 8 条才算做完。「我本地跑绿了」不构成任何一条的替代。

| # | 条件 | 可验证的判据 |
| --- | --- | --- |
| D1 | **不破坏契约** | `00-contracts.md` 第 2 节 R1–R5 逐条成立：既有方法签名未改、新增 `DatabaseDriver` 方法都有默认实现且默认行为等于今天、新增 DTO 带 `#[serde(rename_all = "camelCase")]`、新增能力字段默认「不支持」、生产路径无裸 `unwrap()` / `expect()` |
| D2 | **测试按 §9 矩阵交付** | §9 总表里该方案那一行的每个「数量下限」都达到；交互型方案**至少 1 条 journey test** 且落在 §3 规定的目录 |
| D3 | **门禁逐条全绿** | 按 §7.2 的命令清单跑完，每条都有**退出码 0** 与**逐字结论行** |
| D4 | **无回归，且既有测试同步更新** | 分册第 9 节必须列出「本册不变量是否被破坏 + 需要同步修改的既有测试有哪些」；改了 `DataTableProps` 就必须同步 `src/components/DataTable/__tests__/DataTable.test.tsx` 之类既有用例，而不是删断言 |
| D5 | **证据等级达标（本批硬要求）** | 每份分册至少报告 **1 次反向注入**：亲手把被测行为改坏、**亲眼看到用例转红**，再还原。只写「已覆盖」不算证据（依据 [verification-methodology.md](../../development/verification-methodology.md) 第 2 节「断言存在 ≠ 断言能失败」） |
| D6 | **环境归属写清楚** | 结论里必须记明**驱动集语义**（见 §2.4）与 **Pro 检出状态**；「Host 用例全绿」只在该驱动集语义内成立 |
| D7 | **零文案断言** | 不把任何由 `t()` 渲染出的可见文案钉进断言或定位器；只用 `data-*` 契约属性、`data-i18n-key`、i18n key、必要时 `enCopy()` 字典回读（依据 interaction-and-testing-principles 原则六） |
| D8 | **类型干净** | `pnpm typecheck` 干净。测试文件**参与**类型检查（见 §2.2），mock 只实现子集时用 `as unknown as X` / `satisfies` / `Pick<>` 精确说明，**禁止用 `any` 绕**，也禁止为了让类型通过而删测试或删断言 |

### 1.2 什么算「通过」

一次门禁运行只有同时具备下面三条才算通过，缺一条就是「没跑」或「结论不可归属」：

1. **退出码**：命令的 `$?` 被单独打印并如实记录（不是从日志尾部猜的）。
2. **逐字结论行**：从日志尾部原样抄录工具自己给的那一行 —— `vitest` 是 `Test Files` / `Tests` 两行；`cargo test` 是 `test result:`；`tsc` 是无输出 + `Exit=0`；脚本门禁是脚本自己打印的总结行。**禁止改写成自己的话**。
3. **归属信息**：运行期间没人在同一棵树上动过（§7.3 的首尾 sha），且驱动集与 Pro 状态已记录（§7.4）。

### 1.3 什么**不**算通过

- **只看「绿」不看结论行**：`vitest.config.ts` 的根 `test` 块设了 `retry: 2`，并且该文件自己写明「失败后重试通过会被报告为 PASSED，所以全绿**不是**没有间歇性失败的证据」。看到绿就宣布「无 flake」是无效推断。
- **只跑 happy path**：只测静态合法输入（完整 TSV、格式完备的筛选条件、一次就成功的编辑）等于没测。见 §5 的禁止项。
- **本地 `npx vitest run` 绿了就推断 CI 绿**：驱动集不同（§2.4）。
- **把假守卫当覆盖**：`toContain` 命中别处、断言「存在」而非「恰好一次」、断言两侧同源导出 —— 这三类是已知假守卫形态，发现即删掉重写。

### 1.4 每份分册必须回填的三句话

分册第 9 节结尾必须给出这三句（写不出就是没做完）：

1. **不变量**：本册改了哪条既有行为，`00-contracts.md` 的哪条铁律仍然成立，凭什么。
2. **既有测试影响面**：哪些既有测试会因本册改动而需要同步修改，列出**文件路径 + 用例名/`it` 描述**。
3. **怎么证明**：§7 的命令清单中本册实际跑了哪几条、各自的逐字结论行、以及那 1 次反向注入的内容与观察到的红。

### 1.5 一个改动的完整验收动作顺序

按顺序做，**不要跳步**（跳第 1 步的代价最大：结论不可归属时，前面所有工作都要重做）。

| 序 | 动作 | 完成标志 |
| --- | --- | --- |
| 1 | **先取基线**：记录 HEAD sha、工作区内容 sha（§7.3）、当前驱动集、Pro 检出状态与 EP 版本 | 四项都写下来了 |
| 2 | **先跑一遍既有门禁**（在没改任何东西时） | 得到一条**基线失败集合**；后续所有失败都能与之对比，环境造成的既有失败不会被误算成本批回归 |
| 3 | 按 §3.3 决策树确定落点，按 §4 写测试，按 §5 写 journey | 代码与测试同时在手 |
| 4 | **反向注入**：把被测行为改坏，看目标用例转红，再还原 | 亲眼看到红，且红的是**该用例**而不是别的（§1 D5） |
| 5 | 跑 §7.2 里本改动类型对应的**全部**命令，长输出落系统临时文件 | 每条都有退出码与逐字结论行 |
| 6 | **再取一次** HEAD 与工作区 sha，与第 1 步逐字比对 | 两次完全一致 |
| 7 | 回填分册第 9 节的三句话 + 门禁结论块（§7.4 模板） | 分册可被独立验收 |
| 8 | 交验证方时，验证方在**独立工作树**上重跑（§7.3） | 两份结论互相独立 |

---

## 2. 现状代码事实

以下全部是我在本工作树里**亲自读过**的符号与文件，不是推测。按 AGENTS.md 纪律，位置一律「文件路径 + 符号名」，**不写行号**。

### 2.1 测试定位基建（`tid()` 与契约属性）

| 文件 | 符号 | 对数据浏览测试的含义 |
| --- | --- | --- |
| `packages/ui/src/tid.ts` | `tid(id)`、`TidAttrs` | 当且仅当 `import.meta.env.VITE_E2E` 为真时返回 `{ 'data-testid': id }`，否则返回 `{}`。命名约定 `<area>-<element>-<action>`（kebab-case） |
| `src/lib/tid.ts` | 再导出 `tid` / `TidAttrs` | 宿主统一从 `src/lib/tid` 引，不直接 import 包 |
| `packages/ui/src/__tests__/tid.test.ts` | `describe('tid')` | 钉住上面这条契约：生产构建不渲染任何测试属性、E2E 构建逐字返回 id、每次调用重新求值（无模块级缓存） |
| `src/lib/__tests__/tid.test.ts` | `describe('tid')` | 宿主再导出 shim 的身份守卫 |
| `scripts/e2e-tauri-build.mjs` | `process.env.VITE_E2E = process.env.VITE_E2E \|\| '1'` | E2E / webdriver 构建才是注入 `data-testid` 的那一次构建 |
| `e2e/run.mjs` | 仅当 `isPro && (--suite pro-query-builder \|\| --suite pro-screenshots)` 时，给更早的 `resolve-pro` 打包步骤设 `VITE_E2E = '1'` | 那个分支要让 `window.__qbTest` 测试桥存在；说明 `VITE_E2E` 的开与关是**分步骤**控制的，不是全局开关 |
| `src/components/DataTable/VirtualBody.tsx` | 单元格 `data-testid="data-table-cell"`、`data-dt-row`、`data-dt-col` | **无条件渲染**，与 `VITE_E2E` 无关 —— 这是今天唯一同时适用于单测与 E2E 的单元格锚点 |
| `src/components/DataTable/TableHeader.tsx` | 表头单元格 `data-col-header` | 同上，列名锚点 |
| `src/lib/dataTableContextMenu.ts` | `resolveDataTableCellFromEvent`、`resolveDataTableHeaderColFromEvent` | 用 `target.closest('[data-dt-row][data-dt-col]')` / `closest('[data-col-header]')` 反查。这是本项目确认过的正确做法，**禁止**用视口几何坐标反查 |
| `00-contracts.md` 第 4.4 节 | `data-dt-selected` / `data-dt-dirty` / `data-dt-editing` / `data-dt-cell-type` | **契约新增**，由分册 01 / 05 / 06 实现。本册所有 journey 断言都建立在这四个属性上 |

> **给实习生的第一条硬提醒**：`tid()` 在 vitest（jsdom）里返回 `{}`，因为 `VITE_E2E` 没被设。所以
>
> - **E2E spec** 用 `$('[data-testid="..."]')` 定位，这个 `data-testid` 来自 `tid()`；
> - **Host 组件单测** 不能用 `getByTestId(tid('x'))`，那个属性根本不存在。两条合法出路：
>   1. 首选：改用**无条件渲染的契约属性**（`data-dt-row` / `data-dt-col` / `data-dt-selected` / `data-dt-dirty` / `data-col-header`），这些是数据浏览交互的权威锚点；
>   2. 次选：在用例内 `vi.stubEnv('VITE_E2E', '1')` 让 `tid()` 生效，并在 `afterEach` 里 `vi.unstubAllEnvs()`。既有先例是 `src/windows/connection/__tests__/QueryPanel.paneRouting.test.tsx` 与 `src/windows/connection/__tests__/query.modules.test.tsx`。**用次选必须在用例注释里说明为什么不能走首选。**

### 2.2 测试配置与类型检查

| 文件 | 关键事实 |
| --- | --- |
| `vitest.config.ts` | `environment: 'jsdom'`、`setupFiles: ['./src/test/setup.ts']`、`testTimeout: 10_000`、`retry: 2`。`include` 覆盖 `src/**/*.test.{ts,tsx}`、`scripts/__tests__/**`，以及 `packages/{wapp-sdk,extension-points,driver-sdk,backend-client,ui}` 的测试目录；**不含 `packages/drivers`** |
| `vitest.config.ts` 的 `coverage` | `include` 里有 `src/lib/**`、`src/stores/**`、`src/components/DataTable/**`；阈值 `lines 80 / functions 80 / branches 75 / statements 80`。数据浏览新代码基本全落在这三块里 |
| `vitest.drivers.config.ts` | `include: ['packages/drivers/**/*.test.{ts,tsx}', 'packages/drivers/**/__tests__/**/*.{ts,tsx}']`，`setupFiles` 额外加载 `./src/test/driverUiSetup.ts`，`exclude` 掉 `kiwi` / `olap` / `superset` |
| `vitest.e2e-contract.config.ts` | `environment: 'node'`、`include: ['e2e/contract/**/*.test.ts']`，对 `e2e/contract/fixtures.ts` 与 `e2e/contract/journeys/plan.ts` 单独设 80% 行覆盖阈值 |
| `tsconfig.json` | **没有 `exclude` 字段**，`include` 是 `src` 加各 package 目录（含 `packages/drivers/*/ui`）。因此 `src/**/__tests__/**` 与 `*.test.ts(x)` **参与** `tsc --noEmit` —— 这就是 AGENTS.md 说的「测试文件参与类型检查」 |
| `tsconfig.json` 的 `include` | **不含 `e2e/`** |
| `e2e/tsconfig.json` | 存在，`include` 覆盖 `e2e/**/*.ts` 与若干驱动/Pro 的 e2e 目录，但**当前没有任何 `package.json` script 或 workflow 驱动它**。结论：E2E spec 的类型错误不会被 `pnpm typecheck` 拦住，只能靠真跑该 spec 暴露 |
| `src/test/setup.ts` | 注册 `@testing-library/jest-dom/matchers`；补了 jsdom 缺失的 `Range.prototype.getClientRects`（CodeMirror 需要，与本批方案无关） |
| `src/test/driverUiSetup.ts` | 驱动 UI 套件的 i18n 装配：真实走 `ui/meta.ts` 侧效应注册驱动词条；并在 setup 阶段 stub 掉 `@tanstack/react-virtual`，因为 **jsdom 没有排版引擎，虚拟列表在真实实现下渲染 0 行** |
| `src/test/enCopy.ts` | `enCopy(key)` —— 字典回读的唯一合法入口；查表 miss 或空串**当场抛错**（裸 `en[key]` miss 得到 `undefined`，会让 `getByRole(..., { name: undefined })` 退化成永真匹配） |

### 2.3 既有测试资产（本批方案要接上去的）

| 位置 | 符号 / 内容 |
| --- | --- |
| `src/components/DataTable/__tests__/` | `DataTable.test.tsx`、`VirtualBody.test.tsx`、`VirtualBody.alignment.test.tsx`、`TableHeader.test.tsx`、`CellRenderer.test.tsx`、`CellRenderer.branches.test.tsx`、`EditableCell.test.ts`、`EditableCell.component.test.tsx`、`Pagination.test.tsx`、`DetailPanel.test.tsx`、`DetailPanelToggle.test.tsx`、`DataExportDialog.test.tsx` |
| `src/components/DataTable/__tests__/VirtualBody.test.tsx` | 用 `vi.mock('../../../hooks/useVirtualTable', ...)` + `vi.mock('../../../hooks/useI18n', ...)`，用 `container.querySelector('[data-dt-row]...')` 一类选择器断言。**数据浏览组件测试的默认写法照抄它** |
| `src/components/DataTable/__tests__/DataTable.test.tsx` | 用 `vi.hoisted(() => ({ showNativeContextMenu: vi.fn() }))` 提升 mock 变量（`vi.mock` 声明被提升，闭包引用必须先 hoist，否则运行期报错） |
| `src/stores/__tests__/tableDataStore.test.ts` | `vi.mock('../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }))`，用 `deferred<T>()` 手工控制 Promise 解析来测在途请求与竞态 |
| `src/stores/tableData/__tests__/` | `connectionState.test.ts`、`filterUtils.test.ts`、`pendingChanges.test.ts` |
| `src/stores/tableData/pendingChanges.ts` | `effectivePendingIdentity`、`rowIdentityIsUnique`、`hasPendingIdentityCollision`、`findPendingForRow`、`rebuildEditBuffer`、`overlayPendingRows`、`pendingChangesSignature`、`pendingChangesForWire` |
| `src/stores/tableData/connectionState.ts` | `rowsToRecords`、`editKey`、`toCellValue`、`extractErrorMessage`、`emptyTableState`、`buildTableContext` |
| `src/lib/tableChanges.ts` | `RowIdentity`、`valuesEqual`、`buildRowIdentity`、`rowIdentityKey`、`duplicateRowIdentityKeys`、`clonePendingRowChange`、`isCompleteTableChangeContext` |
| `src/hooks/useVirtualTable.ts` | `useVirtualTable({ rows, rowHeight, overscan, scrollElement })`，内部即 `@tanstack/react-virtual` 的 `useVirtualizer` —— jsdom 下必须 mock |
| `src/lib/__tests__/connectionShareError.test.ts` | **错误前缀分类器测试的现成范式**：「已知消息映射到 i18n key」+「未知消息原样透传」+「空消息走 fallback」 |
| `src/lib/connectionShareError.ts` | `translateConnectionShareError` 用 `trimmed.startsWith(...)` 做前缀匹配，匹配不到 `return trimmed`（不吞错误） |
| `packages/drivers/*/ui/__tests__/` 与 `packages/drivers/*/tests/` | 驱动专属用例的真实形态。例如 `packages/drivers/postgres/ui/__tests__/dialect.test.ts` 直接断言 `postgresqlDialect` 的 DDL 字符串与 `postgresqlDialectProfile` 的语义字段；`packages/drivers/postgres/tests/` 下是集成测试（`real_driver_contract.rs`、`process_commands.rs` 等） |
| `packages/drivers/redis/ui/__tests__/*Journey.test.tsx` | 本仓 journey test 的既有范式（`keyHeaderRowJourney.test.tsx`、`ttlControlsJourney.test.tsx`、`dirtyLeaveJourney.test.tsx` 等），文件头逐条写明「覆盖状态机而非静态快照」 |
| `e2e/specs/table-data.ts` / `table-filter.ts` / `table-edit.ts` | 数据浏览的既有 Host E2E。写法：`import { expect, browser, $ } from '@wdio/globals'`、从 `../helpers.js` 引动作、从 `../i18n.js` 引 `t()` 做文案回读；`before` 用 `executeSQLChecked` 建表灌数、`after` best-effort 清理 |
| `e2e/contract/` | `fixtures.ts`（`DRIVER_FIXTURES`、`DriverFixtureId`、`HostContractJourneyId`、`JOURNEY_REQUIREMENTS`、`journeyAllowed`、`skipReason`、`dataSeedSql`、`filterSeedSql`、`contractTableDdl`）、`journeys/plan.ts`、`journeys/seed.ts`、`journeys/run-core.ts`、`open-fixture.ts`，单测在 `e2e/contract/__tests__/` |
| `e2e/specs/host-contract-matrix.ts` | 契约矩阵的 WDIO 入口，`wdio.conf.ts` 的 `contract` suite 只含这一个 spec |

### 2.4 环境事实（本工作树与 CI 的真实差异）

**（a）驱动集**。本工作树的 `src/extensions/generated.ts` 是由 `--drivers=basic` 铺出来的：`DRIVER_DB_ENTRIES` 当前有 11 个 dbType（`postgresql`、`questdb`、`cloudberry`、`mysql`、`mariadb`、`doris`、`starrocks`、`manticore`、`ob_oracle`、`sqlite`、`redis`），来自 4 个 path driver 包（postgres / mysql / sqlite / redis）。前端 `DB_REGISTRY` 由 `DRIVER_DB_ENTRIES` 合并而来，所以**驱动集一变，同一份 Host 单测读到的注册表就变** —— 例如 `src/lib/__tests__/databaseTypes.test.ts`、依赖 `DB_REGISTRY` 的能力声明用例，都会随驱动集改变输入集合。

由此推出两条纪律：

1. 「Host 用例全绿」只在**该驱动集语义内**成立，不能据此推断 CI 结果。CI 在 `pnpm test:unit`（basic）之后还会跑 `pnpm test:unit:driver-set`（默认 `all`，全部已注册 path driver）。
2. `pnpm typecheck` **不受驱动集影响**：`tsc --noEmit` 只按 `tsconfig.json` 的 `include` 走静态文件，不读 `generated.ts` 的内容分支。

**（b）Pro 检出**。本工作树内 `packages/pro-extensions/sql-editor-pro` 克隆自 Pro `main`，其 `manifest.json` 声明 `extensionPointsVersion: "1.0.0"`，而宿主 `packages/extension-points/src/security.ts` 的 `EXTENSION_POINTS_VERSION` 是 `1.1.0` —— **两端不一致是事实**。

但要准确说清它今天会不会让门禁红，否则实习生会误判：

- `packages/extension-points/src/__tests__/security.test.ts` 里**曾经**有两条读真实 `packages/pro-extensions/**/manifest.json` 做「两端版本一致」断言的用例，失败签名正是 `expected '1.0.0' to be '1.1.0'`。
- 该文件自己写明这两条**已被删除**：一致性断言搬到了 Pro 仓库的 `scripts/verify-host-pin.mjs`（三向比对、在 Pro CI 上跑），理由是宿主 CI 结构上拿不到那个仓库，留下的只有「skip」或「本地红」两种结果，「不是门禁」。
- 今天宿主侧只剩「钉住宿主常量」与「用合成 manifest 钉住精确相等比较规则」三类用例，**不读真实 Pro 检出**。我逐文件确认过：`packages/extension-points/src/__tests__/security.test.ts` 是唯一提到 `pro-extensions` 的宿主测试文件，且只把它写在注释里。

**所以**：如果本地门禁出现 `expected '1.0.0' to be '1.1.0'` 这类版本一致性断言失败，那是**环境 / Pro 分支问题，不是数据浏览改动造成的**；但如果今天没出现，也不要写「一定会红」——那不是本工作树的现状。**不要为了消这条错去改 Pro 包的 `manifest.json`，也不要去改宿主 EP 版本常量**（后者是改动契约，等于破坏 D1）。工单边界见 §11 第 1 条。

**（c）`test/` 手工黑盒层不存在**。`git ls-files test/` 为空；`docs/architecture/testing.md` 第 6 节已把该层标为历史结构。数据浏览的新用例**不要**往 `test/` 放。

**（d）环境变量文件**。仓库根 `.env` / `.env.test` 在本工作树**不存在**；`e2e/.env` 存在且被 git 忽略，`e2e/.env.example` 被跟踪。测试程序（`e2e/setup-e2e-env.sh`、`src-tauri/tests/` 下的 live 用例）读它是**合法的**，但**内容不得进上下文或日志**：命令输出可能含凭据时，只摘录结构性结论（哪个文件、哪条断言、退出码、计数），不转发原始输出。

### 2.5 命令与脚本的真实面貌（本册引用的全部已核对）

| 命令 | 真实定义 |
| --- | --- |
| `pnpm typecheck` | `pnpm check:restore-guards && tsc --noEmit && pnpm typecheck:scripts && pnpm typecheck:pack-ep` |
| `pnpm check:restore-guards` | `node scripts/check-restore-block-guards.mjs` |
| `pnpm typecheck:scripts` / `pnpm typecheck:pack-ep` | `tsc -p tsconfig.scripts.json --noEmit` / `tsc -p tsconfig.pack-ep.json --noEmit` |
| `pnpm test:unit` | `vitest run`（默认配置 `vitest.config.ts`，**不含** `packages/drivers`） |
| `pnpm test:unit:drivers` | `vitest run --config vitest.drivers.config.ts` |
| `pnpm test:unit:driver-set` | `node scripts/run-unit-driver-set.mjs`（默认驱动集 `all`；接受 `--drivers=<set>` 与 `DATAZEN_UNIT_DRIVERS`，并把多余 argv 原样转发给 vitest） |
| `pnpm test:unit:coverage` | `vitest run --coverage` |
| `pnpm test:unit:e2e-contract` / `:coverage` | `vitest run --config vitest.e2e-contract.config.ts`（是否带 `--coverage`） |
| `pnpm test:scripts` | `vitest run scripts/__tests__` |
| `pnpm test:ids` / `test:layers` / `test:ci-docs` / `test:version` / `test:driver-protocol` / `test:boundaries` / `test:driver-types` / `test:i18n-keys` / `test:platform-arch` / `test:platform-crates` | 分别是 `scripts/check-id-terminology.mjs`、`check-module-layers.mjs`、`check-ci-docs-consistency.mjs`、`check-version-consistency.mjs`、`check-driver-protocol-compat.mjs`、`check-driver-import-boundaries.mjs`、`check-driver-type-isolation.mjs`、`i18n-key-collision-check.mjs`、`check-platform-crate-boundaries.mjs`、`run-platform-crate-tests.mjs` |
| `pnpm drivers:codegen` | `node scripts/resolve-drivers.mjs --codegen-only`（默认 `basic`） |
| `pnpm drivers:resolve` | `node scripts/resolve-drivers.mjs`（完整注入：Cargo.toml + capabilities + codegen） |
| `pnpm e2e` / `e2e:minimal` / `e2e:skip-build` | `node e2e/run.mjs`（`--minimal-drivers` / `--skip-build`） |
| `pnpm e2e:smoke` / `e2e:db` / `e2e:contract:matrix` / `e2e:contract:pg` / `e2e:journeys` | 分别是 `node e2e/run.mjs --skip-build -- --suite smoke` / `db` / `contract` / `contract --mochaOpts.grep 'Host contract @ postgres'` / `journeys` |
| `pnpm tauri:build:webdriver` / `:minimal` | `node scripts/generate-menu-labels.mjs && node scripts/with-driver-inject.mjs [--drivers=basic] -- node scripts/e2e-tauri-build.mjs` |
| `node scripts/i18n-sync-check.mjs` | 比对英文侧与其它 locale 的 key 集合；两个 scope：宿主领域包 `src/locales` 与驱动包 `packages/drivers/<id>/locales` |
| `node scripts/check-structure-editor-guardrails.mjs` | CI 严格守卫之一（结构编辑器护栏） |
| `cargo fmt --all -- --check` | CI 的 Rust 格式门 |
| `cargo test -p datazen --lib` | Host Rust 单测；CI 用 `.driver-features.json` 的 `features` 数组拼 `--features` |
| `cargo test -p datazen-driver-<id>` / `--tests` | 驱动 crate 单测 / `tests/` 集成测试 |
| `pnpm e2e:contract:matrix` 实际跑什么 | `wdio.conf.ts` 的 `contract` suite 只含 `./specs/host-contract-matrix.ts`；该 spec 按 `DRIVER_FIXTURES`（postgres / mysql / sqlite）参数化，用 `journeyAllowed` / `skipReason` 按能力跳过，journey 定义与 fixture 定义都在 `e2e/contract/` |

---

## 3. 测试分层与落点

### 3.1 四类落点总表

| 类别 | 放哪个目录 | 跑哪条命令 | 适合测什么 | **不适合**测什么 |
| --- | --- | --- | --- | --- |
| **Rust 单测**（Host） | `src-tauri/src/**` 内联 `#[cfg(test)] mod tests`；宿主编排集成测试在 `src-tauri/tests/` | `cargo test -p datazen --lib`（集成另跑 `cargo test -p datazen`） | SQL 拼装（`QueryExecutor::build_select_sql` / `build_count_sql` / `format_condition` / `filter_is_complete` / `filter_join`）、`RowChangePlan` 组装与 `changes_fingerprint`、`execute_row_change_plan_impl` 的影响行数校验、错误前缀生成、serde 线上形态 | 任何驱动方言细节（那是驱动 crate 的事）、任何前端交互与渲染 |
| **Rust 单测**（`driver-api` 与驱动 crate） | `packages/driver-api/src/**` 与 `packages/drivers/<id>/src/**` 内联 `#[cfg(test)]` | `cargo test -p datazen-driver-api --lib`；`cargo test -p datazen-driver-<id>` | 五个新 trait 方法的**默认实现**及其等价性、`CellWrite` 的 serde 三态、方言覆盖实现（MySQL 的 `INSERT INTO t () VALUES ()`、PG/SQLite 的 `RETURNING`）、`CountStrategy` 推导、键集谓词边界 | Host IPC、React 渲染、跨库 UI 旅程 |
| **Rust 集成**（驱动） | `packages/drivers/<id>/tests/` | `cargo test -p datazen-driver-<id> --tests`（CI 对 basic 四驱动正用这条） | 需要真实/内存数据库的契约（`real_driver_contract.rs` 一类）、跨 schema/跨库、`use_database`、DDL 往返、迁移旅程 | 单个纯函数的句式断言（那是内联单测） |
| **Host 前端单测** | `src/**/__tests__/**` | `pnpm test:unit`（basic）或 `pnpm test:unit:driver-set`（默认 all，且自带 codegen 重铺） | store 动作与竞态（`requestRevision` / `loadingRevision`）、`pendingChanges` 纯函数、`valuesEqual` 语义、`DataTable` 渲染与回调契约、journey test | 真实排版/几何、真实剪贴板、原生对话框、真实驱动 SQL |
| **驱动 UI 单测** | `packages/drivers/<id>/ui/__tests__/` | `pnpm test:unit:drivers` | 只属于某一个驱动的 UI、方言 profile、驱动专属 Command 的 invoke 形状、驱动 locale pack 注册 | 宿主通用网格行为（禁止放这里，见 §3.2 反向） |
| **E2E（Host）** | `e2e/specs/`（用户旅程在 `e2e/specs/journeys/`） | `pnpm e2e` / `pnpm e2e:smoke` / `pnpm e2e:db` / `pnpm e2e:journeys` | 真机上才成立的交互（键盘输入到受控 input、双击、拖拽、真实剪贴板、真实对话框）、IPC 端到端、跨窗口 | 单个分支的穷举（那该在单测里）、驱动方言深水区 |
| **E2E（驱动）** | `packages/drivers/<id>/e2e/` | 该 crate 的显式脚本（如 `pnpm e2e:redis`），**不进**默认 `pnpm e2e` | 某一驱动的方言/类型/专属 UI 深水区 | 宿主通用路径 |
| **契约矩阵** | `e2e/contract/`（纯逻辑）+ `e2e/specs/host-contract-matrix.ts`（WDIO 入口） | `pnpm e2e:contract:matrix`；纯逻辑单测 `pnpm test:unit:e2e-contract:coverage` | 同一套 Host journey 在 PG / MySQL / SQLite 上各跑一遍，验证「不因驱动切换而回归」 | 某一驱动的专有语法与专有对象 |

> `vitest.config.ts` 与 `vitest.drivers.config.ts` 的 `include` 是两条互不重叠的 glob：宿主套件**不收集** `packages/drivers`，驱动套件**不收集** `src/**`。所以「我在 Host 里写了一条驱动 UI 测试」既不会在 `pnpm test:unit` 里跑到，也不会在 `pnpm test:unit:drivers` 里跑到 —— 它等于没写。

### 3.2 驱动专属测试禁止放 Host（本批方案的高频违规点）

**规则**：凡只验证某一个驱动实现、SQL/KV 方言、专属 UI 或 Driver Command 的用例，写到该驱动目录，**禁止放到 Host**。

数据浏览这一批方案会大量触发这条规则，因为 DB-04 / DB-08 / DB-09 / DB-18 都新增了逐驱动可覆盖的 trait 方法。判断方法：**把驱动名换成另一个驱动，这条用例还成立吗？**

- 换成别的驱动也成立 → 它测的是宿主或 `driver-api` 的默认实现 → 放 Host 或 `packages/driver-api`。
- 只有一个驱动才成立 → 放 `packages/drivers/<id>/`。

具体分派：

| 想测的东西 | 正确落点 | 命令 |
| --- | --- | --- |
| `build_insert_sql` 的**默认实现**（`Unset` 不进列清单、全 `Unset` 走 `DEFAULT VALUES`、`insert_returning_clause` 默认 `None`） | `packages/driver-api/src/` 内联单测 | `cargo test -p datazen-driver-api --lib` |
| MySQL 覆盖成 `INSERT INTO t () VALUES ()` | `packages/drivers/mysql/src/` 内联单测 | `cargo test -p datazen-driver-mysql` |
| PostgreSQL / SQLite 的 `RETURNING` 子句 | 各自 crate 内联单测 | `cargo test -p datazen-driver-postgres`（SQLite 同理） |
| `count_strategy()` 由 `skip_count_query()` 推出 | `packages/driver-api` 内联 + Host `query_executor` 侧消费 | 两条都要跑 |
| 某驱动的 `filter_operator_sql` 方言渲染（`Regex` / `JsonContains`） | 该驱动 crate 内联单测 | `cargo test -p datazen-driver-<id>` |
| 网格的选区跃迁、剪贴板、dirty 高亮 | Host 前端单测 / journey | `pnpm test:unit:driver-set` |
| 「跨 PG/MySQL/SQLite 都能筛选」 | 契约矩阵 | `pnpm e2e:contract:matrix` |

**驱动 crate 的一个好消息**：`Cargo.toml` 的 workspace `members` 含 `packages/drivers/*`（只 `exclude` `kiwi` / `olap` / `superset`），所以 `cargo test -p datazen-driver-sqlserver` 这类命令**不依赖当前驱动集选择**，随时可跑。驱动集选择只影响 `src-tauri` 的 feature 注入与前端 codegen（§6.3）。

### 3.3 一条改动的落点决策树

```text
这个行为只在某一个数据库上成立吗？
├─ 是 ── 它需要真实数据库吗？
│        ├─ 是 → packages/drivers/<id>/tests/（cargo test -p datazen-driver-<id> --tests）
│        └─ 否 → packages/drivers/<id>/src/ 内联 #[cfg(test)]（cargo test -p datazen-driver-<id>）
└─ 否 ── 它是 Rust 侧的吗？
         ├─ 是 → 是 Host 的还是 driver-api 的？
         │        ├─ Host（IPC、SQL 拼装、计划、校验）→ src-tauri/src/** 内联（cargo test -p datazen --lib）
         │        └─ driver-api（trait 默认实现、DTO serde）→ packages/driver-api/src/** 内联
         └─ 否 ── 是「连续击键 / 连续鼠标 / 剪贴板」这类多步交互吗？
                  ├─ 是 → 先写 journey test（src/**/__tests__/**，§5），需要真机保真度再加 E2E
                  └─ 否 → 单测（src/**/__tests__/**）
```

### 3.4 契约矩阵怎么扩展（DB-04 / DB-07 / DB-08 / DB-09 / DB-14 / DB-18 会用到）

契约矩阵不是「再写一个 spec」，而是把**纯逻辑**和**WDIO 执行**分开的两层。新增一条 journey 的完整步骤：

| 步 | 改哪个文件 | 加什么 |
| --- | --- | --- |
| 1 | `e2e/contract/fixtures.ts` | 给 `HostContractJourneyId` 加新的 journey id；给 `JOURNEY_REQUIREMENTS` 加该 journey 需要的能力标志（`DriverCapabilities` 的字段）；需要新种子表时加 `DialectSeedHelpers` 的成员与三种方言实现，表名前缀沿用 `_e2e_hc_pg_` / `_e2e_hc_my_` / `_e2e_hc_lt_` |
| 2 | `e2e/contract/journeys/plan.ts` | 把新 id 加进 `ALL_CONTRACT_JOURNEYS`；若它属于「核心数据旅程」，也加进 `F2_CORE_JOURNEYS`；新增的纯断言辅助函数写在这里（例如照 `bodyContainsAll` / `paginationRangeVisible` 的形状） |
| 3 | `e2e/contract/journeys/run-*.ts` | 新 runner 放**新文件**并沿用 `run-` 前缀命名：`vitest.e2e-contract.config.ts` 的 `coverage.exclude` 明确排除了 `e2e/contract/journeys/run-*.ts`，runner 不是纯逻辑、不该被覆盖率门槛绑住 |
| 4 | `e2e/specs/host-contract-matrix.ts` | 把新 runner 接进矩阵循环（该 spec 是 `wdio.conf.ts` 的 `contract` suite 唯一成员） |
| 5 | `e2e/contract/__tests__/` | 给第 1、2 步的纯逻辑补单测 —— 只有 `e2e/contract/fixtures.ts` 与 `e2e/contract/journeys/plan.ts` 被覆盖率门槛覆盖（`lines 80 / statements 80 / functions 80 / branches 70`） |
| 6 | 跑门禁 | `pnpm test:unit:e2e-contract:coverage`（纯逻辑）+ `pnpm e2e:contract:matrix`（真机矩阵，需 webdriver 构建） |

**矩阵层的三条纪律**：

1. **journey 不得断言方言文本**。`fixtures.ts` 的文件头写明：那里的方言 SQL 只用于灌种子，journey 不许断方言专属字符串 —— 要断就断 Host 侧的可观察结果。
2. **按能力跳过，而不是按驱动名跳过**。用 `journeyAllowed(fixture, journey)`；跳过时必须能从 `skipReason(fixture, journey)` 读出「缺哪个能力」，这样矩阵报告里「skip」是可解释的，不是静默的。
3. **种子绑显式会话**。`e2e/contract/journeys/seed.ts` 通过 IPC `connectConfig` 解析出**该 fixture 自己的** `dbSessionId`，不依赖 UI 的「当前活动标签」；数据浏览新增 journey 的灌数也必须照此，否则会串到另一个 fixture 的会话上。

---

## 4. 通用测试约定

### 4.1 命名规范

| 对象 | 规范 | 示例 |
| --- | --- | --- |
| 测试文件（宿主） | `*.test.ts` / `*.test.tsx`，与被测文件同名放 `__tests__/` | `src/stores/tableData/__tests__/gridSelection.test.ts` |
| **journey 测试文件** | 文件名含 `journey`（本仓既有三种写法 `xxxJourney.test.tsx` / `xxx.journey.test.ts` / `xxxJourney.test.ts`，数据浏览**统一用** `<主题>Journey.test.tsx`） | `src/components/DataTable/__tests__/gridSelectionJourney.test.tsx` |
| 驱动 UI 测试 | `packages/drivers/<id>/ui/__tests__/*.test.ts(x)` | `packages/drivers/mysql/ui/__tests__/insertSql.test.ts` |
| Rust 集成测试 | `packages/drivers/<id>/tests/<领域>_<主题>.rs` | `packages/drivers/postgres/tests/insert_returning.rs` |
| E2E spec（Host） | `e2e/specs/<领域>-<主题>.ts`；用户旅程放 `e2e/specs/journeys/` | `e2e/specs/table-insert-row.ts`、`e2e/specs/journeys/grid-selection-journey.ts` |
| `describe` | 用被测单元名或方案号 + 主题，**中文可读**（本仓既有 spec 用中文题目，如 `describe('表数据编辑 (DE-002~DE-005)')`） | `describe('DB-01 单元格区域选择')` |
| `it` | 一句话描述**可观察行为**，不写「should work」；建议在末尾标方案号 | `it('Shift+方向键扩区后 data-dt-selected 覆盖整块 (DB-01)')` |

### 4.2 测试 id 规范

**总原则（对齐 `00-contracts.md` 第 1.2 / 4.4 节）**：测试通过 `data-*` 属性绑定标识，**禁止依赖视口几何坐标反查**，也**禁止按可见文案定位**。

三层锚点，按优先级使用：

1. **契约 `data-*` 属性（首选，单测 + E2E 都可用，无条件渲染）**
   - 既有：`data-dt-row`（行下标）、`data-dt-col`（**列名**）、`data-col-header`（列名）、`data-testid="data-table-cell"`。
   - 契约新增（分册实现）：`data-dt-selected`、`data-dt-dirty`、`data-dt-editing`、`data-dt-cell-type`。值一律是字符串 `"true"` 或**属性不存在**（不要用 `"false"`，否则 `.getAttribute()` 与 CSS 选择器 `[data-dt-selected]` 的语义会分叉）。
   - **列名不是列下标**：`CellCoord.columnName` 与 `data-dt-col` 都用列名，因为 `visibleColumns` 一变，列下标就失去稳定含义。
2. **`tid()` 生成的 `data-testid`（E2E 首选，单测需 stubEnv）**
   - 新 UI 一律写 `{...tid('grid-insert-row-button')}` 这类，命名 `<area>-<element>-<action>`。
   - Host 单测里 `tid()` 返回 `{}`（见 §2.1），要按 `data-testid` 定位必须 `vi.stubEnv('VITE_E2E', '1')`；能改用第 1 层就不用它。
3. **role / i18n key（兜底）**
   - 需要断言「可访问名称」本身就来自字典时，用 `enCopy(key)` 回读（`src/test/enCopy.ts`），**禁止裸写 `en[key]`**。
   - 驱动 UI 测试统一 `useI18n: () => ({ t: (key) => key })`，之后可以 `getByText('query.filter.apply')` 这类 key 定位。

**错误前缀也是一条契约锚点**（`00-contracts.md` 第 8 节）：`CommandError` 序列化到 IPC 之后是**脱敏后的字符串，没有机器可读的 `code` 字段**（`src-tauri/src/commands/error.rs` 的 `Serialize` 实现先脱敏再 `serialize_str`，其内联测试也钉住了「错误类别必须保留在文本里」）。因此新增的、需要前端精确识别的错误，消息以 `grid.<域>.<原因>: <说明>` **前缀**开头，前端用 `classifyGridError(message)`（建议位置 `src/lib/gridErrors.ts`）做前缀匹配，匹配不到返回 `'unknown'` 并**回退显示原始消息**。

- 为什么是前缀而不是 `includes`：前缀是唯一在两侧都能逐字断言的形态；包含匹配会因为消息正文里出现同样的词而误判。
- 现成范式：`src/lib/connectionShareError.ts` 的 `translateConnectionShareError`（`trimmed.startsWith(...)` 逐条前缀匹配，匹配不到 `return trimmed`），对应测试 `src/lib/__tests__/connectionShareError.test.ts` 覆盖「已知映射 / 未知透传 / 空消息 fallback」三类。**`gridErrors.ts` 照它写，测试照它写。**

### 4.3 断言风格

- **断言可观察结果，不断言实现细节**：断 DOM 属性、store 里读得出来的字段、IPC 收到的参数形状；不要断内部私有变量、不要断组件内部 `useState` 的中间值。
- **断「恰好」而不是「存在」**：`toContain` 很容易命中别处（已知假守卫形态）。数量与身份用 `toEqual([...])` / `toHaveLength(n)` / `toBe(x)`。
- **两种断言的取舍**：
  - 精确断言（`toEqual` / `toBe`）用于：错误前缀、`data-dt-*` 取值、序列化产物（TSV / SQL 模板）、store 字段。
  - 子串断言（`toContain`）只在「内容本身是长文本且我断的是其中一段数据」时用，且必须同时断言**唯一性**（例如 `split(...).length` 或先 `querySelectorAll` 再取 `length`）。
- **异步用 `await`，不要漏**：`fireEvent` 是同步的，但 React 18 的状态更新可能被批处理，涉及 store 订阅后再渲染的断言要用 `await waitFor(() => ...)` 或 `await screen.findByX()`。
- **文案零耦合**：见 §1 D7 与 §4.2 第 3 层。

### 4.4 mock 与真实依赖的边界

**只 mock 跨进程、无可信实现或环境不提供的东西；有真实实现的就接真的。**

| 依赖 | 处理 | 理由 / 先例 |
| --- | --- | --- |
| Tauri IPC（`src/commands/**`） | **mock**（`vi.mock('../../commands/database', ...)`） | jsdom 里没有 Tauri；`src/stores/__tests__/tableDataStore.test.ts` 就这么做 |
| `schemaStore` / `settingsStore` / 驱动 SDK 桥 | 用真实 store（`create(...)`），或按驱动 UI 套件的 `bindSettingsStore` / `bindConnectionStore` 注入 | 驱动 UI 测试的既有范式；宿主侧优先用真实 store |
| `@tanstack/react-virtual` / `useVirtualTable` | **必须 mock**（宿主用 `vi.mock('../../../hooks/useVirtualTable', ...)`；驱动 UI 套件已在 `src/test/driverUiSetup.ts` 全局 stub） | jsdom 没有排版引擎，真实虚拟化会渲染 0 行 —— 这是 JSDOM 的能力边界（原则四） |
| `navigator.clipboard` | **stub**（`Object.defineProperty(navigator, 'clipboard', { value: {...}, configurable: true })`） | jsdom 不实现系统剪贴板；先例见 `src/windows/settings/__tests__/sqlSnippetsLifecycleJourney.test.tsx` 的 `setupClipboardMock` |
| `useI18n` | mock 成 `{ t: (key) => key }`，或走真实注册表 + `enCopy` | 前者让断言与文案解耦；后者用于「名称确实来自 `t()`」的接线用例 |
| 真实 SQL 拼装（`QueryExecutor::build_*`） | **不 mock**，真调 | 纯函数、无 IO，mock 掉就失去了本批方案最需要保护的东西 |
| 真实数据库 | 单测不接；跑库的 Rust 集成与 E2E 才接 | `packages/drivers/<id>/tests/` 与 `e2e/` 各自准备 |
| `panic!` 策略 | 测试里允许裸 `unwrap()` / `expect()`（`#[cfg(test)]`、`#[test]`、测试辅助函数豁免，`docs/development/panic-policy.md` 第 2 条） | 但**生产路径**新增代码不得借测试豁免混进去；评审看的是非 `#[cfg(test)]` 的 diff |

**mock 的纪律**：

- 用 `vi.hoisted()` 提升 mock 变量（`vi.mock` 声明会被提升到文件顶部，普通 `const` 引用会命中未初始化）。先例：`src/components/DataTable/__tests__/DataTable.test.tsx`。
- mock 只实现子集时，用 `as unknown as X` / `satisfies` / `Pick<>` **显式**声明子集，**禁止 `any`**；也禁止为了类型通过而删断言。
- 不要在 mock 里复刻被测逻辑（那会让测试恒真）。mock 的返回值要**固定**，断言点在被测代码如何消费它。

### 4.5 测试间互不污染

**铁律：每个用例自建数据；禁止共享可变状态。**

- **夹具工厂而非夹具常量**：需要可变对象（`Map` / `Set` / 数组 / store）时，在 `beforeEach` 或用例内**新建**。模块顶层写 `const ROWS = [...]` 只对**只读**数据成立；一旦某用例 `push` 或改 `Map`，它就污染了后面所有用例，而且失败顺序会随执行顺序漂移。
- **store 显式重置**：`useTableDataStore` 有 `reset()`；`beforeEach` 里调它，或直接把 `byPanel` 设成新的 `Map`。驱动 UI 的 bridge store 用 `create(...)` 每次重建。
- **DOM 清理**：`afterEach(cleanup)`（本仓既有 DataTable 套件都写）。忘记清理时 `getByText` 会撞到上一个用例的残留节点，报「found multiple elements」。
- **环境变量清理**：用了 `vi.stubEnv` 就必须 `afterEach(() => vi.unstubAllEnvs())`，先例见 `packages/ui/src/__tests__/tid.test.ts`。
- **模块级副作用**：i18n 注册表、驱动注册是全局的。驱动 UI 套件靠 `src/test/driverUiSetup.ts` 统一装配；宿主用例不要自己再 `registerLocale` 一堆（除非该用例测的就是注册链）。
- **真实数据库 / 真实文件**：用带前缀的可丢弃对象名（既有 `_e2e_*` 与契约矩阵的 `_e2e_hc_*`），`after` 里 best-effort 清理；**不要**依赖「上一次运行留下的表还在」。
- **禁止 `it.only` / `describe.only` 提交**（会让整套门禁静默缩水），也不要用 `it.skip` 掩盖失败 —— 需要跳过就写明原因并以「已知不可验证」记录在分册第 11 节。

### 4.6 异步等待的正确写法

**禁止 `sleep` / `browser.pause` 作为唯一的同步手段**（E2E 里 `browser.pause` 仅用于驱动已确认抓不到的条件变化，且必须在注释里说明为什么可断言的等待不适用）。

| 场景 | 正确写法 |
| --- | --- |
| 等 React 重渲染后的 DOM | `await waitFor(() => expect(...))` 或 `await screen.findByTestId(...)` |
| 等 store 状态落到某值 | `await waitFor(() => expect(useTableDataStore.getState().byPanel.get(PANEL)?.pendingChanges.size).toBe(1))` |
| 等一个 Promise 被 resolve（测在途态） | 用 `deferred<T>()` 手工控制（先例 `src/stores/__tests__/tableDataStore.test.ts`），先断言 `loading === true` 与 `loadingRevision`，再 `resolve()`，再 `await waitFor(...)` |
| 等节流 / 去抖 | 用 `vi.useFakeTimers()` + `vi.advanceTimersByTime(...)`，不要在两者之间加真实睡眠 |
| E2E 等元素 | `await $('[data-testid="..."]').waitForDisplayed({ timeout })`、`waitUntil(async () => ...)`，并在 `timeoutMsg` 里写清在等什么（先例 `e2e/specs/table-edit.ts` 的「等待回滚后的表数据重新加载」） |
| E2E 等状态属性 | `browser.waitUntil(async () => (await el.getAttribute('aria-busy')) !== 'true')` —— **等可断言的属性**，不是等时间 |

**竞态防护必须复用既有机制**：`TableState.requestRevision` / `loadingRevision` 是本项目既有的在途请求版本号。任何新增异步取数（行数估计、外键候选、键集翻页）都要走同一套判定；测试要**显式**构造「旧请求后返回」的场景（`deferred()` 两个 Promise、先 resolve 新的再 resolve 旧的），断言旧结果被丢弃。绕开版本号直接写 `rows` 的实现，测试必须把它判红。

---

## 5. 连续旅程测试（Journey Test）模板

### 5.1 为什么必须有

依据 [interaction-and-testing-principles.md](../../development/interaction-and-testing-principles.md) 原则三：交互型功能**禁止只写单个静态字符串的断言**。用户的输入 90% 以上的时间处于**残缺中间态**（`SELECT * FROM `、刚点开筛选面板还没填值、粘贴了一行半截 TSV）。静态切片测试会给出「测试通过率陷阱」：全绿，而真机一敲就断。

因此数据浏览的每一个交互型分册（01 / 02 / 03 / 04 / 05 / 06 / 07 / 08 / 10 / 11）**至少交付 1 条 journey test**，且必须覆盖：

1. **进入条件**：什么事件让状态生效；
2. **状态内行为**：状态里的连续动作与它们各自的断言；
3. **退出跃迁**：什么事件结束该状态、交出控制权，且**上一个状态不产生残留抑制**（例如退出筛选面板后 Quick Filter 仍然可用）；
4. **残缺中间态**：至少 2 个不完整输入（半截 TSV、空筛选值、编辑中的未提交值）；
5. **误伤排查**：至少 1 个「看似相似但合法」的邻近场景仍然工作（对应三维影响度自查的维度 2）。

### 5.2 可直接抄改的骨架（组件级）

> 说明：下面这条骨架钉的是 **DB-01（选区）+ DB-02（键盘）+ DB-03（剪贴板）+ DB-06（dirty）** 的联合旅程。标注 `⚠ 契约新增` 的行依赖分册 01/06 落地后的产物，因此**在 01 合入前它不会绿** —— 这是刻意的：先钉契约属性名与状态跃迁，再等实现补齐（红→绿的顺序比反过来可信）。分册落地后，把这些行替换成该分册导出的动作，其余结构（夹具、mock 边界、逐步断言）不用改。
>
> 路径按「测试文件位于 `src/components/DataTable/__tests__/`」写；换位置时同步调整 import 深度。

```tsx
/**
 * 数据浏览联合旅程：选中 → 键盘扩区 → 剪贴板复制 → Escape 退出 → 暂存改动与回滚。
 *
 * 覆盖状态机而非静态快照（interaction-and-testing-principles 原则三）：
 *   Step 1 进入：单击单元格 ⇒ mode: none → cell，`data-dt-selected` 恰好覆盖 1 格
 *   Step 2 残缺中间态：第二次单击（未按修饰键）⇒ 旧区域被替换，不是叠加
 *   Step 3 驻留行为：Shift+方向键逐格扩区，`data-dt-selected` 集合严格单增
 *   Step 4 驻留行为：Mod+C ⇒ 剪贴板载荷是 TSV，且只含选中格
 *   Step 5 残缺中间态：粘贴「一行半截 TSV」（列数不足）⇒ 只写它覆盖到的列，不越界
 *   Step 6 退出跃迁：Escape ⇒ mode: none，选区清空，且**下一个状态不受残留抑制**
 *   Step 7 互斥跃迁：点行号槽 ⇒ mode: row，`selectedRows` 生效且选区已清空
 *   Step 8 暂存与回滚：编辑一格 ⇒ `data-dt-dirty` 出现 ⇒ 回滚 ⇒ 消失
 *
 * 定位口径：一律 `data-dt-row` / `data-dt-col` / `data-dt-*`，禁止几何坐标与可见文案。
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';

import { DataTable } from '../DataTable';
import { useTableDataStore } from '../../../stores/tableDataStore';
// ⚠ 契约新增：00-contracts 第 4.1 节定义的选区模型，由分册 01 交付。
//   `GridSelection` / `CellCoord` / `CellRange` 的类型定义也在该文件里。
import { normalizeCellRange } from '../../../stores/tableData/gridSelection';

// ── mock 边界：只 mock「jsdom / Tauri 不提供」的东西 ──────────────────────
// `vi.mock` 会被提升到文件顶部，因此 mock 里用到的变量必须先 hoist。
const { getTableData } = vi.hoisted(() => ({ getTableData: vi.fn() }));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

// jsdom 没有排版引擎，真实 useVirtualTable 会渲染 0 行（JSDOM 能力边界）。
vi.mock('../../../hooks/useVirtualTable', () => ({
  useVirtualTable: ({ rows }: { rows: unknown[][] }) => ({
    virtualRows: rows.map((_, index) => ({ index, key: String(index), start: index * 40 })),
    totalHeight: rows.length * 40,
  }),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getTableData: (...args: unknown[]) => getTableData(...args),
    previewPendingChanges: vi.fn(),
    commitPendingChanges: vi.fn(),
  },
}));

// ── 夹具：每个用例自建，模块级只放不可变常量 ─────────────────────────────
const PANEL = 'panel-journey-grid';

const COLUMNS = [
  { id: 'id', name: 'id', type: 'integer' },
  { id: 'name', name: 'name', type: 'varchar' },
  { id: 'note', name: 'note', type: 'text' },
];

/** 只读夹具：含 NULL、多字节与 emoji（多字节必须进夹具，见 §6.1）。 */
const ROWS: unknown[][] = [
  [1, 'Alice', 'first'],
  [2, 'Bob', null],
  [3, '中文🙂', 'multi\nline'],
];

const RESPONSE = {
  columns: [
    { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
    { name: 'name', dataType: 'varchar', isPrimaryKey: false, isNullable: false },
    { name: 'note', dataType: 'text', isPrimaryKey: false, isNullable: true },
  ],
  rows: ROWS,
  totalRows: ROWS.length,
  page: 0,
  pageSize: 50,
};

/** 剪贴板探针：jsdom 无系统剪贴板，用可读回的 stub。 */
let clipboardBuffer = '';
function stubClipboard() {
  clipboardBuffer = '';
  Object.defineProperty(navigator, 'clipboard', {
    value: {
      writeText: vi.fn((text: string) => {
        clipboardBuffer = text;
        return Promise.resolve();
      }),
      readText: vi.fn(() => Promise.resolve(clipboardBuffer)),
    },
    configurable: true,
  });
}

/**
 * 组件级 harness：把 store 的真实动作接到 DataTable 的回调上。
 *
 * 这里刻意**不 mock store** —— 旅程要断言的就是状态跃迁，把 store 换掉等于
 * 把被测对象换掉。
 *
 * ⚠ 契约新增：`DataTableProps` 目前**没有**单元格单击 / 选区回调（只有整行的
 *   `onRowClick` / `onRowSelect`），分册 01 会新增一个（形如 `onCellClick` /
 *   `selection` + `onSelectionChange`）。本骨架先不写那一行 —— 落地后把它接上，
 *   Step 1 / 2 / 6 / 7 的断言一个都不用改。
 */
function GridHarness() {
  const panelState = useTableDataStore((s) => s.byPanel.get(PANEL));

  return (
    <DataTable
      columns={COLUMNS}
      rows={ROWS}
      totalRows={ROWS.length}
      editingCell={panelState?.editingCell ?? null}
      selectedRows={panelState?.selectedRows ?? new Set<number>()}
      onRowSelect={(index, opts) => useTableDataStore.getState().selectRow(PANEL, index, opts)}
      onCellDoubleClick={(row, col) =>
        useTableDataStore.getState().startEdit(PANEL, row, col)
      }
      onCellEdit={(row, col, value) =>
        useTableDataStore.getState().stageCellChange(PANEL, row, col, value)
      }
      onCellEditCancel={() => useTableDataStore.getState().cancelEdit(PANEL)}
      enableSetNull
    />
  );
}

/** 按 `(行下标, 列名)` 取单元格 —— 数据属性反查，不是几何坐标。 */
function cell(row: number, col: string): HTMLElement {
  const el = document.querySelector<HTMLElement>(
    `[data-dt-row="${row}"][data-dt-col="${col}"]`,
  );
  if (!el) throw new Error(`cell not rendered: row=${row} col=${col}`);
  return el;
}

/** 当前 `data-dt-selected="true"` 的格子集合，按 `row:col` 归一化成可断言的数组。 */
function selectedCells(): string[] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-dt-selected="true"]'))
    .map((el) => `${el.getAttribute('data-dt-row')}:${el.getAttribute('data-dt-col')}`)
    .sort();
}

function dirtyCells(): string[] {
  return Array.from(document.querySelectorAll<HTMLElement>('[data-dt-dirty="true"]'))
    .map((el) => `${el.getAttribute('data-dt-row')}:${el.getAttribute('data-dt-col')}`)
    .sort();
}

beforeEach(async () => {
  // 每个用例自建数据：先清空 store，再灌入本用例的响应。
  useTableDataStore.getState().reset();
  getTableData.mockReset().mockResolvedValue(RESPONSE);
  stubClipboard();

  await useTableDataStore.getState().loadTableData({
    panelId: PANEL,
    connectionId: 'conn-journey',
    dbSessionId: 'sess-journey',
    driverType: 'postgres',
    database: 'app',
    schema: null,
    table: 'journey_table',
    page: 0,
    pageSize: 50,
  });
});

afterEach(() => {
  cleanup();
  useTableDataStore.getState().reset();
  vi.unstubAllEnvs();
});

describe('DB-01/02/03/06 数据浏览连续旅程', () => {
  it('选区 → 键盘扩区 → 复制 → Escape 退出 → 暂存回滚，全程无残留抑制', async () => {
    render(<GridHarness />);

    // ── Step 1 进入条件：单击单元格 ⇒ none → cell ──────────────────────
    fireEvent.mouseDown(cell(0, 'name'));
    fireEvent.click(cell(0, 'name'));
    await waitFor(() => expect(selectedCells()).toEqual(['0:name']));

    // ── Step 2 残缺中间态：不做修饰的第二次单击替换而不是叠加 ──────────
    fireEvent.mouseDown(cell(1, 'note'));
    fireEvent.click(cell(1, 'note'));
    await waitFor(() => expect(selectedCells()).toEqual(['1:note']));

    // ── Step 3 驻留行为：Shift+方向键逐格扩区 ─────────────────────────
    fireEvent.keyDown(cell(1, 'note'), { key: 'ArrowUp', shiftKey: true });
    await waitFor(() => expect(selectedCells()).toEqual(['0:note', '1:note']));
    fireEvent.keyDown(cell(0, 'note'), { key: 'ArrowLeft', shiftKey: true });
    await waitFor(() => expect(selectedCells()).toEqual(['0:name', '0:note', '1:name', '1:note']));

    // 区域必须经 normalizeCellRange 归一化：列序按 visibleColumns 而不是字典序。
    const normalized = normalizeCellRange({
      start: { rowIndex: 1, columnName: 'note' },
      end: { rowIndex: 0, columnName: 'name' },
    });
    expect(normalized.start.rowIndex).toBeLessThanOrEqual(normalized.end.rowIndex);

    // ── Step 4 驻留行为：Mod+C 载荷是 TSV，且只含选中格 ────────────────
    const grid = document.querySelector<HTMLElement>('[data-testid="data-table-cell"]')!;
    fireEvent.keyDown(grid, { key: 'c', metaKey: true });
    await waitFor(() => expect(navigator.clipboard.writeText).toHaveBeenCalled());
    expect(clipboardBuffer.split('\n')).toHaveLength(2); // 两行
    expect(clipboardBuffer.split('\n')[0].split('\t')).toHaveLength(2); // 每行两列
    expect(clipboardBuffer).not.toContain('first'); // 未选中的列不得混入

    // ── Step 5 残缺中间态：粘贴「一行半截 TSV」⇒ 只写覆盖到的列 ────────
    fireEvent.paste(cell(0, 'name'), {
      clipboardData: { getData: (type: string) => (type === 'text/plain' ? 'Zoe' : '') },
    });
    await waitFor(() =>
      expect(useTableDataStore.getState().byPanel.get(PANEL)?.pendingChanges.size).toBe(1),
    );
    await waitFor(() => expect(dirtyCells()).toEqual(['0:name']));

    // ── Step 6 退出跃迁：Escape ⇒ mode: none，选区清空 ────────────────
    fireEvent.keyDown(cell(0, 'name'), { key: 'Escape' });
    await waitFor(() => expect(selectedCells()).toEqual([]));

    // 退出后不得有残留抑制：紧接着的键盘操作必须重新生效。
    fireEvent.mouseDown(cell(2, 'name'));
    fireEvent.click(cell(2, 'name'));
    await waitFor(() => expect(selectedCells()).toEqual(['2:name']));

    // ── Step 7 互斥跃迁：点行号槽 ⇒ mode: row，且选区清空 ─────────────
    const rowButton = document.querySelectorAll<HTMLButtonElement>(
      'button[title="dataTable.selectRow"]',
    )[0]!;
    fireEvent.click(rowButton);
    await waitFor(() =>
      expect([...useTableDataStore.getState().byPanel.get(PANEL)!.selectedRows]).toEqual([0]),
    );
    await waitFor(() => expect(selectedCells()).toEqual([]));

    // ── Step 8 暂存 → dirty → 回滚 → dirty 消失 ─────────────────────
    fireEvent.doubleClick(cell(2, 'name'));
    await waitFor(() => expect(useTableDataStore.getState().byPanel.get(PANEL)?.editingCell).not.toBeNull());
    const editor = await screen.findByTestId('table-edit-input');
    fireEvent.change(editor, { target: { value: 'Carol' } });
    fireEvent.keyDown(editor, { key: 'Enter' });
    await waitFor(() => expect(dirtyCells()).toEqual(['2:name']));

    useTableDataStore.getState().rollbackPendingChanges(PANEL);
    await waitFor(() => expect(dirtyCells()).toEqual([]));
    expect(useTableDataStore.getState().byPanel.get(PANEL)?.pendingChanges.size).toBe(0);
  });
});
```

> ⚠ 抄用时保留结构、替换契约新增部分：`GridHarness` 里标注 `⚠ 契约新增` 的回调、`selectedCells()` / `dirtyCells()` 依赖的四个 `data-dt-*` 属性、以及 `src/stores/tableData/gridSelection.ts` 的 `normalizeCellRange`，都要等分册 01 交付后才存在。**不要**为了让它现在变绿而把这些断言删掉或改成 mock 断言 —— 那时测的就不是状态跃迁了。

### 5.3 每条 journey 必须交代的三段

写 journey 时把这三段写进用例注释，评审按它逐条对：

| 段 | 内容 | 至少覆盖 |
| --- | --- | --- |
| **进入条件** | 什么事件让状态生效 | 1 个进入事件 + 1 个「不该进入」的邻近事件（例如编辑器聚焦时按 Delete **不该**删行） |
| **状态内行为** | 状态里的连续动作与断言 | ≥3 步连续动作，每步断言状态属性取值 |
| **退出跃迁** | 什么事件结束状态、交出控制权 | 1 个显式退出（Escape / 提交 / 取消）+ 1 个**残留抑制检查**（退出后紧邻的正常操作必须成功） |

### 5.4 第二类 journey 的步骤清单（编辑与提交）

DB-04 / DB-05 的旅程按同样结构写，步骤固定为：

1. 进入编辑（双击 / F2 / Enter）；
2. 残缺中间态 A：输入到一半按 Escape ⇒ 值未提交、`data-dt-dirty` 未出现；
3. 进入编辑、输入完整值并提交 ⇒ `data-dt-dirty` 出现，`pendingChanges` 里 `changedColumns` 只含该列；
4. 残缺中间态 B：把值改回原值 ⇒ **不得**算改动（复用 `valuesEqual`，不得用 `===` 或 `JSON.stringify` 另写一份）；
5. 提交前一格、留一格未提交 ⇒ 预览计划只含已改的列；
6. 预览与提交之间改动暂存 ⇒ 指纹不匹配必须拒绝提交（`grid.commit.stalePlan`），**禁止**降级为「按新数据重算」；
7. 新增行（DB-04）：只填部分列 ⇒ 未填的列必须是 `Unset`（不出现在语句里），自增列不得被补 `NULL`；
8. 回滚 ⇒ `data-dt-dirty` 全部消失，`pendingChanges` 为空。

---

## 6. 测试数据与夹具

### 6.1 必须覆盖的数据形态（每一类都要有对应用例）

| 形态 | 为什么必须 | 放在哪一类测试 |
| --- | --- | --- |
| **无主键表** | `build_select_sql` 现状会退化成「第一列排序」，是 DB-09 / DB-18 的根因；写入路径也不得拿第一列当行身份 | Host Rust 单测（SQL 拼装）+ Host 前端单测（默认排序展示）+ 契约矩阵 |
| **复合主键表** | 行身份是原值快照字典，复合主键必须两个列都进 `row_identity`；漏一列会命中多行 | Host Rust 单测（`buildRowIdentity` / `duplicateRowIdentityKeys`）+ E2E |
| **含 NULL 的列** | 三态语义的核心：`Null` 与 `Unset` 必须可区分；`original_value` 为 NULL 时行身份仍然要能定位 | 全部层级（这是本批方案最容易错的一处） |
| **超长文本**（≥ 64KB 单值） | 单元格渲染截断、剪贴板/导出不被截、SQL 生成不因为长度爆栈 | Host 前端单测（渲染与复制）+ Host Rust 单测（参数摘要） |
| **多字节与 emoji**（`中文🙂`，含组合字符） | 剪贴板行列切分、TSV 转义、光标定位在代理对上不能错位 | Host 前端 journey（§5 夹具已含）+ 驱动 crate 单测（`Value::String` 往返） |
| **含换行/制表符的文本** | TSV 序列化必须转义，否则粘贴时行数会多出来 | Host 前端单测（`serializeDataTableRowsAsTsv` 一类）+ E2E 复制粘贴 |
| **含外键的列** | DB-07 的候选取数与跳转；无 FK 列时**不得渲染入口** | Host 前端单测 + 契约矩阵 |
| **空表**（0 行） | 空状态文案 / 分页边界 / 行数显示（`0` vs `约 0` vs 不可用） | Host 前端单测 + `src-tauri` 单测 |
| **单行表** | 分页边界与「仅一行时不全选」之类 | Host 前端单测 |
| **只读账号 / 只读连接** | `grid.cell.readOnly` 必须给出**具体原因**；写入入口按能力**隐藏**而不是渲染一个点了报错的按钮 | Host 前端单测 + E2E（Safe Mode 下的既有用例是 `e2e/specs/table-edit.ts` 里的 DE-002b） |
| **无排序能力 / `requires_order_by` 的方言** | `PaginationSyntax.requires_order_by` + `order_by_fallback` 的兜底路径 | Host Rust 单测 + 对应驱动 crate 单测 |

### 6.2 前端单测的夹具

- **形状以 Rust 的线上形态为准**：`TableDataResult` 序列化后是 `{ columns, rows, totalRows, page, pageSize }`，`rows` 是 `Vec<Vec<Option<Value>>>` 即**位置数组**（`unknown[][]`）。而 `TableState.rows` 是**按列名索引的记录**（`Record<string, unknown>[]`）。两者不同形，转换由 `rowsToRecords` 负责 —— 夹具写错形状会让测试与真实链路脱节。
- **列定义用 `ColumnDef`**（`{ id, name, type }`），与 `VirtualBody` 的既有夹具一致；需要主键语义时用 `isPrimaryKey` 字段并同时提供 `primaryKeyColumns`（照 `src/components/DataTable/__tests__/DataTable.test.tsx` 的既有写法）。
- **store 状态用 `emptyTableState(context)` 起手**，再覆盖需要的字段。**不要**手写字面量缺字段（`TableState` 字段多，漏一个就会在类型检查里报错，或在运行期读到 `undefined`）。
- **行身份夹具**：`buildRowIdentity(columns, row)` + `rowIdentityKey(identity)` 是既有入口；不要自己拼字符串 key。
- **IPC 返回夹具**：`RowChangePlan` 至少要带 `planId` / `fingerprint` / `table` / `updates` / `deletes` / `warnings`；`commitPendingChanges` 的响应带 `planId` / `fingerprint` / `statements` / `affectedRows`。先例见 `src/stores/__tests__/tableDataStore.test.ts` 的 `samplePlan`。
- **错误前缀夹具**：`throw new Error('grid.commit.affectedMismatch: expected 1 affected row, got 3')` 这类**字符串**（真实 IPC 形态），不要 mock 成 `{ code, message }` 对象 —— 那个结构不存在（§4.2）。

### 6.3 E2E 的种子数据与驱动集

| 层 | 种子来源 | 说明 |
| --- | --- | --- |
| Host E2E 的连接与基础库 | `e2e/setup-e2e-env.sh`（幂等，会 source `e2e/.env`；`e2e/run.mjs` 也会调用），拆解在 `e2e/teardown-e2e-env.sh` | 建库、灌 `product` 之类的基础表 |
| Host E2E 的用例级表 | spec 自己的 `before`：`openQueryTab()` + `executeSQLChecked(...)` 建表灌数，`after` 里 `DROP TABLE IF EXISTS` | 表名统一带 `_e2e_` 前缀（例：`_e2e_edit_test`、`_e2e_data_view_test`）。`executeSQLChecked` 的存在理由就是「后端吞错时只表现为 20s 超时、看不出真因」 |
| 契约矩阵 | `e2e/contract/fixtures.ts` 的 `dataSeedSql` / `filterSeedSql` / `contractTableDdl`，表名由 `seedTableName(fixture, suffix)` 生成，前缀 `_e2e_hc_pg_` / `_e2e_hc_my_` / `_e2e_hc_lt_`；执行在 `e2e/contract/journeys/seed.ts` | 契约表是 `e2e_contract_*` 七个；`seed.ts` 通过 IPC `connectConfig` 解析**该 fixture 自己的** `dbSessionId`，不靠 UI 的「当前活动标签」，避免串到别的会话 |
| SQLite E2E | `e2e/create-sqlite-test-db.mjs`；契约 fixture 用 `E2E-SQLite` | —— |
| live 契约 | 由 `DATAZEN_CONTRACT_REQUIRE_LIVE=1` 把「无法验证」升级为失败（CI 注释里给出的用法） | 缺 fixture 时保持 unverified，**不要**把 unverified 当绿 |

**驱动集如何影响结果（必须写进结论，见 §7.4）**：

1. **Host 单测**：`pnpm test:unit` 只在 `basic` 语义下成立。CI 还会跑 `pnpm test:unit:driver-set`（默认 `all`）。想**完整**跑一遍 Host 套件，用：

   ```bash
   pnpm test:unit:driver-set                                  # 默认 all（全部已注册 path driver）
   pnpm test:unit:driver-set --drivers=basic                  # 显式 basic，复现本地 CI 第一步
   pnpm test:unit:driver-set --drivers=postgres,mysql         # 显式列表
   ```

   这个脚本本身就负责重新生成 `generated.ts` 与 `builtinLocales.ts`（它在文件头说明：pnpm ≥ 7 下 `pretest:unit` / `posttest:unit` 前缀钩子**不生效**，实测驱动集会原样存活，所以不能靠它们重铺）。

2. **只想重铺前端 codegen（不跑测试）**：

   ```bash
   node scripts/resolve-drivers.mjs --codegen-only --drivers=all        # 全部 path driver
   node scripts/resolve-drivers.mjs --codegen-only --drivers=sqlserver,mongodb
   node scripts/generate-builtin-locales.mjs                            # 与上一条配套
   node scripts/resolve-drivers.mjs --codegen-only --drivers=basic      # 跑完还原成 basic
   ```

   `--codegen-only` 的语义在脚本头部写明：**只写 `generated.ts` / `driver_init.rs`**，不注入 `Cargo.toml` 与 capabilities。所以要跑**真实 E2E 二进制**里的 non-basic 驱动，必须走完整注入（`pnpm tauri:build:webdriver` 内部就是 `scripts/with-driver-inject.mjs`，它先完整 resolve、跑命令、再 restore）。

3. **驱动 crate 的 Rust 测试不受驱动集影响**：`Cargo.toml` 的 workspace `members` 含 `packages/drivers/*`，所以 `cargo test -p datazen-driver-sqlserver` 直接可跑；驱动集只影响 `src-tauri` 的 feature 注入（`src-tauri/src/driver_init.rs` 里的 `#[cfg(feature = "driver-<id>")]` 分支）。

4. **E2E 驱动集**：`pnpm e2e:minimal` 用 `DATAZEN_DRIVERS=basic`；`pnpm e2e` 用当前默认。结论里要写清用的是哪一个。

---

## 7. 门禁与结论记录

### 7.1 长输出落系统临时文件（照 AGENTS.md 执行，这里是可直接复制的形态）

测试与门禁命令的输出动辄上千行，**结论恰恰在末尾**，被工具截断就等于没跑。一律重定向到**系统临时目录**，退出码单独打印。

```bash
# 单条命令的模板（把 CMD 换成 §7.2 表里的任意一条）
LOG="$(mktemp -t datazen-unit).log"
CMD 2>&1 | tee "$LOG" >/dev/null
echo "EXIT=${PIPESTATUS[0]}"
tail -n 40 "$LOG"
grep -nE 'FAIL|✗|test result:|error\[|error TS' "$LOG" | head -60
```

```bash
# 不想管管道退出码时，用最直白的形态
LOG="$(mktemp -t datazen-unit).log"
npx vitest run > "$LOG" 2>&1; echo "EXIT=$?"
tail -n 40 "$LOG"
```

**硬纪律**：

- 临时文件**只能放系统 temp 目录**（`mktemp` / `$TMPDIR` / `/tmp`），**不得落进仓库**：`git status` 必须保持干净，仓库内出现的日志文件会污染提交内容、也会让人误以为那是交付物。
- **完整日志留在文件里备查**，不要为了「看全」把整个文件回显进对话 —— 里面可能含凭据（§2.4d）。
- 退出码必须**单独打印并如实记录**；不要从 `tail` 的内容推断退出码。
- 结论行原样抄进报告，不改写。

### 7.2 本批方案的门禁清单

按改动类型选，**每一行都要有结论**：

| 触发条件 | 必跑命令 | 期望结论形态 |
| --- | --- | --- |
| 任何 TypeScript 改动 | `pnpm typecheck` | 无输出 + `EXIT=0` |
| 任何 Host 前端改动 | `pnpm test:unit:driver-set`（首选，自带 codegen 重铺、默认 `all`） | `Test Files` / `Tests` 两行原样记录；`EXIT=0` |
| 快速迭代（已知 codegen 是 basic 且新鲜） | `pnpm test:unit` | 同上，并注明「驱动集 = basic」 |
| 改动 `src/components/DataTable/**`、`src/stores/**`、`src/lib/**` | `pnpm test:unit:coverage` | 覆盖率表 + 阈值行；低于 `lines 80 / functions 80 / branches 75 / statements 80` 即失败 |
| 任何驱动 UI 改动 | `pnpm test:unit:drivers` | `Test Files` / `Tests` |
| 任何 `driver-api` / 驱动 crate Rust 改动 | `cargo fmt --all -- --check`；`cargo test -p datazen-driver-api --lib`；`cargo test -p datazen-driver-<id>`（集成加 `--tests`） | `test result:` 行逐条记录 |
| 任何 `src-tauri` Rust 改动 | `cargo fmt --all -- --check`；`cargo test -p datazen --lib`；要复现 CI 的 feature 条件时用 CI 的原式：`FEATURES=$(node -e "console.log(JSON.parse(require('fs').readFileSync('.driver-features.json','utf8')).features.join(','))")` 然后 `cargo test -p datazen --lib --features "$FEATURES"` | `test result:` |
| 新增/改动 i18n key | `node scripts/i18n-sync-check.mjs` | 脚本自己的总结行。**注意 CI 把它设成 `continue-on-error: true`（warning only），但本批方案要求本地干净** |
| 新增/改动 i18n key 的 key 冲突 | `pnpm test:i18n-keys` | 脚本总结行 |
| 新增/改动 IPC 命令名、类型名、领域术语 | `pnpm test:ids` | 脚本总结行 |
| 改 Host UI 交互路径（必须同 PR 更新 E2E） | `pnpm tauri:build:webdriver` 后 `pnpm e2e:smoke`（或相关 `--suite`） | WDIO 的 `passing` / `failing` 计数行 |
| 改契约矩阵或跨库行为 | `pnpm test:unit:e2e-contract:coverage`；`pnpm e2e:contract:matrix` | 覆盖率行 + `passing` / `failing` |
| 改 `scripts/**` 或宿主 CI 口径 | `pnpm test:scripts`（对应脚本自测在 `scripts/__tests__/`） | `Test Files` / `Tests` |
| CI 完整复现 | `pnpm ci:local`（`bash scripts/ci-local.sh`） | 逐段结论 |

> **CI 里真正会拦人的前端守卫**（决定本批方案会不会因格式/命名被红）：`pnpm typecheck`，然后 CI 把九条守卫合并成一步 fail-fast：`node scripts/check-managed-stubs.mjs`、`node scripts/check-structure-editor-guardrails.mjs`、`pnpm test:ids`、`pnpm test:layers`、`pnpm test:ci-docs`、`pnpm test:version`、`pnpm test:driver-protocol`、`pnpm test:boundaries`、`pnpm test:i18n-keys`，接着是 `pnpm test:unit`、`pnpm test:unit:drivers`、`pnpm test:unit:driver-set`。Rust 侧是 `cargo fmt --all -- --check`、basic 四驱动的 `cargo test --lib` 与 `--tests`、`cargo test -p datazen --lib --features ...`、`cargo test -p datazen-ai-api --lib`。

### 7.3 为什么必须首尾各记录一次 HEAD 与工作区 sha

AGENTS.md 的纪律：**同一棵工作树不得同时被提交方和验证方使用。** 实测代价是「提交方 `git add` 收进去的是验证方变异后的残留，于是出现一个 message 与内容完全相反的提交，而 `rev-parse` / `is-ancestor` 全部放过 —— 旧提交本身合法，只有内容是毒的」。

门禁重跑必须证明「运行期间没人动过这棵树」，做法是在**跑之前**与**跑之后各取一次**，两次相等才可归属：

```bash
# 运行前
git -C . rev-parse HEAD
git -C . status --porcelain=v1 | git hash-object --stdin

# ... 跑门禁 ...

# 运行后（再取一次，必须与上面完全一致）
git -C . rev-parse HEAD
git -C . status --porcelain=v1 | git hash-object --stdin
```

两条都要记进结论：**HEAD sha** 证明提交身份，**工作区内容 sha**（`git status` 输出的哈希，等价于「哪些文件被改过」的指纹）证明没有人中途改了文件。只记 HEAD 不够 —— 变异不改提交，改的是工作区。

验证方要变异、提交方要 `git add` 时，各自用独立工作树：`git worktree add --detach <path> <commit>`。

### 7.4 结论怎么写

每条门禁一条记录，**逐字结论行 + 退出码**，并补齐归属信息。建议直接贴这个模板（`progress.md` 或分册第 9 节都可以）：

```text
[门禁] pnpm test:unit:driver-set
[时间] 2026-XX-XX HH:MM
[HEAD] <sha-跑前> → <sha-跑后>   （必须相等）
[工作区] <content-sha-跑前> → <content-sha-跑后>   （必须相等）
[驱动集] full-all（默认）；Pro = main @ extensionPointsVersion 1.0.0
[EXIT] 0
[结论行-逐字]
 Test Files  123 passed (123)
      Tests  5702 passed (5702)
[覆盖] Test Files / Tests 两行已抄；retry 是否触发：未观察到（未观察 ≠ 没发生，见 vitest.config.ts 的 retry 注释）
[失败清单] 无
[反向注入] <本批方案的 1 次注入>：把 <符号> 的 <行为> 改坏 ⇒ <用例名> 转红（观察到的输出行逐字抄录）⇒ 已还原
```

**必须写进结论的六项**：HEAD 首尾、工作区 sha 首尾、驱动集、Pro 状态与 EP 版本、退出码、逐字结论行。

**报告纪律**：

- 失败时的输出可能含凭据（`.env` 里读出来的连接串等）。一旦怀疑，只摘录**结构性结论**（哪个文件、哪条断言、退出码、计数），不转发原始输出（§2.4d）。
- 环境造成的既有失败必须**单独标注为「环境」**，不与本批方案的回归混在一张清单里（§2.4b、§11 第 1 条）。
- 「retry 后转绿」的用例要显式记下来：`vitest.config.ts` 的 `retry: 2` 让「失败后重试通过」被报告为 PASSED，所以**绿不等于无间歇性失败**。

---

## 8. i18n key 清单

**本册自身不需要新增任何 i18n key**，因为它是纯文档（不含 UI、不含用户可见文案），也不改任何 locale 文件。分册引用本册时不要给它分配 key。

需要留意的是**分册**的 key 落点（`00-contracts.md` 第 8 节的文案纪律）：

1. **真正的英文词条在领域包，不在 `en.ts`**。`src/locales/en.ts` 只有一行 `export { default } from './en/index'`，改它没有任何效果。现有域名：`core` / `connection` / `query` / `schema` / `settings` / `sync` / `chart` / `dashboard` / `ai` / `backup` / `mcp` / `onboarding` / `workflows`。
2. 数据浏览文案按语义就近放入 **`src/locales/en/query.ts`** 或 **`src/locales/en/connection.ts`**，**不要新建域名**。分册的 key 清单必须写清「哪个域名文件 + 完整 key 路径」。
3. 开发期**只改英文侧**，其它语言（`de.ts` / `zh-CN.ts` / …）不要动；发布前由 `scripts/i18n-sync-check.mjs` 与 i18n 同步流程统一补齐。该脚本有两个 scope：宿主领域包 `src/locales` 与驱动包 `packages/drivers/<id>/locales`，它的契约是 **key 集合一致**（不是逐字比对某个聚合文件）。
4. **测试对 key 的用法**（`src/locales/locales.test.ts` 是既有基线，本批方案建立在它之上，不要另起一套）：

   | 既有用例（`src/locales/locales.test.ts`） | 对本批方案的含义 |
   | --- | --- |
   | `loads every built-in locale with non-empty host dictionaries` | 新加的 key 不能让某个内置语种字典变空 |
   | `every built-in locale resolves every UI key without leaking a raw key` | 新 key 必须在所有内置语种里解析成功、**不回显原始 key** |
   | `resolves every literal t() key used in host source (en dictionary)` | **这条会替我们拦住「代码里用了 `t('grid.xxx')` 但字典里没写」** —— 新文案必须同时进 `en/<域名>.ts`，否则这条红 |
   | `falls back to dict chain for unknown keys` | 未知 key 的回落行为是既有契约，测试不得依赖「未知 key 渲染成空」 |
   | `interpolates params for built-in locales` / `replaces multiple distinct params` | 带参数的 grid 文案（如「预期影响 1 行、实际 N 行」）必须断言**参数确实被插值**（`toContain(param)` + 无 `{` 残留），而不是断言整句 |

5. **新增文案的测试写法**：只断言「解析成功且不回显 key」（`text !== key`、`text.length > 0`），或断 i18n key + params 对象；需要可访问名称时走 `enCopy(key)`。**禁止**把 `t()` 渲染出的英文整句钉进断言或定位器（原则六）。

---

## 9. 各分册的测试验收矩阵

下表是**分册验收的检查表**：每个方案交付到「数量下限」并覆盖指定的关键用例类型，才算 D2 达成。「数量下限」是本册规定的**交付下限**，不是对现有代码的统计。

| 方案 | 落点 | 数量下限 | 关键用例类型（必须含） | E2E / 契约 |
| --- | --- | --- | --- | --- |
| **DB-01** 单元格 / 区域选择模型 | Host 前端单测 + journey | 纯函数 ≥12；DOM 断言 ≥5；journey ≥1 | `normalizeCellRange` 的**列序**归一化（不是字典序）、行列反转、单格区域、越界裁剪；`GridSelection` 的 `anchor`/`focus`/`extraRanges`/`mode` 互斥跃迁；**cell 模式必须清空 `selectedRows`**；Escape → `none` | 无（纯前端）；`data-dt-selected` 的渲染断言必含 |
| **DB-02** 键盘导航 | Host 前端单测 + journey | 键位映射 ≥10；journey ≥1 | 修饰键**精确匹配**（`mod+c` 不得在 `mod+shift+c` 时触发）；`delete` 同时匹配 Delete 与 Backspace；`'table'` 作用域在输入框聚焦时**被跳过**；Tab / Enter / Escape 的进入与退出；连续击键的逐格前进与后退 | 无 |
| **DB-03** 剪贴板 | Host 前端单测 + journey | 序列化/反序列化 ≥8；journey ≥1 | TSV 的**转义**（含制表符 / 换行 / 双引号）；行列对齐；空选区不写剪贴板；粘贴**半截 TSV** 只覆盖到已有列、不越界；多字节与 emoji 不被切断；粘贴走 `stageCellChange` 而不是直接改 `rows` | 可选：真实剪贴板的 E2E（jsdom 不可信，原则四） |
| **DB-04** 新增行 INSERT | 三处：`driver-api` / 驱动 crate / Host | `driver-api` ≥8；驱动 crate ≥2；Host（store 组装 + 冲突拒绝）≥8；journey ≥1 | `build_insert_sql` 默认实现：`Unset` 列**不出现在列清单与 VALUES**；全 `Unset` → `DEFAULT VALUES`；`insert_returning_clause` 默认 `None`；MySQL 驱动覆盖为 `INSERT INTO t () VALUES ()`；PG/SQLite 覆盖为 `RETURNING`；`grid.insert.noWritableColumn` / `grid.insert.returningUnavailable` 前缀断言；`delete_marked` 与非 `Unset` 列写入并存 ⇒ `grid.commit.conflictingIntents` | 契约矩阵 HC-EDIT 扩展 1 条；至少 1 个驱动的 crate E2E |
| **DB-05** 类型化编辑器与 Set Value | Host 前端单测 + journey | 类型归一化 ≥12；journey ≥1 | 每种 `data-dt-cell-type` 的编辑器选择；**空串 → NULL** 与「原值为 NULL 时保持不动」的区分；Boolean/Integer/Numeric/JSON 的强转；Enter 提交 / Escape 取消 / blur 未改动即取消；**IME 组合态不得提交**；生成列与只读列不得进入编辑 | 可选：`data-dt-cell-type` 的 E2E 断言 |
| **DB-06** 待提交单元格高亮 | Host 前端单测 + journey | 属性断言 ≥6；journey ≥1 | `data-dt-dirty` **出现与消失**；改回原值不算改动（必须复用 `valuesEqual`）；回滚后全部消失；翻页后 dirty 状态**不串行**（`rowIdentityAnchors` 语义） | 无 |
| **DB-07** 外键跳转与返回栈 | Host 前端单测 + journey | 返回栈 + 跳转 ≥8；journey ≥1 | 返回栈 push/pop 的 LIFO 顺序；连续两次跳转后一次返回回到中间表；无 FK 列时**不渲染入口**；候选值选择器的加载与取消；跳转与选择状态互不污染 | 契约矩阵 ≥1 条（FK 目录来自驱动元数据） |
| **DB-08** 筛选能力与命名视图 | 三处：`driver-api` / 驱动 crate / Host | `driver-api` ≥10；驱动 crate ≥2；Host ≥12；journey ≥1 | `supported_filter_operators()` 默认集与覆盖；`filter_operator_sql` 默认 `None`；**既有 10 个算子的渲染逐字不变**（兼容性回归）；不支持 / 未实现的算子必须报 `grid.filter.unsupportedOperator`，**禁止静默丢弃条件后照常查询**；不完整条件报 `grid.filter.incomplete`；命名视图 CRUD + 重载；`gridErrors` 的前缀匹配与未知回退 | 契约矩阵 HC-FILTER 扩展；驱动 crate 至少 2 条方言专有算子 |
| **DB-09** 行数三态与默认排序 | 两处：`driver-api` + Host | `driver-api` ≥10；Host ≥10；journey ≥1 | `count_strategy()` **由 `skip_count_query()` 推出**（默认实现等价性：不写任何代码的驱动行为与今天完全一致）；`Estimated` 与 `count_estimate_sql`；UI 显示「约 N 行」；`default_order_columns()` 默认实现与 `build_select_sql` 今天的注入逻辑**完全等价**（含无主键退化为第一列）；`allow_first_column_fallback_order() == false` 时无主键表不加排序 | 契约矩阵 HC-DATA |
| **DB-14** 查询结果网格可编辑 | Host 前端单测 + journey | ≥8；journey ≥1 | 只读结果**不渲染任何写入入口**；可写结果与表数据走**同一条** store 路径（不得另起一套）；执行新查询后的状态重置；能力缺失时隐藏入口而不是禁用 | 契约矩阵 HC-QUERY 扩展 |
| **DB-18** 键集分页 | 两处：`driver-api` + Host | `driver-api` ≥10；Host ≥8；journey ≥1 | `supports_keyset_pagination()` 默认 `false`；`keyset_predicate_sql` 的空排序 / 复合排序 / 单向边界 / 不可转换时返回 `None`；宿主在 `None` 或能力缺失时**降级回 OFFSET**（不是报错也不是静默空表）；翻页与排序切换后的游标失效 | 驱动 crate ≥1；契约矩阵扩展 1 条 |
| **跨方案（所有分册）** | `src/lib/gridErrors.ts` + 其单测 | 前缀匹配 ≥5；回退 ≥2 | 每个 `grid.<域>.<原因>` 前缀**逐字**匹配；未知前缀 ⇒ `'unknown'` 且 **UI 回退显示原始消息**（不吞、不变空）；空消息 / 纯空白消息走 fallback；前缀**只匹配开头**（正文里含同样词不得误判）；与 `src/lib/connectionShareError.ts` 的既有范式保持同一形状 | 无 |

**通用下限（每份分册都要满足）**：

- 至少 **1 条 journey test**（§5），且覆盖「进入 / 状态内 / 退出 / 残缺中间态 / 残留抑制检查」；
- 至少 **1 次反向注入**记录（§1 D5）；
- 至少 **1 条边界用例**取自 §6.1 的形态清单（NULL / 无主键 / 多字节 / 空表 至少各命中一次，跨整批而非单册重复）；
- 错误路径**至少各 1 条**：本条新增的每个 `grid.*` 前缀都要有一条断言它的用例。

### 9.2 「一条用例」怎么数（防止充数）

下限是按**用例条数**计的，所以先把计数口径钉死：

| 算一条 | 不算一条 |
| --- | --- |
| 一个 `it(...)` / 一个 `#[test]` fn | 在同一个 `it` 里连写十个不相关的断言（评审会按「一条」计，剩下的算未覆盖） |
| 同一被测符号的**不同分支**各一个 `it`（成功 / 失败 / 边界） | `it.each` 把三组数据塞进一个 `it` 后声称「三条」—— 三组数据算**一组边界**，除非它们触发不同分支 |
| 一个 E2E `it` 覆盖一条真实用户旅程 | 一个 E2E `it` 里跑三条互不相干的旅程 |
| Rust 里同一符号的一个 `#[test]` | 把断言写进辅助函数却不被任何 `#[test]` 调用 |

**换算关系**：E2E / 驱动 E2E 的「1 条」≈ 宿单单测的「若干条」，但**不能互相抵扣**。§9 表里同一行同时写了单测下限与 E2E 要求时，两者都要有。

### 9.3 不计入下限的东西（反面清单）

- **只断「渲染了」的用例**：`expect(getByText('x')).toBeTruthy()` 这类不构成对行为的约束，只算烟测。
- **反向注入后会一起变绿的用例**：至少用它做一次注入验证；通过不了的按假守卫删除重写，不计入下限。
- **被 `it.skip` / `it.only` 包裹的用例**：`.only` 提交即返工；`.skip` 只有写明原因并记入分册第 11 节才算「已知不可验证」，仍不计入下限。
- **在错误的套件里写的用例**：宿主套件不收集 `packages/drivers`，驱动套件不收集 `src/**`（§3.1）。写错地方等于 0 条。
- **只改 mock 的用例**：断言 mock 被调用了几次而不断言被测代码产生的可观察结果，不算。
- **重复既有用例的用例**：与既有 `src/lib/__tests__/databaseTypes.test.ts`、`src/stores/__tests__/tableDataStore.test.ts` 等重叠的部分，改成**扩展现有 `describe`** 而不是新开一份平行副本；重复副本不计入下限（也容易演变成两套真相）。

### 9.4 §6.1 数据形态的归属分配

§6.1 的形态清单是**整批**的覆盖义务，不要求每份分册都全测一遍。归属如下，各分册按此认领，未认领的形态由 12（本册）在总验收时补测：

| 形态 | 认领方 | 落点 |
| --- | --- | --- |
| 无主键表 | DB-09（默认排序）、DB-18（键集降级） | Host Rust 单测 + 契约矩阵 |
| 复合主键表 | DB-01（行身份与选区边界）、DB-04（INSERT 定位） | Host Rust 单测 + Host 前端单测 |
| 含 NULL 的列 | DB-04（`Null` vs `Unset`）、DB-05（空串 vs NULL） | 三处（driver-api / 驱动 crate / Host） |
| 超长文本 | DB-03（剪贴板）、DB-05（编辑器） | Host 前端单测 |
| 多字节与 emoji | DB-03（切分）、DB-05（光标） | Host 前端 journey + 驱动 crate 单测 |
| 含换行 / 制表符的文本 | DB-03（TSV 转义） | Host 前端单测 + E2E 复制粘贴 |
| 含外键的列 | DB-07 | Host 前端单测 + 契约矩阵 |
| 空表 / 单行表 | DB-09（行数显示）、DB-01（全选与区域） | Host 前端单测 |
| 只读账号 / 只读连接 | DB-04 / DB-05（入口按能力隐藏）、DB-14（只读结果） | Host 前端单测 + E2E |
| `requires_order_by` 的方言 | DB-09 / DB-18 | Host Rust 单测 + 对应驱动 crate 单测 |

### 9.5 分册验收时怎么用这张表

1. 打开分册第 9 节，对照 §9.1 该方案那一行，逐条核「数量下限」是否达到、指定关键用例类型是否都有。
2. 核 §9.2 的计数口径有没有被充数；核 §9.3 反面清单有没有混进来。
3. 核通用下限四条（journey / 反向注入 / 边界形态 / `grid.*` 错误前缀各一条）。
4. 核该分册认领的 §9.4 形态是否真有一条用例。
5. 一切通过后，把分册实际跑到门禁结论（§7.4 模板）贴进分册第 9 节，并写明**哪些没跑、为什么**（例如「`pnpm e2e:contract:matrix` 未跑：本机缺 webdriver 构建」）——**没跑就写没跑，不要用「逻辑上应该没问题」代替**。

---

## 10. 自查清单

提交前逐条对自己的 diff 打勾。每条都是本仓真实发生过的错法或本批方案的高危点。

| # | 错误做法 | 正确做法 | 会造成什么后果 |
| --- | --- | --- | --- |
| 1 | 用视口几何坐标反查单元格（`getBoundingClientRect` / 命中测试 / 手算行列） | 用 `data-dt-row` / `data-dt-col`（以及契约新增的 `data-dt-selected` / `data-dt-dirty` / `data-dt-editing` / `data-dt-cell-type`）直接绑定标识；反查走 `resolveDataTableCellFromEvent` | JSDOM 没有排版引擎，几何断言在单测里通过、在真机 WKWebView 上失效（原则四 + 案例 2），这类测试是**假绿** |
| 2 | 用 `sleep` / `browser.pause` / `setTimeout` 等异步 | `await waitFor(...)` / `findByX` / `deferred()` 手工控制 Promise / `waitUntil(async () => 可断言的属性)`；E2E 等 `waitForDisplayed` 与属性变化 | 慢机器上随机红（`vitest.config.ts` 的 `retry: 2` 会把它报告成 PASSED，于是间歇性失败被永久掩埋） |
| 3 | 多个用例共用同一份可变夹具（共享 `Map` / `Set` / store 实例 / 顶层可变数组） | 每个用例自建数据；`beforeEach` 里 `reset()` store、重建 `Map`/`Set`；模块级只放不可变常量 | 失败顺序漂移、串行污染；单独跑绿、全量跑红（或反之） |
| 4 | 只测快乐路径（完整 TSV、一次成功的提交、格式完备的筛选值） | 必须含**残缺中间态**与退出跃迁：半截粘贴、输入到一半 Escape、不完整筛选、计划过期、影响行数 ≠ 1 | 「测试通过率陷阱」：全绿而真机一敲就断（原则三 + 案例 1） |
| 5 | 用 `any` 绕类型，或删断言/删测试让 `tsc` 过 | 测试文件**参与**类型检查；mock 只实现子集时用 `as unknown as X` / `satisfies` / `Pick<>` 精确说明 | 类型漂移长期不被发现（历史上 `TableInfo.rowCount` 实收 `null` 却声明为 `?: number`）；删断言等于丢覆盖，比没有测试更糟 |
| 6 | 把驱动专属测试写进 Host（`src/**` 或 `e2e/specs/`） | 判断「换个驱动还成立吗」：只对一个驱动成立 ⇒ `packages/drivers/<id>/{src,tests,ui/__tests__,e2e}` | 宿主套件**不收集** `packages/drivers`，驱动套件**不收集** `src/**` —— 写错地方等于这条测试从未运行 |
| 7 | 把 `t()` 渲染出的英文文案钉进断言或定位器（`getByText('Apply')`、`toHaveBeenCalledWith('Missing value for :uid')`） | 用 `data-*` / `data-i18n-key` / i18n key；需要字典值时用 `enCopy(key)`（miss 当场抛错）；断 key + params 对象而不是整句 | 一次文案改动放大成 N 个测试文件变红，并诱导「为了测试绿」回退正确的产品口径；裸 `en[key]` miss 会让定位器退化成永真匹配（假守卫） |
| 8 | 断言用 `toContain` 但被断的串在别处也出现；或断「存在」而非「恰好一次」 | 数量与身份用 `toEqual` / `toHaveLength` / `toBe`；必须用子串时同时断言唯一性 | 假守卫：把行为改坏仍然全绿，制造「已覆盖」的错觉 |
| 9 | 绕开 `requestRevision` / `loadingRevision` 直接写 `rows`（新增的行数估计、FK 候选、键集翻页） | 复用既有版本号判定，并在测试里**显式**构造「旧请求后返回」，断言旧结果被丢弃 | 翻页/筛选竞态：界面显示上一次请求的数据，且没有任何用例能拦住 |
| 10 | 把测试日志、截图、临时产物落在仓库里（`> result.log`、`--screenshot` 输出到仓库） | 一律 `LOG="$(mktemp -t datazen-<名>).log"`，只放系统 temp；结论行抄进台账/报告 | 污染 `git status`，日志可能被 `git add` 收进提交，且可能把凭据带进版本库 |
| 11 | 门禁只记「都过了」，或在同一棵树上让验证方变异、提交方 `git add` | 首尾各记一次 HEAD 与工作区 sha，两次必须相等；验证方用独立 worktree | 结论不可归属（message 与内容相反的提交可以合法落地），且失败集合跨 worktree 不可比 |
| 12 | 把环境造成的既有失败当成本批方案的回归（Pro 版本不一致、驱动集不同、缺 codegen 产物） | 先按 §2.4 核对三类环境事实，再下结论；结论里单独标注「环境」 | 误判自己的改动弄坏了构建，或反过来放走真回归 |

---

## 11. 未决问题

需人工裁定，逐条给建议。

| # | 问题 | 建议 |
| --- | --- | --- |
| T-1 | **Pro 检出与宿主 EP 版本不一致要不要纳入本批门禁？** 事实是：Pro 包声明 `1.0.0`、宿主 `EXTENSION_POINTS_VERSION` 是 `1.1.0`；但宿主侧读真实 Pro manifest 做「两端一致」断言的那两条用例**已被删除**（该文件自己写明搬到了 Pro 仓的 `scripts/verify-host-pin.mjs`），我今天在宿主测试里找不到任何读真实 Pro 检出的用例 | **不纳入本批门禁**。理由有三：(1) 宿主门禁里已不存在这条用例，纳入会变成「守一个不存在的门」；(2) 一致性断言的正确归属是 Pro 仓的 `verify-host-pin.mjs`，宿主 CI 结构上拿不到那个仓库（interaction 文件与该测试文件的注释都写明「唯一结果是 skip 或本地红，不是门禁」）；(3) 改 Pro 的 `manifest.json` 或宿主 EP 常量都属于改契约，越出本批范围。**但本册保留 §2.4b 的说明与 §10 第 12 条的自查项**：一旦本地真看到 `expected '1.0.0' to be '1.1.0'`，当场按环境问题归类并在结论里单独标注，不要动手改版本号 |
| T-2 | **`pnpm test:unit:coverage` 要不要作为本批方案的强制门禁？** 它存在且对 `src/lib/**`、`src/stores/**`、`src/components/DataTable/**` 设了 `lines 80 / functions 80 / branches 75 / statements 80`，但 CI 的前端任务跑的是 `pnpm test:unit` / `pnpm test:unit:drivers` / `pnpm test:unit:driver-set`，**没有**跑覆盖率 | 建议**强制**：本批方案主要改的就是这三块，覆盖率是唯一能拦住「新分支没测」的自动门。若嫌重，退一步方案：只在改动这三块时强制，并把 `--coverage` 的结论行抄进门禁记录 |
| T-3 | **反向注入要做多少次？** §1 D5 要求「至少 1 次」，但一条 journey 里可能有多条关键断言 | 建议按**分册**记 1 次（有代表性的一条：例如 DB-09 注入「`count_strategy` 不再由 `skip_query` 推出」，DB-04 注入「`Unset` 被补成 `NULL`」），并在分册第 9 节写明注入的是哪个符号、哪条用例转红。不做「每条断言都注入」——成本过高且会稀释重点 |
| T-4 | **E2E spec 完全没有类型门禁**（`tsconfig.json` 的 `include` 不含 `e2e/`，`e2e/tsconfig.json` 存在但没有任何 script 或 workflow 驱动它） | 建议**单独立项**把 `e2e/tsconfig.json` 接进 `pnpm typecheck`，不要塞进数据浏览这一批（它会一次性暴露 e2e 目录的全部既有类型问题，与本批方案无关，混在一起会让本批的回归清单不可读）。在本批范围内，纪律是：新增/改动的 E2E spec **必须真跑一次**（`pnpm e2e:smoke` 或对应 `--suite`），不能只靠「看起来对」 |
| T-5 | **门禁结论要不要带「驱动集指纹」？** 现在只写驱动集名字（basic / all / 显式列表），但同一个名字在不同 checkout 下解析出的 dbType 集合可能不同 | 建议带：把 `DRIVER_DB_ENTRIES` 的 key 数（本工作树 basic = 11）与 key 列表一并记入结论。这样「同一份 Host 单测读到的注册表变了」这件事在结论里可被复核，而不是只能靠记忆 |
| T-6 | **live 契约用例在缺 fixture 时算不算通过？** CI 注释指出可用 `DATAZEN_CONTRACT_REQUIRE_LIVE=1` 把「无法验证」升级为失败 | 建议在本批方案内**默认不设**该环境变量（保持 unverified 可见，而不是让缺 fixture 静默变红或静默变绿），但要求：凡是声称「跨 PG/MySQL/SQLite 都验证过」的结论，必须附上 `pnpm e2e:contract:matrix` 的真实 `passing` / `failing` 计数行；否则只写「未验证」 |
| T-7 | **DB-04 的「新行保持高亮」承诺怎么测？** `insert_returning_clause` 为 `None` 时宿主走「提交后重新拉取当前页」的降级路径，且契约要求 UI **不承诺**新行会保持高亮 | 建议把这条拆成两条用例：(1) `Some(clause)` 的驱动 ⇒ 提交后新行仍在当前页且可被定位；(2) `None` 的驱动 ⇒ 提交后**必须有可见的刷新提示**且不出现「高亮了一个不存在的行」。第二条的具体 UI 形态需要产品口径裁定，建议在分册 04 里落成 `data-dt-*` 或 `tid()` 断言，而不是断文案 |
