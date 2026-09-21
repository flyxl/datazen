# 外键关系智能预测 — 设计方案

> 状态：**Draft，待决策**（未开始实现）
> 目标：在数据库**没有声明外键约束**的情况下，推断表之间真实的关联关系，
> 供可视化查询构建器（自动 JOIN）与 ER 图共用。
> 相关：`docs/features/query-builder.md`、`docs/todo/002-tech-debt.md`（P0-1 / P0-2 是本方案的前置依赖）

---

## 1. 问题与定位

### 1.1 现状

QB 的自动 JOIN 与 ER 图的连线都**只读声明式外键**：

- `src/components/query-builder/hooks/useAutoJoin.ts` — 过滤 `foreignKeys`
- `src/windows/connection/er/buildErGraph.ts` — 读 `schema.foreignKeys`

### 1.2 为什么不够

大量真实系统不建外键约束：

- **ORM 主导**：Rails / Django / Hibernate 只在应用层表达关联，迁移不落 FK（历史上还有
  「FK 影响写入性能 / 分库分表不支持 / 迁移顺序麻烦」等理由）
- **遗留库**：约束在历史迁移中丢失，或从来没建过
- **数仓 / OLAP**：宽表 + 维表，关联靠命名约定和 ETL 保证
- **分片表**：物理分片后 FK 无法跨片维护

结果是：**这些库在 QB 里拖进两张表后，一条 JOIN 都不会自动出现**，而这恰恰是最需要
辅助的场景——用户得手工查两张表的列名再点两次。

### 1.3 核心原则（先立规矩）

> **匹配错 > 匹配不到。** 一条错误的 JOIN 会把用户的查询结果算错，而结果错了用户
> 未必看得出来；漏掉一条，用户手工补一次即可（手动 JOIN 已实现，`af305cb8`）。

这条原则决定了后面所有取舍：**预测必须可解释、可撤销、有明确的置信度分档，且永远
不能与声明式 FK 混为一谈。**

---

## 2. 可用的信号（按可靠性分层）

### 第 0 层：声明式约束（ground truth，已有）

`information_schema` 的 FK。置信度固定 1.0。**预测不覆盖、不修改、不降级它。**

### 第 1 层：结构信号（零成本，纯元数据）

| 信号 | 说明 | 强度 |
| --- | --- | --- |
| 目标列是目标表主键 | `orders.id` 是 PK | 强 |
| 目标列有唯一索引 | 允许非 PK 的唯一键 | 强 |
| 源列名 == `{目标表名}_{目标列名}` | `orders` + `id` → `orders_id` | 强 |
| 源列名 == `{目标表单数}_{目标列名}` | `users` + `id` → `user_id` | 强 |
| 源列名带表前缀/驼峰/缩写 | `t_user_id`、`userId`、`usr_id` | 中 |
| 类型兼容（见 §5 坑 4） | 必须相同或同族 | **门（gate）** |
| 源列自身有索引 | 真实 FK 通常建索引以加速 JOIN | 中 |
| 源列可空 | 可选关系（`LEFT JOIN` 更可能正确） | 弱（用于选 JOIN 类型） |
| 两表在同一 database/schema | 跨库不预测 | **门** |

### 第 2 层：数据信号（需访问数据，可选、显式触发）

**子集包含检验**——最有力的经验证据：

```sql
-- 源列的取值是否全部落在目标列中？
SELECT COUNT(*) AS bad
FROM (SELECT DISTINCT child.parent_id AS v
      FROM child
      WHERE child.parent_id IS NOT NULL
      LIMIT 1000) s
WHERE NOT EXISTS (SELECT 1 FROM parent p WHERE p.id = s.v);
```

`bad = 0` 且样本量足够 → 关系几乎必然成立。这是**唯一能识破「名字像但语义无关」**
的手段（例如 `status_id` 恰好都落在 `1..5`，与某张码表的 id 偶然重合）。

### 第 3 层：语义信号

- **列注释 / 表注释**：`-- 用户ID` 与 `users` 表注释匹配
- **命名相似度**：编辑距离 / 公共前缀（弱，仅用于排序，不单独触发预测）

### 第 4 层：用户反馈（质量最高、成本最低，且可累积）

用户在 QB 里**接受/拒绝**一条预测、或**手工画了一条 JOIN**，都是标注。持久化后作为
该连接上的强信号：同一对 `(表, 列)` 下次直接高置信度。

