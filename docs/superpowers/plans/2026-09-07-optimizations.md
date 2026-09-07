# Optimization Plan: Review Issues, Multi-DB Navigation, Home Metrics, Column Copy & Keymap Presets

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the 5 optimization requirements: (1) address remaining P0–P3 review items; (2) disable default database auto-expansion on multidb connections in ConnectionNavigatorTree; (3) fix/replace the always-zero "Connected" metric on the landing page; (4) support copying whole-column data in query results; (5) add built-in DBeaver and Navicat keymaps with custom shortcut support.

**Architecture:**
- **Navigation Tree**: Remove default DB auto-expansion in `ConnectionNavigatorTree.tsx` for multi-db connections, keeping expansion strictly at the databases container level.
- **Home Landing Metrics**: Replace the static zero "Connected" card on the landing page with "Pinned Connections" (`pinnedCount`), keeping all three metrics meaningful and accurate.
- **Column Data Copy**: Extend `dataTableContextMenu.ts`, `DataTable.tsx`, and `TableHeader.tsx` to extract and copy all rows for a target column formatted as newline-delimited text.
- **Keymap Presets & Customization**: Create a unified `keymap.ts` module with Default, DBeaver, and Navicat presets. Persist `keymapPreset` and `customKeymap` in `AppSettings` (both Rust backend and frontend Zustand store). Wire dynamic keybindings to CodeMirror extensions in `SqlEditor` and global shortcuts in `ContentView`. Provide UI configuration in Settings.
- **Review Items (P0–P3)**:
  - Add `.gitignore` entry for `e2e/.app-data-*/`.
  - Pass `database` to `StructureView` and `DDLView` for multi-db SSOT cache fidelity.
  - Implement active-statement windowing in `createModelBuilderExtension` for large SQL files (> 1,000 lines).
  - Add Alt+Enter intention QuickFix menu popup support when multiple intentions are available.
  - Add Settings toggles for SQL Editor features (`editorSignatureHelp`, `editorTableHover`).

**Tech Stack:**
- React 18, TypeScript, Tailwind CSS 4, Zustand
- CodeMirror 6 (`@codemirror/view`, `@codemirror/state`, `@codemirror/autocomplete`)
- Tauri v2 (Rust backend `src-tauri/src/store/settings.rs`)
- Vitest

---

## Global Constraints

- Produce zero regressions across existing 3,450+ unit tests and Rust lib tests.
- Strictly adhere to `AGENTS.md` Single Source of Truth (SSOT), no bare unwrap/expect in Rust, and single-file size recommendations.
- Keep English and Chinese i18n keys synchronized.

---

### Task 1: Navigation Tree Multi-DB Auto-Expansion Fix (Requirement 2)

**Files:**
- Modify: `src/windows/connection/ConnectionNavigatorTree.tsx:345-360`
- Test: `src/windows/connection/__tests__/ConnectionNavigatorTree.test.tsx`

**Interfaces:**
- `ConnectionNavigatorTree`: For multi-db drivers, stop adding `${connectionId}::${dbName}` to `expandedDbs` and `${dbKey}::tables` to `expandedCats`.

- [ ] **Step 1: Write test checking multidb connection opens without auto-expanding default database**
- [ ] **Step 2: Modify `ConnectionNavigatorTree.tsx` to omit default database auto-expansion when `isMultiDb` is true**
- [ ] **Step 3: Run Vitest for `ConnectionNavigatorTree.test.tsx` to verify**

---

### Task 2: Landing Page Metric Fix (Requirement 3)

**Files:**
- Modify: `src/windows/connection/ConnectionWorkspaceHome.tsx:150-165,295-325`
- Modify: `src/locales/en/connection.ts`
- Modify: `src/locales/zh-CN/connection.ts`
- Test: `src/windows/connection/__tests__/ConnectionWorkspaceHome.test.tsx`

**Interfaces:**
- `ConnectionWorkspaceHome`: In State 3 (no connection session active), replace the always-zero `connectedCount` card with `pinnedCount` (`savedConnections.filter(c => c.pinned).length`) displaying "Pinned Connections" / "置顶连接" with a `Star` icon.

- [ ] **Step 1: Update i18n locales for `connWin.home.metrics.pinned` ("Pinned" / "置顶连接")**
- [ ] **Step 2: Update `ConnectionWorkspaceHome.tsx` to compute `pinnedCount` and render the Pinned metric card**
- [ ] **Step 3: Update `ConnectionWorkspaceHome.test.tsx` and run test suite**

---

### Task 3: Query Result Copy Whole Column (Requirement 4)

**Files:**
- Modify: `src/lib/dataTableContextMenu.ts:1-70,230-290`
- Modify: `src/components/DataTable/DataTable.tsx:230-320`
- Modify: `src/components/DataTable/TableHeader.tsx:20-110`
- Modify: `src/locales/en/dataTable.ts`
- Modify: `src/locales/zh-CN/dataTable.ts`
- Test: `src/lib/__tests__/dataTableContextMenu.test.ts`
- Test: `src/components/DataTable/__tests__/DataTable.test.tsx`
- Test: `src/components/DataTable/__tests__/TableHeader.test.tsx`

