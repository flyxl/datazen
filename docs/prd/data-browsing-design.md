# 数据浏览（Data Browsing）技术设计 · 总纲

> **文档性质**：本目录是**按用户明确要求保存的一次性技术设计**，属仓库 [文档维护纪律](../../AGENTS.md) 的窄范围例外（同 [PRD](data-browsing-optimization-prd.md) 的处理方式，已在 [docs/README.md](../README.md) 登记）。它记录的是**目标设计**，不代表当前 `main` 已实现；方案落地后应逐节改写为「已实现事实」并入 [architecture/frontend/components.md](../architecture/frontend/components.md) 与 [features/](../features/)，随后删除本目录。
>
> **基线**：`main` = `e0f6ba28b`。
> **引用纪律**：全目录**不写行号**（行号会随重构腐烂，且无门禁校验），位置一律用「文件路径 + 符号名」。**全文禁止出现 `文件名:行号` 形式的引用**。

---

## 1. 这套文档解决什么问题

[PRD](data-browsing-optimization-prd.md) 回答了**「我们和 TablePlus 差在哪、为什么差、值不值得补」**：65 项差距矩阵、7 条根因、20 个方案与优先级。

本目录回答**「具体怎么写代码」**：把 PRD 里 P0 的方案拆成可独立开工、可独立验收、可独立回滚的详细设计，详细到**一个没接触过这个项目的实习生照着做也能写出合格代码**。

两者分工不重叠：**需求与优先级看 PRD，实现细节看本目录。**本目录不重新论证「为什么要做」，也不重新排优先级。

### 目标读者与前置知识

你需要知道（不知道就先补，本目录不教这些）：

| 需要掌握 | 去哪看 |
| --- | --- |
| 项目整体架构与目录 | 仓库根 [`AGENTS.md`](../../AGENTS.md) |
| 前端组件与状态层设计 | [architecture/frontend/components.md](../architecture/frontend/components.md)、[architecture/frontend/state.md](../architecture/frontend/state.md) |
| 四维扩展体系与驱动契约 | [architecture/frontend/extensibility.md](../architecture/frontend/extensibility.md) |
| `connectionId` 与 `dbSessionId` 的区别 | [architecture/naming.md](../architecture/naming.md) |
| 测试怎么写、门禁怎么跑 | [12-testing.md](data-browsing-design/12-testing.md) |

**开始写代码之前必须先读**：[00-contracts.md](data-browsing-design/00-contracts.md)。它是本目录所有分册的**唯一契约源**——单元格坐标系、`CellWrite` 三态、驱动新增方法签名、IPC 变更、错误码，全部在那里冻结。分册只讲「怎么实现」，不讲「契约长什么样」。

---

## 2. 文档地图

| # | 分册 | 方案 | 一句话职责 | 依赖 | 预估（人日） |
| --- | --- | --- | --- | --- | --- |
| 00 | [共享契约](data-browsing-design/00-contracts.md) | — | 冻结所有跨方案契约；无代码 | — | 0 |
| 01 | [单元格与区域选择模型](data-browsing-design/01-selection.md) | DB-01 | 坐标系、锚点、区域、非连续选择、三种模式跃迁、`role="grid"` 骨架与 `rowheader` 改造 | 00 | 4.0 |
| 02 | [键盘导航与快捷键](data-browsing-design/02-keyboard.md) | DB-02 | 方向键/Home/End/PageUp/Down/Tab/Enter 导航与编辑进入 | 00, 01 | 2.5 |
| 03 | [剪贴板复制与粘贴](data-browsing-design/03-clipboard.md) | DB-03 | TSV 复制、批量粘贴、与暂存改动合并 | 00, 01 | 5.5 |
| 04 | [新增行 `INSERT`](data-browsing-design/04-insert.md) | DB-04 | `CellWrite` 三态落地、插入语句构造、主键回填与降级 | 00 | 4.0 |
| 05 | [类型感知编辑器与 Set Value](data-browsing-design/05-editors.md) | DB-05 | 按列类型选编辑器、NULL/默认值、长文本与 JSON 编辑 | 00, 01 | 8 |
| 06 | [待提交单元格高亮](data-browsing-design/06-dirty-cells.md) | DB-06 | 脏单元格标记、行级汇总、提交后清理 | 00, 01 | 2.5 |
| 07 | [外键跳转与返回栈](data-browsing-design/07-fk-navigation.md) | DB-07 | 外键值跳转到被引用表、面包屑与返回栈 | 00, 01 | 4.5 |
| 08 | [筛选能力与命名视图](data-browsing-design/08-filters.md) | DB-08 | 扩展算子集、按类型给算子、**逐条 AND/OR 连接符**、命名视图持久化 | 00 | 约 14 |
| 09 | [行数三态与默认排序可配](data-browsing-design/09-count-and-order.md) | DB-09 | 精确/估计/未知三态、默认排序策略与配置入口 | 00, **08** | 2.5 ~ 3.5 |
| 10 | [查询结果网格可编辑](data-browsing-design/10-result-grid.md) | DB-14 | 让查询结果也能改、明确写回目标、失败可解释 | 00, 01, 04, 05, 06 | 3.5 |
| 11 | [键集分页](data-browsing-design/11-keyset-paging.md) | DB-18 | 深翻页用 seek 替代 OFFSET、边界与降级 | 00, 09 | 5 ~ 6 |
| 12 | [测试基建（共享）](data-browsing-design/12-testing.md) | — | 测试分层落点、journey 模板、门禁与结论记录 | 00 | 随各册计入 |