> 手工画的 JOIN 是「用户已经告诉我们真相」，应当主动询问是否记住。

---

## 3. 打分与置信度分档

不要用布尔判定，用**加权证据 + 阈值**，且分数要能解释。

```
score = Σ weight(信号)
```

建议初值（需用真实库标定，这里给的是起点）：

| 信号 | 权重 |
| --- | --- |
| 目标列是 PK | +0.40 |
| 目标列有唯一索引（非 PK） | +0.30 |
| 名字 == `{表名}_{列名}`（单复数均可） | +0.40 |
| 名字仅部分匹配（前缀/缩写/驼峰） | +0.15 |
| 源列有索引 | +0.10 |
| 类型完全一致 | +0.10 |
| 注释语义匹配 | +0.10 |
| 数据子集检验通过（样本充足） | +0.60 |
| 用户此前接受过 | +0.50 |
| 用户此前拒绝过 | **直接排除** |

分档与行为：

| 档位 | 分数 | UI 行为 |
| --- | --- | --- |
| **Confirmed** | 声明式 FK | 实线；进候选列表，确认后写入 SQL（现状不变） |
| **High** | ≥ 0.80 | **虚线 + `predicted` 徽标**；同样进候选列表，**必须确认才写入 SQL** |
| **Medium** | 0.50 ~ 0.80 | 不展示（构建器没有独立的「建议」列表，候选即决策点） |
| **ambiguous** | 任意 | 不展示：引擎拒绝二选一 |
| **Low** | < 0.50 | 不展示（避免噪声淹没画布） |

**关键**：推测关系虽然进候选列表，但**必须与声明式 FK 视觉可区分**（虚线 + 徽标 +
琥珀色）。用户一眼要能看出「这条是猜的」。

> 落地时的取舍：设计上设想的「High 自动 JOIN + Medium 建议栏」被简化为「High 进候选、
> 其余不展示」。原因是查询构建器的候选流程本身就是确认点，再加一个建议栏会形成两套
> 语义相近、可互相矛盾的入口。

---

## 4. 架构与落点

### 4.1 算法放 Host，信号由驱动提供

算法本身是数据库无关的（名字 + 类型 + 元数据 + 数据），放 Host：

```
src/lib/relationPrediction/
├── normalizeIdentifier.ts    # 驼峰/下划线/前缀剥离，按方言折叠
├── singularize.ts            # 保守单数化 + 不规则表
├── candidateIndex.ts         # 按归一化列名建索引（O(n)，非 O(n²)）
├── typeCompatibility.ts      # 类型门（方言驱动）
├── scoreCandidate.ts         # 加权证据，输出可解释明细
├── predictRelations.ts       # 编排 → RankedRelationCandidate[]
└── types.ts
```

全部是**注入元数据的纯函数**，不碰 IPC——调用方（查询构建器面板、ER 图、编辑器补全）各自负责取数。

### 4.2 必须放共享层，不能各写一份

ER 图（`windows/connection/er/`）与 QB 都需要它。按
`scripts/check-module-layers.mjs` 已确立的规则：**两者是并列的，共享能力下沉到
`lib/`，谁都不 import 谁。**

### 4.3 方言差异靠驱动声明（零硬编码）

需要驱动补充的元数据（`DatabaseTypeMeta`）：

- 类型等价组：`{ integer: ['int','int4','integer','bigint'…] }` —— 或由
  `lib/sqlDialects` 提供通用归一化 + 驱动覆盖
- 标识符大小写折叠：**已有**（`lib/sqlDialects/dialectProfile.ts` 的 `foldCase`）
- 数据抽样语法：`TABLESAMPLE` / `LIMIT` / `SAMPLE` 各不同
- 标识符引用字符：**已有**（`identifierQuoting.ts`）

### 4.4 数据探针的执行路径

建议**不要**新造 Host 专属 IPC 命令，而是：

1. 用 `lib/sqlDialects` 生成探针 SQL（引用、抽样语法按方言）
2. 走既有查询通道执行，但强制：**只读、有语句超时、可取消、不进用户事务**
3. 为有更好原语的驱动（`APPROX_COUNT_DISTINCT`、`TABLESAMPLE`、
   统计信息里的 distinct 计数）提供 Driver Command 覆写

### 4.5 前置依赖

