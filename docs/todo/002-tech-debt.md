# 技术债与优化点清单

> **状态**：待评估
> **来源**：可视化查询构建器（QB）收尾 + 数据源统一整改过程中发现
> **相关提交**：`fbd6efb7`、`a0fda35d`、`af305cb8`、`3465f33b`、`edc6fe71`、`25296e47`
>
> 本文件只记录**已核实**的问题。每条都给出可复现的证据（文件:行、实测输出），
> 未经验证的猜测不写入。P0 是会产生错误 SQL 的缺陷，P1 是架构债，P2 是工程体验。

---

## P0 — 正确性缺陷（会生成错误 SQL）

### P0-1 复合外键生成重复 JOIN，SQL 直接报错

**位置**：`src/components/query-builder/hooks/useAutoJoin.ts`（按列对展开） +
`src/lib/sqlDialects/queryBuilder.ts` 的 `generateJoinClause`（每条 `QbJoin` 输出一个 JOIN）

**证据**（实测，非推断）：声明式复合外键 `lines(order_id, line_no) → orders(id, no)`
展开为两条 `QbJoin` 后，`generateJoinClause` 输出：

```
INNER JOIN "orders" ON "lines"."order_id" = "orders"."id"
INNER JOIN "orders" ON "lines"."line_no"  = "orders"."no"
```

**影响**：`orders` 被 JOIN 两次且未起别名。PostgreSQL 报
`table name "orders" specified more than once`，MySQL 同理。也就是说**只要库里存在
一个复合外键、且两张表同时被拖进画布，生成的 SQL 就是废的**。这是当前已可达的缺陷，
不需要任何新功能触发。

**建议**：`QbJoin` 需要支持多列 ON 条件（`QbJoin.columnPairs: {left,right}[]`），
`generateJoinClause` 对同一 `(leftTable, rightTable, type)` 分组后合并为一个 JOIN：

```sql
INNER JOIN "orders" ON "lines"."order_id" = "orders"."id"
                  AND "lines"."line_no"  = "orders"."no"
```

注意这与「手动 JOIN 永远只有两列」的现有设计取舍冲突（见
`docs/features/query-builder.md` Known Limitations），需要一并重新表述。

### P0-2 拖放丢弃 namespace，跨 schema 同名表被当成同一张表

**位置**：`src/components/query-builder/DiagramCanvas/DiagramCanvas.tsx:111-112`

```ts
const payload = JSON.parse(raw) as { namespace?: { table?: string } };
const tableName = payload.namespace?.table;
```

**证据**：拖放 payload（`SchemaObjectDragPayloadV1`）本身携带
`namespace: { database, schema, table }`，但只取 `table`。QB 的 `selectedTables`
是 `string[]` 裸表名。

**影响**：`sales.orders` 与 `public.orders` 同时拖入时被视为同一张表（去重/覆盖），
列合并、JOIN 目标、生成的 SQL 全部指向错误对象。跨 schema 复用表名是常见设计
（多租户、分模块），不是边缘场景。

**建议**：`selectedTables` 的标识改为带 namespace 的稳定 key，UI 显示短名 + schema
前缀消歧。

**代价已重新评估（2026-09 实测）**：这**不是** QB 局部改动。`getAllColumns`
（`src-tauri/src/commands/schema.rs:464`）只接收 `dbSessionId` + `database`，
`columnMap` 以**表名**为键（`schemaStore.ts:516`），即列存储是**库级而非 schema 级**。
要让表标识真正带 namespace，必须同时改：

1. Rust `get_all_columns` 命令签名（增加 schema）
2. `schemaStore` 的 `columnMap` 键结构
3. schema 树 / 补全 / QB 等所有列消费方

属于跨切面重构，应作为独立任务排期，不要顺手塞进 QB 的改动里。

另注：当前 QB 对「从非当前 schema 拖入的表」会生成**裸表名** SQL，
可能静默命中 search_path 下的同名表 —— 这与本项同源，一并由上述重构解决。

### P0-3 `findRelationMetadata` 的裸名回退对「生成 SQL」不安全

**位置**：`src/components/sql-editor/metadata/findRelation.ts`（第 3 步按裸表名大小写不敏感匹配）

**证据**：QB 侧已规避（按表名逐表取 `getCachedTableSchema`，只认精确表名，并有测试钉住
「不同 schema 同名表不得匹配」）。但该函数仍被 `completion/schemaCompletion.ts` 使用。