> 预估工作量是**净编码+测试**的人日，不含评审与返工。参考实现顺序见第 4 节。
>
> **本表的预估值必须与各分册抬头的「预估工作量」逐字一致，两者都是对同一份工作量的两处记录，不允许各算一套。**改任何一份分册的工作量时，在**同一次提交**里同时改另一处——本表是各册的汇总，分册抬头是就地可查的明细，缺一处就会让人按汇总排期、按明细领活，差额直接变成「进度对不上」。
>
> 「依赖」列同样以各分册抬头的自述为准；其中 **09 依赖 08** 是一条**硬顺序**而非可选项：`query_executor.rs → query_executor/{mod.rs,tests.rs}` 的目录化归属冻结给 08（[00-contracts.md](data-browsing-design/00-contracts.md) 第 9.1 节），09 的 Step 0 在自己合并时必须已经完成。**10 不依赖 02/03**：它要的是选择模型、写路径行身份与编辑态，不是键盘导航与剪贴板。

---

## 3. 给实习生的阅读与开工路径

不要试图一次读完 12 份分册。

**第 1 步（半天）**：读本总纲 → 读 [00-contracts.md](data-browsing-design/00-contracts.md) 全文 → 读 [12-testing.md](data-browsing-design/12-testing.md) 的「测试分层与落点」与「通用测试约定」两节。此时你应该能回答：`CellWrite` 为什么必须是三态？`data-dt-col` 存的是列名还是列下标？为什么写路径不能直接用行号拼 `WHERE`？

**第 2 步（半天）**：打开 `src/components/DataTable/` 与 `src/stores/tableData/` 两个目录，对着 [00-contracts.md](data-browsing-design/00-contracts.md) 第 1 节把真实代码读一遍，确认文档描述与代码一致。**若发现不一致，以代码为准，并把不一致记进对应分册的「未决问题」**——文档腐烂是常态，你的职责是把它修准。

**第 3 步（按顺序开工）**：只领一份分册，按它第 5 节「实现步骤」逐步做，每步做完立刻跑该步自测。**不要跨分册并行**：01 是所有交互类分册的地基，地基没验收就动 02/03/05/06，会产生返工。

**第 4 步（提交前）**：过一遍分册第 10 节「自查清单」，逐条打勾，再把第 7 节「边界与异常清单」当测试用例表跑一遍。**没跑过第 7 节就不算做完**。
> 本步的分册编号指 **01~11**（11 节模板：第 5 节实现步骤 / 第 6 节文件级改动清单 / 第 7 节边界与异常清单 / 第 9 节测试清单 / 第 10 节自查清单 / 第 11 节未决问题）。**12-testing.md 不适用本步**——它不是一份要领的实现任务，而是所有分册共用的测试落点与门禁约定：你领到哪一份分册，就按哪一份的清单走。

---

## 4. 实现顺序与提交切分

### 4.1 依赖关系