- **P0-1（复合外键重复 JOIN）**：预测会产出复合关系，必须先修，否则预测把这个
  缺陷的暴露面放大
- **P0-2（丢弃 namespace）**：~~跨 schema 预测需要带 namespace 的表标识~~
  **已重新评估，不阻塞本功能。** 实现时发现 `getAllColumns(dbSessionId, database)`
  不接收 schema，`columnMap` 是**库级**而非 schema 级的（`schemaStore.ts:516`
  以表名为键）。要让 QB 的表标识真正带 namespace，需要同时改 Rust 命令签名、
  schema store 的列存储与 schema 树/补全的消费方 —— 是跨切面重构，量级远超本功能。
  因此 **P1 阶段限定在同一 schema 上下文内预测**（这正是 QB 今天解析列与关系所用的
  上下文），跨 schema 同名表的冲突作为已知限制记录在
  `docs/todo/002-tech-debt.md` P0-2。

---

## 5. 坑清单（重点）

### 坑 1：把预测 JOIN 悄悄写进 SQL

**最严重的坑。** 用户拖两张表、点应用，SQL 里多出一条没人要他 JOIN 的条件，
结果集静默变小。

**规避**：分档 + 视觉区分 + 可移除 + **移除要持久**（`removedAutoJoinIds` 已有机制，
但预测 id 必须确定性，见坑 11）。另外提供连接级/全局开关关闭预测。

### 坑 2：数据探针把生产库拖垮

子集检验是 `DISTINCT` + 相关子查询，在大表上是全表扫描。用户拖 5 张表就并发 20 条。

**规避**：

- **绝不自动触发**。High 档只靠结构信号；数据探针必须用户显式点（或明确开关）
- 抽样上限（如 1000 个 distinct 值）并 `LIMIT` 收敛
- 语句超时 + 可取消 + 并发上限（建议 2）
- 尊重连接的只读/安全模式；**禁止在用户事务内执行**
- 结果按 `(dbSessionId, table, column)` 缓存，失效跟随 schema 失效链路
- 优先用统计信息（catalog 里的 distinct 计数）代替真实扫描

### 坑 3：`NULL` 与三值逻辑

`child.parent_id NOT IN (SELECT id FROM parent)` —— 只要 `parent.id` 有 `NULL`
或 `parent_id` 有 `NULL`，结果就是 `UNKNOWN` 而非 `TRUE`，**假的「全部匹配」**。

**规避**：显式排除 `NULL`，用 `NOT EXISTS` 而不是 `NOT IN`：

```sql
WHERE child.parent_id IS NOT NULL AND NOT EXISTS (...)
```

同时记录非空样本量——`0` 个非空值时结论无意义（见坑 5）。

### 坑 4：类型必须当「门」，不能只当分数

`user_id VARCHAR(32)` 指向 `users.id INT`：名字完全匹配。MySQL 会把 `'1' = 1` 隐式
转换后判定相等，**连数据探针都会通过**。于是产生一条语义错误的 JOIN。

**规避**：

- 类型不兼容 → **直接排除**，不给任何分数
- 类型归一化要按方言：`int` / `int4` / `INTEGER` / `NUMBER(10)` / `bigint` 的等价关系
- 数字 vs 字符串、字符串 vs 日期的跨族一律排除
- 长度不参与判定（`VARCHAR(32)` vs `VARCHAR(64)` 视为兼容）

### 坑 5：空表/小表「平凡通过」

`child` 只有 0 行时，子集检验必然通过 → 虚假的最高置信度。新库、测试库、
刚清过数据的表都会命中。

**规避**：设**样本量下限**（如至少 30 个非空 distinct 值）与**重叠率阈值**；
样本不足时降级为结构信号打分，不给数据加分。

### 坑 6：复数化/单数化不可靠

`users → user_id` 好办；但 `people → person_id`、`categories → category_id`、
`address → address_id`（不是 `addres`）、`data → data_id`。中文/拼音表名更无从下手。

**规避**：

- 单数化只作为**弱加分项**，绝不作为门
- 维护不规则表（person/people、child/children、category/categories…）
- 匹配时**同时尝试原名与单数名**，不做「只认单数」
- 表名本身就是 `user_id` 这种命名时不要递归剥离

### 坑 7：自引用与层级表

`employees.manager_id → employees.id`、`categories.parent_id → categories.id`。

