# Track Progress: sql-s3-a

- Task: 编辑器元数据缓存层
- Plan: docs/todo/sql-editor/implementation-plan.md §Track S3-A
- BaseCommit: 4fdc336b3
- Worktree: .worktrees/datazen-sql-s3-a
- Branch: feature/sql-s3-a
- Phase: PASSED
- Agent: coder-sql-s3-a
- CodingCommit: efecd8ec9
- FixCommit: —
- TestCommit: 8668552ee
- MergeCommit: —
- LastHeartbeat: 2026-09-05T12:47:00+08:00
- UnresolvedBugs: 0

## 详细进展

- 2026-09-05 12:23 BOOTSTRAP（读必读清单：AGENTS.md / subagent README / _common.md / sql-s3-a brief / implementation-plan §S3-A + §4.4）
- 2026-09-05 12:25 CODING → 12:37 READY_FOR_TEST (feat efecd8ec9)

## 自验结果（Coder 独立实测）

| 套件 | 结果 |
|------|------|
| `npx tsc --noEmit` | 0 errors |
| `npx vitest run src/lib/__tests__/schemaCache.test.ts src/components/sql-editor/metadata/__tests__ src/stores/__tests__/schemaStoreSelectors.test.ts` | 47 passed, 0 failed |
| `npx vitest run src/lib`（全量 lib） | 130 files / 1132 tests passed |
| `npx vitest run src/stores`（全量 stores） | 21 files / 312 tests passed |
| `npx vitest run src/components/sql-editor`（全量 semantic/param-history/metadata） | 12 files / 177 tests passed |
| cache 消费者回归（schemaCache/exportTableStructure/loadBatchExportTable/ContentView/ExportDialog/TableStructureEditor） | 25+20 tests passed |

### 覆盖率（新增/改动模块 · 独立实测）

| 模块 | Stmts | Branch | Funcs | Lines | 阈值 | 结果 |
|------|-------|--------|-------|-------|------|------|
| `src/components/sql-editor/metadata/` | 94.11% | 78.33% | 95.74% | 96.66% | ≥80/70/75/80 | ✓ |
| `src/components/sql-editor/metadata/metadataCache.ts` | 93.52% | 77.65% | 94.11% | 96.64% | ≥80/70/75/80 | ✓ |
| `src/components/sql-editor/metadata/copyDdl.ts` | 96% | 79.16% | 100% | 95.65% | ≥80/70/75/80 | ✓ |
| `src/components/sql-editor/metadata/relationKey.ts` | 100% | 100% | 100% | 100% | ≥80/70/75/80 | ✓ |
| `src/lib/schemaCache.ts` | 96.73% | 88.88% | 100% | 98.78% | ≥80/70/75/80 | ✓ |
| `src/stores/schemaStoreSelectors.ts` | 100% | 92.86% | 100% | 100% | ≥80/55/75/80 | ✓ |

说明：覆盖率用 `--coverage` 跑本轨相关测试文件取样；全量 `npx vitest run --coverage` 时 sql-editor 整包阈值由既有 S2-A/S2-B 测试共同满足。

## 改动摘要（efecd8ec9）

### 新增 `src/components/sql-editor/metadata/*`
- `types.ts`: `EditorRelationKind` / `EditorRelationKey` / `EditorRelationMetadata` /
  `EditorMetadataSnapshot` / `EditorRelationRequest` / `EditorMetadataContext`
- `relationKey.ts`: `relationIdentityKey`、`buildEditorRelationKey`（dbSessionId + namespace + quoted-aware fold）、
  `qualifiedNameText`（驱动可解析 identity 编码，保留 quoted）、`relationBaseName`、`hasNamespace`
- `metadataCache.ts`: `createMetadataCache`（可注入 loadTableSchema / debounce / errorTtl / allowQualified / now / subscribeInvalidation）、
  `ensureRelations`（批量 debounce + inflight/loaded/error-TTL 去重，过滤 cte/subquery，allowQualified=false 时跳过跨 namespace）、
  `getSnapshot`（同步、不可变、referential-stable 至 epoch bump，`useSyncExternalStore` 可用）、
  `getRelation`、`subscribe`、`invalidateRelation`/`invalidateSession`/`invalidateAll`/`switchContext`、`schemaToMetadata`、
  默认单例 `metadataCache`
- `copyDdl.ts`: `copyRelationDdl`（复用 DDLView 方言 DDL SQL 生成 + extractor strategy，经完整 identity 调升级后 `getCachedDDL`）、`resolveDdlQuery`

### 改动 `src/lib/schemaCache.ts`
- `getCachedTableSchema`: 加 inflight dedupe、TTL error（10s，失败不反复请求）、deep-frozen immutable snapshot
- `getCachedDDL`: 同加 inflight + TTL error；cache key 升级为 `dbSessionId + object kind + full namespace`（可选 `identity`，向后兼容旧调用）
- `invalidateSchemaCache(..., identity?)`: 同步清 schema/DDL/error 缓存 + 通知 invalidation 订阅者
- 新增 `subscribeSchemaInvalidation`（DDL mutation / schema refresh / session close 的外接失效钩子）

### 新增 `src/stores/schemaStoreSelectors.ts`
- `readMetadataSnapshot` / `getMetadataRelation`（同步读，无 IPC）、`ensureMetadataRelations`、
  `bindingToRelationRequests`（cte/subquery 过滤）、`relationCacheKey`、`resolveEditorDialectId`
- `useMetadataSnapshot` / `useActiveSessionMetadataSnapshot`（注入 metadataCache 单例；`useSyncExternalStore`）
- 保留既有 `columnMap` 供旧 CodeMirror schema completion

