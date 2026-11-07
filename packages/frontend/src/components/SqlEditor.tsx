/**
 * Backward-compatible entry point for SqlEditor.
 *
 * Re-exports the centralized SqlEditor component and its types.
 * No second source of truth — all logic lives in sql-editor/.
 *
 * §Track S6-D: backward compat wrapper (step 7)
 */
export { SqlEditor } from './sql-editor/SqlEditor';
export type {
  SqlEditorHandle,
  SqlEditorProps,
  SqlSchema,
  DroppedTablePayload,
} from './sql-editor/contracts';
export { resolveCmDialect } from './sql-editor/contracts';
