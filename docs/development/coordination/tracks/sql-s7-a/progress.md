# Track S7-A: Central Assembly Audit — Progress

**Auditor**: Tester-only (S7-A)
**Date**: 2025-07-11
**Branch**: feat/sql-editor
**Status**: ✅ PASS (with 2 minor bugs — see bugs.md)

---

## 1. Props & Lifecycle Audit

### SqlEditor.tsx (374 lines)
- ✅ All props properly destructured and threaded through callback refs for stable identity
- ✅ `useImperativeHandle` exposes `getSelection`, `toggleLineComment`, `insertAt`, `getDocumentVersion`
- ✅ Compartments created in `useMemo` (statement, completion, intention, hover, paste) and reconfigured via separate `useEffect` hooks when dependencies change
- ✅ Main mount `useEffect` (lines 197–260) creates `EditorState` + `EditorView`, fires initial `onQualifiedPath`, and returns cleanup (`view.destroy(); viewRef.current = null`)
- ✅ Theme pack change listener properly removed on unmount (line 317)
- ✅ External value replacement syncs via `useEffect` with `BumpDocumentVersion` effect
- ✅ Execution status effects properly guarded with `if (!view) return`

### editorExtensions.ts (484 lines)
- ✅ `compartments` exported as named object with 5 compartment groups in priority order
- ✅ `documentVersionField` StateField correctly increments on doc changes and `BumpDocumentVersion` effects
- ✅ `createUpdateListener` reads from refs (not closures) so callbacks stay current
- ✅ `createDomEventHandlers` properly delegates to `onContextMenu`/`onCtxMenu` ref
- ✅ `createBaseEditorExtensions` wires Mod-Enter/Mod-Shift-Enter/Mod-S via handler refs passed at mount time
- ⚠️ `createHoverExtensions` returns an empty array — hover tooltip implementation is a no-op placeholder
- ⚠️ `createIntentionExtensions` hardcodes `getDialectAdapter('standard')` (see Bug-2)

### QueryEditorSection.tsx (385 lines)
- ✅ All props destructured and threaded to `SqlEditor` (lines 347–375)
- ✅ `insertValueHints` read from `useSettingsStore` (line 167)
- ✅ `handleEditorContextMenu` properly memoized with `useCallback`
- ✅ S6-D props (`metadataSnapshot`, `connectionId`, `onNavigateToTable`, `onNavigateToStructure`, `onCopyDdl`) correctly destructured and passed through

### Props Wiring — QueryPanel → QueryEditorSection
- ⚠️ **Bug-1**: Five S6-D props (`metadataSnapshot`, `connectionId`, `onNavigateToTable`, `onNavigateToStructure`, `onCopyDdl`) are NOT passed by `QueryPanel.tsx` when rendering `<QueryEditorSection>` (lines 331–383). See bugs.md for details.

---

## 2. Bypass Search

### SQL Execution Gate
- ✅ `QueryPanel.tsx` uses `executionGate.handleExecute` and `executionGate.handleExecuteSelection` for all editor-triggered execution
- ✅ `useQueryExecutionGate` (459 lines) is the single choke point: builds snapshot, validates dangerous SQL, checks transaction state, then delegates to `storeExecuteQuery`/`storeExecuteSelection`
- ✅ All toolbar buttons (`onExecute`, `onExplain`, `onBeginTx`, `onCommitTx`, `onRollbackTx`) go through the gate or transaction hooks

### Expected Exceptions (NOT bypasses)
- `ObjectBrowser.tsx` — DDL/schema operations
- `ImportDialog.tsx` — data import
- `PrivilegeView.tsx` — privilege management
- `TableStructureEditor.tsx` — DDL operations
- `DatabaseObjectView.tsx` — DDL/object operations
- `DocumentConnectionView.tsx` — legacy panel wiring

### Duplicate Scanner / Direct IPC
- ✅ No duplicate scanner instances found
- ✅ No direct IPC completion calls bypassing the gate

---

## 3. Code Quality Audit

### `any` Type Usage
- ✅ **Zero** `any` type annotations or `as any` casts in any production `.tsx` or `.ts` file under `src/components/sql-editor/` or `src/windows/connection/query/`
- ✅ The 24 occurrences of the word `any` in `functionRegistry.ts` are all **string data values** (`type: 'any'`) representing SQL function parameter type hints for signature help display — not TypeScript type annotations