### 测试
- `src/lib/__tests__/schemaCache.test.ts`: 新增 inflight/concurrent、immutable deep-freeze、scheme error TTL、
  DDL kind+namespace 隔离、DDL inflight、DDL error TTL、invalidation 订阅
- `src/components/sql-editor/metadata/__tests__/{relationKey,metadataCache,copyDdl}.test.ts`: session/namespace/quoted-case 隔离、
  table/view 同名、并发 dedupe、失败重试、invalidation、切换上下文、无 table comment 降级、Copy DDL identity
- `src/stores/__tests__/schemaStoreSelectors.test.ts`: 同步 snapshot、binding 映射、dialect 解析、hooks

## 备注

- 完成定义（completion 回调可同步读 snapshot，期间不产生 IPC）：metadataCache.getSnapshot 为同步只读，ensureRelations 提前 debounce/dedupe 预取；符合。
- AC-05 列 detail 顺序（type/nullable/comment）与 AC-10 有界展示：metadata snapshot 以 ColumnSchema（含 comment?）输出，comment 缺省为空不伪造。
- 未触碰 `schemaStore.ts`、QueryPanel、SqlEditor 装配、`docs/todo/sql-editor/dbx`；无 `any` 生产代码；生产文件均 <500 行。
- DDL cache 旧调用（DDLView/exportTableStructure/loadBatchExportTable/ContentView/useNavigatorContextMenus）保持原位签名，未改调用方；缺省 key 含默认 `table` kind。

## Tester 独立复验（tester-sql-s3-a · 全新实例）

- Phase: **PASSED** · 测试 commit `8668552ee`
- 零信任独立重跑，各套件数字（Coder 自报 vs Tester 实测）：

| 套件 | Coder 自报 | Tester 实测 | 一致 |
|------|-----------|-------------|------|
| `npx tsc --noEmit` | 0 errors | 0 errors | ✓ |
| 元数据/缓存主套件（schemaCache + metadata + selectors） | 47 passed | 50 passed（47 + 3 新增） | ✓（+3 tester） |
| `npx vitest run src/lib` | 130 files / 1132 tests | 130 files / 1132 tests | ✓ |
| `npx vitest run src/stores` | 21 files / 312 tests | 21 files / 312 tests | ✓ |
| `npx vitest run src/components/sql-editor` | 12 files / 177 tests | 12 files / 180 tests（含 3 新增） | ✓（+3 tester） |
| 消费者回归（exportTableStructure/loadBatchExportTable/ContentView/ExportDialog/TableStructureEditor） | 25+20 passed | 5 files / 45 passed | ✓ |
| 生产文件 <500 行 | — | max metadataCache.ts 417 行 | ✓ |
| dbx clean-room / 裸 unwrap/expect / Host 驱动 ID 分支 | — | 无 | ✓ |

### 覆盖率（改动模块 · Tester 独立实测）

| 模块 | Stmts | Branch | Funcs | Lines | 阈值 | 结果 |
|------|-------|--------|-------|-------|------|------|
| `src/components/sql-editor/metadata/metadataCache.ts` | 95.29% | 78.72% | 97.05% | 98.65% | ≥80/70/75/80 | ✓ |
| `src/components/sql-editor/metadata/copyDdl.ts` | 96% | 79.16% | 100% | 95.65% | ≥80/70/75/80 | ✓ |
| `src/components/sql-editor/metadata/relationKey.ts` | 100% | 100% | 100% | 100% | ≥80/70/75/80 | ✓ |
| `src/lib/schemaCache.ts` | 96.73% | 88.88% | 100% | 98.78% | ≥80/70/75/80 | ✓ |
| `src/stores/schemaStoreSelectors.ts` | 100% | 92.85% | 100% | 100% | ≥80/55/75/80 | ✓ |

### Tester 新增测试（`test_tester_`/`[tester]`）

`src/components/sql-editor/metadata/__tests__/metadataCache.test.ts`（+3）：
- `[tester] loads a view relation ... kind view`：覆盖 `toLoadableKind` 的 view 分支 + 快照暴露 kind。
- `[tester] keeps a table and a view sharing a name in different namespaces distinct`：`public.users`(table) 与 `audit.users`(view) 同载不碰撞，直接覆盖 brief「table/view 同名」场景。
- `[tester] auto-flushes queued requests once the debounce window elapses`：验证 debounce 定时器自动触发 prefetch（覆盖 scheduleFlush 闭包）。

### 审查发现（非阻断）

- 未发现阻断性 Bug；验收项 AC-05、AC-10 及完成定义（completion 同步读 snapshot、无 IPC）均满足。
- 低严重度观察（不影响 PASSED）：schemaCache 的 `invalidateSchemaCache` 不取消 in-flight 请求（60s TTL 兜底，仅在“请求在途同时发生 DDL 变更”的窄竞态下短暂陈旧）；metadataCache 按未限定表名做同名跨 schema 的 over-invalidation（仅缓存丢失，会重载）；copyDdl 的 DDL cache identity 用 raw namespace 名（未折叠大小写/引号），极端拼写下仅缓存切分、无正确性影响。
- **全量 `npx vitest run`（331 files）出现 2 项失败，均为 `scripts/__tests__/check-ci-docs-consistency.test.ts`**：`windowManager.ts` 缺 `openSingletonWindow` 的 `data-sync`/`schema-diff`/`data-transfer` 参数。此为本轨**范围外、前置存在**的窗口边界一致性缺漏（S3-A 未触碰 `src/lib/windowManager.ts` 及相关文档），不影响 S3-A PASSED；记录供协调者知悉。