**规避**：显式支持「目标表 == 源表」；`parent_id` / `manager_id` / `owner_id`
这类语义名要能匹配本表 PK。注意别让自引用把画布连成环后让用户困惑——
自引用 JOIN 需要别名（`e` / `m`），而当前生成器对同表两次出现正是 P0-1 的缺陷。

### 坑 8：复合键必须成组

`(order_id, line_no) → (id, no)`。按列逐个匹配会产出两条独立 JOIN，语义和语法双错
（见 `docs/todo/002-tech-debt.md` P0-1）。

**规避**：预测产出**关系组**而非单列对；生成器合并为一个 JOIN + 多个 `AND`。
多列同时命中同一目标表时，优先合并而不是并列。

### 坑 9：大小写折叠必须按方言

`UserID` 与 `userid` 在 PostgreSQL（折叠为小写）是同一列；在 SQL Server
（`foldCase: 'preserve'`）是**不同**列。用统一的小写比较会在 SQL Server 上
预测出错误关系。

**规避**：复用 `lib/sqlDialects/dialectProfile.ts` 的 `foldCase`，不要自己
`toLowerCase()`。这条已经就绪。

### 坑 10：多候选歧义必须弃权

`status_id` 可能指向 `order_status` / `user_status` / `task_status`；
`type_id` 更夸张。名字完全一样、类型也兼容。

**规避**：

- 候选数 > 1 且分数接近（差值 < 阈值）→ **一条都不给**
- 可复用编辑器已有的语义状态词汇：`RelationResolveStatus = 'unique' | 'ambiguous' | 'unresolved'`
- 歧义时可以在 UI 上列候选让用户选，但**不能替用户选**

### 坑 11：预测 id 必须确定性且稳定

现有自动 JOIN 的 id 是 `auto-${from}.${fromCol}-${to}.${toCol}`（确定性）。
预测若用随机 id，用户移除后重算又会冒出来 —— **打地鼠**。

**规避**：id 由 `(源表, 源列, 目标表, 目标列, 类型)` 派生；重算后 id 不变；
用户的移除/接受记录按 id 持久化。

### 坑 12：异步到达不能扰动画布

预测可能在用户已经排好布局后返回。若此时重置 `tablePositions` / `zoom` /
`canvasOffset`，用户会瞬间失去工作现场。

**规避**：预测只增删 JOIN 列表，**永不触碰**布局状态。这条要有测试钉住。

### 坑 13：元数据不足导致「预测不出来」而非「预测错」

结构信号需要**主键、唯一索引、数据类型、注释**。QB 目前从 `columnMap` 拿到的只有
列名；PK/唯一索引在 `metadataCache` 的 `EditorRelationMetadata` 里。

**规避**：预测引擎的输入契约必须是完整的 `EditorRelationMetadata`，不是
`string[]`。这正是 `3465f33b` 统一数据源的价值——现在 QB 能拿到这些字段了。
缺少元数据时应**降级**（只做名字匹配并标为 Medium），而不是当作「无关系」。

### 坑 14：分表/分区表制造大量假候选

`orders_2024_01` … `orders_2024_12` 与 `order_items_2024_01` … 名字高度相似、
结构完全一致，但彼此**没有**关联。同理还有审计表、历史表、备份表。

**规避**：

- 表名相似度高但无 PK/唯一索引支撑 → 不给分
- 识别批量后缀模式（`_YYYYMM`、`_YYYYMMDD`、`_bak`、`_copy`、`_old`）并降权
- 同构表的列名相同属于正常现象，**不能**作为关联证据

### 坑 15：多态关联无法用单一目标表达

`comments.commentable_id` + `comments.commentable_type`：目标表由 `type` 列的值决定，
可能是 `posts`、`photos`、`videos`。`commentable_id` 会同时匹配一堆候选表。

**规避**：识别 `*_type` / `*_kind` / `*_entity` 兄弟列存在时，**主动弃权**
（并可在 UI 说明「检测到多态关联，无法自动确定目标」）。硬猜必然错。

### 坑 16：系统目录与元数据表

`information_schema.*`、`pg_catalog.*`、`sqlite_master`、`mysql.*` 里存在大量
名字相似的列。它们出现在 schema 树里，可能被拖进画布。

**规避**：按 `DatabaseTypeMeta` 声明的系统 schema 列表排除；或至少不参与预测。

### 坑 17：视图没有主键