```
00 契约冻结
  │
  ├─ 01 选择模型 ──┬─ 02 键盘
  │                ├─ 03 剪贴板
  │                ├─ 05 类型编辑器 ─┐
  │                ├─ 06 脏单元格高亮 ┼┐
  │                └─ 07 外键跳转     │
  │                                  │
  ├─ 04 新增行 INSERT（只依赖契约里的 CellWrite） ─┘
  │                                                 │
  ├─ 08 筛选（只依赖契约里的算子契约）              │
  │    └─ 09 行数/排序（**硬顺序**：08 的 query_executor/ 目录化必须先落地）
  │          └─ 11 键集分页
  │
  └─ 10 结果网格可编辑（依赖 01 / 04 / 05 / 06，**不依赖 02 / 03**）
```

**关键路径是 01 →（04 ∥ 05 ∥ 06）→ 10**，其次是 **08 → 09 → 11** 这条串行链。

- 04/05/06 互不依赖，可与 01 并行开工，是并行度的来源；
- **08 → 09 是硬顺序，不是「先来先得」**：目录化归属按冻结提交序归 08（[00-contracts.md](data-browsing-design/00-contracts.md) 第 9.1 节的 G-1/G-3），09 的 Step 0 在自己合并时已经不存在了，只需在 08 留下的 `query_executor/tests.rs` 里补用例；
- **10 不依赖 02/03**：它要的是选择模型、写路径的行身份、编辑态与脏标记呈现，键盘导航与剪贴板对它是可选的，因此 10 可与 02/03 **并行**推进，不必等它们。

### 4.2 推荐提交序列

每一条都是**可独立合并、可独立回滚**的原子提交，提交信息建议如下：

| 序 | 提交信息 | 内容 | 验收门禁 |
| --- | --- | --- | --- |
| 1 | `feat(driver-api): add CellWrite and insert builders` | 契约与默认实现 + 单元测试，**不含任何 UI** | `cargo test -p datazen --lib`、`cargo test -p datazen-driver-api` |
| 2 | `feat(grid): add cell selection model` | 分册 01 | `npx vitest run`、`pnpm typecheck` |
| 3 | `feat(grid): add keyboard navigation` | 分册 02 | 同上 + 分册 02 的 journey 测试 |
| 4 | `feat(grid): add clipboard support` | 分册 03 | 同上 |
| 5 | `feat(grid): add type-aware cell editors` | 分册 05 | 同上 |
| 6 | `feat(grid): highlight pending cells` | 分册 06 | 同上 |
| 7 | `feat(grid): support inserting rows` | 分册 04（前后端 + 驱动默认实现） | 上述 + 驱动 crate 测试 |
| 8 | `feat(grid): add filter operators and named views` | 分册 08 | 上述 + i18n 校验 |
| 9 | `feat(grid): add count strategy and configurable default order` | 分册 09 | 上述 |
| 10 | `feat(grid): make query result grid editable` | 分册 10 | 上述 + E2E |
| 11 | `feat(grid): add foreign key navigation` | 分册 07 | 上述 |
| 12 | `feat(grid): add keyset pagination` | 分册 11 | 上述 + 契约矩阵 |

**为什么第 1 条必须先落地**：`CellWrite` 会改到驱动的 trait 契约。先让它独立进入主干，后面所有 UI 提交才有稳定地基；否则 UI 提交与契约变更纠缠在一起，回滚任何一个都会带倒另一个。

**为什么不把 01~06 合成一个提交**：它们分别对应六种可独立失效的交互（选择、键盘、剪贴板、编辑器、脏标记、跳转），合成后一旦出现回归，定位范围会从「一个交互」膨胀到「六个交互」。

---

## 5. 全局约束速查

以下约束**对每一份分册都成立**，分册内不再重复。违反任何一条都会被评审打回。

### 5.1 兼容性（详见 [00-contracts.md](data-browsing-design/00-contracts.md) 第 2 节）