**Interfaces:**
- `DataTableContextMenuLabels`: Add `copyColumnData: string`
- `DataTableContextMenuHandlers`: Add `onCopyColumnData?: () => void`
- `TableHeader`: Support right-click context menu on column headers with "Copy Column Name" and "Copy Column Data"

- [ ] **Step 1: Add unit tests for `copyColumnData` in `dataTableContextMenu.test.ts`**
- [ ] **Step 2: Update `dataTableContextMenu.ts` with `copyColumnData` label and handler wiring**
- [ ] **Step 3: Implement `onCopyColumnData` in `DataTable.tsx` extracting values for `hit.columnIndex` across all `rows`**
- [ ] **Step 4: Add header context menu in `TableHeader.tsx` for direct right-click copy on column headers**
- [ ] **Step 5: Run Vitest on DataTable and TableHeader tests**

---

### Task 4: Built-in DBeaver/Navicat Keymaps & Custom Shortcuts (Requirement 5)

**Files:**
- Create: `src/lib/keymap.ts`
- Create: `src/lib/__tests__/keymap.test.ts`
- Modify: `src-tauri/src/store/settings.rs:90-120`
- Modify: `src/types/index.ts:240-275`
- Modify: `src/stores/settingsStore.ts:20-40`
- Modify: `src/components/sql-editor/editorExtensions.ts:190-250`
- Modify: `src/components/sql-editor/SqlEditor.tsx`
- Modify: `src/windows/connection/ContentView.tsx:280-305`
- Modify: `src/windows/settings/SettingsContent.tsx:235-280`
- Modify: `src/locales/en/settings.ts`
- Modify: `src/locales/zh-CN/settings.ts`
- Test: `src/windows/settings/__tests__/SettingsContent.test.tsx`

**Interfaces:**
- `KeymapPreset`: `'default' | 'dbeaver' | 'navicat' | 'custom'`
- `KeymapConfig`: `{ preset: KeymapPreset; customKeys?: Record<string, string> }`
- `resolveKeymap(preset, customKeys)`: returns effective key combinations for `execute`, `executeAll`, `newQuery`, `saveQuery`, `closeTab`, `formatSql`.

- [ ] **Step 1: Create `src/lib/keymap.ts` with presets (Default, DBeaver, Navicat) and unit tests**
- [ ] **Step 2: Extend Rust `AppSettings` in `src-tauri/src/store/settings.rs` with `keymap_preset` and `custom_keymap`**
- [ ] **Step 3: Extend frontend `AppSettings` and `settingsStore.ts` with keymap settings**
- [ ] **Step 4: Update `editorExtensions.ts` and `SqlEditor.tsx` to use active keymap for `execute`, `executeAll`, `saveQuery`**
- [ ] **Step 5: Update `ContentView.tsx` to use active keymap for `newQuery`, `closeTab`**
- [ ] **Step 6: Add Keymap Configuration UI in `SettingsContent.tsx` under Editor section**
- [ ] **Step 7: Add i18n keys and run Vitest for Settings and Keymap**

---

### Task 5: Complete Remaining Independent Review Items (P0–P3) (Requirement 1)

**Files:**
- Modify: `.gitignore`
- Modify: `src/windows/connection/PanelContentRenderer.tsx`
- Modify: `src/windows/connection/StructureView.tsx`
- Modify: `src/windows/connection/DDLView.tsx`
- Modify: `src/lib/fetchRelationDdl.ts`
- Modify: `src/components/sql-editor/editorExtensions.ts:610-640`
- Modify: `src/components/sql-editor/intentions/analyzeIntentions.ts`
- Modify: `src/windows/settings/SettingsContent.tsx`
- Modify: `src/types/index.ts`
- Modify: `src-tauri/src/store/settings.rs`
- Test: `src/windows/connection/__tests__/StructureView.test.tsx`
- Test: `src/lib/__tests__/fetchRelationDdl.test.ts`
- Test: `src/components/sql-editor/semantic/__tests__/scopeModel.test.ts`

**Interfaces:**
- Multi-DB SSOT: `StructureView` and `DDLView` accept `database?: string` and pass to `getCachedTableSchema` / `fetchRelationDdl`.
- Performance (AC-19): `createModelBuilderExtension` windows semantic model parsing for large documents (> 1,000 lines).
- Dirty file cleanup: Ignore `e2e/.app-data-*/` in `.gitignore`.
- Intentions: Support multi-intention selection menu and fallback column extraction.
- Settings: Add toggles for `editorSignatureHelp` and `editorTableHover`.

- [ ] **Step 1: Add `e2e/.app-data-*/` to `.gitignore`**
- [ ] **Step 2: Pass `database` to `StructureView` and `DDLView` for multi-DB SSOT cache accuracy**
- [ ] **Step 3: Implement active-statement windowing in `createModelBuilderExtension` for large documents**
- [ ] **Step 4: Add SQL Editor feature toggles (`editorSignatureHelp`, `editorTableHover`) in Settings and Editor extensions**
- [ ] **Step 5: Verify all changes with comprehensive tests**

---

### Task 6: Full Verification & Sanity Check

- [ ] **Step 1: Run `pnpm tsc --noEmit`**
- [ ] **Step 2: Run full unit tests `npx vitest run`**
- [ ] **Step 3: Run Rust host tests `cargo test -p datazen --lib`**
- [ ] **Step 4: Review `git status` and verify all requirements are satisfied**