视图可以作 JOIN 目标，但通常没有 PK/唯一索引，结构信号几乎全灭。

**规避**：视图默认只做弱匹配（Medium 档以下）；允许用户手工确认后记住（第 4 层信号）。

### 坑 18：不可解释的黑箱没人敢用

只给一个「建议关联」徽标，用户无法判断该不该接受，也就不会用这个功能。

**规避**：每条候选都输出**证据明细**，UI 悬浮显示：

> 建议：`orders.user_id → users.id`
> · `users.id` 是主键
> · 列名匹配 `users` + `id`
> · 类型一致（`integer`）
> · 1000 个样本值全部命中（0 个例外）

这同时也让算法可调试、可回归测试。

### 坑 19：跨库/跨连接预测

不同 database 之间不可能有 FK。

**规避**：`database` 不同的表对**直接排除**（门）。跨 schema（同库）允许，但需要
带 namespace 的表标识——依赖 P0-2。

### 坑 20：测试落点与 i18n

- 算法是 Host 纯函数 → 单测放 `src/lib/relationPrediction/__tests__/`
- 交互（接受/拒绝/移除后不复现）→ 必须写**连续旅程测试**（AGENTS.md 要求），
  扩展 `e2e/specs/journeys/visual-query-builder-journey.ts`
- 方言相关的类型等价/抽样语法若落到驱动 → 测试放**驱动 crate 目录**，不得进 Host
- 所有新徽标/文案/理由说明都要 `en` + `zh-CN` 的 `query.visualBuilder.*` key

---

## 6. 分期建议

| 阶段 | 内容 | 价值 | 风险 |
| --- | --- | --- | --- |
| **前置** | 修 P0-1（复合 JOIN） | 解除阻塞 | 低 |
| **P1** | 结构信号（名字 + PK/唯一索引 + 类型门）+ 打分 + 分档 UI + 可解释明细 | 覆盖大部分真实场景，**零数据访问**，无性能风险 | 低 |
| **P2** | 数据探针（显式触发、抽样、超时、可取消） | 识破「名字像但无关」；把 Medium 提升为 High | 中（性能/安全） |
| **P3** | 用户反馈持久化（接受/拒绝/手工 JOIN 记忆） | 越用越准；可跨会话累积 | 低 |
| **P4** | 注释语义匹配、统计信息加速 | 边际提升 | 低 |

**建议先做 P1**：它不需要任何数据访问，纯元数据即可跑，风险最低，且能覆盖
「`xxx_id` 指向 `xxx.id`」这一主流约定——这已经是绝大多数 ORM 场景。

---

## 7. 验证方式

- **算法单测**：用合成的元数据夹具覆盖每个坑（空表、类型不符、多候选、
  多态、自引用、复合键、分表后缀、系统 schema、大小写方言）
- **真实库回归**：在 E2E 的 PostgreSQL 上建一组**无 FK 约束**但命名成惯例的表，
  断言预测结果与预期一致；再建一组**故意误导**的表（同名不同类型的列、
  分表、多态），断言**不产出**错误关系
- **反向断言比正向更重要**：漏报可接受，误报必须为零容忍
- **性能基线**：200 张表 × 平均 20 列的元数据下，预测耗时应 < 50ms（纯内存）
- **DDL 后一致性**：新建/删除列后预测结果必须跟随刷新（复用已统一的
  `subscribeSchemaInvalidation` 链路）

---

## 8. 待决策项

1. **High 档是否自动 JOIN？** 我倾向**是**（否则功能价值大打折扣），但必须视觉可区分
   且可一键移除。若产品上更保守，可降为「只进建议列表」。
2. **数据探针是否默认开启？** 我倾向**默认关闭**，连接级可开。
3. **用户反馈存哪？** 连接配置（`connectionId`）下持久化，还是 appData 全局？
   注意 AGENTS.md 的 ID 术语规范：反馈是**配置/归属**语义 → 用 `connectionId`，
   不能用 `dbSessionId`（运行时态、永不落盘）。
4. **类型等价组放哪？** 由 `lib/sqlDialects` 提供通用表 + 驱动 `DatabaseTypeMeta`
   覆盖，还是完全由驱动声明？（倾向后者，符合零硬编码，但需要 8 个驱动都补）
5. **是否也用于 Schema Diff / 数据传输的关联推断？** 若会，算法需更早抽成公共契约。