- **R1 只新增，不改签名**：不改 `build_update_sql` / `build_delete_sql` / `format_sql_literal` / `pagination_syntax` / `get_table_data` 等既有方法签名，也**不改既有 DTO 字段的类型或名字**。需要新语义时新增方法或字段。
- **R2 新增 trait 方法必须有默认实现**：这样任何驱动（50+ path 驱动与 git 驱动）不改一行代码仍能编译。⚠ **默认实现的约束是单向的——允许「扩大」能力面，不得「移除或改变」今天已有的能力**，因此本条**不是**「默认行为逐字等于今天」。像 `supported_filter_operators` 默认返回 19 个算子（今天只有 10 个）就是刻意扩大的：宿主渲染器能处理全部 19 个，对旧驱动只会更宽松。凡刻意扩大，必须在方法注释里写明「今天的返回值是什么、为什么默认改成这样」。
- **R3 JSON 契约靠 serde 自动映射**：Rust 侧已标 `#[serde(rename_all = "camelCase")]`，前端用 camelCase。**不要**手写转换函数，**不要**给前端预留 snake_case 别名，新增 DTO 必须带同名属性宏。
- **R4 默认值必须等于「今天缺省时的实际行为」，不是无条件填 `false`**：今天默认**不可用**的能力（如 `insertReturning`）填 `false`；今天默认**可用**的能力填 `true`——典型如 `DataGridCapabilities.editable`，今天所有驱动都没声明过该字段、前端一律按可编辑渲染，填 `false` 会让所有未声明 `dataGrid` 的驱动**一夜之间变只读**，这正是本条要防的回归；枚举类（如 `countStrategy`）缺省填**今天走的那条分支**，不是「最严格的那个」。新增枚举分支必须在旧数据上反序列化成功（能用 `#[serde(default)]` 就用）；能力缺失时 UI **隐藏入口**，而不是渲染一个点了报错的按钮。
- **R5 生产路径禁止裸 `unwrap()` / `expect()`**，见 [panic-policy.md](../development/panic-policy.md)。
- **不需要提升 `PROTOCOL_VERSION`**（当前为 `4`，`MIN_PROTOCOL_VERSION` 为 `1`）：只新增带默认实现的方法、默认行为等于或不窄于今天、不动既有签名与既有 DTO 字段类型——已由契约第 11 节 C-3 裁定。**一旦改了既有签名就必须提升并同步所有 git 驱动的 `ref` 钉定。**

### 5.2 零硬编码

数据库之间的行为差异**只能**通过驱动 trait 方法或驱动元数据声明表达，**绝不**能在宿主代码里出现 `if (dbType === 'mysql')` 这类分支。前端能力声明写在驱动自己的元数据文件里（绝大多数是 `packages/drivers/<id>/ui/meta.ts`；redis 是 `packages/drivers/redis/ui/shared/meta.ts`），**不写在宿主**。

### 5.3 测试落点

- 驱动专属实现/方言/UI 的测试**必须**写在该驱动 crate 目录（`packages/drivers/<id>/`），**禁止**放进 Host；
- Host 通用 UI 交互测试放组件相邻的 `__tests__/`；
- E2E 放 `e2e/specs/`；
- 测试文件**参与类型检查**，`pnpm typecheck` 必须干净；mock 只实现子集时用精确断言（`as unknown as X` / `satisfies` / `Pick<>`），**禁止**用 `any` 绕过，也禁止为了让类型通过而删测试或删断言。

### 5.4 代码规模与风格

- 单源码文件**不得超过 800 行**；超了必须按职责拆子模块，并在分册第 6 节「文件级改动清单」里体现拆分结果；
- 前端严格模式、无 `any`（generated 文件除外）、absolute imports；
- Rust 用 `rustfmt` + `thiserror` + `tracing` + `CommandError`；**生产路径禁止裸 `unwrap()` / `expect()`**（见 [panic-policy.md](../development/panic-policy.md)）；
- 用户可见文案**只改英文侧领域包** `src/locales/en/<域名>.ts`（数据浏览相关放 `query.ts` 或 `connection.ts`）。注意 `src/locales/en.ts` 只是聚合再导出入口，改它无效；其他语言文件开发期不动，发布前由 i18n 同步流程补齐。

### 5.5 交互与状态机

界面上任何「根据上下文判断该做什么」的逻辑都必须具备**完整三要素**：进入条件、状态内行为、退出跃迁条件。**禁止**只有进入没有退出的单向死锁逻辑（例如「打开了编辑器却没有任何路径能关掉它」）。交互类分册必须交付**连续旅程测试**，覆盖残缺中间态。

点击与选择**必须**通过 DOM `data-*` 属性绑定标识（`data-dt-row` / `data-dt-col` / `data-col-header` 及 [00-contracts.md](data-browsing-design/00-contracts.md) 第 4.4 节新增的四个），**禁止**依赖视口几何坐标反查单元格。

### 5.6 门禁与结论记录

