# Track: sql-s3-c — Schema tree 拆分与拖拽 payload

## Status: READY_FOR_TEST

## LastHeartbeat: 2026-07-09T14:30:00Z

## Phase Flow
- DISPATCHED → BOOTSTRAP → CODING → READY_FOR_TEST

## Commit History
- 841aa88af — feat(sql-s3-c): split UnifiedSchemaTree + versioned drag payload

## Self-Verification
| Check | Result |
|-------|--------|
| SchemaObjectDragPayloadV1 contract | ✅ schemaTreeDrag.ts: version, kind, namespace, connectionId, dbSessionId?, databaseType |
| Legacy application/datazen-table compat | ✅ buildLegacyTablePayload + setDragPayload emits both MIMEs |
| Tree row renderers extracted | ✅ SchemaTreeRow.tsx (359 lines): DbRow, SchemaRow, CategoryRow, TableRow, ObjectRow, CatEmptyRow, EmptyRow |
| State hooks extracted | ✅ useSchemaTreeState.ts (310 lines): all state + actions |
| FlatRows computation extracted | ✅ useSchemaTreeFlatRows.ts (290 lines): filterTableItems, filterObjectItems, flatRows memo |
| UnifiedSchemaTree < 500 lines | ✅ 167 lines (was 808) |
| Unit tests pass | ✅ 23/23 schema-tree tests, 454/454 connection tests |
| Payload always carries connectionId + database/schema/table | ✅ DragPayloadOptions requires all four fields |
| Payload carries dbSessionId when session exists | ✅ useSchemaStore.getState().dbSessionId passed as optional |
| Tree only expresses schema objects, no SQL generation | ✅ No import of editor semantic modules; no SQL text or quote logic |