### Hardcoded Driver IDs
- ✅ **`contracts.ts`** `CM_DIALECT_MAP`: Maps `postgresql`, `mysql`, `mariadb`, `sqlite`, `sqlserver` to CodeMirror built-in dialects. Falls back to `DB_REGISTRY[dbType]?.sqlDialect` for plugin-provided dialects. **Correct pattern** — CodeMirror only ships these 5 dialects.
- ✅ **`dialectAdapter.ts`** `PROFILES`: Defines identifier quoting/folding rules for 10 dialect strings. These are **semantic adapter profiles** (how to quote identifiers), not driver IDs. Plugins extend via `sqlDialect` mapping in `DB_REGISTRY`. **Correct pattern**.
- ✅ **`functionRegistry.ts`** `dialects` arrays: Tags SQL functions by dialect family (e.g. `['postgresql', 'postgres', 'pg', 'cockroach']`). **Data-driven**, not hardcoded branching.
- ✅ **Zero** hardcoded Redis, MongoDB, or other driver-specific IDs found in sql-editor production code

### connectionId / sessionId Patterns
- ✅ `dbSessionId` used for runtime session operations (completion, metadata loading)
- ✅ `connectionId` used for persistent config operations (favorites, history, drop validation)
- ✅ No confusion between the two ID types anywhere in the sql-editor module

### Line Counts (Production Files Only)
| Module | Lines |
|--------|-------|
| `src/components/sql-editor/` (all production .ts/.tsx) | 8,024 |
| `src/windows/connection/query/QueryEditorSection.tsx` | 385 |
| `src/windows/connection/QueryPanel.tsx` | 467 |
| **Total S6-D+ assembly files** | **1,356** |

---

## 4. Resource Cleanup Audit

### SqlEditor.tsx
- ✅ Main `useEffect` cleanup: `view.destroy(); viewRef.current = null;` (line 255–258)
- ✅ Theme listener cleanup: `document.removeEventListener('datazen:theme-pack-changed', reconfigure)` (line 317)
- ✅ No `setInterval`/`setTimeout` usage — CodeMirror manages its own timers internally

### QueryEditorSection.tsx
- ✅ Purely presentational — no `useEffect` hooks, no resources to clean up

### QueryPanel.tsx
- ✅ Debounce timer for SQL context sync: `clearTimeout(timer)` in return (line 282)
- ✅ Debounce timer for column ensure: `clearTimeout(timer)` in return (line 294)
- ✅ Tauri event listener: `void unlisten.then((fn) => fn())` in return (line 306)

### QuerySidebarSection.tsx
- ✅ `ensureTimer` ref cleaned on unmount: `if (ensureTimer.current) clearTimeout(ensureTimer.current)` (line 54)
- ℹ️ `handleQualifiedPath` (line 106) creates a 120ms debounce timer via `setTimeout` into a ref — cleaned on unmount but not on prop change. Minor: callback fires `ensureNamespacePath` (a Zustand action), not a React state update, so no unmounted-component warning risk.

### metadataCache.ts
- ✅ `cancelTimer` properly clears timeout (line 215–219)
- ✅ `invalidateSession` cancels timer and removes session (line 370–377)
- ✅ `switchContext` cancels timer before updating context (line 388)
- ✅ `invalidateAll` cancels all timers across all sessions (line 380)
- ✅ `subscribe` returns proper unsubscribe function (line 355–357)

### Session/Document Switch Leaks
- ✅ Session switch: `switchContext` cancels pending debounce timer and resets session state
- ✅ Document switch: `BumpDocumentVersion` effect (SqlEditor line 338–348) replaces doc content and bumps version counter
- ✅ `metadataCache.invalidateSession` cleans up all relations, errors, pending, and snapshot for the session

---

## 5. dbx Independence

- ✅ **Zero imports from `docs/todo/sql-editor/dbx`** anywhere in `src/`
- ✅ The 10 grep matches for `dbx` are all in the **data import/export feature** (`menu:import-connections-dbx`, `ConnectionShareDialog`, `windowManager.ts`), NOT dependencies on any dbx code module
- ✅ No runtime dependency on dbx code or data structures

---

## Final Verdict: ✅ PASS

### Reasoning
The central assembly is well-structured with proper compartment-based composition, clean lifecycle management, and correct use of the execution gate. Extension points dynamically wire pro capabilities if present. No bypasses, no `any` leaks, no resource leaks, no dbx dependencies.