- 测试/门禁命令的输出动辄上千行，**必须**重定向到**系统临时目录**再取尾部（结论在末尾）；临时文件**不得落进仓库**（会污染 `git status`）；退出码必须单独打印并如实记录。
- **同一棵工作树不得同时被提交方和验证方使用**（见 [worktree-isolation.md](../development/worktree-isolation.md)）。门禁重跑要首尾各记录一次 HEAD 与工作区 sha，证明运行期间没人动过。
- **驱动集必须写进结论**：本工作树以 `--drivers=basic` 铺设（postgres / mysql / sqlite / redis），CI 常以 `--drivers=all` 运行；`DB_REGISTRY` 由 `generated.ts` 合并而来，驱动集一变宿主单测读到的注册表就变，因此「Host 用例全绿」只在本驱动集语义内成立。`pnpm typecheck` 不受影响。
- 本工作树内 `packages/pro-extensions/sql-editor-pro`（Pro `main`，manifest 声明 `extensionPointsVersion=1.0.0`）与宿主 `1.1.0` 不一致，会让 `packages/extension-points/src/__tests__/security.test.ts` 的版本一致性用例失败。**这是环境造成的既有失败，不是本批方案的缺陷。**

### 5.7 共享热文件行数总账（唯一账本）

多个分册会往同一批文件里加接线，**行数账目只能有一个权威来源**：[00-contracts.md](data-browsing-design/00-contracts.md) 第 9.1 节。各册在「文件级改动清单」里申报热文件增量时必须遵守：

- **必须写明基线**以及「在自己之前合并的分册已完成哪些抽取」；只给一个数字而不写基线的，视为**未申报**；
- 若某册的增量会让文件达到或超过 800 行，该册**必须**在**同一次提交**里带上自己的抽取方案（新建文件 + 搬走哪些内容 + 预估行数），不得把超限留给下一个分册去撞；
- 账本里标注「创建归属」的新文件，**只有该分册可以创建**；其他分册只能复用、追加成员或加可选 props，**禁止另建平行文件**（同一职责出现三个文件名的教训已经真实发生过一次）；
- 分册若发现账本与自己的实测不符，**先改账本**（它是契约的一部分），再改自己的推算；禁止各册各算一套——**这条规则本身就是被真实分歧逼出来的：08 判定「本册与 09 合计必超 800」，05 判定「由 01 抽取后不再触红线」，两者对同一个文件得出了相反结论，而两者都只看到了部分增量。**

账本记录的两个结论：`TableView.tsx` 基线 770，逐笔叠加的峰值是 **746**（09 合并后），距 800 还剩 54 行；**把 08 那次 −90 的抽取拿掉重算，越界点不是「各册合计 824」，而是第一个越界提交——09（836 行），11 合并后 891 行**（因此那次抽取是**合并前置条件**，不是优化）；`src-tauri/src/services/query_executor.rs` 基线已 **797 行**，必须先目录化才能继续加子模块。此外 `packages/driver-api/src/types.rs`（939）、`traits.rs`（1020）、`src-tauri/src/commands/data.rs`（1331）**在本批之前就已超 800 行**，属于既有债务，本批只做最小净增并把新内容放进新文件。

---

## 6. 完成定义（Definition of Done）

一份分册只有在**同时**满足以下条件时才算交付：

1. 分册第 5 节的实现步骤**全部**完成，且没有跳过任何一步；
2. 分册第 9 节测试清单的用例**全部**存在并通过；
3. 分册第 7 节边界与异常清单**逐条**有对应行为或对应测试；
4. 分册第 10 节自查清单**逐条**打勾；
5. 分册第 2 节「现状代码事实」与**当时代码**一致（若已腐烂，先修文档再改代码）；
6. `pnpm typecheck` 干净（含测试文件）；
7. Rust 侧改动通过 `cargo test -p datazen --lib`（按改动范围追加 `datazen-driver-api` 或具体驱动 crate）；
8. 前端改动通过 Host 单测；涉及交互的追加 journey 测试；
9. 新增/修改的 i18n key **只改英文侧领域包** `src/locales/en/<域名>.ts`（数据浏览相关放 `query.ts` 或 `connection.ts`）——**不是** `src/locales/en.ts`，后者只是 `export { default } from './en/index'` 的再导出入口，改它没有任何效果。其他语言文件开发期不动；
10. 门禁结论按第 5.6 节格式记录（命令、退出码、逐字结论行、驱动集）。

**未决问题不阻塞交付**：分册第 11 节的条目若尚未裁定，先按「建议选项」实现，并在提交信息中标注待裁定；裁定后再调整。

