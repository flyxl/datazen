# Track S6-D: Central Editor / Query Assembly

> Status: **READY_FOR_TEST**
> Base branch: `feature/sql-s6-d` (on top of `feat/sql-editor`)
> LastHeartbeat: 2025-09-06

---

## Summary

Implemented the central SQL Editor assembly that composes all leaf extension factories (S2-A through S6-C) into CodeMirror 6 `Compartment` groups with a fixed priority order, enabling dynamic reconfiguration without duplicating listeners, timers, or tooltips.

---

## Architecture

### Compartment Priority Order

```
statement → completion/signature → intention/hint → hover/navigation → paste/drop/multipleSelection
```

Each group lives in its own `Compartment` so schema changes, dialect switches, or settings updates reconfigure only the affected layer.

### Key Files Modified

| File | Lines | Role |
|------|-------|------|
| `src/components/sql-editor/editorExtensions.ts` | 484 | Compartment definitions, base extensions, keymaps, theme, create*Extensions factories, update listener, DOM event handlers |
| `src/components/sql-editor/SqlEditor.tsx` | 374 | React component: mounts EditorView, wires compartments, syncs props to refs, reconfiguration effects |
| `src/components/sql-editor/contracts.ts` | — | MSSQL dialect added, new SqlEditorProps fields, getDocumentVersion on handle |
| `src/components/sql-editor/paste/createPasteExtensions.ts` | 40 | Extracted paste/drop/multipleSelection factory (S5-A) |
| `src/components/sql-editor/queryDropHandler.ts` | — | Cross-connection reject, non-empty editor → insert table reference, empty editor → generate SELECT |
| `src/components/SqlEditor.tsx` | 8 | Backward compat re-export |

---

## Deliverables

### 1. MSSQL Dialect Mapping

Added `mssql: MSSQL` to `CM_DIALECT_MAP` in `contracts.ts`, resolving SQL Server syntax highlighting that previously fell through to `StandardSQL`.

### 2. Compartment-Based Extension Architecture

- **statementExts**: Statement frame + gutter (S4-A)
- **completionExts**: Schema completion, join completion, function completions, namespace loading, signature help (S4-B)
- **intentionExts**: Intention analysis + apply (S4-C)
- **hoverExts**: Hover tooltips + navigation clicks (S4-D)
- **pasteExts**: Multiple selections, paste-as-in, drop caret (S5-A)

### 3. Custom Keymap Override

Keymaps for Mod-Enter (execute), Mod-Shift-Enter (execute all), Mod-S (save) are placed in a **separate** `keymap.of()` **before** `defaultKeymap` to override CodeMirror's built-in `Mod-Enter` (insertNewlineKeepIndent).

### 4. External Value Replacement

Added `documentVersionField` StateField + `BumpDocumentVersion` effect for tracking external value replacements. The handle exposes `getDocumentVersion()`.

### 5. Theme-Pack Change Listener

Reconfigures the theme compartment on `datazen:theme-pack-changed` events, ensuring runtime theme changes propagate without remounting.

### 6. Insert Value Hints Setting

Compartment-based toggle controlled by `editorInsertValueHints` from settings store. The S6-B INSERT hint is gated on this setting.

### 7. Backward Compat

`src/components/SqlEditor.tsx` re-exports `SqlEditor` from the new location for existing consumers.

### 8. Drop Handler Improvements

`queryDropHandler.ts` updated:
- Cross-connection drop rejected with error
- Non-empty editor → insert table reference at drop position
- Empty editor → generate SELECT statement

### 9. DomEventRefs Simplification

`createDomEventHandlers` now only handles `contextmenu` — dragover/drop are handled by `createPasteExtensions` via `createDropCaretExtension`.

### 10. getClientRects Polyfill

Added `Range.prototype.getClientRects` polyfill in `src/test/setup.ts` for jsdom CodeMirror gutter support.

---

## Test Updates

Tests were updated to reflect the S5-A architecture:
- **editorExtensions.test.ts**: contextmenu test updated for `getStatementAtCursor` behavior; dragover/drop tests updated to reflect that `createDomEventHandlers` no longer handles drops
- **SqlEditorDrop.test.tsx**: drop tests updated to use `expect.objectContaining` for enriched payload from `createPasteExtensions`
- **SqlEditorLifecycle.test.tsx**: theme-pack listener, placeholder, and onQualifiedPath tests restored

---

## Verification

- `npx tsc --noEmit` — **0 errors**
- `npx vitest run` — **352 test files pass, 3292 tests pass**
  - 3 pre-existing script test failures (unrelated): `check-version-consistency`, `resolve-drivers`, `check-ci-docs-consistency`
- All SQL editor tests pass: `editorExtensions.test.ts` (7), `SqlEditorDrop.test.tsx` (6), `SqlEditorLifecycle.test.tsx` (7), `SqlEditorShortcuts.test.tsx` (4)