**影响**：补全场景下「找到点什么比什么都没有强」是合理取舍；但**任何用它来生成
SQL 的新调用方**都会拿到跨 schema 的错误表。当前没有契约阻止这种误用。

**建议**：函数改名为 `findRelationMetadataForCompletion`，或在文档中明确
「本函数仅供补全/悬停等容错场景，生成 SQL 必须用精确 key」。参考
`docs/features/query-builder.md` 中已记录的取舍。

---

## P1 — 架构债

### P1-1 `src/lib` 向上依赖 `components/stores/windows`（22 处）

**证据**：扫描 `src/lib/**` 的相对 import，命中 22 处，例如：

| 文件 | 依赖 |
| --- | --- |
| `lib/windowManager.ts` | `stores/settingsStore` |
| `lib/wappBridge.ts` | `stores/connectionStore`、`stores/activeConnectionStore` |
| `lib/queryExecutionViewModel.ts` | `stores/queryExecActions` |
| `lib/workflowDraftYaml.ts` | `windows/workflow/WorkflowForm` |
| `lib/processListResult.ts` | `components/DataTable/TableHeader` |
| `lib/connectionShare.ts` | `components/connection/ConnectionShareDialog` |

**影响**：`lib` 在这套代码里不是严格的下层。新代码无法依据目录判断依赖方向，
容易出现循环依赖，也让单测必须拉起上层模块。

**建议**：先分类——把纯算法（可下沉）与真正的视图适配（应上移）分开，再逐批处理。
不要一次性全局加规则，否则守卫会被存量违规淹没（见 P1-2 的处理方式）。

### P1-2 `sql-editor/semantic/` 被误放在 editor 下，实际是共享基础设施

**证据**：`src/lib/` 有 4 个文件依赖它：

- `lib/sqlEditorDefaults.ts` → `semantic/scanner`、`semantic/statementRanges`、`semantic/scope/builder`、`semantic/dialectAdapter`
- `lib/sqlStatementRange.ts` → `semantic/statementRanges`
- `lib/sqlTransactionGuard.ts` → `semantic/scanner`
- `lib/dangerousSql.ts` → `semantic/scanner`、`semantic/tokens`、`semantic/types`

**影响**：`semantic/`（11 文件 / 1849 行）是 SQL 词法、作用域与方言的公共层，
却挂在 `components/sql-editor/` 之下。任何使用 SQL 语义的非编辑器模块都得反向依赖
编辑器目录。本次整改已把方言 profile 与标识符引号/折叠下沉到
`lib/sqlDialects/`，但 `semantic/` 的其余部分仍在原处。

**建议**：整体迁移到 `src/lib/sqlSemantic/`。这是纯路径改动 + 13 个 importer 更新，
风险低；完成后即可把守卫从「3 条定向规则」升级为通用的「lib 不得向上依赖」。

### P1-3 应用内仍有两套列获取路径

**证据**：

| 来源 | 加载方式 | 使用者 |
| --- | --- | --- |
| `schemaStore.columnMap` / `columnInfoMap` | `getAllColumns` 批量 | 编辑器补全命名空间、QB 表格卡片/CriteriaGrid/ConditionBuilder |
| `metadataCache.relations[].columns` | `getTableSchema` 逐表 | 编辑器语义层（悬停、inlay、导航） |

**影响**：同一张表的列可能被取两次（一次批量、一次逐表），且两条链路的失效时机
不同。QB 已统一到前者（列）+ 后者（外键），但**应用整体**仍是两套。

**建议**：评估把补全也统一到 `metadataCache`。需要先量化代价——补全需要「全库列名」
而 `metadataCache` 是按需逐表，改过去会改变补全的加载时机与首屏成本，不能只图架构整齐。

### P1-4 store 依赖组件目录的类型

**证据**：`src/stores/queryBuilderStore.ts:11` → `../components/query-builder/types`

**影响**：状态层依赖视图层目录。类型本身是纯数据，放错了位置。

**建议**：`QbJoin`、`QbCondition` 等移到 `src/lib/queryBuilder/types.ts` 或 store 自身目录。

### P1-5 `supportsOffset` 与 Rust `supports_offset()` 手工同步，无守卫