---

## 7. 本目录明确不包含的内容

- **不包含需求论证与优先级重排**：那是 [PRD](data-browsing-optimization-prd.md) 的职责。本目录不讨论「该不该做 DB-13」。
- **不包含 P1/P2 方案的详设**：DB-10~DB-13、DB-15~DB-17、DB-19、DB-20 以及嵌套布尔分组、单步撤销等，待 PRD 排期后再补分册。
- **不包含具体排期与人名**：第 4 节的顺序是技术依赖顺序，不是日历计划。
- **不包含进度台账**：进度记在分支根目录的 `progress.md`，**交付即销毁**，不得进入 `docs/`。
- **不包含实现后的架构事实**：实现完成后，结论应改写为「已实现」并入 [architecture/](../architecture/) 与 [features/](../features/)，本目录随之删除。

---

## 8. 本目录自身的一致性纪律

- **不写行号**：全目录禁止 `文件名:行号` 形式的引用。
- **不重复契约**：契约只在 [00-contracts.md](data-browsing-design/00-contracts.md) 定义，分册引用而不复制。发现重复定义，删掉分册里的那份。
- **改契约要同步**：若实现过程中确认契约需要调整，**先改 00-contracts.md**，再改受影响的分册，最后改代码。三者的顺序不能颠倒，否则必然出现「代码是这样、文档是那样」的漂移。
- **链接要可达**：`pnpm exec` 下的脚本与 CI **不会**校验 Markdown 链接，失效引用只能靠人工检查；改动目录结构后必须自查一遍链接。
- **机械化自查优先于人眼抽查**：本目录在撰写过程中用过三类脚本（都放在**系统临时目录**，不入库）：① 结构核查（11 个必备小节是否齐全、标题是否逐字一致、代码围栏是否成对、链接是否可达）；② 引用核查（文中提到的仓库文件路径是否真实存在，区分「新增」与「写错」）；③ 符号核查（第 2 节声称读过的标识符是否真在代码里）。**这三类核查抓出的问题远多于人眼抽查**，其中「引用了一个并不存在的 API」和「两个分册对同一文件得出相反行数结论」都是脚本先发现的。新增或修改分册后请重跑同类检查。
- **共享文件的归属只能由冻结提交序裁定**：[00-contracts.md](data-browsing-design/00-contracts.md) 第 9.1 节把每个共享热文件的**创建归属**钉死在某一份分册上，规则是 **G-1 唯一归属**（别的分册只能追加成员、不得另建平行文件）、**G-2 禁止整文件重写**、**G-3 显式先后边**。判定归属时**只认冻结提交序里最早的那一章**，不写「以先落地者为准」「谁先合并谁做」——并行合并下那等于没判，真实事故是整文件覆盖**不产生任何编译错误**，只是静默抹掉先到者的内容。
- **事实陈述必须带来源标记**：分册里凡是「现状代码事实」的断言（行数、符号存在与否、某个函数今天做了什么），要能区分是**读代码读到的**、**从基线推算的**，还是**存疑的**。PRD 用 `［文档］/［变更］/［社区］/［存疑］` 四级标记，分册沿用同一套。**推算值不得写成实测值**——[00-contracts.md](data-browsing-design/00-contracts.md) 第 9.2 节的强制规则 6 要求每个「净增/抽取」数字都附带实测命令与行号区间，就是这条纪律的落地形态。
- **新增错误码必须回写契约账本**：`src/lib/gridErrors.ts` 由 01 创建（契约 §8.1），此后每份分册**只能追加**自己那一段前缀码，且要走完三步——`GridErrorCode` 加成员 → `GRID_ERROR_CODES` 加条目 → **回到契约 §8.2 的全量码表登记**。漏掉第三步的后果是新人只读到 §8.2 就以为码表是全的，照着造一个重名码。**整文件重写该文件一律禁止。**
- **路径必须写全，不得依赖小节标题的上下文省略前缀**：Rust 侧一律写 `src-tauri/src/...` 或 `packages/driver-api/src/...`，**禁止**写成 `src/traits.rs` 这种省略包名的形式（它指向一个不存在的路径，读者与检索脚本都会被误导）。前端才用 `src/...`。分册在小节标题里写明「本节都在 `packages/driver-api` 下」不算合格——引用本身必须自带完整路径。
