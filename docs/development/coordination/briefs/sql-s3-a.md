# Brief: sql-s3-a — 编辑器元数据缓存

> 依赖：S2-B 已 PASSED 并合流（消费 semantic model 的 relation 输出）。
> 完整章节：`docs/todo/sql-editor/implementation-plan.md` §Track S3-A（深查用）。
> 必读：`briefs/_common.md` + 本文件 + `tracks/sql-s3-a/progress.md`。

## 独占写锁

可建/可改：`src/components/sql-editor/metadata/*`、`src/lib/schemaCache.ts`、
新 `src/stores/schemaStoreSelectors.ts` 及对应 tests。
**禁止**：`schemaStore.ts`（若真实接入必须改它，先停止并升级为等价拆分轨）、
QueryPanel、SqlEditor 装配。

## 步骤

1. 实现规范化 relation key：覆盖任意 namespace path、quoted case、同名表。
2. 复用 `getCachedTableSchema`，加 inflight dedupe、TTL error、immutable snapshot。
3. 提供批量 `ensureRelations`，对 semantic model 提取的 relation debounce 去重。
4. 输出 ColumnSchema、PK、index、FK；表 comment 保持 optional（无数据隐藏该行，不伪造）。
5. 接入 schema refresh、DDL mutation、session close 的 invalidation；DDL cache key 升级为
   `dbSessionId + object kind + full identity`，避免跨 schema/table-view 碰撞。
6. 保留现有 `columnMap` 给旧 CodeMirror schema completion，不破坏其他调用方。
7. Copy DDL 复用现有 DDLView 的方言 SQL 生成和 extractor strategy，经完整 identity
   调用升级后的 `getCachedDDL`；不调用不存在的通用 table/view DDL API。

## 测试

session/namespace/quoted-case 隔离、table/view 同名、并发 dedupe、失败重试、
invalidation、切换上下文、无 table comment 降级；回归 DDLView、StructureView、
IndexesView、ExportDialog 的 cache 调用。

## 完成定义

- completion 回调可同步读 snapshot，期间不产生 IPC。
- 相关验收：AC-05（列 detail 顺序 type/nullable/comment）、AC-10 部分（有界展示）。