**证据**：前端读 `DatabaseTypeMeta.supportsOffset`（`src/lib/databaseMeta.ts`），
Rust 侧为 `packages/driver-api` 的 `supports_offset()`（默认 `true`，
`packages/drivers/olap|superset` 覆写为 `false`）。**没有任何脚本校验两者一致**。

**影响**：驱动作者只改 Rust 或只改前端 meta 时，UI 控件与生成的 SQL 会不一致
（控件可点但生成的 SQL 无子句，或反之）。本次整改把默认值语义对齐为
「LIMIT/OFFSET 是标准 SQL，未声明即支持」，但一致性仍靠人工。

**建议**：`resolve-drivers.mjs` 生成 `generated.ts` 时顺带核对 Rust 侧的
`supports_offset()`（驱动可通过元数据文件声明），不一致则构建失败。

---

## P2-5 `e2e/specs/er-diagram.ts` ER-008（PNG 导出）在 HEAD 即失败

**现象**：`Error: ER PNG export did not write <temp>.png`，稳定复现，非抖动。

**已核实与近期改动无关**：在改动前的代码上（`git stash` 后重新构建）跑该 spec 同样
7 passing / 1 failing，失败项就是 ER-008。

**影响**：ER spec 无法作为「全绿」验收依据；新增的 ER-009（推测关系）不受影响。

## P2 — 工程体验

### P2-1 全量 E2E 构建必以 DMG 失败收尾，导致测试被跳过

**证据**：`pnpm e2e` 的构建阶段输出：

```
Bundling DataZen_0.2.1_aarch64.dmg
     Running bundle_dmg.sh
failed to bundle project error running bundle_dmg.sh
```

**影响**：`run.mjs` 在构建失败后**不会继续跑 WDIO**，因此
`node e2e/run.mjs --instances 1 -- --spec ...` 会「成功构建但零测试执行」，
必须先失败一次再补一次 `--skip-build`。这会让人误以为测试跑过了（实际 0 tests）。

**建议**：E2E 构建改用不产出 DMG 的目标（或让 `run.mjs` 把 DMG 失败视为非致命并继续），
并在日志里明确区分「构建失败」与「测试未执行」。

### P2-2 5 个 Redis 驱动 UI 单测在 HEAD 即失败

**证据**：`npx vitest run --config vitest.drivers.config.ts packages/drivers/redis` →
`tryDecompressString` zlib/gzip 返回 `null`、`invokeSetString` / `invokeCreateKey` 参数断言不符。
已用 `git stash` 确认在 HEAD（无本次改动）同样失败。

**影响**：`pnpm test:unit:drivers` 常态性红灯，真实回归会被淹没。

**建议**：要么修，要么显式 skip 并记录原因。属于驱动 crate 目录，修复应落在
`packages/drivers/redis/`（AGENTS.md 规定驱动测试不得移到 Host）。

### P2-3 i18n 8 个语言包缺 632 个 key

**证据**：`node scripts/i18n-sync-check.mjs` 报告 632 处缺失；CI 步骤为
`continue-on-error: true`。开发期只维护 `en` + `zh-CN`（AGENTS.md 规定）。

**影响**：发布前需集中补齐，否则小语种界面出现英文兜底或 key 字面量。

**建议**：发布 checklist 中固化该步骤；或用 i18n-sync skill 批量补齐。

### P2-4 `QbCondition.conjunction` 是残留字段

**证据**：`src/components/query-builder/types.ts:37` 定义 `conjunction: 'AND' | 'OR'`；
生成器只读 `group.logic`（`useSqlGenerator.ts:163` 仍写入该字段但无人消费）。

**影响**：类型上暗示条件自带连接词，与「连接词属于组」的实际模型矛盾，
后续维护者容易误用。

**建议**：删除字段或明确标注为兼容遗留。需要同步 QB 单测与类型。

---

## 附：本次整改已顺手关闭的项

| 项 | 处理 |
| --- | --- |
| QB 外键绕过编辑器缓存，DDL 后陈旧 | 统一到共享缓存（`3465f33b`） |
| QB 用裸表名取 schema，与编辑器缓存键不一致、重复取数 | identity 统一构造（`3465f33b`） |
| QB 读全局 active session 的 `columnMap` | 改读自身 `dbSessionId`（`3465f33b`） |
| QB 与编辑器互为同级依赖 | 共享层下沉 + 守卫（`25296e47`） |
| LIMIT/OFFSET 能力被方言家族硬编码决定 | 改为纯驱动声明、默认支持（`edc6fe71`） |
